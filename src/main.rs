use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{info, warn};
use vertex_bridge::config::AppConfig;
use vertex_bridge::handlers::{chat, health};
use vertex_bridge::middleware::auth::auth_middleware;
use vertex_bridge::services::auth::TokenManager;
use vertex_bridge::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 0. Load Env (Explicitly here too, though Config does it)
    dotenvy::dotenv().ok();

    // 1. Load Config
    let config = AppConfig::new().expect("Failed to load configuration");

    // 2. Setup Logging
    tracing_subscriber::fmt()
        .with_env_filter(format!("{},tower_http=debug", config.log.level))
        .init();

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

    let state = AppState {
        config: Arc::new(config.clone()),
        token_manager,
    };

    // 4. Setup Router
    let app = Router::new()
        .route("/health", get(health::health_check))
        .route("/v1/chat/completions", post(chat::chat_completions))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);

    // 5. Start Server
    let addr: SocketAddr = format!("{}:{}", config.server.host, config.server.port)
        .parse()
        .expect("Invalid address");

    info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
