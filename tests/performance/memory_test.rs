// Performance testing: Memory profiling
// Run with: cargo test --test memory_test --release -- --ignored
// For detailed profiling: valgrind --tool=massif cargo test --test memory_test --release

#[cfg(test)]
mod tests {
    use crate::test_utils::{create_chat_request, create_simple_message, TestServer};

    #[tokio::test]
    #[ignore]
    async fn test_memory_usage_under_load() {
        let server = TestServer::new();
        let iterations = 100;

        // Baseline memory (rough estimate)
        // Skip test on non-Linux platforms
        let baseline = match get_memory_usage() {
            Some(b) => b,
            None => {
                eprintln!("⚠️  Memory test skipped: /proc/self/status only available on Linux");
                return;
            }
        };

        for i in 0..iterations {
            let request_body = create_chat_request(
                "gemini-2.5-flash",
                &create_simple_message("user", &format!("Request {}", i)),
                false,
            );

            let req =
                server.make_request("POST", "/v1/chat/completions", Some(&request_body), None);
            let _response = server.call(req).await;

            // Check memory every 10 requests
            if i % 10 == 0 {
                if let Some(current) = get_memory_usage() {
                    eprintln!("Memory at iteration {}: {} KB", i, current);
                }
            }
        }

        let final_memory = match get_memory_usage() {
            Some(m) => m,
            None => {
                eprintln!("⚠️  Could not read final memory usage");
                return;
            }
        };
        let memory_delta = final_memory.saturating_sub(baseline);

        eprintln!("Memory profiling results:");
        eprintln!("  Baseline: {} KB", baseline);
        eprintln!("  Final: {} KB", final_memory);
        eprintln!("  Delta: {} KB", memory_delta);

        // Assert no significant memory leak (adjust threshold as needed)
        assert!(
            memory_delta < 100_000,
            "Potential memory leak: {} KB increase",
            memory_delta
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_memory_usage_streaming() {
        let server = TestServer::new();
        let iterations = 50;

        // Skip test on non-Linux platforms
        let baseline = match get_memory_usage() {
            Some(b) => b,
            None => {
                eprintln!("⚠️  Memory test skipped: /proc/self/status only available on Linux");
                return;
            }
        };

        for i in 0..iterations {
            let request_body = create_chat_request(
                "gemini-2.5-flash",
                &create_simple_message("user", &format!("Count to {}", i)),
                true,
            );

            let req =
                server.make_request("POST", "/v1/chat/completions", Some(&request_body), None);
            let _response = server.call(req).await;

            if i % 10 == 0 {
                if let Some(current) = get_memory_usage() {
                    eprintln!("Memory at streaming iteration {}: {} KB", i, current);
                }
            }
        }

        let final_memory = match get_memory_usage() {
            Some(m) => m,
            None => {
                eprintln!("⚠️  Could not read final memory usage");
                return;
            }
        };
        let memory_delta = final_memory.saturating_sub(baseline);

        eprintln!("Streaming memory profiling results:");
        eprintln!("  Baseline: {} KB", baseline);
        eprintln!("  Final: {} KB", final_memory);
        eprintln!("  Delta: {} KB", memory_delta);

        assert!(
            memory_delta < 100_000,
            "Potential memory leak in streaming: {} KB increase",
            memory_delta
        );
    }

    fn get_memory_usage() -> Option<u64> {
        // Platform-specific: Only works on Linux
        // Returns None on other platforms to distinguish "no data" from "error"
        #[cfg(target_os = "linux")]
        {
            if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
                for line in status.lines() {
                    if line.starts_with("VmRSS:") {
                        if let Some(kb_str) = line.split_whitespace().nth(1) {
                            if let Ok(kb) = kb_str.parse::<u64>() {
                                return Some(kb);
                            }
                        }
                    }
                }
            }
            None
        }
        #[cfg(not(target_os = "linux"))]
        {
            // On non-Linux platforms, memory test is not supported
            // Consider using sysinfo crate for cross-platform support
            None
        }
    }
}
