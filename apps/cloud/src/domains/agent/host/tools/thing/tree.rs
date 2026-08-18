// 数据实现，留 cloud（D2）
// 4. get_thing_tree — hierarchical tree

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use zeroclaw::tools::{Tool, ToolResult};
use zeroclaw_api::attribution::{Attributable, Role, ToolKind};

use super::{tool_err, tool_ok};
use crate::domains::thing::service::ThingService;

pub struct GetThingTreeTool {
    pub(super) thing_service: Arc<ThingService>,
    pub(super) workspace_id: String,
}

impl Attributable for GetThingTreeTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Search)
    }
    fn alias(&self) -> &str {
        self.name()
    }
}

#[async_trait]
impl Tool for GetThingTreeTool {
    fn name(&self) -> &str {
        "get_thing_tree"
    }

    fn description(&self) -> &str {
        "获取物的层级树结构（仅返回 id/name/type），支持指定根节点和深度。\
         当你需要了解物之间的层级关系（如建筑→楼层→产线→设备）时使用此工具。"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "rootId": {
                    "type": "string",
                    "description": "根节点ID（不指定则返回工作空间完整树）"
                },
                "depth": {
                    "type": "integer",
                    "description": "最大深度（默认3，最大10）",
                    "default": 3
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Input {
            root_id: Option<String>,
            depth: Option<u32>,
        }

        let input: Input = serde_json::from_value(args).map_err(|e| anyhow::anyhow!("参数解析失败: {}", e))?;

        let depth = Some(input.depth.unwrap_or(3).clamp(1, 10));

        match self
            .thing_service
            .get_thing_tree(&self.workspace_id, input.root_id.as_deref(), depth)
            .await
        {
            Ok(result) => tool_ok(result),
            Err(e) => tool_err(e.to_string()),
        }
    }
}
