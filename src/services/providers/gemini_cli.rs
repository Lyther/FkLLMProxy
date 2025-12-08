use async_trait::async_trait;
use futures::stream;
use serde::Deserialize;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::Semaphore;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    models::openai::{
        ChatCompletionChoice, ChatCompletionRequest, ChatCompletionResponse, ChatMessage,
        DeltaMessage, Role,
    },
    services::providers::{
        LLMProvider, Provider, ProviderError, ProviderResult, StreamingResponse,
    },
    state::AppState,
};

const DEFAULT_CLI_TIMEOUT_SECS: u64 = 30;
const MAX_CONCURRENT_REQUESTS: usize = 4;

/// Response structure for Gemini CLI JSON output
#[derive(Deserialize)]
struct GeminiCliResponse {
    response: String,
    #[serde(default)]
    usage: Option<GeminiCliUsage>,
}

#[derive(Deserialize)]
struct GeminiCliUsage {
    #[serde(default)]
    prompt: Option<u32>,
    #[serde(default)]
    candidates: Option<u32>,
    #[serde(default)]
    total: Option<u32>,
}

/// Provider for Google's Gemini CLI.
///
/// This provider spawns `gemini` CLI processes to handle requests.
/// It includes concurrency limiting, comprehensive error handling, and streaming simulation.
pub struct GeminiCliProvider {
    cli_path: String,
    timeout_secs: u64,
    concurrency_semaphore: Arc<Semaphore>,
}

impl GeminiCliProvider {
    fn current_unix_timestamp_secs() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    /// Create a new Gemini CLI provider.
    ///
    /// # Arguments
    /// * `cli_path` - Path to the gemini CLI binary (defaults to "gemini")
    /// * `timeout_secs` - Request timeout in seconds (defaults to 30)
    /// * `max_concurrency` - Maximum concurrent requests (defaults to 4)
    #[must_use]
    pub fn new(
        cli_path: Option<String>,
        timeout_secs: Option<u64>,
        max_concurrency: Option<usize>,
    ) -> Self {
        let max_concurrent = max_concurrency.unwrap_or(MAX_CONCURRENT_REQUESTS);
        Self {
            cli_path: cli_path.unwrap_or_else(|| "gemini".to_string()),
            timeout_secs: timeout_secs.unwrap_or(DEFAULT_CLI_TIMEOUT_SECS),
            concurrency_semaphore: Arc::new(Semaphore::new(max_concurrent)),
        }
    }

    async fn acquire_concurrency_permit(
        &self,
    ) -> Result<tokio::sync::SemaphorePermit<'_>, ProviderError> {
        tokio::time::timeout(
            std::time::Duration::from_secs(30),
            self.concurrency_semaphore.acquire(),
        )
        .await
        .map_err(|_| {
            ProviderError::Unavailable(format!(
                "Gemini CLI concurrency limit reached ({MAX_CONCURRENT_REQUESTS} concurrent requests max) - please try again later"
            ))
        })?
        .map_err(|e| {
            ProviderError::Internal(format!("Failed to acquire concurrency permit: {e}"))
        })
    }

    fn build_cli_command(&self, prompt: &str, model: Option<&str>) -> Command {
        let mut cmd = Command::new(&self.cli_path);
        cmd.arg("-p").arg(prompt);

        if let Some(model_name) = model {
            cmd.arg("-m").arg(model_name);
        }

        cmd.arg("--output-format").arg("json");
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        cmd
    }

    async fn execute_cli_process(
        &self,
        mut cmd: Command,
    ) -> Result<std::process::Output, ProviderError> {
        let child = cmd.spawn().map_err(|e| {
            ProviderError::Internal(format!("Failed to spawn Gemini CLI process: {e}"))
        })?;

        let process_timeout = std::time::Duration::from_secs(self.timeout_secs.saturating_sub(1));
        tokio::time::timeout(process_timeout, child.wait_with_output())
            .await
            .map_err(|_| {
                ProviderError::Timeout(format!(
                    "Gemini CLI process timed out after {} seconds",
                    process_timeout.as_secs()
                ))
            })?
            .map_err(|e| ProviderError::Internal(format!("Failed to execute Gemini CLI: {e}")))
    }

    fn map_cli_error_to_provider_error(stderr: &str) -> ProviderError {
        let error_msg = stderr.to_lowercase();

        // Timeout errors
        if error_msg.contains("timeout")
            || error_msg.contains("timeouterror")
            || error_msg.contains("deadline exceeded")
            || error_msg.contains("request timeout")
        {
            return ProviderError::Timeout(
                "Gemini CLI request timed out - try reducing prompt length or increasing timeout"
                    .to_string(),
            );
        }

        // Rate limiting errors
        if error_msg.contains("rate limit")
            || error_msg.contains("quota exceeded")
            || error_msg.contains("too many requests")
            || error_msg.contains("resource exhausted")
            || error_msg.contains("quota")
            || error_msg.contains("limit exceeded")
        {
            return ProviderError::RateLimited(
                "Gemini CLI rate limit exceeded - please wait before retrying".to_string(),
            );
        }

        // Authentication errors
        if error_msg.contains("auth")
            || error_msg.contains("unauthorized")
            || error_msg.contains("authentication")
            || error_msg.contains("credentials")
            || error_msg.contains("login required")
            || error_msg.contains("not authenticated")
            || error_msg.contains("invalid token")
            || error_msg.contains("permission denied")
        {
            return ProviderError::Auth(
                "Gemini CLI authentication failed - please check your credentials".to_string(),
            );
        }

        // Network connectivity errors
        if error_msg.contains("connection")
            || error_msg.contains("network")
            || error_msg.contains("dns")
            || error_msg.contains("unreachable")
            || error_msg.contains("connect")
            || error_msg.contains("tcp")
        {
            return ProviderError::Network(
                "Gemini CLI network error - please check your internet connection".to_string(),
            );
        }

        // Service unavailable errors
        if error_msg.contains("service unavailable")
            || error_msg.contains("temporarily unavailable")
            || error_msg.contains("maintenance")
            || error_msg.contains("overloaded")
            || error_msg.contains("server error")
            || error_msg.contains("internal error")
        {
            return ProviderError::Unavailable(
                "Gemini CLI service temporarily unavailable - please try again later".to_string(),
            );
        }

        // Invalid request errors (malformed prompts, unsupported features, etc.)
        if error_msg.contains("invalid")
            || error_msg.contains("malformed")
            || error_msg.contains("unsupported")
            || error_msg.contains("bad request")
            || error_msg.contains("invalid argument")
        {
            return ProviderError::InvalidRequest(format!(
                "Gemini CLI rejected request: {}",
                stderr.trim()
            ));
        }

        // Generic internal error fallback
        ProviderError::Internal(format!("Gemini CLI command failed: {stderr}"))
    }

    async fn execute_cli_command(
        &self,
        prompt: &str,
        model: Option<&str>,
    ) -> Result<String, ProviderError> {
        let _permit = self.acquire_concurrency_permit().await?;
        let cmd = self.build_cli_command(prompt, model);

        info!(
            "Gemini CLI: Executing command: {} -p \"{}\"",
            self.cli_path, prompt
        );

        let output = self.execute_cli_process(cmd).await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            error!(
                "Gemini CLI command failed (exit code: {}): {}",
                output.status, stderr
            );

            // Try to map based on stderr content first
            let provider_error = Self::map_cli_error_to_provider_error(&stderr);

            // If it's still a generic internal error, add more context from exit code
            if let ProviderError::Internal(_) = provider_error {
                if let Some(code) = output.status.code() {
                    let detailed_error = match code {
                        1 => ProviderError::InvalidRequest(
                            "Gemini CLI command failed (invalid arguments)".to_string(),
                        ),
                        2 => ProviderError::Auth("Gemini CLI authentication failed".to_string()),
                        126 => {
                            ProviderError::Internal("Gemini CLI command not executable".to_string())
                        }
                        127 => ProviderError::Internal(
                            "Gemini CLI command not found - please install @google/gemini-cli"
                                .to_string(),
                        ),
                        130 => {
                            ProviderError::Internal("Gemini CLI command interrupted".to_string())
                        }
                        _ => ProviderError::Internal(format!(
                            "Gemini CLI failed (exit code: {}): {} (stdout: {})",
                            output.status,
                            stderr.trim(),
                            stdout.trim()
                        )),
                    };
                    return Err(detailed_error);
                }
            }

            return Err(provider_error);
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(stdout)
    }

    fn parse_cli_response(output: &str) -> Result<GeminiCliResponse, ProviderError> {
        let output = output.trim();

        // Handle empty output
        if output.is_empty() {
            warn!("Gemini CLI returned empty output");
            return Err(ProviderError::Internal(
                "Gemini CLI returned empty response".to_string(),
            ));
        }

        // Try to parse as JSON first (structured response)
        if let Ok(response) = serde_json::from_str::<GeminiCliResponse>(output) {
            // Validate the response has actual content
            if response.response.trim().is_empty() {
                warn!("Gemini CLI returned valid JSON but empty response content");
                return Err(ProviderError::Internal(
                    "Gemini CLI returned empty response content".to_string(),
                ));
            }
            return Ok(response);
        }

        // Try to parse as alternative JSON formats (some versions might use different structure)
        if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(output) {
            // Try to extract response from common alternative structures
            if let Some(text) = json_value.get("text").and_then(|v| v.as_str()) {
                return Ok(GeminiCliResponse {
                    response: text.to_string(),
                    usage: None,
                });
            }
            if let Some(content) = json_value.get("content").and_then(|v| v.as_str()) {
                return Ok(GeminiCliResponse {
                    response: content.to_string(),
                    usage: None,
                });
            }
            if let Some(result) = json_value.get("result").and_then(|v| v.as_str()) {
                return Ok(GeminiCliResponse {
                    response: result.to_string(),
                    usage: None,
                });
            }
            // If it's JSON but doesn't match expected structure, treat the whole thing as text
            if let Ok(text) = serde_json::to_string_pretty(&json_value) {
                warn!("Gemini CLI returned unrecognized JSON structure, using as text");
                return Ok(GeminiCliResponse {
                    response: text,
                    usage: None,
                });
            }
        }

        // Look for common error patterns in plain text
        let lower_output = output.to_lowercase();
        if lower_output.contains("error")
            || lower_output.contains("failed")
            || lower_output.contains("exception")
            || lower_output.contains("traceback")
        {
            warn!("Gemini CLI output appears to contain an error: {output}");
            return Err(ProviderError::Internal(format!(
                "Gemini CLI error response: {output}"
            )));
        }

        // Fallback: treat as plain text response
        info!(
            "Gemini CLI output is not valid JSON, treating as plain text response (length: {} chars)",
            output.len()
        );
        Ok(GeminiCliResponse {
            response: output.to_string(),
            usage: None,
        })
    }

    fn create_openai_response(
        cli_response: GeminiCliResponse,
        request: &ChatCompletionRequest,
        request_id: &str,
    ) -> ChatCompletionResponse {
        let created = Self::current_unix_timestamp_secs();

        let choice = ChatCompletionChoice {
            index: 0,
            message: ChatMessage {
                role: Role::Assistant,
                content: cli_response.response,
                name: None,
            },
            finish_reason: Some("stop".to_string()),
        };

        let usage = cli_response.usage.map(|u| crate::models::openai::Usage {
            prompt_tokens: u.prompt.unwrap_or(0),
            completion_tokens: u.candidates.unwrap_or(0),
            total_tokens: u.total.unwrap_or(0),
        });

        ChatCompletionResponse {
            id: request_id.to_string(),
            object: "chat.completion".to_string(),
            created,
            model: request.model.clone(),
            choices: vec![choice],
            usage,
        }
    }
}

impl Default for GeminiCliProvider {
    fn default() -> Self {
        Self::new(None, None, None)
    }
}

#[async_trait]
impl LLMProvider for GeminiCliProvider {
    async fn execute(
        &self,
        request: ChatCompletionRequest,
        _state: &AppState,
    ) -> ProviderResult<ChatCompletionResponse> {
        let request_id = Uuid::new_v4().to_string();
        info!("Gemini CLI: Executing non-streaming request {}", request_id);

        // Convert OpenAI messages to Gemini CLI prompt
        let prompt = Self::convert_messages_to_prompt(&request.messages)?;

        // Execute CLI command
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(self.timeout_secs),
            self.execute_cli_command(&prompt, Some(&request.model)),
        )
        .await
        .map_err(|_| ProviderError::Timeout("Gemini CLI request timed out".to_string()))??;

        // Parse response
        let cli_response = Self::parse_cli_response(&output)?;

        // Convert to OpenAI format
        let response = Self::create_openai_response(cli_response, &request, &request_id);

        Ok(response)
    }

    async fn execute_stream(
        &self,
        request: ChatCompletionRequest,
        _state: &AppState,
    ) -> ProviderResult<StreamingResponse> {
        let request_id = Uuid::new_v4().to_string();
        info!("Gemini CLI: Executing streaming request {}", request_id);

        // Convert OpenAI messages to Gemini CLI prompt
        let prompt = Self::convert_messages_to_prompt(&request.messages)?;

        // For streaming, we'll simulate it by returning the full response as a single chunk
        // Gemini CLI doesn't have native streaming support in non-interactive mode
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(self.timeout_secs),
            self.execute_cli_command(&prompt, Some(&request.model)),
        )
        .await
        .map_err(|_| {
            ProviderError::Timeout("Gemini CLI streaming request timed out".to_string())
        })??;

        let cli_response = Self::parse_cli_response(&output)?;

        // Create streaming response by simulating progressive token emission
        // Since Gemini CLI doesn't support native streaming, we chunk the response
        let content = cli_response.response;
        let created_timestamp = Self::current_unix_timestamp_secs();

        // Split content into reasonable chunks to simulate streaming
        let chunks = Self::create_streaming_chunks(
            &content,
            &request_id,
            &request.model,
            created_timestamp,
        )?;

        let stream = stream::iter(chunks.into_iter().map(Ok));

        Ok(Box::pin(stream))
    }

    fn provider_type(&self) -> Provider {
        Provider::GeminiCLI
    }

    fn supports_model(&self, model: &str) -> bool {
        model.starts_with("gemini-")
    }
}

impl GeminiCliProvider {
    fn create_streaming_chunks(
        content: &str,
        request_id: &str,
        model: &str,
        base_timestamp: u64,
    ) -> ProviderResult<Vec<String>> {
        const CHUNK_SIZE: usize = 50; // Characters per chunk for simulation
        const CHUNK_DELAY_MS: u64 = 10; // Simulated delay between chunks

        let mut chunks = Vec::new();
        let chars: Vec<char> = content.chars().collect();

        // Start with role chunk
        let role_chunk = crate::models::openai::ChatCompletionChunk {
            id: request_id.to_string(),
            object: "chat.completion.chunk".to_string(),
            created: base_timestamp,
            model: model.to_string(),
            choices: vec![crate::models::openai::ChatCompletionChunkChoice {
                index: 0,
                delta: DeltaMessage {
                    role: Some(Role::Assistant),
                    content: None,
                },
                finish_reason: None,
            }],
        };
        let role_json = serde_json::to_string(&role_chunk)
            .map_err(|e| ProviderError::Internal(format!("Failed to serialize role chunk: {e}")))?;
        chunks.push(format!("data: {role_json}\n\n"));

        // Content chunks
        let mut offset = 0;
        while offset < chars.len() {
            let end = (offset + CHUNK_SIZE).min(chars.len());
            let chunk_content: String = chars[offset..end].iter().collect();

            let content_chunk = crate::models::openai::ChatCompletionChunk {
                id: request_id.to_string(),
                object: "chat.completion.chunk".to_string(),
                created: base_timestamp + (offset as u64 * CHUNK_DELAY_MS / 1000),
                model: model.to_string(),
                choices: vec![crate::models::openai::ChatCompletionChunkChoice {
                    index: 0,
                    delta: DeltaMessage {
                        role: None,
                        content: Some(chunk_content),
                    },
                    finish_reason: if end == chars.len() {
                        Some("stop".to_string())
                    } else {
                        None
                    },
                }],
            };
            let content_json = serde_json::to_string(&content_chunk).map_err(|e| {
                ProviderError::Internal(format!("Failed to serialize content chunk: {e}"))
            })?;
            chunks.push(format!("data: {content_json}\n\n"));
            offset = end;
        }

        // Ensure we have at least one content chunk if content is empty
        if content.is_empty() {
            let empty_chunk = crate::models::openai::ChatCompletionChunk {
                id: request_id.to_string(),
                object: "chat.completion.chunk".to_string(),
                created: base_timestamp,
                model: model.to_string(),
                choices: vec![crate::models::openai::ChatCompletionChunkChoice {
                    index: 0,
                    delta: DeltaMessage {
                        role: None,
                        content: Some(String::new()),
                    },
                    finish_reason: Some("stop".to_string()),
                }],
            };
            let empty_json = serde_json::to_string(&empty_chunk).map_err(|e| {
                ProviderError::Internal(format!("Failed to serialize empty chunk: {e}"))
            })?;
            chunks.push(format!("data: {empty_json}\n\n"));
        }

        // Done marker
        chunks.push("data: [DONE]\n\n".to_string());

        Ok(chunks)
    }

    fn convert_messages_to_prompt(messages: &[ChatMessage]) -> Result<String, ProviderError> {
        let mut prompt_parts = Vec::new();

        for message in messages {
            match message.role {
                Role::System => {
                    prompt_parts.push(format!("System: {}", message.content));
                }
                Role::User => {
                    prompt_parts.push(format!("User: {}", message.content));
                }
                Role::Assistant => {
                    prompt_parts.push(format!("Assistant: {}", message.content));
                }
                Role::Tool => {
                    // For now, skip tool messages as Gemini CLI may not handle them
                    warn!("Tool messages not supported by Gemini CLI provider");
                }
            }
        }

        if prompt_parts.is_empty() {
            return Err(ProviderError::InvalidRequest(
                "No valid messages found".to_string(),
            ));
        }

        Ok(prompt_parts.join("\n\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supports_model() {
        let provider = GeminiCliProvider::default();
        assert!(provider.supports_model("gemini-pro"));
        assert!(provider.supports_model("gemini-2.5-flash"));
        assert!(provider.supports_model("gemini-1.5-pro"));
        assert!(!provider.supports_model("claude-3-opus"));
        assert!(!provider.supports_model("gpt-4"));
    }

    #[test]
    fn test_provider_type() {
        let provider = GeminiCliProvider::default();
        assert_eq!(provider.provider_type(), Provider::GeminiCLI);
    }

    #[test]
    fn test_convert_messages_to_prompt() {
        let messages = vec![
            ChatMessage {
                role: Role::System,
                content: "You are a helpful assistant".to_string(),
                name: None,
            },
            ChatMessage {
                role: Role::User,
                content: "Hello".to_string(),
                name: None,
            },
        ];

        let prompt = GeminiCliProvider::convert_messages_to_prompt(&messages)
            .expect("prompt conversion should succeed");
        assert!(prompt.contains("System: You are a helpful assistant"));
        assert!(prompt.contains("User: Hello"));
    }
}
