use crate::models::openai::ChatCompletionRequest;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

#[derive(Clone, Serialize, Deserialize)]
struct CachedResponse {
    response: String,
    cached_at: DateTime<Utc>,
    ttl_secs: u64,
}

impl CachedResponse {
    fn is_expired(&self) -> bool {
        let now = Utc::now();
        let expires_at = self.cached_at + chrono::Duration::seconds(self.ttl_secs as i64);
        now > expires_at
    }
}

#[derive(Clone)]
pub struct Cache {
    store: Arc<RwLock<HashMap<String, CachedResponse>>>,
    default_ttl_secs: u64,
    enabled: bool,
}

impl Cache {
    pub fn new(enabled: bool, default_ttl_secs: u64) -> Self {
        Self {
            store: Arc::new(RwLock::new(HashMap::new())),
            default_ttl_secs,
            enabled,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn cache_key(request: &ChatCompletionRequest) -> String {
        // Generate cache key from model and messages
        // Note: This is a simple implementation. For production, consider:
        // - Hashing the full request
        // - Including temperature, max_tokens, etc. in key
        let messages_str = serde_json::to_string(&request.messages).unwrap_or_default();
        format!("{}:{}", request.model, messages_str)
    }

    pub async fn get(&self, request: &ChatCompletionRequest) -> Option<String> {
        if !self.enabled {
            return None;
        }

        let key = Self::cache_key(request);
        let store = self.store.read().await;

        if let Some(cached) = store.get(&key) {
            if cached.is_expired() {
                debug!("Cache miss (expired): {}", key);
                return None;
            }
            debug!("Cache hit: {}", key);
            return Some(cached.response.clone());
        }

        debug!("Cache miss (not found): {}", key);
        None
    }

    pub async fn set(
        &self,
        request: &ChatCompletionRequest,
        response: String,
        ttl_secs: Option<u64>,
    ) {
        if !self.enabled {
            return;
        }

        let key = Self::cache_key(request);
        let ttl = ttl_secs.unwrap_or(self.default_ttl_secs);

        let cached = CachedResponse {
            response,
            cached_at: Utc::now(),
            ttl_secs: ttl,
        };

        let mut store = self.store.write().await;
        store.insert(key, cached);
        debug!("Cached response with TTL: {}s", ttl);
    }

    pub async fn clear(&self) {
        let mut store = self.store.write().await;
        store.clear();
        debug!("Cache cleared");
    }

    pub async fn stats(&self) -> CacheStats {
        let store = self.store.read().await;
        let total_entries = store.len();
        let expired_entries = store.values().filter(|v| v.is_expired()).count();

        CacheStats {
            total_entries,
            active_entries: total_entries - expired_entries,
            expired_entries,
            enabled: self.enabled,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CacheStats {
    pub total_entries: usize,
    pub active_entries: usize,
    pub expired_entries: usize,
    pub enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::openai::{ChatMessage, Role};

    #[tokio::test]
    async fn test_cache_get_set() {
        let cache = Cache::new(true, 60);
        let request = ChatCompletionRequest {
            model: "test-model".to_string(),
            messages: vec![ChatMessage {
                role: Role::User,
                content: "test".to_string(),
                name: None,
            }],
            stream: false,
            temperature: 1.0,
            max_tokens: None,
            top_p: 1.0,
            stop: None,
        };

        // Cache miss
        assert!(cache.get(&request).await.is_none());

        // Set cache
        cache.set(&request, "test response".to_string(), None).await;

        // Cache hit
        assert_eq!(cache.get(&request).await, Some("test response".to_string()));
    }

    #[tokio::test]
    async fn test_cache_expiration() {
        let cache = Cache::new(true, 1); // 1 second TTL
        let request = ChatCompletionRequest {
            model: "test-model".to_string(),
            messages: vec![ChatMessage {
                role: Role::User,
                content: "test".to_string(),
                name: None,
            }],
            stream: false,
            temperature: 1.0,
            max_tokens: None,
            top_p: 1.0,
            stop: None,
        };

        cache.set(&request, "test response".to_string(), None).await;
        assert!(cache.get(&request).await.is_some());

        // Wait for expiration
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        assert!(cache.get(&request).await.is_none());
    }
}
