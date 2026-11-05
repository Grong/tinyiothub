//! ToolRuntimeContext — 工具构建期注入的运行时句柄（Task 14 自 apps/cloud
//! `host/tools/service.rs` 迁入）。
//!
//! 只保留存储无关字段：device cache / 待确认动作暂存等数据实现句柄由组合层
//! 在注册内建工具 provider 时闭包捕获（D2 —— 本 crate 不引用存储类型）。

use std::collections::BTreeMap;
use std::sync::Arc;

use tinyiothub_skills::registry::ApprovalPolicy;

use crate::domains::agent::thing_agent::DirectiveSink;

/// Workflow revision data that tightens and pins tool approval policy at the
/// tool-resolution boundary.
///
/// `workflow_approval_policy` is the Workflow-level policy applied on top of
/// each `ToolDescriptor` (it can only tighten). `pinned_tool_policies` holds
/// the effective per-tool policy fixed into the revision; a mismatch with the
/// currently resolved policy fails closed and the tool is not exposed.
#[derive(Clone, Default)]
pub struct ToolPolicyContext {
    pub workflow_approval_policy: Option<ApprovalPolicy>,
    pub pinned_tool_policies: BTreeMap<String, ApprovalPolicy>,
}

/// Runtime handles threaded into tool construction (P4-Task22; replaces the
/// old `Option<Arc<AppState>>` backdoor).
#[derive(Clone, Default)]
pub struct ToolRuntimeContext {
    pub data_server: Option<Arc<tinyiothub_runtime::DataServer>>,
    pub directive_sink: Option<Arc<dyn DirectiveSink>>,
    /// Optional Workflow revision policy applied when resolving tools for
    /// dispatch. `None` preserves the pre-descriptor behavior.
    pub tool_policy: Option<ToolPolicyContext>,
}
