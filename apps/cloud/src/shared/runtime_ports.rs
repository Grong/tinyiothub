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
pub struct ApprovalTimeoutAdapter {
    pub db: Db,
    pub sse: Arc<crate::domains::event::sse_manager::SseConnectionManager>,
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
            // 条件迁移防并发（人刚好在批）：非 awaiting_approval 则跳过
            let transitioned = self
                .db
                .transit_judgment(
                    &judgment.id,
                    tinyiothub_storage::judgment::JudgmentStatus::AwaitingApproval,
                    tinyiothub_storage::judgment::JudgmentStatus::Escalated,
                    None,
                )
                .await
                .map_err(|e| e.to_string())?;
            if !transitioned {
                continue;
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
                        .unwrap_or_else(|| format!("approval-timeout:{}", judgment.id)),
                    title: format!("审批超时：{}", judgment.reason.chars().take(70).collect::<String>()),
                    briefing: serde_json::json!({
                        "problem": judgment.reason,
                        "source": "approval_timeout",
                        "judgment_id": judgment.id,
                        "alarm_id": judgment.alarm_id,
                        "suggested_action": judgment.suggested_action,
                    }),
                    failure_hash: format!("pk:approval-timeout:{}", judgment.id),
                },
            )
            .await;
            if let Some(tid) = ticket_id
                && let Err(e) = self.db.link_judgment_ticket(&judgment.id, tid).await
            {
                tracing::warn!(judgment_id = %judgment.id, error = %e, "link ticket failed");
            }
            escalated += 1;
        }
        Ok(escalated)
    }
}

#[cfg(test)]
mod approval_timeout_tests {
    use super::*;

    #[tokio::test]
    async fn stale_approval_escalates_to_ticket() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        tinyiothub_storage::test_helpers::run_all_migrations(&pool).await.unwrap();
        tinyiothub_storage::seed::seed_system(&tinyiothub_storage::Db::new(pool.clone())).await.unwrap();
        sqlx::query("INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) VALUES ('ws1','ws1','tenant-default-001','2025-01-01','2025-01-01')").execute(&pool).await.unwrap();
        let db = tinyiothub_storage::Db::new(pool);

        // 一条 awaiting_approval 且 judged_at 在 25h 前
        let jid = db.insert_judgment("ws1", None, None, Some("t1")).await.unwrap();
        db.judge_judgment(&jid, tinyiothub_storage::judgment::JudgmentVerdict::SelfHealable, "可重连", "{}", Some("重连"), Some("connection_recovery"), None).await.unwrap();
        sqlx::query("UPDATE judgments SET judged_at = datetime('now', '-25 hours') WHERE id = ?").bind(&jid).execute(db.pool()).await.unwrap();
        // 一条新的（不应命中）
        let fresh = db.insert_judgment("ws1", None, None, Some("t2")).await.unwrap();
        db.judge_judgment(&fresh, tinyiothub_storage::judgment::JudgmentVerdict::SelfHealable, "新的", "{}", None, None, None).await.unwrap();

        let adapter = ApprovalTimeoutAdapter {
            db: db.clone(),
            sse: Arc::new(crate::domains::event::sse_manager::SseConnectionManager::new()),
        };
        let cutoff = (chrono::Utc::now() - chrono::Duration::hours(24)).to_rfc3339();
        let n = tinyiothub_runtime::ports::ApprovalTimeoutStore::escalate_stale_approvals(&adapter, &cutoff).await.unwrap();
        assert_eq!(n, 1);

        let j = db.find_judgment_by_id(&jid, "ws1").await.unwrap().unwrap();
        assert_eq!(j.status, tinyiothub_storage::judgment::JudgmentStatus::Escalated);
        assert!(j.ticket_id.is_some());
        let f = db.find_judgment_by_id(&fresh, "ws1").await.unwrap().unwrap();
        assert_eq!(f.status, tinyiothub_storage::judgment::JudgmentStatus::AwaitingApproval, "fresh untouched");
    }
}
