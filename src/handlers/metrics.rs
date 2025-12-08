use crate::openai::metrics::MetricsStats;
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
        "# HELP {validated_name} {help}\n# TYPE {validated_name} {validated_type}\n{validated_name} {value}\n"
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
    let metrics_data = state.metrics.get_stats().await;
    (
        [(
            axum::http::header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static(CACHE_CONTROL_NO_CACHE),
        )],
        Json(metrics_data),
    )
}

fn validate_metrics_stats(stats: &MetricsStats) -> ValidatedMetricsStats {
    ValidatedMetricsStats {
        cache_hit_rate: validate_metric_value(stats.cache_hit_rate),
        waf_block_rate: validate_metric_value(stats.waf_block_rate),
        success_rate: validate_metric_value(stats.success_rate),
        avg_arkose_solve_time_ms: validate_metric_value(stats.avg_arkose_solve_time_ms),
        avg_latency_ms: validate_metric_value(stats.avg_latency_ms),
    }
}

struct ValidatedMetricsStats {
    cache_hit_rate: f64,
    waf_block_rate: f64,
    success_rate: f64,
    avg_arkose_solve_time_ms: f64,
    avg_latency_ms: f64,
}

fn create_metric_definitions(
    stats: &MetricsStats,
    validated: &ValidatedMetricsStats,
) -> Vec<(&'static str, &'static str, &'static str, String)> {
    let mut metrics = Vec::with_capacity(14);

    // Cache metrics
    metrics.extend([
        create_counter_metric(
            "cache_hits_total",
            "Total number of cache hits",
            stats.cache_hits,
        ),
        create_counter_metric(
            "cache_misses_total",
            "Total number of cache misses",
            stats.cache_misses,
        ),
        create_gauge_metric(
            "cache_hit_rate",
            "Cache hit rate percentage",
            validated.cache_hit_rate,
        ),
    ]);

    // WAF metrics
    metrics.extend([
        create_counter_metric(
            "waf_blocks_total",
            "Total number of WAF blocks",
            stats.waf_blocks,
        ),
        create_gauge_metric(
            "waf_block_rate",
            "WAF block rate percentage",
            validated.waf_block_rate,
        ),
    ]);

    // Arkose metrics
    metrics.extend([
        create_counter_metric(
            "arkose_solves_total",
            "Total number of Arkose solves",
            stats.arkose_solves,
        ),
        create_gauge_metric(
            "arkose_solve_time_ms",
            "Average Arkose solve time in milliseconds",
            validated.avg_arkose_solve_time_ms,
        ),
    ]);

    // Request metrics
    metrics.extend([
        create_counter_metric(
            "requests_total",
            "Total number of requests",
            stats.total_requests,
        ),
        create_counter_metric(
            "requests_failed_total",
            "Total number of failed requests",
            stats.failed_requests,
        ),
        create_gauge_metric(
            "request_success_rate",
            "Request success rate percentage",
            validated.success_rate,
        ),
        create_gauge_metric(
            "request_latency_ms",
            "Average request latency in milliseconds",
            validated.avg_latency_ms,
        ),
        create_simple_gauge_metric(
            "request_latency_p50_ms",
            "50th percentile request latency in milliseconds",
            stats.p50_latency_ms,
        ),
        create_simple_gauge_metric(
            "request_latency_p95_ms",
            "95th percentile request latency in milliseconds",
            stats.p95_latency_ms,
        ),
        create_simple_gauge_metric(
            "request_latency_p99_ms",
            "99th percentile request latency in milliseconds",
            stats.p99_latency_ms,
        ),
    ]);

    metrics
}

fn create_counter_metric(
    name: &'static str,
    help: &'static str,
    value: impl std::fmt::Display,
) -> (&'static str, &'static str, &'static str, String) {
    (name, help, "counter", value.to_string())
}

fn create_gauge_metric(
    name: &'static str,
    help: &'static str,
    value: impl std::fmt::Display,
) -> (&'static str, &'static str, &'static str, String) {
    (name, help, "gauge", format!("{value:.2}"))
}

fn create_simple_gauge_metric(
    name: &'static str,
    help: &'static str,
    value: impl std::fmt::Display,
) -> (&'static str, &'static str, &'static str, String) {
    (name, help, "gauge", value.to_string())
}

fn build_prometheus_output(metric_definitions: &[(&str, &str, &str, String)]) -> String {
    let estimated_size: usize = metric_definitions
        .iter()
        .map(|(name, help, _, _)| name.len() + help.len() + 50)
        .sum();
    let mut prom_output = String::with_capacity(estimated_size.max(2048));

    for (name, help, metric_type, value) in metric_definitions {
        prom_output.push_str(&format_prometheus_metric(name, help, metric_type, value));
    }

    prom_output
}

fn build_prometheus_response(body: String) -> Result<Response, axum::http::Error> {
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
        .body(body.into())
}

fn build_error_response() -> Response {
    match Response::builder()
        .status(500)
        .body("Internal server error".into())
    {
        Ok(response) => response,
        Err(build_err) => {
            tracing::error!("Failed to build error response: {}", build_err);
            let mut response = Response::new("Internal server error".into());
            *response.status_mut() = axum::http::StatusCode::INTERNAL_SERVER_ERROR;
            response
        }
    }
}

pub async fn prometheus_metrics_handler(State(state): State<AppState>) -> Response {
    let metrics_stats = state.metrics.get_stats().await;
    let validated_stats = validate_metrics_stats(&metrics_stats);
    let metric_definitions = create_metric_definitions(&metrics_stats, &validated_stats);
    let prom_output = build_prometheus_output(&metric_definitions);

    match build_prometheus_response(prom_output) {
        Ok(response) => response,
        Err(e) => {
            tracing::error!("Failed to build Prometheus metrics response: {}", e);
            build_error_response()
        }
    }
}
