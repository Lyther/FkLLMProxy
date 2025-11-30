use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{error, info, warn};
use vertex_bridge::config::AppConfig;
use vertex_bridge::handlers::{chat, health, metrics};
use vertex_bridge::middleware::{
    api_version::api_version_middleware, auth::auth_middleware, rate_limit::RateLimiter,
    security_headers::security_headers_middleware,
};
use vertex_bridge::openai::circuit_breaker::CircuitBreaker;
use vertex_bridge::openai::metrics::Metrics;
use vertex_bridge::services::auth::TokenManager;
use vertex_bridge::services::cache::Cache;
use vertex_bridge::services::providers::ProviderRegistry;
use vertex_bridge::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 0. Load Env (Explicitly here too, though Config does it)
    dotenvy::dotenv().ok();

    // Initialize Feature Flags from Env
    vertex_bridge::services::flags::FeatureFlags::init();

    // 1. Load Config
    let config = match AppConfig::new() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("FATAL: Failed to load configuration: {}", e);
            eprintln!("Please check your environment variables and configuration.");
            std::process::exit(1);
        }
    };

    // 2. Setup Logging with structured fields
    let log_format = config.log.format.as_str();
    let filter =
        tracing_subscriber::EnvFilter::try_new(format!("{},tower_http=debug", config.log.level))
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&config.log.level));

    match log_format {
        "json" => {
            tracing_subscriber::fmt()
                .json()
                .with_target(false)
                .with_file(true)
                .with_line_number(true)
                .with_current_span(true)
                .with_span_list(true)
                .with_env_filter(filter)
                .init();
        }
        _ => {
            tracing_subscriber::fmt()
                .with_target(true)
                .with_file(true)
                .with_line_number(true)
                .with_env_filter(filter)
                .init();
        }
    }

    info!("Starting Vertex Bridge v{}", env!("CARGO_PKG_VERSION"));
    info!(
        "Config loaded: Host={}, Port={}",
        config.server.host, config.server.port
    );

    // 3. Initialize Services
    // Pass the API Key from config if available
    let token_manager = match TokenManager::new(
        config.vertex.api_key.clone(),
        config.vertex.credentials_file.clone(),
    ) {
        Ok(tm) => tm,
        Err(e) => {
            warn!("Failed to initialize TokenManager: {}", e);
            warn!("Server will start, but Vertex calls will fail until credentials are fixed.");
            return Err(e);
        }
    };

    let rate_limiter = RateLimiter::new(
        config.rate_limit.capacity,
        config.rate_limit.refill_per_second,
    );
    let circuit_breaker = Arc::new(CircuitBreaker::new(
        config.circuit_breaker.failure_threshold,
        config.circuit_breaker.timeout_secs,
        config.circuit_breaker.success_threshold,
    ));
    let metrics = Arc::new(Metrics::new());
    let provider_registry = Arc::new(ProviderRegistry::with_config(Some(
        config.anthropic.bridge_url.clone(),
    )));
    let cache = Arc::new(Cache::new(
        config.cache.enabled,
        config.cache.default_ttl_secs,
    ));

    let state = AppState {
        config: Arc::new(config.clone()),
        token_manager,
        provider_registry,
        rate_limiter,
        circuit_breaker,
        metrics,
        cache,
    };

    // 4. Setup Router
    let max_request_size = config.server.max_request_size;

    // Public routes (no authentication required)
    let public_routes = Router::new().route("/health", get(health::health_check));

    // Protected routes (require authentication)
    let protected_routes = Router::new()
        .route("/metrics", get(metrics::metrics_handler))
        .route(
            "/metrics/prometheus",
            get(metrics::prometheus_metrics_handler),
        )
        .route("/v1/chat/completions", post(chat::chat_completions))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    // Combine routes with shared middleware
    let app = Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .layer(tower_http::limit::RequestBodyLimitLayer::new(
            max_request_size,
        ))
        .layer(tower_http::compression::CompressionLayer::new())
        .layer(middleware::from_fn(security_headers_middleware))
        .layer(middleware::from_fn(api_version_middleware))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);

    // 5. Start Server
    let addr: SocketAddr = format!("{}:{}", config.server.host, config.server.port)
        .parse()
        .map_err(|e| {
            anyhow::anyhow!(
                "Invalid server address {}:{}: {}",
                config.server.host,
                config.server.port,
                e
            )
        })?;

    info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Graceful shutdown signal handling
    let shutdown_signal = async {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};

            let mut sigterm = match signal(SignalKind::terminate()) {
                Ok(s) => Some(s),
                Err(e) => {
                    warn!("Failed to register SIGTERM handler: {}", e);
                    None
                }
            };

            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    info!("Received Ctrl+C, initiating graceful shutdown");
                }
                _ = async {
                    if let Some(ref mut sigterm) = sigterm {
                        let _ = sigterm.recv().await;
                    }
                } => {
                    info!("Received SIGTERM, initiating graceful shutdown");
                }
            }
        }
        #[cfg(not(unix))]
        {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to install Ctrl+C handler");
            info!("Received Ctrl+C, initiating graceful shutdown");
        }
    };

    let server = axum::serve(listener, app).with_graceful_shutdown(shutdown_signal);

    if let Err(e) = server.await {
        error!("Server error: {}", e);
        return Err(anyhow::anyhow!("Server failed: {}", e));
    }

    info!("Server shutdown complete");
    Ok(())
}
