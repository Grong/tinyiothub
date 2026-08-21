// 2. get_thing — lightweight thing view

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use zeroclaw::tools::{Tool, ToolResult};
use zeroclaw_api::attribution::{Attributable, Role, ToolKind};

use super::{tool_err, tool_ok};
use crate::domains::thing::service::ThingService;

pub struct GetThingTool {
    pub(super) thing_service: Arc<ThingService>,
    pub(super) workspace_id: String,
}

impl Attributable for GetThingTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Search)
    }
    fn alias(&self) -> &str {
        self.name()
    }
}

#[async_trait]
impl Tool for GetThingTool {
    fn name(&self) -> &str {
        "get_thing"
    }

    fn description(&self) -> &str {
        "获取单个物的详细信息，包括 ID、名称、类型、面包屑路径、本体摘要和物模型定义。\
         当你需要了解某个具体设备/空间/产线/建筑的详细信息时使用此工具。"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "thingId": {
                    "type": "string",
                    "description": "物ID（必需）"
                }
            },
            "required": ["thingId"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        let thing_id = args
            .get("thingId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("缺少必需参数: thingId"))?;

        match self.thing_service.get_thing(thing_id, &self.workspace_id).await {
            Ok(result) => tool_ok(result),
            Err(e) => tool_err(e.to_string()),
        }
    }
}
