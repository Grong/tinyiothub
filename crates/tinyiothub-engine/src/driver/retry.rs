//! 设备驱动重试机制
//!
//! 提供灵活、可配置的重试策略，支持不同类型的错误采用不同的重试方式

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use tinyiothub_core::error::Error;

/// 重试策略配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// 最大重试次数
    pub max_attempts: u32,
    /// 基础重试间隔
    pub base_interval: Duration,
    /// 最大重试间隔
    pub max_interval: Duration,
    /// 退避策略
    pub backoff_strategy: BackoffStrategy,
    /// 重试超时时间
    pub timeout: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_interval: Duration::from_millis(500),
            max_interval: Duration::from_secs(30),
            backoff_strategy: BackoffStrategy::Exponential { multiplier: 2.0 },
            timeout: Duration::from_secs(300), // 5分钟总超时
        }
    }
}

/// 退避策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BackoffStrategy {
    /// 固定间隔
    Fixed,
    /// 线性增长
    Linear { increment: Duration },
    /// 指数退避
    Exponential { multiplier: f64 },
    /// 自定义间隔序列
    Custom { intervals: Vec<Duration> },
}

/// 重试结果
#[derive(Debug, Clone)]
pub enum RetryResult<T> {
    /// 成功
    Success(T),
    /// 重试中
    Retrying { attempt: u32, next_retry_at: Instant, last_error: Error },
    /// 最终失败
    Failed { attempts: u32, last_error: Error, total_duration: Duration },
    /// 超时
    Timeout { attempts: u32, total_duration: Duration },
}

/// 重试状态
#[derive(Debug, Clone)]
pub struct RetryState {
    /// 当前尝试次数
    pub current_attempt: u32,
    /// 开始时间
    pub start_time: Instant,
    /// 下次重试时间
    pub next_retry_at: Option<Instant>,
    /// 最后一次错误
    pub last_error: Option<Error>,
    /// 连续成功次数（用于恢复判断）
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
    /// 完全重置状态（包括累积统计）
    pub fn reset(&mut self) {
        self.current_attempt = 0;
        self.start_time = Instant::now();
        self.next_retry_at = None;
        self.last_error = None;
        self.consecutive_successes = 0;
    }

    /// 软重置 —— 只重置临时重试状态，保留 consecutive_successes
    pub fn soft_reset(&mut self) {
        self.current_attempt = 0;
        self.start_time = Instant::now();
        self.next_retry_at = None;
        self.last_error = None;
        // consecutive_successes 保留，用于恢复判断
    }

    /// 记录成功
    pub fn record_success(&mut self) {
        self.consecutive_successes += 1;
        self.current_attempt = 0;
        self.last_error = None;
        self.next_retry_at = None;
        self.start_time = Instant::now();
    }

    /// 记录失败
    pub fn record_failure(&mut self, error: Error) {
        self.current_attempt += 1;
        self.last_error = Some(error);
        self.consecutive_successes = 0;
    }

    /// 是否应该重试
    pub fn should_retry(&self, config: &RetryConfig) -> bool {
        // 检查重试次数
        if self.current_attempt >= config.max_attempts {
            return false;
        }

        // 检查总超时时间
        if self.start_time.elapsed() >= config.timeout {
            return false;
        }

        // 检查是否到了重试时间
        if let Some(next_retry) = self.next_retry_at {
            if Instant::now() < next_retry {
                return false;
            }
        }

        true
    }

    /// 计算下次重试时间
    pub fn calculate_next_retry(&mut self, config: &RetryConfig) {
        let interval = match &config.backoff_strategy {
            BackoffStrategy::Fixed => config.base_interval,

            BackoffStrategy::Linear { increment } => {
                let total_increment = increment.mul_f64(self.current_attempt as f64);
                (config.base_interval + total_increment).min(config.max_interval)
            }

            BackoffStrategy::Exponential { multiplier } => {
                let multiplied =
                    config.base_interval.mul_f64(multiplier.powi(self.current_attempt as i32 - 1));
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
    /// 创建新的重试管理器
    pub fn new(config: RetryConfig) -> Self {
        Self { config, state: RetryState::default() }
    }

    /// 使用默认配置创建
    pub fn with_default_config() -> Self {
        Self::new(RetryConfig::default())
    }

    /// 执行操作，带重试逻辑
    pub fn execute_with_retry<T, F>(&mut self, mut operation: F) -> RetryResult<T>
    where
        F: FnMut() -> Result<T, Error>,
    {
        // 重置状态，确保每次执行都从新开始
        self.state.reset();

        tracing::debug!(
            "Starting retry execution, config: max_attempts={}, timeout={:?}",
            self.config.max_attempts,
            self.config.timeout
        );

        loop {
            // 检查是否应该重试
            if !self.state.should_retry(&self.config) {
                let elapsed = self.state.start_time.elapsed();
                tracing::warn!(
                    "Retry execution stopped: attempt={}, elapsed={:?}, timeout={:?}, should_retry=false",
                    self.state.current_attempt,
                    elapsed,
                    self.config.timeout
                );

                return if elapsed >= self.config.timeout {
                    tracing::error!(
                        "Operation timed out: elapsed={:?} >= timeout={:?}, attempts={}",
                        elapsed,
                        self.config.timeout,
                        self.state.current_attempt
                    );
                    RetryResult::Timeout {
                        attempts: self.state.current_attempt,
                        total_duration: elapsed,
                    }
                } else {
                    tracing::error!(
                        "Operation failed after {} attempts, last_error={:?}",
                        self.state.current_attempt,
                        self.state.last_error
                    );
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

            // 执行操作
            tracing::debug!("Executing operation, attempt={}", self.state.current_attempt + 1);
            let operation_start = std::time::Instant::now();

            match operation() {
                Ok(result) => {
                    let operation_elapsed = operation_start.elapsed();
                    let total_elapsed = self.state.start_time.elapsed();
                    tracing::debug!(
                        "Operation succeeded on attempt {}, operation took {:?}, total elapsed {:?}",
                        self.state.current_attempt + 1,
                        operation_elapsed,
                        total_elapsed
                    );
                    self.state.record_success();
                    return RetryResult::Success(result);
                }
                Err(error) => {
                    let operation_elapsed = operation_start.elapsed();
                    tracing::warn!(
                        "Operation failed on attempt {}, took {:?}, error: {:?}",
                        self.state.current_attempt + 1,
                        operation_elapsed,
                        error
                    );

                    self.state.record_failure(error.clone());

                    if !self.state.should_retry(&self.config) {
                        let total_elapsed = self.state.start_time.elapsed();

                        return if total_elapsed >= self.config.timeout {
                            tracing::error!(
                                "Operation timed out after failure: total_elapsed={:?} >= timeout={:?}, attempts={}",
                                total_elapsed,
                                self.config.timeout,
                                self.state.current_attempt
                            );
                            RetryResult::Timeout {
                                attempts: self.state.current_attempt,
                                total_duration: total_elapsed,
                            }
                        } else {
                            tracing::error!(
                                "Operation failed permanently: attempts={}, last_error={:?}",
                                self.state.current_attempt,
                                error
                            );
                            RetryResult::Failed {
                                attempts: self.state.current_attempt,
                                last_error: error,
                                total_duration: total_elapsed,
                            }
                        };
                    }

                    // 计算下次重试时间并等待
                    self.state.calculate_next_retry(&self.config);
                    if let Some(next_retry) = self.state.next_retry_at {
                        let wait_time = next_retry.saturating_duration_since(Instant::now());
                        if !wait_time.is_zero() {
                            tracing::debug!(
                                "Waiting {:?} before next retry (attempt {})",
                                wait_time,
                                self.state.current_attempt + 1
                            );
                            std::thread::sleep(wait_time);
                        }
                    }
                }
            }
        }
    }

    /// 执行一次操作，不阻塞 sleep。
    /// 如果失败且还可以重试，返回 Retrying（由调用者在合适时机再次调用）。
    /// 如果成功，返回 Success。
    /// 如果重试次数耗尽或超时，返回 Failed/Timeout。
    pub fn execute_once<T, F>(&mut self, operation: F) -> RetryResult<T>
    where
        F: FnOnce() -> Result<T, Error>,
    {
        // 如果重试等待期已过且总超时窗口也过期了，重置状态开始新会话。
        // 避免驱动创建后永不重置 start_time，导致超时后永久 Timeout。
        let past_backoff = self.state.next_retry_at.map_or(true, |t| Instant::now() >= t);
        if past_backoff && self.state.start_time.elapsed() >= self.config.timeout {
            self.state.reset();
        }

        // 如果还没到下一次次重试时间，直接返回 Retrying
        if let Some(next_retry) = self.state.next_retry_at {
            if Instant::now() < next_retry {
                return RetryResult::Retrying {
                    attempt: self.state.current_attempt,
                    next_retry_at: next_retry,
                    last_error: self.state.last_error.clone().unwrap_or_else(|| {
                        Error::Internal("Waiting for retry".to_string())
                    }),
                };
            }
        }

        // 检查重试次数
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

        // 检查总超时时间
        if self.state.start_time.elapsed() >= self.config.timeout {
            return RetryResult::Timeout {
                attempts: self.state.current_attempt,
                total_duration: self.state.start_time.elapsed(),
            };
        }

        // 执行操作
        match operation() {
            Ok(result) => {
                self.state.record_success();
                RetryResult::Success(result)
            }
            Err(error) => {
                self.state.record_failure(error.clone());

                // 检查是否还可以重试
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

                // 还可以重试，计算下次重试时间
                self.state.calculate_next_retry(&self.config);
                RetryResult::Retrying {
                    attempt: self.state.current_attempt,
                    next_retry_at: self.state.next_retry_at.unwrap_or_else(Instant::now),
                    last_error: error,
                }
            }
        }
    }

    /// 完全重置重试状态（包括累积统计）
    pub fn reset(&mut self) {
        self.state.reset();
    }

    /// 软重置 —— 只重置临时重试状态，保留 consecutive_successes
    pub fn soft_reset(&mut self) {
        self.state.soft_reset();
    }

    /// 获取当前状态
    pub fn state(&self) -> &RetryState {
        &self.state
    }

    /// 获取配置
    pub fn config(&self) -> &RetryConfig {
        &self.config
    }

    /// 更新配置
    pub fn update_config(&mut self, config: RetryConfig) {
        self.config = config;
    }

    /// 是否可以立即重试
    pub fn can_retry_now(&self) -> bool {
        // 重试次数已耗尽，不再重试
        if self.state.current_attempt >= self.config.max_attempts {
            return false;
        }
        // 总超时已过期，不再重试
        if self.state.start_time.elapsed() >= self.config.timeout {
            return false;
        }
        if let Some(next_retry) = self.state.next_retry_at {
            Instant::now() >= next_retry
        } else {
            true
        }
    }

    /// 距离下次重试的时间
    pub fn time_until_next_retry(&self) -> Option<Duration> {
        self.state
            .next_retry_at
            .map(|next_retry| next_retry.saturating_duration_since(Instant::now()))
    }
}

/// 错误分类器 - 根据错误类型决定重试策略
pub trait ErrorClassifier {
    /// 判断错误是否可重试
    fn is_retryable(&self, error: &Error) -> bool;

    /// 获取错误对应的重试配置
    fn get_retry_config(&self, error: &Error) -> RetryConfig;
}

/// 默认错误分类器
#[derive(Debug, Default)]
pub struct DefaultErrorClassifier;

impl ErrorClassifier for DefaultErrorClassifier {
    fn is_retryable(&self, error: &Error) -> bool {
        match error {
            // 网络相关错误通常可重试
            Error::IOError(_) => true,
            Error::NetworkError(_) => true,

            // 配置错误通常不可重试
            Error::ConfigError(_) => false,
            Error::ValidationError(_) => false,

            // 内部错误根据具体情况
            Error::Internal(msg) => {
                // 可以根据错误消息进一步判断
                !msg.contains("configuration") && !msg.contains("invalid")
            }

            // 其他错误默认可重试
            _ => true,
        }
    }

    fn get_retry_config(&self, error: &Error) -> RetryConfig {
        match error {
            // 网络错误使用较长的重试间隔
            Error::NetworkError(_) => RetryConfig {
                max_attempts: 5,
                base_interval: Duration::from_secs(2),
                max_interval: Duration::from_secs(60),
                backoff_strategy: BackoffStrategy::Exponential { multiplier: 2.0 },
                timeout: Duration::from_secs(600),
            },

            // IO错误使用中等重试间隔
            Error::IOError(_) => RetryConfig {
                max_attempts: 3,
                base_interval: Duration::from_millis(500),
                max_interval: Duration::from_secs(30),
                backoff_strategy: BackoffStrategy::Exponential { multiplier: 1.5 },
                timeout: Duration::from_secs(300),
            },

            // 其他错误使用默认配置
            _ => RetryConfig::default(),
        }
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

        // 初始状态应该可以重试
        assert!(state.should_retry(&config));

        // 超过最大重试次数后不应该重试
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

        // 第一次重试应该使用基础间隔
        assert!(state.next_retry_at.is_some());
    }

    #[test]
    fn test_error_classifier() {
        let classifier = DefaultErrorClassifier;

        // 网络错误应该可重试
        let network_error = Error::NetworkError("Connection failed".to_string());
        assert!(classifier.is_retryable(&network_error));

        // 配置错误不应该重试
        let config_error = Error::ConfigError("Invalid config".to_string());
        assert!(!classifier.is_retryable(&config_error));
    }
}
