//! port Tool → rig DynamicTool 桥。

use std::sync::Arc;

use rig_agent::tool::DynamicTool;
use rig_core::tool::{ToolErrorKind, ToolExecutionError, ToolOutput};

use crate::port::tool::Tool;

/// port Tool → rig DynamicTool。每个 port 工具在构建期包一个 DynamicTool。
///
/// success=false 的 port ToolResult 以 JSON 文本（{"success":false,"error":...}）
/// 回传为普通 tool result —— 对齐 zeroclaw dispatcher 语义，不让 rig 走错误重试
/// 路径；只有 execute() 自身的 Err 才映射为 ToolExecutionError。
pub fn to_dynamic_tool(tool: Arc<dyn Tool>) -> DynamicTool {
    let name = tool.name().to_string();
    let description = tool.description().to_string();
    let parameters = tool.parameters_schema();
    DynamicTool::new(name, description, parameters, move |_ctx, args| {
        let tool = Arc::clone(&tool);
        Box::pin(async move {
            match tool.execute(args).await {
                Ok(res) if res.success => Ok(ToolOutput::text(res.output)),
                Ok(res) => Ok(ToolOutput::text(
                    serde_json::json!({
                        "success": false,
                        "error": res.error.unwrap_or_default(),
                    })
                    .to_string(),
                )),
                Err(e) => Err(ToolExecutionError::new(ToolErrorKind::Other, e.to_string())),
            }
        })
    })
}
