use crate::config::AppConfig;
use crate::openai::models::{HealthResponse, TokenResponse};
use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

const HARVESTER_TIMEOUT_SECS: u64 = 30;
const RETRY_ATTEMPTS: u32 = 3;
const RETRY_BACKOFF_MS: u64 = 500;
const TOKENS_ENDPOINT: &str = "/tokens";
const REFRESH_ENDPOINT: &str = "/refresh";
const HEALTH_ENDPOINT: &str = "/health";

fn calculate_backoff_ms(attempt: u32) -> u64 {
    // Exponential backoff: RETRY_BACKOFF_MS * 2^(attempt-1)
    // For attempt 1: 500ms, attempt 2: 1000ms, attempt 3: 2000ms
    RETRY_BACKOFF_MS * (1 << (attempt.saturating_sub(1) as u64))
}

#[derive(Clone)]
struct CachedToken {
    token: TokenResponse,
    cached_at: SystemTime,
}

pub struct HarvesterClient {
    base_url: String,
    client: reqwest::Client,
    cache: Arc<RwLock<Option<CachedToken>>>,
    access_token_ttl: Duration,
    arkose_token_ttl: Duration,
    metrics: Option<Arc<crate::openai::metrics::Metrics>>,
}

impl HarvesterClient {
    pub fn new(config: &Arc<AppConfig>) -> Result<Self> {
        let base_url = config.openai.harvester_url.clone();
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(HARVESTER_TIMEOUT_SECS))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            base_url,
            client,
            cache: Arc::new(RwLock::new(None)),
            access_token_ttl: Duration::from_secs(config.openai.access_token_ttl_secs),
            arkose_token_ttl: Duration::from_secs(config.openai.arkose_token_ttl_secs),
            metrics: None,
        })
    }

    pub fn with_metrics(mut self, metrics: Arc<crate::openai::metrics::Metrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    fn calculate_age(cached_at: SystemTime) -> Duration {
        SystemTime::now()
            .duration_since(cached_at)
            .unwrap_or_else(|e| {
                warn!(
                    "Clock skew detected: cached_at is in the future by {:?}. Treating cache as expired.",
                    e.duration()
                );
                // Return a duration longer than any TTL to force cache invalidation
                // This ensures we don't treat an expired token as fresh due to clock skew
                Duration::from_secs(u64::MAX)
            })
    }

    fn build_tokens_url(&self) -> String {
        format!("{}{}", self.base_url, TOKENS_ENDPOINT)
    }

    fn build_refresh_url(&self) -> String {
        format!("{}{}", self.base_url, REFRESH_ENDPOINT)
    }

    fn build_health_url(&self) -> String {
        format!("{}{}", self.base_url, HEALTH_ENDPOINT)
    }

    pub async fn get_tokens(&self, require_arkose: bool) -> Result<TokenResponse> {
        let now = SystemTime::now();

        {
            let cached_guard = self.cache.read().await;
            if let Some(cached) = cached_guard.as_ref() {
                let age = Self::calculate_age(cached.cached_at);

                // Fix logic bug: If arkose is required but cached token doesn't have it, invalidate cache
                if require_arkose && cached.token.arkose_token.is_none() {
                    debug!("Arkose token required but cached token doesn't have it, invalidating cache");
                    drop(cached_guard);
                    // Invalidate cache by clearing it
                    *self.cache.write().await = None;
                } else {
                    let token_ttl = if require_arkose && cached.token.arkose_token.is_some() {
                        self.arkose_token_ttl
                    } else {
                        self.access_token_ttl
                    };

                    if age < token_ttl {
                        info!("Using cached token (age: {:?})", age);
                        if let Some(ref m) = self.metrics {
                            m.record_cache_hit().await;
                        }
                        return Ok(cached.token.clone());
                    }
                }
            }
        }

        if let Some(ref m) = self.metrics {
            m.record_cache_miss().await;
        }

        let url = self.build_tokens_url();

        for attempt in 1..=RETRY_ATTEMPTS {
            let response = match self.client.get(&url).send().await {
                Ok(r) => r,
                Err(e) => {
                    if attempt == RETRY_ATTEMPTS {
                        anyhow::bail!(
                            "Failed to connect to Harvester after {} attempts: {}",
                            RETRY_ATTEMPTS,
                            e
                        );
                    }
                    tokio::time::sleep(Duration::from_millis(calculate_backoff_ms(attempt))).await;
                    continue;
                }
            };

            if !response.status().is_success() {
                if attempt == RETRY_ATTEMPTS {
                    let status = response.status();
                    let text = match response.text().await {
                        Ok(t) => t,
                        Err(e) => {
                            warn!("Failed to read error response body: {}", e);
                            String::new()
                        }
                    };
                    error!("Harvester error: {} - {}", status, text);
                    anyhow::bail!("Harvester returned error: {} - {}", status, text);
                }
                tokio::time::sleep(Duration::from_millis(calculate_backoff_ms(attempt))).await;
                continue;
            }

            let token: TokenResponse = match response.json().await {
                Ok(t) => t,
                Err(e) => {
                    if attempt == RETRY_ATTEMPTS {
                        anyhow::bail!("Failed to parse token response: {}", e);
                    }
                    tokio::time::sleep(Duration::from_millis(calculate_backoff_ms(attempt))).await;
                    continue;
                }
            };

            if require_arkose && token.arkose_token.is_none() {
                warn!("Arkose token required but not provided, requesting refresh");
                return self.refresh_tokens(true).await;
            }

            let cached = CachedToken {
                token: token.clone(),
                cached_at: now,
            };
            *self.cache.write().await = Some(cached);

            return Ok(token);
        }

        // Fix unreachable code: All paths in the loop above either return Ok, bail!, or continue
        // This bail! is logically unreachable but required by Rust's type system
        // If this is ever reached, it indicates a logic error in the retry loop
        anyhow::bail!(
            "Failed to get tokens after {} attempts (unreachable code path)",
            RETRY_ATTEMPTS
        );
    }

    pub async fn refresh_tokens(&self, force_arkose: bool) -> Result<TokenResponse> {
        let url = self.build_refresh_url();
        let body = serde_json::json!({
            "force_arkose": force_arkose
        });

        for attempt in 1..=RETRY_ATTEMPTS {
            let response = match self.client.post(&url).json(&body).send().await {
                Ok(r) => r,
                Err(e) => {
                    if attempt == RETRY_ATTEMPTS {
                        anyhow::bail!(
                            "Failed to connect to Harvester after {} attempts: {}",
                            RETRY_ATTEMPTS,
                            e
                        );
                    }
                    tokio::time::sleep(Duration::from_millis(calculate_backoff_ms(attempt))).await;
                    continue;
                }
            };

            if !response.status().is_success() {
                if attempt == RETRY_ATTEMPTS {
                    let status = response.status();
                    let text = match response.text().await {
                        Ok(t) => t,
                        Err(e) => {
                            warn!("Failed to read error response body: {}", e);
                            String::new()
                        }
                    };
                    error!("Harvester refresh error: {} - {}", status, text);
                    anyhow::bail!("Harvester refresh failed: {} - {}", status, text);
                }
                tokio::time::sleep(Duration::from_millis(calculate_backoff_ms(attempt))).await;
                continue;
            }

            let token: TokenResponse = match response.json().await {
                Ok(t) => t,
                Err(e) => {
                    if attempt == RETRY_ATTEMPTS {
                        anyhow::bail!("Failed to parse refresh response: {}", e);
                    }
                    tokio::time::sleep(Duration::from_millis(calculate_backoff_ms(attempt))).await;
                    continue;
                }
            };

            let cached = CachedToken {
                token: token.clone(),
                cached_at: SystemTime::now(),
            };
            *self.cache.write().await = Some(cached);

            return Ok(token);
        }

        anyhow::bail!(
            "Failed to refresh tokens after {} attempts (unreachable code path)",
            RETRY_ATTEMPTS
        );
    }

    pub async fn health_check(&self) -> Result<HealthResponse> {
        let url = self.build_health_url();

        for attempt in 1..=RETRY_ATTEMPTS {
            let response = match self.client.get(&url).send().await {
                Ok(r) => r,
                Err(e) => {
                    if attempt == RETRY_ATTEMPTS {
                        anyhow::bail!(
                            "Failed to connect to Harvester after {} attempts: {}",
                            RETRY_ATTEMPTS,
                            e
                        );
                    }
                    tokio::time::sleep(Duration::from_millis(calculate_backoff_ms(attempt))).await;
                    continue;
                }
            };

            if !response.status().is_success() {
                if attempt == RETRY_ATTEMPTS {
                    anyhow::bail!("Harvester health check failed: {}", response.status());
                }
                tokio::time::sleep(Duration::from_millis(calculate_backoff_ms(attempt))).await;
                continue;
            }

            let health: HealthResponse = match response.json().await {
                Ok(h) => h,
                Err(e) => {
                    if attempt == RETRY_ATTEMPTS {
                        anyhow::bail!("Failed to parse health response: {}", e);
                    }
                    tokio::time::sleep(Duration::from_millis(calculate_backoff_ms(attempt))).await;
                    continue;
                }
            };

            return Ok(health);
        }

        anyhow::bail!(
            "Failed health check after {} attempts (unreachable code path)",
            RETRY_ATTEMPTS
        );
    }
}
