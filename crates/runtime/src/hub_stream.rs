//! E4b Hub 订阅流状态机（spec 2026-10-21-e4b §2.4）。
//!
//! 本模块只做与传输层无关的工作：
//! - `EventBus` 领域事件 → `reading` / `control` 帧映射；
//! - 每 `(thing, source)` 的 latest-wins 合并且有界控制队列（256）；
//! - `snapshot` 装配（按 manifest source 的 `fields` 过滤属性）；
//! - 连接需求生命周期（`DemandGuard`：Drop 即 unsubscribe）。
//!
//! SSE 读写循环住在 cloud handler（`hub_stream.rs`），方便复用 E3 的
//! Claims/租户校验与 `axum::response::sse`。

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use serde_json::{Map, Value};
use tinyiothub_core::models::event::{Event, EventType, ThingEventType};
use tinyiothub_core::models::thing::Thing;

use crate::access::demand::{DemandTracker, SUBSCRIPTION_TTL, SubscriptionId};

/// 单连接订阅的设备上限。
pub const MAX_THINGS: usize = 50;
/// 单连接订阅的 source 上限。
pub const MAX_SOURCES: usize = 16;
/// 控制事件有界队列上限；溢出 → resync 并清空。
pub const CONTROL_QUEUE_LIMIT: usize = 256;
/// `reading` 合并冲刷周期（latest-wins，最多每 50ms 一帧）。
pub const READING_FLUSH_INTERVAL: Duration = Duration::from_millis(50);
/// `ping` 周期。
pub const PING_INTERVAL: Duration = Duration::from_secs(15);

/// 请求中声明的 source 及其字段（空 `fields` = 该 source 返回全部属性）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSpec {
    pub id: String,
    pub fields: Vec<String>,
}

impl SourceSpec {
    pub fn new(id: impl Into<String>, fields: Vec<String>) -> Self {
        Self { id: id.into(), fields }
    }

    /// 该 source 是否声明了 `property`。
    pub fn covers(&self, property: &str) -> bool {
        self.fields.is_empty() || self.fields.iter().any(|field| field == property)
    }
}

/// 一个已通过 workspace / manifest 校验的订阅目标。
#[derive(Debug, Clone)]
pub struct ThingSpec {
    pub thing_id: String,
    pub sources: Vec<SourceSpec>,
    /// 属性 id → 属性名。`EventSource::device_property` 的 source_id 是
    /// `{thing_id}:{property_id}`，据此还原出 manifest 字段名（事件元数据
    /// 里的 `property_name` 可能是 display_name）。
    pub property_names: HashMap<String, String>,
}

impl ThingSpec {
    pub fn new(thing_id: impl Into<String>, sources: Vec<SourceSpec>) -> Self {
        Self {
            thing_id: thing_id.into(),
            sources,
            property_names: HashMap::new(),
        }
    }

    pub fn with_property_names(mut self, property_names: HashMap<String, String>) -> Self {
        self.property_names = property_names;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceSnapshot {
    pub source: String,
    pub values: Map<String, Value>,
    pub last_change_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSnapshot {
    pub thing_id: String,
    pub name: String,
    pub sources: Vec<SourceSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SnapshotFrame {
    pub v: u8,
    pub devices: Vec<DeviceSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadingFrame {
    pub v: u8,
    pub ts: String,
    pub thing_id: String,
    pub source: String,
    pub values: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlFrame {
    pub v: u8,
    pub ts: String,
    pub thing_id: String,
    pub kind: String,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResyncFrame {
    pub v: u8,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PingFrame {
    pub v: u8,
}

/// 一帧待发送的 SSE 数据。
#[derive(Debug, Clone, PartialEq)]
pub enum OutFrame {
    Snapshot(SnapshotFrame),
    Reading(ReadingFrame),
    Control(ControlFrame),
    Resync(ResyncFrame),
    Ping(PingFrame),
}

impl OutFrame {
    /// SSE event 名。
    pub fn name(&self) -> &'static str {
        match self {
            OutFrame::Snapshot(_) => "snapshot",
            OutFrame::Reading(_) => "reading",
            OutFrame::Control(_) => "control",
            OutFrame::Resync(_) => "resync",
            OutFrame::Ping(_) => "ping",
        }
    }

    /// SSE data 负载（JSON）。
    pub fn data(&self) -> String {
        let value = match self {
            OutFrame::Snapshot(frame) => serde_json::to_value(frame),
            OutFrame::Reading(frame) => serde_json::to_value(frame),
            OutFrame::Control(frame) => serde_json::to_value(frame),
            OutFrame::Resync(frame) => serde_json::to_value(frame),
            OutFrame::Ping(frame) => serde_json::to_value(frame),
        };
        value.unwrap_or(Value::Null).to_string()
    }

    pub fn resync(reason: &str) -> Self {
        OutFrame::Resync(ResyncFrame {
            v: 1,
            reason: reason.to_string(),
        })
    }

    pub fn ping() -> Self {
        OutFrame::Ping(PingFrame { v: 1 })
    }

    pub fn snapshot(devices: Vec<DeviceSnapshot>) -> Self {
        OutFrame::Snapshot(SnapshotFrame { v: 1, devices })
    }
}

/// 从 `DataServer::get_device` 的 `Thing` 装配 snapshot。
///
/// `values` 只包含 source 声明的字段（空 `fields` = 全部属性且已有当前值）。
pub fn build_snapshot(things: &[Thing], specs: &[ThingSpec]) -> OutFrame {
    let specs_by_id: HashMap<&str, &ThingSpec> = specs.iter().map(|spec| (spec.thing_id.as_str(), spec)).collect();
    let devices = things
        .iter()
        .filter_map(|thing| {
            let spec = specs_by_id.get(thing.id.as_str())?;
            let properties = thing.properties.as_deref().unwrap_or(&[]);
            let sources = spec
                .sources
                .iter()
                .map(|source| {
                    let mut values = Map::new();
                    for property in properties {
                        if !source.covers(&property.name) {
                            continue;
                        }
                        if let Some(value) = &property.current_value {
                            values.insert(property.name.clone(), Value::String(value.clone()));
                        }
                    }
                    SourceSnapshot {
                        source: source.id.clone(),
                        values,
                        last_change_at: thing.last_heartbeat.clone(),
                    }
                })
                .collect();
            Some(DeviceSnapshot {
                thing_id: thing.id.clone(),
                name: thing.name.clone(),
                sources,
            })
        })
        .collect();
    OutFrame::snapshot(devices)
}

/// 每连接的事件映射 / 合并 / 有界队列状态机。
pub struct HubStreamState {
    /// 请求中的 thing → 已声明的 source。
    things: HashMap<String, Vec<SourceSpec>>,
    /// 属性 id → 属性名（按 thing）。
    property_names: HashMap<String, HashMap<String, String>>,
    /// latest-wins：同一 `(thing, source)` 在冲刷窗口内只保留最后一帧。
    coalesce: BTreeMap<(String, String), ReadingFrame>,
    /// 控制帧按到达顺序排队。
    control: VecDeque<ControlFrame>,
}

impl HubStreamState {
    pub fn new(specs: impl IntoIterator<Item = ThingSpec>) -> Self {
        let mut things = HashMap::new();
        let mut property_names = HashMap::new();
        for spec in specs {
            property_names.insert(spec.thing_id.clone(), spec.property_names);
            things.insert(spec.thing_id, spec.sources);
        }
        Self {
            things,
            property_names,
            coalesce: BTreeMap::new(),
            control: VecDeque::new(),
        }
    }

    pub fn has_thing(&self, thing_id: &str) -> bool {
        self.things.contains_key(thing_id)
    }

    /// 应用一个 EventBus 事件。
    ///
    /// - `PropertyChange` → 合并进 `reading`（返回 `None`，由 50ms 冲刷）；
    /// - 命令事件（started/completed/failed）→ `control` kind `job`；
    /// - 连接迁移 → `control` kind `state_change`；
    /// - 其他类型忽略。
    ///
    /// 控制队列溢出时清空队列并返回 `resync/overflow`。
    pub fn apply(&mut self, event: &Event) -> Option<OutFrame> {
        let thing_id = event.source().thing_id()?;
        if !self.things.contains_key(thing_id) {
            return None;
        }
        match event.event_type() {
            EventType::Device(ThingEventType::PropertyChange) => {
                self.record_reading(event, thing_id);
                None
            }
            EventType::Device(
                ThingEventType::CommandStarted | ThingEventType::CommandCompleted | ThingEventType::CommandFailed,
            ) => self.push_control(event, thing_id, "job"),
            EventType::Device(ThingEventType::Connection) => self.push_control(event, thing_id, "state_change"),
            _ => None,
        }
    }

    fn record_reading(&mut self, event: &Event, thing_id: &str) {
        let Some(property) = self.property_name(event) else {
            return;
        };
        let Some(value) = event
            .content()
            .metadata()
            .get("value")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        let Some(sources) = self.things.get(thing_id) else {
            return;
        };
        let ts = event.timestamp().to_rfc3339();
        for source in sources.iter().filter(|source| source.covers(&property)) {
            let mut values = Map::new();
            values.insert(property.clone(), Value::String(value.clone()));
            self.coalesce.insert(
                (thing_id.to_string(), source.id.clone()),
                ReadingFrame {
                    v: 1,
                    ts: ts.clone(),
                    thing_id: thing_id.to_string(),
                    source: source.id.clone(),
                    values,
                },
            );
        }
    }

    /// 还原属性名：优先按 `{thing}:{property_id}` 查物属性，其次用事件的
    /// `property_name` 元数据（可能是 display_name）。
    fn property_name(&self, event: &Event) -> Option<String> {
        let thing_id = event.source().thing_id()?;
        let source_id = event.source().source_id();
        if let Some(property_id) = source_id.strip_prefix(thing_id).and_then(|rest| rest.strip_prefix(':'))
            && let Some(name) = self
                .property_names
                .get(thing_id)
                .and_then(|names| names.get(property_id))
        {
            return Some(name.clone());
        }
        event
            .content()
            .metadata()
            .get("property_name")
            .and_then(Value::as_str)
            .map(str::to_string)
    }

    fn push_control(&mut self, event: &Event, thing_id: &str, kind: &str) -> Option<OutFrame> {
        let frame = ControlFrame {
            v: 1,
            ts: event.timestamp().to_rfc3339(),
            thing_id: thing_id.to_string(),
            kind: kind.to_string(),
            payload: serde_json::to_value(event.content()).unwrap_or(Value::Null),
        };
        if self.control.len() >= CONTROL_QUEUE_LIMIT {
            self.control.clear();
            return Some(OutFrame::resync("overflow"));
        }
        self.control.push_back(frame);
        None
    }

    /// 取出并清空 50ms 窗口内合并的读数（按 `(thing, source)` 排序）。
    pub fn take_readings(&mut self) -> Vec<OutFrame> {
        std::mem::take(&mut self.coalesce)
            .into_values()
            .map(OutFrame::Reading)
            .collect()
    }

    /// 按到达顺序取走全部控制帧。
    pub fn drain_control(&mut self) -> Vec<OutFrame> {
        self.control.drain(..).map(OutFrame::Control).collect()
    }

    #[cfg(test)]
    fn control_len(&self) -> usize {
        self.control.len()
    }
}

/// 连接级需求守卫：Drop 时从 `DemandTracker` 移除订阅。
///
/// SSE 响应/流被丢弃（含客户端断连导致的流 drop）即触发；TTL 30s 只是
/// 兜底，正常断开应立即释放需求。
pub struct DemandGuard {
    demand: Arc<DemandTracker>,
    id: SubscriptionId,
}

impl DemandGuard {
    pub fn new(demand: Arc<DemandTracker>, id: SubscriptionId) -> Self {
        Self { demand, id }
    }

    pub fn id(&self) -> SubscriptionId {
        self.id
    }

    /// 续期一个完整 TTL（ping / 事件 / 冲刷 tick 调用）。
    pub fn touch(&self) {
        self.demand.touch(&self.id);
    }
}

impl Drop for DemandGuard {
    fn drop(&mut self) {
        self.demand.unsubscribe(&self.id);
    }
}

/// 便捷入口：登记订阅并返回守卫。
pub fn subscribe_with_guard(
    demand: Arc<DemandTracker>,
    workspace_id: &str,
    things: &[String],
    sources: &[String],
) -> DemandGuard {
    let id = demand.subscribe(workspace_id, things, sources, SUBSCRIPTION_TTL);
    DemandGuard::new(demand, id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tinyiothub_core::models::event::{EventLevel, EventSource, RichContent};
    use tinyiothub_core::models::thing::ThingStatus;
    use tinyiothub_core::models::thing_property::ThingProperty;

    fn property_change(thing: &str, property_id: &str, property_name: &str, value: &str) -> Event {
        let content = RichContent::new_text("Property Changed".into(), "changed".into())
            .with_metadata("property_name".to_string(), json!(property_name))
            .with_metadata("value".to_string(), json!(value));
        Event::new_device_event(
            ThingEventType::PropertyChange,
            EventLevel::Info,
            EventSource::device_property(thing.into(), property_id.into(), "test".into()),
            content,
        )
        .unwrap()
    }

    fn command_event(thing: &str, event_type: ThingEventType) -> Event {
        Event::new_device_event(
            event_type,
            EventLevel::Info,
            EventSource::device(thing.into(), Some("driver".into())),
            RichContent::new_text("Command".into(), "done".into()),
        )
        .unwrap()
    }

    fn spec(thing: &str, sources: &[(&str, &[&str])]) -> ThingSpec {
        let sources = sources
            .iter()
            .map(|(id, fields)| SourceSpec::new(*id, fields.iter().map(|f| (*f).to_string()).collect()))
            .collect();
        ThingSpec::new(thing, sources)
    }

    fn property_names(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(id, name)| ((*id).to_string(), (*name).to_string()))
            .collect()
    }

    #[test]
    fn property_change_maps_to_reading_for_each_declaring_source() {
        let mut state = HubStreamState::new([spec("t1", &[("telemetry", &["temperature"]), ("position", &[])])
            .with_property_names(property_names(&[("p1", "temperature")]))]);

        assert!(
            state
                .apply(&property_change("t1", "p1", "temperature", "22.5"))
                .is_none()
        );
        let frames = state.take_readings();
        assert_eq!(frames.len(), 2);
        let OutFrame::Reading(first) = &frames[0] else {
            panic!("expected reading");
        };
        assert_eq!(first.source, "position");
        assert_eq!(first.values.get("temperature"), Some(&json!("22.5")));
    }

    #[test]
    fn latest_wins_per_thing_and_source() {
        let mut state = HubStreamState::new([spec("t1", &[("telemetry", &[])])]);
        state.apply(&property_change("t1", "p1", "temperature", "20"));
        state.apply(&property_change("t1", "p1", "temperature", "21"));

        let frames = state.take_readings();
        assert_eq!(frames.len(), 1);
        let OutFrame::Reading(reading) = &frames[0] else {
            panic!("expected reading");
        };
        assert_eq!(reading.source, "telemetry");
        assert_eq!(reading.values.get("temperature"), Some(&json!("21")));
        assert!(state.take_readings().is_empty(), "flush must clear the window");
    }

    #[test]
    fn events_for_other_things_or_other_event_kinds_are_ignored() {
        let mut state = HubStreamState::new([spec("t1", &[("telemetry", &[])])]);
        state.apply(&property_change("t2", "p1", "temperature", "20"));
        state.apply(&command_event("t2", ThingEventType::CommandCompleted));
        state.apply(&command_event("t1", ThingEventType::PropertyAlarm));
        assert!(state.take_readings().is_empty());
        assert!(state.drain_control().is_empty());
    }

    #[test]
    fn commands_and_connection_map_to_ordered_control_frames() {
        let mut state = HubStreamState::new([spec("t1", &[("telemetry", &[])])]);
        assert!(
            state
                .apply(&command_event("t1", ThingEventType::CommandStarted))
                .is_none()
        );
        assert!(
            state
                .apply(&command_event("t1", ThingEventType::CommandCompleted))
                .is_none()
        );
        assert!(state.apply(&command_event("t1", ThingEventType::Connection)).is_none());

        let frames = state.drain_control();
        assert_eq!(frames.len(), 3);
        let kinds: Vec<_> = frames
            .iter()
            .map(|frame| match frame {
                OutFrame::Control(control) => control.kind.as_str(),
                _ => panic!("expected control"),
            })
            .collect();
        assert_eq!(kinds, ["job", "job", "state_change"]);
    }

    #[test]
    fn control_overflow_clears_the_queue_and_asks_for_resync() {
        let mut state = HubStreamState::new([spec("t1", &[("telemetry", &[])])]);
        for _ in 0..CONTROL_QUEUE_LIMIT {
            assert!(
                state
                    .apply(&command_event("t1", ThingEventType::CommandStarted))
                    .is_none()
            );
        }
        assert_eq!(state.control_len(), CONTROL_QUEUE_LIMIT);

        let overflow = state
            .apply(&command_event("t1", ThingEventType::CommandFailed))
            .expect("overflow must yield resync");
        assert_eq!(overflow, OutFrame::resync("overflow"));
        assert_eq!(state.control_len(), 0, "pending controls must be dropped");
        assert!(state.drain_control().is_empty());
    }

    #[test]
    fn lagged_resync_frame_has_the_expected_shape() {
        assert_eq!(
            OutFrame::resync("lagged").data(),
            json!({"v": 1, "reason": "lagged"}).to_string()
        );
        assert_eq!(OutFrame::ping().data(), json!({"v": 1}).to_string());
    }

    #[test]
    fn event_without_metadata_falls_back_to_property_id_resolution() {
        let mut state = HubStreamState::new([
            spec("t1", &[("telemetry", &["level"])]).with_property_names(property_names(&[("prop-1", "level")]))
        ]);
        // Metadata property_name intentionally absent; only `value` is set.
        let content =
            RichContent::new_text("changed".into(), "x".into()).with_metadata("value".to_string(), json!("7"));
        let event = Event::new_device_event(
            ThingEventType::PropertyChange,
            EventLevel::Info,
            EventSource::device_property("t1".into(), "prop-1".into(), "x".into()),
            content,
        )
        .unwrap();
        state.apply(&event);
        let frames = state.take_readings();
        assert_eq!(frames.len(), 1);
        let OutFrame::Reading(reading) = &frames[0] else {
            panic!("expected reading");
        };
        assert_eq!(reading.values.get("level"), Some(&json!("7")));
    }

    #[test]
    fn snapshot_filters_values_by_declared_fields() {
        let thing = Thing {
            id: "t1".into(),
            name: "Sensor".into(),
            status: ThingStatus::Online,
            last_heartbeat: Some("2026-10-21T10:00:00Z".into()),
            properties: Some(vec![
                ThingProperty {
                    id: "p1".into(),
                    thing_id: "t1".into(),
                    name: "temperature".into(),
                    current_value: Some("22".into()),
                    ..Default::default()
                },
                ThingProperty {
                    id: "p2".into(),
                    thing_id: "t1".into(),
                    name: "humidity".into(),
                    current_value: Some("40".into()),
                    ..Default::default()
                },
            ]),
            ..Default::default()
        };
        let specs = [spec("t1", &[("telemetry", &["temperature"]), ("all", &[])])];
        let OutFrame::Snapshot(snapshot) = build_snapshot(&[thing], &specs) else {
            panic!("expected snapshot");
        };
        assert_eq!(snapshot.v, 1);
        assert_eq!(snapshot.devices.len(), 1);
        let device = &snapshot.devices[0];
        assert_eq!(device.thing_id, "t1");
        assert_eq!(device.name, "Sensor");
        assert_eq!(device.sources.len(), 2);
        assert_eq!(device.sources[0].source, "telemetry");
        assert_eq!(device.sources[0].values.len(), 1);
        assert_eq!(device.sources[0].values.get("temperature"), Some(&json!("22")));
        assert_eq!(
            device.sources[0].last_change_at.as_deref(),
            Some("2026-10-21T10:00:00Z")
        );
        assert_eq!(device.sources[1].values.len(), 2);
    }

    #[test]
    fn guard_drop_removes_demand() {
        let tracker = Arc::new(DemandTracker::new());
        let guard = subscribe_with_guard(tracker.clone(), "ws", &["t1".into()], &["telemetry".into()]);
        assert!(tracker.has_subscription_demand("ws", "t1", "telemetry"));

        guard.touch();
        drop(guard);
        assert!(
            !tracker.has_subscription_demand("ws", "t1", "telemetry"),
            "dropping the connection guard must remove demand immediately"
        );
    }
}
