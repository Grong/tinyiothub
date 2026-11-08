// 框架部分（registry/adapter/trust/catalog）已迁入
// `crate::domains::agent::tools`（Task 14）；本模块只剩组合层工具实现。
pub mod autonomous_invoke;
pub mod canvas;
pub mod composite;
pub mod get_skill;
pub mod service;
pub mod thing;

pub use autonomous_invoke::{AutonomousInvokeActionTool, RunContextSlot, new_run_context_slot};
pub use canvas::CanvasTool;
pub use get_skill::GetSkillTool;
pub use service::{chat_builtin_tools_provider, effective_tool_names};
pub use thing::{TakeError, ThingToolContext, create_thing_tools, take_pending_action, take_pending_action_reasoned};
