use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use super::{
    retry::{BackoffStrategy, RetryConfig, RetryManager, RetryResult},
    status::{DeviceOverview, DeviceStatusManager},
};
use crate::{
    domain::event::{
        entities::Event as DomainEvent,
        value_objects::{
            ContentElement, DeviceEventType, EventLevel, EventSource, RichContent, TextFormat,
        },
    },
    dto::entity::{Device, DeviceCommand},
    infrastructure::event::EventBus,
    shared::error::Error,
};

/// 驱动配置管理器
#[derive(Debug, Clone)]
pub struct DriverConfig {
    /// 配置参数映射
    config: HashMap<String, String>,
}

impl DriverConfig {
    /// 从设备信息创建配置管理器
    pub fn from_device(device: &Device) -> Self {
        let mut config = HashMap::new();

        // 从设备的 driver_options 字段中解析配置（如果存在）
        if let Some(ref driver_options) = device.driver_options {
            if let Ok(parsed_config) =
                serde_json::from_str::<HashMap<String, serde_json::Value>>(driver_options)
            {
                for (key, value) in parsed_config {
                    config.insert(key, value.to_string().trim_matches('"').to_string());
                }
            }
        }

        tracing::debug!(
            "DriverConfig initialized with {} parameters for device: {}",
            config.len(),
            device.display_name.as_deref().unwrap_or(&device.name)
        );

        Self { config }
    }

    /// 使用默认值初始化配置
    pub fn with_defaults(defaults: HashMap<String, String>) -> Self {
        Self { config: defaults }
    }

    /// 合并默认配置和设备配置
    pub fn from_device_with_defaults(device: &Device, defaults: HashMap<String, String>) -> Self {
        let mut config = defaults;

        // 从设备的 driver_options 字段中解析配置并覆盖默认值
        if let Some(ref driver_options) = device.driver_options {
            if let Ok(parsed_config) =
                serde_json::from_str::<HashMap<String, serde_json::Value>>(driver_options)
            {
                for (key, value) in parsed_config {
                    config.insert(key, value.to_string().trim_matches('"').to_string());
                }
            }
        }

        tracing::debug!(
            "DriverConfig initialized with {} parameters (including defaults) for device: {}",
            config.len(),
            device.display_name.as_deref().unwrap_or(&device.name)
        );

        Self { config }
    }

    /// 获取配置参数值
    pub fn get_value(&self, key: &str) -> Option<&String> {
        self.config.get(key)
    }

    /// 获取数值类型配置参数
    pub fn get_number(&self, key: &str, default: f64) -> f64 {
        self.get_value(key).and_then(|v| v.parse::<f64>().ok()).unwrap_or(default)
    }

    /// 获取整数类型配置参数
    pub fn get_integer(&self, key: &str, default: i64) -> i64 {
        self.get_value(key).and_then(|v| v.parse::<i64>().ok()).unwrap_or(default)
    }

    /// 获取布尔类型配置参数
    pub fn get_boolean(&self, key: &str, default: bool) -> bool {
        self.get_value(key)
            .and_then(|v| match v.to_lowercase().as_str() {
                "true" | "1" | "yes" | "on" => Some(true),
                "false" | "0" | "no" | "off" => Some(false),
                _ => v.parse::<bool>().ok(),
            })
            .unwrap_or(default)
    }

    /// 获取字符串类型配置参数
    pub fn get_string(&self, key: &str, default: &str) -> String {
        self.get_value(key).cloned().unwrap_or_else(|| default.to_string())
    }

    /// 设置配置参数
    pub fn set_value(&mut self, key: String, value: String) {
        self.config.insert(key, value);
    }

    /// 获取所有配置参数
    pub fn get_all(&self) -> &HashMap<String, String> {
        &self.config
    }

    /// 检查是否包含指定的配置参数
    pub fn contains_key(&self, key: &str) -> bool {
        self.config.contains_key(key)
    }

    /// 获取配置参数数量
    pub fn len(&self) -> usize {
        self.config.len()
    }

    /// 检查配置是否为空
    pub fn is_empty(&self) -> bool {
        self.config.is_empty()
    }
}

/// 设备驱动执行结果
#[derive(Debug, Clone)]
pub struct DriverExecutionResult<T> {
    /// 执行结果
    pub result: Result<T, Error>,
    /// 执行耗时
    pub elapsed: Duration,
    /// 重试信息
    pub retry_info: Option<RetryInfo>,
}

/// 重试信息
#[derive(Debug, Clone)]
pub struct RetryInfo {
    /// 当前尝试次数
    pub attempt: u32,
    /// 是否还会重试
    pub will_retry: bool,
    /// 下次重试时间间隔
    pub next_retry_in: Option<Duration>,
}

impl RetryInfo {
    pub fn new(attempt: u32, will_retry: bool, next_retry_in: Option<Duration>) -> Self {
        Self { attempt, will_retry, next_retry_in }
    }
}

/// 设备属性读取结果
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ResultValue {
    pub name: String,
    pub value_type: String,
    pub value: Option<String>,
}

impl ResultValue {
    pub fn new(name: String, value_type: String, value: Option<String>) -> Self {
        Self { name, value_type, value }
    }

    /// 创建整数类型结果
    pub fn integer(name: String, value: i64) -> Self {
        Self::new(name, "int".to_string(), Some(value.to_string()))
    }

    /// 创建浮点数类型结果
    pub fn float(name: String, value: f64) -> Self {
        Self::new(name, "float".to_string(), Some(value.to_string()))
    }

    /// 创建浮点数类型结果（指定小数位数）
    pub fn float_with_precision(name: String, value: f64, decimal_places: u32) -> Self {
        let multiplier = 10_f64.powi(decimal_places as i32);
        let rounded_value = (value * multiplier).round() / multiplier;
        Self::new(
            name,
            "float".to_string(),
            Some(format!("{:.precision$}", rounded_value, precision = decimal_places as usize)),
        )
    }

    /// 创建字符串类型结果
    pub fn string(name: String, value: String) -> Self {
        Self::new(name, "string".to_string(), Some(value))
    }

    /// 创建布尔类型结果
    pub fn boolean(name: String, value: bool) -> Self {
        Self::new(name, "boolean".to_string(), Some(value.to_string()))
    }

    /// 创建枚举类型结果
    pub fn enum_value(name: String, value: String) -> Self {
        Self::new(name, "enum".to_string(), Some(value))
    }
}

/// 错误重试策略
/// 用于判断错误是否应该重试
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
            Error::Internal(_) => false,
            Error::ConfigError(_) => false,
            Error::ValidationError(_) => false,
            Error::NotFound => false,
            Error::InvalidArgument(_) => false,
            Error::Unsupported(_) => false,
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

/// 设备驱动特征
///
/// 定义了设备驱动的核心接口，包括数据读取、命令执行等功能
pub trait DeviceDriver: Send + Sync {
    // === 基础信息获取 ===

    /// 获取设备引用
    fn device(&self) -> &Device;

    /// 获取设备可变引用
    fn device_mut(&mut self) -> &mut Device;

    /// 获取设备显示名称
    fn display_name(&self) -> String {
        self.device().display_name.clone().unwrap_or_else(|| self.device().name.clone())
    }

    /// 获取设备协议类型
    fn protocol_type(&self) -> String {
        self.device().protocol_type.clone().unwrap_or_default()
    }

    // === 配置管理 ===

    /// 获取驱动默认配置
    /// 使用 DeviceDriver 宏的驱动会自动重写此方法
    /// 未使用宏的驱动使用空配置
    fn default_config(&self) -> HashMap<String, String> {
        HashMap::new()
    }

    /// 初始化驱动配置
    /// 这是一个默认实现，会自动合并默认配置和设备配置
    fn init_config(&self) -> DriverConfig {
        let defaults = self.default_config();
        if defaults.is_empty() {
            DriverConfig::from_device(self.device())
        } else {
            DriverConfig::from_device_with_defaults(self.device(), defaults)
        }
    }

    /// 获取配置参数值（便捷方法）
    fn get_config_value(&self, key: &str) -> Option<String> {
        let config = self.init_config();
        config.get_value(key).cloned()
    }

    /// 获取数值类型配置参数（便捷方法）
    fn get_config_number(&self, key: &str, default: f64) -> f64 {
        let config = self.init_config();
        config.get_number(key, default)
    }

    /// 获取整数类型配置参数（便捷方法）
    fn get_config_integer(&self, key: &str, default: i64) -> i64 {
        let config = self.init_config();
        config.get_integer(key, default)
    }

    /// 获取布尔类型配置参数（便捷方法）
    fn get_config_boolean(&self, key: &str, default: bool) -> bool {
        let config = self.init_config();
        config.get_boolean(key, default)
    }

    /// 获取字符串类型配置参数（便捷方法）
    fn get_config_string(&self, key: &str, default: &str) -> String {
        let config = self.init_config();
        config.get_string(key, default)
    }

    // === 核心功能接口 ===

    /// 读取设备数据
    fn read_data(&mut self) -> Result<Vec<ResultValue>, Error>;

    /// 执行设备命令
    fn execute_command(&mut self, cmd: &DeviceCommand) -> Result<bool, Error>;

    // === 配置接口 ===

    /// 获取重试配置
    fn retry_config(&self) -> RetryConfig {
        RetryConfig::default()
    }

    /// 获取重试策略
    fn retry_policy(&self) -> Box<dyn RetryPolicy> {
        Box::<DefaultRetryPolicy>::default()
    }

    // === 事件发布方法 ===
    // 驱动可以选择性地实现这些方法来获取事件总线

    /// 获取事件总线（如果可用）
    /// 驱动可以重写此方法来提供事件总线实例
    fn event_bus(&self) -> Option<&std::sync::Arc<EventBus>> {
        None
    }
}

/// 驱动基类
///
/// 为设备驱动提供标准实现，包含重试逻辑和状态管理
/// 所有具体驱动应该继承或使用这个基类
pub struct DriverWrapper {
    /// 内部驱动实现
    inner_driver: Box<dyn DeviceDriver>,
    /// 重试管理器
    retry_manager: RetryManager,
    /// 状态管理器
    status_manager: DeviceStatusManager,
    /// 事件总线（可选）
    event_bus: Option<std::sync::Arc<EventBus>>,
}

impl DriverWrapper {
    /// 创建新的驱动基类实例
    pub fn new(inner_driver: Box<dyn DeviceDriver>) -> Self {
        let device = inner_driver.device().clone();
        let config = inner_driver.retry_config();
        let event_bus = inner_driver.event_bus().cloned();

        Self {
            retry_manager: RetryManager::new(config),
            status_manager: DeviceStatusManager::new(&device),
            inner_driver,
            event_bus,
        }
    }

    /// 设置事件总线
    pub fn set_event_bus(&mut self, event_bus: std::sync::Arc<EventBus>) {
        self.event_bus = Some(event_bus);
    }

    /// 获取设备引用
    pub fn device(&self) -> &Device {
        self.inner_driver.device()
    }

    /// 获取设备可变引用
    pub fn device_mut(&mut self) -> &mut Device {
        self.inner_driver.device_mut()
    }

    /// 获取内部驱动（用于需要直接访问特定驱动功能的场景）
    pub fn inner_driver(&self) -> &dyn DeviceDriver {
        &*self.inner_driver
    }

    /// 获取内部驱动可变引用
    pub fn inner_driver_mut(&mut self) -> &mut dyn DeviceDriver {
        &mut *self.inner_driver
    }

    /// 读取设备数据（带重试和状态管理）
    pub fn read_data(&mut self) -> DriverExecutionResult<Vec<ResultValue>> {
        let start_time = Instant::now();

        let result = self.retry_manager.execute_with_retry(|| self.inner_driver.read_data());

        let elapsed = start_time.elapsed();
        self.update_status(&result, elapsed);

        self.convert_result(result, elapsed)
    }

    /// 执行设备命令（带重试和状态管理）
    pub fn execute_command(&mut self, cmd: &DeviceCommand) -> DriverExecutionResult<bool> {
        let start_time = Instant::now();

        let result =
            self.retry_manager.execute_with_retry(|| self.inner_driver.execute_command(cmd));

        let elapsed = start_time.elapsed();
        self.update_status(&result, elapsed);

        self.convert_result(result, elapsed)
    }

    /// 更新设备状态
    fn update_status(&mut self, result: &RetryResult<impl Sized>, elapsed: Duration) {
        match result {
            RetryResult::Success(_) => {
                self.status_manager.record_success(elapsed);
            }
            RetryResult::Failed { .. } | RetryResult::Timeout { .. } => {
                self.status_manager.record_failure();
            }
            RetryResult::Retrying { .. } => {
                // execute_with_retry 不应该返回这个状态，但为安全起见处理一下
                self.status_manager.record_failure();
            }
        }
    }

    /// 转换重试结果为驱动执行结果
    fn convert_result<T>(
        &self,
        result: RetryResult<T>,
        elapsed: Duration,
    ) -> DriverExecutionResult<T> {
        match result {
            RetryResult::Success(data) => {
                DriverExecutionResult { result: Ok(data), elapsed, retry_info: None }
            }
            RetryResult::Failed { attempts, last_error, total_duration: _ } => {
                DriverExecutionResult {
                    result: Err(last_error),
                    elapsed,
                    retry_info: Some(RetryInfo::new(attempts, false, None)),
                }
            }
            RetryResult::Timeout { attempts, total_duration: _ } => DriverExecutionResult {
                result: Err(Error::IOError("Operation timeout".to_string())),
                elapsed,
                retry_info: Some(RetryInfo::new(attempts, false, None)),
            },
            RetryResult::Retrying { attempt, next_retry_at, last_error } => {
                // execute_with_retry 不应该返回这个状态，但为安全起见处理一下
                let next_retry_in = if next_retry_at > Instant::now() {
                    Some(next_retry_at - Instant::now())
                } else {
                    Some(Duration::from_millis(0))
                };

                DriverExecutionResult {
                    result: Err(last_error),
                    elapsed,
                    retry_info: Some(RetryInfo::new(attempt, true, next_retry_in)),
                }
            }
        }
    }

    /// 获取设备统计信息
    pub fn overview(&self) -> DeviceOverview {
        self.status_manager.get_statistics().clone()
    }

    /// 检查设备是否在线
    pub fn is_online(&self) -> bool {
        self.status_manager.is_online()
    }

    /// 检查设备是否健康
    pub fn is_healthy(&self) -> bool {
        self.status_manager.is_healthy()
    }

    /// 重置驱动状态
    pub fn reset(&mut self) {
        self.status_manager.reset();
        self.retry_manager = RetryManager::new(self.inner_driver.retry_config());
    }

    /// 强制设备离线
    pub fn set_offline(&mut self) {
        self.status_manager.set_offline();
    }

    // === 统一的连接状态管理方法 ===

    /// 设备上线
    /// 同时更新状态和发布事件
    pub fn on_connected(&mut self, ip_address: Option<String>) {
        // 更新状态管理器
        self.status_manager.record_success(Duration::from_millis(0));

        // 发布上线事件
        if let Some(ref event_bus) = self.event_bus {
            let device = self.device();
            let event = DomainEvent::new_device_event(
                DeviceEventType::Connection,
                EventLevel::Info,
                EventSource::device(device.id.clone(), Some("driver".to_string())),
                RichContent::new(
                    format!("Device Online: {}", device.name),
                    vec![
                        ContentElement::Text {
                            content: format!("Device '{}' is now online", device.name),
                            format: TextFormat::Plain,
                        },
                        ContentElement::Text {
                            content: format!(
                                "Protocol: {}",
                                device.protocol_type.as_deref().unwrap_or("Unknown")
                            ),
                            format: TextFormat::Plain,
                        },
                        ContentElement::Text {
                            content: format!("Address: {}", ip_address.as_deref().unwrap_or("N/A")),
                            format: TextFormat::Plain,
                        },
                    ],
                ),
            );

            // if let Ok(event) = event {
            //     let event_bus_clone = event_bus.clone();
            //     crate::utils::publish_event_safe(event_bus_clone, event).await;
            // }
        }

        tracing::info!("Device '{}' connected successfully", self.display_name());
    }

    /// 设备下线
    /// 同时更新状态和发布事件
    pub fn on_disconnected(&mut self, reason: Option<String>) {
        // 更新状态管理器
        self.status_manager.set_offline();

        // 发布下线事件
        if let Some(ref event_bus) = self.event_bus {
            let device = self.device();
            let mut elements = vec![ContentElement::Text {
                content: format!("Device '{}' is now offline", device.name),
                format: TextFormat::Plain,
            }];

            if let Some(ref reason_text) = reason {
                elements.push(ContentElement::Text {
                    content: format!("Reason: {}", reason_text),
                    format: TextFormat::Plain,
                });
            }

            let event = DomainEvent::new_device_event(
                DeviceEventType::Connection,
                EventLevel::Warning,
                EventSource::device(device.id.clone(), Some("driver".to_string())),
                RichContent::new(format!("Device Offline: {}", device.name), elements),
            );

            // if let Ok(event) = event {
            //     let event_bus_clone = event_bus.clone();
            //     crate::utils::publish_event_safe(event_bus_clone, event).await;
            // }
        }

        if let Some(reason) = reason {
            tracing::warn!("Device '{}' disconnected: {}", self.display_name(), reason);
        } else {
            tracing::warn!("Device '{}' disconnected", self.display_name());
        }
    }

    /// 连接失败
    /// 记录失败并发布错误事件
    pub fn on_connection_failed(&mut self, error_message: String) {
        // 记录失败
        self.status_manager.record_failure();

        // 发布连接错误事件
        if let Some(ref event_bus) = self.event_bus {
            let device = self.device();
            let event = DomainEvent::new_device_event(
                DeviceEventType::Connection,
                EventLevel::Error,
                EventSource::device(device.id.clone(), Some("driver".to_string())),
                RichContent::new(
                    format!("Connection Error: {}", device.name),
                    vec![
                        ContentElement::Text {
                            content: format!("Failed to connect to device '{}'", device.name),
                            format: TextFormat::Plain,
                        },
                        ContentElement::Text {
                            content: format!("Error: {}", error_message),
                            format: TextFormat::Plain,
                        },
                    ],
                ),
            );

            // if let Ok(event) = event {
            //     let event_bus_clone = event_bus.clone();
            //     crate::utils::publish_event_safe(event_bus_clone, event).await;
            // }
        }

        tracing::error!("Device '{}' connection failed: {}", self.display_name(), error_message);
    }

    /// 直接访问设备数据（不经过重试和状态管理）
    /// 用于特殊场景，如调试或内部实现
    pub fn direct_read_data(&mut self) -> Result<Vec<ResultValue>, Error> {
        self.inner_driver.read_data()
    }

    /// 直接执行命令（不经过重试和状态管理）
    /// 用于特殊场景，如调试或内部实现
    pub fn direct_execute_command(&mut self, cmd: &DeviceCommand) -> Result<bool, Error> {
        self.inner_driver.execute_command(cmd)
    }

    // === 便捷的事件发布方法 ===

    /// 发布属性值变化事件
    pub fn publish_property_change(
        &self,
        property_id: String,
        property_name: String,
        old_value: Option<String>,
        new_value: String,
    ) {
        if let Some(ref event_bus) = self.event_bus {
            let device = self.device();

            // 构建内容元素
            let mut elements = vec![ContentElement::Text {
                content: format!(
                    "Property '{}' value changed on device '{}'",
                    property_name, device.name
                ),
                format: TextFormat::Plain,
            }];

            // 添加旧值信息
            if let Some(ref old_val) = old_value {
                elements.push(ContentElement::Text {
                    content: format!("Previous value: {}", old_val),
                    format: TextFormat::Plain,
                });
            }

            // 添加新值信息
            elements.push(ContentElement::Text {
                content: format!("Current value: {}", new_value),
                format: TextFormat::Plain,
            });

            let event = DomainEvent::new_device_event(
                DeviceEventType::PropertyChange,
                EventLevel::Info,
                EventSource::device_property(
                    device.id.clone(),
                    property_id,
                    "data_collector".to_string(),
                ),
                RichContent::new(
                    format!("Property Changed: {} - {}", device.name, property_name),
                    elements,
                ),
            );

            // if let Ok(event) = event {
            //     let event_bus_clone = event_bus.clone();
            //     crate::utils::publish_event_safe(event_bus_clone, event).await;
            // }
        }
    }

    /// 发布命令执行事件
    pub fn publish_command_execution(
        &self,
        command_name: String,
        success: bool,
        execution_time_ms: u64,
        error_message: Option<String>,
    ) {
        if let Some(ref event_bus) = self.event_bus {
            let device = self.device();
            let level = if success { EventLevel::Info } else { EventLevel::Error };
            let status = if success { "success" } else { "failed" };

            let mut elements = vec![
                ContentElement::Text {
                    content: format!(
                        "Command '{}' executed on device '{}'",
                        command_name, device.name
                    ),
                    format: TextFormat::Plain,
                },
                ContentElement::Text {
                    content: format!("Status: {}", if success { "Success" } else { "Failed" }),
                    format: TextFormat::Plain,
                },
                ContentElement::Text {
                    content: format!("Execution Time: {}ms", execution_time_ms),
                    format: TextFormat::Plain,
                },
            ];

            if let Some(error) = &error_message {
                elements.push(ContentElement::Text {
                    content: format!("Error: {}", error),
                    format: TextFormat::Plain,
                });
            }

            let content = RichContent::new(
                format!("Command Execution: {} - {}", device.name, command_name),
                elements,
            )
            .with_metadata("command_status".to_string(), serde_json::json!(status))
            .with_metadata("command_name".to_string(), serde_json::json!(command_name))
            .with_metadata("execution_time_ms".to_string(), serde_json::json!(execution_time_ms))
            .with_metadata("success".to_string(), serde_json::json!(success));

            let content = if let Some(error) = error_message {
                content.with_metadata("error_message".to_string(), serde_json::json!(error))
            } else {
                content
            };

            // 根据成功/失败选择合适的事件类型
            let event_type = if success {
                DeviceEventType::CommandCompleted
            } else {
                DeviceEventType::CommandFailed
            };

            let event = DomainEvent::new_device_event(
                event_type,
                level,
                EventSource::device(device.id.clone(), Some("driver".to_string())),
                content,
            );

            // if let Ok(event) = event {
            //     let event_bus_clone = event_bus.clone();
            //     crate::utils::publish_event_safe(event_bus_clone, event).await;
            // }
        }
    }

    /// 获取设备显示名称
    fn display_name(&self) -> String {
        self.device().display_name.clone().unwrap_or_else(|| self.device().name.clone())
    }
}

/// 创建驱动实例的工厂函数
pub fn create_driver(
    driver_type: &str,
    device: &Device,
    context: std::sync::Arc<crate::application::data_context::DataContext>,
) -> Result<DriverWrapper, Error> {
    let inner_driver =
        crate::domain::device::driver::create_driver_by_name(driver_type, device, context)?;
    Ok(DriverWrapper::new(inner_driver))
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_result_value_creation() {
        // 测试基本的 ResultValue 创建方法
        let int_result = ResultValue::integer("test_int".to_string(), 42);
        assert_eq!(int_result.name, "test_int");
        assert_eq!(int_result.value_type, "int");
        assert_eq!(int_result.value, Some("42".to_string()));

        let float_result = ResultValue::float("test_float".to_string(), 3.14);
        assert_eq!(float_result.name, "test_float");
        assert_eq!(float_result.value_type, "float");
        assert_eq!(float_result.value, Some("3.14".to_string()));

        let string_result = ResultValue::string("test_string".to_string(), "hello".to_string());
        assert_eq!(string_result.name, "test_string");
        assert_eq!(string_result.value_type, "string");
        assert_eq!(string_result.value, Some("hello".to_string()));

        let bool_result = ResultValue::boolean("test_bool".to_string(), true);
        assert_eq!(bool_result.name, "test_bool");
        assert_eq!(bool_result.value_type, "boolean");
        assert_eq!(bool_result.value, Some("true".to_string()));

        let enum_result = ResultValue::enum_value("test_enum".to_string(), "option1".to_string());
        assert_eq!(enum_result.name, "test_enum");
        assert_eq!(enum_result.value_type, "enum");
        assert_eq!(enum_result.value, Some("option1".to_string()));
    }

    #[test]
    fn test_result_value_float_with_precision() {
        // 测试浮点数精度控制功能

        // 测试保留2位小数
        let result1 = ResultValue::float_with_precision("temp".to_string(), 3.14159265359, 2);
        assert_eq!(result1.name, "temp");
        assert_eq!(result1.value_type, "float");
        assert_eq!(result1.value, Some("3.14".to_string()));

        // 测试问题中的具体数值
        let result2 =
            ResultValue::float_with_precision("humidity".to_string(), 81.77753185790122, 2);
        assert_eq!(result2.value, Some("81.78".to_string()));

        // 测试保留1位小数
        let result3 = ResultValue::float_with_precision("pressure".to_string(), 60.123456, 1);
        assert_eq!(result3.value, Some("60.1".to_string()));

        // 测试整数值保留小数位
        let result4 = ResultValue::float_with_precision("voltage".to_string(), 25.0, 2);
        assert_eq!(result4.value, Some("25.00".to_string()));

        // 测试0位小数（四舍五入到整数）
        let result5 = ResultValue::float_with_precision("count".to_string(), 42.7, 0);
        assert_eq!(result5.value, Some("43".to_string()));

        // 测试负数
        let result6 = ResultValue::float_with_precision("angle".to_string(), -123.456789, 2);
        assert_eq!(result6.value, Some("-123.46".to_string()));

        // 测试很小的数值
        let result7 = ResultValue::float_with_precision("micro".to_string(), 0.00123456, 4);
        assert_eq!(result7.value, Some("0.0012".to_string()));
    }

    #[test]
    fn test_result_value_float_precision_edge_cases() {
        // 测试边界情况

        // 测试0值
        let result1 = ResultValue::float_with_precision("zero".to_string(), 0.0, 2);
        assert_eq!(result1.value, Some("0.00".to_string()));

        // 测试非常大的数值
        let result2 = ResultValue::float_with_precision("large".to_string(), 1234567.89123, 2);
        assert_eq!(result2.value, Some("1234567.89".to_string()));

        // 测试需要进位的情况
        let result3 = ResultValue::float_with_precision("round_up".to_string(), 2.999, 2);
        assert_eq!(result3.value, Some("3.00".to_string()));

        // 测试需要舍去的情况
        let result4 = ResultValue::float_with_precision("round_down".to_string(), 2.994, 2);
        assert_eq!(result4.value, Some("2.99".to_string()));
    }

    #[test]
    fn test_driver_config_creation() {
        // 测试 DriverConfig 的基本功能
        let mut config = DriverConfig::with_defaults(HashMap::new());
        assert!(config.is_empty());

        config.set_value("test_key".to_string(), "test_value".to_string());
        assert_eq!(config.len(), 1);
        assert!(config.contains_key("test_key"));
        assert_eq!(config.get_value("test_key"), Some(&"test_value".to_string()));

        // 测试类型转换方法
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
        // 测试布尔值解析的各种格式
        let mut config = DriverConfig::with_defaults(HashMap::new());

        // 测试各种 true 值
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

        // 测试各种 false 值
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

        // 测试无效值使用默认值
        config.set_value("invalid".to_string(), "maybe".to_string());
        assert!(config.get_boolean("invalid", true));
        assert!(!config.get_boolean("invalid", false));
    }
}
