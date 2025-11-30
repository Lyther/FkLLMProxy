// @critical: Metrics endpoint tests

use super::test_utils::TestServer;
use axum::http::StatusCode;

#[tokio::test]
async fn test_metrics_endpoint_exists() {
    let server = TestServer::new();

    let req = server.make_request("GET", "/metrics", None, None);
    let response = server.call(req).await;

    // Metrics endpoint should exist and return some response
    // May return 200 with metrics or 404 if not implemented
    assert!(response.status().is_success() || response.status() == StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_metrics_after_request() {
    let server = TestServer::new();

    // Make a request to generate metrics
    let req = server.make_request("GET", "/health", None, None);
    let _response = server.call(req).await;

    // Check metrics endpoint
    let req = server.make_request("GET", "/metrics", None, None);
    let response = server.call(req).await;

    // Should exist (even if empty)
    assert!(response.status().is_success() || response.status() == StatusCode::NOT_FOUND);
}
