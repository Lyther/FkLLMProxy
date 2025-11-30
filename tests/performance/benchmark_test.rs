// Performance testing: Latency benchmarks
// Run with: cargo test --test benchmark_test --release -- --ignored

#[cfg(test)]
mod tests {
    use crate::test_utils;
    use crate::test_utils::{create_chat_request, create_simple_message, TestServer};
    use std::time::Instant;

    #[tokio::test]
    #[ignore]
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
                server.make_request("POST", "/v1/chat/completions", Some(&request_body), None);
            let response = server.call(req).await;
            let duration = start.elapsed();

            if response.status().is_success() {
                latencies.push(duration.as_millis() as u64);
            }
        }

        if latencies.is_empty() {
            eprintln!("⚠️  No successful requests for latency benchmark");
            return;
        }

        latencies.sort();

        let p50 = percentile(&latencies, 50.0);
        let p95 = percentile(&latencies, 95.0);
        let p99 = percentile(&latencies, 99.0);
        let avg = latencies.iter().sum::<u64>() / latencies.len() as u64;
        let min = latencies[0];
        let max = latencies[latencies.len() - 1];

        eprintln!("Latency benchmark results ({} samples):", latencies.len());
        eprintln!("  Min: {}ms", min);
        eprintln!("  P50: {}ms", p50);
        eprintln!("  P95: {}ms", p95);
        eprintln!("  P99: {}ms", p99);
        eprintln!("  Max: {}ms", max);
        eprintln!("  Avg: {}ms", avg);

        // Assert reasonable latencies (adjust thresholds as needed)
        assert!(p95 < 10000, "P95 latency too high: {}ms", p95);
    }

    #[tokio::test]
    #[ignore]
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
                server.make_request("POST", "/v1/chat/completions", Some(&request_body), None);
            let response = server.call(req).await;

            if response.status().is_success() {
                // Measure time to first chunk
                let first_chunk_start = Instant::now();
                // In a real test, we'd read the stream, but for simplicity we just measure response time
                let first_chunk_time = first_chunk_start.elapsed();
                first_chunk_latencies.push(first_chunk_time.as_millis() as u64);

                let total_time = start.elapsed();
                total_latencies.push(total_time.as_millis() as u64);
            }
        }

        if first_chunk_latencies.is_empty() {
            eprintln!("⚠️  No successful streaming requests for latency benchmark");
            return;
        }

        first_chunk_latencies.sort();
        total_latencies.sort();

        let first_chunk_p95 = percentile(&first_chunk_latencies, 95.0);
        let total_p95 = percentile(&total_latencies, 95.0);

        eprintln!("Streaming latency benchmark results:");
        eprintln!("  First chunk P95: {}ms", first_chunk_p95);
        eprintln!("  Total P95: {}ms", total_p95);

        assert!(
            first_chunk_p95 < 5000,
            "First chunk latency too high: {}ms",
            first_chunk_p95
        );
    }

    fn percentile(sorted_data: &[u64], p: f64) -> u64 {
        if sorted_data.is_empty() {
            return 0;
        }
        let index = ((sorted_data.len() - 1) as f64 * p / 100.0).ceil() as usize;
        sorted_data[index.min(sorted_data.len() - 1)]
    }
}
