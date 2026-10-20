// 3. get_thing_profile — full snapshot (properties + events + docs)

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use tinyiothub_agent::port::attribution::{Attributable, Role, ToolKind};
use tinyiothub_agent::port::tool::{Tool, ToolResult};
use tinyiothub_runtime::watch::live_state::Health;

use super::{LiveSummarySource, tool_err, tool_ok};
use crate::domains::thing::service::ThingService;

pub struct GetThingProfileTool {
    pub(super) thing_service: Arc<ThingService>,
    pub(super) workspace_id: String,
    /// E1 值守读面（Task 10）：LiveState 摘要源；None = 未接线（live=null）。
    pub(super) live: Option<Arc<dyn LiveSummarySource>>,
}

/// health 序列化词表（spec 接口契约）。
pub(super) fn health_str(health: Health) -> &'static str {
    match health {
        Health::Ok => "ok",
        Health::Attention => "attention",
        Health::Bad => "bad",
        Health::Unknown => "unknown",
    }
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
        // thing_id 兜底：提案/报告的 parameters 常随模板写成蛇形键。
        let thing_id = args
            .get("thingId")
            .or_else(|| args.get("thing_id"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("缺少必需参数: thingId"))?;

        match self.thing_service.get_thing_profile(thing_id, &self.workspace_id).await {
            Ok(result) => {
                let mut payload = serde_json::to_value(&result).unwrap_or_default();
                // E1 Task 10：附 LiveState 摘要；无该物/未接线 → live=null（不炸）。
                let summary = match &self.live {
                    Some(source) => source.summary(&self.workspace_id, thing_id).await,
                    None => None,
                };
                let live = summary.map_or(Value::Null, |s| {
                    json!({
                        "health": health_str(s.health),
                        "zone": s.zone,
                        "online": s.online,
                    })
                });
                if let Some(map) = payload.as_object_mut() {
                    map.insert("live".to_string(), live);
                }
                tool_ok(payload)
            }
            Err(e) => tool_err(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::LiveSummarySource;
    use super::*;
    use crate::domains::thing::types::CreateThingRequest;
    use sqlx::SqlitePool;
    use tinyiothub_runtime::watch::live_state::{Health, ThingLiveSummary};

    /// 内存桩：固定摘要的 LiveSummarySource。
    struct StubLive(Option<ThingLiveSummary>);

    #[async_trait]
    impl LiveSummarySource for StubLive {
        async fn summary(&self, _workspace_id: &str, _thing_id: &str) -> Option<ThingLiveSummary> {
            self.0.clone()
        }
    }

    /// 造一个工作区物，返回 (service, thing_id)。
    async fn seed_thing() -> (Arc<ThingService>, String) {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        tinyiothub_storage::test_helpers::run_all_migrations(&pool)
            .await
            .unwrap();
        // things.workspace_id → workspaces → tenants FK 链：seed 系统租户后落工作区行。
        tinyiothub_storage::seed::seed_system(&tinyiothub_storage::Db::new(pool.clone()))
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) \
             VALUES ('ws','ws','tenant-default-001','2026-01-01','2026-01-01')",
        )
        .execute(&pool)
        .await
        .unwrap();
        let svc = Arc::new(ThingService::new(pool));
        let req: CreateThingRequest = serde_json::from_value(json!({"name": "温感-1", "thingType": "device"})).unwrap();
        let created = svc.create_thing(&req, Some("ws")).await.unwrap();
        (svc, created.id)
    }

    /// E1 Task 10：LiveState 有该物摘要 → profile 附 live（health/zone/online）。
    #[tokio::test]
    async fn profile_includes_live_summary_when_available() {
        let (svc, id) = seed_thing().await;
        let tool = GetThingProfileTool {
            thing_service: svc,
            workspace_id: "ws".into(),
            live: Some(Arc::new(StubLive(Some(ThingLiveSummary {
                health: Health::Attention,
                zone: "园区A/3号楼".into(),
                online: false,
                values: Default::default(),
            })))),
        };
        let out = tool.execute(json!({"thingId": id})).await.unwrap();
        assert!(out.success, "{}", out.error.unwrap_or_default());
        let v: Value = serde_json::from_str(&out.output).unwrap();
        assert_eq!(v["live"]["zone"], "园区A/3号楼");
        assert_eq!(v["live"]["health"], "attention");
        assert_eq!(v["live"]["online"], false);
    }

    /// LiveState 无该物 / 读面未接线 → live 键存在且为 null（不炸）。
    #[tokio::test]
    async fn profile_live_null_when_summary_missing() {
        // 形态一：源已接线但该物无实时摘要。
        let (svc, id) = seed_thing().await;
        let tool = GetThingProfileTool {
            thing_service: svc,
            workspace_id: "ws".into(),
            live: Some(Arc::new(StubLive(None))),
        };
        let out = tool.execute(json!({"thingId": id})).await.unwrap();
        assert!(out.success, "{}", out.error.unwrap_or_default());
        let v: Value = serde_json::from_str(&out.output).unwrap();
        assert!(
            v.as_object().unwrap().contains_key("live"),
            "live 键必须存在（无摘要时为 null 而非缺省）"
        );
        assert!(v["live"].is_null());

        // 形态二：读面未接线（ThingToolContext.live=None）。
        let (svc, id) = seed_thing().await;
        let tool = GetThingProfileTool {
            thing_service: svc,
            workspace_id: "ws".into(),
            live: None,
        };
        let out = tool.execute(json!({"thingId": id})).await.unwrap();
        assert!(out.success, "{}", out.error.unwrap_or_default());
        let v: Value = serde_json::from_str(&out.output).unwrap();
        assert!(v["live"].is_null());
    }

    /// health 词表序列化（spec 接口契约：ok/attention/bad/unknown）。
    #[test]
    fn health_serializes_to_spec_vocabulary() {
        assert_eq!(health_str(Health::Ok), "ok");
        assert_eq!(health_str(Health::Attention), "attention");
        assert_eq!(health_str(Health::Bad), "bad");
        assert_eq!(health_str(Health::Unknown), "unknown");
    }
}
