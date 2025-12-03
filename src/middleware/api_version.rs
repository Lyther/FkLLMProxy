use axum::{extract::Request, http::HeaderValue, middleware::Next, response::Response};

/// Middleware to add API-Version header to all responses
pub async fn api_version_middleware(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;

    // Add API-Version header using CARGO_PKG_VERSION
    // from_static panics at runtime if the string is invalid, not compile time
    let api_version = env!("CARGO_PKG_VERSION");
    let header_value = HeaderValue::from_static(api_version);
    response.headers_mut().insert("X-API-Version", header_value);

    response
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    #[test]
    fn test_api_version_matches_cargo_version() {
        let api_version = env!("CARGO_PKG_VERSION");
        assert!(!api_version.is_empty());
    }

    #[test]
    fn test_api_version_header_value_valid() {
        // Verify CARGO_PKG_VERSION can be converted to a valid HeaderValue
        // This will panic at runtime if invalid
        let api_version = env!("CARGO_PKG_VERSION");
        let header_value = HeaderValue::from_static(api_version);
        assert_eq!(header_value.to_str().unwrap(), api_version);
    }
}
