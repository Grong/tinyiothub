//! Tickets 持久化：Agent→人 升级原语（工单模块 M1）。
//!
//! 设计契约（设计文档 Eng Review Addendum）：
//! - 状态机：open → claimed → in_progress → resolved → closed；
//!   claimed → open（abandon，清 assignee/claimed_at）；open/resolved → closed。
//!   M1 无 reopen、无 system 自动关闭。全部迁移走条件更新防并发互撞：
//!   `UPDATE ... WHERE state='预期值'`，影响行数 0 = 冲突/非法态。
//! - 去重：`(COALESCE(thing_id,''), failure_hash)` 活跃态部分唯一索引；
//!   撞唯一索引 = 正常去重路径（转复发事件），非创建失败。
//! - 复发折叠：timer 周期触发的重复故障折叠为一条「又触发 N 次」事件
//!   （更新而非追加），payload 带最新 agent_run_id 供钻取。

use sqlx::SqlitePool;

use crate::database::Db;
use crate::error::Result;

// ──────────────────────────────────────────────
// 值类型
// ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketState {
    Open,
    Claimed,
    InProgress,
    Resolved,
    Closed,
}

impl TicketState {
    pub fn as_str(&self) -> &'static str {
        match self {
            TicketState::Open => "open",
            TicketState::Claimed => "claimed",
            TicketState::InProgress => "in_progress",
            TicketState::Resolved => "resolved",
            TicketState::Closed => "closed",
        }
    }

    pub fn from_db(s: &str) -> Option<Self> {
        Some(match s {
            "open" => TicketState::Open,
            "claimed" => TicketState::Claimed,
            "in_progress" => TicketState::InProgress,
            "resolved" => TicketState::Resolved,
            "closed" => TicketState::Closed,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct Ticket {
    pub id: i64,
    pub workspace_id: String,
    pub thing_id: Option<String>,
    pub agent_run_id: String,
    pub session_key: Option<String>,
    pub title: String,
    pub briefing: String,
    pub failure_hash: String,
    pub state: String,
    pub assignee_id: Option<String>,
    pub resolution_text: Option<String>,
    pub created_at: String,
    pub claimed_at: Option<String>,
    pub resolved_at: Option<String>,
    pub closed_at: Option<String>,
    pub reopened_at: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct TicketEvent {
    pub id: i64,
    pub ticket_id: i64,
    pub kind: String,
    pub actor_type: String,
    pub actor_id: Option<String>,
    pub payload: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TicketStatistics {
    pub tickets_total: i64,
    pub runs_total: i64,
    pub escalation_rate: f64,
    pub open: i64,
    pub claimed: i64,
    pub in_progress: i64,
    pub resolved: i64,
    pub closed: i64,
    pub avg_time_to_ack_secs: Option<f64>,
    pub avg_time_to_resolve_secs: Option<f64>,
}

/// 开票入参（订阅者从 RunReport 装配）。
pub struct NewTicket {
    pub workspace_id: String,
    pub thing_id: Option<String>,
    pub agent_run_id: String,
    pub title: String,
    pub briefing: String, // JSON（CHECK json_valid）
    pub failure_hash: String,
}

pub enum CreateOutcome {
    Created(i64),
    /// 活跃工单已存在（去重命中）——调用方转复发事件路径。
    Duplicate(i64),
}

/// 条件更新的三种结果：成功 / 状态冲突（带当前态与当前指派人）/ 不存在。
pub enum TransitionOutcome {
    Done,
    Conflict { state: String, assignee_id: Option<String> },
    NotFound,
}

// ──────────────────────────────────────────────
// 持久化函数（pool 参数）
// ──────────────────────────────────────────────

pub(crate) async fn insert_ticket(pool: &SqlitePool, t: &NewTicket) -> Result<CreateOutcome> {
    let r = sqlx::query(
        "INSERT INTO tickets (workspace_id, thing_id, agent_run_id, title, briefing, failure_hash)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&t.workspace_id)
    .bind(&t.thing_id)
    .bind(&t.agent_run_id)
    .bind(&t.title)
    .bind(&t.briefing)
    .bind(&t.failure_hash)
    .execute(pool)
    .await;
    match r {
        Ok(done) => Ok(CreateOutcome::Created(done.last_insert_rowid())),
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            let existing: Option<(i64,)> = sqlx::query_as(
                "SELECT id FROM tickets
                 WHERE COALESCE(thing_id, '') = COALESCE(?, '')
                   AND failure_hash = ?
                   AND state IN ('open','claimed','in_progress')",
            )
            .bind(&t.thing_id)
            .bind(&t.failure_hash)
            .fetch_optional(pool)
            .await?;
            match existing {
                Some((id,)) => Ok(CreateOutcome::Duplicate(id)),
                // 唯一冲突但查不到活跃行（理论上不应发生）——按原错误上报，
                // 由调用方落 DLQ（创建失败必须可见）。
                None => Err(sqlx::Error::Database(e).into()),
            }
        }
        Err(e) => Err(e.into()),
    }
}

/// 复发折叠（T5）：同一张工单的「又触发 N 次」事件只有一条，重复触发时
/// 更新计数与最新 run_id；首次复发才插入。
pub(crate) async fn record_recurrence(pool: &SqlitePool, ticket_id: i64, agent_run_id: &str) -> Result<()> {
    let existing: Option<(i64, String)> = sqlx::query_as(
        "SELECT id, payload FROM ticket_events
         WHERE ticket_id = ? AND kind = 'system' AND json_extract(payload, '$.kind') = 'recurrence'
         ORDER BY id DESC LIMIT 1",
    )
    .bind(ticket_id)
    .fetch_optional(pool)
    .await?;
    match existing {
        Some((event_id, payload)) => {
            let count = serde_json::from_str::<serde_json::Value>(&payload)
                .ok()
                .and_then(|v| v.get("count").and_then(|c| c.as_i64()))
                .unwrap_or(1)
                + 1;
            let new_payload = serde_json::json!({
                "kind": "recurrence",
                "count": count,
                "agent_run_id": agent_run_id,
            });
            sqlx::query("UPDATE ticket_events SET payload = ?, created_at = datetime('now') WHERE id = ?")
                .bind(new_payload.to_string())
                .bind(event_id)
                .execute(pool)
                .await?;
        }
        None => {
            let payload = serde_json::json!({
                "kind": "recurrence",
                "count": 1,
                "agent_run_id": agent_run_id,
            });
            sqlx::query(
                "INSERT INTO ticket_events (ticket_id, kind, actor_type, actor_id, payload)
                 VALUES (?, 'system', 'agent', ?, ?)",
            )
            .bind(ticket_id)
            .bind(agent_run_id)
            .bind(payload.to_string())
            .execute(pool)
            .await?;
        }
    }
    Ok(())
}

async fn current_state(pool: &SqlitePool, ticket_id: i64) -> Result<Option<(String, Option<String>)>> {
    let row: Option<(String, Option<String>)> = sqlx::query_as("SELECT state, assignee_id FROM tickets WHERE id = ?")
        .bind(ticket_id)
        .fetch_optional(pool)
        .await?;
    Ok(row)
}

async fn record_state_event(pool: &SqlitePool, ticket_id: i64, actor_id: &str, from: &str, to: &str) -> Result<()> {
    let payload = serde_json::json!({"from": from, "to": to});
    sqlx::query(
        "INSERT INTO ticket_events (ticket_id, kind, actor_type, actor_id, payload)
         VALUES (?, 'state_change', 'user', ?, ?)",
    )
    .bind(ticket_id)
    .bind(actor_id)
    .bind(payload.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

/// 条件更新骨架：expected → 字段集。影响行数 0 时读当前态区分冲突/不存在。
/// set_sql 中的占位符用 `?` 并经 set_binds 绑定（绝不字符串插值用户输入）。
async fn transition(
    pool: &SqlitePool,
    ticket_id: i64,
    actor_id: &str,
    expected: &[&str],
    set_sql: &str,
    set_binds: &[&str],
    to: TicketState,
) -> Result<TransitionOutcome> {
    let placeholders = expected.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let set_clause = if set_sql.is_empty() {
        String::new()
    } else {
        format!("{set_sql}, ")
    };
    let sql = format!("UPDATE tickets SET {set_clause}state = ? WHERE id = ? AND state IN ({placeholders})");
    let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
    for b in set_binds {
        q = q.bind(*b);
    }
    q = q.bind(to.as_str()).bind(ticket_id);
    for e in expected {
        q = q.bind(*e);
    }
    let done = q.execute(pool).await?;
    if done.rows_affected() == 1 {
        let from = expected[0];
        record_state_event(pool, ticket_id, actor_id, from, to.as_str()).await?;
        return Ok(TransitionOutcome::Done);
    }
    match current_state(pool, ticket_id).await? {
        Some((state, assignee_id)) => Ok(TransitionOutcome::Conflict { state, assignee_id }),
        None => Ok(TransitionOutcome::NotFound),
    }
}

// ──────────────────────────────────────────────
// Db 门面委托
// ──────────────────────────────────────────────

impl Db {
    pub async fn create_ticket(&self, t: &NewTicket) -> Result<CreateOutcome> {
        insert_ticket(self.pool(), t).await
    }

    pub async fn ticket_recurrence(&self, ticket_id: i64, agent_run_id: &str) -> Result<()> {
        record_recurrence(self.pool(), ticket_id, agent_run_id).await
    }

    /// open → claimed。被抢时返回 Conflict（带当前指派人）。
    pub async fn claim_ticket(&self, ticket_id: i64, user_id: &str) -> Result<TransitionOutcome> {
        transition(
            self.pool(),
            ticket_id,
            user_id,
            &["open"],
            "assignee_id = ?, claimed_at = datetime('now')",
            &[user_id],
            TicketState::Claimed,
        )
        .await
    }

    /// claimed → in_progress。
    pub async fn start_ticket(&self, ticket_id: i64, user_id: &str) -> Result<TransitionOutcome> {
        transition(
            self.pool(),
            ticket_id,
            user_id,
            &["claimed"],
            "",
            &[],
            TicketState::InProgress,
        )
        .await
    }

    /// in_progress → resolved（resolution 非空由 domain service 校验）。
    pub async fn resolve_ticket(&self, ticket_id: i64, user_id: &str, resolution: &str) -> Result<TransitionOutcome> {
        let pool = self.pool();
        let done = sqlx::query(
            "UPDATE tickets SET state = 'resolved', resolution_text = ?, resolved_at = datetime('now')
             WHERE id = ? AND state = 'in_progress'",
        )
        .bind(resolution)
        .bind(ticket_id)
        .execute(pool)
        .await?;
        if done.rows_affected() == 1 {
            record_state_event(pool, ticket_id, user_id, "in_progress", "resolved").await?;
            return Ok(TransitionOutcome::Done);
        }
        match current_state(pool, ticket_id).await? {
            Some((state, assignee_id)) => Ok(TransitionOutcome::Conflict { state, assignee_id }),
            None => Ok(TransitionOutcome::NotFound),
        }
    }

    /// resolved → open（M2 reopen）：原 resolution 快照进 ticket_events，
    /// 清 resolution_text/resolved_at/assignee/claimed_at，置 reopened_at。
    /// 由调用方（domain service）负责重新 SSE 通知。
    pub async fn reopen_ticket(&self, ticket_id: i64, user_id: &str) -> Result<TransitionOutcome> {
        let pool = self.pool();
        // 先取原 resolution（条件更新成功才快照进事件）
        let prior: Option<(String, Option<String>)> =
            sqlx::query_as("SELECT resolution_text, assignee_id FROM tickets WHERE id = ? AND state = 'resolved'")
                .bind(ticket_id)
                .fetch_optional(pool)
                .await?;
        let Some((prior_resolution, _)) = prior else {
            return match current_state(pool, ticket_id).await? {
                Some((state, assignee_id)) => Ok(TransitionOutcome::Conflict { state, assignee_id }),
                None => Ok(TransitionOutcome::NotFound),
            };
        };
        let done = sqlx::query(
            "UPDATE tickets SET state = 'open', resolution_text = NULL, resolved_at = NULL,
                    assignee_id = NULL, claimed_at = NULL, reopened_at = datetime('now')
             WHERE id = ? AND state = 'resolved'",
        )
        .bind(ticket_id)
        .execute(pool)
        .await?;
        if done.rows_affected() != 1 {
            return match current_state(pool, ticket_id).await? {
                Some((state, assignee_id)) => Ok(TransitionOutcome::Conflict { state, assignee_id }),
                None => Ok(TransitionOutcome::NotFound),
            };
        }
        let payload = serde_json::json!({
            "from": "resolved", "to": "open", "prior_resolution": prior_resolution,
        });
        sqlx::query(
            "INSERT INTO ticket_events (ticket_id, kind, actor_type, actor_id, payload)
             VALUES (?, 'state_change', 'user', ?, ?)",
        )
        .bind(ticket_id)
        .bind(user_id)
        .bind(payload.to_string())
        .execute(pool)
        .await?;
        Ok(TransitionOutcome::Done)
    }

    /// open|resolved → closed（无效工单关闭 / 确认关闭；M1 无 system 自动关闭）。
    pub async fn close_ticket(&self, ticket_id: i64, user_id: &str) -> Result<TransitionOutcome> {
        let pool = self.pool();
        let done = sqlx::query(
            "UPDATE tickets SET state = 'closed', closed_at = datetime('now')
             WHERE id = ? AND state IN ('open','resolved')",
        )
        .bind(ticket_id)
        .execute(pool)
        .await?;
        if done.rows_affected() == 1 {
            record_state_event(pool, ticket_id, user_id, "open|resolved", "closed").await?;
            return Ok(TransitionOutcome::Done);
        }
        match current_state(pool, ticket_id).await? {
            Some((state, assignee_id)) => Ok(TransitionOutcome::Conflict { state, assignee_id }),
            None => Ok(TransitionOutcome::NotFound),
        }
    }

    /// claimed → open（放弃认领：清 assignee_id 与 claimed_at）。
    pub async fn abandon_ticket(&self, ticket_id: i64, user_id: &str) -> Result<TransitionOutcome> {
        transition(
            self.pool(),
            ticket_id,
            user_id,
            &["claimed"],
            "assignee_id = NULL, claimed_at = NULL",
            &[],
            TicketState::Open,
        )
        .await
    }

    /// 绑定工单对话 session（M2）：只在未绑定时写入（幂等）；
    /// session_key UNIQUE 冲突即已被绑定，视为成功。
    pub async fn bind_ticket_session(&self, ticket_id: i64, session_key: &str) -> Result<()> {
        sqlx::query("UPDATE tickets SET session_key = ? WHERE id = ? AND session_key IS NULL")
            .bind(session_key)
            .bind(ticket_id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn get_ticket(&self, workspace_id: &str, ticket_id: i64) -> Result<Option<Ticket>> {
        let t = sqlx::query_as::<_, Ticket>("SELECT * FROM tickets WHERE id = ? AND workspace_id = ?")
            .bind(ticket_id)
            .bind(workspace_id)
            .fetch_optional(self.pool())
            .await?;
        Ok(t)
    }

    /// 列表页：state 为空则全部（closed 默认排除，除非显式要）；按 created_at 倒序。
    pub async fn list_tickets(
        &self,
        workspace_id: &str,
        state: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Ticket>> {
        let rows = match state {
            Some(s) => {
                sqlx::query_as::<_, Ticket>(
                    "SELECT * FROM tickets WHERE workspace_id = ? AND state = ?
                     ORDER BY created_at DESC LIMIT ? OFFSET ?",
                )
                .bind(workspace_id)
                .bind(s)
                .bind(limit)
                .bind(offset)
                .fetch_all(self.pool())
                .await?
            }
            None => {
                sqlx::query_as::<_, Ticket>(
                    "SELECT * FROM tickets WHERE workspace_id = ? AND state != 'closed'
                     ORDER BY created_at DESC LIMIT ? OFFSET ?",
                )
                .bind(workspace_id)
                .bind(limit)
                .bind(offset)
                .fetch_all(self.pool())
                .await?
            }
        };
        Ok(rows)
    }

    /// 未认领计数 badge（open 状态数）。
    pub async fn count_unclaimed_tickets(&self, workspace_id: &str) -> Result<i64> {
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tickets WHERE workspace_id = ? AND state = 'open'")
            .bind(workspace_id)
            .fetch_one(self.pool())
            .await?;
        Ok(n)
    }

    pub async fn list_ticket_events(&self, ticket_id: i64) -> Result<Vec<TicketEvent>> {
        let rows =
            sqlx::query_as::<_, TicketEvent>("SELECT * FROM ticket_events WHERE ticket_id = ? ORDER BY created_at, id")
                .bind(ticket_id)
                .fetch_all(self.pool())
                .await?;
        Ok(rows)
    }

    /// `<ticket_resolutions>` 注入源（T7）：同 thing 的已解决工单在前，
    /// workspace 最近补足；只取有 resolution_text 的。
    pub async fn recent_ticket_resolutions(
        &self,
        workspace_id: &str,
        thing_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<(String, String)>> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT title, resolution_text FROM tickets
             WHERE workspace_id = ? AND state IN ('resolved','closed') AND resolution_text IS NOT NULL
             ORDER BY (CASE WHEN thing_id = ? THEN 0 ELSE 1 END), resolved_at DESC
             LIMIT ?",
        )
        .bind(workspace_id)
        .bind(thing_id)
        .bind(limit)
        .fetch_all(self.pool())
        .await?;
        Ok(rows)
    }

    /// 升级指标（M2-d）：升级率 = 工单数 / agent run 总数；
    /// time-to-ack / time-to-resolve 均值（秒，julianday 差）。
    /// 只度量不告警（设计契约 Success Criteria #3）。
    pub async fn ticket_statistics(&self, workspace_id: &str) -> Result<TicketStatistics> {
        let (tickets_total, open_n, claimed_n, in_progress_n, resolved_n, closed_n, avg_ack, avg_resolve): (
            i64,
            i64,
            i64,
            i64,
            i64,
            i64,
            Option<f64>,
            Option<f64>,
        ) = sqlx::query_as(
            "SELECT COUNT(*),
                    SUM(CASE WHEN state = 'open' THEN 1 ELSE 0 END),
                    SUM(CASE WHEN state = 'claimed' THEN 1 ELSE 0 END),
                    SUM(CASE WHEN state = 'in_progress' THEN 1 ELSE 0 END),
                    SUM(CASE WHEN state = 'resolved' THEN 1 ELSE 0 END),
                    SUM(CASE WHEN state = 'closed' THEN 1 ELSE 0 END),
                    AVG((julianday(claimed_at) - julianday(created_at)) * 86400.0)
                        FILTER (WHERE claimed_at IS NOT NULL),
                    AVG((julianday(resolved_at) - julianday(created_at)) * 86400.0)
                        FILTER (WHERE resolved_at IS NOT NULL)
             FROM tickets WHERE workspace_id = ?",
        )
        .bind(workspace_id)
        .fetch_one(self.pool())
        .await?;
        let (runs_total,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM agent_runs WHERE workspace_id = ?")
            .bind(workspace_id)
            .fetch_one(self.pool())
            .await?;
        Ok(TicketStatistics {
            tickets_total,
            runs_total,
            escalation_rate: if runs_total > 0 {
                tickets_total as f64 / runs_total as f64
            } else {
                0.0
            },
            open: open_n,
            claimed: claimed_n,
            in_progress: in_progress_n,
            resolved: resolved_n,
            closed: closed_n,
            avg_time_to_ack_secs: avg_ack,
            avg_time_to_resolve_secs: avg_resolve,
        })
    }

    /// 对账扫描（T5）：outcome∈触发集 且尚无工单的 run（复发已在去重路径
    /// 覆盖——重放这些 run 只会再撞一次唯一索引转追加，幂等）。
    pub async fn unticketed_failed_runs(&self, since: &str) -> Result<Vec<tinyiothub_core::agent_runs::RunReport>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT report FROM agent_runs
             WHERE outcome IN ('failed','budget_exceeded','rejected')
               AND created_at > ?
               AND id NOT IN (SELECT agent_run_id FROM tickets)",
        )
        .bind(since)
        .fetch_all(self.pool())
        .await?;
        let mut out = Vec::with_capacity(rows.len());
        for (json,) in rows {
            if let Ok(report) = serde_json::from_str(&json) {
                out.push(report);
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Db;

    async fn test_db() -> Db {
        Db::new(crate::test_helpers::test_pool().await)
    }

    fn new_ticket(ws: &str, thing: Option<&str>, run: &str, hash: &str) -> NewTicket {
        NewTicket {
            workspace_id: ws.to_string(),
            thing_id: thing.map(str::to_string),
            agent_run_id: run.to_string(),
            title: format!("故障工单 {run}"),
            briefing: serde_json::json!({
                "problem": "泵异响",
                "steps_attempted": [{"action": "reboot", "result": "无效"}],
                "last_error": "E-401",
                "failure_kind": "policy",
                "suggested_next_steps": ["现场检查"],
            })
            .to_string(),
            failure_hash: hash.to_string(),
        }
    }

    #[tokio::test]
    async fn create_then_get_and_list() {
        let db = test_db().await;
        let CreateOutcome::Created(id) = db
            .create_ticket(&new_ticket("ws1", Some("pump3"), "run_1", "h1"))
            .await
            .unwrap()
        else {
            panic!("expected Created");
        };
        let t = db.get_ticket("ws1", id).await.unwrap().expect("ticket exists");
        assert_eq!(t.state, "open");
        assert_eq!(t.thing_id.as_deref(), Some("pump3"));
        // workspace 隔离
        assert!(db.get_ticket("ws2", id).await.unwrap().is_none());
        let list = db.list_tickets("ws1", None, 20, 0).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(db.count_unclaimed_tickets("ws1").await.unwrap(), 1);
    }

    #[tokio::test]
    async fn active_dedup_duplicate_returns_existing() {
        let db = test_db().await;
        let CreateOutcome::Created(id) = db
            .create_ticket(&new_ticket("ws1", Some("pump3"), "run_1", "h1"))
            .await
            .unwrap()
        else {
            panic!("expected Created");
        };
        // 同一故障复发 → Duplicate（不开新票）
        let CreateOutcome::Duplicate(dup_id) = db
            .create_ticket(&new_ticket("ws1", Some("pump3"), "run_2", "h1"))
            .await
            .unwrap()
        else {
            panic!("expected Duplicate");
        };
        assert_eq!(dup_id, id);
        assert_eq!(db.list_tickets("ws1", None, 20, 0).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn resolved_recurrence_opens_new_ticket() {
        let db = test_db().await;
        let CreateOutcome::Created(id) = db
            .create_ticket(&new_ticket("ws1", Some("pump3"), "run_1", "h1"))
            .await
            .unwrap()
        else {
            panic!("expected Created");
        };
        db.claim_ticket(id, "u1").await.unwrap();
        db.start_ticket(id, "u1").await.unwrap();
        db.resolve_ticket(id, "u1", "更换轴承").await.unwrap();
        // resolved 后复发 → 新工单（"上次修复未生效"信号，设计契约）
        let CreateOutcome::Created(new_id) = db
            .create_ticket(&new_ticket("ws1", Some("pump3"), "run_2", "h1"))
            .await
            .unwrap()
        else {
            panic!("expected new ticket after resolved");
        };
        assert_ne!(new_id, id);
    }

    #[tokio::test]
    async fn null_thing_id_dedup_works() {
        let db = test_db().await;
        db.create_ticket(&new_ticket("ws1", None, "run_1", "h_ws"))
            .await
            .unwrap();
        // thing_id=NULL 行也参与去重（COALESCE 索引）
        let CreateOutcome::Duplicate(_) = db
            .create_ticket(&new_ticket("ws1", None, "run_2", "h_ws"))
            .await
            .unwrap()
        else {
            panic!("expected Duplicate for NULL thing_id");
        };
    }

    #[tokio::test]
    async fn recurrence_folds_into_one_event() {
        let db = test_db().await;
        let CreateOutcome::Created(id) = db
            .create_ticket(&new_ticket("ws1", Some("pump3"), "run_1", "h1"))
            .await
            .unwrap()
        else {
            panic!("expected Created");
        };
        db.ticket_recurrence(id, "run_2").await.unwrap();
        db.ticket_recurrence(id, "run_3").await.unwrap();
        db.ticket_recurrence(id, "run_4").await.unwrap();
        let events = db.list_ticket_events(id).await.unwrap();
        assert_eq!(events.len(), 1, "复发折叠为一条事件");
        let payload: serde_json::Value = serde_json::from_str(events[0].payload.as_ref().unwrap()).unwrap();
        assert_eq!(payload["count"], 3);
        assert_eq!(payload["agent_run_id"], "run_4", "保留最新 run_id 供钻取");
    }

    #[tokio::test]
    async fn claim_race_second_loses() {
        let db = test_db().await;
        let CreateOutcome::Created(id) = db.create_ticket(&new_ticket("ws1", None, "run_1", "h1")).await.unwrap()
        else {
            panic!("expected Created");
        };
        let first = db.claim_ticket(id, "u1").await.unwrap();
        assert!(matches!(first, TransitionOutcome::Done));
        let second = db.claim_ticket(id, "u2").await.unwrap();
        match second {
            TransitionOutcome::Conflict { state, assignee_id } => {
                assert_eq!(state, "claimed");
                assert_eq!(assignee_id.as_deref(), Some("u1"));
            }
            _ => panic!("expected Conflict"),
        }
        assert_eq!(db.count_unclaimed_tickets("ws1").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn state_machine_illegal_transitions_conflict() {
        let db = test_db().await;
        let CreateOutcome::Created(id) = db.create_ticket(&new_ticket("ws1", None, "run_1", "h1")).await.unwrap()
        else {
            panic!("expected Created");
        };
        // open → resolve 非法（必须经 claimed → in_progress）
        assert!(matches!(
            db.resolve_ticket(id, "u1", "x").await.unwrap(),
            TransitionOutcome::Conflict { .. }
        ));
        // open → start 非法
        assert!(matches!(
            db.start_ticket(id, "u1").await.unwrap(),
            TransitionOutcome::Conflict { .. }
        ));
        // 合法链：claim → start → resolve → close
        db.claim_ticket(id, "u1").await.unwrap();
        db.start_ticket(id, "u1").await.unwrap();
        db.resolve_ticket(id, "u1", "现场更换轴承 NSK-6205").await.unwrap();
        db.close_ticket(id, "u1").await.unwrap();
        let t = db.get_ticket("ws1", id).await.unwrap().unwrap();
        assert_eq!(t.state, "closed");
        assert!(t.resolved_at.is_some() && t.closed_at.is_some());
        // closed 后一切迁移冲突
        assert!(matches!(
            db.claim_ticket(id, "u2").await.unwrap(),
            TransitionOutcome::Conflict { .. }
        ));
        // 状态事件轨迹完整（claim/start/resolve/close 四条）
        let events = db.list_ticket_events(id).await.unwrap();
        assert_eq!(events.iter().filter(|e| e.kind == "state_change").count(), 4);
    }

    #[tokio::test]
    async fn abandon_clears_assignee_and_claimed_at() {
        let db = test_db().await;
        let CreateOutcome::Created(id) = db.create_ticket(&new_ticket("ws1", None, "run_1", "h1")).await.unwrap()
        else {
            panic!("expected Created");
        };
        db.claim_ticket(id, "u1").await.unwrap();
        db.abandon_ticket(id, "u1").await.unwrap();
        let t = db.get_ticket("ws1", id).await.unwrap().unwrap();
        assert_eq!(t.state, "open");
        assert!(t.assignee_id.is_none() && t.claimed_at.is_none());
        // 放弃后他人可认领
        assert!(matches!(
            db.claim_ticket(id, "u2").await.unwrap(),
            TransitionOutcome::Done
        ));
    }

    #[tokio::test]
    async fn recent_resolutions_same_thing_first() {
        let db = test_db().await;
        // 先解决一张 thing=A 的，再解决一张 thing=B 的（B 更新）
        for (thing, run, hash) in [("A", "run_a", "ha"), ("B", "run_b", "hb")] {
            let CreateOutcome::Created(id) = db
                .create_ticket(&new_ticket("ws1", Some(thing), run, hash))
                .await
                .unwrap()
            else {
                panic!("expected Created");
            };
            db.claim_ticket(id, "u1").await.unwrap();
            db.start_ticket(id, "u1").await.unwrap();
            db.resolve_ticket(id, "u1", &format!("修好了 {thing}")).await.unwrap();
        }
        let rows = db.recent_ticket_resolutions("ws1", Some("A"), 5).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].1, "修好了 A", "同 thing 优先，即使更旧");
        assert_eq!(rows[1].1, "修好了 B");
    }

    #[tokio::test]
    async fn unticketed_failed_runs_scans_agent_runs() {
        let db = test_db().await;
        use tinyiothub_core::agent_runs::{ActionRecord, ActionResult, Outcome, RunReport};
        let report = RunReport {
            run_id: "run_failed".to_string(),
            workspace_id: "ws1".to_string(),
            trigger: "thing:t1:event:x".to_string(),
            outcome: Outcome::Rejected,
            summary: "全拒".to_string(),
            actions: vec![ActionRecord {
                thing_id: "t1".to_string(),
                action_name: "reboot".to_string(),
                params: serde_json::Value::Null,
                result: ActionResult::Failed("denied".to_string()),
                verified: false,
            }],
            verified: false,
            duration_ms: 100,
            tool_calls: 1,
            tokens: 10,
            end_reason: Some(tinyiothub_core::agent_runs::EndReason::Policy),
            thing_id: Some("t1".to_string()),
        };
        db.insert_agent_run(&report, None, None).await.unwrap();
        let unticketed = db.unticketed_failed_runs("2000-01-01").await.unwrap();
        assert_eq!(unticketed.len(), 1);
        // 开票后不再出现在对账扫描里
        db.create_ticket(&new_ticket("ws1", Some("t1"), "run_failed", "h1"))
            .await
            .unwrap();
        assert!(db.unticketed_failed_runs("2000-01-01").await.unwrap().is_empty());
    }
}

#[cfg(test)]
mod reopen_tests {
    use super::*;
    use crate::database::Db;

    async fn test_db() -> Db {
        Db::new(crate::test_helpers::test_pool().await)
    }

    fn ticket(ws: &str, run: &str, hash: &str) -> NewTicket {
        NewTicket {
            workspace_id: ws.to_string(),
            thing_id: Some("t1".to_string()),
            agent_run_id: run.to_string(),
            title: "t".to_string(),
            briefing: "{}".to_string(),
            failure_hash: hash.to_string(),
        }
    }

    #[tokio::test]
    async fn reopen_resolved_resets_to_open_and_snapshots_resolution() {
        let db = test_db().await;
        let CreateOutcome::Created(id) = db.create_ticket(&ticket("ws1", "r1", "h1")).await.unwrap() else {
            panic!("expected Created");
        };
        db.claim_ticket(id, "u1").await.unwrap();
        db.start_ticket(id, "u1").await.unwrap();
        db.resolve_ticket(id, "u1", "换了轴承但没修好").await.unwrap();

        // 非 resolved 状态 reopen → Conflict
        let CreateOutcome::Created(other) = db.create_ticket(&ticket("ws1", "r2", "h2")).await.unwrap() else {
            panic!("expected Created");
        };
        assert!(matches!(
            db.reopen_ticket(other, "u1").await.unwrap(),
            TransitionOutcome::Conflict { .. }
        ));

        db.reopen_ticket(id, "u1").await.unwrap();
        let t = db.get_ticket("ws1", id).await.unwrap().unwrap();
        assert_eq!(t.state, "open");
        assert!(t.resolution_text.is_none(), "resolution 快照进事件后清列");
        assert!(t.resolved_at.is_none() && t.assignee_id.is_none() && t.claimed_at.is_none());
        assert!(t.reopened_at.is_some());

        // 原 resolution 保留在事件里
        let events = db.list_ticket_events(id).await.unwrap();
        let reopen_ev = events
            .iter()
            .rev()
            .find(|e| e.kind == "state_change")
            .expect("reopen event");
        let payload: serde_json::Value = serde_json::from_str(reopen_ev.payload.as_ref().unwrap()).unwrap();
        assert_eq!(payload["from"], "resolved");
        assert_eq!(payload["to"], "open");
        assert_eq!(payload["prior_resolution"], "换了轴承但没修好");

        // reopen 后 active 去重窗口重新覆盖该故障（同 hash 再复发 → Duplicate）
        let CreateOutcome::Duplicate(dup) = db
            .create_ticket(&NewTicket {
                workspace_id: "ws1".to_string(),
                thing_id: Some("t1".to_string()),
                agent_run_id: "r3".to_string(),
                title: "t".to_string(),
                briefing: "{}".to_string(),
                failure_hash: "h1".to_string(),
            })
            .await
            .unwrap()
        else {
            panic!("expected Duplicate after reopen");
        };
        assert_eq!(dup, id);
    }
}

#[cfg(test)]
mod statistics_tests {
    use super::*;
    use crate::database::Db;

    async fn test_db() -> Db {
        Db::new(crate::test_helpers::test_pool().await)
    }

    #[tokio::test]
    async fn statistics_counts_rates_and_durations() {
        let db = test_db().await;
        let t = |run: &str, hash: &str| NewTicket {
            workspace_id: "ws1".to_string(),
            thing_id: Some("t1".to_string()),
            agent_run_id: run.to_string(),
            title: "t".to_string(),
            briefing: "{}".to_string(),
            failure_hash: hash.to_string(),
        };
        let CreateOutcome::Created(id1) = db.create_ticket(&t("r1", "h1")).await.unwrap() else {
            panic!("expected Created");
        };
        let CreateOutcome::Created(_id2) = db.create_ticket(&t("r2", "h2")).await.unwrap() else {
            panic!("expected Created");
        };
        db.claim_ticket(id1, "u1").await.unwrap();
        db.start_ticket(id1, "u1").await.unwrap();
        db.resolve_ticket(id1, "u1", "修好了").await.unwrap();

        // 两条 agent_runs（触发集 outcome）
        for rid in ["r1", "r2"] {
            let report = tinyiothub_core::agent_runs::RunReport {
                run_id: rid.to_string(),
                workspace_id: "ws1".to_string(),
                trigger: "timer:ws1".to_string(),
                outcome: tinyiothub_core::agent_runs::Outcome::Rejected,
                summary: "s".to_string(),
                actions: vec![],
                verified: false,
                duration_ms: 1,
                tool_calls: 0,
                tokens: 0,
                end_reason: Some(tinyiothub_core::agent_runs::EndReason::Policy),
                thing_id: None,
            };
            db.insert_agent_run(&report, None, None).await.unwrap();
        }

        let stats = db.ticket_statistics("ws1").await.unwrap();
        assert_eq!(stats.tickets_total, 2);
        assert_eq!(stats.runs_total, 2);
        assert!((stats.escalation_rate - 1.0).abs() < 1e-9);
        assert_eq!(stats.resolved, 1);
        assert_eq!(stats.open, 1);
        assert!(stats.avg_time_to_ack_secs.is_some());
        assert!(stats.avg_time_to_resolve_secs.is_some());
        // workspace 隔离
        let empty = db.ticket_statistics("ws2").await.unwrap();
        assert_eq!(empty.tickets_total, 0);
        assert_eq!(empty.escalation_rate, 0.0);
    }
}
