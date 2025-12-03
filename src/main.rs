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
    api_version::api_version_middleware,
    auth::auth_middleware,
    rate_limit::{rate_limit_middleware, RateLimiter},
    security_headers::security_headers_middleware,
};
use vertex_bridge::openai::circuit_breaker::CircuitBreaker;
use vertex_bridge::openai::metrics::Metrics;
use vertex_bridge::services::auth::TokenManager;
use vertex_bridge::services::cache::Cache;
use vertex_bridge::services::providers::ProviderRegistry;
use vertex_bridge::state::AppState;

async fn setup_shutdown_signal() {
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
                    if sigterm.recv().await.is_none() {
                        warn!("SIGTERM signal stream closed unexpectedly");
                    }
                }
            } => {
                info!("Received SIGTERM, initiating graceful shutdown");
            }
        }
    }
    #[cfg(not(unix))]
    {
        if let Err(e) = tokio::signal::ctrl_c().await {
            error!("Failed to install Ctrl+C handler: {}", e);
            return;
        }
        info!("Received Ctrl+C, initiating graceful shutdown");
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    vertex_bridge::services::flags::FeatureFlags::init();

    let config = AppConfig::new()
        .map_err(|e| {
            anyhow::anyhow!(
                "Failed to load configuration: {}. Please check your environment variables and configuration.",
                e
            )
        })?;

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

    let token_manager = TokenManager::new(
        config.vertex.api_key.clone(),
        config.vertex.credentials_file.clone(),
    )
    .map_err(|e| {
        error!("Failed to initialize TokenManager: {}", e);
        anyhow::anyhow!("TokenManager initialization failed: {}", e)
    })?;

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

    let max_request_size = config.server.max_request_size;
    let server_host = config.server.host.clone();
    let server_port = config.server.port;

    let state = AppState {
        config: Arc::new(config),
        token_manager,
        provider_registry,
        rate_limiter: rate_limiter.clone(),
        circuit_breaker,
        metrics,
        cache,
    };

    let public_routes = Router::new().route("/health", get(health::health_check));

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
        ))
        .layer(middleware::from_fn_with_state(
            rate_limiter,
            rate_limit_middleware,
        ));

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

    let addr: SocketAddr = format!("{}:{}", server_host, server_port)
        .parse()
        .map_err(|e| {
            anyhow::anyhow!(
                "Invalid server address {}:{}: {}",
                server_host,
                server_port,
                e
            )
        })?;

    info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;

    let server = axum::serve(listener, app).with_graceful_shutdown(setup_shutdown_signal());

    if let Err(e) = server.await {
        error!("Server error: {}", e);
        return Err(anyhow::anyhow!("Server failed: {}", e));
    }

    info!("Server shutdown complete");
    Ok(())
}
