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
//!     thing: Thing,
//! }
//!
//! impl MyDriver {
//!     pub fn new(thing: Thing) -> Self {
//!         Self { thing }
//!     }
//! }
//!
//! impl ThingDriver for MyDriver {
//!     fn thing(&self) -> &Thing { &self.thing }
//!     fn thing_mut(&mut self) -> &mut Thing { &mut self.thing }
//!     fn read_data(&mut self) -> Result<Vec<ResultValue>> { Ok(vec![]) }
//!     fn execute_command(&mut self, _cmd: &ThingCommand) -> Result<bool> { Ok(true) }
//! }
//!
//! export_driver!(MyDriver);
//! ```
//!
//! ## 设计不变量
//! - 驱动作者 SDK；FFI ABI 契约的唯一事实源
//! - 禁止依赖 runtime/web；unsafe 仅限 FFI 边界

pub mod config;
pub mod driver;
pub mod error;
pub mod ffi;
pub mod macros;
pub mod types;

// 重新导出核心类型
pub use config::*;
pub use driver::ThingDriver;
pub use error::*;
pub use types::*;
