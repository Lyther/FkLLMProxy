use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use std::{
    collections::HashMap,
    net::IpAddr,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;
use tracing::{error, warn};

const CLEANUP_INTERVAL: Duration = Duration::from_secs(300);
const MAX_BUCKETS: usize = 10_000;
const UNKNOWN_KEY: &str = "unknown";

fn is_valid_ip(ip_str: &str) -> bool {
    ip_str.parse::<IpAddr>().is_ok()
}

fn extract_client_ip(request: &Request) -> String {
    if let Some(auth_header) = request
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
    {
        return auth_header.to_string();
    }

    let forwarded_ips = request
        .headers()
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.split(',').map(|ip| ip.trim()).collect::<Vec<_>>());

    if let Some(ref ips) = forwarded_ips {
        for ip in ips {
            if is_valid_ip(ip) {
                return ip.to_string();
            }
        }
        warn!(
            "x-forwarded-for header contains no valid IP addresses: {:?}",
            ips
        );
    }

    if let Some(remote_addr) = request
        .headers()
        .get("x-real-ip")
        .and_then(|h| h.to_str().ok())
    {
        if is_valid_ip(remote_addr) {
            return remote_addr.to_string();
        }
    }

    UNKNOWN_KEY.to_string()
}

#[derive(Clone)]
pub struct RateLimiter {
    buckets: Arc<RwLock<HashMap<String, TokenBucket>>>,
    capacity: u32,
    refill_rate: Duration,
    last_cleanup: Arc<RwLock<Instant>>,
}

#[derive(Clone)]
struct TokenBucket {
    tokens: u32,
    last_refill: Instant,
}

#[derive(Debug, Clone)]
pub struct RateLimitInfo {
    pub limit: u32,
    pub remaining: u32,
    pub reset: u64,
}

impl RateLimiter {
    pub fn new(capacity: u32, refill_per_second: u32) -> Self {
        Self {
            buckets: Arc::new(RwLock::new(HashMap::new())),
            capacity,
            refill_rate: Duration::from_secs(1) / refill_per_second,
            last_cleanup: Arc::new(RwLock::new(Instant::now())),
        }
    }

    fn calculate_tokens_to_add(elapsed: Duration, refill_rate: Duration) -> u32 {
        let elapsed_nanos = elapsed.as_nanos() as u64;
        let refill_nanos = refill_rate.as_nanos() as u64;
        if refill_nanos == 0 {
            return 0;
        }
        (elapsed_nanos / refill_nanos) as u32
    }

    async fn cleanup_if_needed(&self) {
        let mut last_cleanup = self.last_cleanup.write().await;
        if last_cleanup.elapsed() >= CLEANUP_INTERVAL {
            let mut buckets = self.buckets.write().await;
            let initial_size = buckets.len();
            let now = Instant::now();
            let expiration_threshold = CLEANUP_INTERVAL * 2;

            buckets
                .retain(|_, bucket| now.duration_since(bucket.last_refill) <= expiration_threshold);

            if buckets.len() > MAX_BUCKETS {
                let to_remove = buckets.len() - MAX_BUCKETS;
                let keys: Vec<String> = buckets.keys().take(to_remove).cloned().collect();
                for key in keys {
                    buckets.remove(&key);
                }
                warn!(
                    "Rate limiter: removed {} buckets to enforce size limit",
                    to_remove
                );
            }
            *last_cleanup = Instant::now();
            let removed = initial_size.saturating_sub(buckets.len());
            if removed > 0 {
                warn!("Rate limiter cleanup: {} expired buckets removed", removed);
            }
        }
    }

    pub async fn check(&self, key: &str) -> bool {
        self.cleanup_if_needed().await;

        let mut buckets = self.buckets.write().await;
        let bucket = buckets
            .entry(key.to_string())
            .or_insert_with(|| TokenBucket {
                tokens: self.capacity,
                last_refill: Instant::now(),
            });

        let now = Instant::now();
        let elapsed = now.duration_since(bucket.last_refill);
        let tokens_to_add = Self::calculate_tokens_to_add(elapsed, self.refill_rate);

        if tokens_to_add > 0 {
            bucket.tokens = (bucket.tokens + tokens_to_add).min(self.capacity);
            bucket.last_refill = now;
        }

        if bucket.tokens > 0 {
            bucket.tokens -= 1;
            true
        } else {
            false
        }
    }

    pub async fn get_info(&self, key: &str) -> RateLimitInfo {
        let buckets = self.buckets.read().await;
        let bucket = buckets.get(key).cloned().unwrap_or_else(|| TokenBucket {
            tokens: self.capacity,
            last_refill: Instant::now(),
        });

        let now = Instant::now();
        let elapsed = now.duration_since(bucket.last_refill);
        let tokens_to_add = Self::calculate_tokens_to_add(elapsed, self.refill_rate);
        let current_tokens = (bucket.tokens + tokens_to_add).min(self.capacity);

        let tokens_needed = self.capacity.saturating_sub(current_tokens);
        let reset_seconds = if tokens_needed > 0 {
            let refill_nanos = self.refill_rate.as_nanos() as u64;
            if refill_nanos == 0 {
                0
            } else {
                (tokens_needed as u64 * refill_nanos) / 1_000_000_000
            }
        } else {
            0
        };

        RateLimitInfo {
            limit: self.capacity,
            remaining: current_tokens,
            reset: now.elapsed().as_secs() + reset_seconds,
        }
    }
}

fn build_rate_limit_headers(
    info: &RateLimitInfo,
) -> Result<Vec<(axum::http::HeaderName, axum::http::HeaderValue)>, String> {
    let limit_header = axum::http::HeaderValue::from_str(&info.limit.to_string())
        .map_err(|e| format!("Failed to construct X-RateLimit-Limit header: {}", e))?;

    let remaining_header = axum::http::HeaderValue::from_str(&info.remaining.to_string())
        .map_err(|e| format!("Failed to construct X-RateLimit-Remaining header: {}", e))?;

    let reset_header = axum::http::HeaderValue::from_str(&info.reset.to_string())
        .map_err(|e| format!("Failed to construct X-RateLimit-Reset header: {}", e))?;

    Ok(vec![
        (
            axum::http::header::HeaderName::from_static("x-ratelimit-limit"),
            limit_header,
        ),
        (
            axum::http::header::HeaderName::from_static("x-ratelimit-remaining"),
            remaining_header,
        ),
        (
            axum::http::header::HeaderName::from_static("x-ratelimit-reset"),
            reset_header,
        ),
    ])
}

pub async fn rate_limit_middleware(
    State(limiter): State<RateLimiter>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let key = extract_client_ip(&request);
    let info = limiter.get_info(&key).await;

    if !limiter.check(&key).await {
        warn!("Rate limit exceeded for key: {}", key);
        let error_body = serde_json::json!({
            "error": {
                "message": "Rate limit exceeded",
                "type": "rate_limit_error",
                "code": "rate_limit_exceeded"
            }
        })
        .to_string();

        let mut response_builder = Response::builder()
            .status(StatusCode::TOO_MANY_REQUESTS)
            .header(axum::http::header::CONTENT_TYPE, "application/json");

        match build_rate_limit_headers(&info) {
            Ok(headers) => {
                for (name, value) in headers {
                    response_builder = response_builder.header(name, value);
                }
            }
            Err(e) => {
                error!("Failed to build rate limit headers: {}", e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }

        let response = response_builder.body(error_body.into()).map_err(|e| {
            error!("Failed to build rate limit response: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        return Ok(response);
    }

    let mut response = next.run(request).await;

    match build_rate_limit_headers(&info) {
        Ok(headers) => {
            for (name, value) in headers {
                response.headers_mut().insert(name, value);
            }
        }
        Err(e) => {
            error!("Failed to build rate limit headers: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_ip() {
        assert!(is_valid_ip("127.0.0.1"));
        assert!(is_valid_ip("::1"));
        assert!(is_valid_ip("192.168.1.1"));
        assert!(!is_valid_ip("invalid"));
        assert!(!is_valid_ip("not.an.ip"));
    }

    #[tokio::test]
    async fn test_rate_limiter_check() {
        let limiter = RateLimiter::new(10, 5);
        let key = "test-key";

        for _ in 0..10 {
            assert!(limiter.check(key).await);
        }

        assert!(!limiter.check(key).await);
    }

    #[tokio::test]
    async fn test_rate_limiter_refill() {
        let limiter = RateLimiter::new(10, 10);
        let key = "test-key";

        for _ in 0..10 {
            assert!(limiter.check(key).await);
        }

        assert!(!limiter.check(key).await);

        tokio::time::sleep(Duration::from_millis(150)).await;

        assert!(limiter.check(key).await);
    }

    #[test]
    fn test_build_rate_limit_headers() {
        let info = RateLimitInfo {
            limit: 100,
            remaining: 50,
            reset: 1234567890,
        };

        let headers = build_rate_limit_headers(&info).unwrap();
        assert_eq!(headers.len(), 3);
    }

    #[tokio::test]
    async fn test_rate_limiter_cleanup_expires_buckets() {
        let limiter = RateLimiter::new(10, 5);

        limiter.check("key1").await;
        limiter.check("key2").await;
        limiter.check("key3").await;

        let buckets = limiter.buckets.read().await;
        assert_eq!(buckets.len(), 3);
        drop(buckets);

        tokio::time::sleep(Duration::from_secs(1)).await;

        let mut last_cleanup = limiter.last_cleanup.write().await;
        *last_cleanup = Instant::now() - CLEANUP_INTERVAL - Duration::from_secs(1);
        drop(last_cleanup);

        let mut buckets = limiter.buckets.write().await;
        for (_, bucket) in buckets.iter_mut() {
            bucket.last_refill = Instant::now() - CLEANUP_INTERVAL * 3;
        }
        drop(buckets);

        limiter.cleanup_if_needed().await;

        let buckets = limiter.buckets.read().await;
        assert_eq!(buckets.len(), 0, "Expired buckets should be removed");
    }
}
