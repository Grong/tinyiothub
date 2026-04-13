//! 动态驱动加载模块
#![allow(deprecated)]

pub mod auto_loader;
pub mod loader;
pub mod registry;
pub mod wrapper;

pub use auto_loader::auto_load_drivers;
