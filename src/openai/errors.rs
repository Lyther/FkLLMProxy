use axum::response::IntoResponse;
use serde::Serialize;
use tracing::error;

#[derive(Debug, Serialize)]
pub struct OpenAIError {
    pub error: ErrorDetail,
}

#[derive(Debug, Serialize)]
pub struct ErrorDetail {
    pub message: String,
    #[serde(rename = "type")]
    pub error_type: String,
    pub code: Option<String>,
}

pub fn map_backend_error(status: u16, message: &str) -> axum::response::Response {
    map_error_with_status(status, message)
}

pub fn map_error_with_status(status: u16, message: &str) -> axum::response::Response {
    let (error_type, code) = match status {
        400 => ("invalid_request_error", Some("invalid_request".to_string())),
        401 => ("invalid_request_error", Some("invalid_api_key".to_string())),
        403 => ("invalid_request_error", Some("forbidden".to_string())),
        404 => ("invalid_request_error", Some("not_found".to_string())),
        429 => ("rate_limit_error", Some("rate_limit_exceeded".to_string())),
        500 => ("server_error", Some("upstream_error".to_string())),
        501 => ("server_error", Some("upstream_error".to_string())),
        502 => ("server_error", Some("bad_gateway".to_string())),
        503 => ("server_error", Some("service_unavailable".to_string())),
        504 => ("server_error", Some("timeout".to_string())),
        505..=599 => ("server_error", Some("upstream_error".to_string())),
        _ => ("invalid_request_error", None),
    };

    error!("Error response: {} - {}", status, message);

    let error_response = OpenAIError {
        error: ErrorDetail {
            message: message.to_string(),
            error_type: error_type.to_string(),
            code,
        },
    };

    (
        axum::http::StatusCode::from_u16(status)
            .unwrap_or(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
        axum::Json(error_response),
    )
        .into_response()
}
