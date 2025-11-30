use crate::config::AppConfig;
use crate::middleware::rate_limit::RateLimiter;
use crate::openai::circuit_breaker::CircuitBreaker;
use crate::openai::metrics::Metrics;
use crate::services::auth::TokenManager;
use crate::services::cache::Cache;
use crate::services::providers::ProviderRegistry;
use std::sync::Arc;

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
