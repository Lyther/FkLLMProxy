use axum::{
    extract::State,
    response::{sse::Event, IntoResponse, Sse},
    Json,
};
use futures::stream::{self, StreamExt};
use tracing::{error, info};
use uuid::Uuid;

use crate::{
    models::openai::{ChatCompletionRequest, ChatCompletionResponse},
    openai::{
        backend::{BackendError, OpenAIBackendClient},
        errors::map_error_with_status,
        harvester::HarvesterClient,
        models::BackendConversationRequest,
        models::TokenResponse,
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
) -> Vec<Event> {
    let events = parser.parse_chunk(bytes);
    let mut sse_events = Vec::new();
    for event in events {
        if let Some(chunk) = transform_sse_to_openai_chunk(&event, model, request_id) {
            match Event::default().json_data(chunk) {
                Ok(e) => sse_events.push(e),
                Err(e) => {
                    error!("Failed to serialize SSE chunk: {}", e);
                    sse_events.push(
                        Event::default().comment(format!("error: serialization failed: {e}")),
                    );
                }
            }
        }
    }
    if sse_events.is_empty() {
        sse_events.push(Event::default().comment("keep-alive"));
    }
    sse_events
}

type HttpResponse = axum::response::Response;
type ClientTuple = (HarvesterClient, OpenAIBackendClient);

fn build_clients(state: &AppState) -> Result<ClientTuple, Box<HttpResponse>> {
    let harvester = HarvesterClient::new(&state.config)
        .map(|h| h.with_metrics(state.metrics.clone()))
        .map_err(|e| {
            error!("Failed to create harvester client: {}", e);
            Box::new(map_error_with_status(
                500,
                &format!("Failed to initialize Harvester client: {e}"),
            ))
        })?;

    let backend_client = OpenAIBackendClient::new(&state.config).map_err(|e| {
        error!("Failed to create backend client: {}", e);
        Box::new(map_error_with_status(
            500,
            &format!("Failed to initialize OpenAI client: {e}"),
        ))
    })?;

    Ok((harvester, backend_client))
}

async fn fetch_tokens(
    harvester: &HarvesterClient,
    requires_arkose: bool,
    metrics: &std::sync::Arc<crate::openai::metrics::Metrics>,
    token_start: std::time::Instant,
) -> Result<TokenResponse, axum::response::Response> {
    match harvester.get_tokens(requires_arkose).await {
        Ok(tokens) => {
            if requires_arkose && tokens.arkose_token.is_some() {
                let duration =
                    u64::try_from(token_start.elapsed().as_millis().min(u128::from(u64::MAX)))
                        .unwrap_or(u64::MAX);
                metrics.record_arkose_solve(duration).await;
            }
            Ok(tokens)
        }
        Err(e) => {
            error!("Failed to get tokens: {}", e);
            Err(map_error_with_status(
                503,
                &format!("Harvester unavailable: {e}"),
            ))
        }
    }
}

struct StreamingContext<'a> {
    backend_client: &'a OpenAIBackendClient,
    circuit_breaker: &'a std::sync::Arc<crate::openai::circuit_breaker::CircuitBreaker>,
    backend_req: BackendConversationRequest,
    tokens: &'a TokenResponse,
    metrics: &'a std::sync::Arc<crate::openai::metrics::Metrics>,
    model: &'a str,
    request_id: &'a str,
    request_start: std::time::Instant,
}

async fn handle_streaming(ctx: StreamingContext<'_>) -> axum::response::Response {
    let StreamingContext {
        backend_client,
        circuit_breaker,
        backend_req,
        tokens,
        metrics,
        model,
        request_id,
        request_start,
    } = ctx;
    let response = match execute_backend_request(
        backend_client,
        circuit_breaker,
        backend_req,
        &tokens.access_token,
        tokens.arkose_token.as_deref(),
        metrics,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            error!("Backend request failed: {}", e);
            let status = e.status_code();
            metrics.record_request(false).await;
            return map_error_with_status(status, &e.to_string());
        }
    };

    let mut parser = SSEParser::new();
    let model_clone = model.to_string();
    let request_id_clone = request_id.to_string();
    let stream = response
        .bytes_stream()
        .map(move |chunk_result| -> Vec<Result<Event, reqwest::Error>> {
            match chunk_result {
                Ok(bytes) => {
                    process_stream_chunk(&mut parser, &bytes, &model_clone, &request_id_clone)
                        .into_iter()
                        .map(Ok::<Event, reqwest::Error>)
                        .collect()
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
                    match Event::default().json_data(error_chunk) {
                        Ok(event) => vec![Ok(event)],
                        Err(serialize_err) => {
                            error!("Failed to serialize error chunk: {}", serialize_err);
                            vec![Ok(Event::default().comment(format!(
                                "error: stream failed: {e} (serialization error: {serialize_err})"
                            )))]
                        }
                    }
                }
            }
        })
        .flat_map(stream::iter);

    let duration_ms = u64::try_from(
        request_start
            .elapsed()
            .as_millis()
            .min(u128::from(u64::MAX)),
    )
    .unwrap_or(u64::MAX);
    metrics.record_request(true).await;
    metrics.record_request_duration(duration_ms).await;
    Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::default())
        .into_response()
}

struct NonStreamingContext<'a> {
    backend_client: &'a OpenAIBackendClient,
    circuit_breaker: &'a std::sync::Arc<crate::openai::circuit_breaker::CircuitBreaker>,
    backend_req: BackendConversationRequest,
    tokens: &'a TokenResponse,
    metrics: &'a std::sync::Arc<crate::openai::metrics::Metrics>,
    model: &'a str,
    request_id: &'a str,
    request_start: std::time::Instant,
}

async fn handle_non_streaming(ctx: NonStreamingContext<'_>) -> axum::response::Response {
    let NonStreamingContext {
        backend_client,
        circuit_breaker,
        backend_req,
        tokens,
        metrics,
        model,
        request_id,
        request_start,
    } = ctx;
    let response = match execute_backend_request(
        backend_client,
        circuit_breaker,
        backend_req,
        &tokens.access_token,
        tokens.arkose_token.as_deref(),
        metrics,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            error!("Backend request failed: {}", e);
            let status = e.status_code();
            metrics.record_request(false).await;
            return map_error_with_status(status, &e.to_string());
        }
    };

    let (full_content, finish_reason) =
        match collect_stream_response(response, model, request_id).await {
            Ok((content, reason)) => (content, reason),
            Err(e) => {
                error!("Stream error during collection: {}", e);
                metrics.record_request(false).await;
                return map_error_with_status(502, &format!("Stream error: {e}"));
            }
        };

    let created = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let response = ChatCompletionResponse {
        id: format!("chatcmpl-{request_id}"),
        object: "chat.completion".to_string(),
        created,
        model: model.to_string(),
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

    let duration_ms = u64::try_from(
        request_start
            .elapsed()
            .as_millis()
            .min(u128::from(u64::MAX)),
    )
    .unwrap_or(u64::MAX);
    metrics.record_request(true).await;
    metrics.record_request_duration(duration_ms).await;
    Json(response).into_response()
}

pub async fn openai_chat_completions(
    State(state): State<AppState>,
    Json(req): Json<ChatCompletionRequest>,
) -> axum::response::Response {
    // Validate request
    if let Err(e) = req.validate() {
        error!("Invalid request: {}", e);
        return map_error_with_status(400, &format!("Invalid request: {e}"));
    }

    let request_start = std::time::Instant::now();
    let request_id = Uuid::new_v4().to_string();
    let span = tracing::span!(tracing::Level::INFO, "openai_chat_completions", request_id = %request_id, model = %req.model, stream = req.stream);
    let _guard = span.enter();
    info!(
        "Received OpenAI request: {} for model: {} (stream={})",
        request_id, req.model, req.stream
    );

    let (harvester, backend_client) = match build_clients(&state) {
        Ok(clients) => clients,
        Err(resp) => return *resp,
    };

    let requires_arkose = req.model.starts_with("gpt-4");
    let token_start = std::time::Instant::now();
    let tokens = match fetch_tokens(&harvester, requires_arkose, &state.metrics, token_start).await
    {
        Ok(tokens) => tokens,
        Err(resp) => return resp,
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
            return map_error_with_status(400, &format!("Invalid request format: {e}"));
        }
    };

    if req.stream {
        return handle_streaming(StreamingContext {
            backend_client: &backend_client,
            circuit_breaker: &state.circuit_breaker,
            backend_req,
            tokens: &tokens,
            metrics: &state.metrics,
            model: &req.model,
            request_id: &request_id,
            request_start,
        })
        .await;
    }

    handle_non_streaming(NonStreamingContext {
        backend_client: &backend_client,
        circuit_breaker: &state.circuit_breaker,
        backend_req,
        tokens: &tokens,
        metrics: &state.metrics,
        model: &req.model,
        request_id: &request_id,
        request_start,
    })
    .await
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_stream_chunk_handles_multiple_events_in_single_chunk() {
        let mut parser = SSEParser::new();
        let chunk = b"data: {\"message\":{\"id\":\"msg_1\",\"content\":{\"content_type\":\"text\",\"parts\":[\"hello\"]}}}\n\ndata: [DONE]\n\n";

        let events = process_stream_chunk(&mut parser, chunk, "gpt-4", "req-1");

        assert_eq!(
            events.len(),
            2,
            "should emit both the message event and the [DONE] event"
        );
    }
}
