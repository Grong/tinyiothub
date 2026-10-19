// 8. search_knowledge — full-text search thing_resources

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::SqlitePool;
use tinyiothub_agent::port::attribution::{Attributable, Role, ToolKind};
use tinyiothub_agent::port::tool::{Tool, ToolResult};

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
            // alias：提案/报告的 parameters 常随模板写成蛇形键。
            #[serde(alias = "thing_id")]
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

        // T6：每条结果补资源元数据——resource_type 与可访问 url
        // （Agent 发现「园区物配了三维模型」并在 A2UI 渲染的前提，spec §五）。
        let results: Vec<Value> = rows
            .into_iter()
            .map(|r| {
                let mut v = serde_json::to_value(&r).unwrap_or_default();
                if let Some(obj) = v.as_object_mut() {
                    obj.insert("resource_type".to_string(), json!(r.doc_type));
                    obj.insert(
                        "url".to_string(),
                        json!(crate::domains::thing::handler::resources::resource_file_url(&r.id)),
                    );
                }
                v
            })
            .collect();
        let count = results.len();
        tool_ok(json!({
            "total": count,
            "results": results,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T6：挂 scene3d 资源 → search_knowledge 返回含 resource_type='scene3d'
    /// 与可访问 url（/api/v1/resources/{id}/file，T5 路由）。
    #[tokio::test]
    async fn search_knowledge_returns_type_and_url() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        tinyiothub_storage::test_helpers::run_all_migrations(&pool)
            .await
            .unwrap();
        // resources.workspace_id → workspaces.id FK：先补租户/工作区链。
        crate::test_utils::seed_test_workspace(&pool, "tenant-1", "ws-a").await;

        sqlx::query(
            "INSERT INTO resources (id, workspace_id, resource_type, name, file_path, tags, created_at, updated_at) \
             VALUES ('res-3d-1', 'ws-a', 'scene3d', '园区三维模型', '/uploads/ws-a/uploads/park.glb', '[]', \
                     '2026-10-19 00:00:00', '2026-10-19 00:00:00')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let tool = SearchKnowledgeTool {
            pool,
            workspace_id: "ws-a".to_string(),
        };
        let result = tool.execute(json!({"q": "三维"})).await.unwrap();
        assert!(result.success, "tool failed: {:?}", result.error);

        let output: Value = serde_json::from_str(&result.output).unwrap();
        assert_eq!(output["total"], 1);
        let row = &output["results"][0];
        assert_eq!(row["resource_type"], "scene3d");
        assert_eq!(row["url"], "/api/v1/resources/res-3d-1/file");
    }
}
