use crate::config::AppConfig;
use crate::middleware::rate_limit::RateLimiter;
use crate::openai::circuit_breaker::CircuitBreaker;
use crate::openai::metrics::Metrics;
use crate::services::auth::TokenManager;
use crate::services::cache::Cache;
use crate::services::providers::ProviderRegistry;
use std::sync::Arc;

/// Application state shared across all request handlers.
///
/// This struct holds all the shared resources needed by handlers:
/// - Configuration (read-only)
/// - Token manager for Google Cloud authentication
/// - Provider registry for routing requests to different LLM providers
/// - Rate limiter for request throttling
/// - Circuit breaker for backend resilience
/// - Metrics collector for observability
/// - Response cache for performance optimization
///
/// All fields are wrapped in `Arc` for efficient sharing across async tasks,
/// except `token_manager` and `rate_limiter` which are `Clone` themselves.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub token_manager: TokenManager,
    pub provider_registry: Arc<ProviderRegistry>,
    pub rate_limiter: RateLimiter,
    pub circuit_breaker: Arc<CircuitBreaker>,
    pub metrics: Arc<Metrics>,
    pub cache: Arc<Cache>,
}
