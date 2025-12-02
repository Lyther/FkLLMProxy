use axum::{
    extract::State,
    response::{sse::Event, IntoResponse, Sse},
    Json,
};
use futures::stream::StreamExt;
use serde_json::Value;
use std::convert::Infallible;
use tracing::{error, info};
use uuid::Uuid;

use crate::{
    handlers::openai_chat,
    models::openai::{ChatCompletionChunk, ChatCompletionRequest},
    openai::errors::map_error_with_status,
    services::providers::ProviderError,
    state::AppState,
};

pub fn is_openai_model(model: &str) -> bool {
    model.starts_with("gpt-") || model.starts_with("gpt-3.5") || model.starts_with("gpt-4")
}

pub async fn chat_completions(
    State(state): State<AppState>,
    Json(req): Json<ChatCompletionRequest>,
) -> axum::response::Response {
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

    let provider = match state.provider_registry.route_by_model(&req.model) {
        Some(p) => p,
        None => {
            error!("No provider found for model: {}", req.model);
            return map_error_with_status(400, &format!("Unsupported model: {}", req.model));
        }
    };

    if req.stream {
        let stream_result = provider.execute_stream(req, &state).await;

        let stream = match stream_result {
            Ok(provider_stream) => provider_stream.map(move |chunk_result| match chunk_result {
                Ok(chunk_data) => {
                    if let Some(json_data) = chunk_data.strip_prefix("data: ") {
                        let json_data = json_data.trim();
                        if json_data == "[DONE]" {
                            return Ok::<Event, Infallible>(Event::default().comment("[DONE]"));
                        }
                        match serde_json::from_str::<ChatCompletionChunk>(json_data) {
                            Ok(chunk) => match Event::default().json_data(chunk) {
                                Ok(e) => Ok::<Event, Infallible>(e),
                                Err(e) => {
                                    error!("Failed to serialize SSE chunk: {}", e);
                                    Ok::<Event, Infallible>(
                                        Event::default()
                                            .comment(format!("error: serialization failed: {}", e)),
                                    )
                                }
                            },
                            Err(_) => match serde_json::from_str::<Value>(json_data) {
                                Ok(value) => match Event::default().json_data(value) {
                                    Ok(e) => Ok::<Event, Infallible>(e),
                                    Err(e) => {
                                        error!("Failed to serialize: {}", e);
                                        Ok::<Event, Infallible>(
                                            Event::default().comment("error: serialization failed"),
                                        )
                                    }
                                },
                                Err(_) => {
                                    Ok::<Event, Infallible>(Event::default().comment(json_data))
                                }
                            },
                        }
                    } else if chunk_data.trim().is_empty() {
                        Ok::<Event, Infallible>(Event::default().comment("keep-alive"))
                    } else {
                        Ok::<Event, Infallible>(Event::default().comment(chunk_data.trim()))
                    }
                }
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
                            Event::default().comment(format!("error: stream failed: {}", e)),
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

        let duration_ms = request_start.elapsed().as_millis() as u64;
        state.metrics.record_request(true).await;
        state.metrics.record_request_duration(duration_ms).await;
        return Sse::new(stream)
            .keep_alive(axum::response::sse::KeepAlive::default())
            .into_response();
    }

    match provider.execute(req, &state).await {
        Ok(response) => {
            let duration_ms = request_start.elapsed().as_millis() as u64;
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
        ProviderError::Unavailable(msg) => {
            if msg.contains("timeout") {
                504
            } else {
                503
            }
        }
        ProviderError::InvalidRequest(_) => 400,
        ProviderError::RateLimited(_) => 429,
        ProviderError::Internal(_) => 500,
        ProviderError::CircuitOpen(_) => 503,
    }
}
