use async_trait::async_trait;
use futures::stream::StreamExt;
use reqwest::Client;
use std::time::Duration;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    models::{
        openai::{ChatCompletionRequest, ChatCompletionResponse},
        vertex::GenerateContentResponse,
    },
    services::{
        providers::{LLMProvider, Provider, ProviderError, ProviderResult, StreamingResponse},
        transformer::{transform_request, transform_response, transform_stream_chunk},
    },
    state::AppState,
};

const API_KEY_BASE_URL: &str = "https://generativelanguage.googleapis.com";
const NON_STREAMING_TIMEOUT_SECS: u64 = 30;
const STREAMING_TIMEOUT_SECS: u64 = 60;
const UNKNOWN_PROJECT_ID: &str = "unknown";

struct VertexUrlBuilder;

impl VertexUrlBuilder {
    fn build_api_key_url(
        api_base: &str,
        model: &str,
        token: &str,
        streaming: bool,
    ) -> (String, String) {
        let base_url = format!("{}/v1beta/models/{}", api_base, model);
        let query = if streaming {
            format!("?key={}&alt=sse", token)
        } else {
            format!("?key={}", token)
        };
        (base_url, query)
    }

    fn build_oauth_url(
        oauth_base: Option<&String>,
        project_id: &str,
        region: &str,
        model: &str,
        streaming: bool,
    ) -> (String, String) {
        let base = oauth_base
            .map(|url| url.trim_end_matches('/').to_string())
            .unwrap_or_else(|| format!("https://{}-aiplatform.googleapis.com", region));

        let base_url = format!(
            "{}/v1/projects/{}/locations/{}/publishers/google/models/{}",
            base, project_id, region, model
        );

        let query = if streaming { "?alt=sse" } else { "" };
        (base_url, query.to_string())
    }

    fn build_url(
        config: &crate::config::VertexConfig,
        token_manager: &crate::services::auth::TokenManager,
        model: &str,
        token: &str,
        streaming: bool,
    ) -> (String, String) {
        let is_api_key = token_manager.is_api_key();

        if is_api_key {
            let api_base = config
                .api_key_base_url
                .as_ref()
                .map(|url| url.trim_end_matches('/').to_string())
                .unwrap_or_else(|| API_KEY_BASE_URL.to_string());
            Self::build_api_key_url(&api_base, model, token, streaming)
        } else {
            let project_id = token_manager
                .get_project_id()
                .map(|s| s.to_string())
                .unwrap_or_else(|| UNKNOWN_PROJECT_ID.to_string());
            Self::build_oauth_url(
                config.oauth_base_url.as_ref(),
                &project_id,
                &config.region,
                model,
                streaming,
            )
        }
    }
}

pub struct VertexProvider;

impl VertexProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for VertexProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LLMProvider for VertexProvider {
    async fn execute(
        &self,
        request: ChatCompletionRequest,
        state: &AppState,
    ) -> ProviderResult<ChatCompletionResponse> {
        let request_id = Uuid::new_v4().to_string();
        info!("Vertex: Executing non-streaming request {}", request_id);

        let token = state
            .token_manager
            .get_token()
            .await
            .map_err(|e| ProviderError::Auth(e.to_string()))?;

        let vertex_req = transform_request(request.clone())
            .map_err(|e| ProviderError::InvalidRequest(e.to_string()))?;

        let client = Client::builder()
            .timeout(Duration::from_secs(NON_STREAMING_TIMEOUT_SECS))
            .build()
            .map_err(|e| ProviderError::Internal(format!("Failed to create HTTP client: {}", e)))?;

        let (base_url, query_param) = VertexUrlBuilder::build_url(
            &state.config.vertex,
            &state.token_manager,
            &request.model,
            &token,
            false,
        );

        let url = format!("{}:generateContent{}", base_url, query_param);

        let mut req_builder = client.post(&url).json(&vertex_req);
        if !state.token_manager.is_api_key() {
            req_builder = req_builder.bearer_auth(&token);
        }

        let res = req_builder.send().await.map_err(|e| {
            if e.is_timeout() {
                ProviderError::Unavailable(format!(
                    "Vertex API request timeout (model: {}, request_id: {}): {}",
                    request.model, request_id, e
                ))
            } else {
                ProviderError::Network(format!(
                    "Vertex API request failed (model: {}, request_id: {}): {}",
                    request.model, request_id, e
                ))
            }
        })?;

        if !res.status().is_success() {
            let status = res.status();
            let text = res.text().await.unwrap_or_else(|e| {
                warn!("Failed to read Vertex error response: {}", e);
                String::new()
            });
            error!("Vertex API error: {} - {}", status, text);
            return Err(ProviderError::Unavailable(format!(
                "Vertex API Error (model: {}, request_id: {}, status: {}): {}",
                request.model, request_id, status, text
            )));
        }

        let vertex_res: GenerateContentResponse = res.json().await.map_err(|e| {
            ProviderError::Internal(format!(
                "Failed to parse Vertex response (model: {}, request_id: {}): {}",
                request.model, request_id, e
            ))
        })?;

        let response = transform_response(vertex_res, request.model.clone(), request_id.clone()).map_err(|e| {
            ProviderError::Internal(format!(
                "Failed to transform Vertex response to OpenAI format (model: {}, request_id: {}): {}",
                request.model, request_id, e
            ))
        })?;

        Ok(response)
    }

    async fn execute_stream(
        &self,
        request: ChatCompletionRequest,
        state: &AppState,
    ) -> ProviderResult<StreamingResponse> {
        let request_id = Uuid::new_v4().to_string();
        info!("Vertex: Executing streaming request {}", request_id);

        let token = state
            .token_manager
            .get_token()
            .await
            .map_err(|e| ProviderError::Auth(e.to_string()))?;

        let vertex_req = transform_request(request.clone())
            .map_err(|e| ProviderError::InvalidRequest(e.to_string()))?;

        let client = Client::builder()
            .timeout(Duration::from_secs(STREAMING_TIMEOUT_SECS))
            .build()
            .map_err(|e| ProviderError::Internal(format!("Failed to create HTTP client: {}", e)))?;

        let (base_url, query_param) = VertexUrlBuilder::build_url(
            &state.config.vertex,
            &state.token_manager,
            &request.model,
            &token,
            true,
        );

        let url = format!("{}:streamGenerateContent{}", base_url, query_param);

        let mut req_builder = client.post(&url).json(&vertex_req);
        if !state.token_manager.is_api_key() {
            req_builder = req_builder.bearer_auth(&token);
        }

        let res = req_builder.send().await.map_err(|e| {
            if e.is_timeout() {
                ProviderError::Unavailable(format!(
                    "Vertex API request timeout (model: {}, request_id: {}): {}",
                    request.model, request_id, e
                ))
            } else {
                ProviderError::Network(format!(
                    "Vertex API request failed (model: {}, request_id: {}): {}",
                    request.model, request_id, e
                ))
            }
        })?;

        if !res.status().is_success() {
            let status = res.status();
            let text = res.text().await.unwrap_or_else(|e| {
                warn!("Failed to read Vertex error response: {}", e);
                String::new()
            });
            error!("Vertex API error: {} - {}", status, text);
            return Err(ProviderError::Unavailable(format!(
                "Vertex API Error (model: {}, request_id: {}, status: {}): {}",
                request.model, request_id, status, text
            )));
        }

        let model = request.model.clone();
        let request_id_clone = request_id.clone();
        let stream = res
            .bytes_stream()
            .map(move |chunk_result| match chunk_result {
                Ok(bytes) => {
                    let s = String::from_utf8_lossy(&bytes);
                    let cleaned = s
                        .trim()
                        .trim_start_matches("data: ")
                        .trim()
                        .trim_start_matches('[')
                        .trim_start_matches(',')
                        .trim_end_matches(',')
                        .trim_end_matches(']');

                    if cleaned.is_empty() {
                        return Ok::<String, Box<dyn std::error::Error + Send + Sync>>(
                            "data: {\"comment\": \"keep-alive\"}\n\n".to_string(),
                        );
                    }

                    match serde_json::from_str::<GenerateContentResponse>(cleaned) {
                        Ok(vertex_res) => {
                            match transform_stream_chunk(
                                vertex_res,
                                model.clone(),
                                request_id_clone.clone(),
                            ) {
                                Ok(openai_chunk) => match serde_json::to_string(&openai_chunk) {
                                    Ok(chunk_data) => {
                                        Ok::<String, Box<dyn std::error::Error + Send + Sync>>(
                                            format!("data: {}\n\n", chunk_data),
                                        )
                                    }
                                    Err(e) => {
                                        error!("Transform error: {}", e);
                                        Ok::<String, Box<dyn std::error::Error + Send + Sync>>(
                                            format!("data: {{\"error\": \"{}\"}}", e),
                                        )
                                    }
                                },
                                Err(e) => {
                                    error!("Transform error: {}", e);
                                    Ok::<String, Box<dyn std::error::Error + Send + Sync>>(format!(
                                        "data: {{\"error\": \"{}\"}}",
                                        e
                                    ))
                                }
                            }
                        }
                        Err(e) => {
                            error!("Parse error: {}", e);
                            Ok::<String, Box<dyn std::error::Error + Send + Sync>>(
                                "data: {\"comment\": \"parse-error\"}\n\n".to_string(),
                            )
                        }
                    }
                }
                Err(e) => {
                    error!("Stream error: {}", e);
                    Ok::<String, Box<dyn std::error::Error + Send + Sync>>(format!(
                        "data: {{\"error\": \"stream-error: {}\"}}",
                        e
                    ))
                }
            });

        Ok(Box::pin(stream))
    }

    fn provider_type(&self) -> Provider {
        Provider::Vertex
    }

    fn supports_model(&self, model: &str) -> bool {
        model.starts_with("gemini-")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        AnthropicConfig, AppConfig, AuthConfig, CacheConfig, CircuitBreakerConfig, LogConfig,
        OpenAIConfig, RateLimitConfig, ServerConfig, VertexConfig,
    };
    use crate::services::auth::TokenManager;
    use crate::services::cache::Cache;
    use crate::services::providers::ProviderRegistry;
    use std::sync::Arc;

    fn create_test_state() -> AppState {
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
                bridge_url: "http://localhost:4001".to_string(),
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
            config: Arc::new(config),
            token_manager: TokenManager::new(None, None)
                .expect("Failed to initialize TokenManager in test"),
            provider_registry: Arc::new(ProviderRegistry::with_config(None)),
            rate_limiter: crate::middleware::rate_limit::RateLimiter::new(100, 10),
            circuit_breaker: Arc::new(crate::openai::circuit_breaker::CircuitBreaker::new(
                10, 60, 3,
            )),
            metrics: Arc::new(crate::openai::metrics::Metrics::new()),
            cache: Arc::new(Cache::new(false, 3600)),
        }
    }

    #[test]
    fn test_vertex_provider_supports_model() {
        let provider = VertexProvider::new();
        assert!(provider.supports_model("gemini-pro"));
        assert!(provider.supports_model("gemini-2.5-flash"));
        assert!(!provider.supports_model("claude-3-5-sonnet"));
    }

    #[test]
    fn test_vertex_provider_with_state() {
        let state = create_test_state();
        let provider = VertexProvider::new();
        assert_eq!(provider.provider_type(), Provider::Vertex);
        assert!(provider.supports_model("gemini-pro"));
        assert_eq!(state.config.vertex.region, "us-central1");
    }
}
