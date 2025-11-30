use axum::{extract::Request, http::HeaderValue, middleware::Next, response::Response};

const API_VERSION: &str = "1.0.0";

/// Middleware to add API-Version header to all responses
pub async fn api_version_middleware(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;

    // Add API-Version header
    // Using from_static since API_VERSION is a compile-time constant
    // This will panic at compile time if the string is invalid, which is correct behavior
    let header_value = HeaderValue::from_static(API_VERSION);
    response.headers_mut().insert("API-Version", header_value);

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn test_api_version_constant() {
        assert_eq!(API_VERSION, "1.0.0");
    }

    #[test]
    fn test_api_version_header_value_valid() {
        // Verify the constant can be converted to a valid HeaderValue
        // This will panic at compile time if invalid, which is correct
        let header_value = HeaderValue::from_static(API_VERSION);
        assert_eq!(header_value.to_str().unwrap(), "1.0.0");
    }
}
