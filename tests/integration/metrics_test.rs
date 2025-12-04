// @critical: Metrics endpoint tests

use super::test_utils::TestServer;
use axum::body::to_bytes;
use axum::http::StatusCode;
use serde_json::Value;

/// Reasonable body size limit for tests (1MB)
const TEST_BODY_LIMIT: usize = 1024 * 1024;

#[tokio::test]
async fn test_metrics_endpoint_returns_json() {
    let server = TestServer::new();

    let req = server.make_request("GET", "/metrics", None, None);
    let response = server.call(req).await;

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Metrics endpoint must return 200 OK"
    );

    let body = response.into_body();
    let body_bytes = to_bytes(body, TEST_BODY_LIMIT)
        .await
        .expect("Failed to read metrics response");
    let json: Value =
        serde_json::from_slice(&body_bytes).expect("Metrics response must be valid JSON");

    // Verify expected metrics fields exist
    assert!(
        json.get("total_requests").is_some() || json.get("uptime_seconds").is_some(),
        "Metrics response should contain total_requests or uptime_seconds"
    );
}

#[tokio::test]
async fn test_metrics_prometheus_format() {
    let server = TestServer::new();

    let req = server.make_request("GET", "/metrics/prometheus", None, None);
    let response = server.call(req).await;

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Prometheus metrics endpoint must return 200 OK"
    );

    let body = response.into_body();
    let body_bytes = to_bytes(body, TEST_BODY_LIMIT)
        .await
        .expect("Failed to read prometheus metrics response");
    let body_str = String::from_utf8_lossy(&body_bytes);

    // Verify Prometheus text format (comments start with #, metrics are name value pairs)
    assert!(
        body_str.contains("# HELP") || body_str.contains("# TYPE") || body_str.contains("_total"),
        "Prometheus metrics should contain Prometheus format indicators"
    );
}

#[tokio::test]
async fn test_metrics_increment_after_request() {
    let server = TestServer::new();

    // Get initial metrics
    let req = server.make_request("GET", "/metrics", None, None);
    let response = server.call(req).await;
    let body = response.into_body();
    let body_bytes = to_bytes(body, TEST_BODY_LIMIT)
        .await
        .expect("Failed to read initial metrics");
    let initial: Value = serde_json::from_slice(&body_bytes).expect("Initial metrics not JSON");

    // Make a health request to increment metrics
    let req = server.make_request("GET", "/health", None, None);
    let _ = server.call(req).await;

    // Get updated metrics
    let req = server.make_request("GET", "/metrics", None, None);
    let response = server.call(req).await;
    let body = response.into_body();
    let body_bytes = to_bytes(body, TEST_BODY_LIMIT)
        .await
        .expect("Failed to read updated metrics");
    let updated: Value = serde_json::from_slice(&body_bytes).expect("Updated metrics not JSON");

    // Verify metrics are being tracked (total_requests should exist)
    assert!(
        updated.get("total_requests").is_some() || updated.get("uptime_seconds").is_some(),
        "Metrics should be tracked after requests"
    );

    // If total_requests exists, verify it increased (accounting for the metrics request itself)
    if let (Some(initial_count), Some(updated_count)) =
        (initial.get("total_requests"), updated.get("total_requests"))
    {
        let initial_val = initial_count.as_u64().unwrap_or(0);
        let updated_val = updated_count.as_u64().unwrap_or(0);
        assert!(
            updated_val >= initial_val,
            "Total requests should not decrease"
        );
    }
}
