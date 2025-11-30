use axum::{extract::Request, http::HeaderValue, middleware::Next, response::Response};

/// Middleware to add security headers to all responses
pub async fn security_headers_middleware(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;

    // Content-Security-Policy: Default to 'self' only, no inline scripts
    if let Ok(header_value) =
        HeaderValue::from_str("default-src 'self'; script-src 'self'; object-src 'none';")
    {
        response
            .headers_mut()
            .insert("Content-Security-Policy", header_value);
    }

    // Strict-Transport-Security: Enforce HTTPS (1 year max-age)
    if let Ok(header_value) = HeaderValue::from_str("max-age=31536000; includeSubDomains") {
        response
            .headers_mut()
            .insert("Strict-Transport-Security", header_value);
    }

    // X-Frame-Options: Prevent clickjacking
    response
        .headers_mut()
        .insert("X-Frame-Options", HeaderValue::from_static("DENY"));

    // X-Content-Type-Options: Prevent MIME sniffing
    response.headers_mut().insert(
        "X-Content-Type-Options",
        HeaderValue::from_static("nosniff"),
    );

    // X-XSS-Protection: Legacy, but still useful for older browsers
    response.headers_mut().insert(
        "X-XSS-Protection",
        HeaderValue::from_static("1; mode=block"),
    );

    // Referrer-Policy: Control referrer information
    if let Ok(header_value) = HeaderValue::from_str("strict-origin-when-cross-origin") {
        response
            .headers_mut()
            .insert("Referrer-Policy", header_value);
    }

    // Permissions-Policy: Restrict browser features
    if let Ok(header_value) = HeaderValue::from_str("geolocation=(), microphone=(), camera=()") {
        response
            .headers_mut()
            .insert("Permissions-Policy", header_value);
    }

    response
}
