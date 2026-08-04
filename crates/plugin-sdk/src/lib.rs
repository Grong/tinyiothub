//! TinyIoTHub 驱动开发SDK
//!
//! 提供驱动开发所需的所有接口和工具
//!
//! # 快速开始
//!
//! ```rust,ignore
//! use tinyiothub_plugin_sdk::*;
//!
//! pub struct MyDriver {
//!     device: Device,
//! }
//!
//! impl MyDriver {
//!     pub fn new(device: Device) -> Self {
//!         Self { device }
//!     }
//! }
//!
//! impl DeviceDriver for MyDriver {
//!     fn device(&self) -> &Device { &self.device }
//!     fn device_mut(&mut self) -> &mut Device { &mut self.device }
//!     fn read_data(&mut self) -> Result<Vec<ResultValue>> { Ok(vec![]) }
//!     fn execute_command(&mut self, _cmd: &DeviceCommand) -> Result<bool> { Ok(true) }
//! }
//!
//! export_driver!(MyDriver);
//! ```

pub mod config;
pub mod driver;
pub mod error;
pub mod ffi;
pub mod macros;
pub mod types;

// 重新导出核心类型
pub use config::*;
pub use driver::DeviceDriver;
pub use error::*;
pub use types::*;
