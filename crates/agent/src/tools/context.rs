//! ToolRuntimeContext — 工具构建期注入的运行时句柄（Task 14 自 apps/cloud
//! `host/tools/service.rs` 迁入）。
//!
//! 只保留存储无关字段：device cache / 待确认动作暂存等数据实现句柄由组合层
//! 在注册内建工具 provider 时闭包捕获（D2 —— 本 crate 不引用存储类型）。

use std::sync::Arc;

use crate::runtime::thing_agent::DirectiveSink;

/// Runtime handles threaded into tool construction (P4-Task22; replaces the
/// old `Option<Arc<AppState>>` backdoor).
#[derive(Clone, Default)]
pub struct ToolRuntimeContext {
    pub data_server: Option<Arc<tinyiothub_runtime::DataServer>>,
    pub directive_sink: Option<Arc<dyn DirectiveSink>>,
}
