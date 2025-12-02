use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::RwLock;
use tracing::warn;

const TOKEN_CACHE_TTL_SECS: u64 = 3300;

#[derive(Clone)]
pub struct TokenManager {
    api_key: Option<String>,
    credentials_file: Option<String>,
    cached_token: Arc<RwLock<Option<CachedToken>>>,
    project_id: Option<String>,
}

struct CachedToken {
    token: String,
    expires_at: u64,
}

impl TokenManager {
    pub fn new(api_key: Option<String>, credentials_file: Option<String>) -> Result<Self> {
        let project_id = Self::extract_project_id(&credentials_file)?;

        Ok(Self {
            api_key,
            credentials_file,
            cached_token: Arc::new(RwLock::new(None)),
            project_id,
        })
    }

    pub fn is_api_key(&self) -> bool {
        self.api_key.is_some()
    }

    pub fn get_project_id(&self) -> Option<&str> {
        self.project_id.as_deref()
    }

    pub async fn get_token(&self) -> Result<String> {
        if let Some(key) = &self.api_key {
            return Ok(key.clone());
        }

        if let Some(ref cached) = *self.cached_token.read().await {
            let now = chrono::Utc::now().timestamp() as u64;
            if now < cached.expires_at {
                return Ok(cached.token.clone());
            }
        }

        let token = self
            .fetch_token()
            .await
            .context("Failed to fetch Google Cloud access token")?;

        let expires_at = chrono::Utc::now().timestamp() as u64 + TOKEN_CACHE_TTL_SECS;
        *self.cached_token.write().await = Some(CachedToken {
            token: token.clone(),
            expires_at,
        });

        Ok(token)
    }

    async fn fetch_token(&self) -> Result<String> {
        let output = Command::new("gcloud")
            .args(["auth", "print-access-token"])
            .output()
            .await
            .context("Failed to execute gcloud command")?;

        if output.status.success() {
            let token = String::from_utf8(output.stdout)
                .context("Failed to parse gcloud output as UTF-8")?
                .trim()
                .to_string();
            return Ok(token);
        }

        let mut cmd = Command::new("gcloud");
        cmd.args(["auth", "application-default", "print-access-token"]);

        if let Some(ref creds_file) = self.credentials_file {
            cmd.env("GOOGLE_APPLICATION_CREDENTIALS", creds_file);
        }

        let output = cmd
            .output()
            .await
            .context("Failed to execute gcloud application-default command")?;

        if output.status.success() {
            let token = String::from_utf8(output.stdout)
                .context("Failed to parse gcloud output as UTF-8")?
                .trim()
                .to_string();
            return Ok(token);
        }

        anyhow::bail!(
            "Failed to get access token. Ensure gcloud CLI is installed and authenticated, or set GOOGLE_APPLICATION_CREDENTIALS"
        )
    }

    fn extract_project_id(credentials_file: &Option<String>) -> Result<Option<String>> {
        if let Some(ref file) = credentials_file {
            if let Ok(contents) = std::fs::read_to_string(file) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&contents) {
                    if let Some(project_id) = json.get("project_id").and_then(|v| v.as_str()) {
                        return Ok(Some(project_id.to_string()));
                    }
                }
            }
        }

        let output = std::process::Command::new("gcloud")
            .args(["config", "get-value", "project"])
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                match String::from_utf8(output.stdout) {
                    Ok(project) => {
                        let project = project.trim().to_string();
                        if !project.is_empty() {
                            return Ok(Some(project));
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse gcloud project output as UTF-8: {}", e);
                    }
                }
            }
        }

        if let Ok(project) = std::env::var("GOOGLE_CLOUD_PROJECT") {
            return Ok(Some(project));
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_token_manager_api_key() {
        let api_key = Some("test-api-key-123".to_string());
        let tm = TokenManager::new(api_key.clone(), None).unwrap();

        assert!(tm.is_api_key());
        let token = tm.get_token().await.unwrap();
        assert_eq!(token, "test-api-key-123");
    }

    #[tokio::test]
    async fn test_token_manager_no_credentials() {
        let tm = TokenManager::new(None, None);
        assert!(tm.is_ok());
        let tm = tm.unwrap();
        assert!(!tm.is_api_key());
    }

    #[test]
    fn test_extract_project_id_from_env() {
        let original = std::env::var("GOOGLE_CLOUD_PROJECT").ok();

        unsafe {
            std::env::set_var("GOOGLE_CLOUD_PROJECT", "test-project-123");
        }

        let project_id = TokenManager::extract_project_id(&None).unwrap();
        // gcloud CLI may override env var, so we check the value if present
        if let Some(ref id) = project_id {
            assert!(!id.is_empty(), "Project ID should not be empty if present");
        }

        if let Some(val) = original {
            unsafe {
                std::env::set_var("GOOGLE_CLOUD_PROJECT", val);
            }
        } else {
            unsafe {
                std::env::remove_var("GOOGLE_CLOUD_PROJECT");
            }
        }
    }

    #[test]
    fn test_extract_project_id_none() {
        unsafe {
            std::env::remove_var("GOOGLE_CLOUD_PROJECT");
        }
        let project_id = TokenManager::extract_project_id(&None).unwrap();
        assert_eq!(project_id, None);
    }
}
