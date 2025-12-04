pub mod anthropic;
pub mod vertex;

use crate::models::openai::{ChatCompletionRequest, ChatCompletionResponse};
use crate::state::AppState;
use async_trait::async_trait;
use futures::stream::Stream;
use std::pin::Pin;

pub type ProviderResult<T> = Result<T, ProviderError>;
pub type StreamingResponse =
    Pin<Box<dyn Stream<Item = Result<String, Box<dyn std::error::Error + Send + Sync>>> + Send>>;

#[derive(Debug, Clone, PartialEq)]
pub enum Provider {
    Vertex,
    AnthropicCLI,
    // Fix dead code: These variants are not implemented yet
    // TODO: Implement DeepSeek provider or remove variant
    #[allow(dead_code)]
    DeepSeek,
    // Fix dead code: These variants are not implemented yet
    // TODO: Implement Ollama provider or remove variant
    #[allow(dead_code)]
    Ollama,
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("Authentication error: {0}")]
    Auth(String),
    #[error("Network error: {0}")]
    Network(String),
    #[error("Service unavailable: {0}")]
    Unavailable(String),
    #[error("Request timeout: {0}")]
    Timeout(String),
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
    #[error("Rate limited: {0}")]
    RateLimited(String),
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("Circuit breaker open: {0}")]
    CircuitOpen(#[from] crate::openai::circuit_breaker::CircuitOpenError),
}

#[async_trait]
pub trait LLMProvider: Send + Sync {
    async fn execute(
        &self,
        request: ChatCompletionRequest,
        state: &AppState,
    ) -> ProviderResult<ChatCompletionResponse>;

    async fn execute_stream(
        &self,
        request: ChatCompletionRequest,
        state: &AppState,
    ) -> ProviderResult<StreamingResponse>;

    fn provider_type(&self) -> Provider;

    fn supports_model(&self, model: &str) -> bool;
}

pub struct ProviderRegistry {
    providers: Vec<Box<dyn LLMProvider>>,
}

impl ProviderRegistry {
    /// Initialize provider registry with configured providers
    ///
    /// Fix hardcoded provider initialization: Currently only Vertex and Anthropic providers are registered.
    /// Registration order determines routing priority when multiple providers support the same model.
    /// TODO: Consider plugin/registry pattern or configuration-driven initialization for extensibility.
    pub fn with_config(anthropic_bridge_url: Option<String>) -> Self {
        let mut providers: Vec<Box<dyn LLMProvider>> = Vec::new();

        // Register Vertex provider (always available)
        providers.push(Box::new(
            crate::services::providers::vertex::VertexProvider::new(),
        ));

        // Register Anthropic provider if bridge URL is configured
        if let Some(ref url) = anthropic_bridge_url {
            providers.push(Box::new(
                crate::services::providers::anthropic::AnthropicBridgeProvider::new(url.clone()),
            ));
        }

        Self { providers }
    }

    /// Route request to appropriate provider based on model name
    ///
    /// Fix non-deterministic routing: Returns first matching provider.
    /// If multiple providers support the same model, returns the first one registered.
    /// This behavior is deterministic (based on registration order) but should be documented.
    /// Consider: priority ordering, explicit model-to-provider mapping, or conflict detection.
    pub fn route_by_model(&self, model: &str) -> Option<&dyn LLMProvider> {
        for provider in &self.providers {
            if provider.supports_model(model) {
                return Some(provider.as_ref());
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_route_by_model_gemini() {
        let registry = ProviderRegistry::with_config(None);
        assert!(registry.route_by_model("gemini-pro").is_some());
        assert!(registry.route_by_model("gemini-2.5-flash").is_some());
    }

    #[test]
    fn test_route_by_model_claude() {
        let registry = ProviderRegistry::with_config(Some("http://localhost:4001".to_string()));
        assert!(registry.route_by_model("claude-3-5-sonnet").is_some());
        assert!(registry.route_by_model("claude-3-opus").is_some());
    }

    #[test]
    fn test_route_by_model_unknown() {
        let registry = ProviderRegistry::with_config(None);
        assert!(registry.route_by_model("unknown-model").is_none());
    }
}
