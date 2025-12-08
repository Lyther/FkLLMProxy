use num_traits::ToPrimitive;
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::RwLock;

const MAX_LATENCY_HISTORY: usize = 100;
const MAX_SORTED_DURATIONS: usize = 1000;

fn to_f64(value: u64) -> f64 {
    value.to_f64().unwrap_or(f64::MAX)
}

fn usize_to_f64(value: usize) -> f64 {
    value.to_f64().unwrap_or(f64::MAX)
}

fn percentile(sorted_data: &[u64], p: u8) -> u64 {
    if sorted_data.is_empty() {
        return 0;
    }

    // Use integer math to avoid float-to-int casts and truncation issues.
    let clamped = u128::from(p.min(100));
    let len = sorted_data.len() as u128;
    let raw_index = (len * clamped).div_ceil(100);
    let safe_index = raw_index.saturating_sub(1).min(len.saturating_sub(1));
    let index = usize::try_from(safe_index).unwrap_or(sorted_data.len().saturating_sub(1));

    sorted_data.get(index).copied().unwrap_or_default()
}

#[derive(Clone, Default, Serialize)]
pub struct MetricsStats {
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_hit_rate: f64,
    pub waf_blocks: u64,
    pub waf_block_rate: f64,
    pub arkose_solves: u64,
    pub avg_arkose_solve_time_ms: f64,
    pub total_requests: u64,
    pub failed_requests: u64,
    pub success_rate: f64,
    pub avg_latency_ms: f64,
    pub p50_latency_ms: u64,
    pub p95_latency_ms: u64,
    pub p99_latency_ms: u64,
}

pub struct Metrics {
    cache_hits: Arc<RwLock<u64>>,
    cache_misses: Arc<RwLock<u64>>,
    waf_blocks: Arc<RwLock<u64>>,
    arkose_solves: Arc<RwLock<u64>>,
    // Fix inefficient remove(0): Use VecDeque for O(1) removal from front
    arkose_solve_times_ms: Arc<RwLock<VecDeque<u64>>>,
    total_requests: Arc<RwLock<u64>>,
    failed_requests: Arc<RwLock<u64>>,
    // Fix inefficient remove(0): Use VecDeque for O(1) removal from front
    request_durations_ms: Arc<RwLock<VecDeque<u64>>>,
}

impl Metrics {
    #[must_use]
    pub fn new() -> Self {
        Self {
            cache_hits: Arc::new(RwLock::new(0)),
            cache_misses: Arc::new(RwLock::new(0)),
            waf_blocks: Arc::new(RwLock::new(0)),
            arkose_solves: Arc::new(RwLock::new(0)),
            arkose_solve_times_ms: Arc::new(RwLock::new(VecDeque::new())),
            total_requests: Arc::new(RwLock::new(0)),
            failed_requests: Arc::new(RwLock::new(0)),
            request_durations_ms: Arc::new(RwLock::new(VecDeque::new())),
        }
    }

    pub async fn record_cache_hit(&self) {
        *self.cache_hits.write().await += 1;
    }

    pub async fn record_cache_miss(&self) {
        *self.cache_misses.write().await += 1;
    }

    pub async fn record_waf_block(&self) {
        *self.waf_blocks.write().await += 1;
    }

    pub async fn record_arkose_solve(&self, duration_ms: u64) {
        *self.arkose_solves.write().await += 1;
        let mut times = self.arkose_solve_times_ms.write().await;
        times.push_back(duration_ms);
        // Fix inefficient remove(0): VecDeque::pop_front() is O(1) vs Vec::remove(0) which is O(n)
        if times.len() > MAX_LATENCY_HISTORY {
            times.pop_front();
        }
    }

    pub async fn record_request(&self, success: bool) {
        *self.total_requests.write().await += 1;
        if !success {
            *self.failed_requests.write().await += 1;
        }
    }

    pub async fn record_request_duration(&self, duration_ms: u64) {
        let mut durations = self.request_durations_ms.write().await;
        durations.push_back(duration_ms);
        // Fix inefficient remove(0): VecDeque::pop_front() is O(1) vs Vec::remove(0) which is O(n)
        if durations.len() > MAX_SORTED_DURATIONS {
            durations.pop_front();
        }
    }

    #[must_use]
    pub async fn get_stats(&self) -> MetricsStats {
        let cache_hits = *self.cache_hits.read().await;
        let cache_misses = *self.cache_misses.read().await;
        let total_cache = cache_hits + cache_misses;
        let cache_hit_rate = if total_cache > 0 {
            to_f64(cache_hits) / to_f64(total_cache) * 100.0
        } else {
            0.0
        };

        let waf_blocks = *self.waf_blocks.read().await;
        let total_requests = *self.total_requests.read().await;
        let waf_block_rate = if total_requests > 0 {
            to_f64(waf_blocks) / to_f64(total_requests) * 100.0
        } else {
            0.0
        };

        let arkose_solves = *self.arkose_solves.read().await;
        let arkose_times = self.arkose_solve_times_ms.read().await;
        let avg_arkose_solve_time_ms = if arkose_times.is_empty() {
            0.0
        } else {
            let total: f64 = arkose_times.iter().map(|&x| to_f64(x)).sum();
            total / usize_to_f64(arkose_times.len())
        };

        let failed_requests = *self.failed_requests.read().await;
        let success_rate = if total_requests > 0 {
            to_f64(total_requests - failed_requests) / to_f64(total_requests) * 100.0
        } else {
            0.0
        };

        let durations = self.request_durations_ms.read().await;
        // Fix performance issue: Clone and sort on every get_stats() call
        // TODO: Consider maintaining sorted state or using incremental sorting
        // For now, we clone and sort which is O(n log n) but acceptable for small datasets (< 1000 items)
        let mut sorted_durations: Vec<u64> = durations.iter().copied().collect();
        sorted_durations.sort_unstable(); // Use sort_unstable for better performance
        let p50 = percentile(&sorted_durations, 50);
        let p95 = percentile(&sorted_durations, 95);
        let p99 = percentile(&sorted_durations, 99);
        let avg_latency = if sorted_durations.is_empty() {
            0.0
        } else {
            let total: f64 = sorted_durations.iter().map(|&x| to_f64(x)).sum();
            total / usize_to_f64(sorted_durations.len())
        };

        MetricsStats {
            cache_hits,
            cache_misses,
            cache_hit_rate,
            waf_blocks,
            waf_block_rate,
            arkose_solves,
            avg_arkose_solve_time_ms,
            total_requests,
            failed_requests,
            success_rate,
            avg_latency_ms: avg_latency,
            p50_latency_ms: p50,
            p95_latency_ms: p95,
            p99_latency_ms: p99,
        }
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}
