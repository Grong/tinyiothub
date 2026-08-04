use std::time::{Duration, Instant};
use tinyiothub_core::models::{device::Device, device_command::DeviceCommand};

use super::{
    retry::{RetryManager, RetryResult},
    status::{DeviceOverview, DeviceStatusManager},
};
use crate::event_bus::{EventBus, publish_event_safe};
use tinyiothub_core::driver::{DeviceDriver, DriverConfig, ResultValue};
use tinyiothub_core::error::Error;
use tinyiothub_core::models::event::{
    ContentElement, DeviceEventType, Event as DomainEvent, EventLevel, EventSource, RichContent, TextFormat,
};

/// 设备驱动执行结果
#[derive(Debug, Clone)]
pub struct DriverExecutionResult<T> {
    pub result: Result<T, Error>,
    pub elapsed: Duration,
    pub retry_info: Option<RetryInfo>,
}

/// 重试信息
#[derive(Debug, Clone)]
pub struct RetryInfo {
    pub attempt: u32,
    pub will_retry: bool,
    pub next_retry_in: Option<Duration>,
}

impl RetryInfo {
    pub fn new(attempt: u32, will_retry: bool, next_retry_in: Option<Duration>) -> Self {
        Self {
            attempt,
            will_retry,
            next_retry_in,
        }
    }
}

/// 驱动基类
///
/// 为设备驱动提供标准实现，包含重试逻辑和状态管理
pub struct DriverWrapper {
    inner_driver: Box<dyn DeviceDriver>,
    retry_manager: RetryManager,
    status_manager: DeviceStatusManager,
    event_bus: Option<std::sync::Arc<EventBus>>,
    cached_config: DriverConfig,
}

impl DriverWrapper {
    pub fn new(inner_driver: Box<dyn DeviceDriver>) -> Self {
        let device = inner_driver.device().clone();
        let config = inner_driver.retry_config();
        let cached_config = inner_driver.init_config();

        Self {
            retry_manager: RetryManager::new(config),
            status_manager: DeviceStatusManager::new(&device),
            inner_driver,
            event_bus: None,
            cached_config,
        }
    }

    pub fn set_event_bus(&mut self, event_bus: std::sync::Arc<EventBus>) {
        self.event_bus = Some(event_bus);
    }

    pub fn event_bus_ref(&self) -> Option<&std::sync::Arc<EventBus>> {
        self.event_bus.as_ref()
    }

    pub fn device(&self) -> &Device {
        self.inner_driver.device()
    }

    pub fn device_mut(&mut self) -> &mut Device {
        self.inner_driver.device_mut()
    }

    pub fn inner_driver(&self) -> &dyn DeviceDriver {
        &*self.inner_driver
    }

    pub fn inner_driver_mut(&mut self) -> &mut dyn DeviceDriver {
        &mut *self.inner_driver
    }

    pub fn read_data_once(&mut self) -> DriverExecutionResult<Vec<ResultValue>> {
        let start_time = Instant::now();
        let result = self.retry_manager.execute_once(|| self.inner_driver.read_data());
        let elapsed = start_time.elapsed();
        self.update_status(&result, elapsed);
        self.convert_result(result, elapsed)
    }

    pub fn can_read_now(&self) -> bool {
        self.retry_manager.can_retry_now()
    }

    pub fn execute_command(&mut self, cmd: &DeviceCommand) -> DriverExecutionResult<bool> {
        let start_time = Instant::now();
        let result = self
            .retry_manager
            .execute_once(|| self.inner_driver.execute_command(cmd));
        let elapsed = start_time.elapsed();
        self.update_status(&result, elapsed);
        self.convert_result(result, elapsed)
    }

    fn update_status(&mut self, result: &RetryResult<impl Sized>, elapsed: Duration) {
        match result {
            RetryResult::Success(_) => {
                self.status_manager.record_success(elapsed);
            }
            RetryResult::Failed { .. } | RetryResult::Timeout { .. } => {
                self.status_manager.record_failure();
            }
            RetryResult::Retrying { .. } => {}
        }
    }

    fn convert_result<T>(&self, result: RetryResult<T>, elapsed: Duration) -> DriverExecutionResult<T> {
        match result {
            RetryResult::Success(data) => DriverExecutionResult {
                result: Ok(data),
                elapsed,
                retry_info: None,
            },
            RetryResult::Failed {
                attempts,
                last_error,
                total_duration: _,
            } => DriverExecutionResult {
                result: Err(last_error),
                elapsed,
                retry_info: Some(RetryInfo::new(attempts, false, None)),
            },
            RetryResult::Timeout {
                attempts,
                total_duration: _,
            } => DriverExecutionResult {
                result: Err(Error::IOError("Operation timeout".to_string())),
                elapsed,
                retry_info: Some(RetryInfo::new(attempts, false, None)),
            },
            RetryResult::Retrying {
                attempt,
                next_retry_at,
                last_error,
            } => {
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

    pub fn overview(&self) -> DeviceOverview {
        self.status_manager.get_statistics().clone()
    }

    pub fn is_online(&self) -> bool {
        self.status_manager.is_online()
    }

    pub fn is_healthy(&self) -> bool {
        self.status_manager.is_healthy()
    }

    pub fn reset(&mut self) {
        self.status_manager.soft_reset();
        self.retry_manager.soft_reset();
    }

    pub fn config_value(&self, key: &str) -> Option<&String> {
        self.cached_config.get_value(key)
    }

    pub fn set_offline(&mut self) {
        self.status_manager.set_offline();
    }

    pub fn on_connected(&mut self, ip_address: Option<String>) -> Option<DomainEvent> {
        self.status_manager.record_success(Duration::from_millis(0));
        self.retry_manager.reset();

        tracing::info!("Device '{}' connected successfully", self.display_name());

        let device = self.device();
        DomainEvent::new_device_event(
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
                        content: format!("Protocol: {}", device.protocol_type.as_deref().unwrap_or("Unknown")),
                        format: TextFormat::Plain,
                    },
                    ContentElement::Text {
                        content: format!("Address: {}", ip_address.as_deref().unwrap_or("N/A")),
                        format: TextFormat::Plain,
                    },
                ],
            ),
        )
        .ok()
    }

    pub fn on_disconnected(&mut self, reason: Option<String>) -> Option<DomainEvent> {
        self.status_manager.set_offline();

        if let Some(ref reason_text) = reason {
            tracing::warn!("Device '{}' disconnected: {}", self.display_name(), reason_text);
        } else {
            tracing::warn!("Device '{}' disconnected", self.display_name());
        }

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

        DomainEvent::new_device_event(
            DeviceEventType::Connection,
            EventLevel::Warning,
            EventSource::device(device.id.clone(), Some("driver".to_string())),
            RichContent::new(format!("Device Offline: {}", device.name), elements),
        )
        .ok()
    }

    pub fn on_connection_failed(&mut self, error_message: String) {
        self.status_manager.record_failure();

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

            if let Ok(event) = event {
                let event_bus_clone = event_bus.clone();
                tokio::spawn(async move {
                    publish_event_safe(event_bus_clone, event).await;
                });
            }
        }

        tracing::error!("Device '{}' connection failed: {}", self.display_name(), error_message);
    }

    pub fn direct_read_data(&mut self) -> Result<Vec<ResultValue>, Error> {
        self.inner_driver.read_data()
    }

    pub fn direct_execute_command(&mut self, cmd: &DeviceCommand) -> Result<bool, Error> {
        self.inner_driver.execute_command(cmd)
    }

    fn display_name(&self) -> String {
        self.device()
            .display_name
            .clone()
            .unwrap_or_else(|| self.device().name.clone())
    }
}
