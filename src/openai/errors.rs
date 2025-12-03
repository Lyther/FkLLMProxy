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

// Removed wrapper function - use map_error_with_status directly

pub fn map_error_with_status(status: u16, message: &str) -> axum::response::Response {
    // Sanitize message to prevent injection in error responses
    let sanitized_message = message
        .chars()
        .take(1000) // Limit length
        .filter(|c| {
            c.is_ascii() || c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.' | ':' | ',')
        })
        .collect::<String>();

    let (error_type, code) = match status {
        400 => ("invalid_request_error", Some("invalid_request".to_string())),
        401 => ("authentication_error", Some("invalid_api_key".to_string())),
        403 => ("authentication_error", Some("forbidden".to_string())),
        404 => ("invalid_request_error", Some("not_found".to_string())),
        429 => ("rate_limit_error", Some("rate_limit_exceeded".to_string())),
        500 => ("server_error", Some("upstream_error".to_string())),
        501 => ("server_error", Some("upstream_error".to_string())),
        502 => ("server_error", Some("bad_gateway".to_string())),
        503 => ("server_error", Some("service_unavailable".to_string())),
        504 => ("server_error", Some("timeout".to_string())),
        505..=599 => ("server_error", Some("upstream_error".to_string())),
        _ => {
            if !(100..=599).contains(&status) {
                tracing::warn!(
                    "Invalid status code {} used for error response (must be 100-599)",
                    status
                );
            }
            ("invalid_request_error", None)
        }
    };

    error!("Error response: {} - {}", status, sanitized_message);

    let error_response = OpenAIError {
        error: ErrorDetail {
            message: sanitized_message,
            error_type: error_type.to_string(),
            code,
        },
    };

    let status_code = match axum::http::StatusCode::from_u16(status) {
        Ok(code) => code,
        Err(e) => {
            tracing::warn!("Failed to convert status code {}: {}", status, e);
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        }
    };

    (status_code, axum::Json(error_response)).into_response()
}
