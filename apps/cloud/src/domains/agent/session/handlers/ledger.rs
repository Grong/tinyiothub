//! E3 账本读面（spec 2026-10-21-e3 §3.1）：
//! `GET /workspaces/{id}/ledger` 范围查询 + `GET /{id}/ledger/state` 状态重建。
//!
//! 只读；按工作区隔离；窗口上限 7d、limit ≤500。回放精度=state_change；
//! health/zone 不在账本内，显式列入 `unknown`，不用当前值冒充历史。

use std::collections::BTreeMap;

use axum::{
    Json, Router,
    extract::{Extension, Path, Query, State},
    routing::get,
};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use tinyiothub_web::api_response::ApiResponse;
use tinyiothub_web::response::ApiResponseBuilder;
use tinyiothub_web::security::Claims;

use crate::domains::agent::AgentState;
use crate::verify_workspace_access_port;

const DEFAULT_LIMIT: i64 = 200;
const MAX_LIMIT: i64 = 500;
const MAX_WINDOW_DAYS: i64 = 7;

pub fn create_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    AgentState: axum::extract::FromRef<S>,
{
    Router::new()
        .route("/{id}/ledger", get(query_ledger))
        .route("/{id}/ledger/state", get(query_state))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerQuery {
    pub from: String,
    pub to: String,
    /// 逗号分隔的 kind 白名单。
    pub kinds: Option<String>,
    #[serde(alias = "thing_id")]
    pub thing_id: Option<String>,
    pub cursor: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct StateQuery {
    pub at: String,
}

fn parse_ts(value: &str, field: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(value)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| format!("{field} 不是 RFC3339: {e}"))
}

/// 校验时间窗并返回 (from, to, limit)。
pub fn validate_range(from: &str, to: &str, limit: Option<i64>) -> Result<(String, String, i64), String> {
    let from_ts = parse_ts(from, "from")?;
    let to_ts = parse_ts(to, "to")?;
    if from_ts > to_ts {
        return Err("from 必须早于等于 to".to_string());
    }
    if to_ts - from_ts > Duration::days(MAX_WINDOW_DAYS) {
        return Err(format!("时间窗上限 {MAX_WINDOW_DAYS} 天"));
    }
    let limit = limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    Ok((from_ts.to_rfc3339(), to_ts.to_rfc3339(), limit))
}

/// GET /workspaces/{id}/ledger
pub async fn query_ledger(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Query(q): Query<LedgerQuery>,
) -> Json<ApiResponse<Value>> {
    verify_workspace_access_port!(state, claims, workspace_id);

    let (from, to, limit) = match validate_range(&q.from, &q.to, q.limit) {
        Ok(v) => v,
        Err(e) => return ApiResponseBuilder::error_with_code(400, e),
    };
    let kinds: Vec<String> = q
        .kinds
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();

    let rows = match tinyiothub_storage::watch::list_ledger_range(
        state.db.pool(),
        &workspace_id,
        &from,
        &to,
        &kinds,
        q.thing_id.as_deref(),
        q.cursor,
        limit,
    )
    .await
    {
        Ok(rows) => rows,
        Err(e) => return ApiResponseBuilder::error(format!("账本查询失败: {e}")),
    };

    let has_more = rows.len() as i64 == limit;
    let next_cursor = rows.last().map(|r| r.id);
    let gaps = if has_more && !rows.is_empty() {
        vec![json!({
            "from": rows.last().map(|r| r.ts.clone()).unwrap_or_else(|| from.clone()),
            "to": to,
            "reason": "query_cap",
        })]
    } else {
        vec![]
    };
    let entries: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            let payload: Value = serde_json::from_str(&r.payload).unwrap_or(Value::Null);
            json!({
                "id": r.id,
                "ts": r.ts,
                "kind": r.kind,
                "thingId": r.thing_id,
                "payload": payload,
            })
        })
        .collect();

    ApiResponseBuilder::success(json!({
        "entries": entries,
        "nextCursor": next_cursor,
        "hasMore": has_more,
        "coverage": { "from": from, "to": to, "gaps": gaps },
    }))
}

/// 把 (thing, field, new) 行折叠成每设备状态。
pub fn fold_state(rows: &[tinyiothub_storage::watch::LedgerRow]) -> Vec<Value> {
    let mut by_thing: BTreeMap<String, (BTreeMap<String, Value>, String, Option<bool>)> = BTreeMap::new();
    for row in rows {
        let Some(thing_id) = row.thing_id.clone() else {
            continue;
        };
        let payload: Value = serde_json::from_str(&row.payload).unwrap_or(Value::Null);
        let Some(field) = payload.get("field").and_then(|v| v.as_str()) else {
            continue;
        };
        let new = payload.get("new").cloned().unwrap_or(Value::Null);
        let entry = by_thing
            .entry(thing_id)
            .or_insert_with(|| (BTreeMap::new(), row.ts.clone(), None));
        if row.ts > entry.1 {
            entry.1 = row.ts.clone();
        }
        if field == "__availability" {
            entry.2 = new.as_bool().or_else(|| new.as_str().map(|s| s == "true"));
        } else {
            entry.0.insert(field.to_string(), new);
        }
    }
    by_thing
        .into_iter()
        .map(|(thing_id, (state, last_change_at, online))| {
            json!({
                "thingId": thing_id,
                "state": state,
                "online": online,
                "lastChangeAt": last_change_at,
            })
        })
        .collect()
}

/// GET /workspaces/{id}/ledger/state?at=
pub async fn query_state(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Query(q): Query<StateQuery>,
) -> Json<ApiResponse<Value>> {
    verify_workspace_access_port!(state, claims, workspace_id);
    let at = match parse_ts(&q.at, "at") {
        Ok(t) => t.to_rfc3339(),
        Err(e) => return ApiResponseBuilder::error_with_code(400, e),
    };
    let rows = match tinyiothub_storage::watch::latest_state_changes_at(state.db.pool(), &workspace_id, &at).await {
        Ok(rows) => rows,
        Err(e) => return ApiResponseBuilder::error(format!("状态重建失败: {e}")),
    };
    let devices = fold_state(&rows);
    ApiResponseBuilder::success(json!({
        "at": at,
        "devices": devices,
        "precision": "state_change",
        // E1 账本未落派生 health/zone 变化——显式声明不可重建，不用当前值冒充。
        "unknown": ["health", "zone"],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_validation_rejects_bad_windows_and_clamps_limit() {
        let (from, to, limit) = validate_range("2026-10-21T00:00:00Z", "2026-10-21T01:00:00Z", Some(999)).unwrap();
        assert_eq!(limit, MAX_LIMIT);
        assert!(from.starts_with("2026-10-21"));
        assert!(to.starts_with("2026-10-21"));

        assert!(validate_range("2026-10-21T00:00:00Z", "2026-10-20T00:00:00Z", None).is_err());
        assert!(validate_range("2026-10-01T00:00:00Z", "2026-10-20T00:00:00Z", None).is_err());
        assert!(validate_range("nope", "2026-10-20T00:00:00Z", None).is_err());
        assert_eq!(
            validate_range("2026-10-20T00:00:00Z", "2026-10-20T01:00:00Z", None)
                .unwrap()
                .2,
            DEFAULT_LIMIT
        );
    }

    fn row(ts: &str, thing: &str, payload: Value) -> tinyiothub_storage::watch::LedgerRow {
        tinyiothub_storage::watch::LedgerRow {
            id: 1,
            workspace_id: "ws".into(),
            ts: ts.into(),
            kind: "state_change".into(),
            thing_id: Some(thing.into()),
            payload: payload.to_string(),
        }
    }

    #[test]
    fn fold_state_keeps_last_change_and_separates_availability() {
        let rows = vec![
            row("2026-10-21T10:00:00Z", "t1", json!({"v":1,"field":"温度","new":"20"})),
            row("2026-10-21T10:01:00Z", "t1", json!({"v":1,"field":"温度","new":"22"})),
            row(
                "2026-10-21T10:02:00Z",
                "t1",
                json!({"v":1,"field":"__availability","new":false}),
            ),
        ];
        let devices = fold_state(&rows);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0]["state"]["温度"], "22");
        assert_eq!(devices[0]["online"], false);
        assert_eq!(devices[0]["lastChangeAt"], "2026-10-21T10:02:00Z");
        assert!(devices[0]["state"].get("__availability").is_none());
    }
}
