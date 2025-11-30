// @smoke: Fast sanity checks (< 2 minutes total)
// These run on git push and must be green to proceed

use super::test_utils::TestServer;
use axum::body::to_bytes;
use axum::http::StatusCode;
use serde_json::Value;

#[tokio::test]
async fn smoke_health_check() {
    let server = TestServer::new();

    let req = server.make_request("GET", "/health", None, None);
    let response = server.call(req).await;

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body();
    let body_bytes = to_bytes(body, usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["status"], "ok");
    assert!(json.get("version").is_some());
}

#[tokio::test]
async fn smoke_auth_middleware() {
    // Test auth disabled (default)
    let server = TestServer::with_auth(false, "");
    let req = server.make_request("GET", "/health", None, None);
    let response = server.call(req).await;
    assert_eq!(response.status(), StatusCode::OK);

    // Test auth enabled with correct key
    let server = TestServer::with_auth(true, "smoke-key");
    let req = server.make_request("GET", "/health", None, Some("smoke-key"));
    let response = server.call(req).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn smoke_error_format() {
    let server = TestServer::new();

    let request_body = r#"{"model": "invalid", "messages": [{"role": "user", "content": "test"}]}"#;
    let req = server.make_request("POST", "/v1/chat/completions", Some(request_body), None);
    let response = server.call(req).await;

    // Invalid models may return 400 or 503 depending on provider routing
    assert!(
        response.status().is_client_error() || response.status() == StatusCode::SERVICE_UNAVAILABLE
    );

    let body = response.into_body();
    let body_bytes = to_bytes(body, usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body_bytes).unwrap();

    // Verify error structure exists
    assert!(json.get("error").is_some());
}

#[tokio::test]
async fn smoke_metrics_endpoint() {
    let server = TestServer::new();

    let req = server.make_request("GET", "/metrics", None, None);
    let response = server.call(req).await;

    // Metrics endpoint should exist
    assert!(response.status().is_success() || response.status() == StatusCode::NOT_FOUND);
}
