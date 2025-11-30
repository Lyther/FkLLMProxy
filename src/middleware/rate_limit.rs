use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;
use tracing::warn;

#[derive(Clone)]
pub struct RateLimiter {
    buckets: Arc<RwLock<HashMap<String, TokenBucket>>>,
    capacity: u32,
    refill_rate: Duration,
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
        }
    }

    pub async fn check(&self, key: &str) -> bool {
        let mut buckets = self.buckets.write().await;
        let bucket = buckets
            .entry(key.to_string())
            .or_insert_with(|| TokenBucket {
                tokens: self.capacity,
                last_refill: Instant::now(),
            });

        let now = Instant::now();
        let elapsed = now.duration_since(bucket.last_refill);
        let tokens_to_add = (elapsed.as_secs_f64() / self.refill_rate.as_secs_f64()) as u32;

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
        let tokens_to_add = (elapsed.as_secs_f64() / self.refill_rate.as_secs_f64()) as u32;
        let current_tokens = (bucket.tokens + tokens_to_add).min(self.capacity);

        // Calculate reset time (when bucket will be full)
        let tokens_needed = self.capacity.saturating_sub(current_tokens);
        let reset_seconds = if tokens_needed > 0 {
            (tokens_needed as f64 * self.refill_rate.as_secs_f64()).ceil() as u64
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

pub async fn rate_limit_middleware(
    State(limiter): State<RateLimiter>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Rate limit key: Use authorization header if present, otherwise use connection IP
    // SECURITY: x-forwarded-for can be spoofed, so we only use it as fallback
    // In production behind a trusted proxy, extract real IP from trusted headers
    let key = request
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            // Extract IP from connection or x-forwarded-for
            // Note: In production, validate x-forwarded-for against trusted proxy list
            let forwarded = request
                .headers()
                .get("x-forwarded-for")
                .and_then(|h| h.to_str().ok())
                .and_then(|s| s.split(',').next())
                .map(|s| s.trim().to_string());

            // Use connection remote_addr if available (requires extension)
            // For now, use x-forwarded-for with warning that it's spoofable
            forwarded.unwrap_or_else(|| "unknown".to_string())
        });

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
        let response = Response::builder()
            .status(StatusCode::TOO_MANY_REQUESTS)
            .header("X-RateLimit-Limit", info.limit.to_string())
            .header("X-RateLimit-Remaining", "0")
            .header("X-RateLimit-Reset", info.reset.to_string())
            .header(axum::http::header::CONTENT_TYPE, "application/json")
            .body(error_body.into())
            .map_err(|e| {
                warn!("Failed to build rate limit response: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        return Ok(response);
    }

    let mut response = next.run(request).await;

    // Header construction can fail with invalid characters, but rate limit values are numeric
    // Log warning but don't fail the request if header construction fails
    if let Ok(header_value) = axum::http::HeaderValue::from_str(&info.limit.to_string()) {
        response
            .headers_mut()
            .insert("X-RateLimit-Limit", header_value);
    } else {
        warn!("Failed to construct X-RateLimit-Limit header");
    }

    if let Ok(header_value) =
        axum::http::HeaderValue::from_str(&info.remaining.saturating_sub(1).to_string())
    {
        response
            .headers_mut()
            .insert("X-RateLimit-Remaining", header_value);
    } else {
        warn!("Failed to construct X-RateLimit-Remaining header");
    }

    if let Ok(header_value) = axum::http::HeaderValue::from_str(&info.reset.to_string()) {
        response
            .headers_mut()
            .insert("X-RateLimit-Reset", header_value);
    } else {
        warn!("Failed to construct X-RateLimit-Reset header");
    }

    Ok(response)
}
