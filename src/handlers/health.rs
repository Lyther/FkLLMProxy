use axum::{extract::State, response::IntoResponse, Json};
use reqwest::Client;
use serde_json::json;
use std::time::Duration;
use tracing::{error, warn};

use crate::openai::harvester::HarvesterClient;
use crate::state::AppState;

const HEALTH_CHECK_TIMEOUT_SECS: u64 = 2;
const CACHE_CONTROL_NO_CACHE: &str = "no-cache, no-store, must-revalidate";
const BRIDGE_HEALTH_PATH: &str = "/health";

async fn check_harvester_health(
    config: &std::sync::Arc<crate::config::AppConfig>,
) -> serde_json::Value {
    match HarvesterClient::new(config) {
        Ok(harvester) => match tokio::time::timeout(
            Duration::from_secs(HEALTH_CHECK_TIMEOUT_SECS),
            harvester.health_check(),
        )
        .await
        {
            Ok(Ok(health)) => {
                json!({
                    "available": true,
                    "browser_alive": health.browser_alive,
                    "session_valid": health.session_valid,
                    "last_token_refresh": health.last_token_refresh
                })
            }
            Ok(Err(e)) => {
                warn!("Harvester health check failed: {}", e);
                json!({
                    "available": false,
                    "error": e.to_string()
                })
            }
            Err(_) => {
                warn!(
                    "Harvester health check timed out after {} seconds",
                    HEALTH_CHECK_TIMEOUT_SECS
                );
                json!({
                    "available": false,
                    "error": format!("Health check timed out after {} seconds", HEALTH_CHECK_TIMEOUT_SECS)
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

    let health_url = bridge_url.to_string() + BRIDGE_HEALTH_PATH;

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

    let harvester_available = harvester_status
        .get("available")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let bridge_available = anthropic_bridge_status
        .get("available")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    let overall_status = if harvester_available && bridge_available {
        "ok"
    } else if !harvester_available && !bridge_available {
        "unhealthy"
    } else {
        "degraded"
    };

    let status_code = if overall_status == "ok" {
        axum::http::StatusCode::OK
    } else {
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status_code,
        [(
            axum::http::header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static(CACHE_CONTROL_NO_CACHE),
        )],
        Json(json!({
            "status": overall_status,
            "version": env!("CARGO_PKG_VERSION"),
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "harvester": harvester_status,
            "anthropic_bridge": anthropic_bridge_status
        })),
    )
}
