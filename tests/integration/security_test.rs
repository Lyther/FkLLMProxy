// @critical: Security boundary tests - access control verification

use super::test_utils::TestServer;
use axum::http::StatusCode;

#[tokio::test]
async fn test_health_endpoint_public_when_auth_enabled() {
    // Security: Health endpoint must remain public for load balancer checks
    let server = TestServer::with_auth(true, "secret-key");

    let req = server.make_request("GET", "/health", None, None);
    let response = server.call(req).await;

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Health endpoint must be accessible without authentication"
    );
}

#[tokio::test]
async fn test_metrics_endpoint_protected_when_auth_enabled() {
    // Security: Metrics endpoint must require authentication
    let server = TestServer::with_auth(true, "secret-key");

    let req = server.make_request("GET", "/metrics", None, None);
    let response = server.call(req).await;

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Metrics endpoint must require authentication"
    );
}

#[tokio::test]
async fn test_chat_endpoint_protected_when_auth_enabled() {
    // Security: Chat endpoint must require authentication
    let server = TestServer::with_auth(true, "secret-key");

    let request_body =
        r#"{"model": "gemini-2.5-flash", "messages": [{"role": "user", "content": "test"}]}"#;
    let req = server.make_request("POST", "/v1/chat/completions", Some(request_body), None);
    let response = server.call(req).await;

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Chat endpoint must require authentication"
    );
}

#[tokio::test]
async fn test_auth_disabled_allows_all_endpoints() {
    // Security: When auth is disabled, all endpoints should be accessible
    let server = TestServer::with_auth(false, "");

    // Health should work
    let req = server.make_request("GET", "/health", None, None);
    let response = server.call(req).await;
    assert_eq!(response.status(), StatusCode::OK);

    // Metrics should work
    let req = server.make_request("GET", "/metrics", None, None);
    let response = server.call(req).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_invalid_bearer_token_format_rejected() {
    // Security: Malformed Bearer tokens must be rejected
    let server = TestServer::with_auth(true, "correct-key");

    let mut req = server.make_request("GET", "/metrics", None, None);
    req.headers_mut()
        .insert("Authorization", "NotBearer correct-key".parse().unwrap());

    let response = server.call(req).await;
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Invalid Bearer token format must be rejected"
    );
}

#[tokio::test]
async fn test_empty_bearer_token_rejected() {
    // Security: Empty Bearer tokens must be rejected
    let server = TestServer::with_auth(true, "correct-key");

    let mut req = server.make_request("GET", "/metrics", None, None);
    req.headers_mut()
        .insert("Authorization", "Bearer ".parse().unwrap());

    let response = server.call(req).await;
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Empty Bearer token must be rejected"
    );
}
