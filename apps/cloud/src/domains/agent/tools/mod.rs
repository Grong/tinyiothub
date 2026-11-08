//! 工具框架 — 注册表、运行时上下文、外部工具端口、信任包装、目录
//!（Task 14 自 apps/cloud `host/tools/` 框架部分迁入；数据工具实现留组合层）。

pub mod capabilities;
pub mod catalog;
pub mod context;
pub mod external;
pub mod registry;
pub mod trust;

pub use capabilities::{CAPABILITY_UNAVAILABLE_RECOVERY, emit_capability_unavailable, resolve_capabilities};
pub use catalog::build_tools_catalog_json;
pub use context::{ToolPolicyContext, ToolRuntimeContext};
pub use external::{ExternalToolContext, ExternalToolHandler, ExternalToolMeta, ExternalToolRegistry, IoTToolAdapter};
pub use registry::{ExternalToolFactory, ToolProvider, ToolRegistry, filter_by_denylist};
pub use trust::TrustAwareTool;
