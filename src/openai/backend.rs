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

pub struct OpenAIBackendClient {
    client: Client,
    base_url: String,
}

impl OpenAIBackendClient {
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

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(CLIENT_TIMEOUT_SECS))
            .user_agent(DEFAULT_USER_AGENT)
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self { client, base_url })
    }

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

        let mut req_builder = self
            .client
            .post(&self.base_url)
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .header("User-Agent", DEFAULT_USER_AGENT)
            .header("Accept-Language", "en-US,en;q=0.9")
            .header("Referer", "https://chatgpt.com/")
            .header("Authorization", format!("Bearer {}", access_token))
            .json(&request);

        if let Some(arkose) = arkose_token {
            // Validate arkose token format
            if !arkose.is_empty() && !arkose.contains('\n') && !arkose.contains('\r') {
                req_builder = req_builder.header("Openai-Sentinel-Arkose-Token", arkose);
            }
        }

        let response = req_builder.send().await.map_err(BackendError::Network)?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
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

        Ok(response)
    }
}
