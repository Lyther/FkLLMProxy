// @critical: E2E provider tests with real APIs
// These tests verify end-to-end functionality with actual provider APIs
//
// To run E2E tests: FORCE_E2E_TESTS=1 cargo test --test integration -- --ignored

use super::test_utils::{
    create_chat_request, create_simple_message, credential_status, has_real_credentials,
    should_run_e2e, TestServer,
};
use axum::body::to_bytes;
use axum::http::StatusCode;
use serde_json::Value;

/// Reasonable body size limit for tests (1MB)
const TEST_BODY_LIMIT: usize = 1024 * 1024;
/// Model name constants
const GEMINI_MODEL: &str = "gemini-2.5-flash";
const CLAUDE_MODEL: &str = "claude-3-5-sonnet";

/// Check if Anthropic credentials are available
fn has_anthropic_credentials() -> bool {
    // Anthropic uses CLI authentication, check if bridge is accessible
    std::env::var("ANTHROPIC_BRIDGE_URL").is_ok()
        || std::env::var("APP_ANTHROPIC__BRIDGE_URL").is_ok()
}

/// Check if Vertex credentials are available - reuses has_real_credentials from test_utils
fn has_vertex_credentials() -> bool {
    has_real_credentials()
}

#[tokio::test]
#[ignore] // Requires real Vertex API credentials - run with FORCE_E2E_TESTS=1
async fn test_vertex_e2e_non_streaming() {
    if !has_vertex_credentials() {
        eprintln!("⏭️  Skipping Vertex E2E test: {}", credential_status());
        return;
    }

    let server = TestServer::new();

    let request_body = create_chat_request(
        GEMINI_MODEL,
        &create_simple_message("user", "Say hello in one word"),
        false,
    );

    let req = server.make_request("POST", "/v1/chat/completions", Some(&request_body), None);
    let response = server.call(req).await;

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body();
    let body_bytes = to_bytes(body, TEST_BODY_LIMIT)
        .await
        .expect("Failed to read Vertex response body");
    let json: Value =
        serde_json::from_slice(&body_bytes).expect("Vertex response is not valid JSON");

    assert_eq!(json["object"], "chat.completion");
    assert!(json.get("id").is_some(), "Response missing 'id' field");
    assert_eq!(json["model"], GEMINI_MODEL);
    assert!(
        json.get("choices").is_some(),
        "Response missing 'choices' field"
    );

    let choices = json["choices"]
        .as_array()
        .expect("choices field is not an array");
    assert!(!choices.is_empty(), "choices array should not be empty");
    assert_eq!(choices[0]["message"]["role"], "assistant");
    assert!(
        choices[0]["message"].get("content").is_some(),
        "Message missing 'content' field"
    );
}

#[tokio::test]
#[ignore] // Requires real Vertex API credentials - run with FORCE_E2E_TESTS=1
async fn test_vertex_e2e_streaming() {
    if !has_vertex_credentials() {
        eprintln!(
            "⏭️  Skipping Vertex E2E streaming test: {}",
            credential_status()
        );
        return;
    }

    let server = TestServer::new();

    let request_body = create_chat_request(
        GEMINI_MODEL,
        &create_simple_message("user", "Count to 3"),
        true,
    );

    let req = server.make_request("POST", "/v1/chat/completions", Some(&request_body), None);
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
        .expect("Failed to read Vertex streaming response body");
    let body_str = String::from_utf8_lossy(&body_bytes);

    assert!(
        body_str.contains("data: "),
        "Streaming response should contain SSE data lines"
    );
    assert!(
        body_str.contains("chat.completion.chunk") || body_str.contains("[DONE]"),
        "Streaming response should contain chunk data or [DONE]"
    );
}

#[tokio::test]
#[ignore] // Requires real Anthropic bridge - run with FORCE_E2E_TESTS=1
async fn test_anthropic_e2e_non_streaming() {
    if !has_anthropic_credentials() {
        eprintln!("⏭️  Skipping Anthropic E2E test: Anthropic bridge not configured");
        return;
    }

    let server = TestServer::new();

    let request_body = create_chat_request(
        CLAUDE_MODEL,
        &create_simple_message("user", "Say hello"),
        false,
    );

    let req = server.make_request("POST", "/v1/chat/completions", Some(&request_body), None);
    let response = server.call(req).await;

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body();
    let body_bytes = to_bytes(body, TEST_BODY_LIMIT)
        .await
        .expect("Failed to read Anthropic response body");
    let json: Value =
        serde_json::from_slice(&body_bytes).expect("Anthropic response is not valid JSON");

    assert_eq!(json["object"], "chat.completion");
    assert!(json.get("id").is_some(), "Response missing 'id' field");
    assert_eq!(json["model"], CLAUDE_MODEL);
    assert!(
        json.get("choices").is_some(),
        "Response missing 'choices' field"
    );

    let choices = json["choices"]
        .as_array()
        .expect("choices field is not an array");
    assert!(!choices.is_empty(), "choices array should not be empty");
    assert_eq!(choices[0]["message"]["role"], "assistant");
    assert!(
        choices[0]["message"].get("content").is_some(),
        "Message missing 'content' field"
    );
}

#[tokio::test]
#[ignore] // Requires real Anthropic bridge - run with FORCE_E2E_TESTS=1
async fn test_anthropic_e2e_streaming() {
    if !has_anthropic_credentials() {
        eprintln!("⏭️  Skipping Anthropic E2E streaming test: Anthropic bridge not configured");
        return;
    }

    let server = TestServer::new();

    let request_body = create_chat_request(
        CLAUDE_MODEL,
        &create_simple_message("user", "Count to 3"),
        true,
    );

    let req = server.make_request("POST", "/v1/chat/completions", Some(&request_body), None);
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
        .expect("Failed to read Anthropic streaming response body");
    let body_str = String::from_utf8_lossy(&body_bytes);

    assert!(
        body_str.contains("data: "),
        "Streaming response should contain SSE data lines"
    );
    assert!(
        body_str.contains("chat.completion.chunk") || body_str.contains("[DONE]"),
        "Streaming response should contain chunk data or [DONE]"
    );
}

#[tokio::test]
#[ignore] // Requires real credentials - run with FORCE_E2E_TESTS=1
async fn test_e2e_latency_benchmark() {
    if !should_run_e2e() {
        eprintln!("⏭️  Skipping latency benchmark: {}", credential_status());
        return;
    }

    let server = TestServer::new();

    let request_body = create_chat_request(
        GEMINI_MODEL,
        &create_simple_message("user", "Say hello"),
        false,
    );

    let start = std::time::Instant::now();
    let req = server.make_request("POST", "/v1/chat/completions", Some(&request_body), None);
    let response = server.call(req).await;
    let duration = start.elapsed();

    assert_eq!(response.status(), StatusCode::OK);

    // Log latency for monitoring (use as_millis() for better precision)
    let latency_ms = duration.as_millis();
    eprintln!("⏱️  E2E latency: {}ms", latency_ms);

    // Assert reasonable latency (should be < 10s for simple request)
    assert!(
        latency_ms < 10_000,
        "Request took too long: {}ms (expected < 10000ms)",
        latency_ms
    );
}
