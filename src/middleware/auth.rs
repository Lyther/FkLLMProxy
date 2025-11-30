use crate::state::AppState;
use axum::{
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use tracing::warn;

pub async fn auth_middleware(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // 1. Check if auth is enabled
    if !state.config.auth.require_auth {
        return Ok(next.run(req).await);
    }

    // 2. Extract Authorization header
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // 3. Validate Bearer token
    if !auth_header.starts_with("Bearer ") {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // 4. Check against master key
    if token != state.config.auth.master_key {
        warn!("Invalid API Key attempt: {}", token);
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(next.run(req).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        AnthropicConfig, AppConfig, AuthConfig, CacheConfig, CircuitBreakerConfig, LogConfig,
        OpenAIConfig, RateLimitConfig, ServerConfig, VertexConfig,
    };
    use std::sync::Arc;

    fn create_test_state(require_auth: bool, master_key: &str) -> AppState {
        let config = AppConfig {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 4000,
                max_request_size: 10 * 1024 * 1024,
            },
            auth: AuthConfig {
                require_auth,
                master_key: master_key.to_string(),
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
            config: Arc::new(config.clone()),
            token_manager: crate::services::auth::TokenManager::new(None, None).unwrap(),
            provider_registry: Arc::new(crate::services::providers::ProviderRegistry::with_config(
                Some(config.anthropic.bridge_url.clone()),
            )),
            rate_limiter: crate::middleware::rate_limit::RateLimiter::new(
                config.rate_limit.capacity,
                config.rate_limit.refill_per_second,
            ),
            circuit_breaker: Arc::new(crate::openai::circuit_breaker::CircuitBreaker::new(
                config.circuit_breaker.failure_threshold,
                config.circuit_breaker.timeout_secs,
                config.circuit_breaker.success_threshold,
            )),
            metrics: Arc::new(crate::openai::metrics::Metrics::new()),
            cache: Arc::new(crate::services::cache::Cache::new(false, 3600)),
        }
    }

    #[test]
    fn test_auth_config_disabled() {
        let config = create_test_state(false, "").config;
        assert!(!config.auth.require_auth);
    }

    #[test]
    fn test_auth_config_enabled() {
        let config = create_test_state(true, "test-key").config;
        assert!(config.auth.require_auth);
        assert_eq!(config.auth.master_key, "test-key");
    }

    #[test]
    fn test_auth_key_validation() {
        let config = create_test_state(true, "correct-key").config;
        assert_ne!(config.auth.master_key, "wrong-key");
        assert_eq!(config.auth.master_key, "correct-key");
    }
}
