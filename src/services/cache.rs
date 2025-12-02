use crate::models::openai::ChatCompletionRequest;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

const MAX_CACHE_SIZE: usize = 10_000;

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

    fn cache_key(request: &ChatCompletionRequest) -> Result<String, serde_json::Error> {
        let messages_str = serde_json::to_string(&request.messages)?;
        Ok(format!("{}:{}", request.model, messages_str))
    }

    async fn cleanup_expired(&self) {
        let mut store = self.store.write().await;
        let initial_size = store.len();
        store.retain(|_, v| !v.is_expired());
        let removed = initial_size - store.len();
        if removed > 0 {
            debug!("Cache cleanup: removed {} expired entries", removed);
        }
    }

    async fn enforce_size_limit(&self) {
        let mut store = self.store.write().await;
        if store.len() > MAX_CACHE_SIZE {
            let to_remove = store.len() - MAX_CACHE_SIZE;
            let mut keys_to_remove: Vec<String> = store
                .iter()
                .filter(|(_, v)| v.is_expired())
                .map(|(k, _)| k.clone())
                .take(to_remove)
                .collect();

            if keys_to_remove.len() < to_remove {
                let mut remaining: Vec<String> = store
                    .keys()
                    .filter(|k| !keys_to_remove.contains(k))
                    .take(to_remove - keys_to_remove.len())
                    .cloned()
                    .collect();
                keys_to_remove.append(&mut remaining);
            }

            for key in keys_to_remove {
                store.remove(&key);
            }
            warn!("Cache size limit exceeded, removed {} entries", to_remove);
        }
    }

    pub async fn get(&self, request: &ChatCompletionRequest) -> Option<String> {
        if !self.enabled {
            return None;
        }

        let key = match Self::cache_key(request) {
            Ok(k) => k,
            Err(e) => {
                warn!("Failed to generate cache key: {}", e);
                return None;
            }
        };

        let store = self.store.read().await;

        if let Some(cached) = store.get(&key) {
            if cached.is_expired() {
                debug!("Cache miss (expired): {}", key);
                drop(store);
                self.cleanup_expired().await;
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

        let key = match Self::cache_key(request) {
            Ok(k) => k,
            Err(e) => {
                warn!("Failed to generate cache key: {}", e);
                return;
            }
        };

        let ttl = ttl_secs.unwrap_or(self.default_ttl_secs);

        let cached = CachedResponse {
            response,
            cached_at: Utc::now(),
            ttl_secs: ttl,
        };

        let mut store = self.store.write().await;
        store.insert(key, cached);
        debug!("Cached response with TTL: {}s", ttl);
        drop(store);

        self.enforce_size_limit().await;
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
        drop(store);

        self.cleanup_expired().await;

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

        assert!(cache.get(&request).await.is_none());

        cache.set(&request, "test response".to_string(), None).await;

        assert_eq!(cache.get(&request).await, Some("test response".to_string()));
    }

    #[tokio::test]
    async fn test_cache_expiration() {
        let cache = Cache::new(true, 1);
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

        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        assert!(cache.get(&request).await.is_none());
    }

    #[tokio::test]
    async fn test_cache_cleanup() {
        let cache = Cache::new(true, 1);
        let mut requests = Vec::new();
        for i in 0..5 {
            requests.push(ChatCompletionRequest {
                model: "test-model".to_string(),
                messages: vec![ChatMessage {
                    role: Role::User,
                    content: format!("test{}", i),
                    name: None,
                }],
                stream: false,
                temperature: 1.0,
                max_tokens: None,
                top_p: 1.0,
                stop: None,
            });
        }

        for req in &requests {
            cache.set(req, "response".to_string(), None).await;
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // Before cleanup: stats should show expired entries
        let stats = cache.stats().await;
        // After stats() call, cleanup runs, so total should be 0
        assert_eq!(stats.total_entries, 5);
        assert_eq!(stats.expired_entries, 5);
        assert_eq!(stats.active_entries, 0);
    }
}
