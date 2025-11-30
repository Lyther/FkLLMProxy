// @critical: Error handling tests - verify OpenAI-compatible error responses

use super::test_utils::TestServer;
use axum::body::to_bytes;
use axum::http::StatusCode;
use serde_json::Value;

#[tokio::test]
async fn test_error_response_format() {
    let server = TestServer::new();

    // Invalid model triggers 400
    let request_body =
        r#"{"model": "invalid-model", "messages": [{"role": "user", "content": "test"}]}"#;
    let req = server.make_request("POST", "/v1/chat/completions", Some(request_body), None);
    let response = server.call(req).await;

    // Invalid models may return 400 or 503 depending on when validation occurs
    assert!(
        response.status().is_client_error() || response.status() == StatusCode::SERVICE_UNAVAILABLE
    );

    let body = response.into_body();
    let body_bytes = to_bytes(body, usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body_bytes).unwrap();

    // Verify OpenAI error format
    assert!(json.get("error").is_some());
    let error = &json["error"];
    assert!(error.get("message").is_some());
    assert!(error.get("type").is_some());
    // Invalid models may return "invalid_request_error" (400) or "server_error" (503)
    // depending on when validation occurs
    assert!(error["type"] == "invalid_request_error" || error["type"] == "server_error");
}

#[tokio::test]
async fn test_401_error_format() {
    // Test 401 on protected endpoint with wrong key
    let server = TestServer::with_auth(true, "correct-key");

    let req = server.make_request("GET", "/metrics", None, Some("wrong-key"));
    let response = server.call(req).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    // Auth middleware returns plain 401, not JSON (by design)
}

#[tokio::test]
async fn test_404_not_found() {
    let server = TestServer::new();

    let req = server.make_request("GET", "/nonexistent", None, None);
    let response = server.call(req).await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_malformed_json_400() {
    let server = TestServer::new();

    let req = server.make_request("POST", "/v1/chat/completions", Some("{invalid}"), None);
    let response = server.call(req).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_missing_required_fields() {
    let server = TestServer::new();

    // Missing messages
    let request_body = r#"{"model": "gemini-2.5-flash"}"#;
    let req = server.make_request("POST", "/v1/chat/completions", Some(request_body), None);
    let response = server.call(req).await;

    assert!(response.status().is_client_error());
}

#[tokio::test]
async fn test_empty_messages_array() {
    let server = TestServer::new();

    let request_body = r#"{"model": "gemini-2.5-flash", "messages": []}"#;
    let req = server.make_request("POST", "/v1/chat/completions", Some(request_body), None);
    let response = server.call(req).await;

    // Empty messages array should fail validation (400) or provider error (503)
    assert!(
        response.status().is_client_error() || response.status() == StatusCode::SERVICE_UNAVAILABLE
    );
}
