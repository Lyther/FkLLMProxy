use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

const GOOGLE_OAUTH_URL: &str = "https://oauth2.googleapis.com/token";
const SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";

#[derive(Debug, Deserialize)]
pub struct ServiceAccount {
    client_email: String,
    private_key: String,
    project_id: String,
}

#[derive(Debug, Serialize)]
struct Claims {
    iss: String,
    scope: String,
    aud: String,
    exp: usize,
    iat: usize,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[allow(dead_code)]
    expires_in: i64,
}

#[derive(Clone)]
struct CachedToken {
    token: String,
    expires_at: chrono::DateTime<Utc>,
}

#[derive(Clone)]
pub enum AuthMode {
    ServiceAccount(Arc<ServiceAccount>),
    ApiKey(String),
}

#[derive(Clone)]
pub struct TokenManager {
    mode: AuthMode,
    client: Client,
    cache: Arc<RwLock<Option<CachedToken>>>,
}

impl TokenManager {
    pub fn new(api_key: Option<String>, credentials_path: Option<String>) -> Result<Self> {
        // 1. Try API Key first (Simpler)
        if let Some(key) = api_key {
            if !key.is_empty() {
                info!("Using Google API Key for authentication.");
                return Ok(Self {
                    mode: AuthMode::ApiKey(key),
                    client: Client::new(),
                    cache: Arc::new(RwLock::new(None)),
                });
            }
        }

        // 2. Fallback to Service Account
        let path = credentials_path
            .or_else(|| env::var("GOOGLE_APPLICATION_CREDENTIALS").ok())
            .context("GOOGLE_APPLICATION_CREDENTIALS not set and no API Key provided")?;

        let content = std::fs::read_to_string(&path)
            .context(format!("Failed to read credentials file: {}", path))?;

        let service_account: ServiceAccount =
            serde_json::from_str(&content).context("Failed to parse service account JSON")?;

        info!("Using Service Account: {}", service_account.client_email);

        Ok(Self {
            mode: AuthMode::ServiceAccount(Arc::new(service_account)),
            client: Client::new(),
            cache: Arc::new(RwLock::new(None)),
        })
    }

    pub fn get_project_id(&self) -> Option<&str> {
        match &self.mode {
            AuthMode::ServiceAccount(sa) => Some(&sa.project_id),
            AuthMode::ApiKey(_) => None,
        }
    }

    pub fn is_api_key(&self) -> bool {
        matches!(self.mode, AuthMode::ApiKey(_))
    }

    pub async fn get_token(&self) -> Result<String> {
        match &self.mode {
            AuthMode::ApiKey(key) => Ok(key.clone()),
            AuthMode::ServiceAccount(_) => self.get_oauth_token().await,
        }
    }

    async fn get_oauth_token(&self) -> Result<String> {
        // 1. Check cache
        {
            let cache = self.cache.read().await;
            if let Some(cached) = &*cache {
                if cached.expires_at > Utc::now() + Duration::minutes(5) {
                    return Ok(cached.token.clone());
                }
            }
        }

        // 2. Refresh
        info!("Refreshing Google Access Token...");
        let token = self.fetch_oauth_token().await?;

        // 3. Update cache
        let mut cache = self.cache.write().await;
        *cache = Some(CachedToken {
            token: token.clone(),
            expires_at: Utc::now() + Duration::seconds(3500),
        });

        Ok(token)
    }

    async fn fetch_oauth_token(&self) -> Result<String> {
        let sa = match &self.mode {
            AuthMode::ServiceAccount(sa) => sa,
            _ => anyhow::bail!("Not in Service Account mode"),
        };

        let now = Utc::now();
        let iat = now.timestamp() as usize;
        let exp = (now + Duration::minutes(60)).timestamp() as usize;

        let claims = Claims {
            iss: sa.client_email.clone(),
            scope: SCOPE.to_string(),
            aud: GOOGLE_OAUTH_URL.to_string(),
            exp,
            iat,
        };

        let header = Header::new(Algorithm::RS256);
        let key = EncodingKey::from_rsa_pem(sa.private_key.as_bytes())?;

        let jwt = encode(&header, &claims, &key)?;

        let params = [
            ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
            ("assertion", &jwt),
        ];

        let res = self
            .client
            .post(GOOGLE_OAUTH_URL)
            .form(&params)
            .send()
            .await?;

        if !res.status().is_success() {
            let status = res.status();
            let text = res.text().await.unwrap_or_default();
            error!("Token fetch failed: {} - {}", status, text);
            anyhow::bail!("Failed to fetch token: {}", status);
        }

        let token_res: TokenResponse = res.json().await?;
        Ok(token_res.access_token)
    }
}
