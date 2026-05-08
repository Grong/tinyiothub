//! Driver contracts — traits and data types for device drivers.
//!
//! These are the pure contracts that both runtime infrastructure and
//! concrete driver implementations depend on.

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::models::device::Device;
use crate::models::device_command::DeviceCommand;

pub mod dynamic;
pub use dynamic::{DRIVER_ABI_VERSION, DriverDestroyFn, DriverInitFn, DriverVTable, DriverVTableFn};

// ─── Retry configuration ────────────────────────────────────────────

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
            timeout: Duration::from_secs(300),
        }
    }
}

/// 错误重试策略
pub trait RetryPolicy: Send + Sync {
    fn should_retry(&self, error: &Error) -> bool;
    fn retry_config(&self, error: &Error) -> RetryConfig;
}

/// 默认重试策略
#[derive(Debug, Clone, Default)]
pub struct DefaultRetryPolicy;

impl RetryPolicy for DefaultRetryPolicy {
    fn should_retry(&self, error: &Error) -> bool {
        match error {
            Error::NetworkError(_) => true,
            Error::IOError(_) => true,
            _ => false,
        }
    }

    fn retry_config(&self, error: &Error) -> RetryConfig {
        match error {
            Error::NetworkError(_) => RetryConfig {
                max_attempts: 3,
                base_interval: Duration::from_millis(1000),
                max_interval: Duration::from_secs(30),
                backoff_strategy: BackoffStrategy::Exponential { multiplier: 2.0 },
                timeout: Duration::from_secs(300),
            },
            Error::IOError(_) => RetryConfig {
                max_attempts: 2,
                base_interval: Duration::from_millis(500),
                max_interval: Duration::from_secs(10),
                backoff_strategy: BackoffStrategy::Exponential { multiplier: 1.5 },
                timeout: Duration::from_secs(180),
            },
            _ => RetryConfig::default(),
        }
    }
}

// ─── Driver configuration ───────────────────────────────────────────

/// 驱动配置管理器
#[derive(Debug, Clone)]
pub struct DriverConfig {
    config: HashMap<String, String>,
}

impl DriverConfig {
    /// 从设备信息创建配置管理器
    pub fn from_device(device: &Device) -> Self {
        let mut config = HashMap::new();
        if let Some(ref driver_options) = device.driver_options {
            if let Ok(parsed_config) = serde_json::from_str::<HashMap<String, serde_json::Value>>(driver_options) {
                for (key, value) in parsed_config {
                    let value_str = match &value {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    config.insert(key, value_str);
                }
            }
        }
        Self { config }
    }

    /// 使用默认值初始化配置
    pub fn with_defaults(defaults: HashMap<String, String>) -> Self {
        Self { config: defaults }
    }

    /// 合并默认配置和设备配置
    pub fn from_device_with_defaults(device: &Device, defaults: HashMap<String, String>) -> Self {
        let mut config = defaults;
        if let Some(ref driver_options) = device.driver_options {
            if let Ok(parsed_config) = serde_json::from_str::<HashMap<String, serde_json::Value>>(driver_options) {
                for (key, value) in parsed_config {
                    let value_str = match &value {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    config.insert(key, value_str);
                }
            }
        }
        Self { config }
    }

    pub fn get_value(&self, key: &str) -> Option<&String> {
        self.config.get(key)
    }

    pub fn get_number(&self, key: &str, default: f64) -> f64 {
        self.get_value(key)
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(default)
    }

    pub fn get_integer(&self, key: &str, default: i64) -> i64 {
        self.get_value(key)
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(default)
    }

    pub fn get_boolean(&self, key: &str, default: bool) -> bool {
        self.get_value(key)
            .and_then(|v| match v.to_lowercase().as_str() {
                "true" | "1" | "yes" | "on" => Some(true),
                "false" | "0" | "no" | "off" => Some(false),
                _ => v.parse::<bool>().ok(),
            })
            .unwrap_or(default)
    }

    pub fn get_string(&self, key: &str, default: &str) -> String {
        self.get_value(key).cloned().unwrap_or_else(|| default.to_string())
    }

    pub fn set_value(&mut self, key: String, value: String) {
        self.config.insert(key, value);
    }

    pub fn get_all(&self) -> &HashMap<String, String> {
        &self.config
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.config.contains_key(key)
    }

    pub fn len(&self) -> usize {
        self.config.len()
    }

    pub fn is_empty(&self) -> bool {
        self.config.is_empty()
    }
}

// ─── Result value ───────────────────────────────────────────────────

/// 设备属性读取结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultValue {
    pub name: String,
    pub value_type: String,
    pub value: Option<String>,
}

impl ResultValue {
    pub fn new(name: String, value_type: String, value: Option<String>) -> Self {
        Self {
            name,
            value_type,
            value,
        }
    }

    pub fn integer(name: String, value: i64) -> Self {
        Self::new(name, "int".to_string(), Some(value.to_string()))
    }

    pub fn float(name: String, value: f64) -> Self {
        Self::new(name, "float".to_string(), Some(value.to_string()))
    }

    pub fn float_with_precision(name: String, value: f64, decimal_places: u32) -> Self {
        let multiplier = 10_f64.powi(decimal_places as i32);
        let rounded_value = (value * multiplier).round() / multiplier;
        Self::new(
            name,
            "float".to_string(),
            Some(format!(
                "{:.precision$}",
                rounded_value,
                precision = decimal_places as usize
            )),
        )
    }

    pub fn string(name: String, value: String) -> Self {
        Self::new(name, "string".to_string(), Some(value))
    }

    pub fn boolean(name: String, value: bool) -> Self {
        Self::new(name, "boolean".to_string(), Some(value.to_string()))
    }

    pub fn enum_value(name: String, value: String) -> Self {
        Self::new(name, "enum".to_string(), Some(value))
    }
}

// ─── DeviceDriver trait ─────────────────────────────────────────────

/// 设备驱动特征
///
/// 定义了设备驱动的核心接口，包括数据读取、命令执行等功能
pub trait DeviceDriver: Send + Sync {
    // === 基础信息获取 ===

    fn device(&self) -> &Device;
    fn device_mut(&mut self) -> &mut Device;

    fn display_name(&self) -> String {
        self.device()
            .display_name
            .clone()
            .unwrap_or_else(|| self.device().name.clone())
    }

    fn protocol_type(&self) -> String {
        self.device().protocol_type.clone().unwrap_or_default()
    }

    // === 配置管理 ===

    fn default_config(&self) -> HashMap<String, String> {
        HashMap::new()
    }

    fn init_config(&self) -> DriverConfig {
        let defaults = self.default_config();
        if defaults.is_empty() {
            DriverConfig::from_device(self.device())
        } else {
            DriverConfig::from_device_with_defaults(self.device(), defaults)
        }
    }

    fn get_config_value(&self, key: &str) -> Option<String> {
        let config = self.init_config();
        config.get_value(key).cloned()
    }

    fn get_config_number(&self, key: &str, default: f64) -> f64 {
        let config = self.init_config();
        config.get_number(key, default)
    }

    fn get_config_integer(&self, key: &str, default: i64) -> i64 {
        let config = self.init_config();
        config.get_integer(key, default)
    }

    fn get_config_boolean(&self, key: &str, default: bool) -> bool {
        let config = self.init_config();
        config.get_boolean(key, default)
    }

    fn get_config_string(&self, key: &str, default: &str) -> String {
        let config = self.init_config();
        config.get_string(key, default)
    }

    // === 核心功能接口 ===

    fn read_data(&mut self) -> Result<Vec<ResultValue>, Error>;
    fn execute_command(&mut self, cmd: &DeviceCommand) -> Result<bool, Error>;

    // === 配置接口 ===

    fn retry_config(&self) -> RetryConfig {
        RetryConfig::default()
    }

    fn retry_policy(&self) -> Box<dyn RetryPolicy> {
        Box::<DefaultRetryPolicy>::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_result_value_creation() {
        let int_result = ResultValue::integer("test_int".to_string(), 42);
        assert_eq!(int_result.name, "test_int");
        assert_eq!(int_result.value_type, "int");
        assert_eq!(int_result.value, Some("42".to_string()));

        let float_result = ResultValue::float("test_float".to_string(), 3.14);
        assert_eq!(float_result.name, "test_float");
        assert_eq!(float_result.value_type, "float");

        let string_result = ResultValue::string("test_string".to_string(), "hello".to_string());
        assert_eq!(string_result.name, "test_string");
        assert_eq!(string_result.value_type, "string");

        let bool_result = ResultValue::boolean("test_bool".to_string(), true);
        assert_eq!(bool_result.name, "test_bool");
        assert_eq!(bool_result.value_type, "boolean");

        let enum_result = ResultValue::enum_value("test_enum".to_string(), "option1".to_string());
        assert_eq!(enum_result.name, "test_enum");
        assert_eq!(enum_result.value_type, "enum");
    }

    #[test]
    fn test_result_value_float_with_precision() {
        let result1 = ResultValue::float_with_precision("temp".to_string(), 3.14159265359, 2);
        assert_eq!(result1.value, Some("3.14".to_string()));

        let result2 = ResultValue::float_with_precision("humidity".to_string(), 81.77753185790122, 2);
        assert_eq!(result2.value, Some("81.78".to_string()));

        let result3 = ResultValue::float_with_precision("pressure".to_string(), 60.123456, 1);
        assert_eq!(result3.value, Some("60.1".to_string()));

        let result4 = ResultValue::float_with_precision("voltage".to_string(), 25.0, 2);
        assert_eq!(result4.value, Some("25.00".to_string()));

        let result5 = ResultValue::float_with_precision("count".to_string(), 42.7, 0);
        assert_eq!(result5.value, Some("43".to_string()));

        let result6 = ResultValue::float_with_precision("angle".to_string(), -123.456789, 2);
        assert_eq!(result6.value, Some("-123.46".to_string()));
    }

    #[test]
    fn test_result_value_float_precision_edge_cases() {
        let result1 = ResultValue::float_with_precision("zero".to_string(), 0.0, 2);
        assert_eq!(result1.value, Some("0.00".to_string()));

        let result2 = ResultValue::float_with_precision("large".to_string(), 1234567.89123, 2);
        assert_eq!(result2.value, Some("1234567.89".to_string()));

        let result3 = ResultValue::float_with_precision("round_up".to_string(), 2.999, 2);
        assert_eq!(result3.value, Some("3.00".to_string()));

        let result4 = ResultValue::float_with_precision("round_down".to_string(), 2.994, 2);
        assert_eq!(result4.value, Some("2.99".to_string()));
    }

    #[test]
    fn test_driver_config_creation() {
        let mut config = DriverConfig::with_defaults(HashMap::new());
        assert!(config.is_empty());

        config.set_value("test_key".to_string(), "test_value".to_string());
        assert_eq!(config.len(), 1);
        assert!(config.contains_key("test_key"));
        assert_eq!(config.get_value("test_key"), Some(&"test_value".to_string()));

        config.set_value("number".to_string(), "42.5".to_string());
        config.set_value("integer".to_string(), "100".to_string());
        config.set_value("boolean".to_string(), "true".to_string());

        assert_eq!(config.get_number("number", 0.0), 42.5);
        assert_eq!(config.get_integer("integer", 0), 100);
        assert!(config.get_boolean("boolean", false));
        assert_eq!(config.get_string("nonexistent", "default"), "default");
    }

    #[test]
    fn test_driver_config_boolean_parsing() {
        let mut config = DriverConfig::with_defaults(HashMap::new());

        config.set_value("true1".to_string(), "true".to_string());
        config.set_value("true2".to_string(), "1".to_string());
        config.set_value("true3".to_string(), "yes".to_string());
        config.set_value("true4".to_string(), "on".to_string());
        config.set_value("true5".to_string(), "TRUE".to_string());

        assert!(config.get_boolean("true1", false));
        assert!(config.get_boolean("true2", false));
        assert!(config.get_boolean("true3", false));
        assert!(config.get_boolean("true4", false));
        assert!(config.get_boolean("true5", false));

        config.set_value("false1".to_string(), "false".to_string());
        config.set_value("false2".to_string(), "0".to_string());
        config.set_value("false3".to_string(), "no".to_string());
        config.set_value("false4".to_string(), "off".to_string());
        config.set_value("false5".to_string(), "FALSE".to_string());

        assert!(!config.get_boolean("false1", true));
        assert!(!config.get_boolean("false2", true));
        assert!(!config.get_boolean("false3", true));
        assert!(!config.get_boolean("false4", true));
        assert!(!config.get_boolean("false5", true));

        config.set_value("invalid".to_string(), "maybe".to_string());
        assert!(config.get_boolean("invalid", true));
        assert!(!config.get_boolean("invalid", false));
    }

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.base_interval, Duration::from_millis(500));
    }
}
