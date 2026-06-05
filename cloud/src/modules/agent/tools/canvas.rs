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
        "Push A2UI components to frontend. jsonl is TWO lines: Line1=createSurface(id, surfaceKind=inline|overlay), Line2=updateComponents(components[]).\n\
        Available componentKind (27 total):\n\
        Basic: Text(text,style?), Image(src,alt,width?,height?,fit?), Icon(name,size?), Button(text,variant?,disabled?,functionId?), Card(title?,content), List(items[{text,secondary?}],ordered?), Tabs(tabs[{id,label}],activeTab?), Modal(title,content,open?,actions[{label,functionId,variant?}]), Column, Row, Divider, TextField(label?,value?,placeholder?,inputType?,functionId?), CheckBox(label?,checked?,functionId?), ChoicePicker(label?,choices[{value,label}],selectedValue?,variant?,functionId?), Slider(label?,value?,min?,max?,step?,functionId?), DateTimeInput(label?,value?,inputType?)\n\
        IoT: DeviceCard(deviceId,name,status,icon?,deviceType?,primaryMetric?{key,value,unit},properties?[{key|name,value,unit}],telemetry?[{key|name,value,unit}],signalStrength?,lastSeen?,sparkline?,tags?[],actions?[{label,functionId}]), DeviceTable(title?,columns,rows,actions?), DataChart(title?,series[{label,data[],color}],labels[],type?), Scene3D(modelUrl,resourceId?,activeFloorId?,selectedDeviceId?,deviceFilter?{floorId?,status?[],deviceType?[]},interactions?{enableOrbit?,enableFloorCut?,showMiniMap?,deviceLabelMode?}), ControlPanel(title?,fields[{type,label,key,value,choices?,min?,max?,functionId}]), ProgressIndicator(label?,value,max?), ConfirmationDialog(title,message,confirmFunctionId?,cancelFunctionId?), AlarmCard(deviceId,deviceName,severity,type,message,time?,actions?), AlarmTable(alarms[]), StatCard(label,value,unit?,trend?,trendLabel?,color?,icon?,description?,actions?)\n\
        Example: canvas(toolCallId,{action:\"a2ui_push\",jsonl:JSON.stringify({createSurface:{id:\"s1\"}})+\"\\n\"+JSON.stringify({updateComponents:{components:[{id:\"c1\",componentKind:\"DeviceCard\",dataModel:{deviceId:\"d1\",name:\"Sensor\",status:\"online\",properties:[{key:\"temp\",value:25,unit:\"°C\"}]}}]}})})"
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
