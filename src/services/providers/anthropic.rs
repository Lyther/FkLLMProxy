use async_trait::async_trait;
use futures::stream::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    models::openai::{
        ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse, ChatMessage, Role,
    },
    services::providers::{
        LLMProvider, Provider, ProviderError, ProviderResult, StreamingResponse,
    },
    state::AppState,
};

const DEFAULT_BRIDGE_URL: &str = "http://localhost:4001";
const ANTHROPIC_CHAT_ENDPOINT: &str = "/anthropic/chat";

#[derive(Serialize)]
struct AnthropicBridgeRequest {
    messages: Vec<crate::models::openai::ChatMessage>,
    model: String,
}

#[derive(Deserialize)]
struct AnthropicBridgeError {
    error: String,
}

pub struct AnthropicBridgeProvider {
    bridge_url: String,
}

impl AnthropicBridgeProvider {
    pub fn new(bridge_url: String) -> Self {
        Self { bridge_url }
    }
}

impl Default for AnthropicBridgeProvider {
    fn default() -> Self {
        Self::new(DEFAULT_BRIDGE_URL.to_string())
    }
}

#[async_trait]
impl LLMProvider for AnthropicBridgeProvider {
    async fn execute(
        &self,
        request: ChatCompletionRequest,
        state: &AppState,
    ) -> ProviderResult<ChatCompletionResponse> {
        let request_id = Uuid::new_v4().to_string();
        let model = request.model.clone();
        info!("Anthropic: Executing non-streaming request {}", request_id);

        let mut stream = self.execute_stream(request, state).await?;

        let mut full_content = String::new();
        let mut finish_reason = None;

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk_data) => {
                    for line in chunk_data.lines() {
                        let line = line.trim();
                        if let Some(json_data) = line.strip_prefix("data: ") {
                            if json_data == "[DONE]" {
                                continue;
                            }
                            if let Ok(chunk) =
                                serde_json::from_str::<ChatCompletionChunk>(json_data)
                            {
                                if let Some(choice) = chunk.choices.first() {
                                    if let Some(content) = &choice.delta.content {
                                        full_content.push_str(content);
                                    }
                                    if let Some(reason) = &choice.finish_reason {
                                        finish_reason = Some(reason.clone());
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    return Err(ProviderError::Internal(format!(
                        "Stream error while processing Anthropic response: {}",
                        e
                    )));
                }
            }
        }

        let response = ChatCompletionResponse {
            id: format!("chatcmpl-{}", request_id),
            object: "chat.completion".to_string(),
            created: chrono::Utc::now().timestamp() as u64,
            model,
            choices: vec![crate::models::openai::ChatCompletionChoice {
                index: 0,
                message: ChatMessage {
                    role: Role::Assistant,
                    content: full_content,
                    name: None,
                },
                finish_reason,
            }],
            usage: None,
        };

        Ok(response)
    }

    async fn execute_stream(
        &self,
        request: ChatCompletionRequest,
        state: &AppState,
    ) -> ProviderResult<StreamingResponse> {
        let request_id = Uuid::new_v4().to_string();
        info!("Anthropic: Executing streaming request {}", request_id);

        let client = Client::new();
        let bridge_request = AnthropicBridgeRequest {
            messages: request.messages.clone(),
            model: request.model.clone(),
        };

        let url = format!("{}{}", self.bridge_url, ANTHROPIC_CHAT_ENDPOINT);

        let response = state
            .circuit_breaker
            .call(async {
                let resp = client
                    .post(&url)
                    .json(&bridge_request)
                    .send()
                    .await
                    .map_err(|e| {
                        ProviderError::Network(format!(
                            "Failed to contact Anthropic bridge at {}: {}",
                            url, e
                        ))
                    })?;

                if !resp.status().is_success() {
                    let status = resp.status();
                    let error_text = resp.text().await.unwrap_or_else(|e| {
                        warn!("Failed to read error response: {}", e);
                        String::new()
                    });

                    if let Ok(error) = serde_json::from_str::<AnthropicBridgeError>(&error_text) {
                        return Err(ProviderError::Unavailable(format!(
                            "Anthropic bridge error (status: {}): {}",
                            status, error.error
                        )));
                    }

                    return Err(ProviderError::Unavailable(format!(
                        "Anthropic bridge HTTP {}: {}",
                        status, error_text
                    )));
                }

                Ok::<reqwest::Response, ProviderError>(resp)
            })
            .await?;

        let stream = response
            .bytes_stream()
            .map(move |chunk_result| match chunk_result {
                Ok(bytes) => {
                    let chunk_str = String::from_utf8_lossy(&bytes);
                    Ok::<String, Box<dyn std::error::Error + Send + Sync>>(chunk_str.to_string())
                }
                Err(e) => {
                    error!("Bridge stream error: {}", e);
                    Err(Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
                }
            });

        Ok(Box::pin(stream))
    }

    fn provider_type(&self) -> Provider {
        Provider::AnthropicCLI
    }

    fn supports_model(&self, model: &str) -> bool {
        model.starts_with("claude-")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        AnthropicConfig, AppConfig, AuthConfig, CacheConfig, CircuitBreakerConfig, LogConfig,
        OpenAIConfig, RateLimitConfig, ServerConfig, VertexConfig,
    };
    use crate::openai::circuit_breaker::CircuitBreaker;
    use crate::openai::metrics::Metrics;
    use crate::services::auth::TokenManager;
    use crate::services::cache::Cache;
    use crate::services::providers::ProviderRegistry;
    use std::sync::Arc;

    fn create_test_state(bridge_url: String) -> AppState {
        let config = AppConfig {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 4000,
                max_request_size: 10 * 1024 * 1024,
            },
            auth: AuthConfig {
                require_auth: false,
                master_key: "test-key".to_string(),
            },
            vertex: VertexConfig {
                project_id: None,
                region: "us-central1".to_string(),
                api_key: None,
                credentials_file: None,
                api_key_base_url: None,
                oauth_base_url: None,
            },
            log: LogConfig {
                level: "info".to_string(),
                format: "pretty".to_string(),
            },
            openai: OpenAIConfig {
                harvester_url: "http://localhost:3001".to_string(),
                access_token_ttl_secs: 3600,
                arkose_token_ttl_secs: 120,
            },
            anthropic: AnthropicConfig {
                bridge_url: bridge_url.clone(),
            },
            rate_limit: RateLimitConfig {
                capacity: 100,
                refill_per_second: 10,
            },
            circuit_breaker: CircuitBreakerConfig {
                failure_threshold: 10,
                timeout_secs: 60,
                success_threshold: 3,
            },
            cache: CacheConfig {
                enabled: false,
                default_ttl_secs: 3600,
            },
        };

        AppState {
            config: Arc::new(config.clone()),
            token_manager: TokenManager::new(None, None).unwrap(),
            provider_registry: Arc::new(ProviderRegistry::with_config(Some(
                config.anthropic.bridge_url.clone(),
            ))),
            rate_limiter: crate::middleware::rate_limit::RateLimiter::new(
                config.rate_limit.capacity,
                config.rate_limit.refill_per_second,
            ),
            circuit_breaker: Arc::new(CircuitBreaker::new(
                config.circuit_breaker.failure_threshold,
                config.circuit_breaker.timeout_secs,
                config.circuit_breaker.success_threshold,
            )),
            metrics: Arc::new(Metrics::new()),
            cache: Arc::new(Cache::new(false, 3600)),
        }
    }

    #[test]
    fn test_anthropic_provider_supports_model() {
        let provider = AnthropicBridgeProvider::new("http://localhost:4001".to_string());
        assert!(provider.supports_model("claude-3-5-sonnet"));
        assert!(provider.supports_model("claude-3-opus"));
        assert!(!provider.supports_model("gemini-pro"));
    }
}
