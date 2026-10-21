//! E4b Hub 订阅流：`GET /v1/workspaces/{id}/hub/stream?things=&sources=`。
//!
//! 首帧 `snapshot` → `reading`（每 50ms latest-wins 合并）→ `control`
//! （有界 256，按到达顺序）→ `ping`（15s）→ `resync`
//! （`lagged`/`overflow`/`reconnect`）。
//!
//! 连接建立即 `DemandTracker.subscribe`；`DemandGuard` 在流 Drop（含客户端
//! 突然断开）时 `unsubscribe`，TTL 30s 仅兜底。

use std::collections::VecDeque;
use std::convert::Infallible;

use axum::{
    Extension, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{
        IntoResponse,
        sse::{Event, KeepAlive, Sse},
    },
    routing::get,
};
use serde::Deserialize;
use tinyiothub_core::driver::manifest::SourceDecl;
use tinyiothub_core::models::event::Event as DomainEvent;
use tinyiothub_core::models::thing::Thing;
use tinyiothub_runtime::hub_stream::{
    DemandGuard, HubStreamState, MAX_SOURCES, MAX_THINGS, OutFrame, PING_INTERVAL, READING_FLUSH_INTERVAL, SourceSpec,
    ThingSpec, build_snapshot, subscribe_with_guard,
};
use tinyiothub_web::security::Claims;
use tokio::sync::broadcast::error::RecvError;

use crate::domains::agent::AgentState;

pub fn create_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    AgentState: axum::extract::FromRef<S>,
{
    Router::new().route("/{id}/hub/stream", get(stream))
}

#[derive(Debug, Deserialize)]
pub struct HubQuery {
    pub things: Option<String>,
    pub sources: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ValidationError {
    BadRequest(String),
    Forbidden(String),
}

impl ValidationError {
    fn bad(message: impl Into<String>) -> Self {
        Self::BadRequest(message.into())
    }

    fn forbidden(message: impl Into<String>) -> Self {
        Self::Forbidden(message.into())
    }

    fn into_response(self) -> axum::response::Response {
        match self {
            ValidationError::BadRequest(message) => (StatusCode::BAD_REQUEST, message).into_response(),
            ValidationError::Forbidden(message) => (StatusCode::FORBIDDEN, message).into_response(),
        }
    }
}

/// 解析并去重查询列表；空列表 / 越过上限 → 400。
pub(crate) fn parse_query(
    things: Option<&str>,
    sources: Option<&str>,
) -> Result<(Vec<String>, Vec<String>), ValidationError> {
    let things = parse_list(things, "things")?;
    let sources = parse_list(sources, "sources")?;
    if things.len() > MAX_THINGS {
        return Err(ValidationError::bad(format!(
            "things 最多 {MAX_THINGS} 个（收到 {}）",
            things.len()
        )));
    }
    if sources.len() > MAX_SOURCES {
        return Err(ValidationError::bad(format!(
            "sources 最多 {MAX_SOURCES} 个（收到 {}）",
            sources.len()
        )));
    }
    Ok((things, sources))
}

fn parse_list(raw: Option<&str>, label: &str) -> Result<Vec<String>, ValidationError> {
    let raw = raw.unwrap_or("").trim();
    if raw.is_empty() {
        return Err(ValidationError::bad(format!("{label} 不能为空")));
    }
    let mut seen = std::collections::HashSet::new();
    let list: Vec<String> = raw
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .filter(|item| seen.insert((*item).to_string()))
        .map(str::to_string)
        .collect();
    if list.is_empty() {
        return Err(ValidationError::bad(format!("{label} 不能为空")));
    }
    Ok(list)
}

/// 校验每个 thing 属于该 workspace、每个 source 都在其 driver manifest 中声明。
///
/// `get_thing` / `get_sources` 由 handler 注入（DataServer + AccessService），
/// 便于单测直接覆盖未知设备 / 跨 workspace / 未声明 source 三条路径。
pub(crate) fn validate_targets<F, M>(
    workspace_id: &str,
    things: &[String],
    sources: &[String],
    get_thing: F,
    get_sources: M,
) -> Result<Vec<(Thing, ThingSpec)>, ValidationError>
where
    F: Fn(&str) -> Option<Thing>,
    M: Fn(&str) -> Option<Vec<SourceDecl>>,
{
    let mut targets = Vec::with_capacity(things.len());
    for thing_id in things {
        let thing = get_thing(thing_id).ok_or_else(|| ValidationError::bad(format!("设备不存在: {thing_id}")))?;
        if thing.workspace_id.as_deref() != Some(workspace_id) {
            return Err(ValidationError::forbidden(format!(
                "设备 {thing_id} 不属于工作空间 {workspace_id}"
            )));
        }
        let driver = thing
            .driver_name
            .clone()
            .ok_or_else(|| ValidationError::bad(format!("设备 {thing_id} 未绑定驱动")))?;
        let declared = get_sources(&driver)
            .ok_or_else(|| ValidationError::bad(format!("设备 {thing_id} 的驱动 {driver} 清单缺失")))?;

        let mut declared_sources = Vec::with_capacity(sources.len());
        for source in sources {
            let decl = declared.iter().find(|decl| decl.id == *source).ok_or_else(|| {
                ValidationError::bad(format!(
                    "source '{source}' 未在设备 {thing_id} 的驱动 {driver} 清单中声明"
                ))
            })?;
            declared_sources.push(SourceSpec::new(decl.id.clone(), decl.fields.clone()));
        }

        let property_names = thing
            .properties
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|property| (property.id.clone(), property.name.clone()))
            .collect();
        targets.push((
            thing.clone(),
            ThingSpec::new(thing.id.clone(), declared_sources).with_property_names(property_names),
        ));
    }
    Ok(targets)
}

/// GET /v1/workspaces/{id}/hub/stream
pub async fn stream(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Query(query): Query<HubQuery>,
    headers: HeaderMap,
) -> axum::response::Response {
    if workspace_id.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "工作空间 ID 不能为空").into_response();
    }

    match state.workspace_access.workspace_tenant_id(&workspace_id).await {
        Ok(Some(tenant_id)) if tenant_id == claims.tenant_id => {}
        Ok(Some(_)) => return (StatusCode::FORBIDDEN, "无权访问此工作空间").into_response(),
        Ok(None) => return (StatusCode::NOT_FOUND, "工作空间不存在").into_response(),
        Err(e) => {
            tracing::error!("Failed to get workspace: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "获取工作空间失败").into_response();
        }
    }

    let Some(access) = state.access.clone() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "E4 驱动访问服务未装配").into_response();
    };
    let Some(data_server) = state.data_server.clone() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "数据服务未装配").into_response();
    };

    // 先订阅 EventBus，再读设备快照：快照读取与校验期间发生的事件会先
    // 进入本连接的 broadcast 缓冲，避免「快照之后、订阅之前」的增量丢失。
    let events = state.event_bus.subscribe();

    let (things, sources) = match parse_query(query.things.as_deref(), query.sources.as_deref()) {
        Ok(parsed) => parsed,
        Err(error) => return error.into_response(),
    };
    let targets = match validate_targets(
        &workspace_id,
        &things,
        &sources,
        |thing_id| data_server.get_device(thing_id),
        |driver| {
            access
                .get_manifest(&workspace_id, driver)
                .map(|package| package.manifest.sources.clone())
        },
    ) {
        Ok(targets) => targets,
        Err(error) => return error.into_response(),
    };

    let (devices, specs): (Vec<Thing>, Vec<ThingSpec>) = targets.into_iter().unzip();
    let guard = subscribe_with_guard(access.demand.clone(), &workspace_id, &things, &sources);

    let mut pending = VecDeque::new();
    pending.push_back(build_snapshot(&devices, &specs));
    if headers.contains_key("last-event-id") {
        pending.push_back(OutFrame::resync("reconnect"));
    }

    let mut readings_tick = tokio::time::interval(READING_FLUSH_INTERVAL);
    readings_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // 首帧已是 snapshot；ping 从第一个完整周期后开始（interval 的首 tick
    // 是立即完成的，这里显式推迟）。
    let mut ping_tick = tokio::time::interval_at(tokio::time::Instant::now() + PING_INTERVAL, PING_INTERVAL);
    ping_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let connection = ConnectionState {
        hub: HubStreamState::new(specs),
        guard,
        events,
        pending,
        readings_tick,
        ping_tick,
    };

    Sse::new(futures::stream::unfold(connection, next_frame))
        .keep_alive(KeepAlive::new().interval(PING_INTERVAL))
        .into_response()
}

struct ConnectionState {
    hub: HubStreamState,
    /// Drop 即 unsubscribe（客户端突然断开也走这里）。
    guard: DemandGuard,
    events: tokio::sync::broadcast::Receiver<DomainEvent>,
    pending: VecDeque<OutFrame>,
    readings_tick: tokio::time::Interval,
    ping_tick: tokio::time::Interval,
}

async fn next_frame(state: ConnectionState) -> Option<(Result<Event, Infallible>, ConnectionState)> {
    let (frame, state) = next_out_frame(state).await?;
    Some((Ok(to_sse(frame)), state))
}

/// 帧选择循环（与 SSE 序列化分离，便于单测快照→增量顺序与生命周期）。
async fn next_out_frame(mut state: ConnectionState) -> Option<(OutFrame, ConnectionState)> {
    loop {
        if let Some(frame) = state.pending.pop_front() {
            return Some((frame, state));
        }

        tokio::select! {
            received = state.events.recv() => {
                // 每个事件都续期；客户端静默也不会在 30s 内被 TTL 清掉。
                state.guard.touch();
                match received {
                    Ok(event) => {
                        // 控制帧留到 50ms 冲刷点统一出队：突发时由状态机的
                        // 256 上限兜底（溢出回 resync），并保持到达顺序。
                        if let Some(frame) = state.hub.apply(&event) {
                            state.pending.push_back(frame);
                        }
                    }
                    Err(RecvError::Lagged(skipped)) => {
                        tracing::warn!(
                            skipped,
                            "hub stream subscriber lagged; sending resync"
                        );
                        state.pending.push_back(OutFrame::resync("lagged"));
                    }
                    Err(RecvError::Closed) => return None,
                }
            }
            _ = state.readings_tick.tick() => {
                state.guard.touch();
                for frame in state.hub.take_readings() {
                    state.pending.push_back(frame);
                }
                for frame in state.hub.drain_control() {
                    state.pending.push_back(frame);
                }
            }
            _ = state.ping_tick.tick() => {
                state.guard.touch();
                state.pending.push_back(OutFrame::ping());
            }
        }
    }
}

fn to_sse(frame: OutFrame) -> Event {
    Event::default().event(frame.name()).data(frame.data())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;

    use serde_json::json;
    use tinyiothub_core::models::event::{EventLevel, EventSource, RichContent, ThingEventType};
    use tinyiothub_core::models::thing::ThingStatus;
    use tinyiothub_core::models::thing_property::ThingProperty;
    use tinyiothub_runtime::access::demand::DemandTracker;

    fn source(id: &str, fields: &[&str]) -> SourceDecl {
        SourceDecl {
            id: id.into(),
            kind: "poll".into(),
            fields: fields.iter().map(|field| (*field).to_string()).collect(),
            interval_ms: 1000,
            min_interval_ms: 200,
            demand_only: false,
            health: false,
        }
    }

    fn thing(id: &str, workspace: &str, driver: &str) -> Thing {
        Thing {
            id: id.into(),
            name: id.into(),
            workspace_id: Some(workspace.into()),
            driver_name: Some(driver.into()),
            status: ThingStatus::Online,
            properties: Some(vec![ThingProperty {
                id: format!("{id}-p1"),
                thing_id: id.into(),
                name: "temperature".into(),
                current_value: Some("22".into()),
                ..Default::default()
            }]),
            ..Default::default()
        }
    }

    #[test]
    fn query_limits_and_empty_inputs_are_rejected() {
        let too_many_things = (0..=MAX_THINGS)
            .map(|index| format!("t{index}"))
            .collect::<Vec<_>>()
            .join(",");
        assert!(matches!(
            parse_query(Some(&too_many_things), Some("telemetry")),
            Err(ValidationError::BadRequest(_))
        ));

        let too_many_sources = (0..=MAX_SOURCES)
            .map(|index| format!("s{index}"))
            .collect::<Vec<_>>()
            .join(",");
        assert!(matches!(
            parse_query(Some("t1"), Some(&too_many_sources)),
            Err(ValidationError::BadRequest(_))
        ));

        assert!(matches!(
            parse_query(None, Some("telemetry")),
            Err(ValidationError::BadRequest(_))
        ));
        assert!(matches!(
            parse_query(Some(" , "), Some("telemetry")),
            Err(ValidationError::BadRequest(_))
        ));
    }

    #[test]
    fn query_deduplicates_and_trims() {
        let (things, sources) = parse_query(Some(" t1, t2 ,t1, "), Some("telemetry, telemetry")).unwrap();
        assert_eq!(things, ["t1", "t2"]);
        assert_eq!(sources, ["telemetry"]);
    }

    #[test]
    fn unknown_foreign_and_undeclared_targets_are_rejected() {
        let sources = HashMap::from([("demo".to_string(), vec![source("telemetry", &["temperature"])])]);
        let get_sources = |driver: &str| sources.get(driver).cloned();
        let local = thing("t1", "ws1", "demo");
        let foreign = thing("t2", "ws2", "demo");

        let unknown = validate_targets(
            "ws1",
            &["missing".to_string()],
            &["telemetry".to_string()],
            |_| None,
            get_sources,
        );
        assert!(matches!(unknown, Err(ValidationError::BadRequest(_))));

        let cross_workspace = validate_targets(
            "ws1",
            &["t2".to_string()],
            &["telemetry".to_string()],
            |id| (id == "t2").then(|| foreign.clone()),
            get_sources,
        );
        assert!(matches!(cross_workspace, Err(ValidationError::Forbidden(_))));

        let undeclared = validate_targets(
            "ws1",
            &["t1".to_string()],
            &["position".to_string()],
            |id| (id == "t1").then(|| local.clone()),
            get_sources,
        );
        assert!(
            matches!(undeclared, Err(ValidationError::BadRequest(ref message)) if message.contains("position")),
            "{undeclared:?}"
        );
    }

    #[test]
    fn valid_targets_carry_declared_sources_and_property_names() {
        let sources = HashMap::from([(
            "demo".to_string(),
            vec![source("telemetry", &["temperature"]), source("health", &[])],
        )]);
        let targets = validate_targets(
            "ws1",
            &["t1".to_string()],
            &["telemetry".to_string(), "health".to_string()],
            |_| Some(thing("t1", "ws1", "demo")),
            |driver| sources.get(driver).cloned(),
        )
        .expect("valid target");

        assert_eq!(targets.len(), 1);
        let (_, spec) = &targets[0];
        assert_eq!(spec.sources.len(), 2);
        assert_eq!(spec.sources[0].id, "telemetry");
        assert_eq!(spec.sources[1].fields, Vec::<String>::new());
        assert_eq!(
            spec.property_names.get("t1-p1").map(String::as_str),
            Some("temperature")
        );
    }

    #[test]
    fn validation_errors_map_to_http_statuses() {
        assert_eq!(
            ValidationError::bad("x").into_response().status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            ValidationError::forbidden("x").into_response().status(),
            StatusCode::FORBIDDEN
        );
    }

    fn property_event(thing_id: &str, property: &str, value: &str) -> DomainEvent {
        let content = RichContent::new_text("Property Changed".into(), "changed".into())
            .with_metadata("property_name".to_string(), json!(property))
            .with_metadata("value".to_string(), json!(value));
        DomainEvent::new_device_event(
            ThingEventType::PropertyChange,
            EventLevel::Info,
            EventSource::device_property(thing_id.into(), format!("{thing_id}-p1"), "test".into()),
            content,
        )
        .unwrap()
    }

    fn connection_state(
        hub: HubStreamState,
        guard: DemandGuard,
        events: tokio::sync::broadcast::Receiver<DomainEvent>,
        pending: VecDeque<OutFrame>,
    ) -> ConnectionState {
        let mut readings_tick = tokio::time::interval(Duration::from_millis(5));
        readings_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut ping_tick = tokio::time::interval_at(
            tokio::time::Instant::now() + Duration::from_secs(3600),
            Duration::from_secs(3600),
        );
        ping_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        ConnectionState {
            hub,
            guard,
            events,
            pending,
            readings_tick,
            ping_tick,
        }
    }

    #[tokio::test]
    async fn next_frame_emits_snapshot_before_reading() {
        let tracker = Arc::new(DemandTracker::new());
        let guard = subscribe_with_guard(tracker.clone(), "ws", &["t1".to_string()], &["telemetry".to_string()]);
        let specs = vec![ThingSpec::new("t1", vec![SourceSpec::new("telemetry", Vec::new())])];
        let mut pending = VecDeque::new();
        pending.push_back(build_snapshot(&[thing("t1", "ws", "demo")], &specs));

        let (tx, rx) = tokio::sync::broadcast::channel(16);
        let state = connection_state(HubStreamState::new(specs), guard, rx, pending);
        tx.send(property_event("t1", "temperature", "23")).unwrap();

        let (first, state) = next_out_frame(state).await.expect("snapshot frame");
        assert!(
            matches!(first, OutFrame::Snapshot(_)),
            "first frame must be the snapshot, got {first:?}"
        );

        let (second, _state) = next_out_frame(state).await.expect("reading frame");
        let OutFrame::Reading(reading) = second else {
            panic!("second frame must be a reading, got {second:?}");
        };
        assert_eq!(reading.source, "telemetry");
        assert_eq!(reading.values.get("temperature"), Some(&json!("23")));
    }

    #[tokio::test]
    async fn dropping_the_stream_state_unsubscribes_demand() {
        let tracker = Arc::new(DemandTracker::new());
        let guard = subscribe_with_guard(tracker.clone(), "ws", &["t1".to_string()], &["telemetry".to_string()]);
        assert!(tracker.has_subscription_demand("ws", "t1", "telemetry"));

        let (_tx, rx) = tokio::sync::broadcast::channel(4);
        let state = connection_state(
            HubStreamState::new([ThingSpec::new("t1", vec![SourceSpec::new("telemetry", Vec::new())])]),
            guard,
            rx,
            VecDeque::new(),
        );

        // 客户端突然断开 → SSE 流被 drop → 连接态（含 DemandGuard）drop。
        drop(state);
        assert!(
            !tracker.has_subscription_demand("ws", "t1", "telemetry"),
            "dropping the connection state must unsubscribe demand immediately"
        );
    }
}
