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
use tinyiothub_runtime::event_bus::EventBus;

use super::profile::health_str;
use super::{LiveSummarySource, tool_err, tool_ok};
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
        // 读取成功（物存在）→ 记录需求：显式 properties 只记存在项，
        // 否则记返回的每个属性。失败/未找到在更早处已返回，不会写入。
        if let Some(demand) = &self.demand {
            match &input.properties {
                Some(wanted) => {
                    for property in wanted {
                        if state.contains_key(property) {
                            demand.record_read(&self.workspace_id, &input.thing_id, property);
                        }
                    }
                }
                None => {
                    for property in state.keys() {
                        demand.record_read(&self.workspace_id, &input.thing_id, property);
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
            Some(registry) => match registry.try_acquire(&input.thing_id, self.holder.clone(), DEFAULT_LEASE_TTL) {
                Ok(lease) => Some(lease),
                Err(busy) => return tool_err(format!("busy: 设备正被 {} 占用，请稍后重试", busy.holder)),
            },
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
