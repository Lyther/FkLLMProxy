use serde::Serialize;
use std::sync::Arc;
use tokio::sync::RwLock;

const MAX_LATENCY_HISTORY: usize = 100;
const MAX_SORTED_DURATIONS: usize = 1000;

fn percentile(sorted_data: &[u64], p: f64) -> u64 {
    if sorted_data.is_empty() {
        return 0;
    }
    let index = (sorted_data.len() as f64 * p / 100.0).ceil() as usize - 1;
    sorted_data[index.min(sorted_data.len() - 1)]
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
    arkose_solve_times_ms: Arc<RwLock<Vec<u64>>>,
    total_requests: Arc<RwLock<u64>>,
    failed_requests: Arc<RwLock<u64>>,
    request_durations_ms: Arc<RwLock<Vec<u64>>>,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            cache_hits: Arc::new(RwLock::new(0)),
            cache_misses: Arc::new(RwLock::new(0)),
            waf_blocks: Arc::new(RwLock::new(0)),
            arkose_solves: Arc::new(RwLock::new(0)),
            arkose_solve_times_ms: Arc::new(RwLock::new(Vec::new())),
            total_requests: Arc::new(RwLock::new(0)),
            failed_requests: Arc::new(RwLock::new(0)),
            request_durations_ms: Arc::new(RwLock::new(Vec::new())),
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
        times.push(duration_ms);
        if times.len() > MAX_LATENCY_HISTORY {
            times.remove(0);
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
        durations.push(duration_ms);
        if durations.len() > MAX_SORTED_DURATIONS {
            durations.remove(0);
        }
    }

    pub async fn get_stats(&self) -> MetricsStats {
        let cache_hits = *self.cache_hits.read().await;
        let cache_misses = *self.cache_misses.read().await;
        let total_cache = cache_hits + cache_misses;
        let cache_hit_rate = if total_cache > 0 {
            (cache_hits as f64 / total_cache as f64) * 100.0
        } else {
            0.0
        };

        let waf_blocks = *self.waf_blocks.read().await;
        let total_requests = *self.total_requests.read().await;
        let waf_block_rate = if total_requests > 0 {
            (waf_blocks as f64 / total_requests as f64) * 100.0
        } else {
            0.0
        };

        let arkose_solves = *self.arkose_solves.read().await;
        let arkose_times = self.arkose_solve_times_ms.read().await;
        let avg_arkose_solve_time_ms = if !arkose_times.is_empty() {
            arkose_times.iter().sum::<u64>() as f64 / arkose_times.len() as f64
        } else {
            0.0
        };

        let failed_requests = *self.failed_requests.read().await;
        let success_rate = if total_requests > 0 {
            ((total_requests - failed_requests) as f64 / total_requests as f64) * 100.0
        } else {
            0.0
        };

        let durations = self.request_durations_ms.read().await;
        let mut sorted_durations: Vec<u64> = durations.iter().copied().collect();
        sorted_durations.sort();
        let p50 = percentile(&sorted_durations, 50.0);
        let p95 = percentile(&sorted_durations, 95.0);
        let p99 = percentile(&sorted_durations, 99.0);
        let avg_latency = if !sorted_durations.is_empty() {
            sorted_durations.iter().sum::<u64>() as f64 / sorted_durations.len() as f64
        } else {
            0.0
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
