//! 设备驱动重试机制
//!
//! RetryManager and related state — uses RetryConfig/BackoffStrategy from core.

use std::time::{Duration, Instant};

use tinyiothub_core::driver::{BackoffStrategy, RetryConfig};
use tinyiothub_core::error::Error;

/// 重试结果
#[derive(Debug, Clone)]
pub enum RetryResult<T> {
    Success(T),
    Retrying {
        attempt: u32,
        next_retry_at: Instant,
        last_error: Error,
    },
    Failed {
        attempts: u32,
        last_error: Error,
        total_duration: Duration,
    },
    Timeout {
        attempts: u32,
        total_duration: Duration,
    },
}

/// 重试状态
#[derive(Debug, Clone)]
pub struct RetryState {
    pub current_attempt: u32,
    pub start_time: Instant,
    pub next_retry_at: Option<Instant>,
    pub last_error: Option<Error>,
    pub consecutive_successes: u32,
}

impl Default for RetryState {
    fn default() -> Self {
        Self {
            current_attempt: 0,
            start_time: Instant::now(),
            next_retry_at: None,
            last_error: None,
            consecutive_successes: 0,
        }
    }
}

impl RetryState {
    pub fn reset(&mut self) {
        self.current_attempt = 0;
        self.start_time = Instant::now();
        self.next_retry_at = None;
        self.last_error = None;
        self.consecutive_successes = 0;
    }

    pub fn soft_reset(&mut self) {
        self.current_attempt = 0;
        self.start_time = Instant::now();
        self.next_retry_at = None;
        self.last_error = None;
    }

    pub fn record_success(&mut self) {
        self.consecutive_successes += 1;
        self.current_attempt = 0;
        self.last_error = None;
        self.next_retry_at = None;
        self.start_time = Instant::now();
    }

    pub fn record_failure(&mut self, error: Error) {
        self.current_attempt += 1;
        self.last_error = Some(error);
        self.consecutive_successes = 0;
    }

    pub fn should_retry(&self, config: &RetryConfig) -> bool {
        if self.current_attempt >= config.max_attempts {
            return false;
        }
        if self.start_time.elapsed() >= config.timeout {
            return false;
        }
        if let Some(next_retry) = self.next_retry_at {
            if Instant::now() < next_retry {
                return false;
            }
        }
        true
    }

    pub fn calculate_next_retry(&mut self, config: &RetryConfig) {
        let interval = match &config.backoff_strategy {
            BackoffStrategy::Fixed => config.base_interval,
            BackoffStrategy::Linear { increment } => {
                let total_increment = increment.mul_f64(self.current_attempt as f64);
                (config.base_interval + total_increment).min(config.max_interval)
            }
            BackoffStrategy::Exponential { multiplier } => {
                let multiplied = config
                    .base_interval
                    .mul_f64(multiplier.powi(self.current_attempt as i32 - 1));
                multiplied.min(config.max_interval)
            }
            BackoffStrategy::Custom { intervals } => {
                let index = (self.current_attempt as usize - 1).min(intervals.len() - 1);
                intervals.get(index).copied().unwrap_or(config.max_interval)
            }
        };
        self.next_retry_at = Some(Instant::now() + interval);
    }
}

/// 重试管理器
#[derive(Debug)]
pub struct RetryManager {
    config: RetryConfig,
    state: RetryState,
}

impl RetryManager {
    pub fn new(config: RetryConfig) -> Self {
        Self {
            config,
            state: RetryState::default(),
        }
    }

    pub fn with_default_config() -> Self {
        Self::new(RetryConfig::default())
    }

    pub fn execute_with_retry<T, F>(&mut self, mut operation: F) -> RetryResult<T>
    where
        F: FnMut() -> Result<T, Error>,
    {
        self.state.reset();

        loop {
            if !self.state.should_retry(&self.config) {
                let elapsed = self.state.start_time.elapsed();
                return if elapsed >= self.config.timeout {
                    RetryResult::Timeout {
                        attempts: self.state.current_attempt,
                        total_duration: elapsed,
                    }
                } else {
                    RetryResult::Failed {
                        attempts: self.state.current_attempt,
                        last_error: self
                            .state
                            .last_error
                            .clone()
                            .unwrap_or_else(|| Error::Internal("Unknown error".to_string())),
                        total_duration: elapsed,
                    }
                };
            }

            match operation() {
                Ok(result) => {
                    self.state.record_success();
                    return RetryResult::Success(result);
                }
                Err(error) => {
                    self.state.record_failure(error.clone());

                    if !self.state.should_retry(&self.config) {
                        let total_elapsed = self.state.start_time.elapsed();
                        return if total_elapsed >= self.config.timeout {
                            RetryResult::Timeout {
                                attempts: self.state.current_attempt,
                                total_duration: total_elapsed,
                            }
                        } else {
                            RetryResult::Failed {
                                attempts: self.state.current_attempt,
                                last_error: error,
                                total_duration: total_elapsed,
                            }
                        };
                    }

                    self.state.calculate_next_retry(&self.config);
                    if let Some(next_retry) = self.state.next_retry_at {
                        let wait_time = next_retry.saturating_duration_since(Instant::now());
                        if !wait_time.is_zero() {
                            std::thread::sleep(wait_time);
                        }
                    }
                }
            }
        }
    }

    pub fn execute_once<T, F>(&mut self, operation: F) -> RetryResult<T>
    where
        F: FnOnce() -> Result<T, Error>,
    {
        let past_backoff = self.state.next_retry_at.map_or(true, |t| Instant::now() >= t);
        if past_backoff && self.state.start_time.elapsed() >= self.config.timeout {
            self.state.reset();
        }

        if let Some(next_retry) = self.state.next_retry_at {
            if Instant::now() < next_retry {
                return RetryResult::Retrying {
                    attempt: self.state.current_attempt,
                    next_retry_at: next_retry,
                    last_error: self
                        .state
                        .last_error
                        .clone()
                        .unwrap_or_else(|| Error::Internal("Waiting for retry".to_string())),
                };
            }
        }

        if self.state.current_attempt >= self.config.max_attempts {
            let elapsed = self.state.start_time.elapsed();
            return RetryResult::Failed {
                attempts: self.state.current_attempt,
                last_error: self
                    .state
                    .last_error
                    .clone()
                    .unwrap_or_else(|| Error::Internal("Max retries exceeded".to_string())),
                total_duration: elapsed,
            };
        }

        if self.state.start_time.elapsed() >= self.config.timeout {
            return RetryResult::Timeout {
                attempts: self.state.current_attempt,
                total_duration: self.state.start_time.elapsed(),
            };
        }

        match operation() {
            Ok(result) => {
                self.state.record_success();
                RetryResult::Success(result)
            }
            Err(error) => {
                self.state.record_failure(error.clone());

                if self.state.current_attempt >= self.config.max_attempts
                    || self.state.start_time.elapsed() >= self.config.timeout
                {
                    let elapsed = self.state.start_time.elapsed();
                    return if elapsed >= self.config.timeout {
                        RetryResult::Timeout {
                            attempts: self.state.current_attempt,
                            total_duration: elapsed,
                        }
                    } else {
                        RetryResult::Failed {
                            attempts: self.state.current_attempt,
                            last_error: error,
                            total_duration: elapsed,
                        }
                    };
                }

                self.state.calculate_next_retry(&self.config);
                RetryResult::Retrying {
                    attempt: self.state.current_attempt,
                    next_retry_at: self.state.next_retry_at.unwrap_or_else(Instant::now),
                    last_error: error,
                }
            }
        }
    }

    pub fn reset(&mut self) {
        self.state.reset();
    }

    pub fn soft_reset(&mut self) {
        self.state.soft_reset();
    }

    pub fn state(&self) -> &RetryState {
        &self.state
    }

    pub fn config(&self) -> &RetryConfig {
        &self.config
    }

    pub fn update_config(&mut self, config: RetryConfig) {
        self.config = config;
    }

    pub fn can_retry_now(&self) -> bool {
        if self.state.current_attempt >= self.config.max_attempts {
            return false;
        }
        if self.state.start_time.elapsed() >= self.config.timeout {
            return false;
        }
        if let Some(next_retry) = self.state.next_retry_at {
            Instant::now() >= next_retry
        } else {
            true
        }
    }

    pub fn time_until_next_retry(&self) -> Option<Duration> {
        self.state
            .next_retry_at
            .map(|next_retry| next_retry.saturating_duration_since(Instant::now()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.base_interval, Duration::from_millis(500));
    }

    #[test]
    fn test_retry_state_should_retry() {
        let config = RetryConfig::default();
        let mut state = RetryState::default();
        assert!(state.should_retry(&config));

        state.current_attempt = config.max_attempts;
        assert!(!state.should_retry(&config));
    }

    #[test]
    fn test_backoff_calculation() {
        let config = RetryConfig {
            base_interval: Duration::from_millis(100),
            max_interval: Duration::from_secs(10),
            backoff_strategy: BackoffStrategy::Exponential { multiplier: 2.0 },
            ..Default::default()
        };

        let mut state = RetryState::default();
        state.current_attempt = 1;
        state.calculate_next_retry(&config);
        assert!(state.next_retry_at.is_some());
    }
}
