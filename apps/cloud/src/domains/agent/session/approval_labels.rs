//! Resolve approval copy from the thing ontology (display names and action
//! metadata), rather than maintaining a second name mapping in the UI.

use std::sync::Arc;

use serde_json::Value;

use crate::domains::thing::service::ThingService;

#[derive(Debug, Clone)]
pub struct ApprovalLabel {
    pub title: String,
    pub description: String,
}

pub struct ThingApprovalLabels {
    service: Arc<ThingService>,
}

impl ThingApprovalLabels {
    pub fn new(service: Arc<ThingService>) -> Self {
        Self { service }
    }

    pub async fn resolve(&self, workspace_id: &str, tool: &str, args: &Value) -> Option<ApprovalLabel> {
        let thing_id = args.get("thingId").and_then(Value::as_str)?;
        let thing = self.service.get_thing(thing_id, workspace_id).await.ok()?;
        let thing_name = thing.display_name.as_deref().unwrap_or(&thing.name).to_string();
        let action_name = args
            .get("actionName")
            .or_else(|| args.get("action"))
            .and_then(Value::as_str);
        let action = if let Some(action_name) = action_name {
            self.service
                .load_actions(thing_id)
                .await?
                .into_iter()
                .find(|item| item.get("name").and_then(Value::as_str) == Some(action_name))
        } else {
            None
        };
        let title = action
            .as_ref()
            .and_then(|item| item.get("displayName").and_then(Value::as_str))
            .unwrap_or(match tool {
                "set_device" | "write_properties" => "调整设备属性",
                _ => "执行设备操作",
            })
            .to_string();
        let mut lines = vec![format!("设备：{thing_name}")];
        lines.push(format!("动作：{title}"));
        if let Some(description) = action
            .as_ref()
            .and_then(|item| item.get("description").and_then(Value::as_str))
            .filter(|description| !description.trim().is_empty())
        {
            lines.push(format!("说明：{description}"));
        }
        if let Some(params) = args.get("params").and_then(Value::as_object) {
            let mut clean = params.clone();
            clean.remove("confirmationToken");
            clean.remove("confirmation_token");
            if !clean.is_empty() {
                lines.push(format!("参数：{}", Value::Object(clean)));
            }
        }
        Some(ApprovalLabel {
            title,
            description: lines.join("\n"),
        })
    }
}
