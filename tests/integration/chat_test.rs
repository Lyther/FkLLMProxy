// @critical: Chat completions E2E tests
// These tests verify the money/data flow: Request -> Transform -> Provider -> Transform -> Response

use super::test_utils::{
    create_chat_request, create_simple_message, credential_status, should_run_e2e, TestServer,
};
use axum::body::to_bytes;
use axum::http::StatusCode;
use serde_json::Value;

#[tokio::test]
#[ignore] // Requires real provider credentials - mark as @critical for CI
async fn test_chat_completions_non_streaming() {
    if !should_run_e2e() {
        eprintln!("⏭️  Skipping E2E test: {}", credential_status());
        return;
    }
    let server = TestServer::new();

    let request_body = create_chat_request(
        "gemini-2.5-flash",
        &create_simple_message("user", "Say hello"),
        false,
    );

    let req = server.make_request("POST", "/v1/chat/completions", Some(&request_body), None);
    let response = server.call(req).await;

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body();
    let body_bytes = to_bytes(body, usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body_bytes).unwrap();

    // Verify OpenAI-compatible response structure
    assert_eq!(json["object"], "chat.completion");
    assert!(json.get("id").is_some());
    assert!(json.get("model").is_some());
    assert!(json.get("choices").is_some());

    let choices = json["choices"].as_array().unwrap();
    assert!(!choices.is_empty());

    let first_choice = &choices[0];
    assert!(first_choice.get("message").is_some());
    assert_eq!(first_choice["message"]["role"], "assistant");
    assert!(first_choice["message"].get("content").is_some());
}

#[tokio::test]
#[ignore] // Requires real provider credentials
async fn test_chat_completions_streaming() {
    if !should_run_e2e() {
        eprintln!("⏭️  Skipping E2E test: {}", credential_status());
        return;
    }
    let server = TestServer::new();

    let request_body = create_chat_request(
        "gemini-2.5-flash",
        &create_simple_message("user", "Count to 3"),
        true,
    );

    let req = server.make_request("POST", "/v1/chat/completions", Some(&request_body), None);
    let response = server.call(req).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap(),
        "text/event-stream"
    );

    let body = response.into_body();
    let body_bytes = to_bytes(body, usize::MAX).await.unwrap();
    let body_str = String::from_utf8_lossy(&body_bytes);

    // Verify SSE format
    assert!(body_str.contains("data: "));

    // Parse first chunk
    let lines: Vec<&str> = body_str.lines().collect();
    let data_line = lines.iter().find(|l| l.starts_with("data: ")).unwrap();
    let json_str = data_line.strip_prefix("data: ").unwrap();

    if json_str != "[DONE]" {
        let chunk: Value = serde_json::from_str(json_str).unwrap();
        assert_eq!(chunk["object"], "chat.completion.chunk");
        assert!(chunk.get("choices").is_some());
    }
}

#[tokio::test]
async fn test_chat_completions_invalid_model() {
    let server = TestServer::new();

    let request_body = create_chat_request(
        "invalid-model-xyz",
        &create_simple_message("user", "Hello"),
        false,
    );

    let req = server.make_request("POST", "/v1/chat/completions", Some(&request_body), None);
    let response = server.call(req).await;

    // Invalid models are routed to providers which fail with 503 (service unavailable)
    // or 400 if caught early. Accept either as valid error response.
    assert!(
        response.status().is_client_error() || response.status() == StatusCode::SERVICE_UNAVAILABLE
    );

    let body = response.into_body();
    let body_bytes = to_bytes(body, usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body_bytes).unwrap();

    // Invalid models may return "invalid_request_error" (400) or "server_error" (503)
    let error_type = json["error"]["type"].as_str().unwrap();
    assert!(error_type == "invalid_request_error" || error_type == "server_error");
    assert!(
        json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Unsupported model")
            || json["error"]["message"]
                .as_str()
                .unwrap()
                .contains("Vertex API Error")
    );
}

#[tokio::test]
async fn test_chat_completions_malformed_request() {
    let server = TestServer::new();

    let req = server.make_request("POST", "/v1/chat/completions", Some("invalid json"), None);
    let response = server.call(req).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_chat_completions_missing_messages() {
    let server = TestServer::new();

    let request_body = r#"{"model": "gemini-2.5-flash"}"#;
    let req = server.make_request("POST", "/v1/chat/completions", Some(request_body), None);
    let response = server.call(req).await;

    // Should fail validation
    assert!(response.status().is_client_error());
}
