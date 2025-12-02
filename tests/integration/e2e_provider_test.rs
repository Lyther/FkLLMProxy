// @critical: E2E provider tests with real APIs
// These tests verify end-to-end functionality with actual provider APIs

use super::test_utils::{
    create_chat_request, create_simple_message, credential_status, should_run_e2e, TestServer,
};
use axum::body::to_bytes;
use axum::http::StatusCode;
use serde_json::Value;

/// Check if Anthropic credentials are available
fn has_anthropic_credentials() -> bool {
    // Anthropic uses CLI authentication, check if bridge is accessible
    std::env::var("ANTHROPIC_BRIDGE_URL").is_ok()
        || std::env::var("APP_ANTHROPIC__BRIDGE_URL").is_ok()
}

/// Check if Vertex credentials are available
fn has_vertex_credentials() -> bool {
    std::env::var("VERTEX_API_KEY").is_ok()
        || std::env::var("GOOGLE_APPLICATION_CREDENTIALS").is_ok()
        || (std::env::var("GOOGLE_CLOUD_PROJECT").is_ok()
            && std::env::var("GOOGLE_APPLICATION_CREDENTIALS").is_ok())
}

#[tokio::test]
#[ignore] // Requires real Vertex API credentials
async fn test_vertex_e2e_non_streaming() {
    if !has_vertex_credentials() {
        eprintln!("⏭️  Skipping Vertex E2E test: {}", credential_status());
        return;
    }

    let server = TestServer::new();

    let request_body = create_chat_request(
        "gemini-2.5-flash",
        &create_simple_message("user", "Say hello in one word"),
        false,
    );

    let req = server.make_request("POST", "/v1/chat/completions", Some(&request_body), None);
    let response = server.call(req).await;

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body();
    let body_bytes = to_bytes(body, usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["object"], "chat.completion");
    assert!(json.get("id").is_some());
    assert_eq!(json["model"], "gemini-2.5-flash");
    assert!(json.get("choices").is_some());

    let choices = json["choices"].as_array().unwrap();
    assert!(!choices.is_empty());
    assert_eq!(choices[0]["message"]["role"], "assistant");
    assert!(choices[0]["message"].get("content").is_some());
}

#[tokio::test]
#[ignore] // Requires real Vertex API credentials
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

    assert!(body_str.contains("data: "));
    assert!(body_str.contains("chat.completion.chunk") || body_str.contains("[DONE]"));
}

#[tokio::test]
#[ignore] // Requires real Anthropic bridge
async fn test_anthropic_e2e_non_streaming() {
    if !has_anthropic_credentials() {
        eprintln!("⏭️  Skipping Anthropic E2E test: Anthropic bridge not configured");
        return;
    }

    let server = TestServer::new();

    let request_body = create_chat_request(
        "claude-3-5-sonnet",
        &create_simple_message("user", "Say hello"),
        false,
    );

    let req = server.make_request("POST", "/v1/chat/completions", Some(&request_body), None);
    let response = server.call(req).await;

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body();
    let body_bytes = to_bytes(body, usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["object"], "chat.completion");
    assert!(json.get("id").is_some());
    assert_eq!(json["model"], "claude-3-5-sonnet");
    assert!(json.get("choices").is_some());

    let choices = json["choices"].as_array().unwrap();
    assert!(!choices.is_empty());
    assert_eq!(choices[0]["message"]["role"], "assistant");
    assert!(choices[0]["message"].get("content").is_some());
}

#[tokio::test]
#[ignore] // Requires real Anthropic bridge
async fn test_anthropic_e2e_streaming() {
    if !has_anthropic_credentials() {
        eprintln!("⏭️  Skipping Anthropic E2E streaming test: Anthropic bridge not configured");
        return;
    }

    let server = TestServer::new();

    let request_body = create_chat_request(
        "claude-3-5-sonnet",
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

    assert!(body_str.contains("data: "));
    assert!(body_str.contains("chat.completion.chunk") || body_str.contains("[DONE]"));
}

#[tokio::test]
#[ignore] // Requires real credentials
async fn test_e2e_latency_benchmark() {
    if !should_run_e2e() {
        eprintln!("⏭️  Skipping latency benchmark: {}", credential_status());
        return;
    }

    let server = TestServer::new();

    let request_body = create_chat_request(
        "gemini-2.5-flash",
        &create_simple_message("user", "Say hello"),
        false,
    );

    let start = std::time::Instant::now();
    let req = server.make_request("POST", "/v1/chat/completions", Some(&request_body), None);
    let response = server.call(req).await;
    let duration = start.elapsed();

    assert_eq!(response.status(), StatusCode::OK);

    // Log latency for monitoring
    eprintln!("⏱️  E2E latency: {}ms", duration.as_millis());

    // Assert reasonable latency (should be < 10s for simple request)
    assert!(
        duration.as_secs() < 10,
        "Request took too long: {:?}",
        duration
    );
}
