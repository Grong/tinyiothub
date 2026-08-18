// 数据实现，留 cloud（D2）
// 1. list_things — list things in workspace (paginated)

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use zeroclaw::tools::{Tool, ToolResult};
use zeroclaw_api::attribution::{Attributable, Role, ToolKind};

use super::{clamp_limit, tool_err, tool_ok};
use crate::domains::thing::{service::ThingService, types::ListThingsParams};

pub struct ListThingsTool {
    pub(super) thing_service: Arc<ThingService>,
    pub(super) workspace_id: String,
}

impl Attributable for ListThingsTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Search)
    }
    fn alias(&self) -> &str {
        self.name()
    }
}

#[async_trait]
impl Tool for ListThingsTool {
    fn name(&self) -> &str {
        "list_things"
    }

    fn description(&self) -> &str {
        "列出工作空间内的物（Things）。支持按类型(device/space/line/building)、\
         父节点ID、标签和关键词(q)筛选，支持分页(limit/offset)。\
         当你需要了解工作空间中有哪些设备、空间、产线或建筑时使用此工具。"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "thingType": {
                    "type": "string",
                    "description": "物类型筛选: device, space, line, building"
                },
                "parentId": {
                    "type": "string",
                    "description": "父节点ID，仅返回该节点下的直接子节点"
                },
                "tags": {
                    "type": "string",
                    "description": "按标签筛选（逗号分隔）"
                },
                "q": {
                    "type": "string",
                    "description": "关键词，模糊匹配名称和描述"
                },
                "limit": {
                    "type": "integer",
                    "description": "返回数量上限（默认50，最大200）",
                    "default": 50
                },
                "offset": {
                    "type": "integer",
                    "description": "分页偏移量（默认0）",
                    "default": 0
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Input {
            thing_type: Option<String>,
            parent_id: Option<String>,
            tags: Option<String>,
            q: Option<String>,
            limit: Option<u32>,
            offset: Option<u32>,
        }

        let input: Input = serde_json::from_value(args).map_err(|e| anyhow::anyhow!("参数解析失败: {}", e))?;

        let limit = clamp_limit(input.limit, 50, 200);
        let offset = input.offset.unwrap_or(0);

        let params = ListThingsParams {
            thing_type: input.thing_type,
            parent_id: input.parent_id,
            tags: input.tags,
            q: input.q,
            limit: Some(limit),
            offset: Some(offset),
        };

        match self.thing_service.list_things(&self.workspace_id, &params).await {
            Ok(result) => tool_ok(result),
            Err(e) => tool_err(e.to_string()),
        }
    }
}
