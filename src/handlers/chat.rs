use axum::{
    extract::State,
    response::{sse::Event, IntoResponse, Sse},
    Json,
};
use futures::stream::StreamExt;
use reqwest::Client;
use std::convert::Infallible;
use tracing::{error, info};
use uuid::Uuid;

use crate::{
    models::{openai::ChatCompletionRequest, vertex::GenerateContentResponse},
    services::transformer::{transform_request, transform_response, transform_stream_chunk},
    state::AppState,
};

pub async fn chat_completions(
    State(state): State<AppState>,
    Json(req): Json<ChatCompletionRequest>,
) -> Result<impl IntoResponse, (axum::http::StatusCode, String)> {
    let request_id = Uuid::new_v4().to_string();
    info!(
        "Received request: {} for model: {} (stream={})",
        request_id, req.model, req.stream
    );

    // 1. Get Access Token (or API Key)
    let token = state.token_manager.get_token().await.map_err(|e| {
        error!("Auth error: {}", e);
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to authenticate with Vertex AI".to_string(),
        )
    })?;

    // 2. Transform Request
    let vertex_req = transform_request(req.clone()).map_err(|e| {
        error!("Transform error: {}", e);
        (
            axum::http::StatusCode::BAD_REQUEST,
            "Invalid request format".to_string(),
        )
    })?;

    // 3. Prepare Vertex API Call
    let client = Client::new();
    let model = &req.model;
    let is_api_key = state.token_manager.is_api_key();

    // Determine URL based on Auth Mode
    let (base_url, _query_param) = if is_api_key {
        // Google AI Studio (Gemini API)
        // https://generativelanguage.googleapis.com/v1beta/models/gemini-pro:generateContent?key=API_KEY
        (
            format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{}",
                model
            ),
            format!("?key={}", token),
        )
    } else {
        // Vertex AI (GCP)
        // https://{region}-aiplatform.googleapis.com/v1/projects/{project}/locations/{region}/publishers/google/models/{model}:generateContent
        let project_id = state.token_manager.get_project_id().unwrap_or("unknown");
        let region = &state.config.vertex.region;
        (
            format!(
                "https://{}-aiplatform.googleapis.com/v1/projects/{}/locations/{}/publishers/google/models/{}",
                region, project_id, region, model
            ),
            "".to_string(),
        )
    };

    if req.stream {
        // Streaming Path
        // let _url = format!("{}:streamGenerateContent{}&alt=sse", base_url, query_param);
        // Note: For API Key, we append &alt=sse. For Vertex, we might need to handle it differently if it doesn't support alt=sse on that endpoint.
        // Actually, `generativelanguage` supports `alt=sse`.
        // `aiplatform` also supports `alt=sse` on `streamGenerateContent`.
        // Clean up query param logic:
        let url = if is_api_key {
            format!("{}:streamGenerateContent?key={}&alt=sse", base_url, token)
        } else {
            format!("{}:streamGenerateContent?alt=sse", base_url)
        };

        let mut req_builder = client.post(&url).json(&vertex_req);

        if !is_api_key {
            req_builder = req_builder.bearer_auth(token);
        }

        let res = req_builder.send().await.map_err(|e| {
            error!("Network error: {}", e);
            (
                axum::http::StatusCode::BAD_GATEWAY,
                "Failed to contact Vertex AI".to_string(),
            )
        })?;

        if !res.status().is_success() {
            let status = res.status();
            let text = res.text().await.unwrap_or_default();
            error!("Vertex API error: {} - {}", status, text);
            return Err((
                axum::http::StatusCode::BAD_GATEWAY,
                format!("Vertex API Error: {}", text),
            ));
        }

        let stream = res
            .bytes_stream()
            .map(move |chunk_result| match chunk_result {
                Ok(bytes) => {
                    let s = String::from_utf8_lossy(&bytes);
                    let cleaned = s
                        .trim()
                        .trim_start_matches("data: ")
                        .trim()
                        .trim_start_matches('[')
                        .trim_start_matches(',')
                        .trim_end_matches(',')
                        .trim_end_matches(']');

                    if cleaned.is_empty() {
                        return Ok::<Event, Infallible>(Event::default().comment("keep-alive"));
                    }

                    match serde_json::from_str::<GenerateContentResponse>(cleaned) {
                        Ok(vertex_res) => {
                            match transform_stream_chunk(
                                vertex_res,
                                req.model.clone(),
                                request_id.clone(),
                            ) {
                                Ok(openai_chunk) => Ok::<Event, Infallible>(
                                    Event::default().json_data(openai_chunk).unwrap(),
                                ),
                                Err(e) => {
                                    error!("Transform error: {}", e);
                                    Ok::<Event, Infallible>(
                                        Event::default().comment(format!("error: {}", e)),
                                    )
                                }
                            }
                        }
                        Err(_) => Ok::<Event, Infallible>(Event::default().comment("parse-error")),
                    }
                }
                Err(e) => {
                    error!("Stream error: {}", e);
                    Ok::<Event, Infallible>(
                        Event::default().comment(format!("stream-error: {}", e)),
                    )
                }
            });

        return Ok(Sse::new(stream)
            .keep_alive(axum::response::sse::KeepAlive::default())
            .into_response());
    }

    // Non-Streaming Path
    let url = if is_api_key {
        format!("{}:generateContent?key={}", base_url, token)
    } else {
        format!("{}:generateContent", base_url)
    };

    let mut req_builder = client.post(&url).json(&vertex_req);

    if !is_api_key {
        req_builder = req_builder.bearer_auth(token);
    }

    let res = req_builder.send().await.map_err(|e| {
        error!("Network error: {}", e);
        (
            axum::http::StatusCode::BAD_GATEWAY,
            "Failed to contact Vertex AI".to_string(),
        )
    })?;

    if !res.status().is_success() {
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        error!("Vertex API error: {} - {}", status, text);
        return Err((
            axum::http::StatusCode::BAD_GATEWAY,
            format!("Vertex API Error: {}", text),
        ));
    }

    let vertex_res: GenerateContentResponse = res.json().await.map_err(|e| {
        error!("Deserialization error: {}", e);
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to parse Vertex response".to_string(),
        )
    })?;

    let response = transform_response(vertex_res, req.model, request_id).map_err(|e| {
        error!("Response transform error: {}", e);
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to transform response".to_string(),
        )
    })?;

    Ok(Json(response).into_response())
}
