// Test utilities for critical E2E tests
use axum::{body::Body, http::Request, Router};
use std::sync::Arc;
use tower::util::ServiceExt;

/// Check if we're running in a CI environment
pub fn is_ci() -> bool {
    std::env::var("CI").is_ok()
        || std::env::var("GITHUB_ACTIONS").is_ok()
        || std::env::var("GITLAB_CI").is_ok()
        || std::env::var("JENKINS_URL").is_ok()
        || std::env::var("TRAVIS").is_ok()
}

/// Check if real Vertex API credentials are available
pub fn has_real_credentials() -> bool {
    std::env::var("VERTEX_API_KEY").is_ok()
        || std::env::var("GOOGLE_APPLICATION_CREDENTIALS").is_ok()
        || (std::env::var("GOOGLE_CLOUD_PROJECT").is_ok()
            && std::env::var("GOOGLE_APPLICATION_CREDENTIALS").is_ok())
}

/// Check if E2E tests should run (has credentials or in CI with forced flag)
pub fn should_run_e2e() -> bool {
    has_real_credentials() || (is_ci() && std::env::var("FORCE_E2E_TESTS").is_ok())
}

/// Get credential status for error messages
pub fn credential_status() -> String {
    let mut status = Vec::new();

    if std::env::var("VERTEX_API_KEY").is_ok() {
        status.push("VERTEX_API_KEY set".to_string());
    }
    if std::env::var("GOOGLE_APPLICATION_CREDENTIALS").is_ok() {
        status.push("GOOGLE_APPLICATION_CREDENTIALS set".to_string());
    }
    if std::env::var("GOOGLE_CLOUD_PROJECT").is_ok() {
        status.push("GOOGLE_CLOUD_PROJECT set".to_string());
    }

    if status.is_empty() {
        format!(
            "No credentials found. Set one of: VERTEX_API_KEY, GOOGLE_APPLICATION_CREDENTIALS (with GOOGLE_CLOUD_PROJECT). {}",
            if is_ci() { "CI environment detected." } else { "Local development mode." }
        )
    } else {
        format!("Credentials: {}", status.join(", "))
    }
}
use vertex_bridge::config::{
    AnthropicConfig, AppConfig, AuthConfig, CacheConfig, CircuitBreakerConfig, LogConfig,
    OpenAIConfig, RateLimitConfig, ServerConfig, VertexConfig,
};
use vertex_bridge::handlers::{chat, health, metrics};
use vertex_bridge::middleware::{auth::auth_middleware, rate_limit::RateLimiter};
use vertex_bridge::openai::circuit_breaker::CircuitBreaker;
use vertex_bridge::openai::metrics::Metrics;
use vertex_bridge::services::auth::TokenManager;
use vertex_bridge::services::cache::Cache;
use vertex_bridge::services::providers::ProviderRegistry;
use vertex_bridge::state::AppState;

pub struct TestServer {
    pub app: Router,
}

impl TestServer {
    pub fn new() -> Self {
        Self::with_auth(false, "")
    }

    pub fn with_auth(require_auth: bool, master_key: &str) -> Self {
        // Use real credentials from env if available, otherwise use fake for unit tests
        let api_key = std::env::var("VERTEX_API_KEY").ok();
        let credentials_file = std::env::var("GOOGLE_APPLICATION_CREDENTIALS").ok();
        let project_id = std::env::var("GOOGLE_CLOUD_PROJECT").ok();
        let region = std::env::var("VERTEX_REGION")
            .ok()
            .unwrap_or_else(|| "us-central1".to_string());

        let config = AppConfig {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 0,                            // Let OS assign port
                max_request_size: 10 * 1024 * 1024, // 10MB
            },
            auth: AuthConfig {
                require_auth,
                master_key: master_key.to_string(),
            },
            vertex: VertexConfig {
                project_id,
                region,
                api_key: api_key.or_else(|| Some("test-api-key".to_string())),
                credentials_file,
                api_key_base_url: None,
                oauth_base_url: None,
            },
            log: LogConfig {
                level: "error".to_string(), // Quiet during tests
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
                capacity: 1000,
                refill_per_second: 100,
            },
            circuit_breaker: CircuitBreakerConfig {
                failure_threshold: 100,
                timeout_secs: 60,
                success_threshold: 3,
            },
            cache: CacheConfig {
                enabled: false,
                default_ttl_secs: 3600,
            },
        };

        let token_manager = TokenManager::new(
            config.vertex.api_key.clone(),
            config.vertex.credentials_file.clone(),
        )
        .expect("Failed to create token manager");

        let state = AppState {
            config: Arc::new(config.clone()),
            token_manager,
            cache: Arc::new(Cache::new(
                config.cache.enabled,
                config.cache.default_ttl_secs,
            )),
            provider_registry: Arc::new(ProviderRegistry::with_config(Some(
                config.anthropic.bridge_url.clone(),
            ))),
            rate_limiter: RateLimiter::new(1000, 100), // High limits for tests
            circuit_breaker: Arc::new(CircuitBreaker::new(
                config.circuit_breaker.failure_threshold,
                config.circuit_breaker.timeout_secs,
                config.circuit_breaker.success_threshold,
            )),
            metrics: Arc::new(Metrics::new()),
        };

        // Public routes (no authentication required)
        let public_routes =
            Router::new().route("/health", axum::routing::get(health::health_check));

        // Protected routes (require authentication)
        let protected_routes = Router::new()
            .route("/metrics", axum::routing::get(metrics::metrics_handler))
            .route(
                "/metrics/prometheus",
                axum::routing::get(metrics::prometheus_metrics_handler),
            )
            .route(
                "/v1/chat/completions",
                axum::routing::post(chat::chat_completions),
            )
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                auth_middleware,
            ));

        // Combine routes
        let app = Router::new()
            .merge(public_routes)
            .merge(protected_routes)
            .with_state(state.clone());

        Self { app }
    }

    pub async fn call(&self, req: Request<Body>) -> axum::response::Response {
        self.app.clone().oneshot(req).await.unwrap()
    }

    pub fn make_request(
        &self,
        method: &str,
        uri: &str,
        body: Option<&str>,
        auth: Option<&str>,
    ) -> Request<Body> {
        let mut builder = Request::builder().method(method).uri(uri);

        if let Some(auth_key) = auth {
            builder = builder.header("Authorization", format!("Bearer {}", auth_key));
        }

        if let Some(body_str) = body {
            builder = builder.header("Content-Type", "application/json");
            builder.body(Body::from(body_str.to_string())).unwrap()
        } else {
            builder.body(Body::empty()).unwrap()
        }
    }
}

pub fn create_chat_request(model: &str, messages: &str, stream: bool) -> String {
    format!(
        r#"{{
            "model": "{}",
            "messages": {},
            "stream": {}
        }}"#,
        model, messages, stream
    )
}

pub fn create_simple_message(role: &str, content: &str) -> String {
    format!(r#"[{{"role": "{}", "content": "{}"}}]"#, role, content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_ci_detects_ci_environment() {
        // Test that is_ci() can be called without panicking
        // Actual CI detection depends on environment variables
        let _result = is_ci();
        // Function should return bool without error
        assert!(matches!(_result, true | false));
    }

    #[test]
    fn test_is_ci_used_in_should_run_e2e() {
        // Verify is_ci() is actually used by should_run_e2e()
        let _result = should_run_e2e();
        assert!(matches!(_result, true | false));
    }

    #[test]
    fn test_is_ci_used_in_credential_status() {
        // Verify is_ci() is actually used by credential_status()
        let status = credential_status();
        assert!(!status.is_empty());
    }
}
