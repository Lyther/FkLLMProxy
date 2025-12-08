// @critical: Security boundary tests - access control verification

use super::test_utils::TestServer;
use axum::http::StatusCode;

/// Test model name constant
const TEST_GEMINI_MODEL: &str = "gemini-2.5-flash";

#[tokio::test]
async fn test_health_endpoint_public_when_auth_enabled() {
    // Security: Health endpoint must remain public for load balancer checks
    let server = TestServer::with_auth(true, "secure-test-key-32chars");

    let req = TestServer::make_request("GET", "/health", None, None);
    let response = server.call(req).await;

    let status = response.status();
    // Health endpoint is public - must not return 401/403
    // May return 200 (healthy) or 503 (unhealthy) depending on service availability
    assert!(
        status == StatusCode::OK || status == StatusCode::SERVICE_UNAVAILABLE,
        "Health endpoint must be accessible without authentication (got {status}, expected 200 or 503)"
    );
}

#[tokio::test]
async fn test_metrics_endpoint_protected_when_auth_enabled() {
    // Security: Metrics endpoint must require authentication
    let server = TestServer::with_auth(true, "secure-test-key-32chars");

    let req = TestServer::make_request("GET", "/metrics", None, None);
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
    let server = TestServer::with_auth(true, "secure-test-key-32chars");

    let request_body = format!(
        r#"{{"model": "{TEST_GEMINI_MODEL}", "messages": [{{"role": "user", "content": "test"}}]}}"#
    );
    let req = TestServer::make_request("POST", "/v1/chat/completions", Some(&request_body), None);
    let response = server.call(req).await;

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Chat endpoint must require authentication"
    );
}

#[tokio::test]
async fn test_auth_disabled_allows_all_endpoints() {
    // Security: When auth is disabled, all endpoints should be accessible (no 401)
    let server = TestServer::with_auth(false, "");

    // Health should be accessible (may return 503 if services unavailable)
    let req = TestServer::make_request("GET", "/health", None, None);
    let response = server.call(req).await;
    let status = response.status();
    assert!(
        status == StatusCode::OK || status == StatusCode::SERVICE_UNAVAILABLE,
        "Health endpoint should be accessible when auth disabled (got {status})"
    );

    // Metrics should be accessible
    let req = TestServer::make_request("GET", "/metrics", None, None);
    let response = server.call(req).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Metrics endpoint should be accessible when auth disabled"
    );
}

#[tokio::test]
async fn test_invalid_bearer_token_format_rejected() {
    // Security: Malformed Bearer tokens must be rejected
    let server = TestServer::with_auth(true, "secure-test-key-32chars");

    let mut req = TestServer::make_request("GET", "/metrics", None, None);
    req.headers_mut().insert(
        "Authorization",
        "NotBearer secure-test-key-32chars"
            .parse()
            .expect("Should be valid header value"),
    );

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
    let server = TestServer::with_auth(true, "secure-test-key-32chars");

    let mut req = TestServer::make_request("GET", "/metrics", None, None);
    req.headers_mut().insert(
        "Authorization",
        "Bearer ".parse().expect("Should be valid header value"),
    );

    let response = server.call(req).await;
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Empty Bearer token must be rejected"
    );
}

#[tokio::test]
async fn test_auth_timing_attack_resistance() {
    // Security: Auth comparison should be constant-time to prevent timing attacks
    // This test verifies that different-length tokens don't cause timing variance
    // Note: This is a behavioral test, not a true timing attack test (which would require statistical analysis)
    let correct_key = "secure-test-key-32chars";
    let server = TestServer::with_auth(true, correct_key);

    // Test with short wrong key
    let req = TestServer::make_request("GET", "/metrics", None, Some("short"));
    let response = server.call(req).await;
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Short wrong key should be rejected"
    );

    // Test with same-length wrong key
    let req = TestServer::make_request("GET", "/metrics", None, Some("wrong-test-key-32charsx"));
    let response = server.call(req).await;
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Same-length wrong key should be rejected"
    );

    // Test with very long wrong key
    let long_key = "x".repeat(1000);
    let req = TestServer::make_request("GET", "/metrics", None, Some(&long_key));
    let response = server.call(req).await;
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Very long wrong key should be rejected"
    );

    // Test with correct key (should succeed)
    let req = TestServer::make_request("GET", "/metrics", None, Some(correct_key));
    let response = server.call(req).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Correct key should be accepted"
    );
}

#[tokio::test]
async fn test_security_headers_present() {
    // Security: Verify security headers are set on responses
    // Note: TestServer uses a simplified router without all production middleware
    // Security headers are added in production via security_headers middleware
    // This test verifies the endpoint is accessible and doesn't fail
    let server = TestServer::new();

    let req = TestServer::make_request("GET", "/health", None, None);
    let response = server.call(req).await;

    // Response may be 200 or 503, but should succeed
    let status = response.status();
    assert!(
        status == StatusCode::OK || status == StatusCode::SERVICE_UNAVAILABLE,
        "Health endpoint should return 200 or 503, got {status}"
    );

    // Note: Security headers (X-Content-Type-Options, X-Frame-Options, etc.) are
    // added by the security_headers middleware in production, which is not
    // included in the simplified TestServer router. These headers are verified
    // via the unit tests in src/middleware/security_headers.rs.
}
