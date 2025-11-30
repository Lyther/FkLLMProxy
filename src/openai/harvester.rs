use crate::config::AppConfig;
use crate::openai::models::{HealthResponse, TokenResponse};
use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use tracing::{error, info, warn};

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
            .timeout(Duration::from_secs(30))
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

    pub async fn get_tokens(&self, require_arkose: bool) -> Result<TokenResponse> {
        let now = SystemTime::now();

        if let Some(cached) = self.cache.read().await.as_ref() {
            let age = now
                .duration_since(cached.cached_at)
                .unwrap_or(Duration::ZERO);
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

        if let Some(ref m) = self.metrics {
            m.record_cache_miss().await;
        }

        let url = format!("{}/tokens", self.base_url);

        for attempt in 1..=3 {
            let response = match self.client.get(&url).send().await {
                Ok(r) => r,
                Err(e) => {
                    if attempt == 3 {
                        anyhow::bail!("Failed to connect to Harvester after 3 attempts: {}", e);
                    }
                    tokio::time::sleep(Duration::from_millis(500 * attempt as u64)).await;
                    continue;
                }
            };

            if !response.status().is_success() {
                if attempt == 3 {
                    let status = response.status();
                    let text = response.text().await.unwrap_or_default();
                    error!("Harvester error: {} - {}", status, text);
                    anyhow::bail!("Harvester returned error: {} - {}", status, text);
                }
                tokio::time::sleep(Duration::from_millis(500 * attempt as u64)).await;
                continue;
            }

            let token: TokenResponse = match response.json().await {
                Ok(t) => t,
                Err(e) => {
                    if attempt == 3 {
                        anyhow::bail!("Failed to parse token response: {}", e);
                    }
                    tokio::time::sleep(Duration::from_millis(500 * attempt as u64)).await;
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

        anyhow::bail!("Failed to get tokens after 3 attempts");
    }

    pub async fn refresh_tokens(&self, force_arkose: bool) -> Result<TokenResponse> {
        let url = format!("{}/refresh", self.base_url);
        let body = serde_json::json!({
            "force_arkose": force_arkose
        });

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("Failed to connect to Harvester")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            error!("Harvester refresh error: {} - {}", status, text);
            anyhow::bail!("Harvester refresh failed: {} - {}", status, text);
        }

        let token: TokenResponse = response
            .json()
            .await
            .context("Failed to parse refresh response")?;

        let cached = CachedToken {
            token: token.clone(),
            cached_at: SystemTime::now(),
        };
        *self.cache.write().await = Some(cached);

        Ok(token)
    }

    pub async fn health_check(&self) -> Result<HealthResponse> {
        let url = format!("{}/health", self.base_url);
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to connect to Harvester")?;

        if !response.status().is_success() {
            anyhow::bail!("Harvester health check failed: {}", response.status());
        }

        let health: HealthResponse = response
            .json()
            .await
            .context("Failed to parse health response")?;

        Ok(health)
    }
}
