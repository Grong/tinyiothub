//! 鸿蒙系统硬件抽象层
//!
//! 为鸿蒙系统提供硬件接口的统一抽象，包括：
//! - GPIO控制
//! - 显示设备
//! - 串口通信
//! - 网络接口

#[cfg(feature = "harmonyos")]
pub mod gpio;

#[cfg(feature = "harmonyos")]
pub mod display;

#[cfg(feature = "harmonyos")]
pub mod serial;

#[cfg(feature = "harmonyos")]
pub mod network;

// 重新导出鸿蒙系统的硬件接口
