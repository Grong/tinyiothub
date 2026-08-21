// 框架部分（registry/adapter/trust/catalog）已迁入
// `tinyiothub_agent::tools`（Task 14）；本模块只剩组合层工具实现。
pub mod autonomous_invoke;
pub mod canvas;
pub mod dispatch_task;
pub mod get_skill;
pub mod handler;
pub mod service;
pub mod thing;

pub use autonomous_invoke::{AutonomousInvokeActionTool, RunContextSlot, new_run_context_slot};
pub use canvas::CanvasTool;
pub use dispatch_task::DispatchThingTaskTool;
pub use get_skill::GetSkillTool;
pub use service::{chat_builtin_tools_provider, effective_tool_names};
pub use thing::{ThingToolContext, create_thing_tools, take_pending_action};
