//! 持久化订阅者（Task 8）：AgentEvent 广播 → DB 投影。
//!
//! 事件 → 落库映射（context 核实版）：
//! - `RunRecorded` → `agent_runs` insert-once：裸 INSERT 撞 unique 约束，
//!   预 SELECT 把"已存在"视为幂等成功（幂等即 fencing，stale 回放不覆盖）
//! - `HeartbeatResultReady` → `agent_actions`（首次失败 spawn 独立重试任务：
//!   2s base、2^attempt 退避、5 次 → DLQ + `AiEvent::HeartbeatPersistFailed`）
//! - `TrustConfigChanged` → `workspaces.heartbeat_trust_config` fencing upsert
//!   （CEO review T2：occurred_at 不早于已应用时间戳才写入；与 handler 先写
//!   路径双写同值无害，D11-⑤）
//! - `HeartbeatTasksChanged` → 无投影（tasks 由 handler 先写 DB）
//! - `DlqEntryAdded` → `agent_dead_letters`
//!
//! 丢事件恢复：`RecvError::Lagged` → `dump_state()` 全量重投影；另有
//! 周期全量对账（生产 5 分钟）。subscriber 只消费事件，不反向影响 crate。
//!
//! 启动顺序契约（Task 9，D11-①③）：AgentEventBus 由调用方先建并经
//! RuntimeDeps 注入 `AgentRuntime::restore`；持久化 receiver 在 restore
//! **之前**从该 bus 取得并传入 [`run_persistence_subscriber`]，保证
//! restore 期间及之后的事件不丢。shutdown 经 CancellationToken 编排：
//! 主循环与心跳重试任务都响应取消（Task 8 遗留 TODO 已接）。

use std::sync::Arc;
use std::time::Duration;

use sqlx::SqlitePool;
use tokio::sync::broadcast::error::RecvError;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, warn};

use tinyiothub_core::agent_runs::RunReport;
use tinyiothub_core::heartbeat::HeartbeatResult;
use tinyiothub_storage::Db;

use super::dlq_repo::SqliteDeadLetterQueue;
use tinyiothub_agent::runtime::event::bus::AiEventPublisher;
use tinyiothub_agent::runtime::event::dlq::DeadLetterQueue;
use tinyiothub_agent::runtime::event::types::AiEvent;
use tinyiothub_agent::runtime::events::{AgentEvent, AgentEventBus, AgentEventKind};
use tinyiothub_agent::runtime::runtime::AgentRuntime;
use tinyiothub_agent::runtime::snapshot::RestoreSnapshot;

/// 周期全量对账间隔（生产）。
const RECONCILE_INTERVAL: Duration = Duration::from_secs(300);
/// 心跳结果落库重试：5 次，2s base，2^attempt 退避（Task 6 删除的
/// `retry_with_backoff` 语义在此重建）。
const RETRY_MAX_ATTEMPTS: u32 = 5;
const RETRY_BASE_DELAY: Duration = Duration::from_secs(2);
/// resync 单项连续失败升级阈值（约 3 个对账周期）：周期对账是最后的
/// 安全网，永久失败若只 warn 会被每 5 分钟一条日志静默掩盖，DB 与内存
/// 真相源分叉——达阈值升级 error! + DLQ，恢复后计数清零（Task 8 fix round 1）。
const RESYNC_FAILURE_ESCALATION_THRESHOLD: u32 = 3;

/// resync 单项连续失败计数（key：run_id / workspace_id）。
#[derive(Default)]
pub(crate) struct ResyncFailures {
    pub(crate) runs: std::collections::HashMap<String, u32>,
    pub(crate) trust_configs: std::collections::HashMap<String, u32>,
}

fn record_failure(map: &mut std::collections::HashMap<String, u32>, key: &str) -> u32 {
    let count = map.entry(key.to_string()).or_insert(0);
    *count += 1;
    *count
}

/// 持久化订阅者入口（Task 9 定稿）：`rx` 必须是 restore 之前从共享 bus
/// 取得的 receiver（见模块文档的顺序契约）；`shutdown` 取消时主循环退出、
/// 在飞重试任务中止。
pub async fn run_persistence_subscriber(
    runtime: Arc<AgentRuntime>,
    db: Arc<Db>,
    rx: tokio::sync::broadcast::Receiver<AgentEvent>,
    shutdown: CancellationToken,
) {
    let publisher = runtime.orchestrator().event_publisher().clone();
    let dump = {
        let runtime = runtime.clone();
        move || runtime.dump_state()
    };
    run_persistence_loop(rx, dump, db, publisher, RECONCILE_INTERVAL, shutdown).await
}

/// subscriber 监管循环（CEO review T1）。
///
/// 持久化是内存真相源的唯一落库通道：订阅任务 panic 或异常退出曾是无告警、
/// 无重启的静默终点（JoinHandle 只在关停时被排空，bus 继续覆盖旧事件，
/// runs/心跳结果/DLQ 全部停止落库而系统看起来健康）。监管循环把任务死亡
/// 变成响亮、可恢复的事件：
/// 1. 内层任务以 spawn 包裹，panic 被捕获分类（Exited/Panicked）；
/// 2. error! 记录死亡（生产默认级别可见）；
/// 3. **先取新 receiver 再立即全量 resync**——闭合死亡窗口（D11-① 同款
///    顺序：订阅先于状态导出，resync 期间的事件不丢）；
/// 4. 指数退避后重启（防 panic 风暴榨干 CPU）；
/// 5. 仅当 shutdown 取消时退出。
pub async fn supervise_persistence_subscriber(
    runtime: Arc<AgentRuntime>,
    db: Arc<Db>,
    bus: Arc<AgentEventBus>,
    first_rx: tokio::sync::broadcast::Receiver<AgentEvent>,
    shutdown: CancellationToken,
) {
    let publisher = runtime.orchestrator().event_publisher().clone();
    let dump: Arc<dyn Fn() -> RestoreSnapshot + Send + Sync> = {
        let runtime = runtime.clone();
        Arc::new(move || runtime.dump_state())
    };
    let dump_for_loop = dump.clone();
    supervise_impl(dump, db, bus, first_rx, shutdown, move |rx, task_db, child| {
        let publisher = publisher.clone();
        let dump = dump_for_loop.clone();
        async move {
            run_persistence_loop(rx, move || dump(), task_db, publisher, RECONCILE_INTERVAL, child).await;
        }
    })
    .await
}

/// 可测监管核心（T1）：与生产循环解耦——测试注入会 panic 的 `run_once`
/// 验证"死亡 → error! → 新 receiver → resync → 退避 → 重启"全链路。
pub(crate) async fn supervise_impl<F, Fut>(
    dump: Arc<dyn Fn() -> RestoreSnapshot + Send + Sync>,
    db: Arc<Db>,
    bus: Arc<AgentEventBus>,
    first_rx: tokio::sync::broadcast::Receiver<AgentEvent>,
    shutdown: CancellationToken,
    run_once: F,
) where
    F: Fn(tokio::sync::broadcast::Receiver<AgentEvent>, Arc<Db>, CancellationToken) -> Fut + Send,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let mut rx = Some(first_rx);
    let mut restart_count: u32 = 0;
    loop {
        let child = shutdown.child_token();
        let task_rx = rx.take().unwrap_or_else(|| bus.subscribe());
        let handle = tokio::spawn(run_once(task_rx, db.clone(), child));
        match handle.await {
            Ok(()) if shutdown.is_cancelled() => return,
            Ok(()) => {
                error!("persistence subscriber exited unexpectedly — restarting (persistence was DOWN)");
            }
            Err(e) if shutdown.is_cancelled() => {
                error!(error = %e, "persistence subscriber panicked during shutdown");
                return;
            }
            Err(e) => {
                error!(error = %e, "persistence subscriber task PANICKED — restarting (persistence was DOWN)");
            }
        }
        restart_count += 1;
        // 死亡窗口闭合：新 receiver 从队尾开始，死窗内的事件靠立即全量
        // resync 补齐（等价一次 Lagged）。顺序敏感：先订阅后 resync。
        rx = Some(bus.subscribe());
        let mut failures = ResyncFailures::default();
        resync(dump(), db.pool(), &mut failures).await;
        tokio::select! {
            _ = tokio::time::sleep(restart_backoff(restart_count)) => {}
            _ = shutdown.cancelled() => return,
        }
    }
}

/// 监管重启退避：2s base、2^n 指数、60s 封顶。
fn restart_backoff(restart_count: u32) -> Duration {
    let shift = restart_count.saturating_sub(1).min(5);
    (Duration::from_secs(2) * 2u32.pow(shift)).min(Duration::from_secs(60))
}

/// 订阅主循环。与 `run_persistence_subscriber` 分离以便测试注入
/// pre-lagged receiver / 短对账周期。
pub async fn run_persistence_loop(
    mut rx: tokio::sync::broadcast::Receiver<AgentEvent>,
    dump_state: impl Fn() -> RestoreSnapshot + Send,
    db: Arc<Db>,
    publisher: Arc<AiEventPublisher>,
    reconcile_interval: Duration,
    shutdown: CancellationToken,
) {
    // 首次 tick 推迟一个完整周期：启动对账已由 Task 9 启动编排（restore
    // 预热 + 僵尸 reconcile）覆盖，避免与 restore 竞态。
    let mut interval = tokio::time::interval_at(tokio::time::Instant::now() + reconcile_interval, reconcile_interval);
    let mut resync_failures = ResyncFailures::default();

    loop {
        tokio::select! {
            msg = rx.recv() => match msg {
                Ok(event) => project(&event, db.pool(), &publisher, &shutdown).await,
                Err(RecvError::Lagged(n)) => {
                    warn!(dropped = n, "persistence subscriber lagged, full resync");
                    resync(dump_state(), db.pool(), &mut resync_failures).await;
                }
                Err(RecvError::Closed) => {
                    debug!("agent event bus closed, persistence subscriber exiting");
                    return;
                }
            },
            _ = interval.tick() => {
                resync(dump_state(), db.pool(), &mut resync_failures).await;
            }
            _ = shutdown.cancelled() => {
                debug!("persistence subscriber shutdown");
                return;
            }
        }
    }
}

/// 单事件投影。所有错误就地处理（log / 重试 / DLQ），不向上传播——
/// subscriber 永不因单个事件失败而退出。
async fn project(
    event: &AgentEvent,
    pool: &SqlitePool,
    publisher: &Arc<AiEventPublisher>,
    shutdown: &CancellationToken,
) {
    match &event.kind {
        AgentEventKind::RunRecorded {
            report,
            problem_key,
            dedup_key,
        } => {
            project_run(pool, report, problem_key.as_deref(), dedup_key.as_deref()).await;
        }
        AgentEventKind::HeartbeatResultReady { result } => {
            let db = Db::new(pool.clone());
            if let Err(e) = db.insert_heartbeat_result(&result.workspace_id, result).await {
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
                    shutdown.clone(),
                );
            }
        }
        AgentEventKind::TrustConfigChanged { workspace_id, config } => {
            // fencing upsert（CEO review T2）：以事件的 occurred_at 与已应用
            // 时间戳比较，乱序/回放的旧事件不覆盖新配置；与 handler 先写路径
            // 双写同值无害（D11-⑤）。
            let db = Db::new(pool.clone());
            let occurred_at = event.occurred_at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            match db
                .save_heartbeat_trust_config_fenced(workspace_id, config, &occurred_at)
                .await
            {
                Ok(true) => {}
                Ok(false) => {
                    debug!(
                        workspace_id = %workspace_id,
                        "trust config projection fenced (event older than applied state)"
                    );
                }
                Err(e) => {
                    error!(workspace_id = %workspace_id, error = %e, "trust config projection failed");
                }
            }
        }
        AgentEventKind::HeartbeatTasksChanged { workspace_id } => {
            // 无投影：tasks 由 handler 先写 DB（D11-⑤ 写序），事件仅作内存信号。
            debug!(workspace_id = %workspace_id, "heartbeat tasks changed — no projection (handler writes first)");
        }
        AgentEventKind::DlqEntryAdded { entry } => {
            let dlq = SqliteDeadLetterQueue::new(pool.clone());
            if let Err(e) = dlq
                .enqueue(
                    &entry.workspace_id,
                    &entry.event_type,
                    &entry.payload_json,
                    &entry.failure_reason,
                )
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
    let db = Db::new(pool.clone());
    if let Err(e) = db.insert_agent_run(report, problem_key, dedup_key).await {
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
    Db::new(pool.clone()).agent_run_exists(run_id).await
}

/// 全量重投影（Lagged resync / 周期对账）：recent_runs 幂等插入、
/// trust config 幂等 upsert。单项失败计入连续失败计数：达
/// RESYNC_FAILURE_ESCALATION_THRESHOLD 升级 error! + DLQ（越阈值只升级
/// 一次，不每周期刷 DLQ），恢复后计数清零（Task 8 fix round 1）。
pub(crate) async fn resync(snapshot: RestoreSnapshot, pool: &SqlitePool, failures: &mut ResyncFailures) {
    let db = Db::new(pool.clone());
    for report in &snapshot.recent_runs {
        let outcome: Result<(), String> = async {
            match run_exists(pool, &report.run_id).await {
                Ok(true) => return Ok(()), // 已存在：幂等 no-op
                Ok(false) => {}
                Err(e) => return Err(format!("existence check: {e}")),
            }
            // resync 补插恢复 O11 dedup 元数据（CEO review T3）：keys 来自
            // dump_state 导出的 RunRegistry 旁路映射；查不到（启动预热行，
            // 正常已由事件路径落库）才回退 None——此前无条件 None 会使
            // Lagged 补插的行永久丢失 problem_key，重启后问题重复派发。
            let (problem_key, dedup_key) = snapshot
                .recent_run_meta
                .get(&report.run_id)
                .map(|k| (k.problem_key.as_deref(), k.dedup_key.as_deref()))
                .unwrap_or((None, None));
            match db.insert_agent_run(report, problem_key, dedup_key).await {
                Ok(_) => Ok(()),
                Err(e) if e.to_string().contains("UNIQUE") => Ok(()), // 并发重复：幂等成功
                Err(e) => Err(e.to_string()),
            }
        }
        .await;
        match outcome {
            Ok(()) => {
                failures.runs.remove(&report.run_id);
            }
            Err(reason) => {
                let count = record_failure(&mut failures.runs, &report.run_id);
                if count == RESYNC_FAILURE_ESCALATION_THRESHOLD {
                    error!(
                        run_id = %report.run_id,
                        consecutive_failures = count,
                        error = %reason,
                        "resync run projection failing repeatedly, enqueuing to DLQ"
                    );
                    let payload = serde_json::to_string(report).unwrap_or_else(|se| {
                        format!(
                            r#"{{"unserializable":true,"run_id":"{}","serialize_error":"{}"}}"#,
                            report.run_id, se
                        )
                    });
                    let dlq = SqliteDeadLetterQueue::new(pool.clone());
                    if let Err(dlq_err) = dlq
                        .enqueue(&report.workspace_id, "RunRecorded", &payload, &reason)
                        .await
                    {
                        error!(run_id = %report.run_id, error = %dlq_err, "DLQ enqueue failed — resync run record lost");
                    }
                } else {
                    warn!(
                        run_id = %report.run_id,
                        consecutive_failures = count,
                        error = %reason,
                        "resync run projection failed"
                    );
                }
            }
        }
    }
    let db = Db::new(pool.clone());
    for state in &snapshot.heartbeat {
        match db
            .save_heartbeat_trust_config(&state.workspace_id, &state.trust_config)
            .await
        {
            Ok(()) => {
                failures.trust_configs.remove(&state.workspace_id);
            }
            Err(e) => {
                let reason = e.to_string();
                let count = record_failure(&mut failures.trust_configs, &state.workspace_id);
                if count == RESYNC_FAILURE_ESCALATION_THRESHOLD {
                    error!(
                        workspace_id = %state.workspace_id,
                        consecutive_failures = count,
                        error = %reason,
                        "resync trust config failing repeatedly, enqueuing to DLQ"
                    );
                    let dlq = SqliteDeadLetterQueue::new(pool.clone());
                    if let Err(dlq_err) = dlq
                        .enqueue(
                            &state.workspace_id,
                            "TrustConfigChanged",
                            &state.trust_config.to_db_json(),
                            &reason,
                        )
                        .await
                    {
                        error!(workspace_id = %state.workspace_id, error = %dlq_err, "DLQ enqueue failed — resync trust config lost");
                    }
                } else {
                    warn!(
                        workspace_id = %state.workspace_id,
                        consecutive_failures = count,
                        error = %reason,
                        "resync trust config failed"
                    );
                }
            }
        }
    }
}

/// 心跳结果重试任务（重建自 4d102722 `retry_with_backoff`）：独立 spawn，
/// 2s base、2^attempt 退避，累计 5 次失败 → DLQ + HeartbeatPersistFailed。
/// subscriber 侧采用简化形式：无 in-flight 计数。shutdown（Task 9 接线）：
/// backoff 睡眠响应 CancellationToken，进程退出时不滞留重试任务。
fn spawn_heartbeat_retry(
    pool: SqlitePool,
    publisher: Arc<AiEventPublisher>,
    workspace_id: String,
    result: HeartbeatResult,
    shutdown: CancellationToken,
) {
    tokio::spawn(async move {
        let db = Db::new(pool.clone());
        let dlq = SqliteDeadLetterQueue::new(pool);
        let mut attempt: u32 = 0;
        loop {
            tokio::select! {
                _ = tokio::time::sleep(RETRY_BASE_DELAY * 2u32.pow(attempt)) => {}
                _ = shutdown.cancelled() => {
                    debug!(workspace_id = %workspace_id, attempt, "heartbeat persist retry aborted by shutdown");
                    return;
                }
            }
            match db.insert_heartbeat_result(&workspace_id, &result).await {
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
