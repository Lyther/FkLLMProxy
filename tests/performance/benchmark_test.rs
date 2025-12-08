// Performance testing: Latency benchmarks
// Run with: cargo test --test benchmark_test --release -- --ignored

#[cfg(test)]
mod tests {
    use crate::test_utils::{create_chat_request, create_simple_message, TestServer};
    use num_traits::ToPrimitive;
    use std::time::Instant;

    #[tokio::test]
    #[ignore = "Performance benchmark - requires real API credentials"]
    async fn test_latency_p50_p95_p99() {
        let server = TestServer::new();
        let iterations = 50;
        let mut latencies = Vec::new();

        for _ in 0..iterations {
            let request_body = create_chat_request(
                "gemini-2.5-flash",
                &create_simple_message("user", "Hello"),
                false,
            );

            let start = Instant::now();
            let req =
                TestServer::make_request("POST", "/v1/chat/completions", Some(&request_body), None);
            let response = server.call(req).await;
            let duration = start.elapsed();

            if response.status().is_success() {
                let Ok(millis) = u64::try_from(duration.as_millis()) else {
                    eprintln!("Warning: Duration too large for u64, skipping sample");
                    continue;
                };
                latencies.push(millis);
            }
        }

        assert!(
            !latencies.is_empty(),
            "No successful requests for latency benchmark - test cannot provide meaningful results"
        );

        latencies.sort_unstable();

        let p50 = percentile(&latencies, 50);
        let p95 = percentile(&latencies, 95);
        let p99 = percentile(&latencies, 99);
        // Calculate average with f64 precision (acceptable for benchmark statistics)
        let sum: u64 = latencies.iter().sum();
        let count = latencies.len();
        let avg = sum.to_f64().unwrap_or(f64::MAX) / count.to_f64().unwrap_or(1.0);
        let min = latencies[0];
        let max = latencies[latencies.len() - 1];

        eprintln!(
            "Latency benchmark results ({sample_count} samples):",
            sample_count = latencies.len()
        );
        eprintln!("  Min: {min}ms");
        eprintln!("  P50: {p50}ms");
        eprintln!("  P95: {p95}ms");
        eprintln!("  P99: {p99}ms");
        eprintln!("  Max: {max}ms");
        eprintln!("  Avg: {avg:.2}ms");

        // Assert reasonable latencies (adjust thresholds as needed)
        assert!(p95 < 10000, "P95 latency too high: {p95}ms");
    }

    #[tokio::test]
    #[ignore = "Performance benchmark - requires real API credentials"]
    async fn test_streaming_latency() {
        let server = TestServer::new();
        let iterations = 20;
        let mut first_chunk_latencies = Vec::new();
        let mut total_latencies = Vec::new();

        for _ in 0..iterations {
            let request_body = create_chat_request(
                "gemini-2.5-flash",
                &create_simple_message("user", "Count to 5"),
                true,
            );

            let start = Instant::now();
            let req =
                TestServer::make_request("POST", "/v1/chat/completions", Some(&request_body), None);
            let response = server.call(req).await;

            if response.status().is_success() {
                // Fix fake first chunk latency: Actually read the stream to measure first chunk
                use axum::body::to_bytes;
                let body = response.into_body();
                let first_chunk_start = Instant::now();

                // Read first chunk from SSE stream
                let body_bytes = to_bytes(body, 1024 * 1024).await.expect("Should read body");
                let first_chunk_time = first_chunk_start.elapsed();

                // Parse first data line from SSE
                let body_str = String::from_utf8_lossy(&body_bytes);
                let lines: Vec<&str> = body_str.lines().collect();
                let has_data = lines.iter().any(|l| l.starts_with("data: "));

                if has_data {
                    let Ok(millis) = u64::try_from(first_chunk_time.as_millis()) else {
                        eprintln!(
                            "Warning: First chunk duration too large for u64, skipping sample"
                        );
                        continue;
                    };
                    first_chunk_latencies.push(millis);
                }

                let total_time = start.elapsed();
                let Ok(total_millis) = u64::try_from(total_time.as_millis()) else {
                    eprintln!("Warning: Total duration too large for u64, skipping sample");
                    continue;
                };
                total_latencies.push(total_millis);
            }
        }

        assert!(!first_chunk_latencies.is_empty(), "No successful streaming requests with data chunks for latency benchmark - test cannot provide meaningful results");

        first_chunk_latencies.sort_unstable();
        total_latencies.sort_unstable();

        let first_chunk_p95 = percentile(&first_chunk_latencies, 95);
        let total_p95 = percentile(&total_latencies, 95);

        eprintln!("Streaming latency benchmark results:");
        eprintln!("  First chunk P95: {first_chunk_p95}ms");
        eprintln!("  Total P95: {total_p95}ms");

        assert!(
            first_chunk_p95 < 5000,
            "First chunk latency too high: {first_chunk_p95}ms"
        );
    }

    fn percentile(sorted_data: &[u64], p: u8) -> u64 {
        if sorted_data.is_empty() {
            return 0;
        }
        let clamped = u128::from(p.min(100));
        let len = u128::try_from(sorted_data.len().saturating_sub(1)).unwrap_or(0);
        let index = (len * clamped).div_ceil(100);
        let safe_index = usize::try_from(index).unwrap_or(sorted_data.len().saturating_sub(1));
        sorted_data[safe_index.min(sorted_data.len().saturating_sub(1))]
    }
}
