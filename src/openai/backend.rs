use crate::config::AppConfig;
use crate::openai::models::BackendConversationRequest;
use anyhow::{Context, Result};
use reqwest::Client;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

pub struct OpenAIBackendClient {
    client: Client,
    base_url: String,
    #[allow(dead_code)] // Stored for future TLS fingerprinting implementation
    tls_fingerprint_enabled: bool,
    #[allow(dead_code)] // Stored for future TLS fingerprinting implementation
    tls_fingerprint_target: String,
}

impl OpenAIBackendClient {
    pub fn new(config: &Arc<AppConfig>) -> Result<Self> {
        let tls_fingerprint_enabled = config.openai.tls_fingerprint_enabled;
        let tls_fingerprint_target = config.openai.tls_fingerprint_target.clone();

        if tls_fingerprint_enabled {
            warn!(
                "TLS fingerprinting enabled (target: {}), but reqwest-impersonate not yet implemented. \
                Requests may still be blocked by WAF. See docs/dev/adr/005-tls-fingerprinting.md",
                tls_fingerprint_target
            );
        } else {
            info!(
                "TLS fingerprinting disabled. OpenAI requests may be blocked by Cloudflare WAF. \
                Enable with APP_OPENAI__TLS_FINGERPRINT_ENABLED=true"
            );
        }

        // Build client with enhanced TLS configuration
        // Note: Full TLS fingerprinting requires reqwest-impersonate or similar library
        // For now, we configure the client with browser-like settings
        let client_builder = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .danger_accept_invalid_certs(false); // Use proper TLS validation

        // Configure TLS to match target browser fingerprint
        // This is a placeholder - full implementation requires reqwest-impersonate
        match tls_fingerprint_target.as_str() {
            "chrome120" | "chrome" => {
                // Chrome 120 TLS settings would go here
                // Currently using default rustls which may not match Chrome exactly
                info!("TLS fingerprint target: Chrome 120 (not yet fully implemented)");
            }
            "firefox120" | "firefox" => {
                // Firefox 120 TLS settings would go here
                info!("TLS fingerprint target: Firefox 120 (not yet fully implemented)");
            }
            _ => {
                warn!("Unknown TLS fingerprint target: {}", tls_fingerprint_target);
            }
        }

        let client = client_builder
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            base_url: "https://chatgpt.com/backend-api/conversation".to_string(),
            tls_fingerprint_enabled,
            tls_fingerprint_target,
        })
    }

    pub async fn send_request(
        &self,
        request: BackendConversationRequest,
        access_token: &str,
        arkose_token: Option<&str>,
    ) -> Result<reqwest::Response> {
        let mut req_builder = self
            .client
            .post(&self.base_url)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .header("Accept-Language", "en-US,en;q=0.9")
            .header("Referer", "https://chatgpt.com/")
            .header("Authorization", format!("Bearer {}", access_token))
            .json(&request);

        if let Some(arkose) = arkose_token {
            req_builder = req_builder.header("Openai-Sentinel-Arkose-Token", arkose);
        }

        let response = req_builder
            .send()
            .await
            .context("Failed to send request to OpenAI backend")?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();

            let error_msg = match status {
                401 => "Authentication failed - token may be expired",
                403 => "WAF blocked - TLS fingerprint may be detected",
                429 => "Rate limit exceeded",
                _ => "Backend error",
            };

            anyhow::bail!("{}: {}", error_msg, text);
        }

        Ok(response)
    }
}
