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
        // 取 thing_id 供记忆归物（transit 内会再做一次隔离取单）。
        let thing_id = self
            .db
            .get_ticket(workspace_id, ticket_id)
            .await
            .map_err(TicketError::internal)?
            .and_then(|t| t.thing_id);
        let title = self
            .db
            .get_ticket(workspace_id, ticket_id)
            .await
            .map_err(TicketError::internal)?
            .map(|t| t.title)
            .unwrap_or_default();
        let r = self
            .transit(workspace_id, ticket_id, "解决", |db, id| async move {
                db.resolve_ticket(id, user_id, &resolution).await
            })
            .await;
        // M2-c：解决成功 → 写入 agent_memories 知识层（best-effort，
        // 不阻断 resolve 主路径）。content = "{title}\n{resolution}"。
        if r.is_ok() {
            let input = tinyiothub_core::memory::MemoryInput {
                workspace_id: workspace_id.to_string(),
                agent_id: "default".to_string(),
                zone: tinyiothub_core::memory::MemoryZone::Work,
                content: format!("{title}\n{}", resolution_text.trim()),
                source: tinyiothub_core::memory::MemorySource::User,
                confidence: tinyiothub_core::memory::Confidence::High,
                tags: vec!["ticket-resolution".to_string(), format!("ticket:{ticket_id}")],
                supersedes: None,
                thing_id,
                snapshot_data: None,
                snapshot_time: None,
            };
            let store = tinyiothub_storage::memory::MemoryStore::new(self.db.pool().clone());
            if let Err(e) = store.put(input).await {
                tracing::warn!(ticket_id, error = %e, "resolution memory ingest failed");
            }
        }
        r
    }

    pub async fn close(&self, workspace_id: &str, ticket_id: i64, user_id: &str) -> TicketResult<()> {
        self.transit(workspace_id, ticket_id, "关闭", |db, id| async move {
            db.close_ticket(id, user_id).await
        })
        .await
    }

    pub async fn reopen(&self, workspace_id: &str, ticket_id: i64, user_id: &str) -> TicketResult<()> {
        let r = self
            .transit(workspace_id, ticket_id, "重新打开", |db, id| async move {
                db.reopen_ticket(id, user_id).await
            })
            .await;
        // M2-c：reopen = "上次修复未生效"——退役该工单的解法记忆，
        // 防止过期解法继续注入 prompt 误导 Agent。
        if r.is_ok()
            && let Err(e) = tinyiothub_storage::memory::MemoryStore::new(self.db.pool().clone())
                .delete_ticket_resolutions(workspace_id, ticket_id)
                .await
        {
            tracing::warn!(ticket_id, error = %e, "stale resolution memory retire failed");
        }
        r
    }

    pub async fn abandon(&self, workspace_id: &str, ticket_id: i64, user_id: &str) -> TicketResult<()> {
        self.transit(workspace_id, ticket_id, "放弃认领", |db, id| async move {
            db.abandon_ticket(id, user_id).await
        })
        .await
    }

    /// 工单对话 session 确保（M2-b）：已绑定直接返回；未绑定则建
    /// `agent:{ws}:default/ticket-{id}` 会话、播种 Agent 首条升级消息、
    /// 回写 tickets.session_key。订阅者开票时与 detail GET（懒回填 pre-M2
    /// 工单）共用。失败不致命——调用方降级为无对话面板。
    pub async fn ensure_ticket_session(db: &Db, ticket: &Ticket) -> Option<String> {
        if let Some(key) = &ticket.session_key {
            return Some(key.clone());
        }
        let key = format!("agent:{}:default/ticket-{}", ticket.workspace_id, ticket.id);
        let mut session =
            tinyiothub_storage::session::Session::new(key.clone(), ticket.workspace_id.clone(), "default".to_string());
        session.set_label(format!("工单 #{} {}", ticket.id, ticket.title));
        session.set_metadata("ticket_id", serde_json::json!(ticket.id));
        if let Err(e) = db.create_session(&session).await {
            tracing::warn!(ticket_id = ticket.id, error = %e, "ticket session create failed");
            return None;
        }
        // 种子消息：Agent 的升级说明（对话上下文的起点，也是 Agent 后续回答
        // 时的工单背景——会话记忆携带它）。
        let briefing: serde_json::Value = serde_json::from_str(&ticket.briefing).unwrap_or(serde_json::Value::Null);
        let problem = briefing
            .get("problem")
            .and_then(|p| p.as_str())
            .unwrap_or(&ticket.title);
        let first_line = problem.lines().next().unwrap_or(problem);
        let suggested = briefing
            .get("suggested_next_steps")
            .and_then(|s| s.as_array())
            .and_then(|a| a.first())
            .and_then(|v| v.as_str());
        let mut seed = format!("我已升级此工单。\n问题：{first_line}");
        if let Some(sg) = suggested {
            seed.push_str(&format!("\n我的建议：{sg}"));
        }
        seed.push_str("\n你可以在这里直接问我：查历史工单、生成维修指引、或让我换个方案再试。");
        if let Err(e) = db
            .append_session_message(&key, "assistant", &seed, &ticket.agent_run_id)
            .await
        {
            tracing::warn!(ticket_id = ticket.id, error = %e, "ticket session seed failed");
        }
        if let Err(e) = db.bind_ticket_session(ticket.id, &key).await {
            tracing::warn!(ticket_id = ticket.id, error = %e, "ticket session bind failed");
            return None;
        }
        Some(key)
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
