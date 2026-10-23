//! E3 实时投影 SSE（spec 2026-10-21-e3 §3.2）：
//! `GET /workspaces/{id}/live/stream`。
//!
//! 首帧 `snapshot`（LiveState）→ 账本增量（`id:` = hub_ledger.id）→
//! `resync`（重连/缺口）→ keepalive ping（15s）。客户端用 fetch + Bearer
//! 认证（EventSource 不能带 header），token 不进 URL。
//!
//! 实现：以账本 id 游标做 1s 尾随查询——与 `Last-Event-ID` 同一坐标系，
//! 重放/补发可验证；EventBus 直连留作后续优化。

use std::collections::VecDeque;
use std::convert::Infallible;
use std::time::Duration;

use axum::http::StatusCode;
use axum::{
    Router,
    extract::{Extension, Path, State},
    http::HeaderMap,
    response::{
        IntoResponse,
        sse::{Event, KeepAlive, Sse},
    },
    routing::get,
};
use serde_json::{Value, json};
use tinyiothub_web::security::Claims;

use crate::domains::agent::AgentState;

/// 账本尾随查询周期。
const TAIL_INTERVAL: Duration = Duration::from_secs(1);
/// 每次尾随最多取多少行。
const TAIL_LIMIT: i64 = 200;
/// SSE keepalive。
const KEEPALIVE: Duration = Duration::from_secs(15);

pub fn create_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    AgentState: axum::extract::FromRef<S>,
{
    Router::new().route("/{id}/live/stream", get(stream))
}

/// 账本 kind → SSE event 名。
pub fn event_name(kind: &str) -> Option<&'static str> {
    match kind {
        "state_change" => Some("state_change"),
        "watch_hit" | "watch_timeout" | "watch_expired" => Some("watch"),
        "job" => Some("job"),
        _ => None,
    }
}

pub fn row_to_event(row: &tinyiothub_storage::watch::LedgerRow) -> Option<Event> {
    let name = event_name(&row.kind)?;
    let payload: Value = serde_json::from_str(&row.payload).unwrap_or(Value::Null);
    let data = json!({
        "v": 1,
        "ledgerId": row.id,
        "ts": row.ts,
        "kind": row.kind,
        "thingId": row.thing_id,
        "payload": payload,
    });
    Some(
        Event::default()
            .id(row.id.to_string())
            .event(name)
            .data(data.to_string()),
    )
}

struct StreamState {
    pool: sqlx::SqlitePool,
    workspace_id: String,
    last_id: i64,
    pending: VecDeque<Event>,
    tick: tokio::time::Interval,
}

async fn next_event(mut st: StreamState) -> Option<(Result<Event, Infallible>, StreamState)> {
    loop {
        if let Some(ev) = st.pending.pop_front() {
            return Some((Ok(ev), st));
        }
        st.tick.tick().await;
        let rows = tinyiothub_storage::watch::list_ledger_range(
            &st.pool,
            &st.workspace_id,
            "1970-01-01T00:00:00Z",
            "9999-12-31T23:59:59Z",
            &[],
            None,
            Some(st.last_id),
            TAIL_LIMIT,
        )
        .await;
        let Ok(rows) = rows else {
            st.pending.push_back(
                Event::default()
                    .event("resync")
                    .data(json!({"v":1,"reason":"ledger_read_error"}).to_string()),
            );
            continue;
        };
        if rows.is_empty() {
            continue;
        }
        // 缺口：id 不连续 → 客户端先 resync 再接收增量。
        if let Some(first) = rows.first()
            && first.id > st.last_id + 1
        {
            st.pending.push_back(
                Event::default()
                    .event("resync")
                    .data(json!({"v":1,"reason":"gap"}).to_string()),
            );
        }
        for row in &rows {
            st.last_id = row.id;
            if let Some(ev) = row_to_event(row) {
                st.pending.push_back(ev);
            }
        }
    }
}

/// GET /workspaces/{id}/live/stream
pub async fn stream(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    headers: HeaderMap,
) -> axum::response::Response {
    // SSE 返回 Response，不套 Json 宏：租户校验失败直接转 Response。
    match state.workspace_access.workspace_tenant_id(&workspace_id).await {
        Ok(Some(tenant_id)) if tenant_id == claims.tenant_id => {}
        Ok(Some(_)) => return (StatusCode::FORBIDDEN, "无权访问此工作空间").into_response(),
        Ok(None) => return (StatusCode::NOT_FOUND, "工作空间不存在").into_response(),
        Err(e) => {
            tracing::error!("Failed to get workspace: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "获取工作空间失败").into_response();
        }
    }

    let last_event_id = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok());

    let pool = state.db.pool().clone();
    let mut pending: VecDeque<Event> = VecDeque::new();

    // 首帧 snapshot（engine 未运行 → 空快照/devices=[]，不炸）。
    let snapshot = match &state.thing_agents {
        Some(manager) => manager.workspace_snapshot(&workspace_id).await,
        None => None,
    };
    let snapshot_payload = match snapshot {
        Some(s) => {
            let devices: Vec<Value> = s
                .devices
                .iter()
                .map(|d| {
                    json!({
                        "thingId": d.thing_id,
                        "name": d.name,
                        "health": format!("{:?}", d.health).to_lowercase(),
                        "zone": d.zone,
                        "online": d.online,
                        "values": d.values,
                    })
                })
                .collect();
            json!({"v": 1, "devices": devices})
        }
        None => json!({"v": 1, "devices": []}),
    };
    pending.push_back(Event::default().event("snapshot").data(snapshot_payload.to_string()));

    let last_id = match last_event_id {
        Some(id) if id >= 0 => {
            // 重连：先让客户端 resync 一次，再接着推增量。
            pending.push_back(
                Event::default()
                    .event("resync")
                    .data(json!({"v":1,"reason":"reconnect"}).to_string()),
            );
            id
        }
        _ => tinyiothub_storage::watch::max_ledger_id(&pool, &workspace_id)
            .await
            .unwrap_or(0),
    };

    let mut tick = tokio::time::interval(TAIL_INTERVAL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let st = StreamState {
        pool,
        workspace_id,
        last_id,
        pending,
        tick,
    };
    let stream = futures::stream::unfold(st, next_event);
    Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(KEEPALIVE))
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(kind: &str) -> tinyiothub_storage::watch::LedgerRow {
        tinyiothub_storage::watch::LedgerRow {
            id: 42,
            workspace_id: "ws".into(),
            ts: "2026-10-21T10:00:00Z".into(),
            kind: kind.into(),
            thing_id: Some("t1".into()),
            payload: r#"{"v":1,"field":"温度","new":"22"}"#.into(),
        }
    }

    #[test]
    fn kind_mapping_covers_flow_events_and_skips_rate_limited() {
        assert_eq!(event_name("state_change"), Some("state_change"));
        assert_eq!(event_name("watch_hit"), Some("watch"));
        assert_eq!(event_name("watch_timeout"), Some("watch"));
        assert_eq!(event_name("job"), Some("job"));
        assert_eq!(event_name("rate_limited"), None);
        assert!(row_to_event(&row("rate_limited")).is_none());
    }
}
