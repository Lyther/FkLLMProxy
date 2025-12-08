use crate::config::AppConfig;
use crate::openai::models::BackendConversationRequest;
use anyhow::{Context, Result};
use reqwest::Client;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tracing::warn;

#[derive(Debug, Error)]
pub enum BackendError {
    #[error("Authentication failed - token may be expired: {0}")]
    Auth(String),
    #[error("WAF blocked - TLS fingerprint may be detected: {0}")]
    WafBlocked(String),
    #[error("Rate limit exceeded: {0}")]
    RateLimited(String),
    #[error("Backend error (status {0}): {1}")]
    HttpError(u16, String),
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("Circuit breaker is open")]
    CircuitOpen(#[from] crate::openai::circuit_breaker::CircuitOpenError),
}

impl BackendError {
    #[must_use]
    pub fn status_code(&self) -> u16 {
        match self {
            BackendError::Auth(_) => 401,
            BackendError::WafBlocked(_) => 403,
            BackendError::RateLimited(_) => 429,
            BackendError::HttpError(status, _) => *status,
            BackendError::Network(_) => 502,
            BackendError::CircuitOpen(_) => 503,
        }
    }
}

const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
const DEFAULT_BASE_URL: &str = "https://chatgpt.com/backend-api/conversation";
const CLIENT_TIMEOUT_SECS: u64 = 60;
const REQUEST_TIMEOUT_SECS: u64 = 30;
const RETRY_ATTEMPTS: u32 = 3;
const RETRY_BACKOFF_MS: u64 = 500;

fn calculate_backoff_ms(attempt: u32) -> u64 {
    // Exponential backoff: RETRY_BACKOFF_MS * 2^(attempt-1)
    RETRY_BACKOFF_MS * (1 << u64::from(attempt.saturating_sub(1)))
}

pub struct OpenAIBackendClient {
    client: Client,
    base_url: String,
    user_agent: String,
}

impl OpenAIBackendClient {
    /// Creates a new `OpenAI` backend client.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be created.
    pub fn new(config: &Arc<AppConfig>) -> Result<Self> {
        // Use config for base_url/user_agent if provided, otherwise use defaults
        let base_url = config
            .openai
            .harvester_url
            .replace("/v1/tokens", "/backend-api/conversation")
            .replace(":3001", "")
            .replace("http://", "https://");

        let base_url = if base_url.contains("backend-api") {
            base_url
        } else {
            DEFAULT_BASE_URL.to_string()
        };

        // Fix hardcoded values: Make user agent configurable via env var
        let user_agent =
            std::env::var("BACKEND_USER_AGENT").unwrap_or_else(|_| DEFAULT_USER_AGENT.to_string());

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(CLIENT_TIMEOUT_SECS))
            .user_agent(&user_agent)
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            base_url,
            user_agent,
        })
    }

    /// Sends a request to the `OpenAI` backend.
    ///
    /// # Errors
    ///
    /// Returns a [`BackendError`] if:
    /// - The access token is invalid or malformed
    /// - The request is blocked by WAF
    /// - Rate limiting is triggered
    /// - Network errors occur
    /// - The circuit breaker is open
    /// - The backend returns an HTTP error
    pub async fn send_request(
        &self,
        request: BackendConversationRequest,
        access_token: &str,
        arkose_token: Option<&str>,
    ) -> Result<reqwest::Response, BackendError> {
        // Validate token format - should be non-empty and not contain newlines
        if access_token.is_empty() || access_token.contains('\n') || access_token.contains('\r') {
            return Err(BackendError::Auth(
                "Invalid access token format".to_string(),
            ));
        }

        // Fix: Add retry logic with exponential backoff for transient failures
        for attempt in 1..=RETRY_ATTEMPTS {
            let mut req_builder = self
                .client
                .post(&self.base_url)
                .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
                .header("User-Agent", &self.user_agent)
                .header("Accept-Language", "en-US,en;q=0.9")
                .header("Referer", "https://chatgpt.com/")
                .header("Authorization", format!("Bearer {access_token}"))
                .json(&request);

            if let Some(arkose) = arkose_token {
                // Validate arkose token format
                if !arkose.is_empty() && !arkose.contains('\n') && !arkose.contains('\r') {
                    req_builder = req_builder.header("Openai-Sentinel-Arkose-Token", arkose);
                }
            }

            let response = match req_builder.send().await {
                Ok(r) => r,
                Err(e) => {
                    // Network errors are retryable
                    if attempt == RETRY_ATTEMPTS {
                        return Err(BackendError::Network(e));
                    }
                    tokio::time::sleep(Duration::from_millis(calculate_backoff_ms(attempt))).await;
                    continue;
                }
            };

            if !response.status().is_success() {
                let status = response.status().as_u16();

                // Don't retry on auth/WAF/rate limit errors (4xx)
                if (400..500).contains(&status) {
                    let text = match response.text().await {
                        Ok(t) => t,
                        Err(e) => {
                            warn!("Failed to read error response body: {}", e);
                            String::new()
                        }
                    };

                    return Err(match status {
                        401 => BackendError::Auth(text),
                        403 => BackendError::WafBlocked(text),
                        429 => BackendError::RateLimited(text),
                        _ => BackendError::HttpError(status, text),
                    });
                }

                // Retry on 5xx errors
                if attempt == RETRY_ATTEMPTS {
                    let text = match response.text().await {
                        Ok(t) => t,
                        Err(e) => {
                            warn!("Failed to read error response body: {}", e);
                            String::new()
                        }
                    };
                    return Err(BackendError::HttpError(status, text));
                }
                tokio::time::sleep(Duration::from_millis(calculate_backoff_ms(attempt))).await;
                continue;
            }

            return Ok(response);
        }

        // Unreachable, but required by type system
        unreachable!("Retry loop exhausted without returning")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_error_status_code() {
        assert_eq!(BackendError::Auth("test".to_string()).status_code(), 401);
        assert_eq!(
            BackendError::WafBlocked("test".to_string()).status_code(),
            403
        );
        assert_eq!(
            BackendError::RateLimited("test".to_string()).status_code(),
            429
        );
        assert_eq!(
            BackendError::HttpError(500, "test".to_string()).status_code(),
            500
        );
        assert_eq!(
            BackendError::HttpError(503, "test".to_string()).status_code(),
            503
        );
    }

    #[test]
    fn test_calculate_backoff_exponential() {
        // Attempt 1: 500 * 2^0 = 500
        assert_eq!(calculate_backoff_ms(1), 500);
        // Attempt 2: 500 * 2^1 = 1000
        assert_eq!(calculate_backoff_ms(2), 1000);
        // Attempt 3: 500 * 2^2 = 2000
        assert_eq!(calculate_backoff_ms(3), 2000);
        // Edge case: Attempt 0 should be handled safely (saturating_sub)
        assert_eq!(calculate_backoff_ms(0), 500);
    }

    #[tokio::test]
    async fn test_empty_access_token_rejected() {
        // Skip if config cannot be loaded
        let config = match AppConfig::new() {
            Ok(c) => Arc::new(c),
            Err(_) => return,
        };
        let Ok(client) = OpenAIBackendClient::new(&config) else {
            return;
        };

        let request = BackendConversationRequest {
            action: "next".to_string(),
            messages: vec![],
            model: "test".to_string(),
            parent_message_id: Some("test".to_string()),
            conversation_id: None,
            temperature: None,
            max_tokens: None,
        };

        let result = client.send_request(request.clone(), "", None).await;
        assert!(matches!(result, Err(BackendError::Auth(_))));
    }

    #[tokio::test]
    async fn test_newline_in_token_rejected() {
        let config = match AppConfig::new() {
            Ok(c) => Arc::new(c),
            Err(_) => return,
        };
        let Ok(client) = OpenAIBackendClient::new(&config) else {
            return;
        };

        let request = BackendConversationRequest {
            action: "next".to_string(),
            messages: vec![],
            model: "test".to_string(),
            parent_message_id: Some("test".to_string()),
            conversation_id: None,
            temperature: None,
            max_tokens: None,
        };

        let result = client
            .send_request(request.clone(), "token\nwith\nnewlines", None)
            .await;
        assert!(matches!(result, Err(BackendError::Auth(_))));

        let result = client
            .send_request(request.clone(), "token\rwith\rcarriage", None)
            .await;
        assert!(matches!(result, Err(BackendError::Auth(_))));
    }

    #[test]
    fn test_default_constants() {
        // Test non-constant values
        assert!(!DEFAULT_USER_AGENT.is_empty());
        assert!(DEFAULT_BASE_URL.starts_with("https://"));

        // Constants are guaranteed by their definitions:
        // CLIENT_TIMEOUT_SECS = 60, REQUEST_TIMEOUT_SECS = 30, RETRY_ATTEMPTS = 3
    }
}
