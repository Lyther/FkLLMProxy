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
        backend::OpenAIBackendClient,
        errors::map_backend_error,
        harvester::HarvesterClient,
        sse_parser::SSEParser,
        transformer::{transform_sse_to_openai_chunk, transform_to_backend},
    },
    state::AppState,
};

pub async fn openai_chat_completions(
    State(state): State<AppState>,
    Json(req): Json<ChatCompletionRequest>,
) -> axum::response::Response {
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
            return map_backend_error(
                500,
                &format!("Failed to initialize Harvester client: {}", e),
            );
        }
    };
    let backend_client = match OpenAIBackendClient::new(&state.config) {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to create backend client: {}", e);
            return map_backend_error(500, &format!("Failed to initialize OpenAI client: {}", e));
        }
    };

    let requires_arkose = req.model.starts_with("gpt-4");
    let token_start = std::time::Instant::now();
    let tokens = match harvester.get_tokens(requires_arkose).await {
        Ok(t) => {
            if requires_arkose && t.arkose_token.is_some() {
                let duration = token_start.elapsed().as_millis() as u64;
                state.metrics.record_arkose_solve(duration).await;
            }
            t
        }
        Err(e) => {
            error!("Failed to get tokens: {}", e);
            return map_backend_error(503, &format!("Harvester unavailable: {}", e));
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
            return map_backend_error(400, &format!("Invalid request format: {}", e));
        }
    };

    if req.stream {
        let response = match state
            .circuit_breaker
            .call(async {
                backend_client
                    .send_request(
                        backend_req,
                        &tokens.access_token,
                        tokens.arkose_token.as_deref(),
                    )
                    .await
            })
            .await
        {
            Ok(r) => r,
            Err(e) => {
                error!("Backend request failed: {}", e);
                let status = if e.to_string().contains("401") {
                    401
                } else if e.to_string().contains("403") {
                    state.metrics.record_waf_block().await;
                    403
                } else if e.to_string().contains("429") {
                    429
                } else {
                    502
                };
                state.metrics.record_request(false).await;
                return map_backend_error(status, &e.to_string());
            }
        };

        let mut parser = SSEParser::new();
        let stream = response
            .bytes_stream()
            .map(move |chunk_result| match chunk_result {
                Ok(bytes) => {
                    let events = parser.parse_chunk(&bytes);

                    for event in events {
                        if let Some(chunk) =
                            transform_sse_to_openai_chunk(&event, &req.model, &request_id)
                        {
                            match Event::default().json_data(chunk) {
                                Ok(e) => return Ok::<Event, Infallible>(e),
                                Err(e) => {
                                    error!("Failed to serialize SSE chunk: {}", e);
                                    return Ok::<Event, Infallible>(
                                        Event::default()
                                            .comment(format!("error: serialization failed: {}", e)),
                                    );
                                }
                            }
                        }
                    }

                    Ok::<Event, Infallible>(Event::default().comment("keep-alive"))
                }
                Err(e) => {
                    error!("Stream error: {}", e);
                    Ok::<Event, Infallible>(
                        Event::default().comment(format!("stream-error: {}", e)),
                    )
                }
            });

        let duration_ms = request_start.elapsed().as_millis() as u64;
        state.metrics.record_request(true).await;
        state.metrics.record_request_duration(duration_ms).await;
        return Sse::new(stream)
            .keep_alive(axum::response::sse::KeepAlive::default())
            .into_response();
    }

    // Non-Streaming Path
    let response = match state
        .circuit_breaker
        .call(async {
            backend_client
                .send_request(
                    backend_req,
                    &tokens.access_token,
                    tokens.arkose_token.as_deref(),
                )
                .await
        })
        .await
    {
        Ok(r) => r,
        Err(e) => {
            error!("Backend request failed: {}", e);
            let status = if e.to_string().contains("401") {
                401
            } else if e.to_string().contains("403") {
                state.metrics.record_waf_block().await;
                403
            } else if e.to_string().contains("429") {
                429
            } else {
                502
            };
            state.metrics.record_request(false).await;
            return map_backend_error(status, &e.to_string());
        }
    };

    // Collect stream into full response
    let mut parser = SSEParser::new();
    let mut full_content = String::new();
    let mut finish_reason = None;

    let mut stream = response.bytes_stream();
    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(bytes) => {
                let events = parser.parse_chunk(&bytes);
                for event in events {
                    if let Some(chunk) =
                        transform_sse_to_openai_chunk(&event, &req.model, &request_id)
                    {
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
                error!("Stream error during collection: {}", e);
                state.metrics.record_request(false).await;
                return map_backend_error(502, &format!("Stream error: {}", e));
            }
        }
    }

    let response = ChatCompletionResponse {
        id: format!("chatcmpl-{}", request_id),
        object: "chat.completion".to_string(),
        created: chrono::Utc::now().timestamp() as u64,
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

    let duration_ms = request_start.elapsed().as_millis() as u64;
    state.metrics.record_request(true).await;
    state.metrics.record_request_duration(duration_ms).await;
    Json(response).into_response()
}
