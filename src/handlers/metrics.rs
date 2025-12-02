use crate::state::AppState;
use axum::{
    extract::State,
    http::HeaderValue,
    response::{IntoResponse, Response},
    Json,
};

const CACHE_CONTROL_NO_CACHE: &str = "no-cache, no-store, must-revalidate";
const PROMETHEUS_CONTENT_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";

fn format_prometheus_metric(
    name: &str,
    help: &str,
    metric_type: &str,
    value: impl std::fmt::Display,
) -> String {
    format!(
        "# HELP {} {}\n# TYPE {} {}\n{} {}\n",
        name, help, name, metric_type, name, value
    )
}

pub async fn metrics_handler(State(state): State<AppState>) -> impl IntoResponse {
    let stats = state.metrics.get_stats().await;
    (
        [(
            axum::http::header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static(CACHE_CONTROL_NO_CACHE),
        )],
        Json(stats),
    )
}

pub async fn prometheus_metrics_handler(State(state): State<AppState>) -> Response {
    let stats = state.metrics.get_stats().await;

    let mut prom_output = String::with_capacity(2048);

    prom_output.push_str(&format_prometheus_metric(
        "cache_hits_total",
        "Total number of cache hits",
        "counter",
        stats.cache_hits,
    ));
    prom_output.push_str(&format_prometheus_metric(
        "cache_misses_total",
        "Total number of cache misses",
        "counter",
        stats.cache_misses,
    ));
    prom_output.push_str(&format_prometheus_metric(
        "cache_hit_rate",
        "Cache hit rate percentage",
        "gauge",
        stats.cache_hit_rate,
    ));

    prom_output.push_str(&format_prometheus_metric(
        "waf_blocks_total",
        "Total number of WAF blocks",
        "counter",
        stats.waf_blocks,
    ));
    prom_output.push_str(&format_prometheus_metric(
        "waf_block_rate",
        "WAF block rate percentage",
        "gauge",
        stats.waf_block_rate,
    ));

    prom_output.push_str(&format_prometheus_metric(
        "arkose_solves_total",
        "Total number of Arkose solves",
        "counter",
        stats.arkose_solves,
    ));
    prom_output.push_str(&format_prometheus_metric(
        "arkose_solve_time_ms",
        "Average Arkose solve time in milliseconds",
        "gauge",
        stats.avg_arkose_solve_time_ms as u64,
    ));

    prom_output.push_str(&format_prometheus_metric(
        "requests_total",
        "Total number of requests",
        "counter",
        stats.total_requests,
    ));
    prom_output.push_str(&format_prometheus_metric(
        "requests_failed_total",
        "Total number of failed requests",
        "counter",
        stats.failed_requests,
    ));
    prom_output.push_str(&format_prometheus_metric(
        "request_success_rate",
        "Request success rate percentage",
        "gauge",
        stats.success_rate,
    ));

    prom_output.push_str(&format_prometheus_metric(
        "request_latency_ms",
        "Average request latency in milliseconds",
        "gauge",
        stats.avg_latency_ms as u64,
    ));
    prom_output.push_str(&format_prometheus_metric(
        "request_latency_p50_ms",
        "50th percentile request latency in milliseconds",
        "gauge",
        stats.p50_latency_ms,
    ));
    prom_output.push_str(&format_prometheus_metric(
        "request_latency_p95_ms",
        "95th percentile request latency in milliseconds",
        "gauge",
        stats.p95_latency_ms,
    ));
    prom_output.push_str(&format_prometheus_metric(
        "request_latency_p99_ms",
        "99th percentile request latency in milliseconds",
        "gauge",
        stats.p99_latency_ms,
    ));

    Response::builder()
        .status(200)
        .header(
            "Content-Type",
            HeaderValue::from_static(PROMETHEUS_CONTENT_TYPE),
        )
        .header(
            "Cache-Control",
            HeaderValue::from_static(CACHE_CONTROL_NO_CACHE),
        )
        .body(prom_output.into())
        .unwrap_or_else(|_| {
            Response::builder()
                .status(500)
                .body("Internal server error".into())
                .unwrap_or_else(|_| Response::new("Internal server error".into()))
        })
}
