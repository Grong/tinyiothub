//! T4 统一升级入口（D6）：所有"转工单"路径的唯一创建函数。
//!
//! 五路调用方共用：run 失败（ticket_subscriber）、judgment=needs_human、
//! 审批拒绝、审批 24h 超时（cron）、Critical/Error 报警直达。
//!
//! 去重语义：撞 tickets_active_dedup 部分唯一索引 → 折叠为既有工单的
//! recurrence 事件，不开新票。failure_hash 四源规则的真源在
//! `ticket_subscriber::failure_hash`；报警/judgment 路径统一用
//! `pk:{problem_key}`（problem_key = alarm:{thing_id}:{rule_id}）。

use std::sync::Arc;

use tinyiothub_storage::Db;
use tinyiothub_storage::ticket::{CreateOutcome, NewTicket};
use tracing::{debug, error, info};

use crate::domains::event::sse_manager::SseConnectionManager;
use crate::domains::notify::channels::sse_channel::SseMessage;

/// 一次升级的参数。failure_hash 由调用方按四源规则计算（报警路径：
/// `format!("pk:alarm:{{thing_id}}:{{rule_id}}")`）。
pub struct Escalation {
    pub workspace_id: String,
    pub thing_id: Option<String>,
    /// NewTicket.agent_run_id NOT NULL；无 run 的直达路径用 "alarm:{id}" 占位。
    pub agent_run_id: String,
    pub title: String,
    pub briefing: serde_json::Value,
    pub failure_hash: String,
}

/// 唯一工单创建入口。返回 ticket_id（Duplicate 时为既有工单）；失败返回 None
///（错误已记录；run 失败路径的 DLQ 兜底留在 ticket_subscriber）。
pub async fn create_escalation(db: &Db, sse: &SseConnectionManager, esc: Escalation) -> Option<i64> {
    let new_ticket = NewTicket {
        workspace_id: esc.workspace_id.clone(),
        thing_id: esc.thing_id.clone(),
        agent_run_id: esc.agent_run_id.clone(),
        title: esc.title.clone(),
        briefing: esc.briefing.to_string(),
        failure_hash: esc.failure_hash.clone(),
    };
    match db.create_ticket(&new_ticket).await {
        Ok(CreateOutcome::Created(id)) => {
            info!(ticket_id = id, workspace_id = %esc.workspace_id, "escalation ticket created");
            // 建工单对话 session（best-effort，失败不影响开票）
            if let Ok(Some(ticket)) = db.get_ticket(&esc.workspace_id, id).await {
                crate::domains::ticket::service::TicketService::ensure_ticket_session(db, &ticket).await;
            }
            let msg = SseMessage::new(
                "ticket_created".to_string(),
                serde_json::json!({
                    "workspace_id": esc.workspace_id,
                    "ticket_id": id,
                    "title": esc.title,
                    "url": format!("/tickets/{id}"),
                    "hint": "点击查看简报",
                }),
            );
            sse.broadcast_message(msg).await;
            Some(id)
        }
        Ok(CreateOutcome::Duplicate(existing_id)) => {
            debug!(ticket_id = existing_id, "escalation dedup hit — recurrence");
            if let Err(e) = db.ticket_recurrence(existing_id, &esc.agent_run_id).await {
                error!(ticket_id = existing_id, error = %e, "recurrence append failed");
            }
            Some(existing_id)
        }
        Err(e) => {
            error!(workspace_id = %esc.workspace_id, error = %e, "escalation ticket creation failed");
            None
        }
    }
}

/// alarm 域 → ticket 域的单向端口（EventAlarmHook 同款的反向边）：
/// ticket 定义端口，adapter 实现，alarm 注入消费。
#[async_trait::async_trait]
pub trait AlarmEscalation: Send + Sync {
    /// Critical/Error 报警直达工单。返回 ticket_id（去重命中时为既有工单）。
    async fn escalate_alarm(&self, alarm: &tinyiothub_storage::alarm::Alarm) -> Option<i64>;
}

/// 生产实现：包 TicketService 的创建路径 + SSE 广播。
pub struct AlarmEscalationAdapter {
    db: Arc<Db>,
    sse: Arc<SseConnectionManager>,
}

impl AlarmEscalationAdapter {
    pub fn new(db: Arc<Db>, sse: Arc<SseConnectionManager>) -> Self {
        Self { db, sse }
    }
}

#[async_trait::async_trait]
impl AlarmEscalation for AlarmEscalationAdapter {
    async fn escalate_alarm(&self, alarm: &tinyiothub_storage::alarm::Alarm) -> Option<i64> {
        let workspace_id = alarm.workspace_id.clone().unwrap_or_else(|| alarm.thing_id.clone());
        let rule_part = alarm.rule_id.as_deref().unwrap_or("-");
        create_escalation(
            &self.db,
            &self.sse,
            Escalation {
                workspace_id,
                thing_id: Some(alarm.thing_id.clone()),
                agent_run_id: format!("alarm:{}", alarm.id),
                title: alarm.message.chars().take(80).collect(),
                briefing: serde_json::json!({
                    "problem": alarm.message,
                    "source": "critical_alarm_direct",
                    "alarm_id": alarm.id,
                    "alarm_level": alarm.alarm_level.as_str(),
                    "alarm_value": alarm.alarm_value,
                    "threshold_value": alarm.threshold_value,
                    "suggested_next_steps": ["AI 正在并行整理调查上下文，稍后附入本工单"],
                }),
                failure_hash: format!("pk:alarm:{}:{}", alarm.thing_id, rule_part),
            },
        )
        .await
    }
}
