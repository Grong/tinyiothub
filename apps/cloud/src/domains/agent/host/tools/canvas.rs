// 组合层工具实现，留 cloud（D2）—— 经 ToolRegistry provider 注入（Task 14）
// CanvasTool — A2UI Tool (zeroclaw Tool, NOT MCP ToolHandler)
//
// This tool echoes back A2UI pushes to the frontend. It is intentionally simple:
// the real rendering happens client-side. CanvasTool is always allowed and
// never subject to denylist filtering.

use async_trait::async_trait;
use zeroclaw::tools::{Tool, ToolResult};
use zeroclaw_api::attribution::{Attributable, Role, ToolKind};

pub struct CanvasTool;

impl Attributable for CanvasTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Plugin)
    }
    fn alias(&self) -> &str {
        self.name()
    }
}

#[async_trait]
impl Tool for CanvasTool {
    fn name(&self) -> &str {
        "canvas"
    }

    fn description(&self) -> &str {
        "Push A2UI components to Workspace page.\n\
        Surface kinds: stage (left, spatial/3D/scene), insight (right, data/stats/lists), inline, overlay.\n\
        jsonl is TWO lines (always create surface THEN push components):\n\
        Line1: {\"createSurface\":{\"id\":\"<id>\",\"surfaceKind\":\"stage\"}}\n\
        Line2: {\"updateComponents\":{\"surfaceId\":\"<same id>\",\"components\":[...]}}\n\
        Key components: Scene3D(modelUrl), Image(src), StatCard(label,value,unit?), StatRow(items[{label,value,unit?}]), DeviceCard(deviceId,name,status,properties?[]), DeviceTable(columns[],rows[][]), AlarmCard(alarmId,severity,title,message,deviceName,timestamp), AlarmTable(alarms[]), DataChart(type,data[],labels?), Text(content)\n\
        Example: {\"createSurface\":{\"id\":\"scene\",\"surfaceKind\":\"stage\"}}\n\
        {\"updateComponents\":{\"surfaceId\":\"scene\",\"components\":[{\"id\":\"m1\",\"componentKind\":\"Scene3D\",\"dataModel\":{\"modelUrl\":\"/uploads/model.glb\"}}]}}"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["a2ui_push"] },
                "jsonl": { "type": "string", "description": "JSONL string with createSurface and updateComponents messages" },
            },
            "required": ["action", "jsonl"],
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
        let jsonl = args.get("jsonl").and_then(|v| v.as_str()).unwrap_or("");
        if action == "a2ui_push" {
            Ok(ToolResult {
                success: true,
                output: format!("A2UI pushed: {} bytes", jsonl.len()),
                error: None,
            })
        } else {
            Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("Unknown action".into()),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canvas_tool_name_and_description() {
        let tool = CanvasTool;
        assert_eq!(tool.name(), "canvas");
        assert!(tool.description().contains("A2UI"));
    }

    #[test]
    fn test_canvas_tool_parameters_schema() {
        let tool = CanvasTool;
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
    }

    #[tokio::test]
    async fn test_canvas_tool_execute_a2ui_push() {
        let tool = CanvasTool;
        let args =
            serde_json::json!({"action": "a2ui_push", "jsonl": "{\"createSurface\":{}}\n{\"updateComponents\":{}}"});
        let result = tool.execute(args).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_canvas_tool_execute_unknown_action() {
        let tool = CanvasTool;
        let args = serde_json::json!({"action": "unknown"});
        let result = tool.execute(args).await.unwrap();
        assert!(!result.success);
    }
}
