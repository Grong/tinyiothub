// CanvasTool — A2UI Tool (zeroclaw Tool, NOT MCP ToolHandler)
//
// This tool echoes back A2UI pushes to the frontend. It is intentionally simple:
// the real rendering happens client-side. CanvasTool is always allowed and
// never subject to denylist filtering.

use async_trait::async_trait;
use zeroclaw::tools::{Tool, ToolResult};

pub struct CanvasTool;

#[async_trait]
impl Tool for CanvasTool {
    fn name(&self) -> &str {
        "canvas"
    }

    fn description(&self) -> &str {
        "Push A2UI UI components to frontend. jsonl must be a string with TWO lines: Line1={\"createSurface\":{\"id\":\"<id>\",\"surfaceKind\":\"inline\"}}, Line2={\"updateComponents\":{\"components\":[{\"id\":\"<id>\",\"componentKind\":\"DeviceCard\",\"dataModel\":{...}}]}}. \
Component kinds: Basic: Text(content), Image(src), Icon(name), Row(children), Column(children), List(items), Card(title,children), Tabs(tabs), Modal(title,children,visible), Button(label), TextField(label,value), CheckBox(label,checked), ChoicePicker(options), Slider(min,max,value), DateTimeInput(value). \
IoT: DeviceCard(deviceId,name,status,properties[]), DeviceTable(devices[],columns?), DataChart(type,data[],labels?), Scene3D(resourceId,activeFloorId?,selectedDeviceId?,deviceFilter?{floorId?,status?[],deviceType?[]},interactions?{enableOrbit?,enableFloorCut?,showMiniMap?,deviceLabelMode?}), ControlPanel(controls[],layout?), ProgressIndicator(value,max,label?). \
Example: canvas(toolCallId, {action:\"a2ui_push\",jsonl:JSON.stringify({createSurface:{id:\"s1\",surfaceKind:\"inline\"}})+\"\\n\"+JSON.stringify({updateComponents:{components:[{id:\"c1\",componentKind:\"DeviceCard\",dataModel:{deviceId:\"d1\",name:\"Device\",status:\"online\",properties:[]}}]}})})"
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
        let args = serde_json::json!({"action": "a2ui_push", "jsonl": "{\"createSurface\":{}}\n{\"updateComponents\":{}}"});
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
