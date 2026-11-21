//! port Tool → rig DynamicTool 桥。

use std::sync::Arc;

use rig_agent::tool::DynamicTool;
use rig_core::tool::{ToolErrorKind, ToolExecutionError, ToolOutput};

use crate::port::tool::Tool;

const MAX_MODEL_TOOL_OUTPUT_CHARS: usize = 16_000;

fn prune_tool_output(output: String) -> String {
    let total = output.chars().count();
    if total <= MAX_MODEL_TOOL_OUTPUT_CHARS {
        return output;
    }
    let kept = output.chars().take(MAX_MODEL_TOOL_OUTPUT_CHARS).collect::<String>();
    format!(
        "{kept}\n… [tool output truncated: {total} chars total, {} omitted]",
        total - MAX_MODEL_TOOL_OUTPUT_CHARS
    )
}

pub(crate) fn tool_output_from_result(
    mut result: crate::port::tool::ToolResult,
) -> Result<ToolOutput, ToolExecutionError> {
    if !result.success {
        result.output = serde_json::json!({
            "success": false,
            "error": result.error.take().unwrap_or_default(),
        })
        .to_string();
    }
    result.output = prune_tool_output(result.output);
    let content = super::provider::tool_result_to_rig(&result)
        .map_err(|error| ToolExecutionError::new(ToolErrorKind::Other, error.to_string()))?;
    ToolOutput::content(content).map_err(|error| ToolExecutionError::new(ToolErrorKind::Other, error.to_string()))
}

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
                Ok(result) => tool_output_from_result(result),
                Err(e) => Err(ToolExecutionError::new(ToolErrorKind::Other, e.to_string())),
            }
        })
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn oversized_tool_output_is_pruned_with_a_marker() {
        let output = "x".repeat(super::MAX_MODEL_TOOL_OUTPUT_CHARS + 100);
        let pruned = super::prune_tool_output(output);
        assert!(pruned.starts_with("xxxxxxxx"));
        assert!(pruned.contains("tool output truncated"));
        assert!(pruned.contains("100 omitted"));
        assert!(pruned.chars().count() <= super::MAX_MODEL_TOOL_OUTPUT_CHARS + 120);
    }

    #[tokio::test]
    async fn failed_result_preserves_error_semantics_and_image_attachment() {
        let attachment = tinyiothub_core::media::MediaAttachment {
            evidence_id: "e1".into(),
            mime: "image/png".into(),
            bytes: vec![1, 2, 3],
            source_seq: 4,
            media_time: chrono::Utc::now(),
        };
        let output = super::tool_output_from_result(crate::port::tool::ToolResult {
            success: false,
            output: String::new(),
            error: Some("camera failed".into()),
            attachments: vec![attachment],
        })
        .expect("failed tool output");
        let text = output
            .as_content()
            .iter()
            .find_map(|block| match block {
                rig_core::message::ToolResultContent::Text(text) => Some(text.text.as_str()),
                _ => None,
            })
            .expect("error text");
        assert!(text.contains("\"success\":false"));
        assert!(text.contains("camera failed"));
        assert!(output.as_content().iter().any(|block| matches!(
            block,
            rig_core::message::ToolResultContent::Image(image)
                if image.data == rig_core::message::DocumentSourceKind::Raw(vec![1, 2, 3])
        )));
    }
}
