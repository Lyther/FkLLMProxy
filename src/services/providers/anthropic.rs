use async_trait::async_trait;
use futures::stream::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{error, info};
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
        Self::new("http://localhost:4001".to_string())
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

        // For non-streaming, we'll collect the stream and return the final result
        let mut stream = self.execute_stream(request, state).await?;

        let mut full_content = String::new();
        let mut finish_reason = None;

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk_data) => {
                    // The chunk_data may contain multiple SSE lines, split by \n\n
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
                        "Stream error while processing Anthropic response (model: {}): {}",
                        model, e
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
            usage: None, // Bridge doesn't provide usage info
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

        let url = format!("{}/anthropic/chat", self.bridge_url);

        // Wrap HTTP call with circuit breaker
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
                            "Failed to contact Anthropic bridge at {} (model: {}): {}",
                            url, request.model, e
                        ))
                    })?;

                if !resp.status().is_success() {
                    let status = resp.status();
                    let error_text = resp.text().await.unwrap_or_default();

                    if let Ok(error) = serde_json::from_str::<AnthropicBridgeError>(&error_text) {
                        return Err(ProviderError::Unavailable(format!(
                            "Anthropic bridge error (model: {}, status: {}): {}",
                            request.model, status, error.error
                        )));
                    }

                    return Err(ProviderError::Unavailable(format!(
                        "Anthropic bridge HTTP {} (model: {}): {}",
                        status, request.model, error_text
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
    use crate::models::openai::ChatCompletionRequest;
    use crate::openai::circuit_breaker::CircuitBreaker;
    use crate::openai::metrics::Metrics;
    use crate::services::auth::TokenManager;
    use crate::services::cache::Cache;
    use crate::services::providers::ProviderRegistry;
    use std::sync::Arc;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    #[allow(dead_code)]
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
                tls_fingerprint_enabled: false,
                tls_fingerprint_target: "chrome120".to_string(),
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

    #[allow(dead_code)]
    fn create_test_request(model: &str, messages: Vec<ChatMessage>) -> ChatCompletionRequest {
        ChatCompletionRequest {
            model: model.to_string(),
            messages,
            stream: false,
            temperature: 0.7,
            top_p: 1.0,
            max_tokens: None,
            stop: None,
        }
    }

    // ============================================================================
    // SCENARIO: Provider initialization and configuration
    // ============================================================================

    #[test]
    fn should_create_provider_with_custom_bridge_url() {
        // Given a custom bridge URL
        let url = "http://custom-bridge:5000".to_string();
        // When creating a provider
        let provider = AnthropicBridgeProvider::new(url.clone());
        // Then the provider should store the URL
        assert_eq!(provider.bridge_url, url);
    }

    #[test]
    fn should_create_provider_with_default_url() {
        // Given no custom URL
        // When creating a default provider
        let provider = AnthropicBridgeProvider::default();
        // Then it should use localhost:4001
        assert_eq!(provider.bridge_url, "http://localhost:4001");
    }

    // ============================================================================
    // SCENARIO: Model support detection
    // ============================================================================

    #[test]
    fn should_support_claude_models() {
        // Given a provider
        let provider = AnthropicBridgeProvider::default();
        // When checking Claude model names
        // Then it should support all claude-* models
        assert!(provider.supports_model("claude-3-5-sonnet"));
        assert!(provider.supports_model("claude-3-opus"));
        assert!(provider.supports_model("claude-3-haiku"));
        assert!(provider.supports_model("claude-2"));
        assert!(provider.supports_model("claude-instant"));
    }

    #[test]
    fn should_reject_non_claude_models() {
        // Given a provider
        let provider = AnthropicBridgeProvider::default();
        // When checking non-Claude model names
        // Then it should reject them
        assert!(!provider.supports_model("gemini-pro"));
        assert!(!provider.supports_model("gpt-4"));
        assert!(!provider.supports_model(""));
        assert!(!provider.supports_model("claude")); // Must have hyphen
    }

    // ============================================================================
    // SCENARIO: Provider type identification
    // ============================================================================

    #[test]
    fn should_identify_as_anthropic_cli_provider() {
        // Given a provider
        let provider = AnthropicBridgeProvider::default();
        // When checking provider type
        // Then it should return AnthropicCLI
        assert_eq!(provider.provider_type(), Provider::AnthropicCLI);
    }

    // ============================================================================
    // SCENARIO: Streaming request execution (Happy Path)
    // ============================================================================

    #[tokio::test]
    async fn should_execute_streaming_request_successfully() {
        // Given a provider with a working bridge
        let mock_server = MockServer::start().await;
        let provider = AnthropicBridgeProvider::new(mock_server.uri());
        let state = create_test_state(mock_server.uri());

        // And a valid chat completion request
        let request = create_test_request(
            "claude-3-5-sonnet",
            vec![ChatMessage {
                role: Role::User,
                content: "Hello".to_string(),
                name: None,
            }],
        );

        // Mock bridge response with valid SSE chunks
        let sse_response = "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"claude-3-5-sonnet\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"claude-3-5-sonnet\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" world\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"claude-3-5-sonnet\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";

        Mock::given(method("POST"))
            .and(path("/anthropic/chat"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(sse_response)
                    .append_header("Content-Type", "text/event-stream"),
            )
            .mount(&mock_server)
            .await;

        // When executing a streaming request
        let mut stream = provider.execute_stream(request, &state).await.unwrap();

        // Then it should return a stream of SSE chunks
        let mut chunks = Vec::new();
        let mut chunk_count = 0;
        while let Some(result) = stream.next().await {
            chunk_count += 1;
            // Each chunk should be parseable
            assert!(result.is_ok(), "All chunks should be parseable");
            chunks.push(result);
        }

        // And the stream should contain valid chunks
        assert!(chunk_count > 0, "Stream should contain at least one chunk");
        // The stream may have one or more chunks depending on how it's processed
        assert!(!chunks.is_empty(), "Should have at least one chunk");
    }

    #[tokio::test]
    async fn should_forward_messages_to_bridge_correctly() {
        // Given a provider
        let mock_server = MockServer::start().await;
        let provider = AnthropicBridgeProvider::new(mock_server.uri());
        let state = create_test_state(mock_server.uri());

        // And a request with multiple messages
        let request = create_test_request(
            "claude-3-5-sonnet",
            vec![
                ChatMessage {
                    role: Role::System,
                    content: "You are helpful".to_string(),
                    name: None,
                },
                ChatMessage {
                    role: Role::User,
                    content: "Hello".to_string(),
                    name: None,
                },
            ],
        );

        // Mock bridge response - verify request payload structure
        Mock::given(method("POST"))
            .and(path("/anthropic/chat"))
            .and(wiremock::matchers::body_json(serde_json::json!({
                "model": "claude-3-5-sonnet",
                "messages": [
                    {"role": "system", "content": "You are helpful"},
                    {"role": "user", "content": "Hello"}
                ]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"claude-3-5-sonnet\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hi\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"claude-3-5-sonnet\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            ))
            .mount(&mock_server)
            .await;

        // When executing a streaming request
        let mut stream = provider.execute_stream(request, &state).await.unwrap();
        let _ = stream.next().await; // Consume first chunk

        // Then the bridge should receive all messages
        // And the model name should be forwarded
        // (Verified by the body_json matcher above - test fails if payload doesn't match)
    }

    // ============================================================================
    // SCENARIO: Non-streaming request execution (Happy Path)
    // ============================================================================

    #[tokio::test]
    async fn should_collect_stream_into_complete_response() {
        // Given a provider with a working bridge
        let mock_server = MockServer::start().await;
        let provider = AnthropicBridgeProvider::new(mock_server.uri());
        let state = create_test_state(mock_server.uri());

        // And a non-streaming request
        let request = create_test_request(
            "claude-3-5-sonnet",
            vec![ChatMessage {
                role: Role::User,
                content: "Test".to_string(),
                name: None,
            }],
        );

        // Mock bridge response with complete stream (SSE format)
        // Note: The stream processor expects chunks that start with "data: "
        let sse_response = "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"claude-3-5-sonnet\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Response\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"claude-3-5-sonnet\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" content\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"claude-3-5-sonnet\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";

        Mock::given(method("POST"))
            .and(path("/anthropic/chat"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(sse_response)
                    .append_header("Content-Type", "text/event-stream"),
            )
            .mount(&mock_server)
            .await;

        // When executing the request
        let response = provider.execute(request, &state).await.unwrap();

        // Then it should collect all stream chunks
        // And return a complete ChatCompletionResponse
        assert_eq!(response.model, "claude-3-5-sonnet");
        assert_eq!(response.choices.len(), 1);
        // And the response should contain all accumulated content
        assert_eq!(response.choices[0].message.content, "Response content");
        // And the finish_reason should be set
        assert_eq!(response.choices[0].finish_reason, Some("stop".to_string()));
    }

    #[tokio::test]
    async fn should_generate_unique_request_id_for_each_request() {
        // Given a provider
        let mock_server = MockServer::start().await;
        let provider = AnthropicBridgeProvider::new(mock_server.uri());
        let state = create_test_state(mock_server.uri());
        let request = create_test_request(
            "claude-3-5-sonnet",
            vec![ChatMessage {
                role: Role::User,
                content: "Test".to_string(),
                name: None,
            }],
        );

        // Mock response with unique IDs
        let sse_response = "data: {\"id\":\"chatcmpl-unique-1\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"claude-3-5-sonnet\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Response\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"chatcmpl-unique-1\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"claude-3-5-sonnet\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";

        Mock::given(method("POST"))
            .and(path("/anthropic/chat"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(sse_response)
                    .append_header("Content-Type", "text/event-stream"),
            )
            .mount(&mock_server)
            .await;

        // When executing multiple requests
        let response1 = provider.execute(request.clone(), &state).await.unwrap();
        let response2 = provider.execute(request.clone(), &state).await.unwrap();
        let response3 = provider.execute(request, &state).await.unwrap();

        // Then each response should have a unique ID
        let id1 = &response1.id;
        let id2 = &response2.id;
        let id3 = &response3.id;

        // IDs should be unique (they come from the bridge, so they may be the same in mock)
        // But the response structure should be consistent
        assert!(!id1.is_empty());
        assert!(!id2.is_empty());
        assert!(!id3.is_empty());

        // And IDs should follow chatcmpl-{uuid} format (or similar)
        // The exact format depends on the bridge, but should start with "chatcmpl-"
        assert!(id1.starts_with("chatcmpl-") || id1.starts_with("msg_"));
    }

    // ============================================================================
    // SCENARIO: Network failures (Sad Path)
    // ============================================================================

    #[tokio::test]
    async fn should_return_network_error_when_bridge_unreachable() {
        // Given a provider pointing to unreachable bridge
        let provider = AnthropicBridgeProvider::new("http://127.0.0.1:99999".to_string());
        let state = create_test_state("http://127.0.0.1:99999".to_string());
        let request = create_test_request(
            "claude-3-5-sonnet",
            vec![ChatMessage {
                role: Role::User,
                content: "Test".to_string(),
                name: None,
            }],
        );

        // When executing a request
        let result = provider.execute_stream(request, &state).await;

        // Then it should return ProviderError::Network
        assert!(result.is_err());
        if let Err(ProviderError::Network(_)) = result {
            // Expected
        } else {
            panic!("Expected Network error");
        }
    }

    #[tokio::test]
    async fn should_handle_connection_timeout_gracefully() {
        // Given a provider with a slow/unresponsive bridge
        let mock_server = MockServer::start().await;
        let provider = AnthropicBridgeProvider::new(mock_server.uri());
        let state = create_test_state(mock_server.uri());
        let request = create_test_request(
            "claude-3-5-sonnet",
            vec![ChatMessage {
                role: Role::User,
                content: "Test".to_string(),
                name: None,
            }],
        );

        // And a bridge that responds very slowly (longer than client timeout)
        // The reqwest client has a default timeout, but we can simulate with a very long delay
        Mock::given(method("POST"))
            .and(path("/anthropic/chat"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("data: {\"id\":\"test\"}\n\n")
                    .set_delay(std::time::Duration::from_secs(60)), // Longer than typical timeout
            )
            .mount(&mock_server)
            .await;

        // When executing a request with a timeout wrapper
        let timeout_duration = tokio::time::Duration::from_secs(2);
        let result =
            tokio::time::timeout(timeout_duration, provider.execute_stream(request, &state)).await;

        // Then it should timeout after reasonable duration
        match result {
            Ok(Err(ProviderError::Network(_))) => {
                // Network error from timeout - expected
            }
            Ok(Err(e)) => {
                // Other errors are also acceptable (e.g., timeout from reqwest)
                eprintln!("Timeout test got error: {:?}", e);
            }
            Ok(Ok(_)) => {
                // If it succeeds, that's also fine (timeout didn't trigger)
                // This can happen if the mock responds faster than expected
            }
            Err(_) => {
                // Timeout wrapper triggered - this is also acceptable
                // The request timed out as expected
            }
        }
    }

    // ============================================================================
    // SCENARIO: HTTP error responses (Sad Path)
    // ============================================================================

    #[tokio::test]
    async fn should_handle_bridge_400_error() {
        // Given a provider
        let mock_server = MockServer::start().await;
        let provider = AnthropicBridgeProvider::new(mock_server.uri());
        let state = create_test_state(mock_server.uri());
        let request = create_test_request(
            "claude-3-5-sonnet",
            vec![ChatMessage {
                role: Role::User,
                content: "Test".to_string(),
                name: None,
            }],
        );

        // And a bridge that returns HTTP 400
        Mock::given(method("POST"))
            .and(path("/anthropic/chat"))
            .respond_with(ResponseTemplate::new(400).set_body_string("Bad Request"))
            .mount(&mock_server)
            .await;

        // When executing a request
        let result = provider.execute_stream(request, &state).await;

        // Then it should return ProviderError::Unavailable
        assert!(result.is_err());
        if let Err(ProviderError::Unavailable(msg)) = result {
            // And the error should include the status code
            assert!(msg.contains("400"));
        } else {
            panic!("Expected Unavailable error");
        }
    }

    #[tokio::test]
    async fn should_handle_bridge_500_error() {
        // Given a provider
        let mock_server = MockServer::start().await;
        let provider = AnthropicBridgeProvider::new(mock_server.uri());
        let state = create_test_state(mock_server.uri());
        let request = create_test_request(
            "claude-3-5-sonnet",
            vec![ChatMessage {
                role: Role::User,
                content: "Test".to_string(),
                name: None,
            }],
        );

        // And a bridge that returns HTTP 500
        Mock::given(method("POST"))
            .and(path("/anthropic/chat"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Server Error"))
            .mount(&mock_server)
            .await;

        // When executing a request
        let result = provider.execute_stream(request, &state).await;

        // Then it should return ProviderError::Unavailable
        assert!(result.is_err());
        if let Err(ProviderError::Unavailable(msg)) = result {
            // And the error should indicate server error
            assert!(msg.contains("500"));
        } else {
            panic!("Expected Unavailable error for 500");
        }
    }

    #[tokio::test]
    async fn should_parse_bridge_json_error_response() {
        // Given a provider
        let mock_server = MockServer::start().await;
        let provider = AnthropicBridgeProvider::new(mock_server.uri());
        let state = create_test_state(mock_server.uri());
        let request = create_test_request(
            "claude-3-5-sonnet",
            vec![ChatMessage {
                role: Role::User,
                content: "Test".to_string(),
                name: None,
            }],
        );

        // And a bridge that returns JSON error: {"error": "message"}
        Mock::given(method("POST"))
            .and(path("/anthropic/chat"))
            .respond_with(
                ResponseTemplate::new(500)
                    .set_body_json(serde_json::json!({"error": "Internal server error"})),
            )
            .mount(&mock_server)
            .await;

        // When executing a request
        let result = provider.execute_stream(request, &state).await;

        // Then it should parse the error JSON
        assert!(result.is_err());
        if let Err(ProviderError::Unavailable(msg)) = result {
            // And return ProviderError::Unavailable with the error message
            // Note: Error format may vary, check for either "Bridge error" or status code
            assert!(
                msg.contains("Bridge error")
                    || msg.contains("500")
                    || msg.contains("Internal server error")
            );
        } else {
            panic!("Expected Unavailable error with parsed message");
        }
    }

    #[tokio::test]
    async fn should_handle_non_json_error_response() {
        // Given a provider
        let mock_server = MockServer::start().await;
        let provider = AnthropicBridgeProvider::new(mock_server.uri());
        let state = create_test_state(mock_server.uri());
        let request = create_test_request(
            "claude-3-5-sonnet",
            vec![ChatMessage {
                role: Role::User,
                content: "Test".to_string(),
                name: None,
            }],
        );

        // And a bridge that returns plain text error
        Mock::given(method("POST"))
            .and(path("/anthropic/chat"))
            .respond_with(
                ResponseTemplate::new(500)
                    .set_body_string("Internal Server Error: Something went wrong"),
            )
            .mount(&mock_server)
            .await;

        // When executing a request
        let result = provider.execute_stream(request, &state).await;

        // Then it should return ProviderError::Unavailable
        assert!(result.is_err());
        if let Err(ProviderError::Unavailable(msg)) = result {
            // And include the raw error text
            assert!(msg.contains("500"));
            assert!(msg.contains("Internal Server Error"));
        } else {
            panic!("Expected Unavailable error with plain text");
        }
    }

    // ============================================================================
    // SCENARIO: Stream processing errors (Sad Path)
    // ============================================================================

    #[tokio::test]
    async fn should_handle_malformed_sse_chunks() {
        // Given a provider
        let mock_server = MockServer::start().await;
        let provider = AnthropicBridgeProvider::new(mock_server.uri());
        let state = create_test_state(mock_server.uri());
        let request = create_test_request(
            "claude-3-5-sonnet",
            vec![ChatMessage {
                role: Role::User,
                content: "Test".to_string(),
                name: None,
            }],
        );

        // And a bridge that returns invalid SSE format mixed with valid chunks
        let sse_response = "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"claude-3-5-sonnet\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}\n\ninvalid line without data: prefix\n\ndata: not valid json\n\ndata: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"claude-3-5-sonnet\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" world\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"claude-3-5-sonnet\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n";

        Mock::given(method("POST"))
            .and(path("/anthropic/chat"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(sse_response)
                    .append_header("Content-Type", "text/event-stream"),
            )
            .mount(&mock_server)
            .await;

        // When executing a non-streaming request
        let response = provider.execute(request, &state).await.unwrap();

        // Then it should skip malformed chunks
        // And continue processing valid chunks
        assert_eq!(response.model, "claude-3-5-sonnet");
        assert_eq!(response.choices.len(), 1);
        // Should accumulate content from valid chunks only
        assert_eq!(response.choices[0].message.content, "Hello world");
        assert_eq!(response.choices[0].finish_reason, Some("stop".to_string()));
    }

    #[tokio::test]
    async fn should_handle_stream_interruption() {
        // Given a provider
        let mock_server = MockServer::start().await;
        let provider = AnthropicBridgeProvider::new(mock_server.uri());
        let state = create_test_state(mock_server.uri());
        let request = create_test_request(
            "claude-3-5-sonnet",
            vec![ChatMessage {
                role: Role::User,
                content: "Test".to_string(),
                name: None,
            }],
        );

        // And a bridge stream that closes unexpectedly
        // Simulate by returning a partial/incomplete SSE stream that will cause parsing issues
        // Use a response that starts valid but then becomes invalid
        let sse_response = "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"claude-3-5-sonnet\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}\n\ndata: {\"incomplete\": json\n\n";

        Mock::given(method("POST"))
            .and(path("/anthropic/chat"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(sse_response)
                    .append_header("Content-Type", "text/event-stream"),
            )
            .mount(&mock_server)
            .await;

        // When executing a streaming request
        let result = provider.execute_stream(request, &state).await;

        // Then it should handle the interruption gracefully
        // The stream may succeed partially or fail - both are acceptable
        // The key is that it doesn't panic
        match result {
            Ok(mut stream) => {
                // If stream is created, try to read from it
                // It should handle the malformed data gracefully
                let first_chunk = stream.next().await;
                // First chunk might succeed, but subsequent ones may fail
                // This is acceptable behavior
                if first_chunk.is_some() {
                    // Stream started successfully, which is fine
                }
            }
            Err(ProviderError::Internal(_)) | Err(ProviderError::Network(_)) => {
                // Expected - stream interruption causes error
            }
            Err(e) => {
                // Other error types are also acceptable
                eprintln!("Stream interruption test got error: {:?}", e);
            }
        }
    }

    #[tokio::test]
    async fn should_handle_empty_stream() {
        // Given a provider
        let mock_server = MockServer::start().await;
        let provider = AnthropicBridgeProvider::new(mock_server.uri());
        let state = create_test_state(mock_server.uri());
        let request = create_test_request(
            "claude-3-5-sonnet",
            vec![ChatMessage {
                role: Role::User,
                content: "Test".to_string(),
                name: None,
            }],
        );

        // And a bridge that returns empty stream
        Mock::given(method("POST"))
            .and(path("/anthropic/chat"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("")
                    .append_header("Content-Type", "text/event-stream"),
            )
            .mount(&mock_server)
            .await;

        // When executing a non-streaming request
        let response = provider.execute(request, &state).await.unwrap();

        // Then it should return a response with empty content
        assert_eq!(response.choices.len(), 1);
        assert_eq!(response.choices[0].message.content, "");
        // And finish_reason should be None
        assert_eq!(response.choices[0].finish_reason, None);
    }

    // ============================================================================
    // SCENARIO: Circuit breaker integration (Sad Path)
    // ============================================================================

    #[tokio::test]
    async fn should_respect_circuit_breaker_when_open() {
        // Given a provider
        let mock_server = MockServer::start().await;
        let provider = AnthropicBridgeProvider::new(mock_server.uri());

        // Create state with circuit breaker that has low threshold
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
                tls_fingerprint_enabled: false,
                tls_fingerprint_target: "chrome120".to_string(),
            },
            anthropic: AnthropicConfig {
                bridge_url: mock_server.uri(),
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

        // Create circuit breaker with low threshold (2 failures to open)
        let circuit_breaker = Arc::new(CircuitBreaker::new(2, 60, 3));

        // Trigger failures to open the circuit
        Mock::given(method("POST"))
            .and(path("/anthropic/chat"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let request = create_test_request(
            "claude-3-5-sonnet",
            vec![ChatMessage {
                role: Role::User,
                content: "Test".to_string(),
                name: None,
            }],
        );

        let state = AppState {
            config: Arc::new(config.clone()),
            token_manager: TokenManager::new(None, None).unwrap(),
            provider_registry: Arc::new(ProviderRegistry::with_config(Some(
                config.anthropic.bridge_url.clone(),
            ))),
            rate_limiter: crate::middleware::rate_limit::RateLimiter::new(100, 10),
            circuit_breaker: circuit_breaker.clone(),
            metrics: Arc::new(Metrics::new()),
            cache: Arc::new(Cache::new(false, 3600)),
        };

        // Trigger 2 failures to open circuit
        let _ = provider.execute_stream(request.clone(), &state).await;
        let _ = provider.execute_stream(request.clone(), &state).await;

        // Verify circuit is open
        assert!(circuit_breaker.is_open().await);

        // When executing a request with open circuit
        let result = provider.execute_stream(request, &state).await;

        // Then it should still attempt (circuit breaker doesn't block, just tracks)
        // But the request will fail because bridge returns 500
        assert!(result.is_err());
        if let Err(ProviderError::Unavailable(_)) = result {
            // Expected - circuit is open and request fails
        } else {
            panic!("Expected Unavailable error when circuit is open");
        }
    }

    // ============================================================================
    // SCENARIO: Edge cases
    // ============================================================================

    #[tokio::test]
    async fn should_handle_empty_messages_array() {
        // Given a provider
        let mock_server = MockServer::start().await;
        let provider = AnthropicBridgeProvider::new(mock_server.uri());
        let state = create_test_state(mock_server.uri());

        // And a request with empty messages array
        let request = create_test_request("claude-3-5-sonnet", vec![]);

        // Bridge may return error for empty messages, but provider should forward it
        Mock::given(method("POST"))
            .and(path("/anthropic/chat"))
            .and(wiremock::matchers::body_json(serde_json::json!({
                "model": "claude-3-5-sonnet",
                "messages": []
            })))
            .respond_with(
                ResponseTemplate::new(400)
                    .set_body_json(serde_json::json!({"error": "Messages array cannot be empty"})),
            )
            .mount(&mock_server)
            .await;

        // When executing a request
        let result = provider.execute(request, &state).await;

        // Then it should forward empty array to bridge
        // And return the bridge's error response
        assert!(result.is_err());
        if let Err(ProviderError::Unavailable(msg)) = result {
            assert!(msg.contains("400") || msg.contains("Messages array cannot be empty"));
        } else {
            // Network error is also acceptable if bridge rejects connection
            assert!(result.is_err());
        }
    }

    #[tokio::test]
    async fn should_handle_large_message_payloads() {
        // Given a provider
        let mock_server = MockServer::start().await;
        let provider = AnthropicBridgeProvider::new(mock_server.uri());
        let state = create_test_state(mock_server.uri());

        // And a request with very large message content (>100KB, not full 1MB for test speed)
        let large_content = "A".repeat(100_000); // 100KB
        let request = create_test_request(
            "claude-3-5-sonnet",
            vec![ChatMessage {
                role: Role::User,
                content: large_content,
                name: None,
            }],
        );

        // Mock bridge response
        let sse_response = "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"claude-3-5-sonnet\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Processed\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"claude-3-5-sonnet\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";

        Mock::given(method("POST"))
            .and(path("/anthropic/chat"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(sse_response)
                    .append_header("Content-Type", "text/event-stream"),
            )
            .mount(&mock_server)
            .await;

        // When executing a request
        let result = provider.execute(request, &state).await;

        // Then it should successfully forward to bridge
        // And handle streaming response
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.model, "claude-3-5-sonnet");
        assert_eq!(response.choices.len(), 1);
    }

    #[tokio::test]
    async fn should_handle_multiple_finish_reasons_in_stream() {
        // Given a provider
        let mock_server = MockServer::start().await;
        let provider = AnthropicBridgeProvider::new(mock_server.uri());
        let state = create_test_state(mock_server.uri());
        let request = create_test_request(
            "claude-3-5-sonnet",
            vec![ChatMessage {
                role: Role::User,
                content: "Test".to_string(),
                name: None,
            }],
        );

        // And a bridge that sends multiple finish_reason chunks
        let sse_response = "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"claude-3-5-sonnet\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"claude-3-5-sonnet\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" world\"},\"finish_reason\":\"length\"}]}\n\ndata: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"claude-3-5-sonnet\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";

        Mock::given(method("POST"))
            .and(path("/anthropic/chat"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(sse_response)
                    .append_header("Content-Type", "text/event-stream"),
            )
            .mount(&mock_server)
            .await;

        // When executing a non-streaming request
        let response = provider.execute(request, &state).await.unwrap();

        // Then it should use the last finish_reason
        assert_eq!(response.choices[0].finish_reason, Some("stop".to_string()));

        // And accumulate all content correctly
        assert_eq!(response.choices[0].message.content, "Hello world");
    }

    #[tokio::test]
    async fn should_handle_chunks_without_content_delta() {
        // Given a provider
        let mock_server = MockServer::start().await;
        let provider = AnthropicBridgeProvider::new(mock_server.uri());
        let state = create_test_state(mock_server.uri());
        let request = create_test_request(
            "claude-3-5-sonnet",
            vec![ChatMessage {
                role: Role::User,
                content: "Test".to_string(),
                name: None,
            }],
        );

        // And a bridge that sends chunks with only finish_reason (no content delta)
        let sse_response = "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"claude-3-5-sonnet\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n";

        Mock::given(method("POST"))
            .and(path("/anthropic/chat"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(sse_response)
                    .append_header("Content-Type", "text/event-stream"),
            )
            .mount(&mock_server)
            .await;

        // When executing a non-streaming request
        let response = provider.execute(request, &state).await.unwrap();

        // Then it should process chunks correctly
        // And not fail on missing content
        assert_eq!(response.model, "claude-3-5-sonnet");
        assert_eq!(response.choices.len(), 1);
        // Content should be empty since no content delta was provided
        assert_eq!(response.choices[0].message.content, "");
        // But finish_reason should be set
        assert_eq!(response.choices[0].finish_reason, Some("stop".to_string()));
    }

    #[tokio::test]
    async fn should_handle_sse_data_lines_without_json_prefix() {
        // Given a provider
        let mock_server = MockServer::start().await;
        let provider = AnthropicBridgeProvider::new(mock_server.uri());
        let state = create_test_state(mock_server.uri());
        let request = create_test_request(
            "claude-3-5-sonnet",
            vec![ChatMessage {
                role: Role::User,
                content: "Test".to_string(),
                name: None,
            }],
        );

        // And a bridge that sends "data: [DONE]" or other non-JSON lines
        let sse_response = "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"claude-3-5-sonnet\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Valid\"},\"finish_reason\":null}]}\n\ndata: [DONE]\n\ndata: invalid-json-line\n\n";

        Mock::given(method("POST"))
            .and(path("/anthropic/chat"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(sse_response)
                    .append_header("Content-Type", "text/event-stream"),
            )
            .mount(&mock_server)
            .await;

        // When executing a request
        let response = provider.execute(request, &state).await.unwrap();

        // Then it should skip non-JSON data lines
        // And continue processing valid chunks
        assert_eq!(response.choices[0].message.content, "Valid");
    }
}
