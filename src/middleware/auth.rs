use crate::state::AppState;
use axum::{
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use tracing::warn;

pub async fn auth_middleware(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // 1. Check if auth is enabled
    if !state.config.auth.require_auth {
        return Ok(next.run(req).await);
    }

    // 2. Extract Authorization header
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // 3. Validate Bearer token
    if !auth_header.starts_with("Bearer ") {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let token = &auth_header[7..];

    // 4. Check against master key
    if token != state.config.auth.master_key {
        warn!("Invalid API Key attempt: {}", token);
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(next.run(req).await)
}
