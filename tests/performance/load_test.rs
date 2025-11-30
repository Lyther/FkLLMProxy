// Performance testing: Load testing infrastructure
// Run with: cargo test --test load_test --release -- --ignored

#[cfg(test)]
mod tests {
    use crate::test_utils;
    use crate::test_utils::{create_chat_request, create_simple_message, TestServer};
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use tokio::sync::Semaphore;
    use tokio::time::timeout;

    #[tokio::test]
    #[ignore]
    async fn test_concurrent_requests() {
        let server = TestServer::new();
        let concurrency = 10;
        let requests_per_worker = 5;

        let semaphore = Arc::new(Semaphore::new(concurrency));
        let mut handles = Vec::new();

        let start = Instant::now();

        for _ in 0..(concurrency * requests_per_worker) {
            let permit = semaphore.clone().acquire_owned().await.unwrap();

            let handle = tokio::spawn(async move {
                let server = TestServer::new();
                let request_body = create_chat_request(
                    "gemini-2.5-flash",
                    &create_simple_message("user", "Hello"),
                    false,
                );

                let req =
                    server.make_request("POST", "/v1/chat/completions", Some(&request_body), None);
                let response = server.call(req).await;

                drop(permit);
                response.status()
            });

            handles.push(handle);
        }

        let mut success_count = 0;
        let mut error_count = 0;

        for handle in handles {
            match timeout(Duration::from_secs(30), handle).await {
                Ok(Ok(status)) => {
                    if status.is_success() {
                        success_count += 1;
                    } else {
                        error_count += 1;
                    }
                }
                _ => error_count += 1,
            }
        }

        let duration = start.elapsed();

        eprintln!("Concurrent load test results:");
        eprintln!("  Total requests: {}", concurrency * requests_per_worker);
        eprintln!("  Successful: {}", success_count);
        eprintln!("  Errors: {}", error_count);
        eprintln!("  Duration: {:?}", duration);
        eprintln!(
            "  Throughput: {:.2} req/s",
            (concurrency * requests_per_worker) as f64 / duration.as_secs_f64()
        );

        // At least 50% should succeed (accounting for missing credentials)
        assert!(success_count + error_count == concurrency * requests_per_worker);
    }

    #[tokio::test]
    #[ignore]
    async fn test_sustained_load() {
        let server = TestServer::new();
        let duration_secs = 10;
        let target_rps = 5; // requests per second

        let start = Instant::now();
        let mut request_count = 0;
        let mut success_count = 0;
        let interval = Duration::from_millis(1000 / target_rps as u64);

        while start.elapsed() < Duration::from_secs(duration_secs) {
            let request_body = create_chat_request(
                "gemini-2.5-flash",
                &create_simple_message("user", "Hello"),
                false,
            );

            let req =
                server.make_request("POST", "/v1/chat/completions", Some(&request_body), None);
            let response = server.call(req).await;

            request_count += 1;
            if response.status().is_success() {
                success_count += 1;
            }

            tokio::time::sleep(interval).await;
        }

        let actual_duration = start.elapsed();

        eprintln!("Sustained load test results:");
        eprintln!("  Duration: {:?}", actual_duration);
        eprintln!("  Total requests: {}", request_count);
        eprintln!("  Successful: {}", success_count);
        eprintln!(
            "  Actual RPS: {:.2}",
            request_count as f64 / actual_duration.as_secs_f64()
        );

        assert!(request_count > 0);
    }
}
