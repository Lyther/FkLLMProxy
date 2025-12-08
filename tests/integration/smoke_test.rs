// @smoke: Fast sanity checks (< 2 minutes total)
// These run on git push and must be green to proceed

use super::test_utils::TestServer;
use axum::body::to_bytes;
use axum::http::StatusCode;
use serde_json::Value;

/// Reasonable body size limit for tests (1MB)
const TEST_BODY_LIMIT: usize = 1024 * 1024;

#[tokio::test]
async fn smoke_health_check() {
    let server = TestServer::new();

    let req = TestServer::make_request("GET", "/health", None, None);
    let response = server.call(req).await;

    let status = response.status();
    // Health endpoint returns 200 (healthy) or 503 (unhealthy) depending on service availability
    assert!(
        status == StatusCode::OK || status == StatusCode::SERVICE_UNAVAILABLE,
        "Health endpoint should return 200 or 503, got {status}"
    );

    let body = response.into_body();
    let body_bytes = to_bytes(body, TEST_BODY_LIMIT)
        .await
        .expect("Failed to read health check response body");
    let json: Value =
        serde_json::from_slice(&body_bytes).expect("Health check response is not valid JSON");

    // When healthy, status is "ok"
    if status == StatusCode::OK {
        assert_eq!(json["status"], "ok");
    }
    assert!(json.get("version").is_some());
}

#[tokio::test]
async fn smoke_auth_middleware() {
    // Test auth disabled (default) - should not require auth
    let server = TestServer::with_auth(false, "");
    let req = TestServer::make_request("GET", "/health", None, None);
    let response = server.call(req).await;
    let status = response.status();
    // Should not get 401 (auth not required), may get 503 (service unavailable)
    assert!(
        status == StatusCode::OK || status == StatusCode::SERVICE_UNAVAILABLE,
        "Auth disabled: expected 200 or 503, got {status}"
    );

    // Test auth enabled with correct key - should pass auth check
    let server = TestServer::with_auth(true, "smoke-key");
    let req = TestServer::make_request("GET", "/health", None, Some("smoke-key"));
    let response = server.call(req).await;
    let status = response.status();
    // Health endpoint is public, but with key should still work
    // Should not get 401, may get 503 (service unavailable)
    assert!(
        status == StatusCode::OK || status == StatusCode::SERVICE_UNAVAILABLE,
        "Auth enabled with key: expected 200 or 503, got {status}"
    );
}

#[tokio::test]
async fn smoke_error_format() {
    let server = TestServer::new();

    let request_body = r#"{"model": "invalid", "messages": [{"role": "user", "content": "test"}]}"#;
    let req = TestServer::make_request("POST", "/v1/chat/completions", Some(request_body), None);
    let response = server.call(req).await;

    let status = response.status();
    // Invalid models return 400 (bad request) or 503 (no provider available)
    assert!(
        status == StatusCode::BAD_REQUEST
            || status == StatusCode::SERVICE_UNAVAILABLE
            || status == StatusCode::NOT_FOUND,
        "Expected 400, 404, or 503 for invalid model, got {status}"
    );

    let body = response.into_body();
    let body_bytes = to_bytes(body, TEST_BODY_LIMIT)
        .await
        .expect("Failed to read error response body");
    let json: Value =
        serde_json::from_slice(&body_bytes).expect("Error response is not valid JSON");

    // Verify error structure follows OpenAI format
    assert!(
        json.get("error").is_some(),
        "Error response missing 'error' field"
    );
}

#[tokio::test]
async fn smoke_metrics_endpoint() {
    let server = TestServer::new();

    let req = TestServer::make_request("GET", "/metrics", None, None);
    let response = server.call(req).await;

    // Metrics endpoint must exist and return 200
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Metrics endpoint should return 200 OK"
    );

    // Verify response contains metrics data (JSON format)
    let body = response.into_body();
    let body_bytes = to_bytes(body, TEST_BODY_LIMIT)
        .await
        .expect("Failed to read metrics response body");

    // Metrics endpoint returns JSON
    let json: Value =
        serde_json::from_slice(&body_bytes).expect("Metrics response is not valid JSON");
    assert!(
        json.get("total_requests").is_some()
            || json.get("requests").is_some()
            || json.get("uptime_seconds").is_some(),
        "Metrics response should contain metric fields"
    );
}
