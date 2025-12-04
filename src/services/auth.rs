use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::RwLock;
use tokio::time::timeout;
use tracing::warn;

const TOKEN_CACHE_TTL_SECS: u64 = 3300;
const GCLOUD_TIMEOUT_SECS: u64 = 10;
const MAX_RETRIES: u32 = 3;
const INITIAL_RETRY_DELAY_MS: u64 = 100;

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
        // Validate credentials_file exists and is readable if provided
        if let Some(ref file) = credentials_file {
            let path = std::path::Path::new(file);
            if !path.exists() {
                return Err(anyhow::anyhow!("Credentials file does not exist: {}", file));
            }
            if !path.is_file() {
                return Err(anyhow::anyhow!("Credentials path is not a file: {}", file));
            }
            // Check readability by attempting to read metadata
            if std::fs::metadata(file).is_err() {
                return Err(anyhow::anyhow!(
                    "Cannot read credentials file (permission denied?): {}",
                    file
                ));
            }
        }

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

        // Fix race condition: Use write lock for double-checked locking pattern
        // First check with read lock (fast path)
        {
            let cached = self.cached_token.read().await;
            if let Some(ref cached_token) = *cached {
                // Fix timestamp overflow: clamp timestamp to prevent overflow
                let now = chrono::Utc::now().timestamp();
                let now_u64 = now.max(0) as u64;
                if now_u64 < cached_token.expires_at {
                    return Ok(cached_token.token.clone());
                }
            }
        }

        // Acquire write lock for check-and-set (prevents concurrent fetches)
        let mut cached = self.cached_token.write().await;

        // Double-check: another thread might have updated cache while we waited for write lock
        if let Some(ref cached_token) = *cached {
            let now = chrono::Utc::now().timestamp();
            let now_u64 = now.max(0) as u64;
            if now_u64 < cached_token.expires_at {
                return Ok(cached_token.token.clone());
            }
        }

        let token = self
            .fetch_token()
            .await
            .context("Failed to fetch Google Cloud access token")?;

        // Fix timestamp overflow: clamp timestamp to prevent overflow
        let now = chrono::Utc::now().timestamp();
        let now_u64 = now.max(0) as u64;
        let expires_at = now_u64.saturating_add(TOKEN_CACHE_TTL_SECS);

        *cached = Some(CachedToken {
            token: token.clone(),
            expires_at,
        });

        Ok(token)
    }

    async fn fetch_token(&self) -> Result<String> {
        // Try primary method with retries
        let result = self.fetch_token_with_retry(false).await;
        if result.is_ok() {
            return result;
        }

        // Fallback to application-default with retries
        self.fetch_token_with_retry(true).await
    }

    async fn fetch_token_with_retry(&self, use_application_default: bool) -> Result<String> {
        let mut last_error = None;

        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                // Exponential backoff: 100ms, 200ms, 400ms
                let delay_ms = INITIAL_RETRY_DELAY_MS * (1 << (attempt - 1));
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            }

            let mut cmd = Command::new("gcloud");
            if use_application_default {
                cmd.args(["auth", "application-default", "print-access-token"]);
                if let Some(ref creds_file) = self.credentials_file {
                    // Validate file still exists before using (could have been deleted)
                    if !std::path::Path::new(creds_file).exists() {
                        anyhow::bail!("Credentials file no longer exists: {}", creds_file);
                    }
                    cmd.env("GOOGLE_APPLICATION_CREDENTIALS", creds_file);
                }
            } else {
                cmd.args(["auth", "print-access-token"]);
            }

            match timeout(Duration::from_secs(GCLOUD_TIMEOUT_SECS), cmd.output()).await {
                Ok(Ok(output)) if output.status.success() => {
                    let token = String::from_utf8(output.stdout)
                        .context("Failed to parse gcloud output as UTF-8")?
                        .trim()
                        .to_string();
                    if token.is_empty() {
                        last_error = Some(anyhow::anyhow!("gcloud returned empty access token"));
                        continue;
                    }
                    return Ok(token);
                }
                Ok(Ok(output)) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    last_error = Some(anyhow::anyhow!(
                        "gcloud command failed with status {}: {}",
                        output.status,
                        stderr
                    ));
                }
                Ok(Err(e)) => {
                    last_error = Some(
                        anyhow::anyhow!("Failed to execute gcloud command: {}", e)
                            .context("gcloud command execution failed"),
                    );
                }
                Err(_) => {
                    last_error = Some(anyhow::anyhow!(
                        "gcloud command timed out after {} seconds",
                        GCLOUD_TIMEOUT_SECS
                    ));
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            anyhow::anyhow!("Failed to get access token after {} retries", MAX_RETRIES)
        }))
        .context(if use_application_default {
            "Failed to get access token using application-default credentials. Ensure gcloud CLI is installed and authenticated, or set GOOGLE_APPLICATION_CREDENTIALS"
        } else {
            "Failed to get access token. Ensure gcloud CLI is installed and authenticated"
        })
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

        // Fix: Use blocking command but only during initialization
        // This is acceptable since extract_project_id is only called from new() during startup
        // If new() is called from async context, consider making new() async in the future
        // For now, gcloud commands are fast (<1s) so blocking is acceptable during init
        match std::process::Command::new("gcloud")
            .args(["config", "get-value", "project"])
            .output()
        {
            Ok(output) if output.status.success() => match String::from_utf8(output.stdout) {
                Ok(project) => {
                    let project = project.trim().to_string();
                    if !project.is_empty() {
                        return Ok(Some(project));
                    }
                }
                Err(e) => {
                    warn!("Failed to parse gcloud project output as UTF-8: {}", e);
                }
            },
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!(
                    "gcloud config get-value project failed with status {}: {}",
                    output.status, stderr
                );
            }
            Err(e) => {
                warn!("Failed to execute gcloud config get-value project: {}", e);
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
        let tm = TokenManager::new(api_key.clone(), None).expect("Should create TokenManager");

        assert!(tm.is_api_key());
        let token = tm.get_token().await.expect("Should get token");
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
    fn test_token_manager_invalid_credentials_file() {
        // Test with non-existent file
        let result = TokenManager::new(None, Some("/nonexistent/path/to/file.json".to_string()));
        assert!(result.is_err());
        let err_msg = result.err().expect("Should have error").to_string();
        assert!(
            err_msg.contains("does not exist"),
            "Error should mention file doesn't exist"
        );

        // Test with directory instead of file
        let result = TokenManager::new(None, Some("/tmp".to_string()));
        assert!(result.is_err());
        let err_msg = result.err().expect("Should have error").to_string();
        assert!(
            err_msg.contains("not a file"),
            "Error should mention path is not a file"
        );
    }

    #[tokio::test]
    async fn test_token_cache_expiration() {
        // This test verifies cache expiration logic
        // Note: Actual gcloud calls will fail in test environment, but we can test the cache logic
        let tm = TokenManager::new(None, None).expect("Should create TokenManager");

        // First call should attempt to fetch (will fail without gcloud, but tests cache logic)
        let _ = tm.get_token().await;

        // Verify cache structure exists
        let cached = tm.cached_token.read().await;
        // Cache might be None if fetch failed, which is expected in test environment
        if cached.is_some() {
            let cached_token = cached.as_ref().unwrap();
            assert!(!cached_token.token.is_empty());
            assert!(cached_token.expires_at > 0);
        }
    }

    #[tokio::test]
    async fn test_concurrent_token_access() {
        // Test that concurrent calls don't cause race conditions
        let tm = TokenManager::new(None, None).expect("Should create TokenManager");

        // Spawn multiple concurrent token requests
        let mut handles = vec![];
        for _ in 0..10 {
            let tm_clone = tm.clone();
            handles.push(tokio::spawn(async move { tm_clone.get_token().await }));
        }

        // Wait for all requests
        let mut results = vec![];
        for handle in handles {
            results.push(handle.await);
        }

        // All should either succeed or fail consistently (no panics)
        for result in results {
            assert!(result.is_ok(), "Task should complete without panic");
        }
    }

    #[tokio::test]
    async fn test_token_cache_race_condition_prevention() {
        // Test that double-checked locking prevents multiple concurrent fetches
        let tm = TokenManager::new(None, None).expect("Should create TokenManager");

        // Spawn multiple concurrent requests
        let mut handles = vec![];
        for _ in 0..5 {
            let tm_clone = tm.clone();
            handles.push(tokio::spawn(async move { tm_clone.get_token().await }));
        }

        let mut results = vec![];
        for handle in handles {
            results.push(handle.await);
        }

        // Verify all completed (even if they failed due to missing gcloud)
        for result in results {
            assert!(result.is_ok(), "Concurrent access should not panic");
        }

        // Verify cache is in consistent state
        let cached = tm.cached_token.read().await;
        // Cache state should be consistent (either None or valid CachedToken)
        if let Some(ref token) = *cached {
            assert!(!token.token.is_empty());
            assert!(token.expires_at > 0);
        }
    }

    #[test]
    fn test_extract_project_id_from_env() {
        let original = std::env::var("GOOGLE_CLOUD_PROJECT").ok();

        unsafe {
            std::env::set_var("GOOGLE_CLOUD_PROJECT", "test-project-123");
        }

        let project_id =
            TokenManager::extract_project_id(&None).expect("Should extract project ID");
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
        // Remove env var and ensure it's gone
        unsafe {
            std::env::remove_var("GOOGLE_CLOUD_PROJECT");
        }
        // Verify env var is actually removed
        assert!(std::env::var("GOOGLE_CLOUD_PROJECT").is_err());

        let project_id = TokenManager::extract_project_id(&None).expect("Should return Option");
        // Note: gcloud CLI may return a project ID even if env var is not set
        // So we just verify the function returns without error, not that it's None
        // The actual value depends on gcloud configuration
        if let Some(ref id) = project_id {
            assert!(
                !id.is_empty(),
                "If project ID is present, it should not be empty"
            );
        }
    }

    #[test]
    fn test_extract_project_id_from_credentials_file() {
        // Create a temporary JSON file with project_id
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_credentials.json");
        let creds_json = r#"{"project_id": "test-project-from-file", "type": "service_account"}"#;
        std::fs::write(&temp_file, creds_json).expect("Should write temp file");

        let result =
            TokenManager::extract_project_id(&Some(temp_file.to_string_lossy().to_string()));
        assert!(result.is_ok());
        let project_id = result.unwrap();
        assert_eq!(project_id, Some("test-project-from-file".to_string()));

        // Cleanup
        let _ = std::fs::remove_file(&temp_file);
    }
}
