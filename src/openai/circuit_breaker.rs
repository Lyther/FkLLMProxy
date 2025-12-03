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

impl CircuitBreaker {
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

    pub async fn call<F, T, E>(&self, f: F) -> Result<T, E>
    where
        F: std::future::Future<Output = Result<T, E>>,
        E: From<CircuitOpenError>,
    {
        // Fix race condition: acquire write lock immediately to check and transition atomically
        {
            let mut state_guard = self.state.write().await;
            if matches!(*state_guard, CircuitState::Open) {
                let last_failure = *self.last_failure.read().await;
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
                    drop(state_guard); // Release lock before returning
                    warn!("Circuit breaker: Open, rejecting request");
                    return Err(CircuitOpenError.into());
                }
            }
        }

        let result = f.await;

        {
            let mut state_guard = self.state.write().await;
            let current_state = *state_guard;

            match &result {
                Ok(_) => {
                    if matches!(current_state, CircuitState::HalfOpen) {
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
                }
                Err(_) => {
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
        }

        result
    }

    pub async fn is_open(&self) -> bool {
        matches!(*self.state.read().await, CircuitState::Open)
    }
}
