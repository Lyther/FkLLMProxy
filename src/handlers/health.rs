use axum::{extract::State, response::IntoResponse, Json};
use reqwest::Client;
use serde_json::json;
use std::time::Duration;
use tracing::{error, warn};

use crate::openai::harvester::HarvesterClient;
use crate::state::AppState;

const HEALTH_CHECK_TIMEOUT_SECS: u64 = 2;
const CACHE_CONTROL_NO_CACHE: &str = "no-cache, no-store, must-revalidate";

async fn check_harvester_health(
    config: &std::sync::Arc<crate::config::AppConfig>,
) -> serde_json::Value {
    match HarvesterClient::new(config) {
        Ok(harvester) => match harvester.health_check().await {
            Ok(health) => {
                json!({
                    "available": true,
                    "browser_alive": health.browser_alive,
                    "session_valid": health.session_valid,
                    "last_token_refresh": health.last_token_refresh
                })
            }
            Err(e) => {
                warn!("Harvester health check failed: {}", e);
                json!({
                    "available": false,
                    "error": e.to_string()
                })
            }
        },
        Err(e) => {
            error!("Failed to create harvester client: {}", e);
            json!({
                "available": false,
                "error": format!("Harvester client initialization failed: {}", e)
            })
        }
    }
}

async fn check_anthropic_bridge_health(bridge_url: &str) -> serde_json::Value {
    let client = match Client::builder()
        .timeout(Duration::from_secs(HEALTH_CHECK_TIMEOUT_SECS))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            error!(
                "Failed to create HTTP client for bridge health check: {}",
                e
            );
            return json!({
                "available": false,
                "error": format!("Client initialization failed: {}", e)
            });
        }
    };

    let health_url = format!("{}/health", bridge_url);

    match client.get(&health_url).send().await {
        Ok(resp) if resp.status().is_success() => {
            json!({
                "available": true,
                "url": bridge_url
            })
        }
        Ok(resp) => {
            warn!(
                "Anthropic bridge returned non-200 status: {}",
                resp.status()
            );
            json!({
                "available": false,
                "error": format!("HTTP {}", resp.status())
            })
        }
        Err(e) => {
            warn!("Anthropic bridge health check failed: {}", e);
            json!({
                "available": false,
                "error": e.to_string()
            })
        }
    }
}

pub async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    let harvester_status = check_harvester_health(&state.config).await;
    let anthropic_bridge_status =
        check_anthropic_bridge_health(&state.config.anthropic.bridge_url).await;

    (
        [(
            axum::http::header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static(CACHE_CONTROL_NO_CACHE),
        )],
        Json(json!({
            "status": "ok",
            "version": env!("CARGO_PKG_VERSION"),
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "harvester": harvester_status,
            "anthropic_bridge": anthropic_bridge_status
        })),
    )
}
