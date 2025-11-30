use crate::state::AppState;
use axum::{
    extract::State,
    http::HeaderValue,
    response::{IntoResponse, Response},
    Json,
};

pub async fn metrics_handler(State(state): State<AppState>) -> impl IntoResponse {
    let stats = state.metrics.get_stats().await;
    (
        [(
            axum::http::header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static("no-cache, no-store, must-revalidate"),
        )],
        Json(stats),
    )
}

pub async fn prometheus_metrics_handler(State(state): State<AppState>) -> Response {
    let stats = state.metrics.get_stats().await;

    let mut prom_output = String::new();

    // Cache metrics
    prom_output.push_str(&format!(
        "# HELP cache_hits_total Total number of cache hits\n# TYPE cache_hits_total counter\ncache_hits_total {}\n",
        stats.cache_hits
    ));
    prom_output.push_str(&format!(
        "# HELP cache_misses_total Total number of cache misses\n# TYPE cache_misses_total counter\ncache_misses_total {}\n",
        stats.cache_misses
    ));
    prom_output.push_str(&format!(
        "# HELP cache_hit_rate Cache hit rate percentage\n# TYPE cache_hit_rate gauge\ncache_hit_rate {}\n",
        stats.cache_hit_rate
    ));

    // WAF metrics
    prom_output.push_str(&format!(
        "# HELP waf_blocks_total Total number of WAF blocks\n# TYPE waf_blocks_total counter\nwaf_blocks_total {}\n",
        stats.waf_blocks
    ));
    prom_output.push_str(&format!(
        "# HELP waf_block_rate WAF block rate percentage\n# TYPE waf_block_rate gauge\nwaf_block_rate {}\n",
        stats.waf_block_rate
    ));

    // Arkose metrics
    prom_output.push_str(&format!(
        "# HELP arkose_solves_total Total number of Arkose solves\n# TYPE arkose_solves_total counter\narkose_solves_total {}\n",
        stats.arkose_solves
    ));
    prom_output.push_str(&format!(
        "# HELP arkose_solve_time_ms Average Arkose solve time in milliseconds\n# TYPE arkose_solve_time_ms gauge\narkose_solve_time_ms {}\n",
        stats.avg_arkose_solve_time_ms as u64
    ));

    // Request metrics
    prom_output.push_str(&format!(
        "# HELP requests_total Total number of requests\n# TYPE requests_total counter\nrequests_total {}\n",
        stats.total_requests
    ));
    prom_output.push_str(&format!(
        "# HELP requests_failed_total Total number of failed requests\n# TYPE requests_failed_total counter\nrequests_failed_total {}\n",
        stats.failed_requests
    ));
    prom_output.push_str(&format!(
        "# HELP request_success_rate Request success rate percentage\n# TYPE request_success_rate gauge\nrequest_success_rate {}\n",
        stats.success_rate
    ));

    // Latency metrics
    prom_output.push_str(&format!(
        "# HELP request_latency_ms Average request latency in milliseconds\n# TYPE request_latency_ms gauge\nrequest_latency_ms {}\n",
        stats.avg_latency_ms as u64
    ));
    prom_output.push_str(&format!(
        "# HELP request_latency_p50_ms 50th percentile request latency in milliseconds\n# TYPE request_latency_p50_ms gauge\nrequest_latency_p50_ms {}\n",
        stats.p50_latency_ms
    ));
    prom_output.push_str(&format!(
        "# HELP request_latency_p95_ms 95th percentile request latency in milliseconds\n# TYPE request_latency_p95_ms gauge\nrequest_latency_p95_ms {}\n",
        stats.p95_latency_ms
    ));
    prom_output.push_str(&format!(
        "# HELP request_latency_p99_ms 99th percentile request latency in milliseconds\n# TYPE request_latency_p99_ms gauge\nrequest_latency_p99_ms {}\n",
        stats.p99_latency_ms
    ));

    Response::builder()
        .status(200)
        .header(
            "Content-Type",
            HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
        )
        .header(
            "Cache-Control",
            HeaderValue::from_static("no-cache, no-store, must-revalidate"),
        )
        .body(prom_output.into())
        .unwrap_or_else(|_| {
            // Fallback: if response construction fails, return 500
            Response::builder()
                .status(500)
                .body("Internal server error".into())
                .unwrap_or_else(|_| Response::new("Internal server error".into()))
        })
}
