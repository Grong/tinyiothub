//! Port traits — runtime's only window onto persistence (D15, Task 11.5).
//!
//! runtime is a framework crate and must not depend on `db`/sqlx; the
//! composition root (apps/cloud) injects db-backed adapters implementing
//! these traits. Method sets cover exactly the current call sites — add a
//! method only when a real caller appears (YAGNI).
//!
//! Error type is `String`: adapters map concrete backend errors (sqlx etc.)
//! at the boundary so the port surface stays backend-agnostic.

use async_trait::async_trait;

use tinyiothub_core::models::thing::Thing;
use tinyiothub_core::models::thing_command::ThingCommand;

/// Read-side queries for device commands (cron `device_command` executor).
#[async_trait]
pub trait ThingCommandQueries: Send + Sync {
    /// Find a command by device ID and command name.
    async fn find_by_thing_and_name(&self, thing_id: &str, name: &str) -> Result<Option<ThingCommand>, String>;
}

/// Event-retention writes (cron `event_retention` executor).
#[async_trait]
pub trait EventRetentionStore: Send + Sync {
    /// Delete occurrence-type events (`is_status = 0`) with timestamp older
    /// than `cutoff_rfc3339`. Status rows (`is_status = 1`, live device
    /// state) are exempt. Returns rows deleted.
    async fn delete_occurrence_events_before(&self, cutoff_rfc3339: &str) -> Result<u64, String>;
}

/// Approval-timeout escalations (cron `approval_timeout` executor, T6):
/// awaiting_approval 超过 24h 的判断自动升级为工单（防"审批堆积"——设计
/// 文档状态机）。返回升级的条数。
///
/// 2026-09-14 硬化（E2）：三态 SLA 清扫。
/// - investigating 超 SLA（默认 30min）：dispatch 被拦/调查挂起 → 标
///   dispatch_suppressed 标记（不开票、不计预算；迟到 RunRecorded 恢复路由）
/// - executing 超 SLA（默认 1h）：执行未验证/队列卡死 → escalated + 人工
///   确认工单（T-16/C4：起算点是 state_entered_at，不是批准时刻）
#[async_trait]
pub trait ApprovalTimeoutStore: Send + Sync {
    async fn escalate_stale_approvals(&self, cutoff_rfc3339: &str) -> Result<u64, String>;
    async fn mark_stale_investigating(&self, cutoff_rfc3339: &str) -> Result<u64, String>;
    async fn escalate_stale_executing(&self, cutoff_rfc3339: &str) -> Result<u64, String>;
}

/// Thing cache used by `DataServer`. Sync because every call site is sync
/// (the backing implementation is an in-memory cache); making this async
/// would add `.await` noise with no benefit.
pub trait ThingCacheSource: Send + Sync {
    fn all(&self) -> Vec<Thing>;
    fn get(&self, id: &str) -> Option<Thing>;
    fn get_by_name(&self, name: &str) -> Option<Thing>;
    fn insert(&self, device: Thing);
    fn update(&self, device: Thing);
    fn remove(&self, id: &str);
}
