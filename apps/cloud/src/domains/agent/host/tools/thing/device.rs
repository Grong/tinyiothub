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

/// Per-workspace `runtime_media_enabled` read at call time. Every failure
/// (missing config, malformed JSON, storage error) is fail-closed `false`.
#[async_trait]
pub trait RuntimeMediaGate: Send + Sync {
    async fn media_enabled(&self, workspace_id: &str) -> bool;
}

pub struct DbRuntimeMediaGate {
    pub db: Arc<tinyiothub_storage::Db>,
}

#[async_trait]
impl RuntimeMediaGate for DbRuntimeMediaGate {
    async fn media_enabled(&self, workspace_id: &str) -> bool {
        match self.db.load_heartbeat_config_json(workspace_id).await {
            Ok(json) => json
                .as_deref()
                .and_then(|json| serde_json::from_str::<Value>(json).ok())
                .map(|json| crate::domains::agent::runtime::types::RuntimeFlags::from_heartbeat_json(&json).media)
                .unwrap_or(false),
            Err(error) => {
                tracing::warn!(%workspace_id, %error, "runtime media flag lookup failed; media disabled");
                false
            }
        }
    }
}

#[async_trait]
pub(super) trait DeviceSourceReader: Send + Sync {
    async fn read_sources(
        &self,
        thing_id: &str,
        sources: Vec<String>,
    ) -> anyhow::Result<Vec<tinyiothub_core::media::SourceItem>>;
}

#[async_trait]
impl DeviceSourceReader for tinyiothub_runtime::DataServer {
    async fn read_sources(
        &self,
        thing_id: &str,
        sources: Vec<String>,
    ) -> anyhow::Result<Vec<tinyiothub_core::media::SourceItem>> {
        self.read_sources(thing_id, sources)
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))
    }
}

pub(super) struct ReadDeviceTool {
    pub(super) thing_service: Arc<ThingService>,
    pub(super) workspace_id: String,
    pub(super) live: Option<Arc<dyn LiveSummarySource>>,
    /// 无实时态时的属性值回退源（cache 是最新真相；None = 只回 live）。
    pub(super) device_cache: Option<Arc<tinyiothub_storage::cache::ThingCache>>,
    /// E4b：读取成功后写入 DemandTracker（None = 未接线）。
    pub(super) demand: Option<Arc<dyn DemandRecorder>>,
    pub(super) source_reader: Option<Arc<dyn DeviceSourceReader>>,
    pub(super) evidence: Option<Arc<tinyiothub_runtime::media::evidence::EvidenceService>>,
    /// `runtime_media_enabled` gate. Missing = fail-closed (image capture off).
    pub(super) media: Option<Arc<dyn RuntimeMediaGate>>,
    pub(super) run_ctx: Option<super::super::RunContextSlot>,
    pub(super) runtime_recorder: Option<crate::domains::agent::runtime::stream::RuntimeRecorder>,
}

#[derive(Default)]
struct ModelSourceOutput {
    sources: Vec<Value>,
    errors: Vec<Value>,
    attachments: Vec<tinyiothub_core::media::MediaAttachment>,
}

impl ReadDeviceTool {
    async fn read_model_sources(&self, thing_id: &str, requested: &[String]) -> Result<ModelSourceOutput, String> {
        use tinyiothub_core::media::{DataKind, ImageInput, MediaError};

        let Some(reader) = &self.source_reader else {
            return Err("source reads unavailable".into());
        };
        let items = reader
            .read_sources(thing_id, requested.to_vec())
            .await
            .map_err(|error| format!("source read failed: {error}"))?;
        let selected = tinyiothub_runtime::media::source::select_model_sources(&items, requested)
            .map_err(|error| error.to_string())?;
        if selected.iter().filter(|item| item.data_kind == DataKind::Image).count() > 1 {
            return Err(MediaError::TooManyImages.to_string());
        }
        // Fail-closed: an unwired or unreadable flag never enables image capture.
        let media_enabled = match &self.media {
            Some(gate) => gate.media_enabled(&self.workspace_id).await,
            None => false,
        };

        let (task_id, run_id) = match &self.run_ctx {
            Some(slot) => {
                let context = slot.read().await.clone();
                match context {
                    Some(context) => {
                        let context = context.read().await;
                        (context.task_id.clone(), Some(context.run_id.clone()))
                    }
                    None => (None, None),
                }
            }
            None => (None, None),
        };
        let mut output = ModelSourceOutput::default();
        for item in selected {
            match item.data_kind {
                DataKind::Detections => output.sources.push(json!({
                    "source": item.source,
                    "dataKind": item.data_kind,
                    "seq": item.seq,
                    "time": item.time,
                    "text": item.text,
                    "data": item.data,
                    "error": item.error,
                })),
                DataKind::Image => {
                    if !media_enabled {
                        output.errors.push(json!({
                            "source": item.source,
                            "error": "image_unavailable",
                            "reason": "media_disabled",
                        }));
                        continue;
                    }
                    let result = async {
                        let evidence = self
                            .evidence
                            .as_ref()
                            .ok_or(MediaError::Storage("media evidence unavailable".into()))?;
                        let media = item.media.as_ref().ok_or(MediaError::InvalidImage)?;
                        if media.bytes.len() > tinyiothub_runtime::media::evidence::MAX_IMAGE_BYTES {
                            return Err(MediaError::ImageTooLarge);
                        }
                        let reference = evidence
                            .put_image(ImageInput {
                                workspace_id: self.workspace_id.clone(),
                                task_id: task_id.clone(),
                                run_id: run_id.clone(),
                                tool_call_id: None,
                                job_id: None,
                                device_id: thing_id.to_string(),
                                source_id: item.source.clone(),
                                source_seq: item.seq,
                                media_time: item.time,
                                mime: media.mime.clone(),
                                bytes: media.bytes.clone(),
                                max_age: std::time::Duration::from_secs(60),
                            })
                            .await?;
                        let attachment = evidence
                            .get_image(&reference.evidence_id)
                            .await?
                            .ok_or_else(|| MediaError::Storage("stored evidence disappeared".into()))?;
                        Ok::<_, MediaError>((reference, attachment))
                    }
                    .await;
                    match result {
                        Ok((reference, attachment)) => {
                            output.sources.push(json!({
                                "source": item.source,
                                "dataKind": item.data_kind,
                                "seq": item.seq,
                                "time": item.time,
                                "evidence": reference,
                            }));
                            output.attachments.push(attachment);
                        }
                        Err(error) => output.errors.push(json!({
                            "source": item.source,
                            "error": error.to_string(),
                        })),
                    }
                }
                DataKind::Values | DataKind::Video | DataKind::Custom(_) => {}
            }
        }
        Ok(output)
    }

    async fn record_read(&self, thing_id: &str, state: Value, source_ids: Vec<String>) {
        let (Some(recorder), Some(slot)) = (&self.runtime_recorder, &self.run_ctx) else {
            return;
        };
        let context = slot.read().await.clone();
        let Some(context) = context else { return };
        let context = context.read().await;
        let Some(task_id) = context.task_id.clone() else { return };
        let run_id = context.run_id.clone();
        drop(context);
        let input = tinyiothub_storage::runtime_event::RuntimeEventInput {
            event_id: format!("read_device:{run_id}:{thing_id}:{}", tinyiothub_core::generate_id()),
            event_type: "read_device".into(),
            occurred_at: chrono::Utc::now(),
            workspace_id: self.workspace_id.clone(),
            task_id: Some(task_id),
            run_id: Some(run_id),
            tool_call_id: None,
            job_id: None,
            approval_id: None,
            device_id: Some(thing_id.to_string()),
            source_id: source_ids.first().cloned(),
            source_seq: None,
            evidence_id: None,
            data: json!({"state": state, "sourceIds": source_ids}),
        };
        if let Err(error) = recorder.record(input).await {
            tracing::warn!(workspace_id = %self.workspace_id, %thing_id, %error, "persist runtime device read failed");
        }
    }
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
         properties 可选——只回指定属性；缺省回全部已上报值。\
         sources 可选——按 Source ID 读取 detections 或一张图片。"
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
                },
                "sources": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "按 Source ID 读取 detections 或一张 JPEG/PNG 图片；video 不供模型读取"
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
            sources: Option<Vec<String>>,
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

        let mut payload = json!({
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
        });
        let mut attachments = Vec::new();
        if let Some(requested) = input.sources.as_deref().filter(|sources| !sources.is_empty()) {
            match self.read_model_sources(&input.thing_id, requested).await {
                Ok(source_output) => {
                    payload["sources"] = Value::Array(source_output.sources);
                    if !source_output.errors.is_empty() {
                        payload["sourceErrors"] = Value::Array(source_output.errors);
                    }
                    attachments = source_output.attachments;
                }
                Err(error) => return tool_err(error),
            }
        }
        self.record_read(
            &input.thing_id,
            payload["state"].clone(),
            input.sources.clone().unwrap_or_default(),
        )
        .await;
        let mut result = tool_ok(payload)?;
        result.attachments = attachments;
        Ok(result)
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

        // 先做工作区归属解析：stop 闸门按 thing_id 全局键，若先读闸门，
        // 跨工作区同 id 的调用会看到 `stopped:` 而非 404（评审 Issue 2）。
        let thing = match self.thing_service.get_thing(&input.thing_id, &self.workspace_id).await {
            Ok(t) => t,
            Err(e) => return tool_err(format!("物不存在: {e}")),
        };
        if thing.thing_type != "device" {
            return tool_err(format!("操作不支持: 物类型为 '{}'", thing.thing_type));
        }
        // 归属确认后再暴露闸门原因；claim 路径仍会兜底拒绝（fail-closed）。
        if let Some(registry) = &self.arbitration
            && let Some(reason) = registry.stop_reason(&input.thing_id)
        {
            return tool_err(format!("stopped: {reason}"));
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
    /// 工作区归属校验（评审 Issue 2）：stop 与 call/set 一样必须先按
    /// `workspace_id` 解析到物，禁止跨租户置闸门/取消在途调用。
    pub(super) thing_service: Arc<ThingService>,
    pub(super) workspace_id: String,
    pub(super) arbitration: Option<Arc<ArbitrationRegistry>>,
    /// E4c：取消在途调用的入口（None = 只置闸门，无法协作取消）。
    pub(super) data_server: Option<Arc<tinyiothub_runtime::DataServer>>,
}

/// stop 闸门时长边界（秒）：下限 1s 防止零/负窗口，上限 1h 防止
/// `Instant` 溢出与闸门长期残留（评审 Issue 7）。
pub(super) const MIN_STOP_TTL_SECS: u64 = 1;
pub(super) const MAX_STOP_TTL_SECS: u64 = 3600;

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
        "立即停止对设备的后续操作：置 stop 闸门（默认 60s）、释放设备占用并请求\
         取消在途调用。工具立即返回 cancellation_requested（不代表已中止）；\
         驱动的异步终态（cancelled / cancelled_unknown）随后落事件与账本。\
         闸门期间 call_device/set_device 返回 stopped。"
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
        // 工作区归属优先：跨租户 id 返回正常的“物不存在”，不得置闸门或
        // 取消别人的在途调用（评审 Issue 2）。
        if let Err(e) = self.thing_service.get_thing(&input.thing_id, &self.workspace_id).await {
            return tool_err(format!("物不存在: {e}"));
        }
        let Some(registry) = &self.arbitration else {
            return tool_err("arbitration unavailable: stop NOT applied".to_string());
        };
        // ttlSecs 来自模型/用户输入：clamp 到 1..=3600s，避免超大值溢出
        // （评审 Issue 7）。
        let ttl_secs = input
            .ttl_secs
            .unwrap_or(DEFAULT_STOP_TTL.as_secs())
            .clamp(MIN_STOP_TTL_SECS, MAX_STOP_TTL_SECS);
        let ttl = std::time::Duration::from_secs(ttl_secs);
        let reason = input.reason.unwrap_or_else(|| "operator stop".to_string());
        // 1) 闸门 + 释放全部 claim；2) 协作式取消当前在途调用；3) 立即返回，
        // 不等待驱动退出（异步终态由 worker → DataServer 留痕）。
        registry.set_stop(&input.thing_id, reason.clone(), ttl);
        let in_flight = self
            .data_server
            .as_ref()
            .map(|server| server.cancel_calls(&input.thing_id))
            .unwrap_or(0);
        let until = chrono::Utc::now() + chrono::Duration::seconds(ttl_secs as i64);
        tool_ok(json!({
            "v": 1,
            "thingId": input.thing_id,
            "status": "cancellation_requested",
            "reason": reason,
            "until": until.to_rfc3339(),
            "inFlight": in_flight,
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

    // ── E4c：stop 立即返回 + 闸门文案一致 ───────────────

    /// stop 立即返回 cancellation_requested（不等待驱动）、置闸门；随后
    /// invoke_action 以 `stopped: <reason>` 拒绝（不是 claim 的 busy 文案）。
    #[tokio::test]
    async fn stop_returns_cancellation_requested_and_gate_rejects_next_call() {
        let registry = Arc::new(ArbitrationRegistry::default());
        let pool = pool_with_device().await;
        let tool = StopTool {
            thing_service: Arc::new(ThingService::new(pool.clone())),
            workspace_id: "ws-set".into(),
            arbitration: Some(registry.clone()),
            data_server: None,
        };

        let out = tool
            .execute(json!({"thingId": "dev-1", "reason": "ops emergency", "ttlSecs": 60}))
            .await
            .expect("execute");
        assert!(out.success, "{:?}", out.error);
        let payload: Value = serde_json::from_str(&out.output).unwrap();
        assert_eq!(payload["status"], "cancellation_requested");
        assert_eq!(payload["thingId"], "dev-1");
        assert_eq!(payload["reason"], "ops emergency");
        assert_eq!(payload["inFlight"], 0, "无 DataServer 时无在途调用可取消");
        assert!(payload["until"].as_str().is_some_and(|s| !s.is_empty()));
        assert_eq!(registry.stop_reason("dev-1").as_deref(), Some("ops emergency"));

        // 后续调用：闸门优先于确认/claim，统一 `stopped:` 文案。
        let invoke = super::super::invoke_action::InvokeActionTool {
            thing_service: Arc::new(ThingService::new(pool.clone())),
            pool,
            workspace_id: "ws-set".into(),
            data_server: None,
            pending_actions: Arc::new(super::super::pending_action::PendingActionStore::new()),
            arbitration: Some(registry),
            arbitration_holder: "chat:ws-set".into(),
            approval: None,
            access: None,
            resources: None,
            jobs: None,
            run_ctx: None,
        };
        let blocked = invoke
            .execute(json!({"thingId": "dev-1", "actionName": "reboot"}))
            .await
            .expect("execute");
        assert!(!blocked.success, "闸门期间必须拒绝");
        assert_eq!(
            blocked.error.as_deref(),
            Some("stopped: ops emergency"),
            "必须是 stopped: <reason> 而非 busy: stop:..."
        );
    }

    /// stop 闸门对 set_device 同样生效：正常 set 先成功并发布属性事件；置闸后
    /// 统一 `stopped: <reason>` 拒绝，且不得发布任何属性变更事件。
    #[tokio::test]
    async fn set_device_stop_gate_rejects_and_publishes_no_event() {
        let pool = pool_with_device().await;
        let access = resource_access(&pool, set_tools()).await;
        let registry = Arc::new(ArbitrationRegistry::default());
        let bus = Arc::new(EventBus::new());
        let mut rx = bus.subscribe();
        let tool = SetDeviceTool {
            thing_service: Arc::new(ThingService::new(pool.clone())),
            pool: pool.clone(),
            workspace_id: "ws-set".into(),
            event_bus: Some(bus),
            arbitration: Some(registry.clone()),
            holder: "watch-run-B".into(),
            resources: Some(access as Arc<dyn ResourceResolver>),
        };

        // 闸门未置：正常写入成功并发布一条属性变更事件。
        let ok = tool
            .execute(json!({"thingId": "dev-1", "properties": {"level": 10}}))
            .await
            .expect("execute");
        assert!(ok.success, "正常 set 必须成功: {:?}", ok.error);
        let event = rx.try_recv().expect("正常 set 应发布属性变更事件");
        assert_eq!(event.source().thing_id(), Some("dev-1"));

        // 置闸门后：闸门先于物查询/claim，文案为 `stopped: <reason>`。
        registry.set_stop("dev-1", "ops emergency".into(), DEFAULT_LEASE_TTL);
        let blocked = tool
            .execute(json!({"thingId": "dev-1", "properties": {"level": 11}}))
            .await
            .expect("execute");
        assert!(!blocked.success, "闸门期间必须拒绝");
        assert_eq!(blocked.error.as_deref(), Some("stopped: ops emergency"));
        assert!(rx.try_recv().is_err(), "闸门拒绝不得发布属性变更事件");
    }

    // ── E4c 评审修复：stop 归属 / ttl clamp / 跨租户闸门 ──────────

    /// Issue 2：stop 必须先按工作区解析物——跨工作区 id 返回正常的
    /// “物不存在”，且不得置闸门或取消别的租户的在途调用。
    #[tokio::test]
    async fn stop_rejects_cross_workspace_thing_without_setting_gate() {
        let pool = pool_with_device().await;
        seed_test_workspace(&pool, "tenant-1", "ws-other").await;
        sqlx::query(
            "INSERT INTO things (id, name, workspace_id, thing_type) VALUES ('dev-2','Other','ws-other','device')",
        )
        .execute(&pool)
        .await
        .expect("insert other device");
        let registry = Arc::new(ArbitrationRegistry::default());
        let tool = StopTool {
            thing_service: Arc::new(ThingService::new(pool.clone())),
            workspace_id: "ws-set".into(),
            arbitration: Some(registry.clone()),
            data_server: None,
        };

        let denied = tool
            .execute(json!({"thingId": "dev-2", "reason": "cross tenant"}))
            .await
            .expect("execute");
        assert!(!denied.success, "跨工作区 stop 必须拒绝");
        assert!(
            denied.error.unwrap_or_default().contains("物不存在"),
            "必须是正常 not-found，而不是执行了控制动作"
        );
        assert_eq!(registry.stop_reason("dev-2"), None, "跨租户不得置闸门");

        // 本工作区的物仍然可 stop。
        let ok = tool
            .execute(json!({"thingId": "dev-1", "reason": "ops"}))
            .await
            .expect("execute");
        assert!(ok.success, "{:?}", ok.error);
        assert_eq!(registry.stop_reason("dev-1").as_deref(), Some("ops"));
    }

    /// Issue 7：ttlSecs clamp 到 1..=3600s，超大/零值都不得溢出或留空窗。
    #[tokio::test]
    async fn stop_clamps_ttl_seconds() {
        let pool = pool_with_device().await;
        let registry = Arc::new(ArbitrationRegistry::default());
        let tool = StopTool {
            thing_service: Arc::new(ThingService::new(pool.clone())),
            workspace_id: "ws-set".into(),
            arbitration: Some(registry),
            data_server: None,
        };

        let until_secs = |payload: &Value| {
            let until = chrono::DateTime::parse_from_rfc3339(payload["until"].as_str().unwrap()).unwrap();
            (until.with_timezone(&chrono::Utc) - chrono::Utc::now()).num_seconds()
        };

        // 超大：clamp 到 3600，而不是 panic/溢出。
        let out = tool
            .execute(json!({"thingId": "dev-1", "ttlSecs": 999_999_999u64}))
            .await
            .expect("execute");
        assert!(out.success, "{:?}", out.error);
        let payload: Value = serde_json::from_str(&out.output).unwrap();
        let secs = until_secs(&payload);
        assert!((3599..=3600).contains(&secs), "上限必须 clamp 到 3600s: {secs}");

        // 0：clamp 到 1s，闸门仍需存在。
        let out = tool
            .execute(json!({"thingId": "dev-1", "ttlSecs": 0}))
            .await
            .expect("execute");
        assert!(out.success);
        let payload: Value = serde_json::from_str(&out.output).unwrap();
        let secs = until_secs(&payload);
        assert!((0..=1).contains(&secs), "下限必须 clamp 到 1s: {secs}");
    }

    /// Issue 2：set_device 的 stop 闸门在 workspace 解析之后才暴露——
    /// 跨租户 id 得到 not-found，而不是 `stopped: <reason>`。
    #[tokio::test]
    async fn set_device_cross_tenant_does_not_leak_stop_reason() {
        let pool = pool_with_device().await;
        seed_test_workspace(&pool, "tenant-1", "ws-other").await;
        let access = resource_access(&pool, set_tools()).await;
        let registry = Arc::new(ArbitrationRegistry::default());
        let bus = Arc::new(EventBus::new());
        // ws-other 的物被其工作区置闸门。
        sqlx::query("INSERT INTO things (id, name, workspace_id, thing_type) VALUES ('dev-x','X','ws-other','device')")
            .execute(&pool)
            .await
            .expect("insert other device");
        registry.set_stop("dev-x", "other-tenant stop".into(), DEFAULT_STOP_TTL);

        let tool = SetDeviceTool {
            thing_service: Arc::new(ThingService::new(pool.clone())),
            pool: pool.clone(),
            workspace_id: "ws-set".into(),
            event_bus: Some(bus),
            arbitration: Some(registry),
            holder: "chat:ws-set".into(),
            resources: Some(access as Arc<dyn ResourceResolver>),
        };
        let out = tool
            .execute(json!({"thingId": "dev-x", "properties": {"level": 1}}))
            .await
            .expect("execute");
        assert!(!out.success);
        let msg = out.error.unwrap_or_default();
        assert!(msg.contains("物不存在"), "跨租户必须是 not-found: {msg}");
        assert!(
            !msg.contains("other-tenant stop"),
            "不得向跨租户调用者泄漏闸门原因: {msg}"
        );
    }

    struct StubMedia(bool);

    #[async_trait]
    impl super::RuntimeMediaGate for StubMedia {
        async fn media_enabled(&self, _workspace_id: &str) -> bool {
            self.0
        }
    }

    struct StubSources {
        items: Vec<tinyiothub_core::media::SourceItem>,
        calls: std::sync::Mutex<Vec<Vec<String>>>,
    }

    #[async_trait]
    impl super::DeviceSourceReader for StubSources {
        async fn read_sources(
            &self,
            _thing_id: &str,
            sources: Vec<String>,
        ) -> anyhow::Result<Vec<tinyiothub_core::media::SourceItem>> {
            self.calls.lock().expect("source calls").push(sources);
            Ok(self.items.clone())
        }
    }

    fn source(source: &str, kind: &str, seq: u64) -> tinyiothub_core::media::SourceItem {
        serde_json::from_value(json!({
            "v": 1,
            "source": source,
            "dataKind": kind,
            "seq": seq,
            "time": chrono::Utc::now(),
            "data": [{"label": "person", "confidence": 0.9}]
        }))
        .expect("source item")
    }

    fn png() -> Vec<u8> {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScL9WQAAAABJRU5ErkJggg==")
            .expect("PNG fixture")
    }

    fn evidence_service(
        pool: &sqlx::SqlitePool,
        store: Arc<dyn tinyiothub_runtime::media::evidence::MediaStore>,
    ) -> Arc<tinyiothub_runtime::media::evidence::EvidenceService> {
        Arc::new(tinyiothub_runtime::media::evidence::EvidenceService::new(
            store,
            Arc::new(crate::shared::runtime_ports::DbMediaEvidenceStore::new(
                tinyiothub_storage::Db::new(pool.clone()),
            )),
        ))
    }

    #[tokio::test]
    async fn read_device_persists_only_explicit_model_sources_and_attaches_exact_image() {
        let pool = pool_with_device().await;
        let root = tempfile::tempdir().expect("media root");
        let store = Arc::new(
            tinyiothub_runtime::media::evidence::LocalMediaStore::new(root.path())
                .await
                .expect("media store"),
        );
        let mut image = source("front", "image", 4);
        image.media = Some(tinyiothub_core::media::MediaPayload {
            mime: "image/png".into(),
            bytes: png(),
        });
        let reader = Arc::new(StubSources {
            items: vec![
                source("temperature", "values", 1),
                source("lidar", "x_lidar", 2),
                source("objects", "detections", 3),
                image,
            ],
            calls: std::sync::Mutex::new(Vec::new()),
        });
        let tool = ReadDeviceTool {
            thing_service: Arc::new(ThingService::new(pool.clone())),
            workspace_id: "ws-set".into(),
            live: None,
            device_cache: None,
            demand: None,
            source_reader: Some(reader.clone()),
            evidence: Some(evidence_service(&pool, store)),
            media: Some(Arc::new(StubMedia(true))),
            run_ctx: None,
            runtime_recorder: None,
        };
        let requested = vec!["temperature", "lidar", "objects", "front"];
        let result = tool
            .execute(json!({"thingId":"dev-1", "sources":requested}))
            .await
            .expect("execute");
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.attachments.len(), 1);
        assert_eq!(result.attachments[0].bytes, png());
        assert!(!result.attachments[0].evidence_id.is_empty());
        let output: Value = serde_json::from_str(&result.output).expect("tool JSON");
        let sources = output["sources"].as_array().expect("source results");
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0]["source"], "objects");
        assert_eq!(sources[1]["source"], "front");
        assert!(sources[1]["evidence"]["evidenceId"].as_str().is_some());
        assert!(sources.iter().all(|item| item.get("media").is_none()));
        assert_eq!(
            reader.calls.lock().expect("calls").as_slice(),
            &[requested.into_iter().map(String::from).collect::<Vec<String>>()]
        );
        tool.execute(json!({"thingId":"dev-1"})).await.expect("legacy read");
        assert_eq!(
            reader.calls.lock().expect("calls").len(),
            1,
            "omitted sources must not read driver media"
        );
    }

    fn image_reader() -> Arc<StubSources> {
        let mut image = source("front", "image", 4);
        image.media = Some(tinyiothub_core::media::MediaPayload {
            mime: "image/png".into(),
            bytes: png(),
        });
        Arc::new(StubSources {
            items: vec![source("objects", "detections", 3), image],
            calls: std::sync::Mutex::new(Vec::new()),
        })
    }

    fn files_under(root: &std::path::Path) -> usize {
        fn walk(dir: &std::path::Path) -> usize {
            std::fs::read_dir(dir)
                .map(|entries| {
                    entries
                        .flatten()
                        .map(|entry| {
                            let path = entry.path();
                            if path.is_dir() { walk(&path) } else { 1 }
                        })
                        .sum()
                })
                .unwrap_or(0)
        }
        walk(root)
    }

    async fn assert_media_disabled_read(tool: ReadDeviceTool, pool: &sqlx::SqlitePool, root: &std::path::Path) {
        let result = tool
            .execute(json!({"thingId":"dev-1", "sources":["objects","front"]}))
            .await
            .expect("execute");
        assert!(result.success, "{:?}", result.error);
        assert!(
            result.attachments.is_empty(),
            "media-disabled reads must not attach image bytes"
        );
        assert!(!result.output.contains("evidenceId"), "{}", result.output);
        let output: Value = serde_json::from_str(&result.output).expect("tool JSON");
        let sources = output["sources"].as_array().expect("source results");
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0]["source"], "objects");
        let errors = output["sourceErrors"].as_array().expect("source errors");
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0]["source"], "front");
        assert_eq!(errors[0]["error"], "image_unavailable");
        assert_eq!(errors[0]["reason"], "media_disabled");
        let evidence_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM media_evidence")
            .fetch_one(pool)
            .await
            .expect("count evidence");
        assert_eq!(evidence_rows, 0, "media-disabled reads must not persist evidence rows");
        assert_eq!(
            files_under(root),
            0,
            "media-disabled reads must not write evidence bytes"
        );
    }

    #[tokio::test]
    async fn read_device_media_flag_off_skips_image_without_evidence() {
        let pool = pool_with_device().await;
        let root = tempfile::tempdir().expect("media root");
        let store = Arc::new(
            tinyiothub_runtime::media::evidence::LocalMediaStore::new(root.path())
                .await
                .expect("media store"),
        );
        let tool = ReadDeviceTool {
            thing_service: Arc::new(ThingService::new(pool.clone())),
            workspace_id: "ws-set".into(),
            live: None,
            device_cache: None,
            demand: None,
            source_reader: Some(image_reader()),
            evidence: Some(evidence_service(&pool, store)),
            media: Some(Arc::new(StubMedia(false))),
            run_ctx: None,
            runtime_recorder: None,
        };
        assert_media_disabled_read(tool, &pool, root.path()).await;
    }

    #[tokio::test]
    async fn read_device_unwired_media_gate_fails_closed() {
        let pool = pool_with_device().await;
        let root = tempfile::tempdir().expect("media root");
        let store = Arc::new(
            tinyiothub_runtime::media::evidence::LocalMediaStore::new(root.path())
                .await
                .expect("media store"),
        );
        let tool = ReadDeviceTool {
            thing_service: Arc::new(ThingService::new(pool.clone())),
            workspace_id: "ws-set".into(),
            live: None,
            device_cache: None,
            demand: None,
            source_reader: Some(image_reader()),
            evidence: Some(evidence_service(&pool, store)),
            media: None,
            run_ctx: None,
            runtime_recorder: None,
        };
        assert_media_disabled_read(tool, &pool, root.path()).await;
    }

    #[tokio::test]
    async fn db_media_gate_fails_closed_on_missing_or_malformed_config() {
        let pool = pool_with_device().await;
        let gate = DbRuntimeMediaGate {
            db: Arc::new(tinyiothub_storage::Db::new(pool.clone())),
        };
        assert!(
            !gate.media_enabled("missing-workspace").await,
            "absent workspace config must be fail-closed"
        );
        assert!(
            !gate.media_enabled("ws-set").await,
            "default workspace config must be fail-closed"
        );
        sqlx::query("UPDATE workspaces SET heartbeat_config = ? WHERE id = 'ws-set'")
            .bind(r#"{"runtime_media_enabled":true}"#)
            .execute(&pool)
            .await
            .expect("enable media");
        assert!(gate.media_enabled("ws-set").await);
        sqlx::query("UPDATE workspaces SET heartbeat_config = ? WHERE id = 'ws-set'")
            .bind("{not-json")
            .execute(&pool)
            .await
            .expect("malformed config");
        assert!(
            !gate.media_enabled("ws-set").await,
            "malformed config must be fail-closed"
        );
    }

    #[tokio::test]
    async fn read_device_rejects_selected_video_and_multiple_images() {
        let pool = pool_with_device().await;
        let root = tempfile::tempdir().expect("media root");
        let store = Arc::new(
            tinyiothub_runtime::media::evidence::LocalMediaStore::new(root.path())
                .await
                .expect("media store"),
        );
        let mut first = source("first", "image", 1);
        first.media = Some(tinyiothub_core::media::MediaPayload {
            mime: "image/png".into(),
            bytes: png(),
        });
        let mut second = source("second", "image", 2);
        second.media = first.media.clone();
        let reader = Arc::new(StubSources {
            items: vec![source("live", "video", 1), first, second],
            calls: std::sync::Mutex::new(Vec::new()),
        });
        let tool = ReadDeviceTool {
            thing_service: Arc::new(ThingService::new(pool.clone())),
            workspace_id: "ws-set".into(),
            live: None,
            device_cache: None,
            demand: None,
            source_reader: Some(reader),
            evidence: Some(evidence_service(&pool, store)),
            run_ctx: None,
            runtime_recorder: None,
            media: None,
        };
        let video = tool
            .execute(json!({"thingId":"dev-1", "sources":["live"]}))
            .await
            .expect("execute");
        assert!(!video.success);
        assert_eq!(video.error.as_deref(), Some("video_is_for_human_not_model"));
        let multiple = tool
            .execute(json!({"thingId":"dev-1", "sources":["first", "second"]}))
            .await
            .expect("execute");
        assert!(!multiple.success);
        assert!(multiple.error.unwrap_or_default().contains("at most one image"));
    }

    #[tokio::test]
    async fn read_device_persists_task_first_reference_and_state() {
        let pool = pool_with_device().await;
        let db = Arc::new(tinyiothub_storage::Db::new(pool.clone()));
        let recorder = crate::domains::agent::runtime::stream::RuntimeRecorder::new(
            db.clone(),
            Arc::new(crate::domains::agent::runtime::stream::RuntimeStreamHub::default()),
        );
        let run_ctx = crate::domains::agent::host::tools::new_run_context_slot(Arc::new(tokio::sync::RwLock::new(
            tinyiothub_agent::runtime::thing_agent::RunContextInner {
                run_id: "run-read".into(),
                workspace_id: "ws-set".into(),
                task_id: Some("task-read".into()),
                ..Default::default()
            },
        )));
        let tool = ReadDeviceTool {
            thing_service: Arc::new(ThingService::new(pool)),
            workspace_id: "ws-set".into(),
            live: None,
            device_cache: None,
            demand: None,
            source_reader: None,
            evidence: None,
            run_ctx: Some(run_ctx),
            runtime_recorder: Some(recorder),
            media: None,
        };

        let result = tool.execute(json!({"thingId":"dev-1"})).await.expect("execute");
        assert!(result.success, "{:?}", result.error);
        let events = db
            .runtime_events_after("ws-set", None, Some("task-read"), 10)
            .await
            .expect("runtime events");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "read_device");
        assert_eq!(events[0].device_id.as_deref(), Some("dev-1"));
        assert_eq!(events[0].run_id.as_deref(), Some("run-read"));
        assert!(events[0].data["state"].is_object());
        assert!(events[0].data.get("text").is_none());
    }

    struct FailingStore;
    #[async_trait]
    impl tinyiothub_runtime::media::evidence::MediaStore for FailingStore {
        async fn put(
            &self,
            _workspace_id: &str,
            _sha256: &str,
            _bytes: &[u8],
        ) -> Result<String, tinyiothub_core::media::MediaError> {
            Err(tinyiothub_core::media::MediaError::Storage("injected".into()))
        }
        async fn read(&self, _storage_path: &str) -> Result<Vec<u8>, tinyiothub_core::media::MediaError> {
            Err(tinyiothub_core::media::MediaError::Storage("injected".into()))
        }
        async fn list(&self) -> Result<Vec<String>, tinyiothub_core::media::MediaError> {
            Ok(Vec::new())
        }
        async fn delete(
            &self,
            _storage_path: &str,
            _grace: std::time::Duration,
        ) -> Result<(), tinyiothub_core::media::MediaError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn read_device_isolates_media_store_failure_and_preserves_detections() {
        let pool = pool_with_device().await;
        let mut image = source("front", "image", 4);
        image.media = Some(tinyiothub_core::media::MediaPayload {
            mime: "image/png".into(),
            bytes: png(),
        });
        let reader = Arc::new(StubSources {
            items: vec![source("objects", "detections", 3), image],
            calls: std::sync::Mutex::new(Vec::new()),
        });
        let tool = ReadDeviceTool {
            thing_service: Arc::new(ThingService::new(pool.clone())),
            workspace_id: "ws-set".into(),
            live: None,
            device_cache: None,
            demand: None,
            source_reader: Some(reader),
            evidence: Some(evidence_service(&pool, Arc::new(FailingStore))),
            media: Some(Arc::new(StubMedia(true))),
            run_ctx: None,
            runtime_recorder: None,
        };
        let result = tool
            .execute(json!({"thingId":"dev-1", "sources":["objects", "front"]}))
            .await
            .expect("execute");
        assert!(result.success, "{:?}", result.error);
        assert!(result.attachments.is_empty());
        let output: Value = serde_json::from_str(&result.output).expect("tool JSON");
        assert_eq!(output["sources"][0]["source"], "objects");
        assert_eq!(output["sourceErrors"][0]["source"], "front");
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
