use crate::state::AppState;
use axum::{
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tracing::warn;

fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub async fn auth_middleware(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if !state.config.auth.require_auth {
        return Ok(next.run(req).await);
    }

    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if !auth_header.starts_with("Bearer ") {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // Use constant-time comparison to prevent timing attacks
    let token_hash = hash_token(token);
    let master_key_hash = hash_token(&state.config.auth.master_key);

    // Compare hashes using constant-time comparison
    // ct_eq returns Choice which can be converted to bool
    let tokens_match = token_hash.as_bytes().ct_eq(master_key_hash.as_bytes());
    if !bool::from(tokens_match) {
        warn!(
            "Invalid API Key attempt: {}...",
            &token_hash[..8.min(token_hash.len())]
        );
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
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        Router,
    };
    use std::sync::Arc;
    use tower::util::ServiceExt;

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
            token_manager: crate::services::auth::TokenManager::new(None, None)
                .expect("Failed to initialize TokenManager in test"),
            provider_registry: Arc::new(crate::services::providers::ProviderRegistry::with_config(
                None,
            )),
            rate_limiter: crate::middleware::rate_limit::RateLimiter::new(100, 10),
            circuit_breaker: Arc::new(crate::openai::circuit_breaker::CircuitBreaker::new(
                10, 60, 3,
            )),
            metrics: Arc::new(crate::openai::metrics::Metrics::new()),
            cache: Arc::new(crate::services::cache::Cache::new(false, 3600)),
        }
    }

    #[tokio::test]
    async fn test_auth_disabled() {
        let state = create_test_state(false, "");
        let app = Router::new()
            .route("/test", axum::routing::get(|| async { StatusCode::OK }))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                auth_middleware,
            ))
            .with_state(state);
        let req = Request::builder().uri("/test").body(Body::empty()).unwrap();
        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_auth_missing_header() {
        let state = create_test_state(true, "test-key");
        let app = Router::new()
            .route("/test", axum::routing::get(|| async { StatusCode::OK }))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                auth_middleware,
            ))
            .with_state(state);
        let req = Request::builder().uri("/test").body(Body::empty()).unwrap();
        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_auth_invalid_token() {
        let state = create_test_state(true, "correct-key");
        let app = Router::new()
            .route("/test", axum::routing::get(|| async { StatusCode::OK }))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                auth_middleware,
            ))
            .with_state(state);
        let req = Request::builder()
            .uri("/test")
            .header("Authorization", "Bearer wrong-key")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_auth_valid_token() {
        let state = create_test_state(true, "test-key");
        let app = Router::new()
            .route("/test", axum::routing::get(|| async { StatusCode::OK }))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                auth_middleware,
            ))
            .with_state(state);
        let req = Request::builder()
            .uri("/test")
            .header("Authorization", "Bearer test-key")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
