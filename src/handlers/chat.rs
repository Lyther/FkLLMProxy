use axum::{
    extract::State,
    response::{sse::Event, IntoResponse, Sse},
    Json,
};
use futures::stream::StreamExt;
use serde_json::Value;
use std::convert::Infallible;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    handlers::openai_chat,
    models::openai::{ChatCompletionChunk, ChatCompletionRequest},
    openai::errors::map_error_with_status,
    services::providers::ProviderError,
    state::AppState,
};

#[must_use]
pub fn is_openai_model(model: &str) -> bool {
    // gpt-3.5 and gpt-4 are already covered by starts_with("gpt-")
    model.starts_with("gpt-")
}

fn parse_sse_chunk(chunk_data: &str) -> Event {
    // Validate SSE format: should start with "data: "
    if !chunk_data.starts_with("data: ") {
        if !chunk_data.trim().is_empty() {
            warn!(
                "Invalid SSE format: chunk does not start with 'data: ': {}",
                chunk_data
            );
        }
        return Event::default().comment(chunk_data.trim());
    }

    let Some(json_data) = chunk_data.strip_prefix("data: ") else {
        warn!(
            "Invalid SSE format after prefix check: missing 'data: ' prefix in chunk: {}",
            chunk_data
        );
        return Event::default().comment("error: invalid sse chunk");
    };
    let json_data = json_data.trim();
    if json_data == "[DONE]" {
        return Event::default().comment("[DONE]");
    }

    // Try to parse as ChatCompletionChunk first
    match serde_json::from_str::<ChatCompletionChunk>(json_data) {
        Ok(chunk) => match Event::default().json_data(chunk) {
            Ok(e) => e,
            Err(e) => {
                error!("Failed to serialize SSE chunk: {e}");
                Event::default().comment(format!("error: serialization failed: {e}"))
            }
        },
        Err(e) => {
            // Log parse error before fallback
            warn!("Failed to parse SSE chunk as ChatCompletionChunk: {}", e);
            // Try parsing as generic JSON Value
            match serde_json::from_str::<Value>(json_data) {
                Ok(value) => match Event::default().json_data(value) {
                    Ok(e) => e,
                    Err(ser_err) => {
                        error!("Failed to serialize JSON value: {ser_err}");
                        Event::default().comment("error: serialization failed")
                    }
                },
                Err(json_err) => {
                    // Log both parse errors for debugging
                    error!("Failed to parse JSON from SSE chunk. ChatCompletionChunk error: {e}, JSON error: {json_err}");
                    // Return error event instead of silently converting to comment
                    let error_event = serde_json::json!({
                        "error": {
                            "message": format!("Failed to parse SSE chunk: {e}"),
                            "type": "parse_error",
                            "code": "invalid_chunk_format"
                        }
                    });
                    match Event::default().json_data(error_event) {
                        Ok(event) => event,
                        Err(_) => Event::default().comment(format!("error: parse failed: {e}")),
                    }
                }
            }
        }
    }
}

pub async fn chat_completions(
    State(state): State<AppState>,
    Json(req): Json<ChatCompletionRequest>,
) -> axum::response::Response {
    // Validate request
    if let Err(e) = req.validate() {
        error!("Invalid request: {e}");
        return map_error_with_status(400, &format!("Invalid request: {e}"));
    }

    if is_openai_model(&req.model) {
        return openai_chat::openai_chat_completions(State(state), Json(req)).await;
    }

    let request_start = std::time::Instant::now();
    let request_id = Uuid::new_v4().to_string();
    let span = tracing::span!(
        tracing::Level::INFO,
        "chat_completions",
        request_id = %request_id,
        model = %req.model,
        stream = req.stream
    );
    let _guard = span.enter();
    info!(
        "Received request: {} for model: {} (stream={})",
        request_id, req.model, req.stream
    );

    let Some(provider) = state.provider_registry.route_by_model(&req.model) else {
        error!("No provider found for model: {}", req.model);
        return map_error_with_status(400, &format!("Unsupported model: {}", req.model));
    };

    if req.stream {
        let stream_result = provider.execute_stream(req, &state).await;

        let stream = match stream_result {
            Ok(provider_stream) => provider_stream.map(move |chunk_result| match chunk_result {
                Ok(chunk_data) => Ok::<Event, Infallible>(parse_sse_chunk(&chunk_data)),
                Err(e) => {
                    error!("Provider stream error: {}", e);
                    let error_chunk = serde_json::json!({
                        "error": {
                            "message": format!("Stream error: {}", e),
                            "type": "stream_error",
                            "code": "stream_failed"
                        }
                    });
                    match Event::default().json_data(error_chunk) {
                        Ok(event) => Ok::<Event, Infallible>(event),
                        Err(_) => Ok::<Event, Infallible>(
                            Event::default().comment(format!("error: stream failed: {e}")),
                        ),
                    }
                }
            }),
            Err(e) => {
                error!("Provider execution error: {}", e);
                let status = map_provider_error_to_status(&e);
                state.metrics.record_request(false).await;
                return map_error_with_status(status, &e.to_string());
            }
        };

        // Note: Metrics for streaming requests are recorded when stream is created
        // Full stream completion metrics would require consuming the stream, which isn't feasible
        // For accurate metrics, consider using a wrapper stream that records on completion
        return Sse::new(stream)
            .keep_alive(axum::response::sse::KeepAlive::default())
            .into_response();
    }

    match provider.execute(req, &state).await {
        Ok(response) => {
            // Fix: Prevent overflow when converting duration to milliseconds
            let duration_ms = u64::try_from(
                request_start
                    .elapsed()
                    .as_millis()
                    .min(u128::from(u64::MAX)),
            )
            .unwrap_or(u64::MAX);
            state.metrics.record_request(true).await;
            state.metrics.record_request_duration(duration_ms).await;
            Json(response).into_response()
        }
        Err(e) => {
            error!("Provider execution error: {}", e);
            let status = map_provider_error_to_status(&e);
            state.metrics.record_request(false).await;
            map_error_with_status(status, &e.to_string())
        }
    }
}

fn map_provider_error_to_status(error: &ProviderError) -> u16 {
    match error {
        ProviderError::Auth(_) => 401,
        ProviderError::Network(_) => 502,
        ProviderError::Unavailable(_) | ProviderError::CircuitOpen(_) => 503,
        ProviderError::Timeout(_) => 504,
        ProviderError::InvalidRequest(_) => 400,
        ProviderError::RateLimited(_) => 429,
        ProviderError::Internal(_) => 500,
    }
}
