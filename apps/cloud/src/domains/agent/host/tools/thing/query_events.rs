// 数据实现，留 cloud（D2）
// 7. query_events — query events for a thing

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::SqlitePool;
use zeroclaw::tools::{Tool, ToolResult};
use zeroclaw_api::attribution::{Attributable, Role, ToolKind};

use super::{clamp_limit, tool_ok};

pub struct QueryEventsTool {
    pub(super) pool: SqlitePool,
    pub(super) workspace_id: String,
}

impl Attributable for QueryEventsTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Search)
    }
    fn alias(&self) -> &str {
        self.name()
    }
}

#[async_trait]
impl Tool for QueryEventsTool {
    fn name(&self) -> &str {
        "query_events"
    }

    fn description(&self) -> &str {
        "查询物的事件记录，支持按事件类型、级别、时间范围筛选和分页。\
         当你需要了解设备告警、变更、错误等历史事件时使用此工具。"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "thingId": {
                    "type": "string",
                    "description": "物ID（必需）"
                },
                "eventName": {
                    "type": "string",
                    "description": "事件类型筛选（匹配 event_type 字段）"
                },
                "level": {
                    "type": "integer",
                    "description": "事件级别筛选（0=debug, 1=info, 2=warning, 3=error, 4=critical）"
                },
                "since": {
                    "type": "string",
                    "description": "起始时间（ISO 8601 格式，如 2026-01-01T00:00:00Z）"
                },
                "limit": {
                    "type": "integer",
                    "description": "返回数量上限（默认50，最大200）",
                    "default": 50
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Input {
            thing_id: String,
            event_name: Option<String>,
            level: Option<i32>,
            since: Option<String>,
            limit: Option<u32>,
        }

        let input: Input = serde_json::from_value(args).map_err(|e| anyhow::anyhow!("参数解析失败: {}", e))?;

        let limit = clamp_limit(input.limit, 50, 200) as i64;

        // Build dynamic query with QueryBuilder
        let mut builder = sqlx::QueryBuilder::new(
            "SELECT id, event_type, event_subtype, event_level, timestamp, \
             source_type, source_id, title, content, created_at \
             FROM events WHERE device_id = ",
        );
        builder.push_bind(&input.thing_id);
        builder.push(" AND workspace_id = ");
        builder.push_bind(&self.workspace_id);

        if let Some(ref event_name) = input.event_name {
            builder.push(" AND event_type = ");
            builder.push_bind(event_name);
        }
        if let Some(level) = input.level {
            builder.push(" AND event_level = ");
            builder.push_bind(level);
        }
        if let Some(ref since) = input.since {
            builder.push(" AND created_at >= ");
            builder.push_bind(since);
        }

        builder.push(" ORDER BY created_at DESC LIMIT ");
        builder.push_bind(limit);

        #[derive(Debug, serde::Serialize, sqlx::FromRow)]
        struct EventResult {
            id: String,
            event_type: String,
            event_subtype: Option<String>,
            event_level: i32,
            timestamp: Option<String>,
            source_type: String,
            source_id: String,
            title: Option<String>,
            content: String,
            created_at: String,
        }

        let rows = builder
            .build_query_as::<EventResult>()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| anyhow::anyhow!("事件查询失败: {}", e))?;

        let count = rows.len();
        tool_ok(json!({
            "thingId": input.thing_id,
            "total": count,
            "limit": limit,
            "events": rows,
        }))
    }
}
