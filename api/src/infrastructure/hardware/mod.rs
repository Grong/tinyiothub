//! 硬件抽象层
//!
//! 提供跨平台的硬件接口抽象，支持Linux和鸿蒙系统

#[cfg(not(feature = "harmonyos"))]
pub mod gpio;

// 鸿蒙系统的硬件模块
#[cfg(feature = "harmonyos")]
pub mod harmonyos;

