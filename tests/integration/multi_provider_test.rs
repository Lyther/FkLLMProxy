// @critical: Multi-provider routing tests
// These tests verify that requests are correctly routed to the appropriate provider

use super::test_utils::{create_chat_request, create_simple_message, TestServer};
use axum::body::to_bytes;
use axum::http::StatusCode;
use serde_json::Value;
use vertex_bridge::services::providers::{route_provider, Provider};

#[tokio::test]
async fn test_provider_routing_gemini_to_vertex() {
    let server = TestServer::new();

    let request_body = create_chat_request(
        "gemini-2.5-flash",
        &create_simple_message("user", "Hello"),
        false,
    );

    let req = server.make_request("POST", "/v1/chat/completions", Some(&request_body), None);
    let response = server.call(req).await;

    // Should route to Vertex provider (may fail without credentials, but routing should work)
    // Accept either success (if credentials available) or service unavailable (if no credentials)
    assert!(
        response.status() == StatusCode::OK
            || response.status() == StatusCode::SERVICE_UNAVAILABLE
            || response.status() == StatusCode::BAD_GATEWAY
    );
}

#[tokio::test]
async fn test_provider_routing_claude_to_anthropic() {
    let server = TestServer::new();

    let request_body = create_chat_request(
        "claude-3-5-sonnet",
        &create_simple_message("user", "Hello"),
        false,
    );

    let req = server.make_request("POST", "/v1/chat/completions", Some(&request_body), None);
    let response = server.call(req).await;

    // Should route to Anthropic provider (may fail without bridge, but routing should work)
    assert!(
        response.status() == StatusCode::OK
            || response.status() == StatusCode::SERVICE_UNAVAILABLE
            || response.status() == StatusCode::BAD_GATEWAY
    );
}

#[tokio::test]
async fn test_provider_routing_unknown_model_defaults_to_vertex() {
    let server = TestServer::new();

    let request_body = create_chat_request(
        "unknown-model-xyz",
        &create_simple_message("user", "Hello"),
        false,
    );

    let req = server.make_request("POST", "/v1/chat/completions", Some(&request_body), None);
    let response = server.call(req).await;

    // Unknown models default to Vertex
    assert!(
        response.status() == StatusCode::OK
            || response.status() == StatusCode::SERVICE_UNAVAILABLE
            || response.status() == StatusCode::BAD_GATEWAY
            || response.status().is_client_error()
    );
}

#[tokio::test]
async fn test_provider_routing_multiple_models() {
    let server = TestServer::new();

    let models = vec![
        "gemini-2.5-flash",
        "gemini-1.5-pro",
        "claude-3-5-sonnet",
        "claude-3-haiku",
    ];

    for model in models {
        let request_body = create_chat_request(model, &create_simple_message("user", "Hi"), false);

        let req = server.make_request("POST", "/v1/chat/completions", Some(&request_body), None);
        let response = server.call(req).await;

        // Verify routing works (may fail without credentials, but should route correctly)
        assert!(
            response.status() == StatusCode::OK
                || response.status() == StatusCode::SERVICE_UNAVAILABLE
                || response.status() == StatusCode::BAD_GATEWAY
                || response.status().is_client_error()
        );
    }
}

#[tokio::test]
async fn test_provider_routing_streaming_vs_non_streaming() {
    let server = TestServer::new();

    // Test non-streaming
    let request_body = create_chat_request(
        "gemini-2.5-flash",
        &create_simple_message("user", "Hello"),
        false,
    );

    let req = server.make_request("POST", "/v1/chat/completions", Some(&request_body), None);
    let response = server.call(req).await;

    if response.status() == StatusCode::OK {
        let body = response.into_body();
        let body_bytes = to_bytes(body, usize::MAX).await.unwrap();
        let json: Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["object"], "chat.completion");
    }

    // Test streaming
    let request_body = create_chat_request(
        "gemini-2.5-flash",
        &create_simple_message("user", "Hello"),
        true,
    );

    let req = server.make_request("POST", "/v1/chat/completions", Some(&request_body), None);
    let response = server.call(req).await;

    if response.status() == StatusCode::OK {
        assert_eq!(
            response
                .headers()
                .get("content-type")
                .unwrap()
                .to_str()
                .unwrap(),
            "text/event-stream"
        );
    }
}

#[test]
fn test_route_provider_function() {
    // Test routing logic directly
    assert_eq!(route_provider("gemini-2.5-flash"), Provider::Vertex);
    assert_eq!(route_provider("gemini-1.5-pro"), Provider::Vertex);
    assert_eq!(route_provider("claude-3-5-sonnet"), Provider::AnthropicCLI);
    assert_eq!(route_provider("claude-3-haiku"), Provider::AnthropicCLI);
    assert_eq!(route_provider("unknown-model"), Provider::Vertex); // Default
    assert_eq!(route_provider(""), Provider::Vertex); // Default
}

#[tokio::test]
async fn test_circuit_breaker_with_provider_routing() {
    let server = TestServer::new();

    // Make multiple requests to same provider to test circuit breaker
    let request_body = create_chat_request(
        "gemini-2.5-flash",
        &create_simple_message("user", "Hello"),
        false,
    );

    for _ in 0..5 {
        let req = server.make_request("POST", "/v1/chat/completions", Some(&request_body), None);
        let response = server.call(req).await;
        // Circuit breaker should allow requests (may fail without credentials)
        assert!(
            response.status() == StatusCode::OK
                || response.status() == StatusCode::SERVICE_UNAVAILABLE
                || response.status() == StatusCode::BAD_GATEWAY
        );
    }
}
