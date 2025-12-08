use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{error, info, warn};

#[derive(Clone, Copy, Debug)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug, thiserror::Error)]
#[error("Circuit breaker is open")]
pub struct CircuitOpenError;

pub struct CircuitBreaker {
    state: Arc<RwLock<CircuitState>>,
    failure_count: Arc<RwLock<u32>>,
    success_count: Arc<RwLock<u32>>,
    last_failure: Arc<RwLock<Option<Instant>>>,
    failure_threshold: u32,
    success_threshold: u32,
    timeout: Duration,
}

#[derive(Debug, Clone, Copy)]
pub struct CircuitBreakerStats {
    pub state: CircuitState,
    pub failure_count: u32,
    pub success_count: u32,
    pub failure_threshold: u32,
    pub success_threshold: u32,
    pub timeout_secs: u64,
}

impl CircuitBreaker {
    #[must_use]
    pub fn new(failure_threshold: u32, timeout_secs: u64, success_threshold: u32) -> Self {
        // Validate parameters to prevent invalid state
        let failure_threshold = failure_threshold.max(1);
        let success_threshold = success_threshold.max(1);
        let timeout_secs = timeout_secs.max(1);

        Self {
            state: Arc::new(RwLock::new(CircuitState::Closed)),
            failure_count: Arc::new(RwLock::new(0)),
            success_count: Arc::new(RwLock::new(0)),
            last_failure: Arc::new(RwLock::new(None)),
            failure_threshold,
            success_threshold,
            timeout: Duration::from_secs(timeout_secs),
        }
    }

    /// Execute a function with circuit breaker protection.
    ///
    /// # Errors
    ///
    /// Returns the original error from the function `f`, or `CircuitOpenError` if the circuit is open.
    pub async fn call<F, T, E>(&self, f: F) -> Result<T, E>
    where
        F: std::future::Future<Output = Result<T, E>>,
        E: From<CircuitOpenError>,
    {
        // Fix race condition: acquire write lock immediately to check and transition atomically
        {
            let mut state_guard = self.state.write().await;
            if matches!(*state_guard, CircuitState::Open) {
                // Fix inconsistent state: if Open but last_failure is None, initialize it
                // This handles the case where state was manually set incorrectly
                let last_failure = {
                    let mut last_failure_guard = self.last_failure.write().await;
                    if last_failure_guard.is_none() {
                        warn!("Circuit breaker: Open state but no last_failure timestamp, initializing");
                        *last_failure_guard = Some(Instant::now());
                    }
                    *last_failure_guard
                };

                if let Some(last) = last_failure {
                    if last.elapsed() >= self.timeout {
                        // Double-check state is still Open before transitioning
                        if matches!(*state_guard, CircuitState::Open) {
                            info!("Circuit breaker: Transitioning to HalfOpen");
                            *state_guard = CircuitState::HalfOpen;
                            *self.failure_count.write().await = 0;
                            *self.success_count.write().await = 0;
                        }
                    } else {
                        drop(state_guard); // Release lock before returning
                        warn!("Circuit breaker: Open, rejecting request");
                        return Err(CircuitOpenError.into());
                    }
                } else {
                    // This should never happen after the fix above, but defensive check
                    drop(state_guard);
                    warn!("Circuit breaker: Open, rejecting request (no failure timestamp)");
                    return Err(CircuitOpenError.into());
                }
            }
        }

        let result = f.await;

        {
            let mut state_guard = self.state.write().await;
            // Fix redundant read: use *state_guard directly instead of reading into current_state
            if result.is_ok() {
                if matches!(*state_guard, CircuitState::HalfOpen) {
                    let mut count = self.success_count.write().await;
                    *count += 1;
                    if *count >= self.success_threshold {
                        info!("Circuit breaker: Transitioning to Closed");
                        *state_guard = CircuitState::Closed;
                        *self.failure_count.write().await = 0;
                        *count = 0;
                    }
                }
                // Fix logic bug: Don't reset failure_count on every success in Closed state
                // Only reset after sustained success (handled in HalfOpen) or when circuit closes
                // This allows failures to accumulate properly
            } else {
                let mut failure_count = self.failure_count.write().await;
                *failure_count += 1;
                *self.last_failure.write().await = Some(Instant::now());

                if *failure_count >= self.failure_threshold {
                    error!(
                        "Circuit breaker: Transitioning to Open ({} failures)",
                        failure_count
                    );
                    *state_guard = CircuitState::Open;
                }
            }
        }

        result
    }

    pub async fn is_open(&self) -> bool {
        matches!(*self.state.read().await, CircuitState::Open)
    }

    /// Returns a snapshot of breaker state for diagnostics.
    pub async fn stats(&self) -> CircuitBreakerStats {
        CircuitBreakerStats {
            state: *self.state.read().await,
            failure_count: *self.failure_count.read().await,
            success_count: *self.success_count.read().await,
            failure_threshold: self.failure_threshold,
            success_threshold: self.success_threshold,
            timeout_secs: self.timeout.as_secs(),
        }
    }

    // Test helpers
    #[cfg(test)]
    pub async fn get_state(&self) -> CircuitState {
        *self.state.read().await
    }

    #[cfg(test)]
    pub async fn get_failure_count(&self) -> u32 {
        *self.failure_count.read().await
    }

    #[cfg(test)]
    pub async fn get_success_count(&self) -> u32 {
        *self.success_count.read().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_circuit_breaker_closed_to_open() {
        // Test: Circuit breaker transitions from Closed to Open after failure threshold
        let cb = CircuitBreaker::new(3, 1, 2);

        // Initial state should be Closed
        assert!(matches!(cb.get_state().await, CircuitState::Closed));

        // Fail 3 times - should open circuit
        for _ in 0..3 {
            let _ = cb
                .call(async { Result::<(), CircuitOpenError>::Err(CircuitOpenError) })
                .await;
        }

        assert!(matches!(cb.get_state().await, CircuitState::Open));
        assert_eq!(cb.get_failure_count().await, 3);
    }

    #[tokio::test]
    async fn test_circuit_breaker_open_rejects_requests() {
        // Test: Open circuit rejects requests immediately
        let cb = CircuitBreaker::new(2, 1, 2);

        // Open the circuit
        for _ in 0..2 {
            let _ = cb
                .call(async { Result::<(), CircuitOpenError>::Err(CircuitOpenError) })
                .await;
        }

        assert!(matches!(cb.get_state().await, CircuitState::Open));

        // Request should be rejected
        let result = cb.call(async { Ok::<(), CircuitOpenError>(()) }).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_circuit_breaker_open_to_halfopen_after_timeout() {
        // Test: Circuit transitions from Open to HalfOpen after timeout
        let cb = CircuitBreaker::new(2, 1, 2);

        // Open the circuit
        for _ in 0..2 {
            let _ = cb
                .call(async { Result::<(), CircuitOpenError>::Err(CircuitOpenError) })
                .await;
        }

        assert!(matches!(cb.get_state().await, CircuitState::Open));

        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(1100)).await;

        // Next call should transition to HalfOpen
        let _ = cb.call(async { Ok::<(), CircuitOpenError>(()) }).await;

        assert!(matches!(cb.get_state().await, CircuitState::HalfOpen));
    }

    #[tokio::test]
    async fn test_circuit_breaker_halfopen_to_closed_on_success() {
        // Test: HalfOpen circuit transitions to Closed after success threshold
        let cb = CircuitBreaker::new(2, 1, 2);

        // Open the circuit
        for _ in 0..2 {
            let _ = cb
                .call(async { Result::<(), CircuitOpenError>::Err(CircuitOpenError) })
                .await;
        }

        // Wait for timeout to transition to HalfOpen
        tokio::time::sleep(Duration::from_millis(1100)).await;

        // Succeed 2 times - should close circuit
        for _ in 0..2 {
            let result = cb.call(async { Ok::<(), CircuitOpenError>(()) }).await;
            assert!(result.is_ok());
        }

        assert!(matches!(cb.get_state().await, CircuitState::Closed));
        assert_eq!(cb.get_success_count().await, 0); // Reset after closing
    }

    #[tokio::test]
    async fn test_circuit_breaker_failure_accumulation() {
        // Test: Failures accumulate in Closed state (don't reset on success)
        let cb = CircuitBreaker::new(3, 1, 2);

        // Fail twice, then succeed once
        let _ = cb
            .call(async { Result::<(), CircuitOpenError>::Err(CircuitOpenError) })
            .await;
        let _ = cb
            .call(async { Result::<(), CircuitOpenError>::Err(CircuitOpenError) })
            .await;
        let _ = cb.call(async { Ok::<(), CircuitOpenError>(()) }).await;

        // Failure count should still be 2 (not reset)
        assert_eq!(cb.get_failure_count().await, 2);

        // One more failure should open circuit
        let _ = cb
            .call(async { Result::<(), CircuitOpenError>::Err(CircuitOpenError) })
            .await;

        assert!(matches!(cb.get_state().await, CircuitState::Open));
        assert_eq!(cb.get_failure_count().await, 3);
    }

    #[tokio::test]
    async fn test_circuit_breaker_parameter_validation() {
        // Test: Parameters are validated (min value is 1)
        let cb = CircuitBreaker::new(0, 0, 0);

        // Should use minimum values (1)
        assert_eq!(cb.failure_threshold, 1);
        assert_eq!(cb.success_threshold, 1);
        assert_eq!(cb.timeout, Duration::from_secs(1));
    }
}
