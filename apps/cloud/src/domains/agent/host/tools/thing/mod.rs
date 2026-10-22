// Thing Agent Tools — 9 AI agent tools for Thing Ontology
//
// These tools give the AI agent read+execute access to the Thing Ontology.
// They follow the zeroclaw Tool trait pattern (same as CanvasTool, GetSkillTool).
//
// The 9 tools (one file per tool, G9 flat layout):
//   1. list_things       — list things in workspace (paginated)        [list.rs]
//   2. get_thing         — lightweight thing view                       [get.rs]
//   3. get_thing_profile — full snapshot (properties + events + docs)   [profile.rs]
//   4. get_thing_tree    — hierarchical tree                            [tree.rs]
//   5. read_property     — current property value from device_cache     [read_property.rs]
//   6. invoke_action     — execute a device action (type='device' guard)[invoke_action.rs]
//   7. query_events      — query events for a thing                     [query_events.rs]
//   8. search_knowledge  — full-text search thing_resources             [search_knowledge.rs]
//   9. read_document     — full document content                        [read_document.rs]
//
// Shared homes: confirmation store [pending_action.rs], param validation
// [validate.rs], the create_thing_tools factory and helpers live here.

mod composite_tools;
mod device;
mod get;
mod invoke_action;
mod list;
mod pending_action;
mod profile;
mod query_events;
mod read_document;
mod read_property;
mod search_knowledge;
mod tree;
mod validate;
mod watch_tools;

use std::sync::Arc;

use crate::domains::thing::service::ThingService;
use sqlx::SqlitePool;
use tinyiothub_agent::port::tool::{Tool, ToolResult};
use tinyiothub_agent::runtime::thing_agent::{
    ArbitrationRegistry, ResourceMode, ResourceSpec, ThingAgentManager, WatchCommand,
};
use tinyiothub_runtime::access::resources::{ResourceClaim, ResourceResolver};
use tinyiothub_runtime::watch::live_state::ThingLiveSummary;
use tinyiothub_skills::trust::ToolSafety;

pub use get::GetThingTool;
pub use invoke_action::InvokeActionTool;
pub use list::ListThingsTool;
pub use pending_action::{
    PendingAction, PendingActionStore, TakeError, cleanup_expired_tokens, spawn_pending_action_sweeper,
    spawn_pending_action_sweeper_default, store_pending_action, take_pending_action, take_pending_action_reasoned,
};
pub use profile::GetThingProfileTool;
pub use query_events::QueryEventsTool;
pub use read_document::ReadDocumentTool;
pub use read_property::ReadPropertyTool;
pub use search_knowledge::SearchKnowledgeTool;
pub use tree::GetThingTreeTool;
pub use validate::validate_action_params;

// ============================================================================
// E1 Task 10：LiveState 读面（get_thing_profile 附 live 摘要）
// ============================================================================

/// 单物实时摘要读面。profile 工具经它取 LiveState::summary；
/// 返回 None = 该物无实时态（live 序列化为 null，不炸）。
#[async_trait::async_trait]
pub trait LiveSummarySource: Send + Sync {
    async fn summary(&self, workspace_id: &str, thing_id: &str) -> Option<ThingLiveSummary>;
}

/// 生产实现：迟绑定槽。bootstrap 早期注入工具上下文（manager 尚未就绪——
/// provider 注册先于 AgentRuntime 构建），manager 就位后 `set`；查询走该
/// 工作区 watch engine 命令通道（actor 单写者不破）。未 set / 工作区 loop
/// 未运行 / engine 无该物 → None（profile live=null）。
#[derive(Clone, Default)]
pub struct LiveSummarySlot {
    manager: Arc<std::sync::RwLock<Option<Arc<ThingAgentManager>>>>,
}

impl LiveSummarySlot {
    /// bootstrap：thing_agent_manager 就绪后调用（与 app_state.thing_agents
    /// 赋值同点）。
    pub fn set(&self, manager: Arc<ThingAgentManager>) {
        *self.manager.write().expect("live summary slot poisoned") = Some(manager);
    }
}

#[async_trait::async_trait]
impl LiveSummarySource for LiveSummarySlot {
    async fn summary(&self, workspace_id: &str, thing_id: &str) -> Option<ThingLiveSummary> {
        let manager = self.manager.read().expect("live summary slot poisoned").clone()?;
        let sink = manager.watch_command_sink(workspace_id)?;
        let (reply, rx) = tokio::sync::oneshot::channel();
        sink.send(WatchCommand::Summary {
            thing_id: thing_id.to_string(),
            reply,
        })
        .await
        .ok()?;
        rx.await.ok().flatten()
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Wrap a serializable payload into a successful ToolResult.
pub(crate) fn tool_ok(payload: impl serde::Serialize) -> anyhow::Result<ToolResult> {
    Ok(ToolResult {
        success: true,
        output: serde_json::to_string(&payload).unwrap_or_default(),
        error: None,
    })
}

/// Wrap an error message into a failed ToolResult.
pub(crate) fn tool_err(msg: impl Into<String>) -> anyhow::Result<ToolResult> {
    Ok(ToolResult {
        success: false,
        output: String::new(),
        error: Some(msg.into()),
    })
}

/// Clamp limit to [1, max], defaulting when None.
pub(crate) fn clamp_limit(limit: Option<u32>, default: u32, max: u32) -> u32 {
    limit.unwrap_or(default).clamp(1, max)
}

// ============================================================================
// E2：manager 迟绑定槽（watch 工具经命令通道写穿内存注册表）
// ============================================================================

/// 照 [`LiveSummarySlot`] 先例的迟绑定槽：bootstrap 早期构建工具上下文时
/// manager 尚未就绪，就绪后 `set`；查询走每工作区 watch 命令通道（actor
/// 单写者不破）。未 set = 只落 DB（下次启动重织自愈）。
#[derive(Clone, Default)]
pub struct ManagerSlot {
    inner: Arc<std::sync::RwLock<Option<Arc<ThingAgentManager>>>>,
}

impl ManagerSlot {
    pub fn set(&self, manager: Arc<ThingAgentManager>) {
        *self.inner.write().expect("manager slot poisoned") = Some(manager);
    }

    fn sink(&self, workspace_id: &str) -> Option<tokio::sync::mpsc::Sender<WatchCommand>> {
        self.inner.read().ok()?.as_ref()?.watch_command_sink(workspace_id)
    }

    pub fn upsert(&self, workspace_id: &str, rule: tinyiothub_core::watch::WatchRule) {
        if let Some(sink) = self.sink(workspace_id)
            && let Err(e) = sink.try_send(WatchCommand::Upsert(rule))
        {
            tracing::warn!(workspace_id, error = %e, "watch 命令通道投递失败（DB 已写，重启重织自愈）");
        }
    }

    pub fn remove(&self, workspace_id: &str, rule_id: &str) {
        if let Some(sink) = self.sink(workspace_id)
            && let Err(e) = sink.try_send(WatchCommand::Remove(rule_id.to_string()))
        {
            tracing::warn!(workspace_id, error = %e, "unwatch 命令通道投递失败（DB 已删）");
        }
    }
}

// ============================================================================
// Factory: E2 固定设备工具集 + 隐藏旧别名 + 非设备工具
// ============================================================================

/// thing 工具构建期需要的数据句柄（组合层持有；agent crate 的
/// `ToolRuntimeContext` 只携带存储无关字段，这些由组合层捕获/拼装）。
#[derive(Clone, Default)]
pub struct ThingToolContext {
    pub device_cache: Option<Arc<tinyiothub_storage::cache::ThingCache>>,
    pub data_server: Option<Arc<tinyiothub_runtime::DataServer>>,
    pub pending_actions: Option<Arc<PendingActionStore>>,
    /// E1 仲裁（Task 9）：跨 chat/自治 run 共享的每设备互斥锁注册表；
    /// None = 不仲裁。
    pub arbitration: Option<Arc<ArbitrationRegistry>>,
    /// E1 值守读面（Task 10）：get_thing_profile 附 live 摘要；
    /// None = 未接线（live=null）。
    pub live: Option<Arc<dyn LiveSummarySource>>,
    /// E2 set_device：属性变更事件发布（DataServer 消费后更新缓存）。
    pub event_bus: Option<Arc<tinyiothub_runtime::event_bus::EventBus>>,
    /// E2 watch/unwatch：manager 迟绑定槽（内存注册表写穿）。
    pub manager: Option<ManagerSlot>,
    /// E4a 调用边界校验：None = 未接线（既有行为不变）。
    pub access: Option<Arc<dyn tinyiothub_runtime::access::validator::AccessCallValidator>>,
    /// E4b 近期读取：read_device/read_property 成功读取后写入 DemandTracker。
    /// None = 未接线（既有行为不变）。
    pub demand: Option<Arc<dyn tinyiothub_runtime::access::DemandRecorder>>,
    /// E4c 声明资源解析：call/set 在 claim 前从声明解析资源规格。
    /// None = 未接线（沿用 E1 每设备合成资源 claim）。
    pub resources: Option<Arc<dyn ResourceResolver>>,
}

/// E4c：runtime 的声明解析结果 → agent 仲裁规格（agent 资源类型不进
/// runtime，转换留在组合层）。
pub(crate) fn resource_specs(claims: &[ResourceClaim]) -> Vec<ResourceSpec> {
    claims
        .iter()
        .map(|claim| ResourceSpec {
            id: claim.id.clone(),
            mode: if claim.exclusive {
                ResourceMode::Exclusive
            } else {
                ResourceMode::Shared
            },
            capacity: if claim.exclusive { 1 } else { claim.capacity },
        })
        .collect()
}

/// Create the E2 fixed device tool set + non-device tools + hidden legacy aliases.
///
/// 模型可见设备工具恒为 7 个：list_devices / read_device / call_device /
/// set_device / stop / watch / unwatch。旧 5 个设备名作为隐藏别名注册
/// （批准通道可解析存量提案），《E3 删除》。
pub fn create_thing_tools(
    pool: SqlitePool,
    workspace_id: &str,
    runtime: &ThingToolContext,
) -> Vec<(Box<dyn Tool>, ToolSafety)> {
    let thing_service = Arc::new(ThingService::new(pool.clone()));
    let ws = workspace_id.to_string();
    let manager = runtime.manager.clone().unwrap_or_default();
    let pending = runtime
        .pending_actions
        .clone()
        .expect("pending_actions must be wired in ThingToolContext");

    let read_only = |t: Box<dyn Tool>| (t, ToolSafety::ReadOnly);
    let destructive = |t: Box<dyn Tool>| (t, ToolSafety::Destructive);

    let mut tools: Vec<(Box<dyn Tool>, ToolSafety)> = vec![
        // ── 固定设备工具（模型可见）──
        read_only(device::list_devices_tool(thing_service.clone(), ws.clone())),
        read_only(Box::new(device::ReadDeviceTool {
            thing_service: thing_service.clone(),
            workspace_id: ws.clone(),
            live: runtime.live.clone(),
            device_cache: runtime.device_cache.clone(),
            demand: runtime.demand.clone(),
        })),
        destructive(device::call_device_tool(Box::new(InvokeActionTool {
            thing_service: thing_service.clone(),
            pool: pool.clone(),
            workspace_id: ws.clone(),
            data_server: runtime.data_server.clone(),
            pending_actions: pending.clone(),
            arbitration: runtime.arbitration.clone(),
            arbitration_holder: format!("chat:{ws}"),
            approval: None,
            access: runtime.access.clone(),
            resources: runtime.resources.clone(),
        }))),
        destructive(Box::new(device::SetDeviceTool {
            thing_service: thing_service.clone(),
            pool: pool.clone(),
            workspace_id: ws.clone(),
            event_bus: runtime.event_bus.clone(),
            arbitration: runtime.arbitration.clone(),
            holder: format!("chat:{ws}"),
            resources: runtime.resources.clone(),
        })),
        destructive(Box::new(device::StopTool {
            arbitration: runtime.arbitration.clone(),
            data_server: runtime.data_server.clone(),
        })),
        read_only(Box::new(watch_tools::WatchTool {
            pool: pool.clone(),
            workspace_id: ws.clone(),
            manager: manager.clone(),
        })),
        read_only(Box::new(watch_tools::UnwatchTool {
            pool: pool.clone(),
            workspace_id: ws.clone(),
            manager,
        })),
        // ── 非设备工具（独立类别）──
        read_only(Box::new(GetThingTreeTool {
            thing_service: thing_service.clone(),
            workspace_id: ws.clone(),
        })),
        read_only(Box::new(QueryEventsTool {
            pool: pool.clone(),
            workspace_id: ws.clone(),
        })),
        read_only(Box::new(SearchKnowledgeTool {
            pool: pool.clone(),
            workspace_id: ws.clone(),
        })),
        read_only(Box::new(ReadDocumentTool {
            pool: pool.clone(),
            workspace_id: ws.clone(),
        })),
        // ── E2 composite（工具列表恒定两条）──
        read_only(Box::new(composite_tools::ListCompositesTool {
            pool: pool.clone(),
            workspace_id: ws.clone(),
        })),
        destructive(Box::new(composite_tools::RunCompositeTool {
            pool: pool.clone(),
            workspace_id: ws.clone(),
            pending_actions: pending.clone(),
            executor: Arc::new(crate::domains::agent::host::composite::CompositeExecutor {
                pool: pool.clone(),
                workspace_id: ws.clone(),
                data_server: runtime.data_server.clone(),
                event_bus: runtime.event_bus.clone(),
                arbitration: runtime.arbitration.clone(),
                holder: format!("chat:{ws}"),
            }),
        })),
        // ── 旧 9 工具（隐藏别名：批准通道兼容，模型不可见）──
        read_only(Box::new(device::HiddenTool(Box::new(ListThingsTool {
            thing_service: thing_service.clone(),
            workspace_id: ws.clone(),
        })))),
        read_only(Box::new(device::HiddenTool(Box::new(GetThingTool {
            thing_service: thing_service.clone(),
            workspace_id: ws.clone(),
        })))),
        read_only(Box::new(device::HiddenTool(Box::new(GetThingProfileTool {
            thing_service: thing_service.clone(),
            workspace_id: ws.clone(),
            live: runtime.live.clone(),
            demand: runtime.demand.clone(),
        })))),
        read_only(Box::new(device::HiddenTool(Box::new(ReadPropertyTool {
            thing_service: thing_service.clone(),
            pool: pool.clone(),
            workspace_id: ws.clone(),
            device_cache: runtime.device_cache.clone(),
            demand: runtime.demand.clone(),
        })))),
        destructive(Box::new(device::HiddenTool(Box::new(InvokeActionTool {
            thing_service,
            pool,
            workspace_id: ws.clone(),
            data_server: runtime.data_server.clone(),
            pending_actions: pending,
            arbitration: runtime.arbitration.clone(),
            arbitration_holder: format!("chat:{ws}"),
            approval: None,
            access: runtime.access.clone(),
            resources: runtime.resources.clone(),
        })))),
    ];

    // 工具名唯一性由注册表/测试保证；这里只排序让输出稳定（测试友好）。
    tools.sort_by(|a, b| a.0.name().cmp(b.0.name()));
    tools
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ── list_things pagination clamp ──────────────────────

    /// Verify that limit=500 is clamped down to 200 (max).
    #[test]
    fn test_list_things_pagination_clamp() {
        assert_eq!(clamp_limit(None, 50, 200), 50, "None => default 50");
        assert_eq!(clamp_limit(Some(30), 50, 200), 30, "explicit within range");
        assert_eq!(clamp_limit(Some(500), 50, 200), 200, "500 => max 200");
        assert_eq!(clamp_limit(Some(0), 50, 200), 1, "0 => min 1");
        assert_eq!(clamp_limit(Some(1000), 50, 200), 200, "1000 => max 200");
    }

    // ── get_thing_tree depth clamp ──────────────────────

    #[test]
    fn test_get_thing_tree_depth_clamp() {
        // Depth used via .clamp(1, 10) in execute
        assert_eq!(3.clamp(1, 10), 3, "None default 3");
        assert_eq!(5u32.clamp(1, 10), 5, "explicit 5");
        assert_eq!(0u32.clamp(1, 10), 1, "0 => min 1");
        assert_eq!(50u32.clamp(1, 10), 10, "50 => max 10");
    }

    // ── invoke_action non-device rejection ──────────────

    /// Verify that invoke_action is classified as Write (not Destructive) by
    /// name-based classification. The factory explicitly declares it as
    /// Destructive for trust enforcement.
    #[test]
    fn test_invoke_action_rejects_non_device_type_in_schema() {
        // Name-based classification: invoke_action → Write (not Destructive)
        assert_eq!(
            tinyiothub_skills::trust::classify_tool_safety("invoke_action"),
            ToolSafety::Write,
            "invoke_action is Write by name pattern; factory overrides to Destructive"
        );
    }

    // ── snake_case 参数键兼容（提案盲写兜底）──────────────

    /// 回归（2026-09-24 实测）：巡检报告的 proposals[].parameters 随模板写成
    /// 蛇形键（thing_id），批准执行时 serde 报 missing field `thingId`。
    /// 工具 Input 加 alias / 取值兜底后，驼峰与蛇形键都接受。
    #[tokio::test]
    async fn tools_accept_snake_case_parameter_keys() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        tinyiothub_storage::test_helpers::run_all_migrations(&pool)
            .await
            .unwrap();
        let tools = create_thing_tools(
            pool,
            "ws",
            &ThingToolContext {
                pending_actions: Some(Arc::new(PendingActionStore::default())),
                ..Default::default()
            },
        );
        let find = |name: &str| tools.iter().find(|(t, _)| t.name() == name).map(|(t, _)| t);

        // serde 路径：解析必须过关——失败只能来自下游（物不存在/文档未找到），
        // 不能是「参数解析失败」。每个加了 alias 的工具都覆盖一个蛇形键载荷。
        for (name, args) in [
            (
                "read_property",
                serde_json::json!({"thing_id": "dev_1", "property_name": "temp"}),
            ),
            (
                "invoke_action",
                serde_json::json!({"thing_id": "dev_1", "action_name": "reboot"}),
            ),
            (
                "list_things",
                serde_json::json!({"thing_type": "device", "parent_id": "p1"}),
            ),
            ("get_thing_tree", serde_json::json!({"root_id": "dev_1"})),
            (
                "query_events",
                serde_json::json!({"thing_id": "dev_1", "event_name": "offline"}),
            ),
            (
                "search_knowledge",
                serde_json::json!({"thing_id": "dev_1", "q": "轴承"}),
            ),
            ("read_document", serde_json::json!({"resource_id": "doc_1"})),
        ] {
            let msg = match find(name).unwrap().execute(args).await {
                Ok(r) => r.error.unwrap_or_default(),
                Err(e) => e.to_string(),
            };
            assert!(!msg.contains("参数解析失败"), "{name} 必须接受蛇形键: {msg}");
        }

        // 手动取值路径（get_thing / get_thing_profile）：不能报「缺少必需参数」。
        for name in ["get_thing", "get_thing_profile"] {
            let msg = match find(name)
                .unwrap()
                .execute(serde_json::json!({"thing_id": "dev_1"}))
                .await
            {
                Ok(r) => r.error.unwrap_or_default(),
                Err(e) => e.to_string(),
            };
            assert!(!msg.contains("缺少必需参数"), "{name} 必须接受蛇形键: {msg}");
        }
    }

    // ── LiveSummarySlot（E1 Task 10）────────────────────

    /// 未 set（manager 未就绪）→ None：profile live=null，不炸。
    #[tokio::test]
    async fn live_summary_slot_unset_returns_none() {
        let slot = LiveSummarySlot::default();
        assert!(slot.summary("ws", "t1").await.is_none());
    }

    // ── tool name uniqueness ────────────────────────────

    /// Verify all 9 tools are uniquely named.
    #[test]
    fn test_all_9_tool_names_unique() {
        // This test validates that the 9 tool names are correct and unique.
        let names = vec![
            "list_things",
            "get_thing",
            "get_thing_profile",
            "get_thing_tree",
            "read_property",
            "invoke_action",
            "query_events",
            "search_knowledge",
            "read_document",
        ];
        assert_eq!(names.len(), 9, "should have exactly 9 tools");

        // Check uniqueness
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 9, "all 9 tool names must be unique");
    }

    // ── safety classification on name pattern ──────────

    #[test]
    fn test_classify_tool_safety_by_name() {
        use tinyiothub_skills::trust::classify_tool_safety;

        // Read-only: starts with list_/get_/read_/search_
        assert_eq!(classify_tool_safety("list_things"), ToolSafety::ReadOnly);
        assert_eq!(classify_tool_safety("get_thing"), ToolSafety::ReadOnly);
        assert_eq!(classify_tool_safety("get_thing_profile"), ToolSafety::ReadOnly);
        assert_eq!(classify_tool_safety("get_thing_tree"), ToolSafety::ReadOnly);
        assert_eq!(classify_tool_safety("read_property"), ToolSafety::ReadOnly);
        assert_eq!(classify_tool_safety("read_document"), ToolSafety::ReadOnly);
        assert_eq!(classify_tool_safety("search_knowledge"), ToolSafety::ReadOnly);

        // Write: doesn't match read/destructive patterns
        assert_eq!(classify_tool_safety("invoke_action"), ToolSafety::Write);
        assert_eq!(classify_tool_safety("query_events"), ToolSafety::Write);
    }

    // ── E2：固定设备工具集 + 隐藏别名 ─────────────────

    #[tokio::test]
    async fn fixed_device_tools_exposed_and_legacy_aliases_hidden() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        let tools = create_thing_tools(
            pool,
            "ws",
            &ThingToolContext {
                pending_actions: Some(Arc::new(PendingActionStore::default())),
                ..Default::default()
            },
        );

        let mut exposed: Vec<&str> = tools
            .iter()
            .filter(|(t, _)| t.exposed())
            .map(|(t, _)| t.name())
            .collect();
        exposed.sort_unstable();
        assert_eq!(
            exposed,
            vec![
                "call_device",
                "get_thing_tree",
                "list_composites",
                "list_devices",
                "query_events",
                "read_device",
                "read_document",
                "run_composite",
                "search_knowledge",
                "set_device",
                "stop",
                "unwatch",
                "watch",
            ],
            "模型可见设备工具恒为固定 7 个 + 非设备 4 个 + composite 2 个"
        );

        for legacy in [
            "list_things",
            "get_thing",
            "get_thing_profile",
            "read_property",
            "invoke_action",
        ] {
            let (tool, _) = tools
                .iter()
                .find(|(t, _)| t.name() == legacy)
                .unwrap_or_else(|| panic!("legacy alias {legacy} must stay registered"));
            assert!(!tool.exposed(), "legacy alias {legacy} must be hidden");
        }
    }

    /// E2 watch/unwatch：真实 DB 往返（manager 未接线时只落 DB）。
    #[tokio::test]
    async fn watch_tool_registers_and_unwatch_removes() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        tinyiothub_storage::test_helpers::run_all_migrations(&pool)
            .await
            .unwrap();
        crate::domains::agent::host::test_utils::seed_test_workspace(&pool, "tenant-1", "ws").await;
        sqlx::query("INSERT INTO things (id, name, workspace_id, thing_type) VALUES ('t1','T1','ws','device')")
            .execute(&pool)
            .await
            .unwrap();

        let watch = watch_tools::WatchTool {
            pool: pool.clone(),
            workspace_id: "ws".into(),
            manager: ManagerSlot::default(),
        };
        let out = watch
            .execute(serde_json::json!({
                "thingId": "t1",
                "property": "温度",
                "op": "gt",
                "value": 30
            }))
            .await
            .unwrap();
        assert!(out.success, "{}", out.error.unwrap_or_default());
        let payload: serde_json::Value = serde_json::from_str(&out.output).unwrap();
        let rule_id = payload["ruleId"].as_str().expect("ruleId");
        assert_eq!(payload["persistent"], false, "会话 watch 必须不持久");
        let rows = tinyiothub_storage::watch::list_rules(&pool).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].persistent, 0);

        let unwatch = watch_tools::UnwatchTool {
            pool: pool.clone(),
            workspace_id: "ws".into(),
            manager: ManagerSlot::default(),
        };
        let out = unwatch.execute(serde_json::json!({"ruleId": rule_id})).await.unwrap();
        assert!(out.success, "{}", out.error.unwrap_or_default());
        assert!(tinyiothub_storage::watch::list_rules(&pool).await.unwrap().is_empty());
    }

    /// E2 read_device：live 摘要存在时一次返回 state+health+zone+text。
    #[tokio::test]
    async fn read_device_returns_live_summary_and_text() {
        use tinyiothub_runtime::watch::live_state::{Health, ThingLiveSummary};

        struct StubLive(ThingLiveSummary);

        #[async_trait::async_trait]
        impl LiveSummarySource for StubLive {
            async fn summary(&self, _ws: &str, _thing_id: &str) -> Option<ThingLiveSummary> {
                Some(self.0.clone())
            }
        }

        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        tinyiothub_storage::test_helpers::run_all_migrations(&pool)
            .await
            .unwrap();
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
        let service = Arc::new(ThingService::new(pool));
        let created = service
            .create_thing(
                &serde_json::from_value(serde_json::json!({"name":"温感-1","thingType":"device"})).unwrap(),
                Some("ws"),
            )
            .await
            .unwrap();

        let mut values = std::collections::HashMap::new();
        values.insert("温度".to_string(), "31.5".to_string());
        let tool = device::ReadDeviceTool {
            thing_service: service,
            workspace_id: "ws".into(),
            live: Some(Arc::new(StubLive(ThingLiveSummary {
                health: Health::Attention,
                zone: "园区A/3号楼".into(),
                online: false,
                values,
            }))),
            device_cache: None,
            demand: None,
        };
        let out = tool
            .execute(serde_json::json!({"thingId": created.id, "properties": ["温度"]}))
            .await
            .unwrap();
        assert!(out.success, "{}", out.error.unwrap_or_default());
        let v: serde_json::Value = serde_json::from_str(&out.output).unwrap();
        assert_eq!(v["health"], "attention");
        assert_eq!(v["zone"], "园区A/3号楼");
        assert_eq!(v["state"]["温度"], "31.5");
        assert!(v["text"].as_str().unwrap().contains("园区A/3号楼"));
        assert_eq!(v["v"], 1);
    }

    // ── E4b：读取写入 DemandRecorder ─────────────────────

    #[derive(Default)]
    struct RecordingDemand(std::sync::Mutex<Vec<(String, String, String)>>);

    impl RecordingDemand {
        fn reads(&self) -> Vec<(String, String, String)> {
            self.0.lock().unwrap().clone()
        }
    }

    impl tinyiothub_runtime::access::DemandRecorder for RecordingDemand {
        fn record_read(&self, workspace_id: &str, thing_id: &str, property: &str) {
            self.0
                .lock()
                .unwrap()
                .push((workspace_id.into(), thing_id.into(), property.into()));
        }
    }

    async fn pool_with_thing_and_property(name: &str, data_type: &str) -> (SqlitePool, String) {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        tinyiothub_storage::test_helpers::run_all_migrations(&pool)
            .await
            .unwrap();
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
        let service = ThingService::new(pool.clone());
        let created = service
            .create_thing(
                &serde_json::from_value(serde_json::json!({"name":"温感-1","thingType":"device"})).unwrap(),
                Some("ws"),
            )
            .await
            .unwrap();
        sqlx::query("INSERT INTO thing_properties (id, thing_id, name, data_type) VALUES ('prop-1', ?, ?, ?)")
            .bind(&created.id)
            .bind(name)
            .bind(data_type)
            .execute(&pool)
            .await
            .unwrap();
        (pool, created.id)
    }

    #[tokio::test]
    async fn read_device_records_requested_or_returned_properties() {
        use tinyiothub_runtime::watch::live_state::{Health, ThingLiveSummary};

        struct StubLive(ThingLiveSummary);

        #[async_trait::async_trait]
        impl LiveSummarySource for StubLive {
            async fn summary(&self, _ws: &str, _thing_id: &str) -> Option<ThingLiveSummary> {
                Some(self.0.clone())
            }
        }

        let (pool, thing_id) = pool_with_thing_and_property("温度", "float").await;
        let service = Arc::new(ThingService::new(pool));
        let mut values = std::collections::HashMap::new();
        values.insert("温度".to_string(), "31.5".to_string());
        values.insert("湿度".to_string(), "60".to_string());
        let live: Arc<dyn LiveSummarySource> = Arc::new(StubLive(ThingLiveSummary {
            health: Health::Ok,
            zone: String::new(),
            online: true,
            values,
        }));
        let demand = Arc::new(RecordingDemand::default());
        let tool = device::ReadDeviceTool {
            thing_service: service,
            workspace_id: "ws".into(),
            live: Some(live),
            device_cache: None,
            demand: Some(demand.clone()),
        };

        // Explicit properties: every requested name is recorded — including
        // ones absent from the current state (unsampled), which is exactly
        // the case demand is meant to pull into sampling.
        tool.execute(serde_json::json!({"thingId": thing_id, "properties": ["温度", "不存在"]}))
            .await
            .unwrap();
        assert_eq!(
            demand.reads(),
            vec![
                ("ws".to_string(), thing_id.clone(), "温度".to_string()),
                ("ws".to_string(), thing_id.clone(), "不存在".to_string()),
            ],
            "every requested property is recorded, sampled or not"
        );

        // Requested-but-unsampled (never present in LiveState) still records.
        demand.0.lock().unwrap().clear();
        tool.execute(serde_json::json!({"thingId": thing_id, "properties": ["未采样"]}))
            .await
            .unwrap();
        assert_eq!(
            demand.reads(),
            vec![("ws".to_string(), thing_id.clone(), "未采样".to_string())],
            "unsampled requested property must gain demand"
        );

        // No properties arg: every *declared* property is recorded; the
        // undeclared LiveState key 湿度 is not.
        demand.0.lock().unwrap().clear();
        tool.execute(serde_json::json!({"thingId": thing_id})).await.unwrap();
        assert_eq!(
            demand.reads(),
            vec![("ws".to_string(), thing_id.clone(), "温度".to_string())],
            "only declared properties are recorded when properties is omitted"
        );

        // Failure path records nothing.
        demand.0.lock().unwrap().clear();
        let out = tool.execute(serde_json::json!({"thingId": "missing"})).await.unwrap();
        assert!(!out.success);
        assert!(demand.reads().is_empty(), "failed reads must not record demand");
    }

    // ── E4c：只读工具不解析/claim 资源 ──────────────────

    #[derive(Default)]
    struct RecordingResolver(std::sync::Mutex<Vec<String>>);

    #[async_trait::async_trait]
    impl ResourceResolver for RecordingResolver {
        async fn resolve_action(
            &self,
            _ws: &str,
            _thing_id: &str,
            tool_name: &str,
        ) -> Result<Vec<ResourceClaim>, String> {
            self.0.lock().unwrap().push(format!("action:{tool_name}"));
            Ok(vec![])
        }

        async fn resolve_properties(
            &self,
            _ws: &str,
            _thing_id: &str,
            properties: &[String],
        ) -> Result<Vec<ResourceClaim>, String> {
            self.0.lock().unwrap().push(format!("properties:{properties:?}"));
            Ok(vec![])
        }
    }

    /// runtime 侧合成资源 id 与 agent 仲裁常量必须一致（两者各自定义以免
    /// runtime 依赖 agent）。
    #[test]
    fn device_resource_constants_agree_between_runtime_and_agent() {
        assert_eq!(
            tinyiothub_runtime::access::resources::DEVICE_RESOURCE,
            tinyiothub_agent::runtime::thing_agent::DEVICE_RESOURCE
        );
    }

    #[tokio::test]
    async fn read_tools_do_not_resolve_or_claim_resources() {
        let (pool, thing_id) = pool_with_thing_and_property("温度", "float").await;
        let resolver = Arc::new(RecordingResolver::default());
        let tools = create_thing_tools(
            pool.clone(),
            "ws",
            &ThingToolContext {
                pending_actions: Some(Arc::new(PendingActionStore::default())),
                resources: Some(resolver.clone()),
                ..Default::default()
            },
        );
        let find = |name: &str| tools.iter().find(|(t, _)| t.name() == name).map(|(t, _)| t);

        let read = find("read_device")
            .expect("read_device")
            .execute(serde_json::json!({"thingId": thing_id}))
            .await
            .unwrap();
        assert!(read.success, "{:?}", read.error);
        let read = find("read_property")
            .expect("read_property")
            .execute(serde_json::json!({"thingId": thing_id, "propertyName": "温度"}))
            .await
            .unwrap();
        assert!(read.success, "{:?}", read.error);

        assert!(resolver.0.lock().unwrap().is_empty(), "只读工具不得解析或 claim 资源");
    }

    #[tokio::test]
    async fn read_property_records_only_successful_reads() {
        let (pool, thing_id) = pool_with_thing_and_property("温度", "float").await;
        let demand = Arc::new(RecordingDemand::default());
        let tool = ReadPropertyTool {
            thing_service: Arc::new(ThingService::new(pool.clone())),
            pool,
            workspace_id: "ws".into(),
            device_cache: None,
            demand: Some(demand.clone()),
        };

        let out = tool
            .execute(serde_json::json!({"thingId": thing_id, "propertyName": "温度"}))
            .await
            .unwrap();
        assert!(out.success, "{}", out.error.unwrap_or_default());
        assert_eq!(
            demand.reads(),
            vec![("ws".to_string(), thing_id.clone(), "温度".to_string())]
        );

        // Unknown property → not-found error, no demand recorded.
        demand.0.lock().unwrap().clear();
        let result = tool
            .execute(serde_json::json!({"thingId": thing_id, "propertyName": "不存在"}))
            .await;
        assert!(result.is_err(), "unknown property must fail");
        assert!(demand.reads().is_empty());
    }
}
