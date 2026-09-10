//! Ticket DTO — API 出入参。

use serde::{Deserialize, Serialize};
use tinyiothub_storage::ticket::{Ticket, TicketEvent};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketDto {
    pub id: i64,
    pub workspace_id: String,
    pub thing_id: Option<String>,
    pub agent_run_id: String,
    pub session_key: Option<String>,
    pub title: String,
    pub briefing: serde_json::Value,
    pub state: String,
    pub assignee_id: Option<String>,
    pub resolution_text: Option<String>,
    pub created_at: String,
    pub claimed_at: Option<String>,
    pub resolved_at: Option<String>,
    pub closed_at: Option<String>,
}

impl From<Ticket> for TicketDto {
    fn from(t: Ticket) -> Self {
        Self {
            id: t.id,
            workspace_id: t.workspace_id,
            thing_id: t.thing_id,
            agent_run_id: t.agent_run_id,
            session_key: t.session_key,
            title: t.title,
            briefing: serde_json::from_str(&t.briefing).unwrap_or(serde_json::Value::Null),
            state: t.state,
            assignee_id: t.assignee_id,
            resolution_text: t.resolution_text,
            created_at: t.created_at,
            claimed_at: t.claimed_at,
            resolved_at: t.resolved_at,
            closed_at: t.closed_at,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketEventDto {
    pub id: i64,
    pub kind: String,
    pub actor_type: String,
    pub actor_id: Option<String>,
    pub payload: serde_json::Value,
    pub created_at: String,
}

impl From<TicketEvent> for TicketEventDto {
    fn from(e: TicketEvent) -> Self {
        Self {
            id: e.id,
            kind: e.kind,
            actor_type: e.actor_type,
            actor_id: e.actor_id,
            payload: e
                .payload
                .and_then(|p| serde_json::from_str(&p).ok())
                .unwrap_or(serde_json::Value::Null),
            created_at: e.created_at,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketDetailDto {
    #[serde(flatten)]
    pub ticket: TicketDto,
    pub events: Vec<TicketEventDto>,
}

#[derive(Debug, Deserialize)]
pub struct ListTicketsQuery {
    pub state: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct ResolveTicketRequest {
    pub resolution_text: String,
}
