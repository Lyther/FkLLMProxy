// @critical: Error handling tests - verify OpenAI-compatible error responses

use super::test_utils::TestServer;
use axum::body::to_bytes;
use axum::http::StatusCode;
use serde_json::Value;

/// Reasonable body size limit for tests (1MB)
const TEST_BODY_LIMIT: usize = 1024 * 1024;
/// Test model name constant
const TEST_INVALID_MODEL: &str = "invalid-model";
const TEST_GEMINI_MODEL: &str = "gemini-2.5-flash";

#[tokio::test]
async fn test_error_response_format() {
    let server = TestServer::new();

    let request_body = format!(
        r#"{{"model": "{TEST_INVALID_MODEL}", "messages": [{{"role": "user", "content": "test"}}]}}"#
    );
    let req = TestServer::make_request("POST", "/v1/chat/completions", Some(&request_body), None);
    let response = server.call(req).await;

    let status = response.status();
    // Invalid models return 400 (bad request) or 503 (no provider available)
    assert!(
        status == StatusCode::BAD_REQUEST
            || status == StatusCode::NOT_FOUND
            || status == StatusCode::SERVICE_UNAVAILABLE,
        "Expected 400, 404, or 503 for invalid model, got {status}"
    );

    let body = response.into_body();
    let body_bytes = to_bytes(body, TEST_BODY_LIMIT)
        .await
        .expect("Failed to read error response body");
    let json: Value =
        serde_json::from_slice(&body_bytes).expect("Error response is not valid JSON");

    // Verify OpenAI error format
    assert!(
        json.get("error").is_some(),
        "Error response missing 'error' field"
    );
    let error = &json["error"];
    assert!(
        error.get("message").is_some(),
        "Error object missing 'message' field"
    );
    assert!(
        error.get("type").is_some(),
        "Error object missing 'type' field"
    );
}

#[tokio::test]
async fn test_401_error_format() {
    // Test 401 on protected endpoint with wrong key
    let server = TestServer::with_auth(true, "correct-key");

    let req = TestServer::make_request("GET", "/metrics", None, Some("wrong-key"));
    let response = server.call(req).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    // Auth middleware returns plain 401, not JSON (by design)
}

#[tokio::test]
async fn test_404_not_found() {
    let server = TestServer::new();

    let req = TestServer::make_request("GET", "/nonexistent", None, None);
    let response = server.call(req).await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_malformed_json_400() {
    let server = TestServer::new();

    let req = TestServer::make_request("POST", "/v1/chat/completions", Some("{invalid}"), None);
    let response = server.call(req).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_missing_required_fields() {
    let server = TestServer::new();

    // Missing messages
    let request_body = format!(r#"{{"model": "{TEST_GEMINI_MODEL}"}}"#);
    let req = TestServer::make_request("POST", "/v1/chat/completions", Some(&request_body), None);
    let response = server.call(req).await;

    assert!(
        response.status().is_client_error(),
        "Missing messages should return 4xx error, got {}",
        response.status()
    );
}

#[tokio::test]
async fn test_empty_messages_array() {
    let server = TestServer::new();

    let request_body = format!(r#"{{"model": "{TEST_GEMINI_MODEL}", "messages": []}}"#);
    let req = TestServer::make_request("POST", "/v1/chat/completions", Some(&request_body), None);
    let response = server.call(req).await;

    let status = response.status();
    // Empty messages array should fail validation (400) or provider error (503)
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::SERVICE_UNAVAILABLE,
        "Empty messages should return 400 or 503, got {status}"
    );
}
