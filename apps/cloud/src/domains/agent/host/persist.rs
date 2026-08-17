//! 持久化订阅者（Task 8）：AgentEvent 广播 → DB 投影。
//!
//! 事件 → 落库映射（context 核实版）：
//! - `RunRecorded` → `agent_runs` insert-once：裸 INSERT 撞 unique 约束，
//!   预 SELECT 把"已存在"视为幂等成功（幂等即 fencing，stale 回放不覆盖）
//! - `HeartbeatResultReady` → `agent_actions`（首次失败 spawn 独立重试任务：
//!   2s base、2^attempt 退避、5 次 → DLQ + `AiEvent::HeartbeatPersistFailed`）
//! - `TrustConfigChanged` → `workspaces.heartbeat_trust_config` 幂等 upsert
//!   （与 handler 先写路径双写同值无害，D11-⑤）
//! - `HeartbeatTasksChanged` → 无投影（tasks 由 handler 先写 DB）
//! - `DlqEntryAdded` → `agent_dead_letters`
//!
//! 丢事件恢复：`RecvError::Lagged` → `dump_state()` 全量重投影；另有
//! 周期全量对账（生产 5 分钟）。subscriber 只消费事件，不反向影响 crate。

use std::sync::Arc;
use std::time::Duration;

use sqlx::SqlitePool;
use tokio::sync::broadcast::error::RecvError;
use tracing::{debug, error, warn};

use tinyiothub_core::agent_runs::RunReport;
use tinyiothub_core::heartbeat::HeartbeatResult;
use tinyiothub_storage::agent_runs::AgentRunsRepository;
use tinyiothub_storage::heartbeat::HeartbeatTaskRepository;
use tinyiothub_storage::Database;

use super::dlq_repo::SqliteDeadLetterQueue;
use crate::domains::agent::loop_::event::bus::AiEventPublisher;
use crate::domains::agent::loop_::event::dlq::DeadLetterQueue;
use crate::domains::agent::loop_::event::types::AiEvent;
use crate::domains::agent::loop_::events::{AgentEvent, AgentEventKind};
use crate::domains::agent::loop_::runtime::AgentRuntime;
use crate::domains::agent::loop_::snapshot::RestoreSnapshot;

/// 周期全量对账间隔（生产）。
const RECONCILE_INTERVAL: Duration = Duration::from_secs(300);
/// 心跳结果落库重试：5 次，2s base，2^attempt 退避（Task 6 删除的
/// `retry_with_backoff` 语义在此重建）。
const RETRY_MAX_ATTEMPTS: u32 = 5;
const RETRY_BASE_DELAY: Duration = Duration::from_secs(2);

/// Brief 接口：service_manager 在 runtime 就绪后 spawn（Task 9 定最终
/// 启动顺序：订阅先于 restore、僵尸 reconcile）。
pub async fn run_persistence_subscriber(runtime: Arc<AgentRuntime>, db: Arc<Database>) {
    let publisher = runtime.orchestrator().event_publisher().clone();
    let rx = runtime.subscribe();
    let dump = {
        let runtime = runtime.clone();
        move || runtime.dump_state()
    };
    run_persistence_loop(rx, dump, db, publisher, RECONCILE_INTERVAL).await
}

/// 订阅主循环。与 `run_persistence_subscriber` 分离以便 service_manager
/// 在 AgentRuntime 门面接线完成（Task 9）前先用现有组件接线，也便于测试
/// 注入 pre-lagged receiver / 短对账周期。
pub async fn run_persistence_loop(
    mut rx: tokio::sync::broadcast::Receiver<AgentEvent>,
    dump_state: impl Fn() -> RestoreSnapshot + Send,
    db: Arc<Database>,
    publisher: Arc<AiEventPublisher>,
    reconcile_interval: Duration,
) {
    // 首次 tick 推迟一个完整周期：启动对账由 Task 9 启动编排骨架负责，
    // 避免与 restore 竞态。
    let mut interval = tokio::time::interval_at(
        tokio::time::Instant::now() + reconcile_interval,
        reconcile_interval,
    );

    loop {
        tokio::select! {
            msg = rx.recv() => match msg {
                Ok(event) => project(&event, db.pool(), &publisher).await,
                Err(RecvError::Lagged(n)) => {
                    warn!(dropped = n, "persistence subscriber lagged, full resync");
                    resync(dump_state(), db.pool()).await;
                }
                Err(RecvError::Closed) => {
                    debug!("agent event bus closed, persistence subscriber exiting");
                    return;
                }
            },
            _ = interval.tick() => {
                resync(dump_state(), db.pool()).await;
            }
        }
    }
}

/// 单事件投影。所有错误就地处理（log / 重试 / DLQ），不向上传播——
/// subscriber 永不因单个事件失败而退出。
async fn project(event: &AgentEvent, pool: &SqlitePool, publisher: &Arc<AiEventPublisher>) {
    match &event.kind {
        AgentEventKind::RunRecorded {
            report,
            problem_key,
            dedup_key,
        } => {
            project_run(pool, report, problem_key.as_deref(), dedup_key.as_deref()).await;
        }
        AgentEventKind::HeartbeatResultReady { result } => {
            let task_repo = HeartbeatTaskRepository::new(pool.clone());
            if let Err(e) = task_repo.insert_result(&result.workspace_id, result).await {
                warn!(
                    workspace_id = %result.workspace_id,
                    error = %e,
                    "heartbeat result persist failed, spawning retry task"
                );
                spawn_heartbeat_retry(
                    pool.clone(),
                    publisher.clone(),
                    result.workspace_id.clone(),
                    (**result).clone(),
                );
            }
        }
        AgentEventKind::TrustConfigChanged { workspace_id, config } => {
            // 幂等 upsert：与 handler 先写路径双写同值无害（D11-⑤）。
            let task_repo = HeartbeatTaskRepository::new(pool.clone());
            if let Err(e) = task_repo.save_trust_config(workspace_id, config).await {
                error!(workspace_id = %workspace_id, error = %e, "trust config projection failed");
            }
        }
        AgentEventKind::HeartbeatTasksChanged { workspace_id } => {
            // 无投影：tasks 由 handler 先写 DB（D11-⑤ 写序），事件仅作内存信号。
            debug!(workspace_id = %workspace_id, "heartbeat tasks changed — no projection (handler writes first)");
        }
        AgentEventKind::DlqEntryAdded { entry } => {
            let dlq = SqliteDeadLetterQueue::new(pool.clone());
            if let Err(e) = dlq
                .enqueue(&entry.workspace_id, &entry.event_type, &entry.payload_json, &entry.failure_reason)
                .await
            {
                error!(workspace_id = %entry.workspace_id, error = %e, "DLQ entry projection failed");
            }
        }
    }
}

/// run 记录幂等投影：预 SELECT 把重复 run_id 视为成功；并发下预检穿透
/// 撞 unique 约束同样按幂等成功处理（insert-once，幂等即 fencing）。
async fn project_run(pool: &SqlitePool, report: &RunReport, problem_key: Option<&str>, dedup_key: Option<&str>) {
    match run_exists(pool, &report.run_id).await {
        Ok(true) => {
            debug!(run_id = %report.run_id, "run already persisted, skipping (idempotent)");
            return;
        }
        Ok(false) => {}
        Err(e) => {
            error!(run_id = %report.run_id, error = %e, "run existence check failed");
            return;
        }
    }
    let runs_repo = AgentRunsRepository::new(pool.clone());
    if let Err(e) = runs_repo.insert_run(report, problem_key, dedup_key).await {
        if e.to_string().contains("UNIQUE") {
            debug!(run_id = %report.run_id, "run insert hit unique constraint (concurrent duplicate), treating as success");
            return;
        }
        error!(run_id = %report.run_id, error = %e, "run record projection failed");
        let payload = serde_json::to_string(report).unwrap_or_else(|se| {
            format!(
                r#"{{"unserializable":true,"run_id":"{}","serialize_error":"{}"}}"#,
                report.run_id, se
            )
        });
        let dlq = SqliteDeadLetterQueue::new(pool.clone());
        if let Err(dlq_err) = dlq
            .enqueue(&report.workspace_id, "RunRecorded", &payload, &e.to_string())
            .await
        {
            error!(run_id = %report.run_id, error = %dlq_err, "DLQ enqueue failed — run record lost");
        }
    }
}

async fn run_exists(pool: &SqlitePool, run_id: &str) -> Result<bool, sqlx::Error> {
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM agent_runs WHERE id = ?")
        .bind(run_id)
        .fetch_one(pool)
        .await?;
    Ok(n > 0)
}

/// 全量重投影（Lagged resync / 周期对账）：recent_runs 幂等插入、
/// trust config 幂等 upsert。单项失败 warn 后继续——下一周期自愈。
async fn resync(snapshot: RestoreSnapshot, pool: &SqlitePool) {
    let runs_repo = AgentRunsRepository::new(pool.clone());
    for report in &snapshot.recent_runs {
        match run_exists(pool, &report.run_id).await {
            Ok(true) => continue, // 已存在：幂等 no-op
            Ok(false) => {}
            Err(e) => {
                warn!(run_id = %report.run_id, error = %e, "resync existence check failed");
                continue;
            }
        }
        // resync 路径没有 problem_key/dedup_key 元数据（RunRegistry 不持有）；
        // 正常情形下该行已由事件路径落库，此处 insert 为幂等 no-op。
        if let Err(e) = runs_repo.insert_run(report, None, None).await {
            if e.to_string().contains("UNIQUE") {
                continue; // 并发重复：幂等成功
            }
            warn!(run_id = %report.run_id, error = %e, "resync run projection failed");
        }
    }
    let task_repo = HeartbeatTaskRepository::new(pool.clone());
    for state in &snapshot.heartbeat {
        if let Err(e) = task_repo
            .save_trust_config(&state.workspace_id, &state.trust_config)
            .await
        {
            warn!(workspace_id = %state.workspace_id, error = %e, "resync trust config failed");
        }
    }
}

/// 心跳结果重试任务（重建自 4d102722 `retry_with_backoff`）：独立 spawn，
/// 2s base、2^attempt 退避，累计 5 次失败 → DLQ + HeartbeatPersistFailed。
/// subscriber 侧采用简化形式：无 in-flight 计数，进程退出时未完成任务随
/// runtime drop（context：订阅者任务在 shutdown 时完成当前投影即退出）。
fn spawn_heartbeat_retry(
    pool: SqlitePool,
    publisher: Arc<AiEventPublisher>,
    workspace_id: String,
    result: HeartbeatResult,
) {
    tokio::spawn(async move {
        let task_repo = HeartbeatTaskRepository::new(pool.clone());
        let dlq = SqliteDeadLetterQueue::new(pool);
        let mut attempt: u32 = 0;
        loop {
            tokio::time::sleep(RETRY_BASE_DELAY * 2u32.pow(attempt)).await;
            match task_repo.insert_result(&workspace_id, &result).await {
                Ok(_) => {
                    debug!(workspace_id = %workspace_id, attempt, "heartbeat result persisted on retry");
                    return;
                }
                Err(e) => {
                    attempt += 1;
                    let last_error = e.to_string();
                    if attempt >= RETRY_MAX_ATTEMPTS {
                        error!(
                            workspace_id = %workspace_id,
                            attempts = attempt,
                            error = %last_error,
                            "heartbeat persist exhausted retries, enqueuing to DLQ"
                        );
                        let payload = serde_json::to_string(&result).unwrap_or_else(|se| {
                            format!(
                                r#"{{"unserializable":true,"workspace_id":"{}","serialize_error":"{}"}}"#,
                                result.workspace_id, se
                            )
                        });
                        if let Err(dlq_err) = dlq
                            .enqueue(&workspace_id, "HeartbeatCompleted", &payload, &last_error)
                            .await
                        {
                            error!(
                                workspace_id = %workspace_id,
                                error = %dlq_err,
                                "DLQ enqueue failed — heartbeat result lost"
                            );
                        }
                        publisher.publish(AiEvent::HeartbeatPersistFailed {
                            workspace_id: workspace_id.clone(),
                            reason: format!("Failed after {} attempts: {}", RETRY_MAX_ATTEMPTS, last_error),
                        });
                        return;
                    }
                    warn!(workspace_id = %workspace_id, attempt, error = %last_error, "heartbeat persist retry");
                }
            }
        }
    });
}
