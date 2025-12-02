use async_trait::async_trait;
use futures::Stream;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

use crate::models::openai::{ChatCompletionRequest, ChatCompletionResponse};
use crate::state::AppState;

pub mod anthropic;
pub mod vertex;

pub use anthropic::AnthropicBridgeProvider;
pub use vertex::VertexProvider;

pub type ProviderResult<T> = Result<T, ProviderError>;
pub type StreamingResponse =
    Pin<Box<dyn Stream<Item = Result<String, Box<dyn std::error::Error + Send + Sync>>> + Send>>;

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum Provider {
    Vertex,
    AnthropicCLI,
    DeepSeek,
    Ollama,
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("Network error: {0}")]
    Network(String),
    #[error("Authentication error: {0}")]
    Auth(String),
    #[error("Provider unavailable: {0}")]
    Unavailable(String),
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
    #[error("Rate limited: {0}")]
    RateLimited(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

#[async_trait]
pub trait LLMProvider: Send + Sync {
    /// Execute a chat completion request (non-streaming)
    async fn execute(
        &self,
        request: ChatCompletionRequest,
        state: &AppState,
    ) -> ProviderResult<ChatCompletionResponse>;

    /// Execute a streaming chat completion request
    async fn execute_stream(
        &self,
        request: ChatCompletionRequest,
        state: &AppState,
    ) -> ProviderResult<StreamingResponse>;

    /// Get the provider type
    fn provider_type(&self) -> Provider;

    /// Check if provider supports the given model
    fn supports_model(&self, model: &str) -> bool;
}

pub struct ProviderRegistry {
    providers: HashMap<Provider, Arc<dyn LLMProvider>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self::with_config(None)
    }

    pub fn with_config(bridge_url: Option<String>) -> Self {
        let mut providers = HashMap::new();

        // Register providers
        providers.insert(
            Provider::Vertex,
            Arc::new(VertexProvider::new()) as Arc<dyn LLMProvider>,
        );

        let anthropic_provider = match bridge_url {
            Some(url) => AnthropicBridgeProvider::new(url),
            None => AnthropicBridgeProvider::default(),
        };
        providers.insert(
            Provider::AnthropicCLI,
            Arc::new(anthropic_provider) as Arc<dyn LLMProvider>,
        );

        Self { providers }
    }

    pub fn get_provider(&self, provider: &Provider) -> Option<Arc<dyn LLMProvider>> {
        self.providers.get(provider).cloned()
    }

    pub fn route_by_model(&self, model: &str) -> Option<Arc<dyn LLMProvider>> {
        let provider_type = route_provider(model);
        self.get_provider(&provider_type)
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub fn route_provider(model: &str) -> Provider {
    if model.starts_with("gemini-") {
        Provider::Vertex
    } else if model.starts_with("claude-") {
        Provider::AnthropicCLI
    } else if model.starts_with("deepseek-") {
        // DeepSeek provider not yet implemented - will return error via ProviderRegistry
        Provider::DeepSeek
    } else if model.starts_with("ollama-") {
        // Ollama provider not yet implemented - will return error via ProviderRegistry
        Provider::Ollama
    } else {
        // Default to Vertex for unknown models
        Provider::Vertex
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_route_provider_gemini() {
        assert_eq!(route_provider("gemini-pro"), Provider::Vertex);
        assert_eq!(route_provider("gemini-2.5-flash"), Provider::Vertex);
        assert_eq!(route_provider("gemini-3.0-pro"), Provider::Vertex);
    }

    #[test]
    fn test_route_provider_claude() {
        assert_eq!(route_provider("claude-3-5-sonnet"), Provider::AnthropicCLI);
        assert_eq!(route_provider("claude-3-opus"), Provider::AnthropicCLI);
        assert_eq!(route_provider("claude-3-haiku"), Provider::AnthropicCLI);
    }

    #[test]
    fn test_route_provider_default() {
        assert_eq!(route_provider("unknown-model"), Provider::Vertex);
        assert_eq!(route_provider(""), Provider::Vertex);
    }

    #[test]
    fn test_provider_registry_routing() {
        let registry = ProviderRegistry::with_config(None);

        let vertex_provider = registry.route_by_model("gemini-pro");
        assert!(vertex_provider.is_some());
        assert_eq!(vertex_provider.unwrap().provider_type(), Provider::Vertex);

        let claude_provider = registry.route_by_model("claude-3-5-sonnet");
        assert!(claude_provider.is_some());
        assert_eq!(
            claude_provider.unwrap().provider_type(),
            Provider::AnthropicCLI
        );

        let default_provider = registry.route_by_model("unknown");
        assert!(default_provider.is_some());
        assert_eq!(default_provider.unwrap().provider_type(), Provider::Vertex);
    }

    #[test]
    fn test_provider_supports_model() {
        let registry = ProviderRegistry::with_config(None);

        let vertex = registry.route_by_model("gemini-pro").unwrap();
        assert!(vertex.supports_model("gemini-pro"));
        assert!(vertex.supports_model("gemini-2.5-flash"));
        assert!(!vertex.supports_model("claude-3-5-sonnet"));

        let claude = registry.route_by_model("claude-3-5-sonnet").unwrap();
        assert!(claude.supports_model("claude-3-5-sonnet"));
        assert!(!claude.supports_model("gemini-pro"));
    }
}
