use crate::state::AppState;
use axum::{
    extract::State,
    http::HeaderValue,
    response::{IntoResponse, Response},
    Json,
};

const CACHE_CONTROL_NO_CACHE: &str = "no-cache, no-store, must-revalidate";
const PROMETHEUS_CONTENT_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";

fn validate_metric_name(name: &str) -> String {
    // Prometheus metric names must match [a-zA-Z_:][a-zA-Z0-9_:]*
    // Replace invalid characters with underscores
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == ':' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn validate_metric_type(metric_type: &str) -> &str {
    // Valid Prometheus metric types: counter, gauge, histogram, summary
    match metric_type {
        "counter" | "gauge" | "histogram" | "summary" => metric_type,
        _ => {
            tracing::warn!(
                "Invalid Prometheus metric type: {}, defaulting to 'gauge'",
                metric_type
            );
            "gauge"
        }
    }
}

fn format_prometheus_metric(
    name: &str,
    help: &str,
    metric_type: &str,
    value: impl std::fmt::Display,
) -> String {
    // Fix: Validate inputs to prevent malformed Prometheus output
    let validated_name = validate_metric_name(name);
    let validated_type = validate_metric_type(metric_type);

    format!(
        "# HELP {} {}\n# TYPE {} {}\n{} {}\n",
        validated_name, help, validated_name, validated_type, validated_name, value
    )
}

fn validate_metric_value(value: f64) -> f64 {
    if value.is_nan() || value.is_infinite() || value < 0.0 {
        0.0
    } else {
        value
    }
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

    // Validate stats values before formatting
    let cache_hit_rate = validate_metric_value(stats.cache_hit_rate);
    let waf_block_rate = validate_metric_value(stats.waf_block_rate);
    let success_rate = validate_metric_value(stats.success_rate);
    let avg_arkose_solve_time_ms = validate_metric_value(stats.avg_arkose_solve_time_ms);
    let avg_latency_ms = validate_metric_value(stats.avg_latency_ms);

    // Define metrics using iterator to reduce duplication
    let metric_definitions: Vec<(&str, &str, &str, String)> = vec![
        (
            "cache_hits_total",
            "Total number of cache hits",
            "counter",
            stats.cache_hits.to_string(),
        ),
        (
            "cache_misses_total",
            "Total number of cache misses",
            "counter",
            stats.cache_misses.to_string(),
        ),
        (
            "cache_hit_rate",
            "Cache hit rate percentage",
            "gauge",
            format!("{:.2}", cache_hit_rate),
        ),
        (
            "waf_blocks_total",
            "Total number of WAF blocks",
            "counter",
            stats.waf_blocks.to_string(),
        ),
        (
            "waf_block_rate",
            "WAF block rate percentage",
            "gauge",
            format!("{:.2}", waf_block_rate),
        ),
        (
            "arkose_solves_total",
            "Total number of Arkose solves",
            "counter",
            stats.arkose_solves.to_string(),
        ),
        (
            "arkose_solve_time_ms",
            "Average Arkose solve time in milliseconds",
            "gauge",
            format!("{:.2}", avg_arkose_solve_time_ms),
        ),
        (
            "requests_total",
            "Total number of requests",
            "counter",
            stats.total_requests.to_string(),
        ),
        (
            "requests_failed_total",
            "Total number of failed requests",
            "counter",
            stats.failed_requests.to_string(),
        ),
        (
            "request_success_rate",
            "Request success rate percentage",
            "gauge",
            format!("{:.2}", success_rate),
        ),
        (
            "request_latency_ms",
            "Average request latency in milliseconds",
            "gauge",
            format!("{:.2}", avg_latency_ms),
        ),
        (
            "request_latency_p50_ms",
            "50th percentile request latency in milliseconds",
            "gauge",
            stats.p50_latency_ms.to_string(),
        ),
        (
            "request_latency_p95_ms",
            "95th percentile request latency in milliseconds",
            "gauge",
            stats.p95_latency_ms.to_string(),
        ),
        (
            "request_latency_p99_ms",
            "99th percentile request latency in milliseconds",
            "gauge",
            stats.p99_latency_ms.to_string(),
        ),
    ];

    // Calculate required capacity dynamically
    let estimated_size: usize = metric_definitions
        .iter()
        .map(|(name, help, _, _)| {
            name.len() + help.len() + 50 // Rough estimate per metric
        })
        .sum();
    let mut prom_output = String::with_capacity(estimated_size.max(2048));

    for (name, help, metric_type, value) in metric_definitions {
        prom_output.push_str(&format_prometheus_metric(name, help, metric_type, value));
    }

    match Response::builder()
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
    {
        Ok(response) => response,
        Err(e) => {
            tracing::error!("Failed to build Prometheus metrics response: {}", e);
            // Fix double unwrap_or_else: Use proper error handling
            Response::builder()
                .status(500)
                .body("Internal server error".into())
                .unwrap_or_else(|build_err| {
                    tracing::error!("Failed to build error response: {}", build_err);
                    // Last resort: create minimal response
                    let mut response = Response::new("Internal server error".into());
                    *response.status_mut() = axum::http::StatusCode::INTERNAL_SERVER_ERROR;
                    response
                })
        }
    }
}
