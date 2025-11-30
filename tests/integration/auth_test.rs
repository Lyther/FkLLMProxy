// @critical: Auth middleware tests - security boundary

use super::test_utils::TestServer;
use axum::http::StatusCode;

#[tokio::test]
async fn test_auth_disabled_allows_access() {
    let server = TestServer::with_auth(false, "");

    let req = server.make_request("GET", "/health", None, None);
    let response = server.call(req).await;

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_health_endpoint_always_public() {
    // Health endpoint should be accessible even when auth is enabled
    let server = TestServer::with_auth(true, "test-master-key-123");

    let req = server.make_request("GET", "/health", None, None);
    let response = server.call(req).await;

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_metrics_endpoint_requires_auth() {
    // Metrics endpoint should require authentication when auth is enabled
    let server = TestServer::with_auth(true, "test-key-123");

    let req = server.make_request("GET", "/metrics", None, None);
    let response = server.call(req).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_metrics_endpoint_with_wrong_key_rejected() {
    let server = TestServer::with_auth(true, "correct-key-456");

    let req = server.make_request("GET", "/metrics", None, Some("wrong-key-789"));
    let response = server.call(req).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_metrics_endpoint_with_correct_key_allowed() {
    let server = TestServer::with_auth(true, "correct-key-456");

    let req = server.make_request("GET", "/metrics", None, Some("correct-key-456"));
    let response = server.call(req).await;

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_prometheus_metrics_endpoint_requires_auth() {
    let server = TestServer::with_auth(true, "test-key-123");

    let req = server.make_request("GET", "/metrics/prometheus", None, None);
    let response = server.call(req).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_prometheus_metrics_endpoint_with_key_allowed() {
    let server = TestServer::with_auth(true, "test-key-123");

    let req = server.make_request("GET", "/metrics/prometheus", None, Some("test-key-123"));
    let response = server.call(req).await;

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_auth_enabled_chat_endpoint_protected() {
    let server = TestServer::with_auth(true, "test-key");

    let request_body =
        r#"{"model": "gemini-2.5-flash", "messages": [{"role": "user", "content": "test"}]}"#;
    let req = server.make_request("POST", "/v1/chat/completions", Some(request_body), None);
    let response = server.call(req).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_auth_enabled_chat_endpoint_with_key_allowed() {
    let server = TestServer::with_auth(true, "test-key");

    let request_body =
        r#"{"model": "gemini-2.5-flash", "messages": [{"role": "user", "content": "test"}]}"#;
    let req = server.make_request(
        "POST",
        "/v1/chat/completions",
        Some(request_body),
        Some("test-key"),
    );
    let response = server.call(req).await;

    // May fail due to missing provider, but should pass auth
    assert_ne!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_auth_malformed_bearer_token() {
    // Test malformed Bearer token on protected endpoint
    let server = TestServer::with_auth(true, "test-key");

    let mut req = server.make_request("GET", "/metrics", None, None);
    // Use a header that doesn't start with "Bearer "
    req.headers_mut()
        .insert("Authorization", "Token test-key".parse().unwrap());

    let response = server.call(req).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
