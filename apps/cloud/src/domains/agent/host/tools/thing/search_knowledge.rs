// 8. search_knowledge — full-text search thing_resources

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::SqlitePool;
use zeroclaw::tools::{Tool, ToolResult};
use zeroclaw_api::attribution::{Attributable, Role, ToolKind};

use super::{clamp_limit, tool_ok};

pub struct SearchKnowledgeTool {
    pub(super) pool: SqlitePool,
    pub(super) workspace_id: String,
}

impl Attributable for SearchKnowledgeTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Search)
    }
    fn alias(&self) -> &str {
        self.name()
    }
}

#[async_trait]
impl Tool for SearchKnowledgeTool {
    fn name(&self) -> &str {
        "search_knowledge"
    }

    fn description(&self) -> &str {
        "搜索物的知识文档（文档/手册/说明书等），使用 LIKE 模糊匹配名称和标签。\
         返回文档元数据（不含正文），获取正文请用 read_document 工具。\
         当你需要查找设备相关的文档和知识时使用此工具。"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "thingId": {
                    "type": "string",
                    "description": "物ID（可选，不指定则搜索所有文档）"
                },
                "q": {
                    "type": "string",
                    "description": "搜索关键词（必需，匹配名称和标签）"
                },
                "tags": {
                    "type": "string",
                    "description": "按标签筛选（逗号分隔）"
                },
                "limit": {
                    "type": "integer",
                    "description": "返回数量上限（默认50，最大200）",
                    "default": 50
                }
            },
            "required": ["q"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Input {
            thing_id: Option<String>,
            q: String,
            tags: Option<String>,
            limit: Option<u32>,
        }

        let input: Input = serde_json::from_value(args).map_err(|e| anyhow::anyhow!("参数解析失败: {}", e))?;

        let limit = clamp_limit(input.limit, 50, 200) as i64;

        let db = tinyiothub_storage::Db::new(self.pool.clone());
        let rows = db
            .search_thing_knowledge_docs(
                &self.workspace_id,
                input.thing_id.as_deref(),
                &input.q,
                input.tags.as_deref(),
                limit,
            )
            .await
            .map_err(|e| anyhow::anyhow!("知识搜索失败: {}", e))?;

        let count = rows.len();
        tool_ok(json!({
            "total": count,
            "results": rows,
        }))
    }
}
