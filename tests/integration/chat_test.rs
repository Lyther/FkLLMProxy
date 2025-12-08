// @critical: Chat completions E2E tests
// These tests verify the money/data flow: Request -> Transform -> Provider -> Transform -> Response
//
// To run E2E tests: FORCE_E2E_TESTS=1 cargo test --test integration -- --ignored

use super::test_utils::{
    create_chat_request, create_simple_message, credential_status, should_run_e2e, TestServer,
};
use axum::body::to_bytes;
use axum::http::StatusCode;
use serde_json::Value;

/// Reasonable body size limit for tests (1MB)
const TEST_BODY_LIMIT: usize = 1024 * 1024;
/// Test model name constant
const TEST_GEMINI_MODEL: &str = "gemini-2.5-flash";
const TEST_INVALID_MODEL: &str = "invalid-model-xyz";

#[tokio::test]
#[ignore = "Requires real provider credentials - run with FORCE_E2E_TESTS=1"]
async fn test_chat_completions_non_streaming() {
    if !should_run_e2e() {
        eprintln!("⏭️  Skipping E2E test: {}", credential_status());
        return;
    }
    let server = TestServer::new();

    let request_body = create_chat_request(
        TEST_GEMINI_MODEL,
        &create_simple_message("user", "Say hello"),
        false,
    );

    let req = TestServer::make_request("POST", "/v1/chat/completions", Some(&request_body), None);
    let response = server.call(req).await;

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body();
    let body_bytes = to_bytes(body, TEST_BODY_LIMIT)
        .await
        .expect("Failed to read chat response body");
    let json: Value = serde_json::from_slice(&body_bytes).expect("Chat response is not valid JSON");

    // Verify OpenAI-compatible response structure
    assert_eq!(json["object"], "chat.completion");
    assert!(json.get("id").is_some(), "Response missing 'id' field");
    assert!(
        json.get("model").is_some(),
        "Response missing 'model' field"
    );
    assert!(
        json.get("choices").is_some(),
        "Response missing 'choices' field"
    );

    let choices = json["choices"]
        .as_array()
        .expect("choices field is not an array");
    assert!(!choices.is_empty(), "choices array should not be empty");

    let first_choice = &choices[0];
    assert!(
        first_choice.get("message").is_some(),
        "First choice missing 'message' field"
    );
    assert_eq!(first_choice["message"]["role"], "assistant");
    assert!(
        first_choice["message"].get("content").is_some(),
        "Message missing 'content' field"
    );
}

#[tokio::test]
#[ignore = "Requires real provider credentials - run with FORCE_E2E_TESTS=1"]
async fn test_chat_completions_streaming() {
    if !should_run_e2e() {
        eprintln!("⏭️  Skipping E2E test: {}", credential_status());
        return;
    }
    let server = TestServer::new();

    let request_body = create_chat_request(
        TEST_GEMINI_MODEL,
        &create_simple_message("user", "Count to 3"),
        true,
    );

    let req = TestServer::make_request("POST", "/v1/chat/completions", Some(&request_body), None);
    let response = server.call(req).await;

    assert_eq!(response.status(), StatusCode::OK);

    let content_type = response
        .headers()
        .get("content-type")
        .expect("Streaming response should have content-type header")
        .to_str()
        .expect("Content-type header should be valid UTF-8");
    assert_eq!(content_type, "text/event-stream");

    let body = response.into_body();
    let body_bytes = to_bytes(body, TEST_BODY_LIMIT)
        .await
        .expect("Failed to read streaming response body");
    let body_str = String::from_utf8_lossy(&body_bytes);

    // Verify SSE format
    assert!(
        body_str.contains("data: "),
        "Streaming response should contain SSE data lines"
    );

    // Parse first data chunk
    let lines: Vec<&str> = body_str.lines().collect();
    let data_line = lines
        .iter()
        .find(|l| l.starts_with("data: "))
        .expect("Streaming response should contain at least one data line");
    let json_str = data_line
        .strip_prefix("data: ")
        .expect("Data line should have 'data: ' prefix");

    if json_str != "[DONE]" {
        let chunk: Value = serde_json::from_str(json_str).expect("SSE data should be valid JSON");
        assert_eq!(chunk["object"], "chat.completion.chunk");
        assert!(
            chunk.get("choices").is_some(),
            "Chunk should contain 'choices' field"
        );
    }
}

#[tokio::test]
async fn test_chat_completions_invalid_model() {
    let server = TestServer::new();

    let request_body = create_chat_request(
        TEST_INVALID_MODEL,
        &create_simple_message("user", "Hello"),
        false,
    );

    let req = TestServer::make_request("POST", "/v1/chat/completions", Some(&request_body), None);
    let response = server.call(req).await;

    let status = response.status();
    // Invalid models: 400 (bad request), 404 (not found), or 503 (no provider)
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
        serde_json::from_slice(&body_bytes).expect("Error response should be valid JSON");

    // Verify error structure exists
    assert!(
        json.get("error").is_some(),
        "Error response should contain 'error' field"
    );
}

#[tokio::test]
async fn test_chat_completions_malformed_request() {
    let server = TestServer::new();

    let req = TestServer::make_request("POST", "/v1/chat/completions", Some("invalid json"), None);
    let response = server.call(req).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_chat_completions_missing_messages() {
    let server = TestServer::new();

    let request_body = format!(r#"{{"model": "{TEST_GEMINI_MODEL}"}}"#);
    let req = TestServer::make_request("POST", "/v1/chat/completions", Some(&request_body), None);
    let response = server.call(req).await;

    // Should fail validation - missing required messages field
    assert!(
        response.status().is_client_error(),
        "Missing messages should return 4xx error, got {}",
        response.status()
    );
}
