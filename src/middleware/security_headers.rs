use axum::{extract::Request, http::HeaderValue, middleware::Next, response::Response};
use tracing::warn;

/// Middleware to add security headers to all responses
pub async fn security_headers_middleware(request: Request, next: Next) -> Response {
    // Strict-Transport-Security: Only set if HTTPS is detected
    // Check if request was made over HTTPS (via X-Forwarded-Proto or scheme)
    let is_https = if let Some(proto) = request
        .headers()
        .get("x-forwarded-proto")
        .and_then(|h| h.to_str().ok())
    {
        proto == "https"
    } else if let Some(scheme) = request.uri().scheme() {
        scheme.as_str() == "https"
    } else {
        false
    };

    let mut response = next.run(request).await;

    // Content-Security-Policy: For API endpoints, use default-src 'none' to block all resources
    // API endpoints typically don't need CSP, but if set, should be restrictive
    match HeaderValue::from_str("default-src 'none'") {
        Ok(header_value) => {
            response
                .headers_mut()
                .insert("Content-Security-Policy", header_value);
        }
        Err(e) => {
            warn!("Failed to create CSP header value: {}", e);
        }
    }

    if is_https {
        match HeaderValue::from_str("max-age=31536000; includeSubDomains") {
            Ok(header_value) => {
                response
                    .headers_mut()
                    .insert("Strict-Transport-Security", header_value);
            }
            Err(e) => {
                warn!("Failed to create HSTS header value: {}", e);
            }
        }
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

    // X-XSS-Protection: Deprecated header - removed per modern security best practices
    // Modern browsers ignore this header. CSP provides better protection.

    // Referrer-Policy: Control referrer information
    match HeaderValue::from_str("strict-origin-when-cross-origin") {
        Ok(header_value) => {
            response
                .headers_mut()
                .insert("Referrer-Policy", header_value);
        }
        Err(e) => {
            warn!("Failed to create Referrer-Policy header value: {}", e);
        }
    }

    // Permissions-Policy: Restrict browser features
    // Validate format: geolocation=(), microphone=(), camera=() is valid
    match HeaderValue::from_str("geolocation=(), microphone=(), camera=()") {
        Ok(header_value) => {
            response
                .headers_mut()
                .insert("Permissions-Policy", header_value);
        }
        Err(e) => {
            warn!("Failed to create Permissions-Policy header value: {}", e);
        }
    }

    response
}
