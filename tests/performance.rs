// Performance Test Module
// To run: cargo test --test performance

#[cfg(test)]
#[path = "integration/test_utils.rs"]
mod test_utils;

#[cfg(test)]
#[path = "performance/benchmark_test.rs"]
mod benchmark_test;
#[cfg(test)]
#[path = "performance/load_test.rs"]
mod load_test;
#[cfg(test)]
#[path = "performance/memory_test.rs"]
mod memory_test;
