// 数据实现，留 cloud（D2）
// 3. get_thing_profile — full snapshot (properties + events + docs)

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use zeroclaw::tools::{Tool, ToolResult};
use zeroclaw_api::attribution::{Attributable, Role, ToolKind};

use super::{tool_err, tool_ok};
use crate::domains::thing::service::ThingService;

pub struct GetThingProfileTool {
    pub(super) thing_service: Arc<ThingService>,
    pub(super) workspace_id: String,
}

impl Attributable for GetThingProfileTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Search)
    }
    fn alias(&self) -> &str {
        self.name()
    }
}

#[async_trait]
impl Tool for GetThingProfileTool {
    fn name(&self) -> &str {
        "get_thing_profile"
    }

    fn description(&self) -> &str {
        "获取物的完整快照：基本信息 + 属性值 + 最近10条事件 + 关联知识文档（不含正文）。\
         当你需要全面了解一个设备的状态、历史和知识库时使用此工具。"
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

        match self.thing_service.get_thing_profile(thing_id, &self.workspace_id).await {
            Ok(result) => tool_ok(result),
            Err(e) => tool_err(e.to_string()),
        }
    }
}
