//! Session 事件、Todo、问题回答的 HTTP 端点。
//!
//! 所有端点先校验 session_key 的 workspace 归属，再访问 DB / broker。

use std::convert::Infallible;
use std::time::Duration;

use async_stream::stream;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use tinyiothub_agent::session::SessionKey;
use tinyiothub_core::harness::{ExecutionMode, HarnessEvent};
use tinyiothub_storage::harness_event::EventFilter;
use tinyiothub_web::security::Claims;

use crate::domains::agent::AgentState;
use crate::domains::agent::session::QuestionAnswer;
use crate::domains::agent::session::types::PermissionMode;
use crate::domains::agent::session::{ApprovalDecision, ApprovalError};

#[derive(Debug, Deserialize)]
pub struct EventQuery {
    #[serde(default)]
    pub after: i64,
    pub limit: Option<i64>,
    pub turns: Option<i64>,
    pub before: Option<i64>,
}

pub(crate) fn verify_session_workspace(session_key: &str, authorized_workspace: &str) -> Option<SessionKey> {
    let parsed = SessionKey::parse(session_key).ok()?;
    if parsed.workspace_id != authorized_workspace {
        return None;
    }
    Some(parsed)
}

pub(crate) fn parse_permission_mode(value: &str) -> Option<PermissionMode> {
    PermissionMode::parse(value.trim())
}

#[derive(Debug, Deserialize)]
pub struct UpdatePermissionRequest {
    pub mode: String,
}

pub async fn update_permission(
    State(state): State<AgentState>,
    claims: Claims,
    Path(session_key): Path<String>,
    Json(request): Json<UpdatePermissionRequest>,
) -> Response {
    let Some(parsed) = verify_session_workspace(&session_key, &claims.workspace_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(mode) = parse_permission_mode(&request.mode) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let key = parsed.session_key();
    let Some(mut session) = state.db.get_session(&key).await.ok().flatten() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    session.permission_mode = mode.as_str().to_string();
    session.touch();
    match state.db.update_session(&session).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => {
            tracing::warn!(%error, %key, "update session permission failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn event_filter(session_id: &str) -> EventFilter {
    EventFilter {
        session_id: Some(session_id.to_string()),
        run_id: None,
        task_id: None,
        execution_mode: ExecutionMode::Production,
    }
}

/// Backfill viewKey for rows written before the canonical catalog. This is a
/// read-time projection; stored payloads and their digests stay untouched.
fn with_tool_view_key(mut event: HarnessEvent) -> HarnessEvent {
    if !matches!(event.event_type.as_str(), "tool/call" | "tool/result") {
        return event;
    }
    let Some(name) = event.payload_json.get("tool").and_then(|value| value.as_str()) else {
        return event;
    };
    let view_key = crate::domains::agent::tools::catalog::tool_view_key(name);
    if let Some(payload) = event.payload_json.as_object_mut() {
        payload
            .entry("viewKey")
            .or_insert_with(|| serde_json::Value::String(view_key.to_string()));
    }
    event
}

pub async fn list_events(
    State(state): State<AgentState>,
    claims: Claims,
    Path(session_key): Path<String>,
    Query(query): Query<EventQuery>,
) -> Response {
    let Some(parsed) = verify_session_workspace(&session_key, &claims.workspace_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let filter = event_filter(&parsed.session_key());
    if let Some(turns) = query.turns {
        return match state
            .db
            .session_turn_page(&parsed.workspace_id, &filter, query.before, turns)
            .await
        {
            Ok(page) => Json(serde_json::json!({
                "events": page.events.into_iter().map(with_tool_view_key).collect::<Vec<_>>(),
                "highWatermark": page.high_watermark,
                "nextBefore": page.next_before,
                "hasMore": page.has_more,
            }))
            .into_response(),
            Err(error) => {
                tracing::warn!(%error, %session_key, "session turn page failed");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        };
    }
    match state
        .db
        .session_event_page(&parsed.workspace_id, &filter, query.after, query.limit.unwrap_or(200))
        .await
    {
        Ok(page) => Json(serde_json::json!({
            "events": page
                .events
                .into_iter()
                .filter(|event| super::is_public_session_event(&event.event_type))
                .map(with_tool_view_key)
                .collect::<Vec<_>>(),
            "highWatermark": page.high_watermark,
        }))
        .into_response(),
        Err(error) => {
            tracing::warn!(%error, %session_key, "session event page failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn stream_events(
    State(state): State<AgentState>,
    claims: Claims,
    Path(session_key): Path<String>,
    Query(query): Query<EventQuery>,
    headers: HeaderMap,
) -> Response {
    let Some(parsed) = verify_session_workspace(&session_key, &claims.workspace_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let filter = event_filter(&parsed.session_key());
    let db = state.db.clone();
    // EventSource reconnect semantics: prefer the standard header when the
    // query cursor is absent, while keeping the explicit cursor authoritative.
    let last_event_id = headers
        .get("last-event-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0);
    let after = query.after.max(last_event_id);
    let notifier = state.session_stream.subscribe(&claims.workspace_id);
    let mut live = state.session_runtime.live.subscribe(&parsed.session_key());

    let event_stream = stream! {
        let mut cursor = after;
        loop {
            let mut emitted = 0usize;
            match db
                .session_event_page(&parsed.workspace_id, &filter, cursor, 200)
                .await
            {
                Ok(page) => {
                    emitted = page.events.len();
                    for event in page.events {
                        cursor = event.seq;
                        if !super::is_public_session_event(&event.event_type) {
                            continue;
                        }
                        let event = with_tool_view_key(event);
                        if let Ok(frame) = Event::default()
                            .event("session")
                            .id(event.seq.to_string())
                            .json_data(event)
                        {
                            yield Ok::<_, Infallible>(frame);
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, %session_key, "session event tail failed");
                }
            }
            // Drain a full page immediately; otherwise wait for a committed
            // write hint, with a slow fallback for writers in another process.
            if emitted >= 200 {
                continue;
            }
            tokio::select! {
                _ = notifier.notified() => {}
                _ = tokio::time::sleep(Duration::from_secs(1)) => {}
                frame = live.recv() => {
                    if let Ok(frame) = frame
                        && let Ok(event) = Event::default().event("live").json_data(frame)
                    {
                        yield Ok::<_, Infallible>(event);
                    }
                }
            }
        }
    };

    let mut response = Sse::new(event_stream).keep_alive(KeepAlive::default()).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response.headers_mut().insert(
        axum::http::HeaderName::from_static("x-accel-buffering"),
        HeaderValue::from_static("no"),
    );
    response
}

pub async fn todos(State(state): State<AgentState>, claims: Claims, Path(session_key): Path<String>) -> Response {
    let Some(parsed) = verify_session_workspace(&session_key, &claims.workspace_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    Json(state.session_runtime.todos.get(&parsed.session_key())).into_response()
}

pub async fn answer_question(
    State(state): State<AgentState>,
    claims: Claims,
    Path((session_key, question_id)): Path<(String, String)>,
    Json(answer): Json<QuestionAnswer>,
) -> Response {
    let Some(parsed) = verify_session_workspace(&session_key, &claims.workspace_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match state
        .session_runtime
        .questions
        .answer(&parsed.session_key(), &question_id, answer)
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(crate::domains::agent::session::questions::QuestionError::NotFound) => {
            StatusCode::NOT_FOUND.into_response()
        }
        Err(crate::domains::agent::session::questions::QuestionError::SessionMismatch) => {
            StatusCode::FORBIDDEN.into_response()
        }
        Err(crate::domains::agent::session::questions::QuestionError::Closed) => StatusCode::CONFLICT.into_response(),
    }
}

pub(crate) fn parse_approval_decision(value: &str) -> Option<ApprovalDecision> {
    match value.trim() {
        "approved" => Some(ApprovalDecision::Approved),
        "denied" => Some(ApprovalDecision::Denied),
        _ => None,
    }
}

#[derive(Debug, Deserialize)]
pub struct ApprovalAnswerRequest {
    pub decision: String,
}

pub async fn answer_approval(
    State(state): State<AgentState>,
    claims: Claims,
    Path((session_key, approval_id)): Path<(String, String)>,
    Json(request): Json<ApprovalAnswerRequest>,
) -> Response {
    let Some(parsed) = verify_session_workspace(&session_key, &claims.workspace_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(decision) = parse_approval_decision(&request.decision) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    match state
        .session_runtime
        .approvals
        .answer(&parsed.session_key(), &approval_id, decision)
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(ApprovalError::NotFound) => StatusCode::NOT_FOUND.into_response(),
        Err(ApprovalError::SessionMismatch) => StatusCode::FORBIDDEN.into_response(),
        Err(ApprovalError::Closed) => StatusCode::CONFLICT.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_mismatch_is_rejected() {
        let key = "agent:ws-a:agent_main/s1";
        assert!(verify_session_workspace(key, "ws-a").is_some());
        assert!(verify_session_workspace(key, "ws-b").is_none());
    }

    #[test]
    fn malformed_session_key_is_rejected() {
        assert!(verify_session_workspace("chat:ws:agent/s1", "ws").is_none());
        assert!(verify_session_workspace("agent:ws:agent", "ws").is_none());
    }

    #[test]
    fn permission_mode_body_defaults_and_rejects_unknown() {
        assert_eq!(parse_permission_mode("ask"), Some(PermissionMode::Ask));
        assert_eq!(parse_permission_mode("read_only"), Some(PermissionMode::ReadOnly));
        assert_eq!(parse_permission_mode("admin"), None);
    }

    fn event(event_type: &str, payload_json: serde_json::Value) -> HarnessEvent {
        HarnessEvent {
            seq: 1,
            event_id: "e1".into(),
            workspace_id: "ws".into(),
            session_id: "agent:ws:agent_main/s1".into(),
            execution_mode: ExecutionMode::Production,
            run_id: None,
            task_id: None,
            event_type: event_type.into(),
            occurred_at: chrono::Utc::now(),
            cause_id: None,
            schema_v: 1,
            payload_digest: "d".into(),
            payload_json,
        }
    }

    #[test]
    fn historical_tool_events_are_projected_with_a_view_key() {
        let call = with_tool_view_key(event(
            "tool/call",
            serde_json::json!({"tool": "read_device", "args": {}}),
        ));
        assert_eq!(call.payload_json["viewKey"], "device.read");
        let result = with_tool_view_key(event(
            "tool/result",
            serde_json::json!({"tool": "alarm_list", "result": "{}"}),
        ));
        assert_eq!(result.payload_json["viewKey"], "alarm.list");
        let kept = with_tool_view_key(event(
            "tool/result",
            serde_json::json!({"tool": "read_device", "viewKey": "custom.view"}),
        ));
        assert_eq!(kept.payload_json["viewKey"], "custom.view");
    }

    #[test]
    fn approval_decision_parses() {
        assert_eq!(parse_approval_decision("approved"), Some(ApprovalDecision::Approved));
        assert_eq!(parse_approval_decision("denied"), Some(ApprovalDecision::Denied));
        assert_eq!(parse_approval_decision("maybe"), None);
    }
}
