// @critical: Health endpoint test
use super::test_utils::TestServer;
use axum::body::to_bytes;
use axum::http::StatusCode;
use serde_json::Value;

/// Reasonable body size limit for tests (1MB)
const TEST_BODY_LIMIT: usize = 1024 * 1024;

#[tokio::test]
async fn test_health_endpoint() {
    let server = TestServer::new();

    let req = server.make_request("GET", "/health", None, None);
    let response = server.call(req).await;

    let status = response.status();
    // Health endpoint returns 200 (healthy) or 503 (unhealthy) depending on service availability
    assert!(
        status == StatusCode::OK || status == StatusCode::SERVICE_UNAVAILABLE,
        "Health endpoint should return 200 or 503, got {}",
        status
    );

    let body = response.into_body();
    let body_bytes = to_bytes(body, TEST_BODY_LIMIT)
        .await
        .expect("Failed to read health response body");
    let json: Value =
        serde_json::from_slice(&body_bytes).expect("Health response is not valid JSON");

    // When healthy, status is "ok"; when unhealthy, status may differ
    if status == StatusCode::OK {
        assert_eq!(
            json["status"], "ok",
            "Health status should be 'ok' when healthy"
        );
    }
    assert!(
        json.get("version").is_some(),
        "Health response should include version"
    );
}
