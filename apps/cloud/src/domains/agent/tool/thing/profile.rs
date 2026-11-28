// 3. get_thing_profile — full snapshot (properties + events + docs)

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use tinyiothub_agent::port::attribution::{Attributable, Role, ToolKind};
use tinyiothub_agent::port::tool::{Tool, ToolResult};
use tinyiothub_runtime::access::DemandRecorder;
use tinyiothub_runtime::watch::live_state::Health;

use super::{LiveSummarySource, tool_err, tool_ok};
use crate::domains::thing::service::ThingService;

pub struct GetThingProfileTool {
    pub(super) thing_service: Arc<ThingService>,
    pub(super) workspace_id: String,
    /// E1 值守读面（Task 10）：LiveState 摘要源；None = 未接线（live=null）。
    pub(super) live: Option<Arc<dyn LiveSummarySource>>,
    /// 无实时态时的属性值回退源（cache 是最新真相；None = 只回 live）。
    pub(super) device_cache: Option<Arc<tinyiothub_storage::cache::ThingCache>>,
    /// E4b：快照读取成功后把全部声明属性写入 DemandTracker（None = 未接线）。
    pub(super) demand: Option<Arc<dyn DemandRecorder>>,
}

/// Total Markdown chars returned across all knowledge docs in one profile call.
/// The profile is meant to be a one-shot read; full docs beyond this budget stay
/// available through `read_document`.
const PROFILE_DOC_TOTAL_CHARS: usize = 12_000;
const PROFILE_DOC_MAX_CHARS: usize = 4_000;

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
        "获取设备/空间的完整档案：基本信息 + 属性定义与当前值 + 动作 + 知识文档正文 + 最近事件。\
         一次调用即可完成设备详情读取；只有需要更多文档正文时才继续用 read_document。"
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
                // E4b §3.2：快照装配即读取——记录全部声明属性，供
                // demandOnly source 下一轮采样。
                if let Some(demand) = &self.demand {
                    for property in result.properties.iter().flatten() {
                        if let Some(name) = property.get("name").and_then(|v| v.as_str()) {
                            demand.record_read(&self.workspace_id, thing_id, name);
                        }
                    }
                }
                let mut payload = serde_json::to_value(&result).unwrap_or_default();
                let summary = match &self.live {
                    Some(source) => source.summary(&self.workspace_id, thing_id).await,
                    None => None,
                };

                // Current values: LiveState wins; fall back to the cache when the
                // workspace watcher has not projected this thing yet.
                let mut state: BTreeMap<String, Value> = BTreeMap::new();
                if let Some(s) = &summary {
                    for (k, v) in &s.values {
                        state.insert(k.clone(), Value::String(v.clone()));
                    }
                } else if let Some(thing_live) = self.device_cache.as_ref().and_then(|c| c.get(thing_id))
                    && let Some(props) = &thing_live.properties
                {
                    for p in props {
                        if let Some(v) = &p.current_value {
                            state.insert(p.name.clone(), Value::String(v.clone()));
                        }
                    }
                }

                // Enrich each property definition with its current value so the
                // model does not need a separate read_property round-trip.
                if let Some(properties) = payload.get_mut("properties").and_then(Value::as_array_mut) {
                    for property in properties {
                        let Some(name) = property.get("name").and_then(Value::as_str).map(str::to_string) else {
                            continue;
                        };
                        let display = property.get("displayName").and_then(Value::as_str).map(str::to_string);
                        let value = state
                            .get(&name)
                            .cloned()
                            .or_else(|| display.and_then(|name| state.get(&name).cloned()))
                            .unwrap_or(Value::Null);
                        if let Some(obj) = property.as_object_mut() {
                            obj.insert("currentValue".to_string(), value);
                        }
                    }
                }

                // Knowledge docs: the profile is the one-shot read. Return the
                // Markdown body within a bounded budget; keep metadata otherwise.
                let mut doc_budget = PROFILE_DOC_TOTAL_CHARS;
                let mut knowledge_truncated = false;
                if let Some(docs) = payload.get_mut("knowledgeDocs").and_then(Value::as_array_mut) {
                    for doc in docs {
                        let Some(obj) = doc.as_object_mut() else { continue };
                        let Some(content) = obj.get("content").and_then(Value::as_str).map(str::to_string) else {
                            obj.insert("contentAvailable".to_string(), json!(false));
                            continue;
                        };
                        let chars: Vec<char> = content.chars().collect();
                        let allowed = chars.len().min(PROFILE_DOC_MAX_CHARS).min(doc_budget);
                        if allowed < chars.len() {
                            knowledge_truncated = true;
                            let mut truncated: String = chars[..allowed].iter().collect();
                            truncated.push_str("\n\n…[内容已截断，完整正文用 read_document]");
                            obj.insert("content".to_string(), json!(truncated));
                            obj.insert("contentTruncated".to_string(), json!(true));
                        }
                        obj.insert("contentLength".to_string(), json!(chars.len()));
                        doc_budget = doc_budget.saturating_sub(allowed);
                    }
                }

                let live = summary.as_ref().map_or(Value::Null, |s| {
                    json!({
                        "health": health_str(s.health),
                        "zone": s.zone,
                        "online": s.online,
                    })
                });
                if let Some(map) = payload.as_object_mut() {
                    map.insert("live".to_string(), live);
                    map.insert("state".to_string(), serde_json::to_value(&state).unwrap_or_default());
                    map.insert("profileVersion".to_string(), json!(1));
                    map.insert("knowledgeTruncated".to_string(), json!(knowledge_truncated));
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
        let (service, id, _) = seed_thing_with_pool().await;
        (service, id)
    }

    async fn seed_thing_with_pool() -> (Arc<ThingService>, String, SqlitePool) {
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
        let svc = Arc::new(ThingService::new(pool.clone()));
        let req: CreateThingRequest = serde_json::from_value(json!({"name": "温感-1", "thingType": "device"})).unwrap();
        let created = svc.create_thing(&req, Some("ws")).await.unwrap();
        sqlx::query(
            "INSERT INTO thing_properties (id, thing_id, name, data_type) VALUES ('prop-1', ?, '温度', 'float')",
        )
        .bind(&created.id)
        .execute(&pool)
        .await
        .unwrap();
        (svc, created.id, pool)
    }

    /// E1 Task 10：LiveState 有该物摘要 → profile 附 live（health/zone/online）。
    #[tokio::test]
    async fn profile_includes_live_summary_when_available() {
        let (svc, id) = seed_thing().await;
        let tool = GetThingProfileTool {
            thing_service: svc,
            workspace_id: "ws".into(),
            device_cache: None,
            live: Some(Arc::new(StubLive(Some(ThingLiveSummary {
                health: Health::Attention,
                zone: "园区A/3号楼".into(),
                online: false,
                values: Default::default(),
            })))),
            demand: None,
        };
        let out = tool.execute(json!({"thingId": id})).await.unwrap();
        assert!(out.success, "{}", out.error.unwrap_or_default());
        let v: Value = serde_json::from_str(&out.output).unwrap();
        assert_eq!(v["live"]["zone"], "园区A/3号楼");
        assert_eq!(v["live"]["health"], "attention");
        assert_eq!(v["live"]["online"], false);
    }

    /// Profile is the one-shot read: property definitions carry currentValue and
    /// knowledge docs carry Markdown content (bounded).
    #[tokio::test]
    async fn profile_merges_live_values_and_document_content() {
        let (svc, id, pool) = seed_thing_with_pool().await;
        sqlx::query(
            "INSERT INTO resources (id, workspace_id, thing_id, resource_type, name, file_path, content, tags, created_at, updated_at) \
             VALUES ('doc-1', 'ws', ?, 'document', '温感手册', '', '# 温感手册\n\n量程 0-100°C。', '[]', \
                     '2026-01-01', '2026-01-01')",
        )
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();

        let mut values = std::collections::HashMap::new();
        values.insert("温度".to_string(), "31.5".to_string());
        let tool = GetThingProfileTool {
            thing_service: svc,
            workspace_id: "ws".into(),
            live: Some(Arc::new(StubLive(Some(ThingLiveSummary {
                health: Health::Ok,
                zone: "车间".into(),
                online: true,
                values,
            })))),
            device_cache: None,
            demand: None,
        };
        let out = tool.execute(json!({"thingId": id})).await.unwrap();
        assert!(out.success, "{}", out.error.unwrap_or_default());
        let v: Value = serde_json::from_str(&out.output).unwrap();
        assert_eq!(v["properties"][0]["name"], "温度");
        assert_eq!(v["properties"][0]["currentValue"], "31.5");
        assert_eq!(v["state"]["温度"], "31.5");
        assert_eq!(v["knowledgeDocs"][0]["name"], "温感手册");
        assert!(v["knowledgeDocs"][0]["content"].as_str().unwrap().contains("量程"));
        assert_eq!(v["profileVersion"], 1);
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
            device_cache: None,
            demand: None,
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
            device_cache: None,
            demand: None,
        };
        let out = tool.execute(json!({"thingId": id})).await.unwrap();
        assert!(out.success, "{}", out.error.unwrap_or_default());
        let v: Value = serde_json::from_str(&out.output).unwrap();
        assert!(v["live"].is_null());
    }

    // ── E4b：快照装配记录声明属性 ─────────────────────────

    #[derive(Default)]
    struct RecordingDemand(std::sync::Mutex<Vec<(String, String, String)>>);

    impl DemandRecorder for RecordingDemand {
        fn record_read(&self, workspace_id: &str, thing_id: &str, property: &str) {
            self.0
                .lock()
                .unwrap()
                .push((workspace_id.into(), thing_id.into(), property.into()));
        }
    }

    #[tokio::test]
    async fn profile_records_all_declared_properties_on_success() {
        let (svc, id) = seed_thing().await;
        let demand = Arc::new(RecordingDemand::default());
        let tool = GetThingProfileTool {
            thing_service: svc,
            workspace_id: "ws".into(),
            live: None,
            device_cache: None,
            demand: Some(demand.clone()),
        };

        let out = tool.execute(json!({"thingId": id})).await.unwrap();
        assert!(out.success, "{}", out.error.unwrap_or_default());
        assert_eq!(
            demand.0.lock().unwrap().as_slice(),
            [("ws".to_string(), id.clone(), "温度".to_string())]
        );

        // Not-found → tool error, no demand recorded.
        demand.0.lock().unwrap().clear();
        let out = tool.execute(json!({"thingId": "missing"})).await.unwrap();
        assert!(!out.success);
        assert!(
            demand.0.lock().unwrap().is_empty(),
            "failed profile must not record demand"
        );
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
