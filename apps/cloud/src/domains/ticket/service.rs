//! TicketService — 工单业务规则（校验 + TransitionOutcome → 语义错误）。

use std::sync::Arc;

use tinyiothub_storage::Db;
use tinyiothub_storage::ticket::{Ticket, TicketEvent, TransitionOutcome};

/// 领域错误：带 HTTP 语义（status, message）。
#[derive(Debug)]
pub struct TicketError {
    pub status: i32,
    pub message: String,
}

impl TicketError {
    fn conflict(state: &str, assignee_id: Option<&str>, action: &str) -> Self {
        let message = match (state, assignee_id) {
            ("claimed", Some(who)) => format!("已被 {who} 认领"),
            ("claimed", None) => "工单已被认领".to_string(),
            (s, _) => format!("工单当前状态为 {s}，无法{action}"),
        };
        Self { status: 409, message }
    }

    fn not_found() -> Self {
        Self {
            status: 404,
            message: "工单不存在".to_string(),
        }
    }

    fn internal(e: impl std::fmt::Display) -> Self {
        Self {
            status: 500,
            message: format!("工单操作失败: {e}"),
        }
    }
}

type TicketResult<T> = std::result::Result<T, TicketError>;

#[derive(Clone)]
pub struct TicketService {
    db: Arc<Db>,
}

impl TicketService {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    pub async fn list(
        &self,
        workspace_id: &str,
        state: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> TicketResult<(Vec<Ticket>, i64)> {
        let limit = page_size.min(100) as i64;
        let offset = ((page.max(1) - 1) * page_size.min(100)) as i64;
        let tickets = self
            .db
            .list_tickets(workspace_id, state, limit, offset)
            .await
            .map_err(TicketError::internal)?;
        let unclaimed = self
            .db
            .count_unclaimed_tickets(workspace_id)
            .await
            .map_err(TicketError::internal)?;
        Ok((tickets, unclaimed))
    }

    pub async fn detail(&self, workspace_id: &str, ticket_id: i64) -> TicketResult<(Ticket, Vec<TicketEvent>)> {
        let ticket = self
            .db
            .get_ticket(workspace_id, ticket_id)
            .await
            .map_err(TicketError::internal)?
            .ok_or_else(TicketError::not_found)?;
        let events = self
            .db
            .list_ticket_events(ticket_id)
            .await
            .map_err(TicketError::internal)?;
        Ok((ticket, events))
    }

    pub async fn claim(&self, workspace_id: &str, ticket_id: i64, user_id: &str) -> TicketResult<()> {
        self.transit(workspace_id, ticket_id, "认领", |db, id| async move {
            db.claim_ticket(id, user_id).await
        })
        .await
    }

    pub async fn start(&self, workspace_id: &str, ticket_id: i64, user_id: &str) -> TicketResult<()> {
        self.transit(workspace_id, ticket_id, "开始处理", |db, id| async move {
            db.start_ticket(id, user_id).await
        })
        .await
    }

    pub async fn resolve(
        &self,
        workspace_id: &str,
        ticket_id: i64,
        user_id: &str,
        resolution_text: &str,
    ) -> TicketResult<()> {
        // 设计契约：解决必填 resolution（知识闭环的入海口）。
        if resolution_text.trim().is_empty() {
            return Err(TicketError {
                status: 400,
                message: "必填：写清楚你实际怎么解决的".to_string(),
            });
        }
        let resolution = resolution_text.to_string();
        self.transit(workspace_id, ticket_id, "解决", |db, id| async move {
            db.resolve_ticket(id, user_id, &resolution).await
        })
        .await
    }

    pub async fn close(&self, workspace_id: &str, ticket_id: i64, user_id: &str) -> TicketResult<()> {
        self.transit(workspace_id, ticket_id, "关闭", |db, id| async move {
            db.close_ticket(id, user_id).await
        })
        .await
    }

    pub async fn abandon(&self, workspace_id: &str, ticket_id: i64, user_id: &str) -> TicketResult<()> {
        self.transit(workspace_id, ticket_id, "放弃认领", |db, id| async move {
            db.abandon_ticket(id, user_id).await
        })
        .await
    }

    /// 迁移统一骨架：先按 workspace 取工单（隔离），再走条件更新。
    async fn transit<F, Fut>(&self, workspace_id: &str, ticket_id: i64, action: &str, op: F) -> TicketResult<()>
    where
        F: FnOnce(Arc<Db>, i64) -> Fut,
        Fut: std::future::Future<Output = tinyiothub_storage::error::Result<TransitionOutcome>>,
    {
        // workspace 隔离：工单不属于本 workspace 即 404（不泄露存在性）。
        self.db
            .get_ticket(workspace_id, ticket_id)
            .await
            .map_err(TicketError::internal)?
            .ok_or_else(TicketError::not_found)?;
        match op(Arc::clone(&self.db), ticket_id)
            .await
            .map_err(TicketError::internal)?
        {
            TransitionOutcome::Done => Ok(()),
            TransitionOutcome::Conflict { state, assignee_id } => {
                Err(TicketError::conflict(&state, assignee_id.as_deref(), action))
            }
            TransitionOutcome::NotFound => Err(TicketError::not_found()),
        }
    }
}
