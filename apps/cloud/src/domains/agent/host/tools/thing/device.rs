//! E2 固定设备工具集（spec 2026-10-21-e2 §3）：
//! list_devices / read_device / call_device / set_device / stop。
//!
//! 工具列表不随设备增减变化；设备自有 action 是 `call_device` 的参数。
//! 旧 9 工具保留为隐藏别名（[`HiddenTool`]，`exposed() == false`）。

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use tinyiothub_agent::port::attribution::{Attributable, Role, ToolKind};
use tinyiothub_agent::port::tool::{Tool, ToolResult};
use tinyiothub_agent::runtime::thing_agent::arbitration::DEFAULT_STOP_TTL;
use tinyiothub_agent::runtime::thing_agent::{ArbitrationRegistry, DEFAULT_LEASE_TTL};
use tinyiothub_core::models::event::{ContentElement, Event, EventSource, RichContent, TextFormat};
use tinyiothub_runtime::access::DemandRecorder;
use tinyiothub_runtime::access::resources::ResourceResolver;
use tinyiothub_runtime::event_bus::EventBus;

use super::profile::health_str;
use super::{LiveSummarySource, resource_specs, tool_err, tool_ok};
use crate::domains::thing::service::ThingService;

/// 隐藏别名：注册可解析（存量提案/批准通道），但不进模型工具列表与目录。
pub(super) struct HiddenTool(pub(super) Box<dyn Tool>);

impl Attributable for HiddenTool {
    fn role(&self) -> Role {
        self.0.role()
    }
    fn alias(&self) -> &str {
        self.0.alias()
    }
}

#[async_trait]
impl Tool for HiddenTool {
    fn name(&self) -> &str {
        self.0.name()
    }
    fn description(&self) -> &str {
        self.0.description()
    }
    fn parameters_schema(&self) -> Value {
        self.0.parameters_schema()
    }
    fn exposed(&self) -> bool {
        false
    }
    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        self.0.execute(args).await
    }
}

/// 把既有实现以固定名/固定 schema 暴露（新工具是独立契约，旧实现语义不改）。
pub(super) struct ToolAlias {
    inner: Box<dyn Tool>,
    name: &'static str,
    description: &'static str,
    schema: fn() -> Value,
}

impl ToolAlias {
    pub(super) fn new(
        inner: Box<dyn Tool>,
        name: &'static str,
        description: &'static str,
        schema: fn() -> Value,
    ) -> Self {
        Self {
            inner,
            name,
            description,
            schema,
        }
    }
}

impl Attributable for ToolAlias {
    fn role(&self) -> Role {
        self.inner.role()
    }
    fn alias(&self) -> &str {
        self.name
    }
}

#[async_trait]
impl Tool for ToolAlias {
    fn name(&self) -> &str {
        self.name
    }
    fn description(&self) -> &str {
        self.description
    }
    fn parameters_schema(&self) -> Value {
        (self.schema)()
    }
    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        self.inner.execute(args).await
    }
}

fn list_devices_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "thingType": { "type": "string", "description": "device, space, line, building" },
            "parentId": { "type": "string", "description": "父节点 ID（可选）" },
            "q": { "type": "string", "description": "名称/描述关键词" },
            "tags": { "type": "string", "description": "标签（逗号分隔）" },
            "limit": { "type": "integer", "default": 50 },
            "offset": { "type": "integer", "default": 0 }
        }
    })
}

fn call_device_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "thingId": { "type": "string", "description": "物 ID（必需）" },
            "action": { "type": "string", "description": "该设备声明的动作名（必需）" },
            "params": { "type": "object", "description": "动作参数（可选）" }
        },
        "required": ["thingId", "action"]
    })
}

/// list_devices：固定形态的设备清单（实现复用 list_things）。
pub(super) fn list_devices_tool(thing_service: Arc<ThingService>, workspace_id: String) -> Box<dyn Tool> {
    Box::new(ToolAlias::new(
        Box::new(super::list::ListThingsTool {
            thing_service,
            workspace_id,
        }),
        "list_devices",
        "列出工作区内的物/设备（分页）。需要总览时优先用它，而不是逐物查询。",
        list_devices_schema,
    ))
}

/// call_device：固定形态的动作调用（实现复用 invoke_action 的确认/仲裁/下发）。
pub(super) fn call_device_tool(inner: Box<dyn Tool>) -> Box<dyn Tool> {
    Box::new(ToolAlias::new(
        inner,
        "call_device",
        "对设备执行其声明的动作（开关/重启/设置参数等）。参数：thingId + action + params。",
        call_device_schema,
    ))
}

// ============================================================================
// read_device：一次返回 state + health + zone + text（AI 不再逐物查 profile）
// ============================================================================

pub(super) struct ReadDeviceTool {
    pub(super) thing_service: Arc<ThingService>,
    pub(super) workspace_id: String,
    pub(super) live: Option<Arc<dyn LiveSummarySource>>,
    /// 无实时态时的属性值回退源（cache 是最新真相；None = 只回 live）。
    pub(super) device_cache: Option<Arc<tinyiothub_storage::cache::ThingCache>>,
    /// E4b：读取成功后写入 DemandTracker（None = 未接线）。
    pub(super) demand: Option<Arc<dyn DemandRecorder>>,
}

impl Attributable for ReadDeviceTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Search)
    }
    fn alias(&self) -> &str {
        self.name()
    }
}

#[async_trait]
impl Tool for ReadDeviceTool {
    fn name(&self) -> &str {
        "read_device"
    }

    fn description(&self) -> &str {
        "读取单个设备的实时摘要：state（属性值）+ health（ok/attention/bad/unknown）\
         + zone（空间归属）+ online，并附一句给模型的 text。\
         properties 可选——只回指定属性；缺省回全部已上报值。"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "thingId": { "type": "string", "description": "物 ID（必需）" },
                "properties": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "只返回这些属性名（可选）"
                }
            },
            "required": ["thingId"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Input {
            #[serde(alias = "thing_id")]
            thing_id: String,
            properties: Option<Vec<String>>,
        }

        let input: Input = serde_json::from_value(args).map_err(|e| anyhow::anyhow!("参数解析失败: {}", e))?;
        let thing = match self.thing_service.get_thing(&input.thing_id, &self.workspace_id).await {
            Ok(t) => t,
            Err(e) => return tool_err(format!("物不存在: {e}")),
        };

        let summary = match &self.live {
            Some(source) => source.summary(&self.workspace_id, &input.thing_id).await,
            None => None,
        };

        // state：LiveState 已上报值优先；无实时态回退缓存里的属性当前值。
        let mut state: BTreeMap<String, Value> = BTreeMap::new();
        if let Some(s) = &summary {
            for (k, v) in &s.values {
                state.insert(k.clone(), Value::String(v.clone()));
            }
        } else if let Some(thing_live) = self.device_cache.as_ref().and_then(|c| c.get(&input.thing_id))
            && let Some(props) = &thing_live.properties
        {
            for p in props {
                if let Some(v) = &p.current_value {
                    state.insert(p.name.clone(), Value::String(v.clone()));
                }
            }
        }
        // 读取成功（物存在）→ 记录需求。显式 properties 记全部请求名
        // （即使当前未采样、state 里没有值——这正是需求要拉起的场景）；
        // 缺省则记该物全部声明属性，不把 LiveState 里的未声明键当成需求。
        // 失败/未找到在更早处已返回，不会写入。
        if let Some(demand) = &self.demand {
            match &input.properties {
                Some(wanted) => {
                    for property in wanted {
                        demand.record_read(&self.workspace_id, &input.thing_id, property);
                    }
                }
                None => {
                    for property in self.thing_service.load_property_names(&input.thing_id).await {
                        demand.record_read(&self.workspace_id, &input.thing_id, &property);
                    }
                }
            }
        }

        if let Some(wanted) = &input.properties {
            let filtered: BTreeMap<String, Value> = wanted
                .iter()
                .map(|k| (k.clone(), state.get(k).cloned().unwrap_or(Value::Null)))
                .collect();
            state = filtered;
        }

        let (health, zone, online) = match &summary {
            Some(s) => (health_str(s.health), s.zone.clone(), s.online),
            None => ("unknown", String::new(), false),
        };
        let display = thing.display_name.as_deref().unwrap_or(&thing.name);
        let zone_text = if zone.is_empty() {
            "未归属区域".to_string()
        } else {
            zone.clone()
        };
        let text = format!(
            "{display}（{}）位于 {zone_text}，健康度 {health}，{}",
            thing.thing_type,
            if online { "在线" } else { "离线" }
        );

        tool_ok(json!({
            "v": 1,
            "thingId": input.thing_id,
            "name": display,
            "thingType": thing.thing_type,
            "health": health,
            "zone": zone,
            "online": online,
            "position": thing.address,
            "state": state,
            "text": text,
        }))
    }
}

// ============================================================================
// set_device：写属性目标值（只读/派生属性拒绝）
// ============================================================================

pub(super) struct SetDeviceTool {
    pub(super) thing_service: Arc<ThingService>,
    pub(super) pool: sqlx::SqlitePool,
    pub(super) workspace_id: String,
    pub(super) event_bus: Option<Arc<EventBus>>,
    pub(super) arbitration: Option<Arc<ArbitrationRegistry>>,
    pub(super) holder: String,
    /// E4c 声明资源解析（None = 未接线）：按写入属性并入的 set 工具资源
    /// claim；未声明 resources 的属性仍 claim 合成 `__device__`（E1）。
    pub(super) resources: Option<Arc<dyn ResourceResolver>>,
}

impl Attributable for SetDeviceTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Plugin)
    }
    fn alias(&self) -> &str {
        self.name()
    }
}

#[async_trait]
impl Tool for SetDeviceTool {
    fn name(&self) -> &str {
        "set_device"
    }

    fn description(&self) -> &str {
        "写入设备属性目标值（properties: {属性名: 值}）。\
         只读属性与派生属性拒绝。执行前会检查 stop 闸门与设备占用。"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "thingId": { "type": "string", "description": "物 ID（必需）" },
                "properties": {
                    "type": "object",
                    "description": "属性名 → 目标值（必需）"
                }
            },
            "required": ["thingId", "properties"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Input {
            #[serde(alias = "thing_id")]
            thing_id: String,
            properties: BTreeMap<String, Value>,
        }

        let input: Input = serde_json::from_value(args).map_err(|e| anyhow::anyhow!("参数解析失败: {}", e))?;
        if input.properties.is_empty() {
            return tool_err("properties 不能为空".to_string());
        }
        if let Some(registry) = &self.arbitration
            && let Some(reason) = registry.stop_reason(&input.thing_id)
        {
            return tool_err(format!("stopped: {reason}"));
        }

        let thing = match self.thing_service.get_thing(&input.thing_id, &self.workspace_id).await {
            Ok(t) => t,
            Err(e) => return tool_err(format!("物不存在: {e}")),
        };
        if thing.thing_type != "device" {
            return tool_err(format!("操作不支持: 物类型为 '{}'", thing.thing_type));
        }
        let Some(event_bus) = &self.event_bus else {
            return tool_err("event bus unavailable: property write NOT executed".to_string());
        };

        let _lease = match &self.arbitration {
            Some(registry) => {
                let acquired = match &self.resources {
                    Some(resolver) => {
                        let names: Vec<String> = input.properties.keys().cloned().collect();
                        match resolver
                            .resolve_properties(&self.workspace_id, &input.thing_id, &names)
                            .await
                        {
                            Ok(claims) => registry.try_acquire_resources(
                                &input.thing_id,
                                self.holder.clone(),
                                &resource_specs(&claims),
                                DEFAULT_LEASE_TTL,
                            ),
                            Err(e) => return tool_err(format!("资源解析失败: {e}")),
                        }
                    }
                    None => registry.try_acquire(&input.thing_id, self.holder.clone(), DEFAULT_LEASE_TTL),
                };
                match acquired {
                    Ok(lease) => Some(lease),
                    Err(busy) if self.resources.is_some() => return tool_err(busy.to_string()),
                    Err(busy) => return tool_err(format!("busy: 设备正被 {} 占用，请稍后重试", busy.holder)),
                }
            }
            None => None,
        };

        let props = match tinyiothub_storage::Db::new(self.pool.clone())
            .find_thing_properties_by_thing_id(&input.thing_id)
            .await
        {
            Ok(p) => p,
            Err(e) => return tool_err(format!("属性查询失败: {e}")),
        };
        let mut updated = Vec::new();
        for (name, value) in &input.properties {
            let Some(prop) = props.iter().find(|p| &p.name == name) else {
                return tool_err(format!("属性不存在: {name}"));
            };
            if prop.is_read_only != 0 {
                return tool_err(format!("只读属性不支持写入: {name}"));
            }
            if prop.source.as_deref() == Some("derived") {
                return tool_err(format!("派生属性不支持手动写值: {name}"));
            }
            let Some(text) = scalar_text(value) else {
                return tool_err(format!("属性值必须是标量: {name}"));
            };
            let source = EventSource::device_property(
                input.thing_id.clone(),
                prop.id.clone(),
                format!("{}:{}", input.thing_id, prop.id),
            );
            let content = RichContent::new(
                format!(
                    "Property Changed: {} - {}",
                    thing.display_name.as_deref().unwrap_or(&thing.name),
                    prop.name
                ),
                vec![ContentElement::Text {
                    content: format!("Current value: {text}"),
                    format: TextFormat::Plain,
                }],
            );
            let event = Event::new_property_change_event(input.thing_id.clone(), prop.id.clone(), source, content)
                .map_err(|e| anyhow::anyhow!("事件构造失败: {e}"))?;
            if let Err(e) = event_bus.publish(event).await {
                return tool_err(format!("属性写入失败: {e}"));
            }
            updated.push(name.clone());
        }

        tool_ok(json!({
            "v": 1,
            "thingId": input.thing_id,
            "status": "ok",
            "updated": updated,
        }))
    }
}

fn scalar_text(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

// ============================================================================
// stop：设备 stop 闸门（E2 退化形态——阻断后续动作+释放占用）
// ============================================================================

pub(super) struct StopTool {
    pub(super) arbitration: Option<Arc<ArbitrationRegistry>>,
}

impl Attributable for StopTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Plugin)
    }
    fn alias(&self) -> &str {
        self.name()
    }
}

#[async_trait]
impl Tool for StopTool {
    fn name(&self) -> &str {
        "stop"
    }

    fn description(&self) -> &str {
        "立即停止对设备的后续操作：置 stop 闸门（默认 60s）并释放设备占用。\
         在途的单次调用不会被中断；闸门期间 call_device/set_device 返回 stopped。"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "thingId": { "type": "string", "description": "物 ID（必需）" },
                "reason": { "type": "string", "description": "停止原因（可选）" },
                "ttlSecs": { "type": "integer", "description": "闸门时长秒（默认 60）" }
            },
            "required": ["thingId"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Input {
            #[serde(alias = "thing_id")]
            thing_id: String,
            reason: Option<String>,
            #[serde(alias = "ttl_secs")]
            ttl_secs: Option<u64>,
        }

        let input: Input = serde_json::from_value(args).map_err(|e| anyhow::anyhow!("参数解析失败: {}", e))?;
        let Some(registry) = &self.arbitration else {
            return tool_err("arbitration unavailable: stop NOT applied".to_string());
        };
        let ttl = input
            .ttl_secs
            .map(std::time::Duration::from_secs)
            .unwrap_or(DEFAULT_STOP_TTL);
        let reason = input.reason.unwrap_or_else(|| "operator stop".to_string());
        registry.set_stop(&input.thing_id, reason.clone(), ttl);
        let until = chrono::Utc::now() + chrono::Duration::seconds(ttl.as_secs() as i64);
        tool_ok(json!({
            "v": 1,
            "thingId": input.thing_id,
            "status": "stopped",
            "reason": reason,
            "until": until.to_rfc3339(),
        }))
    }
}

// ============================================================================
// set_device 测试（E4c：按属性并入的 set 工具资源 claim）
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::agent::host::test_utils::seed_test_workspace;
    use crate::shared::access_ports::{
        DbAcceptanceStore, DbDeviceStore, DbDriverStore, DbProjectionStore, DbThingSource,
    };
    use tinyiothub_agent::runtime::thing_agent::{ResourceMode, ResourceSpec};
    use tinyiothub_core::driver::declaration::{DeviceDeclaration, DeviceDeclarationRecord};
    use tinyiothub_runtime::access::demand::DemandTracker;
    use tinyiothub_runtime::access::registry::ManifestRegistry;
    use tinyiothub_runtime::access::service::AccessService;

    const RESOURCE_MANIFEST: &str = r#"{
      "manifestVersion":"1.0",
      "identity":{"driver":"resource-demo","version":"1.0.0","protocol":"demo",
                  "abiVersion":2,"dynamicCapabilities":true},
      "config":{"type":"object"},"tools":[],
      "stateFields":[{"name":"level","type":"integer"},{"name":"mode","type":"integer"}],
      "sources":[],
      "resources":[
        {"id":"actuator","mode":"exclusive","capacity":1},
        {"id":"camera","mode":"exclusive","capacity":1}
      ],
      "placement":{"mode":"runtime"},
      "limits":{"maxConcurrentJobs":1,"maxCallsPerMin":60}
    }"#;

    async fn pool_with_device() -> sqlx::SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        tinyiothub_storage::migrations::run_migrations(&pool)
            .await
            .expect("migrations");
        seed_test_workspace(&pool, "tenant-1", "ws-set").await;
        sqlx::query("INSERT INTO things (id, name, workspace_id, thing_type) VALUES ('dev-1','Dev','ws-set','device')")
            .execute(&pool)
            .await
            .expect("insert device");
        for name in ["level", "mode"] {
            sqlx::query(
                "INSERT INTO thing_properties (id, thing_id, name, data_type) VALUES (?, 'dev-1', ?, 'integer')",
            )
            .bind(format!("prop-{name}"))
            .bind(name)
            .execute(&pool)
            .await
            .expect("insert property");
        }
        pool
    }

    async fn resource_access(pool: &sqlx::SqlitePool, tools: Value) -> Arc<AccessService> {
        let db = Arc::new(tinyiothub_storage::Db::new(pool.clone()));
        let manifests = Arc::new(ManifestRegistry::new());
        manifests.seed_builtin(RESOURCE_MANIFEST).expect("resource manifest");
        let access = Arc::new(AccessService::new(
            std::path::PathBuf::from("."),
            manifests,
            Arc::new(DbDriverStore(db.clone())),
            Arc::new(DbDeviceStore(db.clone())),
            Arc::new(DbProjectionStore(db.clone())),
            Arc::new(DbThingSource(db.clone())),
            Arc::new(DbAcceptanceStore(db.clone())),
            Arc::new(DemandTracker::new()),
        ));
        let decl: DeviceDeclaration = serde_json::from_value(json!({
            "manifestVersion": "1.0",
            "driver": "resource-demo",
            "driverVersion": "1.0.0",
            "stateFields": [{"name":"level","type":"integer"},{"name":"mode","type":"integer"}],
            "tools": tools,
        }))
        .expect("declaration json");
        db.upsert_device_declaration(&DeviceDeclarationRecord {
            workspace_id: "ws-set".into(),
            thing_id: "dev-1".into(),
            driver_name: decl.driver.clone(),
            driver_version: decl.driver_version.clone(),
            declaration_json: serde_json::to_string(&decl).unwrap(),
            declaration_hash: "test-hash".into(),
            status: "active".into(),
            updated_at: "2026-10-22T00:00:00Z".into(),
        })
        .await
        .expect("declaration row");
        access
    }

    fn set_tool(
        pool: &sqlx::SqlitePool,
        registry: Arc<ArbitrationRegistry>,
        access: Arc<AccessService>,
    ) -> SetDeviceTool {
        SetDeviceTool {
            thing_service: Arc::new(ThingService::new(pool.clone())),
            pool: pool.clone(),
            workspace_id: "ws-set".into(),
            event_bus: Some(Arc::new(EventBus::new())),
            arbitration: Some(registry),
            holder: "watch-run-B".into(),
            resources: Some(access as Arc<dyn ResourceResolver>),
        }
    }

    fn exclusive(id: &str) -> ResourceSpec {
        ResourceSpec {
            id: id.into(),
            mode: ResourceMode::Exclusive,
            capacity: 1,
        }
    }

    fn set_tools() -> Value {
        json!([
            {"name":"set_level","kind":"set","targetField":"level",
             "parameters":{"type":"object","properties":{"level":{"type":"integer"}}},
             "resources":["actuator"]},
            {"name":"set_mode","kind":"set","targetField":"mode",
             "parameters":{"type":"object","properties":{"mode":{"type":"integer"}}},
             "resources":["camera"]}
        ])
    }

    /// 两个属性的 set 工具资源取并集：任一资源被占都 busy（带 holder+resource）。
    #[tokio::test]
    async fn set_device_unions_set_tool_resources_for_two_properties() {
        let pool = pool_with_device().await;
        let access = resource_access(&pool, set_tools()).await;
        let args = json!({"thingId": "dev-1", "properties": {"level": 10, "mode": 2}});

        let actuator_registry = Arc::new(ArbitrationRegistry::default());
        let tool = set_tool(&pool, actuator_registry.clone(), access.clone());
        let held = actuator_registry
            .try_acquire_resources(
                "dev-1",
                "watch-run-A".into(),
                &[exclusive("actuator")],
                DEFAULT_LEASE_TTL,
            )
            .expect("run-A 占 actuator");
        let busy = tool.execute(args.clone()).await.expect("execute");
        assert!(!busy.success, "actuator 被占必须 busy");
        let msg = busy.error.unwrap_or_default();
        assert!(msg.contains("watch-run-A"), "busy 带持有者: {msg}");
        assert!(msg.contains("actuator"), "busy 带资源 id: {msg}");
        drop(held);

        // 并集的另一半：camera 被占同样 busy。
        let camera_registry = Arc::new(ArbitrationRegistry::default());
        let tool = set_tool(&pool, camera_registry.clone(), access.clone());
        let _held = camera_registry
            .try_acquire_resources("dev-1", "watch-run-A".into(), &[exclusive("camera")], DEFAULT_LEASE_TTL)
            .expect("run-A 占 camera");
        let busy = tool.execute(args.clone()).await.expect("execute");
        assert!(!busy.success, "camera 被占必须 busy");
        assert!(busy.error.unwrap_or_default().contains("camera"));

        // 全部空闲 → 成功写两个属性。
        let registry = Arc::new(ArbitrationRegistry::default());
        let tool = set_tool(&pool, registry, access);
        let ok = tool.execute(args).await.expect("execute");
        assert!(ok.success, "资源空闲必须放行: {:?}", ok.error);
        let payload: Value = serde_json::from_str(&ok.output).unwrap();
        assert_eq!(payload["updated"], json!(["level", "mode"]));
    }

    /// 未知资源 id fail-closed：写入被拒绝且不发布任何属性事件。
    #[tokio::test]
    async fn set_device_unknown_resource_is_rejected() {
        let pool = pool_with_device().await;
        let access = resource_access(
            &pool,
            json!([{"name":"set_level","kind":"set","targetField":"level",
                    "parameters":{"type":"object","properties":{"level":{"type":"integer"}}},
                    "resources":["not-declared"]}]),
        )
        .await;
        let tool = set_tool(&pool, Arc::new(ArbitrationRegistry::default()), access);

        let failed = tool
            .execute(json!({"thingId": "dev-1", "properties": {"level": 10}}))
            .await
            .expect("execute");
        assert!(!failed.success, "未知资源必须拒绝");
        assert!(
            failed
                .error
                .unwrap_or_default()
                .contains("unknown resource 'not-declared'"),
            "fail-closed 文案"
        );
    }

    /// 未声明 resources 的 set 工具回落合成 `__device__`（E1 语义）。
    #[tokio::test]
    async fn set_device_empty_resources_claims_device_resource() {
        let pool = pool_with_device().await;
        let access = resource_access(
            &pool,
            json!([{"name":"set_level","kind":"set","targetField":"level",
                    "parameters":{"type":"object","properties":{"level":{"type":"integer"}}},
                    "resources":[]}]),
        )
        .await;
        let registry = Arc::new(ArbitrationRegistry::default());
        let _held = registry
            .try_acquire_default("dev-1", "someone-else".to_string())
            .expect("held");
        let tool = set_tool(&pool, registry, access);

        let busy = tool
            .execute(json!({"thingId": "dev-1", "properties": {"level": 10}}))
            .await
            .expect("execute");
        assert!(!busy.success, "空声明必须回落到设备互斥");
        let msg = busy.error.unwrap_or_default();
        assert!(msg.contains("someone-else"), "busy 带持有者: {msg}");
        assert!(msg.contains("__device__"), "busy 带合成资源: {msg}");
    }
}
