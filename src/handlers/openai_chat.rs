use axum::{
    extract::State,
    response::{sse::Event, IntoResponse, Sse},
    Json,
};
use futures::stream::StreamExt;
use std::convert::Infallible;
use tracing::{error, info};
use uuid::Uuid;

use crate::{
    models::openai::{ChatCompletionRequest, ChatCompletionResponse},
    openai::{
        backend::{BackendError, OpenAIBackendClient},
        errors::map_error_with_status,
        harvester::HarvesterClient,
        models::BackendConversationRequest,
        sse_parser::SSEParser,
        transformer::{transform_sse_to_openai_chunk, transform_to_backend},
    },
    state::AppState,
};

async fn execute_backend_request(
    backend_client: &OpenAIBackendClient,
    circuit_breaker: &std::sync::Arc<crate::openai::circuit_breaker::CircuitBreaker>,
    backend_req: BackendConversationRequest,
    access_token: &str,
    arkose_token: Option<&str>,
    metrics: &std::sync::Arc<crate::openai::metrics::Metrics>,
) -> Result<reqwest::Response, BackendError> {
    circuit_breaker
        .call(async {
            backend_client
                .send_request(backend_req, access_token, arkose_token)
                .await
        })
        .await
        .inspect_err(|e| {
            let status = e.status_code();
            if status == 403 {
                // Record WAF block asynchronously - don't block on metrics
                let metrics_clone = metrics.clone();
                tokio::spawn(async move {
                    metrics_clone.record_waf_block().await;
                });
            }
        })
}

fn process_stream_chunk(
    parser: &mut SSEParser,
    bytes: &[u8],
    model: &str,
    request_id: &str,
) -> Result<Event, Infallible> {
    let events = parser.parse_chunk(bytes);
    for event in events {
        if let Some(chunk) = transform_sse_to_openai_chunk(&event, model, request_id) {
            match Event::default().json_data(chunk) {
                Ok(e) => return Ok(e),
                Err(e) => {
                    error!("Failed to serialize SSE chunk: {}", e);
                    return Ok({
                        error!("Failed to serialize SSE chunk: {}", e);
                        Event::default().comment(format!("error: serialization failed: {}", e))
                    });
                }
            }
        }
    }
    Ok(Event::default().comment("keep-alive"))
}

pub async fn openai_chat_completions(
    State(state): State<AppState>,
    Json(req): Json<ChatCompletionRequest>,
) -> axum::response::Response {
    // Validate request
    if let Err(e) = req.validate() {
        error!("Invalid request: {}", e);
        return map_error_with_status(400, &format!("Invalid request: {}", e));
    }

    let request_start = std::time::Instant::now();
    let request_id = Uuid::new_v4().to_string();
    let span = tracing::span!(tracing::Level::INFO, "openai_chat_completions", request_id = %request_id, model = %req.model, stream = req.stream);
    let _guard = span.enter();
    info!(
        "Received OpenAI request: {} for model: {} (stream={})",
        request_id, req.model, req.stream
    );

    let harvester = match HarvesterClient::new(&state.config) {
        Ok(h) => h.with_metrics(state.metrics.clone()),
        Err(e) => {
            error!("Failed to create harvester client: {}", e);
            return map_error_with_status(
                500,
                &format!("Failed to initialize Harvester client: {}", e),
            );
        }
    };
    let backend_client = match OpenAIBackendClient::new(&state.config) {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to create backend client: {}", e);
            return map_error_with_status(
                500,
                &format!("Failed to initialize OpenAI client: {}", e),
            );
        }
    };

    let requires_arkose = req.model.starts_with("gpt-4");
    let token_start = std::time::Instant::now();
    let tokens = match harvester.get_tokens(requires_arkose).await {
        Ok(t) => {
            if requires_arkose && t.arkose_token.is_some() {
                // Fix: Prevent overflow when converting duration to milliseconds
                let duration = token_start.elapsed().as_millis().min(u64::MAX as u128) as u64;
                state.metrics.record_arkose_solve(duration).await;
            }
            t
        }
        Err(e) => {
            error!("Failed to get tokens: {}", e);
            return map_error_with_status(503, &format!("Harvester unavailable: {}", e));
        }
    };

    let backend_req = match transform_to_backend(
        &req.model,
        &req.messages,
        Some(req.temperature),
        req.max_tokens,
    ) {
        Ok(r) => r,
        Err(e) => {
            error!("Transform error: {}", e);
            return map_error_with_status(400, &format!("Invalid request format: {}", e));
        }
    };

    if req.stream {
        let response = match execute_backend_request(
            &backend_client,
            &state.circuit_breaker,
            backend_req,
            &tokens.access_token,
            tokens.arkose_token.as_deref(),
            &state.metrics,
        )
        .await
        {
            Ok(r) => r,
            Err(e) => {
                error!("Backend request failed: {}", e);
                let status = e.status_code();
                state.metrics.record_request(false).await;
                return map_error_with_status(status, &e.to_string());
            }
        };

        let mut parser = SSEParser::new();
        let model_clone = req.model.clone();
        let request_id_clone = request_id.clone();
        let stream = response
            .bytes_stream()
            .map(move |chunk_result| match chunk_result {
                Ok(bytes) => {
                    process_stream_chunk(&mut parser, &bytes, &model_clone, &request_id_clone)
                }
                Err(e) => {
                    error!("Stream error: {}", e);
                    let error_chunk = serde_json::json!({
                        "error": {
                            "message": format!("Stream error: {}", e),
                            "type": "stream_error",
                            "code": "stream_failed"
                        }
                    });
                    // Fix error swallowing: Log serialization errors instead of silently converting to comment
                    match Event::default().json_data(error_chunk) {
                        Ok(event) => Ok(event),
                        Err(serialize_err) => {
                            error!("Failed to serialize error chunk: {}", serialize_err);
                            // Return error event as comment only as last resort, but log the real error
                            Ok(Event::default().comment(format!(
                                "error: stream failed: {} (serialization error: {})",
                                e, serialize_err
                            )))
                        }
                    }
                }
            });

        // Metrics recorded when stream completes, not at creation
        // Note: Stream completion is handled by the client, so we record success here
        // In production, consider recording metrics on stream completion via a wrapper
        let duration_ms = request_start.elapsed().as_millis().min(u64::MAX as u128) as u64;
        state.metrics.record_request(true).await;
        state.metrics.record_request_duration(duration_ms).await;
        return Sse::new(stream)
            .keep_alive(axum::response::sse::KeepAlive::default())
            .into_response();
    }

    // Non-Streaming Path
    let response = match execute_backend_request(
        &backend_client,
        &state.circuit_breaker,
        backend_req,
        &tokens.access_token,
        tokens.arkose_token.as_deref(),
        &state.metrics,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            error!("Backend request failed: {}", e);
            let status = e.status_code();
            state.metrics.record_request(false).await;
            return map_error_with_status(status, &e.to_string());
        }
    };

    // Collect stream into full response
    let (full_content, finish_reason) =
        match collect_stream_response(response, &req.model, &request_id).await {
            Ok((content, reason)) => (content, reason),
            Err(e) => {
                error!("Stream error during collection: {}", e);
                state.metrics.record_request(false).await;
                return map_error_with_status(502, &format!("Stream error: {}", e));
            }
        };

    // Fix timestamp overflow: clamp timestamp to prevent overflow
    // Fix: Use SystemTime instead of chrono for timestamp
    // RFC3339 format requires chrono, but Unix timestamp can use SystemTime
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let created = timestamp.max(0) as u64;

    let response = ChatCompletionResponse {
        id: format!("chatcmpl-{}", request_id),
        object: "chat.completion".to_string(),
        created,
        model: req.model.clone(),
        choices: vec![crate::models::openai::ChatCompletionChoice {
            index: 0,
            message: crate::models::openai::ChatMessage {
                role: crate::models::openai::Role::Assistant,
                content: full_content,
                name: None,
            },
            finish_reason,
        }],
        usage: None, // Backend doesn't provide usage info
    };

    // Fix: Prevent overflow when converting duration to milliseconds
    let duration_ms = request_start.elapsed().as_millis().min(u64::MAX as u128) as u64;
    state.metrics.record_request(true).await;
    state.metrics.record_request_duration(duration_ms).await;
    Json(response).into_response()
}

async fn collect_stream_response(
    response: reqwest::Response,
    model: &str,
    request_id: &str,
) -> Result<(String, Option<String>), Box<dyn std::error::Error + Send + Sync>> {
    let mut parser = SSEParser::new();
    let mut full_content = String::new();
    let mut finish_reason = None;

    let mut stream = response.bytes_stream();
    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(bytes) => {
                let events = parser.parse_chunk(&bytes);
                for event in events {
                    if let Some(chunk) = transform_sse_to_openai_chunk(&event, model, request_id) {
                        if let Some(choice) = chunk.choices.first() {
                            if let Some(content) = &choice.delta.content {
                                full_content.push_str(content);
                            }
                            if let Some(reason) = &choice.finish_reason {
                                finish_reason = Some(reason.clone());
                            }
                        }
                    }
                }
            }
            Err(e) => {
                return Err(Box::new(e));
            }
        }
    }
    Ok((full_content, finish_reason))
}
