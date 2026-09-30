//! Db-backed adapters for `tinyiothub_runtime::ports` (D15, Task 11.5).
//!
//! runtime 不依赖 db/sqlx；组合根（service_manager）在这里把
//! `tinyiothub_storage` 的具体类型包装成 runtime 端口 trait 的实现并注入。
//! 直接委托现有 storage 函数/类型，不做二次抽象（8/3 db 层模式）。

use std::sync::Arc;

use async_trait::async_trait;
use tinyiothub_core::models::thing::Thing;
use tinyiothub_core::models::thing_command::ThingCommand;
use tinyiothub_runtime::ports::{EventRetentionStore, ThingCacheSource, ThingCommandQueries};
use tinyiothub_storage::Db;
use tinyiothub_storage::cache::ThingCache;

/// `ThingCache` → `ThingCacheSource`（全部方法同步直转）。
pub struct ThingCacheAdapter(pub Arc<ThingCache>);

impl ThingCacheSource for ThingCacheAdapter {
    fn all(&self) -> Vec<Thing> {
        self.0.all()
    }

    fn get(&self, id: &str) -> Option<Thing> {
        self.0.get(id)
    }

    fn get_by_name(&self, name: &str) -> Option<Thing> {
        self.0.get_by_name(name)
    }

    fn insert(&self, device: Thing) {
        self.0.insert(device);
    }

    fn update(&self, device: Thing) {
        self.0.update(device);
    }

    fn remove(&self, id: &str) {
        self.0.remove(id);
    }
}

/// `Db` → `ThingCommandQueries`。
pub struct ThingCommandQueriesAdapter(pub Db);

#[async_trait]
impl ThingCommandQueries for ThingCommandQueriesAdapter {
    async fn find_by_thing_and_name(&self, thing_id: &str, name: &str) -> Result<Option<ThingCommand>, String> {
        self.0
            .find_thing_command_by_thing_and_name(thing_id, name)
            .await
            .map_err(|e| e.to_string())
    }
}

/// `Db` → `EventRetentionStore`。SQL 已收编进 `db::event` 领域函数（Task 10），
/// 与原 runtime 内联语句逐字一致。
pub struct EventRetentionAdapter(pub Db);

#[async_trait]
impl EventRetentionStore for EventRetentionAdapter {
    async fn delete_occurrence_events_before(&self, cutoff_rfc3339: &str) -> Result<u64, String> {
        self.0
            .delete_occurrence_events_before(cutoff_rfc3339)
            .await
            .map_err(|e| e.to_string())
    }
}

/// T6：approval_timeout 执行器的 db 适配器。超时判断逐条：awaiting_approval
/// → escalated（条件迁移）→ create_escalation 工单 → 关联。
///
/// 2026-09-14 硬化（E2/2-1A）：三态 SLA + 逐条失败隔离（单条失败记 error
/// 继续扫，不株连整轮——一条坏数据不能反复阻断全部清扫）。
pub struct ApprovalTimeoutAdapter {
    pub db: Db,
    pub sse: Arc<crate::domains::event::sse_manager::SseConnectionManager>,
    /// 对账恢复用：stale investigating 且存在已完成 run 时重投影路由
    /// （2026-09-16 实测：run 完成但事件丢失，judgment 永久卡死）。
    pub alarm_service: Arc<crate::domains::alarm::service::AlarmService>,
    /// X1：SLA 升级（审批/执行超时转工单）时的「需要你」通知出口；None 静默。
    pub notify: Option<Arc<crate::domains::notify::NotificationManager>>,
}

impl ApprovalTimeoutAdapter {
    /// 从已完成的调查 run 重投影判断（事件丢失的对账兜底）。judgment 滞留
    /// investigating 但其 alarm 键已有完成 run 时，把存储的 RunReport 重新
    /// 走一遍 subscriber 路由（条件迁移幂等）。返回 true = 已恢复路由。
    async fn reproject_from_completed_run(&self, judgment: &tinyiothub_storage::judgment::Judgment) -> bool {
        let (Some(thing_id), Some(alarm_id)) = (judgment.thing_id.as_deref(), judgment.alarm_id.as_deref()) else {
            return false;
        };
        let rule_id = match self.db.find_alarm_by_id(alarm_id, Some(&judgment.workspace_id)).await {
            Ok(Some(a)) => a.rule_id.clone(),
            Ok(None) => {
                tracing::warn!(judgment_id = %judgment.id, alarm_id, "reproject: alarm row missing");
                return false;
            }
            Err(e) => {
                tracing::warn!(judgment_id = %judgment.id, error = %e, "reproject: alarm load failed");
                return false;
            }
        };
        let problem_key = format!("alarm:{}:{}", thing_id, rule_id.as_deref().unwrap_or("-"));
        let report = match self
            .db
            .latest_agent_run_report_by_problem_key(&judgment.workspace_id, &problem_key)
            .await
        {
            Ok(Some(r)) => r,
            Ok(None) => return false, // 没有已完成的 run——真的没被受理
            Err(e) => {
                tracing::warn!(judgment_id = %judgment.id, error = %e, "reproject: run report load failed");
                return false;
            }
        };
        tracing::info!(judgment_id = %judgment.id, problem_key, run_id = %report.run_id,
            "reprojecting stale judgment from completed run (RunRecorded event was lost)");
        let event = tinyiothub_agent::runtime::events::AgentEvent {
            seq: 0,
            occurred_at: chrono::Utc::now(),
            kind: tinyiothub_agent::runtime::events::AgentEventKind::RunRecorded {
                report: Box::new(report),
                problem_key: Some(problem_key),
                dedup_key: None,
            },
        };
        crate::domains::agent::host::judgment_subscriber::project(
            &event,
            &self.db,
            &self.sse,
            &self.alarm_service,
            &self.notify,
        )
        .await;
        // project 走条件迁移——确认判断真的离开了 investigating
        matches!(
            self.db.find_judgment_by_id(&judgment.id, &judgment.workspace_id).await,
            Ok(Some(j)) if j.status != tinyiothub_storage::judgment::JudgmentStatus::Investigating
        )
    }

    /// 升级一条判断为工单（共用 awaiting_approval/executing 两路）。
    async fn escalate_one(
        &self,
        judgment: &tinyiothub_storage::judgment::Judgment,
        from: tinyiothub_storage::judgment::JudgmentStatus,
        title_prefix: &str,
        source: &str,
    ) -> Result<(), String> {
        let transitioned = self
            .db
            .transit_judgment(
                &judgment.id,
                from,
                tinyiothub_storage::judgment::JudgmentStatus::Escalated,
                None,
            )
            .await
            .map_err(|e| e.to_string())?;
        if !transitioned {
            return Ok(()); // 并发互撞（人刚好在批/执行刚好完成）→ 跳过
        }
        let ticket_id = crate::domains::ticket::create_escalation(
            &self.db,
            &self.sse,
            crate::domains::ticket::Escalation {
                workspace_id: judgment.workspace_id.clone(),
                thing_id: judgment.thing_id.clone(),
                agent_run_id: judgment
                    .run_id
                    .clone()
                    .unwrap_or_else(|| format!("{source}:{}", judgment.id)),
                title: format!(
                    "{title_prefix}：{}",
                    judgment.reason.chars().take(70).collect::<String>()
                ),
                briefing: serde_json::json!({
                    "problem": judgment.reason,
                    "source": source,
                    "judgment_id": judgment.id,
                    "alarm_id": judgment.alarm_id,
                    "suggested_action": judgment.suggested_action,
                }),
                failure_hash: format!("pk:{source}:{}", judgment.id),
            },
        )
        .await;
        match ticket_id {
            Some(tid) => {
                if let Err(e) = self.db.link_judgment_ticket(&judgment.id, tid).await {
                    tracing::warn!(judgment_id = %judgment.id, error = %e, "link ticket failed");
                }
                // X1：SLA 升级也是「需要你」事件——通知到人（best-effort）。
                if let Some(manager) = &self.notify {
                    let message = crate::domains::notify::NotificationMessage::new(
                        "需要你：AI 已转工单".to_string(),
                        format!("{}（工单 #{}）", title_prefix, tid),
                        tinyiothub_core::models::event::EventLevel::Error,
                        vec![tinyiothub_core::notification_types::NotificationChannelType::Sse],
                        vec![judgment.workspace_id.clone()],
                    );
                    let manager = manager.clone();
                    let judgment_id = judgment.id.clone();
                    tokio::spawn(async move {
                        if let Err(e) = manager.send_notification(&message).await {
                            tracing::warn!(judgment_id, error = %e, "needs-you notify dispatch failed (best-effort)");
                        }
                    });
                }
                Ok(())
            }
            // 票建失败：judgment 已 escalated 但无工单——不计入升级数并 error
            // （该判断此后不再被 SLA 命中，必须响亮）
            None => Err(format!(
                "judgment {} escalated but ticket creation failed (orphan escalated state)",
                judgment.id
            )),
        }
    }
}

#[async_trait]
impl tinyiothub_runtime::ports::ApprovalTimeoutStore for ApprovalTimeoutAdapter {
    async fn escalate_stale_approvals(&self, cutoff_rfc3339: &str) -> Result<u64, String> {
        let stale = self
            .db
            .stale_awaiting_approvals(cutoff_rfc3339)
            .await
            .map_err(|e| e.to_string())?;
        let mut escalated = 0u64;
        for judgment in stale {
            // 逐条失败隔离（2-1A）：单条失败记 error 继续，不中断整轮
            if let Err(e) = self
                .escalate_one(
                    &judgment,
                    tinyiothub_storage::judgment::JudgmentStatus::AwaitingApproval,
                    "审批超时",
                    "approval-timeout",
                )
                .await
            {
                tracing::error!(judgment_id = %judgment.id, error = %e, "approval-timeout escalation failed (isolated)");
                continue;
            }
            escalated += 1;
        }
        Ok(escalated)
    }

    /// investigating 超 SLA → 先对账：存在已完成的调查 run 时重投影路由
    /// （RunRecorded 事件丢失的兜底——2026-09-16 实测 run 完成、verdict
    /// 合法，但 judgment 永久卡 investigating）；无 run 才标
    /// dispatch_suppressed（不开票不计预算；迟到 RunRecorded 由 subscriber
    /// 恢复路由，T-7/L1）。
    async fn mark_stale_investigating(&self, cutoff_rfc3339: &str) -> Result<u64, String> {
        let stale = self
            .db
            .stale_by_status(
                tinyiothub_storage::judgment::JudgmentStatus::Investigating,
                cutoff_rfc3339,
            )
            .await
            .map_err(|e| e.to_string())?;
        let mut marked = 0u64;
        for judgment in stale {
            if self.reproject_from_completed_run(&judgment).await {
                continue; // 已从完成的 run 恢复路由，不再是滞留判断
            }
            match self
                .db
                .fail_judgment(
                    &judgment.id,
                    tinyiothub_storage::judgment::JudgmentStatus::DispatchSuppressed,
                    "调查未受理或超时（防抖/队列），已标记",
                )
                .await
            {
                Ok(true) => marked += 1,
                Ok(false) => {} // 并发迁移（刚好判出）→ 跳过
                Err(e) => {
                    tracing::error!(judgment_id = %judgment.id, error = %e, "mark dispatch_suppressed failed (isolated)")
                }
            }
        }
        Ok(marked)
    }

    /// executing 超 SLA → escalated + 人工确认工单（T-16/C4：覆盖
    /// Acted-未验证、NoActionNeeded-冲突、exec 挂起三种「执行未闭环」）。
    async fn escalate_stale_executing(&self, cutoff_rfc3339: &str) -> Result<u64, String> {
        let stale = self
            .db
            .stale_by_status(tinyiothub_storage::judgment::JudgmentStatus::Executing, cutoff_rfc3339)
            .await
            .map_err(|e| e.to_string())?;
        let mut escalated = 0u64;
        for judgment in stale {
            if let Err(e) = self
                .escalate_one(
                    &judgment,
                    tinyiothub_storage::judgment::JudgmentStatus::Executing,
                    "执行超时/未验证，需人工确认",
                    "execution-timeout",
                )
                .await
            {
                tracing::error!(judgment_id = %judgment.id, error = %e, "execution-timeout escalation failed (isolated)");
                continue;
            }
            escalated += 1;
        }
        Ok(escalated)
    }

    /// X5：dismissed 超 cutoff 且关联报警仍 Active → 重浮 investigating 并重派
    /// 调查（否决有兜底——「用户说不用处理」不能成为永久静默）。报警已消/无
    /// 报警上下文的保持 dismissed（用户对了一半：事已经过去了）。
    async fn resurface_stale_dismissed(&self, cutoff_rfc3339: &str) -> Result<u64, String> {
        let stale = self
            .db
            .stale_by_status(tinyiothub_storage::judgment::JudgmentStatus::Dismissed, cutoff_rfc3339)
            .await
            .map_err(|e| e.to_string())?;
        let mut resurfaced = 0u64;
        for judgment in stale {
            let active_alarm = match &judgment.alarm_id {
                Some(alarm_id) => matches!(
                    self.alarm_service
                        .get_alarm_by_id(alarm_id, Some(&judgment.workspace_id))
                        .await,
                    Ok(Some(alarm)) if alarm.status.is_active()
                ),
                None => false,
            };
            if !active_alarm {
                continue;
            }
            // R-F1 守卫：同报警已有未终态判断（dismissed 不占 active 索引，
            // 新报警可合法触发新判断）→ 新判断才是活的，旧行保持 dismissed，
            // 不撞 judgments_active_alarm 唯一索引（每周期 error log 消除）。
            // 已知残窗（check-then-act，外部声音 #2）：本查询与下方 transit 非
            // 同事务，并发 insert_judgment 仍可能撞索引——主路径（存量 dismissed
            // 行的周期重试噪声）已消除，残窗以 error log 形式残留可接受。
            if let Some(alarm_id) = &judgment.alarm_id {
                match self.db.has_open_judgment_for_alarm(alarm_id).await {
                    Ok(true) => {
                        tracing::debug!(judgment_id = %judgment.id, alarm_id, "resurface skipped: newer open judgment owns the alarm");
                        continue;
                    }
                    Ok(false) => {}
                    Err(e) => {
                        tracing::warn!(judgment_id = %judgment.id, error = %e, "open-judgment guard query failed, skipping resurface this cycle");
                        continue;
                    }
                }
            }
            match self
                .db
                .transit_judgment(
                    &judgment.id,
                    tinyiothub_storage::judgment::JudgmentStatus::Dismissed,
                    tinyiothub_storage::judgment::JudgmentStatus::Investigating,
                    None,
                )
                .await
            {
                Ok(true) => {
                    // 重派调查（boot race 同机制：subscriber 按 thing+rule 匹配既有
                    // judgment 不重复建行）；失败只记日志——investigating SLA 兜底。
                    self.alarm_service.redispatch_judgment_investigation(&judgment).await;
                    resurfaced += 1;
                }
                Ok(false) => {} // 并发互撞（刚好被其他路径迁移）→ 跳过
                Err(e) => {
                    tracing::error!(judgment_id = %judgment.id, error = %e, "resurface dismissed failed (isolated)");
                    continue;
                }
            }
        }
        Ok(resurfaced)
    }
}

#[cfg(test)]
mod approval_timeout_tests {
    use super::*;

    async fn fixture() -> (tinyiothub_storage::Db, ApprovalTimeoutAdapter) {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        tinyiothub_storage::test_helpers::run_all_migrations(&pool)
            .await
            .unwrap();
        tinyiothub_storage::seed::seed_system(&tinyiothub_storage::Db::new(pool.clone()))
            .await
            .unwrap();
        sqlx::query("INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) VALUES ('ws1','ws1','tenant-default-001','2025-01-01','2025-01-01')").execute(&pool).await.unwrap();
        let db = tinyiothub_storage::Db::new(pool);
        let adapter = ApprovalTimeoutAdapter {
            db: db.clone(),
            sse: Arc::new(crate::domains::event::sse_manager::SseConnectionManager::new()),
            alarm_service: Arc::new(crate::domains::alarm::service::AlarmService::new(Arc::new(db.clone()))),
            notify: None,
        };
        (db, adapter)
    }

    #[tokio::test]
    async fn stale_approval_escalates_to_ticket() {
        let (db, adapter) = fixture().await;

        // 一条 awaiting_approval 且 judged_at 在 25h 前
        let jid = db
            .insert_judgment("ws1", None, None, Some("t1"), "annotate")
            .await
            .unwrap();
        db.judge_judgment(
            &jid,
            tinyiothub_storage::judgment::JudgmentVerdict::SelfHealable,
            "可重连",
            "{}",
            Some("重连"),
            Some("connection_recovery"),
            None,
        )
        .await
        .unwrap();
        sqlx::query("UPDATE judgments SET judged_at = datetime('now', '-25 hours') WHERE id = ?")
            .bind(&jid)
            .execute(db.pool())
            .await
            .unwrap();
        // 一条新的（不应命中）
        let fresh = db
            .insert_judgment("ws1", None, None, Some("t2"), "annotate")
            .await
            .unwrap();
        db.judge_judgment(
            &fresh,
            tinyiothub_storage::judgment::JudgmentVerdict::SelfHealable,
            "新的",
            "{}",
            None,
            None,
            None,
        )
        .await
        .unwrap();

        let cutoff = (chrono::Utc::now() - chrono::Duration::hours(24)).to_rfc3339();
        let n = tinyiothub_runtime::ports::ApprovalTimeoutStore::escalate_stale_approvals(&adapter, &cutoff)
            .await
            .unwrap();
        assert_eq!(n, 1);

        let j = db.find_judgment_by_id(&jid, "ws1").await.unwrap().unwrap();
        assert_eq!(j.status, tinyiothub_storage::judgment::JudgmentStatus::Escalated);
        assert!(j.ticket_id.is_some());
        let f = db.find_judgment_by_id(&fresh, "ws1").await.unwrap().unwrap();
        assert_eq!(
            f.status,
            tinyiothub_storage::judgment::JudgmentStatus::AwaitingApproval,
            "fresh untouched"
        );
    }

    /// E2：investigating 超 SLA → dispatch_suppressed 标记（不开票不计预算）。
    #[tokio::test]
    async fn stale_investigating_marked_suppressed_no_ticket() {
        let (db, adapter) = fixture().await;
        let jid = db
            .insert_judgment("ws1", None, None, Some("t1"), "annotate")
            .await
            .unwrap();
        sqlx::query("UPDATE judgments SET state_entered_at = datetime('now', '-40 minutes') WHERE id = ?")
            .bind(&jid)
            .execute(db.pool())
            .await
            .unwrap();
        let cutoff = (chrono::Utc::now() - chrono::Duration::minutes(30)).to_rfc3339();
        let n = tinyiothub_runtime::ports::ApprovalTimeoutStore::mark_stale_investigating(&adapter, &cutoff)
            .await
            .unwrap();
        assert_eq!(n, 1);
        let j = db.find_judgment_by_id(&jid, "ws1").await.unwrap();
        assert_eq!(
            j.unwrap().status,
            tinyiothub_storage::judgment::JudgmentStatus::DispatchSuppressed
        );
        // 不开票
        assert_eq!(
            db.count_by_statuses("ws1", &[tinyiothub_storage::judgment::JudgmentStatus::Escalated])
                .await
                .unwrap(),
            0
        );
    }

    /// X5：dismissed 超 72h 且关联报警仍 Active → 重浮 investigating（否决有兜底）。
    #[tokio::test]
    async fn dismissed_with_active_alarm_resurfaces() {
        let (db, adapter) = fixture().await;
        sqlx::query("INSERT INTO things (id, name, workspace_id, created_at, updated_at) VALUES ('t1','t1','ws1','2025-01-01','2025-01-01')")
            .execute(db.pool()).await.unwrap();
        let alarm = tinyiothub_storage::alarm::Alarm::new(
            "t1".to_string(),
            None,
            None,
            tinyiothub_storage::alarm::AlarmType::PropertyThreshold,
            tinyiothub_storage::alarm::AlarmLevel::Warning,
            "温度越限".to_string(),
            None,
            None,
            Some("ws1".to_string()),
        );
        let alarm_id = alarm.id.clone();
        db.insert_alarm(&alarm).await.unwrap();

        let jid = db
            .insert_judgment("ws1", Some(&alarm_id), None, Some("t1"), "annotate")
            .await
            .unwrap();
        db.judge_judgment(
            &jid,
            tinyiothub_storage::judgment::JudgmentVerdict::SelfHealable,
            "可重连",
            "{}",
            Some("重连"),
            Some("connection_recovery"),
            None,
        )
        .await
        .unwrap();
        let flipped = db
            .transit_judgment(
                &jid,
                tinyiothub_storage::judgment::JudgmentStatus::AwaitingApproval,
                tinyiothub_storage::judgment::JudgmentStatus::Dismissed,
                None,
            )
            .await
            .unwrap();
        assert!(flipped, "awaiting_approval → dismissed 是合法边");
        sqlx::query("UPDATE judgments SET state_entered_at = datetime('now', '-73 hours') WHERE id = ?")
            .bind(&jid)
            .execute(db.pool())
            .await
            .unwrap();

        let cutoff = (chrono::Utc::now() - chrono::Duration::hours(72)).to_rfc3339();
        let n = tinyiothub_runtime::ports::ApprovalTimeoutStore::resurface_stale_dismissed(&adapter, &cutoff)
            .await
            .unwrap();
        assert_eq!(n, 1, "报警仍 Active 的 dismissed 应重浮");
        let j = db.find_judgment_by_id(&jid, "ws1").await.unwrap().unwrap();
        assert_eq!(j.status, tinyiothub_storage::judgment::JudgmentStatus::Investigating);
    }

    /// X5 反向：报警已消（Resolved）的 dismissed 保持终态——用户对了一半。
    #[tokio::test]
    async fn dismissed_with_resolved_alarm_stays() {
        let (db, adapter) = fixture().await;
        sqlx::query("INSERT INTO things (id, name, workspace_id, created_at, updated_at) VALUES ('t1','t1','ws1','2025-01-01','2025-01-01')")
            .execute(db.pool()).await.unwrap();
        let alarm = tinyiothub_storage::alarm::Alarm::new(
            "t1".to_string(),
            None,
            None,
            tinyiothub_storage::alarm::AlarmType::PropertyThreshold,
            tinyiothub_storage::alarm::AlarmLevel::Warning,
            "温度越限".to_string(),
            None,
            None,
            Some("ws1".to_string()),
        );
        let alarm_id = alarm.id.clone();
        db.insert_alarm(&alarm).await.unwrap();
        sqlx::query("UPDATE thing_alarms SET is_resolved = 1 WHERE id = ?")
            .bind(&alarm_id)
            .execute(db.pool())
            .await
            .unwrap();

        let jid = db
            .insert_judgment("ws1", Some(&alarm_id), None, Some("t1"), "annotate")
            .await
            .unwrap();
        db.judge_judgment(
            &jid,
            tinyiothub_storage::judgment::JudgmentVerdict::SelfHealable,
            "可重连",
            "{}",
            Some("重连"),
            Some("connection_recovery"),
            None,
        )
        .await
        .unwrap();
        db.transit_judgment(
            &jid,
            tinyiothub_storage::judgment::JudgmentStatus::AwaitingApproval,
            tinyiothub_storage::judgment::JudgmentStatus::Dismissed,
            None,
        )
        .await
        .unwrap();
        sqlx::query("UPDATE judgments SET state_entered_at = datetime('now', '-73 hours') WHERE id = ?")
            .bind(&jid)
            .execute(db.pool())
            .await
            .unwrap();

        let cutoff = (chrono::Utc::now() - chrono::Duration::hours(72)).to_rfc3339();
        let n = tinyiothub_runtime::ports::ApprovalTimeoutStore::resurface_stale_dismissed(&adapter, &cutoff)
            .await
            .unwrap();
        assert_eq!(n, 0, "报警已消的 dismissed 不重浮");
        let j = db.find_judgment_by_id(&jid, "ws1").await.unwrap().unwrap();
        assert_eq!(j.status, tinyiothub_storage::judgment::JudgmentStatus::Dismissed);
    }

    /// R-F1 守卫：同报警已有新的未终态判断 → 重浮跳过（旧行保持 dismissed，
    /// 不撞 judgments_active_alarm 唯一索引；新判断才是活的）。
    #[tokio::test]
    async fn dismissed_with_newer_open_judgment_not_resurfaced() {
        let (db, adapter) = fixture().await;
        sqlx::query("INSERT INTO things (id, name, workspace_id, created_at, updated_at) VALUES ('t1','t1','ws1','2025-01-01','2025-01-01')")
            .execute(db.pool()).await.unwrap();
        let alarm = tinyiothub_storage::alarm::Alarm::new(
            "t1".to_string(),
            None,
            None,
            tinyiothub_storage::alarm::AlarmType::PropertyThreshold,
            tinyiothub_storage::alarm::AlarmLevel::Warning,
            "温度越限".to_string(),
            None,
            None,
            Some("ws1".to_string()),
        );
        let alarm_id = alarm.id.clone();
        db.insert_alarm(&alarm).await.unwrap();

        // j1：dismissed（旧，72h+）
        let j1 = db
            .insert_judgment("ws1", Some(&alarm_id), None, Some("t1"), "annotate")
            .await
            .unwrap();
        db.judge_judgment(
            &j1,
            tinyiothub_storage::judgment::JudgmentVerdict::SelfHealable,
            "可重连",
            "{}",
            Some("重连"),
            Some("connection_recovery"),
            None,
        )
        .await
        .unwrap();
        db.transit_judgment(
            &j1,
            tinyiothub_storage::judgment::JudgmentStatus::AwaitingApproval,
            tinyiothub_storage::judgment::JudgmentStatus::Dismissed,
            None,
        )
        .await
        .unwrap();
        sqlx::query("UPDATE judgments SET state_entered_at = datetime('now', '-73 hours') WHERE id = ?")
            .bind(&j1)
            .execute(db.pool())
            .await
            .unwrap();
        // j2：同报警的新 investigating 判断（dismissed 不占 active 索引,合法建行）
        let j2 = db
            .insert_judgment("ws1", Some(&alarm_id), None, Some("t1"), "annotate")
            .await
            .unwrap();

        let cutoff = (chrono::Utc::now() - chrono::Duration::hours(72)).to_rfc3339();
        let n = tinyiothub_runtime::ports::ApprovalTimeoutStore::resurface_stale_dismissed(&adapter, &cutoff)
            .await
            .unwrap();
        assert_eq!(n, 0, "新判断接管的重浮必须跳过");
        let old = db.find_judgment_by_id(&j1, "ws1").await.unwrap().unwrap();
        assert_eq!(
            old.status,
            tinyiothub_storage::judgment::JudgmentStatus::Dismissed,
            "旧行保持 dismissed（不撞唯一索引）"
        );
        let new = db.find_judgment_by_id(&j2, "ws1").await.unwrap().unwrap();
        assert_eq!(
            new.status,
            tinyiothub_storage::judgment::JudgmentStatus::Investigating,
            "新判断不受影响"
        );
    }

    /// G2：SLA 升级时 notify=Some → 「需要你」通知真实派发且投递目标正确（X1 适配器分支）。
    #[tokio::test]
    async fn sla_escalation_sends_needs_you_notify() {
        struct RecordingChannel {
            sent: std::sync::Arc<std::sync::Mutex<Vec<(String, Vec<String>)>>>,
        }
        #[async_trait::async_trait]
        impl crate::domains::notify::dto::NotificationChannel for RecordingChannel {
            fn channel_type(&self) -> tinyiothub_core::notification_types::NotificationChannelType {
                tinyiothub_core::notification_types::NotificationChannelType::Sse
            }
            async fn send(
                &self,
                message: &crate::domains::notify::dto::NotificationMessage,
            ) -> std::result::Result<(), String> {
                self.sent
                    .lock()
                    .unwrap()
                    .push((message.title.clone(), message.recipients.clone()));
                Ok(())
            }
            async fn is_available(&self) -> bool {
                true
            }
            fn get_config(&self) -> std::collections::HashMap<String, String> {
                Default::default()
            }
        }

        let (db, _) = fixture().await;
        let sent = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut manager = crate::domains::notify::NotificationManager::new(std::sync::Arc::new(db.clone()));
        manager.register_channel(Box::new(RecordingChannel { sent: sent.clone() }));
        let adapter = ApprovalTimeoutAdapter {
            db: db.clone(),
            sse: std::sync::Arc::new(crate::domains::event::sse_manager::SseConnectionManager::new()),
            alarm_service: std::sync::Arc::new(crate::domains::alarm::service::AlarmService::new(std::sync::Arc::new(
                db.clone(),
            ))),
            notify: Some(std::sync::Arc::new(manager)),
        };

        // 一条 awaiting_approval 且 judged_at 在 25h 前 → SLA 升级
        let jid = db
            .insert_judgment("ws1", None, None, Some("t1"), "annotate")
            .await
            .unwrap();
        db.judge_judgment(
            &jid,
            tinyiothub_storage::judgment::JudgmentVerdict::SelfHealable,
            "可重连",
            "{}",
            Some("重连"),
            Some("connection_recovery"),
            None,
        )
        .await
        .unwrap();
        sqlx::query("UPDATE judgments SET judged_at = datetime('now', '-25 hours') WHERE id = ?")
            .bind(&jid)
            .execute(db.pool())
            .await
            .unwrap();

        let cutoff = (chrono::Utc::now() - chrono::Duration::hours(24)).to_rfc3339();
        let n = tinyiothub_runtime::ports::ApprovalTimeoutStore::escalate_stale_approvals(&adapter, &cutoff)
            .await
            .unwrap();
        assert_eq!(n, 1);

        // spawn 派发不 await——短轮询等异步落盘
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while sent.lock().unwrap().is_empty() && std::time::Instant::now() < deadline {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let got = sent.lock().unwrap();
        assert_eq!(got.len(), 1, "SLA 升级应发「需要你」通知");
        // 外部声音 #5：断言投递目标——「例外到人」的路由维度，不只是派发次数
        assert_eq!(got[0].1, vec!["ws1".to_string()], "通知必须投递到本工作区");
    }

    /// 对账恢复（2026-09-16 实测：run 完成且 verdict 合法，但 RunRecorded
    /// 事件丢失 → judgment 永久卡 investigating）：超 SLA 的 investigating
    /// 判断若已有完成的 run，从存储的 RunReport 重投影路由，而不是标
    /// dispatch_suppressed。
    #[tokio::test]
    async fn stale_investigating_reprojects_from_completed_run() {
        let (db, adapter) = fixture().await;
        // FK 链：thing → rule → alarm → judgment
        sqlx::query("INSERT INTO things (id, name, workspace_id, created_at, updated_at) VALUES ('t1','t1','ws1','2025-01-01','2025-01-01')")
            .execute(db.pool()).await.unwrap();
        sqlx::query("INSERT INTO thing_alarm_rules (id, thing_id, rule_name, rule_type, condition_config, alarm_level, workspace_id) VALUES ('r1','t1','温度阈值','threshold','{}','warning','ws1')")
            .execute(db.pool()).await.unwrap();
        sqlx::query("INSERT INTO thing_alarms (id, thing_id, rule_id, workspace_id, alarm_level, alarm_message, alarm_time) VALUES ('a1','t1','r1','ws1','warning','温度越限','2025-01-01')")
            .execute(db.pool()).await.unwrap();
        let jid = db
            .insert_judgment("ws1", Some("a1"), None, Some("t1"), "annotate")
            .await
            .unwrap();
        // 卡在 investigating 超过 SLA
        sqlx::query("UPDATE judgments SET state_entered_at = datetime('now', '-40 minutes') WHERE id = ?")
            .bind(&jid)
            .execute(db.pool())
            .await
            .unwrap();
        // 完成的调查 run（事件丢失前的真实产出）：verdict = noise
        let report = tinyiothub_core::agent_runs::RunReport {
            run_id: "run-lost-1".to_string(),
            workspace_id: "ws1".to_string(),
            trigger: "alarm".to_string(),
            outcome: tinyiothub_core::agent_runs::Outcome::NoActionNeeded,
            summary: "分析了设备状态，温度属正常波动。\n```json\n{\"verdict\": \"noise\", \"reason\": \"正常波动\", \"suggested_action\": null, \"action_category\": \"other\"}\n```".to_string(),
            actions: vec![],
            verified: false,
            duration_ms: 1000,
            tool_calls: 1,
            tokens: 100,
            end_reason: None,
            thing_id: Some("t1".to_string()),
        };
        db.insert_agent_run(&report, Some("alarm:t1:r1"), None).await.unwrap();

        let cutoff = (chrono::Utc::now() - chrono::Duration::minutes(30)).to_rfc3339();
        let n = tinyiothub_runtime::ports::ApprovalTimeoutStore::mark_stale_investigating(&adapter, &cutoff)
            .await
            .unwrap();
        assert_eq!(n, 0, "有完成 run 的滞留判断应被重投影，而不是标 suppressed");
        let j = db.find_judgment_by_id(&jid, "ws1").await.unwrap().unwrap();
        assert_eq!(
            j.status,
            tinyiothub_storage::judgment::JudgmentStatus::NoiseArchived,
            "应从 run 的 verdict 恢复路由"
        );
        assert_eq!(j.run_id.as_deref(), Some("run-lost-1"), "run_id 应回填");
    }

    /// E2/T-16：executing 超 SLA → escalated + 人工确认工单。
    #[tokio::test]
    async fn stale_executing_escalates_to_human_confirm_ticket() {
        let (db, adapter) = fixture().await;
        let jid = db
            .insert_judgment("ws1", None, None, Some("t1"), "annotate")
            .await
            .unwrap();
        db.judge_judgment(
            &jid,
            tinyiothub_storage::judgment::JudgmentVerdict::SelfHealable,
            "可重连",
            "{}",
            Some("重连"),
            Some("connection_recovery"),
            None,
        )
        .await
        .unwrap();
        db.transit_judgment(
            &jid,
            tinyiothub_storage::judgment::JudgmentStatus::AwaitingApproval,
            tinyiothub_storage::judgment::JudgmentStatus::Executing,
            None,
        )
        .await
        .unwrap();
        sqlx::query("UPDATE judgments SET state_entered_at = datetime('now', '-2 hours') WHERE id = ?")
            .bind(&jid)
            .execute(db.pool())
            .await
            .unwrap();
        let cutoff = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
        let n = tinyiothub_runtime::ports::ApprovalTimeoutStore::escalate_stale_executing(&adapter, &cutoff)
            .await
            .unwrap();
        assert_eq!(n, 1);
        let j = db.find_judgment_by_id(&jid, "ws1").await.unwrap().unwrap();
        assert_eq!(j.status, tinyiothub_storage::judgment::JudgmentStatus::Escalated);
        assert!(j.ticket_id.is_some(), "人工确认工单已建");
    }
}
