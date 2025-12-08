// @critical: Auth middleware tests - security boundary

use super::test_utils::TestServer;
use axum::http::StatusCode;

#[tokio::test]
async fn test_auth_disabled_allows_access() {
    let server = TestServer::with_auth(false, "");

    let req = TestServer::make_request("GET", "/health", None, None);
    let response = server.call(req).await;

    // Health endpoint should not return 401/403 when auth is disabled
    // May return 503 if underlying services are unavailable in test env
    assert!(
        !response.status().is_client_error()
            || response.status() == StatusCode::SERVICE_UNAVAILABLE,
        "Expected non-auth-error status, got {}",
        response.status()
    );
}

#[tokio::test]
async fn test_health_endpoint_always_public() {
    // Health endpoint should be accessible even when auth is enabled (no 401)
    let server = TestServer::with_auth(true, "test-master-key-123");

    let req = TestServer::make_request("GET", "/health", None, None);
    let response = server.call(req).await;

    // Health endpoint is public - should not require authentication
    // May return 200 (healthy) or 503 (unhealthy) but never 401/403
    let status = response.status();
    assert!(
        status == StatusCode::OK || status == StatusCode::SERVICE_UNAVAILABLE,
        "Health endpoint should be public (not require auth), got {status}"
    );
}

#[tokio::test]
async fn test_metrics_endpoint_requires_auth() {
    // Metrics endpoint should require authentication when auth is enabled
    let server = TestServer::with_auth(true, "test-key-123");

    let req = TestServer::make_request("GET", "/metrics", None, None);
    let response = server.call(req).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_metrics_endpoint_with_wrong_key_rejected() {
    let server = TestServer::with_auth(true, "correct-key-456");

    let req = TestServer::make_request("GET", "/metrics", None, Some("wrong-key-789"));
    let response = server.call(req).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_metrics_endpoint_with_correct_key_allowed() {
    let server = TestServer::with_auth(true, "correct-key-456");

    let req = TestServer::make_request("GET", "/metrics", None, Some("correct-key-456"));
    let response = server.call(req).await;

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_prometheus_metrics_endpoint_requires_auth() {
    let server = TestServer::with_auth(true, "test-key-123");

    let req = TestServer::make_request("GET", "/metrics/prometheus", None, None);
    let response = server.call(req).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_prometheus_metrics_endpoint_with_key_allowed() {
    let server = TestServer::with_auth(true, "test-key-123");

    let req = TestServer::make_request("GET", "/metrics/prometheus", None, Some("test-key-123"));
    let response = server.call(req).await;

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_auth_enabled_chat_endpoint_protected() {
    let server = TestServer::with_auth(true, "test-key");

    let request_body =
        r#"{"model": "gemini-2.5-flash", "messages": [{"role": "user", "content": "test"}]}"#;
    let req = TestServer::make_request("POST", "/v1/chat/completions", Some(request_body), None);
    let response = server.call(req).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_auth_enabled_chat_endpoint_with_key_allowed() {
    let server = TestServer::with_auth(true, "test-key");

    let request_body =
        r#"{"model": "gemini-2.5-flash", "messages": [{"role": "user", "content": "test"}]}"#;
    let req = TestServer::make_request(
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

    let mut req = TestServer::make_request("GET", "/metrics", None, None);
    // Use a header that doesn't start with "Bearer "
    req.headers_mut()
        .insert("Authorization", "Token test-key".parse().unwrap());

    let response = server.call(req).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
