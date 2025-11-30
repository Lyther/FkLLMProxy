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

pub struct CircuitBreaker {
    state: Arc<RwLock<CircuitState>>,
    failure_count: Arc<RwLock<u32>>,
    last_failure: Arc<RwLock<Option<Instant>>>,
    failure_threshold: u32,
    success_threshold: u32,
    timeout: Duration,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, timeout_secs: u64, success_threshold: u32) -> Self {
        Self {
            state: Arc::new(RwLock::new(CircuitState::Closed)),
            failure_count: Arc::new(RwLock::new(0)),
            last_failure: Arc::new(RwLock::new(None)),
            failure_threshold,
            success_threshold,
            timeout: Duration::from_secs(timeout_secs),
        }
    }

    pub async fn call<F, T, E>(&self, f: F) -> Result<T, E>
    where
        F: std::future::Future<Output = Result<T, E>>,
    {
        let state = *self.state.read().await;

        if matches!(state, CircuitState::Open) {
            let last_failure = *self.last_failure.read().await;
            if let Some(last) = last_failure {
                if last.elapsed() >= self.timeout {
                    info!("Circuit breaker: Attempting to transition to HalfOpen");
                    *self.state.write().await = CircuitState::HalfOpen;
                    *self.failure_count.write().await = 0;
                } else {
                    warn!("Circuit breaker: Open, rejecting request");
                    let result = f.await;
                    return result;
                }
            } else {
                warn!("Circuit breaker: Open, rejecting request");
                let result = f.await;
                return result;
            }
        }

        let result = f.await;
        let current_state = *self.state.read().await;

        match &result {
            Ok(_) => {
                if matches!(current_state, CircuitState::HalfOpen) {
                    let mut count = self.failure_count.write().await;
                    *count += 1;
                    if *count >= self.success_threshold {
                        info!("Circuit breaker: Transitioning to Closed");
                        *self.state.write().await = CircuitState::Closed;
                        *count = 0;
                    }
                } else if matches!(current_state, CircuitState::Closed) {
                    *self.failure_count.write().await = 0;
                }
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
                    *self.state.write().await = CircuitState::Open;
                }
            }
        }

        result
    }

    pub async fn is_open(&self) -> bool {
        matches!(*self.state.read().await, CircuitState::Open)
    }
}
