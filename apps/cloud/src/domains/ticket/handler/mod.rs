// Ticket HTTP handlers — 与 alarm 域 handler 同构。

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use serde::Serialize;
use tinyiothub_web::middleware::workspace::AuthClaims;
use tinyiothub_web::response::{ApiResponse, ApiResponseBuilder};

use crate::domains::ticket::dto::*;
use crate::domains::ticket::service::TicketService;
use crate::state::AppState;

pub fn create_ticket_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    AppState: axum::extract::FromRef<S>,
    std::sync::Arc<tinyiothub_authn::jwt::JwtService>: axum::extract::FromRef<S>,
{
    Router::new()
        .route("/", get(list_tickets))
        .route("/{id}", get(get_ticket))
        .route("/{id}/claim", post(claim_ticket))
        .route("/{id}/start", post(start_ticket))
        .route("/{id}/resolve", post(resolve_ticket))
        .route("/{id}/close", post(close_ticket))
        .route("/{id}/abandon", post(abandon_ticket))
}

#[derive(Serialize)]
struct TicketListPayload {
    tickets: Vec<TicketDto>,
    unclaimed_count: i64,
}

fn svc(state: &AppState) -> TicketService {
    TicketService::new(state.db.clone())
}

async fn list_tickets(
    Query(q): Query<ListTicketsQuery>,
    State(state): State<AppState>,
    claims: AuthClaims,
) -> Json<ApiResponse<TicketListPayload>> {
    match svc(&state)
        .list(
            &claims.0.workspace_id,
            q.state.as_deref(),
            q.page.unwrap_or(1),
            q.page_size.unwrap_or(20),
        )
        .await
    {
        Ok((tickets, unclaimed)) => ApiResponseBuilder::success(TicketListPayload {
            tickets: tickets.into_iter().map(TicketDto::from).collect(),
            unclaimed_count: unclaimed,
        }),
        Err(e) => ApiResponseBuilder::error_with_code(e.status, &e.message),
    }
}

async fn get_ticket(
    Path(id): Path<i64>,
    State(state): State<AppState>,
    claims: AuthClaims,
) -> Json<ApiResponse<TicketDetailDto>> {
    match svc(&state).detail(&claims.0.workspace_id, id).await {
        Ok((ticket, events)) => ApiResponseBuilder::success(TicketDetailDto {
            ticket: TicketDto::from(ticket),
            events: events.into_iter().map(TicketEventDto::from).collect(),
        }),
        Err(e) => ApiResponseBuilder::error_with_code(e.status, &e.message),
    }
}

macro_rules! transition_handler {
    ($name:ident, $method:ident) => {
        async fn $name(
            Path(id): Path<i64>,
            State(state): State<AppState>,
            claims: AuthClaims,
        ) -> Json<ApiResponse<()>> {
            match svc(&state)
                .$method(&claims.0.workspace_id, id, &claims.0.user_id)
                .await
            {
                Ok(()) => ApiResponseBuilder::success(()),
                Err(e) => ApiResponseBuilder::error_with_code(e.status, &e.message),
            }
        }
    };
}

transition_handler!(claim_ticket, claim);
transition_handler!(start_ticket, start);
transition_handler!(close_ticket, close);
transition_handler!(abandon_ticket, abandon);

async fn resolve_ticket(
    Path(id): Path<i64>,
    State(state): State<AppState>,
    claims: AuthClaims,
    Json(req): Json<ResolveTicketRequest>,
) -> Json<ApiResponse<()>> {
    match svc(&state)
        .resolve(&claims.0.workspace_id, id, &claims.0.user_id, &req.resolution_text)
        .await
    {
        Ok(()) => ApiResponseBuilder::success(()),
        Err(e) => ApiResponseBuilder::error_with_code(e.status, &e.message),
    }
}
