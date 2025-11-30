use axum::{extract::State, response::IntoResponse, Json};
use reqwest::Client;
use serde_json::json;
use std::time::Duration;
use tracing::{error, warn};

use crate::openai::harvester::HarvesterClient;
use crate::state::AppState;

pub async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    let harvester_status = match HarvesterClient::new(&state.config) {
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
    };

    // Check Anthropic bridge connectivity
    let anthropic_bridge_status = {
        let client = Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap_or_else(|_| Client::new());
        let bridge_url = format!("{}/health", state.config.anthropic.bridge_url);

        match client.get(&bridge_url).send().await {
            Ok(resp) if resp.status().is_success() => {
                json!({
                    "available": true,
                    "url": state.config.anthropic.bridge_url
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
    };

    (
        [(
            axum::http::header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static("no-cache, no-store, must-revalidate"),
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
