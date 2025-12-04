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
    last_access: DateTime<Utc>, // Track last access for LRU eviction
}

impl CachedResponse {
    fn is_expired(&self) -> bool {
        let now = Utc::now();
        // Fix: Prevent overflow when converting u64 to i64 for chrono::Duration
        let ttl_secs_i64 = self.ttl_secs.min(i64::MAX as u64) as i64;
        let expires_at = self.cached_at + chrono::Duration::seconds(ttl_secs_i64);
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
        // Fix incomplete cache key: Include all parameters that affect response
        // Fix collision risk: Use structured format with delimiter that won't appear in model names
        // Use "|" as delimiter (unlikely in model names) and include all relevant params

        let messages_str = serde_json::to_string(&request.messages)?;
        let temperature_str = format!("{:.6}", request.temperature); // Use fixed precision
        let max_tokens_str = request
            .max_tokens
            .map(|v| v.to_string())
            .unwrap_or_else(|| "none".to_string());
        let top_p_str = format!("{:.6}", request.top_p);
        let stop_str = request
            .stop
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?
            .unwrap_or_else(|| "none".to_string());

        // Format: model|messages|temperature|max_tokens|top_p|stop
        // Using "|" delimiter which is unlikely to appear in model names or JSON
        Ok(format!(
            "{}|{}|{}|{}|{}|{}",
            request.model, messages_str, temperature_str, max_tokens_str, top_p_str, stop_str
        ))
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

            // Fix inefficient eviction: Single pass with LRU ordering
            // Fix non-deterministic eviction: Sort by last_access for LRU eviction
            let mut entries: Vec<(String, DateTime<Utc>)> = store
                .iter()
                .map(|(k, v)| (k.clone(), v.last_access))
                .collect();

            // Sort by last_access (oldest first) for LRU eviction
            entries.sort_by_key(|(_, access_time)| *access_time);

            // Remove oldest entries first
            let keys_to_remove: Vec<String> = entries
                .iter()
                .take(to_remove)
                .map(|(k, _)| k.clone())
                .collect();

            for key in keys_to_remove {
                store.remove(&key);
            }
            warn!(
                "Cache size limit exceeded, removed {} oldest entries (LRU)",
                to_remove
            );
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

        // Fix race condition: Use write lock to atomically check and remove expired entry
        // This prevents entry from being re-inserted between check and cleanup
        let mut store = self.store.write().await;

        if let Some(cached) = store.get_mut(&key) {
            if cached.is_expired() {
                debug!("Cache miss (expired): {}", key);
                // Remove expired entry atomically while holding write lock
                store.remove(&key);
                drop(store);
                return None;
            }
            // Fix LRU: Update last_access on cache hit
            cached.last_access = Utc::now();
            debug!("Cache hit: {}", key);
            let response = cached.response.clone();
            drop(store);
            return Some(response);
        }

        drop(store);
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

        let now = Utc::now();
        let cached = CachedResponse {
            response,
            cached_at: now,
            ttl_secs: ttl,
            last_access: now, // Initialize last_access
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

    // Fix: Add cache invalidation API for manual invalidation
    pub async fn invalidate(&self, request: &ChatCompletionRequest) -> bool {
        if !self.enabled {
            return false;
        }

        let key = match Self::cache_key(request) {
            Ok(k) => k,
            Err(e) => {
                warn!("Failed to generate cache key for invalidation: {}", e);
                return false;
            }
        };

        let mut store = self.store.write().await;
        let removed = store.remove(&key).is_some();
        if removed {
            debug!("Cache entry invalidated: {}", key);
        }
        removed
    }

    pub async fn stats(&self) -> CacheStats {
        // Fix stale stats: cleanup expired entries first, then count
        // This ensures active_entries calculation is accurate
        self.cleanup_expired().await;

        let store = self.store.read().await;
        let total_entries = store.len();
        let expired_entries = store.values().filter(|v| v.is_expired()).count();

        CacheStats {
            total_entries,
            // Fix potential underflow: use saturating_sub to prevent underflow
            active_entries: total_entries.saturating_sub(expired_entries),
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

        // stats() now cleans up expired entries first, so expired entries should be 0
        let stats = cache.stats().await;
        assert_eq!(stats.total_entries, 0);
        assert_eq!(stats.expired_entries, 0);
        assert_eq!(stats.active_entries, 0);
    }
}
