// 数据实现，留 cloud（D2）
// 9. read_document — full document content

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::SqlitePool;
use zeroclaw::tools::{Tool, ToolResult};
use zeroclaw_api::attribution::{Attributable, Role, ToolKind};

use super::tool_ok;

pub struct ReadDocumentTool {
    pub(super) pool: SqlitePool,
    pub(super) workspace_id: String,
}

impl Attributable for ReadDocumentTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Search)
    }
    fn alias(&self) -> &str {
        self.name()
    }
}

#[async_trait]
impl Tool for ReadDocumentTool {
    fn name(&self) -> &str {
        "read_document"
    }

    fn description(&self) -> &str {
        "读取知识文档的完整内容。传入 resourceId 返回文档正文。\
         当你需要查看设备手册、说明书或其他知识文档的详细内容时使用此工具。"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "resourceId": {
                    "type": "string",
                    "description": "文档资源ID（必需），由 search_knowledge 返回"
                }
            },
            "required": ["resourceId"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Input {
            resource_id: String,
        }

        let input: Input = serde_json::from_value(args).map_err(|e| anyhow::anyhow!("参数解析失败: {}", e))?;

        #[derive(Debug, serde::Serialize, sqlx::FromRow)]
        struct DocFull {
            id: String,
            name: String,
            #[sqlx(rename = "type")]
            doc_type: String,
            file_path: String,
            content: Option<String>,
            tags: String,
            device_id: Option<String>,
            created_at: String,
            updated_at: String,
        }

        let doc: DocFull = sqlx::query_as::<_, DocFull>(
            "SELECT id, name, resource_type AS type, file_path, content, tags, device_id, \
             created_at, updated_at FROM resources WHERE id = ? AND workspace_id = ?",
        )
        .bind(&input.resource_id)
        .bind(&self.workspace_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| anyhow::anyhow!("数据库查询失败: {}", e))?
        .ok_or_else(|| anyhow::anyhow!("文档 {} 未找到", input.resource_id))?;

        tool_ok(doc)
    }
}
