//! 每设备 driver worker（E4b spec §2.3）
//!
//! 单写者独占驱动：驱动生命周期与所有调用都归这个任务，外部只持
//! [`WorkerHandle`]。队列策略：
//!
//! - **Execute**：有界 FIFO（32）；满时 [`WorkerHandle::submit_execute`] 立即
//!   返回 `DriverError("busy")`，不丢已入队的命令。
//! - **Read**：`watch`（容量 1 语义）。worker 忙时新 Read 覆盖待读 source 集
//!   （latest-wins），同一忙期内 N 次 Read 只补一次读；`submit_read` 永不阻塞
//!   采样器。
//! - **SetDemand**：`watch`（容量 1），驱动 Push 订阅起停。
//! - **Stop**：`watch`。worker 在操作边界退出；在途调用经共享 `CancelToken`
//!   协作式取消（E4c），不强杀驱动。
//!
//! **通道布局（有意为之）**：`WorkerCommand::Execute` 走 `mpsc`（有界 32 FIFO）；
//! `Read` / `SetDemand` / `Stop` **不经过** mpsc，分别走容量 1 的独立 `watch`
//! 槽位——这样 worker 忙时新请求仍能覆盖旧请求，且队列满不会阻塞 Stop 或采样器。
//! [`WorkerCommand`] 保留完整命令面以匹配 spec 契约。
//!
//! 调度用 `tokio::select! { biased; ... }`：Execute 优先于 Read。
//! ValueSink 使用非阻塞 `try_send`：满/关闭时丢弃本条读数（下一 tick 会重采），
//! 绝不因下游背压阻塞 worker 的 Execute/Stop。
//!
//! **结果通道（E4b Task 4）**：worker 是驱动的唯一所有者，DataServer 不再能
//! 直接读驱动状态。命令结果、连接状态迁移经 [`WorkerResult`] 上报，由
//! DataServer 的消费任务翻译成领域事件（CommandCompleted/CommandFailed/
//! Connection）。上报使用无界通道且从不等待——worker 的 Execute/Read 绝不因
//! 结果消费端背压而阻塞。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use parking_lot::{Mutex, RwLock};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

use tinyiothub_core::driver::{CancelToken, DriverMode, DriverReading, PropertyChangeSource, ResultValue, ValueSink};
use tinyiothub_core::error::Error;
use tinyiothub_core::models::thing::{Thing, ThingStatus};
use tinyiothub_core::models::thing_command::ThingCommand;

use super::status::ThingOverview;
use super::wrapper::DriverWrapper;

/// Execute 队列容量（spec §2.3 队列上限 32）。
const EXEC_QUEUE_CAP: usize = 32;

/// stop 宽限期：调用在取消后 ≤1s 内退出视为 `cancelled`，否则 `cancelled_unknown`
/// （E4c spec §2.3）。
const CANCEL_GRACE: Duration = Duration::from_secs(1);

/// 当前调用的取消槽（worker 与 [`WorkerHandle`] 共享）。
///
/// worker 在每次 read/execute 前写入新 token 并置 in-flight；调用返回后清空并
/// 读取取消时间。`cancel_current` 只取消当前 token、记录时刻，不打断调用——
/// 驱动自行在检查点协作退出。
#[derive(Clone, Default)]
struct CancelSlot {
    current_token: Arc<Mutex<Option<CancelToken>>>,
    cancel_requested_at: Arc<Mutex<Option<Instant>>>,
    in_flight: Arc<AtomicBool>,
}

impl CancelSlot {
    /// 开始一次调用：安装新 token、清空上一轮的取消时刻、置 in-flight。
    fn begin(&self) -> CancelToken {
        let token = CancelToken::new();
        // 锁序固定为 current_token → cancel_requested_at（与 cancel_current 一致）。
        let mut slot = self.current_token.lock();
        *slot = Some(token.clone());
        *self.cancel_requested_at.lock() = None;
        self.in_flight.store(true, Ordering::SeqCst);
        token
    }

    /// 结束当前调用：原子取走 token、置 in-flight=false，再判定是否取消。
    ///
    /// 取走发生在任何结果处理之前：此后的 `cancel_current` 看到空槽位返回
    /// false，不会把已完成的调用误标为取消。若调用期间 token 被取消，返回自
    /// 首次取消时刻起的耗时（缺时刻时 0）。
    fn finish(&self, token: &CancelToken) -> Option<Duration> {
        let mut slot = self.current_token.lock();
        let current = slot.take();
        self.in_flight.store(false, Ordering::SeqCst);
        let cancelled = current.is_some_and(|installed| installed.is_cancelled()) && token.is_cancelled();
        let requested_at = self.cancel_requested_at.lock().take();
        cancelled.then(|| requested_at.map_or(Duration::ZERO, |at| at.elapsed()))
    }

    /// 取消在途调用；返回是否确有在途调用。
    ///
    /// 重复取消保留首次时刻，避免 elapsed/宽限期被后一次调用重置。
    fn cancel_current(&self) -> bool {
        let slot = self.current_token.lock();
        let Some(token) = slot.as_ref() else {
            return false;
        };
        self.cancel_requested_at.lock().get_or_insert_with(Instant::now);
        token.cancel();
        self.in_flight.load(Ordering::SeqCst)
    }

    fn in_flight(&self) -> bool {
        self.in_flight.load(Ordering::SeqCst)
    }
}

/// worker 命令面（契约见 spec §2.3）。
///
/// `Read` / `SetDemand` / `Stop` 走各自的无阻塞 `watch` 槽位；`Execute` 走
/// 有界 FIFO 队列。枚举保留完整命令面供外部按契约引用。
pub enum WorkerCommand {
    Read {
        sources: Vec<String>,
    },
    Execute {
        cmd: ThingCommand,
    },
    SetDemand {
        sources: Vec<String>,
    },
    /// 软重置驱动状态（DataServer::reset_device_driver）。
    Reset,
    /// 标记离线（DataServer::set_device_offline）。
    SetOffline,
    Stop,
}

/// worker → DataServer 的结果上报（DataServer 据此构造领域事件/缓存状态）。
///
/// worker 持有驱动，事件构造留在 DataServer——两侧只经这个只读结果面对话。
#[derive(Debug)]
pub enum WorkerResult {
    /// 一条命令执行完毕（成功或失败）。
    Command {
        thing_id: String,
        cmd: ThingCommand,
        success: bool,
        elapsed_ms: u64,
        error: Option<String>,
    },
    /// 轮询从离线转为在线（on_connected 等价迁移）。
    Connected { thing_id: String, address: Option<String> },
    /// 轮询从在线转为离线（on_disconnected 等价迁移，终态失败才上报）。
    Disconnected { thing_id: String, reason: Option<String> },
    /// 一次终态读取失败（未产生事件；DataServer 据此刷新缓存状态）。
    ReadFailed { thing_id: String },
    /// 调用返回时 token 已被取消（每次调用至多上报一次）。
    ///
    /// `within_grace` = 自取消时刻起 ≤1s 退出；`elapsed_ms` 为取消到返回的耗时。
    Cancelled {
        thing_id: String,
        within_grace: bool,
        elapsed_ms: u64,
    },
}

/// worker 计数器快照（atomics，无指标后端依赖）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorkerMetrics {
    /// Execute FIFO 中排队待处理的命令数（不含正在执行的）。
    pub queue_depth: usize,
    /// 因 worker 忙而覆盖掉未处理 Read 的次数（latest-wins 合并）。
    pub reads_coalesced: u64,
    /// ValueSink 满/关闭导致丢弃的读数条数。
    pub readings_dropped: u64,
}

/// 每设备 worker 句柄：所有提交方法都不阻塞（Execute 满时返回 busy）。
pub struct WorkerHandle {
    exec_tx: mpsc::Sender<WorkerCommand>,
    read_tx: watch::Sender<Option<Vec<String>>>,
    demand_tx: watch::Sender<Vec<String>>,
    stop_tx: watch::Sender<bool>,
    status: Arc<RwLock<ThingOverview>>,
    last_read_sources: Arc<RwLock<Vec<String>>>,
    is_push: bool,
    join: JoinHandle<()>,
    queue_depth: Arc<AtomicUsize>,
    reads_coalesced: Arc<AtomicU64>,
    readings_dropped: Arc<AtomicU64>,
    /// 已提交但尚未被 worker 取走的 Read 是否占用合并槽位。
    read_pending: Arc<AtomicBool>,
    /// 当前调用的取消槽（E4c stop 协作取消）。
    cancel: CancelSlot,
}

impl WorkerHandle {
    /// 提交一次轮询读取。忙碌期间多次提交只保留最后一次的 source 集。
    ///
    /// 使用 `watch::send_replace`，接收端已退出也不 panic、不阻塞。
    pub fn submit_read(&self, sources: Vec<String>) {
        // 上一次 Read 还没被 worker 取走就被覆盖 → 合并计数。
        if self.read_pending.swap(true, Ordering::Relaxed) {
            self.reads_coalesced.fetch_add(1, Ordering::Relaxed);
        }
        self.read_tx.send_replace(Some(sources));
    }

    /// 提交命令执行。队列（32）已满时返回 `DriverError("busy")`。
    pub fn submit_execute(&self, cmd: ThingCommand) -> Result<(), Error> {
        // 先加再发：避免 worker 抢先取出并减计数导致瞬时下溢。
        self.queue_depth.fetch_add(1, Ordering::Relaxed);
        match self.exec_tx.try_send(WorkerCommand::Execute { cmd }) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.queue_depth.fetch_sub(1, Ordering::Relaxed);
                Err(Error::DriverError("busy".to_string()))
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                self.queue_depth.fetch_sub(1, Ordering::Relaxed);
                Err(Error::DriverError("worker stopped".to_string()))
            }
        }
    }

    /// 软重置驱动状态（排队执行，队列满/已停时返回 false）。
    pub fn reset(&self) -> bool {
        self.enqueue_control(WorkerCommand::Reset)
    }

    /// 标记驱动离线（排队执行，队列满/已停时返回 false）。
    pub fn set_offline(&self) -> bool {
        self.enqueue_control(WorkerCommand::SetOffline)
    }

    fn enqueue_control(&self, command: WorkerCommand) -> bool {
        self.queue_depth.fetch_add(1, Ordering::Relaxed);
        let queued = self.exec_tx.try_send(command).is_ok();
        if !queued {
            self.queue_depth.fetch_sub(1, Ordering::Relaxed);
        }
        queued
    }

    /// 是否为 Push 驱动（Push 由 start_push 上报，不经轮询/Read）。
    pub fn is_push(&self) -> bool {
        self.is_push
    }

    /// 更新 Push 驱动的活跃 source 集（latest-wins，不阻塞）。
    pub fn set_demand(&self, sources: Vec<String>) {
        self.demand_tx.send_replace(sources);
    }

    /// 请求停止：worker 不再接受后续操作；在途调用返回后退出。
    pub fn stop(&self) {
        self.stop_tx.send_replace(true);
    }

    /// 取消当前在途调用（E4c stop）：置当前 `CancelToken` 并记录取消时刻。
    ///
    /// 返回是否确有在途调用；不打断调用本身——驱动在检查点协作退出。调用返回
    /// 后 worker 会上报 [`WorkerResult::Cancelled`]。
    pub fn cancel_current(&self) -> bool {
        self.cancel.cancel_current()
    }

    /// 是否有 read/execute 正在驱动内执行。
    pub fn in_flight(&self) -> bool {
        self.cancel.in_flight()
    }

    /// 当前设备状态快照。
    pub fn status(&self) -> ThingOverview {
        self.status.read().clone()
    }

    /// 最近一次实际执行的 Read 的 source 集（诊断/指标用）。
    pub fn last_read_sources(&self) -> Vec<String> {
        self.last_read_sources.read().clone()
    }

    /// 当前计数器快照。
    pub fn metrics(&self) -> WorkerMetrics {
        WorkerMetrics {
            queue_depth: self.queue_depth(),
            reads_coalesced: self.reads_coalesced(),
            readings_dropped: self.readings_dropped(),
        }
    }

    /// Execute FIFO 中排队待处理的命令数。
    pub fn queue_depth(&self) -> usize {
        self.queue_depth.load(Ordering::Relaxed)
    }

    /// 因 worker 忙而合并掉的 Read 次数。
    pub fn reads_coalesced(&self) -> u64 {
        self.reads_coalesced.load(Ordering::Relaxed)
    }

    /// ValueSink 满/关闭丢弃的读数条数。
    pub fn readings_dropped(&self) -> u64 {
        self.readings_dropped.load(Ordering::Relaxed)
    }

    /// worker 任务是否已退出。
    pub fn is_finished(&self) -> bool {
        self.join.is_finished()
    }
}

/// 为单个物创建独占驱动的 worker 任务。
///
/// Push 驱动在 spawn 时调用 `start_push(value_sink, events)`；Poll 驱动等待
/// `submit_read` 调度。
pub fn spawn_worker(
    thing: Thing,
    mut driver: DriverWrapper,
    value_sink: ValueSink,
    events: Arc<dyn PropertyChangeSource>,
    results: mpsc::UnboundedSender<WorkerResult>,
) -> WorkerHandle {
    let is_push = driver.inner_driver().mode() == DriverMode::Push;
    if is_push {
        driver.inner_driver_mut().start_push(value_sink.clone(), events);
    }

    let (exec_tx, exec_rx) = mpsc::channel(EXEC_QUEUE_CAP);
    let (read_tx, read_rx) = watch::channel(None::<Vec<String>>);
    let (demand_tx, demand_rx) = watch::channel(Vec::new());
    let (stop_tx, stop_rx) = watch::channel(false);

    let status = Arc::new(RwLock::new(ThingOverview::new(&thing)));
    let last_read_sources = Arc::new(RwLock::new(Vec::new()));
    let queue_depth = Arc::new(AtomicUsize::new(0));
    let reads_coalesced = Arc::new(AtomicU64::new(0));
    let readings_dropped = Arc::new(AtomicU64::new(0));
    let read_pending = Arc::new(AtomicBool::new(false));
    let cancel = CancelSlot::default();
    let thing_id = thing.id.clone();

    let join = tokio::spawn(run_worker(
        driver,
        thing_id,
        value_sink,
        is_push,
        exec_rx,
        read_rx,
        demand_rx,
        stop_rx,
        results,
        status.clone(),
        last_read_sources.clone(),
        WorkerCounters {
            queue_depth: queue_depth.clone(),
            readings_dropped: readings_dropped.clone(),
            read_pending: read_pending.clone(),
        },
        cancel.clone(),
    ));

    WorkerHandle {
        exec_tx,
        read_tx,
        demand_tx,
        stop_tx,
        status,
        last_read_sources,
        is_push,
        join,
        queue_depth,
        reads_coalesced,
        readings_dropped,
        read_pending,
        cancel,
    }
}

/// worker 任务侧共享的计数器（句柄侧另有 coalesced 计数）。
struct WorkerCounters {
    queue_depth: Arc<AtomicUsize>,
    readings_dropped: Arc<AtomicU64>,
    read_pending: Arc<AtomicBool>,
}

#[allow(clippy::too_many_arguments)]
async fn run_worker(
    mut driver: DriverWrapper,
    thing_id: String,
    value_sink: ValueSink,
    is_push: bool,
    mut exec_rx: mpsc::Receiver<WorkerCommand>,
    mut read_rx: watch::Receiver<Option<Vec<String>>>,
    mut demand_rx: watch::Receiver<Vec<String>>,
    mut stop_rx: watch::Receiver<bool>,
    results: mpsc::UnboundedSender<WorkerResult>,
    status: Arc<RwLock<ThingOverview>>,
    last_read_sources: Arc<RwLock<Vec<String>>>,
    counters: WorkerCounters,
    cancel: CancelSlot,
) {
    // 失败 streak 状态：终态失败只上报一次（否则 retry timeout 前每 100ms 刷
    // 一条 ReadFailed/告警）；恢复时上报一次 Connected 事件。
    let mut failing = false;
    loop {
        tokio::select! {
            biased;

            // Stop 最先观察：只停止后续操作；在途调用在 E4c 接取消。
            res = stop_rx.changed() => {
                if res.is_err() || *stop_rx.borrow_and_update() {
                    break;
                }
            }

            // Execute 优先于 Read（spec §2.3）。
            cmd = exec_rx.recv() => {
                if cmd.is_some() {
                    counters.queue_depth.fetch_sub(1, Ordering::Relaxed);
                }
                match cmd {
                    Some(WorkerCommand::Execute { cmd }) => {
                        execute_and_report(&mut driver, &cmd, &results, &cancel);
                        *status.write() = driver.overview();
                    }
                    Some(WorkerCommand::Reset) => {
                        driver.reset();
                        *status.write() = driver.overview();
                    }
                    Some(WorkerCommand::SetOffline) => {
                        driver.set_offline();
                        *status.write() = driver.overview();
                    }
                    // Read/SetDemand 的契约路径是各自的无阻塞 watch 槽位。
                    Some(WorkerCommand::Read { .. } | WorkerCommand::SetDemand { .. }) => {}
                    Some(WorkerCommand::Stop) | None => break,
                }
            }

            res = read_rx.changed() => {
                if res.is_err() {
                    break;
                }
                counters.read_pending.store(false, Ordering::Relaxed);
                let sources = read_rx.borrow_and_update().clone().unwrap_or_default();
                // Push 驱动由 start_push 上报，Read 不触发轮询。
                if is_push {
                    continue;
                }
                last_read_sources.write().clone_from(&sources);
                // 重试退避由 DriverWrapper::execute_once 内部裁决：终态失败后的
                // 下一次 Read 仍会进入（否则 max_attempts 用尽就永远无法恢复），
                // 退避期内 execute_once 返回 Retrying → 不上报。
                let was_online = driver.is_online();
                // 每次读使用新 token；调用期间 stop 可通过 WorkerHandle::cancel_current
                // 取消它，返回后至多上报一次 Cancelled。
                let token = cancel.begin();
                let outcome = driver.read_data_once_with(&token);
                if report_if_cancelled(&cancel, &token, &thing_id, &results) {
                    // 已取消：本次调用只有 Cancelled 一个结果。
                    continue;
                }
                match outcome.result {
                    Ok(values) => {
                        let address = driver.thing().address.clone();
                        apply_read_values(&mut driver, &values, true);
                        *status.write() = driver.overview();
                        if failing {
                            failing = false;
                            tracing::info!(thing_id = %thing_id, "driver read recovered");
                        }
                        if !was_online {
                            let _ = results.send(WorkerResult::Connected {
                                thing_id: thing_id.clone(),
                                address,
                            });
                        }
                        send_reading(
                            &value_sink,
                            &counters.readings_dropped,
                            DriverReading {
                                thing_id: thing_id.clone(),
                                values,
                            },
                        );
                    }
                    Err(err) => {
                        // 重试中的失败由驱动自身负责；只有终态失败才迁移状态。
                        if outcome.retry_info.as_ref().is_some_and(|info| info.will_retry) {
                            continue;
                        }
                        apply_read_values(&mut driver, &[], false);
                        *status.write() = driver.overview();
                        // 同一失败 streak 只在进入时上报/告警一次；读取仍每 tick
                        // 重试，恢复后 Connected 恰好一次。
                        if !failing {
                            failing = true;
                            tracing::warn!(
                                thing_id = %thing_id,
                                error = %err,
                                elapsed_ms = outcome.elapsed.as_millis() as u64,
                                "driver read failed"
                            );
                            if was_online {
                                let _ = results.send(WorkerResult::Disconnected {
                                    thing_id: thing_id.clone(),
                                    reason: Some(err.to_string()),
                                });
                            }
                            let _ = results.send(WorkerResult::ReadFailed {
                                thing_id: thing_id.clone(),
                            });
                        }
                    }
                }
            }

            res = demand_rx.changed() => {
                if res.is_err() {
                    break;
                }
                let sources = demand_rx.borrow_and_update().clone();
                driver.inner_driver_mut().set_demand(&sources);
            }
        }
    }

    // 停止后未处理的命令不会再执行；队列深度归零以免指标长期悬空。
    counters.queue_depth.store(0, Ordering::Relaxed);
    tracing::debug!(thing_id = %thing_id, "driver worker stopped");
}

/// 把一批读数写回驱动自己的 `Thing`（worker 是驱动单写者）。
///
/// 与 `DataServer::apply_property_values` 同语义：按属性名匹配、只写
/// `current_value`（`set_current_value` 同时刷新 `updated_at`）；成功轮询另
/// 刷新 `last_heartbeat` 与在线状态。worker 不发布事件——事件仍由 DataServer
/// 的共享管线在消费 ValueSink / WorkerResult 时产生。
fn apply_read_values(driver: &mut DriverWrapper, values: &[ResultValue], online: bool) {
    let mut thing = driver.thing().clone();
    if let Some(properties) = thing.properties.as_mut() {
        for property in properties.iter_mut() {
            if let Some(result) = values.iter().find(|v| v.name == property.name)
                && let Some(value) = result.value.as_ref()
            {
                property.set_current_value(value.clone());
            }
        }
    }
    thing.status = if online {
        thing.last_heartbeat = Some(chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string());
        ThingStatus::Online
    } else {
        ThingStatus::Offline
    };
    *driver.thing_mut() = thing;
}

/// 非阻塞上报：sink 满/关闭时丢弃本条读数并告警（下一次采样会补）。
fn send_reading(value_sink: &ValueSink, dropped: &AtomicU64, reading: DriverReading) {
    let thing_id = reading.thing_id.clone();
    match value_sink.try_send(reading) {
        Ok(()) => {}
        Err(mpsc::error::TrySendError::Full(_)) => {
            dropped.fetch_add(1, Ordering::Relaxed);
            tracing::warn!(thing_id = %thing_id, "driver worker value sink full; dropping reading");
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {
            dropped.fetch_add(1, Ordering::Relaxed);
            tracing::warn!(thing_id = %thing_id, "driver worker value sink closed; dropping reading");
        }
    }
}

/// 调用返回后结束取消槽；若期间被取消，上报一次 `Cancelled` 并返回 true。
///
/// 调用方必须在 true 时直接结束本次调用处理——一次调用只有一个结果，取消的
/// 调用不得再产生读数、Connected/Disconnected、ReadFailed 或 Command。
fn report_if_cancelled(
    cancel: &CancelSlot,
    token: &CancelToken,
    thing_id: &str,
    results: &mpsc::UnboundedSender<WorkerResult>,
) -> bool {
    let Some(elapsed) = cancel.finish(token) else {
        return false;
    };
    let _ = results.send(WorkerResult::Cancelled {
        thing_id: thing_id.to_string(),
        within_grace: elapsed <= CANCEL_GRACE,
        elapsed_ms: elapsed.as_millis() as u64,
    });
    true
}

/// 执行一条命令并把结果上报给 DataServer（事件构造/发布归 DataServer）。
fn execute_and_report(
    driver: &mut DriverWrapper,
    cmd: &ThingCommand,
    results: &mpsc::UnboundedSender<WorkerResult>,
    cancel: &CancelSlot,
) {
    let thing_id = driver.thing().id.clone();
    let token = cancel.begin();
    let outcome = driver.execute_command_with(cmd, &token);
    if report_if_cancelled(cancel, &token, &thing_id, results) {
        // 已取消：只上报 Cancelled，不再产生 Command 结果。
        return;
    }
    let success = outcome.result.is_ok();
    let error = outcome.result.as_ref().err().map(|err| err.to_string());
    let elapsed_ms = outcome.elapsed.as_millis() as u64;
    if let Some(ref err) = error {
        tracing::warn!(
            thing_id = %thing_id,
            command = %cmd.name,
            error = %err,
            elapsed_ms = elapsed_ms,
            "driver command failed"
        );
    }
    let _ = results.send(WorkerResult::Command {
        thing_id,
        cmd: cmd.clone(),
        success,
        elapsed_ms,
        error,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Mutex, mpsc as std_mpsc};
    use std::time::Duration;

    use tokio::sync::Notify;

    use tinyiothub_core::driver::{ResultValue, RetryConfig, ThingDriver};
    use tinyiothub_core::models::thing_property::ThingProperty;

    #[derive(Default)]
    struct FakeLog {
        reads: AtomicUsize,
        executes: AtomicUsize,
        push_starts: AtomicUsize,
        /// 接下来 N 次读返回 Err。
        fail_next_reads: AtomicUsize,
        events: Mutex<Vec<String>>,
        /// 假驱动在 execute 时自报的 thing 属性当前值（验证 worker 写回）。
        observed_value: Mutex<Option<String>>,
        read_started: Notify,
        execute_started: Notify,
    }

    /// 同步阻塞的假驱动：`read_data` 先记录并通知，再等测试放行。
    struct FakeDriver {
        thing: Thing,
        mode: DriverMode,
        log: Arc<FakeLog>,
        release_rx: Mutex<std_mpsc::Receiver<()>>,
    }

    impl ThingDriver for FakeDriver {
        fn thing(&self) -> &Thing {
            &self.thing
        }

        fn thing_mut(&mut self) -> &mut Thing {
            &mut self.thing
        }

        fn mode(&self) -> DriverMode {
            self.mode
        }

        fn start_push(&mut self, _sink: ValueSink, _events: Arc<dyn PropertyChangeSource>) {
            self.log.push_starts.fetch_add(1, Ordering::SeqCst);
        }

        /// 单次失败即终态（max_attempts=1）且 1ms 超时：失败立即可见，
        /// 下一次操作会重置重试状态。单测不必等默认的 500ms/300s。
        fn retry_config(&self) -> RetryConfig {
            RetryConfig {
                max_attempts: 1,
                base_interval: Duration::from_millis(1),
                max_interval: Duration::from_millis(1),
                timeout: Duration::from_millis(1),
                ..RetryConfig::default()
            }
        }

        fn read_data(&mut self, cancel: &CancelToken) -> Result<Vec<ResultValue>, Error> {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            self.log.reads.fetch_add(1, Ordering::SeqCst);
            self.log.events.lock().unwrap().push("read".to_string());
            self.log.read_started.notify_one();
            // 放行令牌未到就阻塞；sender 已 drop 时视为放行。
            let _ = self.release_rx.lock().unwrap().recv();

            if self.log.fail_next_reads.load(Ordering::SeqCst) > 0 {
                self.log.fail_next_reads.fetch_sub(1, Ordering::SeqCst);
                return Err(Error::IOError("fake read failure".to_string()));
            }
            Ok(vec![ResultValue::new(
                "temp".to_string(),
                "float".to_string(),
                Some("42".to_string()),
            )])
        }

        fn execute_command(&mut self, _cmd: &ThingCommand, cancel: &CancelToken) -> Result<bool, Error> {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            // 先写观测值再递增计数：测试以 executes 计数为屏障读取 observed_value。
            *self.log.observed_value.lock().unwrap() = self
                .thing
                .properties
                .as_ref()
                .and_then(|properties| properties.iter().find(|p| p.name == "temp"))
                .and_then(|p| p.current_value.clone());
            self.log.events.lock().unwrap().push("execute".to_string());
            self.log.executes.fetch_add(1, Ordering::SeqCst);
            Ok(true)
        }
    }

    struct FakeEvents(tokio::sync::broadcast::Sender<tinyiothub_core::models::event::Event>);

    impl FakeEvents {
        fn new() -> Self {
            Self(tokio::sync::broadcast::channel(1).0)
        }
    }

    impl PropertyChangeSource for FakeEvents {
        fn subscribe(&self) -> tokio::sync::broadcast::Receiver<tinyiothub_core::models::event::Event> {
            self.0.subscribe()
        }
    }

    struct Harness {
        handle: WorkerHandle,
        log: Arc<FakeLog>,
        release: std_mpsc::Sender<()>,
        sink_tx: ValueSink,
        sink_rx: mpsc::Receiver<DriverReading>,
        result_rx: mpsc::UnboundedReceiver<WorkerResult>,
    }

    fn make(mode: DriverMode) -> Harness {
        make_with_cap(mode, 16)
    }

    fn make_with_cap(mode: DriverMode, cap: usize) -> Harness {
        let (release, release_rx) = std_mpsc::channel();
        let log = Arc::new(FakeLog::default());
        let property = ThingProperty {
            thing_id: "thing-1".to_string(),
            name: "temp".to_string(),
            ..ThingProperty::default()
        };
        let thing = Thing {
            id: "thing-1".to_string(),
            name: "thing-1".to_string(),
            properties: Some(vec![property]),
            ..Thing::default()
        };
        let driver = DriverWrapper::new(Box::new(FakeDriver {
            thing: thing.clone(),
            mode,
            log: log.clone(),
            release_rx: Mutex::new(release_rx),
        }));
        let (sink_tx, sink_rx) = mpsc::channel(cap);
        let (result_tx, result_rx) = mpsc::unbounded_channel();
        let handle = spawn_worker(thing, driver, sink_tx.clone(), Arc::new(FakeEvents::new()), result_tx);
        Harness {
            handle,
            log,
            release,
            sink_tx,
            sink_rx,
            result_rx,
        }
    }

    #[derive(Clone, Copy)]
    struct CancelOptions {
        /// true：等待放行后检查 token；false：忽略 token 阻塞 `block` 时长。
        cooperative: bool,
        block: Duration,
        /// execute 也阻塞等待放行（验证在途命令取消）。
        gate_execute: bool,
        max_attempts: u32,
        retry_interval: Duration,
    }

    impl Default for CancelOptions {
        fn default() -> Self {
            Self {
                cooperative: true,
                block: Duration::ZERO,
                gate_execute: false,
                max_attempts: 1,
                retry_interval: Duration::from_millis(1),
            }
        }
    }

    /// 取消语义测试驱动：read 在测试放行/固定时长后才返回。
    struct CancelDriver {
        thing: Thing,
        log: Arc<FakeLog>,
        release_rx: Mutex<std_mpsc::Receiver<()>>,
        options: CancelOptions,
    }

    impl ThingDriver for CancelDriver {
        fn thing(&self) -> &Thing {
            &self.thing
        }

        fn thing_mut(&mut self) -> &mut Thing {
            &mut self.thing
        }

        fn mode(&self) -> DriverMode {
            DriverMode::Poll
        }

        /// 长超时：阻塞读不能被重试层的短超时提前截断。
        fn retry_config(&self) -> RetryConfig {
            RetryConfig {
                max_attempts: self.options.max_attempts,
                base_interval: self.options.retry_interval,
                max_interval: self.options.retry_interval,
                timeout: Duration::from_secs(300),
                ..RetryConfig::default()
            }
        }

        fn read_data(&mut self, cancel: &CancelToken) -> Result<Vec<ResultValue>, Error> {
            self.log.reads.fetch_add(1, Ordering::SeqCst);
            self.log.read_started.notify_one();
            if self.options.cooperative {
                // 长 IO 阻塞点：测试放行后检查 token 并协作退出。
                let _ = self.release_rx.lock().unwrap().recv();
                if cancel.is_cancelled() {
                    return Err(Error::Cancelled);
                }
            } else {
                // 不合作驱动：完全忽略 token，直到阻塞时长结束。
                std::thread::sleep(self.options.block);
            }
            Ok(vec![ResultValue::new(
                "temp".to_string(),
                "float".to_string(),
                Some("42".to_string()),
            )])
        }

        fn execute_command(&mut self, _cmd: &ThingCommand, cancel: &CancelToken) -> Result<bool, Error> {
            self.log.execute_started.notify_one();
            if self.options.gate_execute {
                // 在途命令：测试放行后检查 token。
                let _ = self.release_rx.lock().unwrap().recv();
            }
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            self.log.executes.fetch_add(1, Ordering::SeqCst);
            Ok(true)
        }
    }

    fn make_cancel(cooperative: bool, block: Duration) -> Harness {
        make_cancel_with(CancelOptions {
            cooperative,
            block,
            ..CancelOptions::default()
        })
    }

    fn make_cancel_with(options: CancelOptions) -> Harness {
        let (release, release_rx) = std_mpsc::channel();
        let log = Arc::new(FakeLog::default());
        let property = ThingProperty {
            thing_id: "thing-1".to_string(),
            name: "temp".to_string(),
            ..ThingProperty::default()
        };
        let thing = Thing {
            id: "thing-1".to_string(),
            name: "thing-1".to_string(),
            properties: Some(vec![property]),
            ..Thing::default()
        };
        let driver = DriverWrapper::new(Box::new(CancelDriver {
            thing: thing.clone(),
            log: log.clone(),
            release_rx: Mutex::new(release_rx),
            options,
        }));
        let (sink_tx, sink_rx) = mpsc::channel(16);
        let (result_tx, result_rx) = mpsc::unbounded_channel();
        let handle = spawn_worker(thing, driver, sink_tx.clone(), Arc::new(FakeEvents::new()), result_tx);
        Harness {
            handle,
            log,
            release,
            sink_tx,
            sink_rx,
            result_rx,
        }
    }

    fn cmd(id: usize) -> ThingCommand {
        ThingCommand {
            id: format!("cmd-{id}"),
            thing_id: "thing-1".to_string(),
            name: "ping".to_string(),
            display_name: None,
            description: None,
            parameters: None,
            created_at: String::new(),
        }
    }

    async fn wait_finished(handle: &WorkerHandle) {
        wait_until(|| handle.is_finished()).await;
    }

    async fn wait_until(mut condition: impl FnMut() -> bool) {
        tokio::time::timeout(Duration::from_secs(2), async {
            while !condition() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("condition should become true");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn two_reads_while_busy_coalesce_to_latest_source_set() {
        let h = make(DriverMode::Poll);

        h.handle.submit_read(vec!["a".to_string()]);
        h.log.read_started.notified().await; // 第一次读已开始并阻塞

        h.handle.submit_read(vec!["b".to_string()]);
        h.handle.submit_read(vec!["c".to_string()]);
        assert_eq!(h.handle.reads_coalesced(), 1, "覆盖未处理的 Read 计一次合并");
        h.release.send(()).unwrap(); // 放行第一次读

        h.log.read_started.notified().await; // 合并后只补一次读
        assert_eq!(h.log.reads.load(Ordering::SeqCst), 2);
        assert_eq!(h.handle.last_read_sources(), vec!["c".to_string()]);

        h.release.send(()).unwrap();
        h.handle.stop();
        wait_finished(&h.handle).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn execute_is_served_before_pending_read() {
        let h = make(DriverMode::Poll);

        h.handle.submit_read(vec!["a".to_string()]);
        h.log.read_started.notified().await;

        // 在 worker 忙时同时挂起一次 Read 与一次 Execute。
        h.handle.submit_read(vec!["b".to_string()]);
        h.handle.submit_execute(cmd(0)).expect("queue has room");
        h.release.send(()).unwrap();

        h.log.read_started.notified().await;
        assert_eq!(
            *h.log.events.lock().unwrap(),
            vec!["read".to_string(), "execute".to_string(), "read".to_string()]
        );
        assert_eq!(h.log.executes.load(Ordering::SeqCst), 1);

        h.release.send(()).unwrap();
        h.handle.stop();
        wait_finished(&h.handle).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn exec_queue_overflow_returns_busy() {
        let h = make(DriverMode::Poll);

        h.handle.submit_read(vec!["a".to_string()]);
        h.log.read_started.notified().await; // worker 忙，Execute 只能排队

        for i in 0..EXEC_QUEUE_CAP {
            h.handle.submit_execute(cmd(i)).expect("queue below cap must accept");
        }
        assert_eq!(h.handle.queue_depth(), EXEC_QUEUE_CAP, "队列深度反映未处理命令");
        match h.handle.submit_execute(cmd(EXEC_QUEUE_CAP)) {
            Err(Error::DriverError(msg)) => assert_eq!(msg, "busy"),
            Err(other) => panic!("expected busy, got {other}"),
            Ok(()) => panic!("expected busy, got Ok"),
        }
        assert_eq!(h.handle.queue_depth(), EXEC_QUEUE_CAP, "busy 拒绝不得改变队列深度");

        // 放行并让 32 条命令全部执行完。
        h.release.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while h.log.executes.load(Ordering::SeqCst) < EXEC_QUEUE_CAP {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("queued commands should all run");
        wait_until(|| h.handle.queue_depth() == 0).await;

        h.handle.stop();
        wait_finished(&h.handle).await;
    }

    #[tokio::test]
    async fn stop_ends_task_and_late_sends_do_not_panic() {
        let h = make(DriverMode::Poll);

        h.handle.stop();
        wait_finished(&h.handle).await;

        // 停止后的提交不得 panic；Execute 返回错误而不是静默丢弃。
        h.handle.submit_read(vec!["x".to_string()]);
        h.handle.set_demand(vec!["y".to_string()]);
        h.handle.stop();
        assert!(matches!(h.handle.submit_execute(cmd(0)), Err(Error::DriverError(_))));
    }

    #[tokio::test]
    async fn push_driver_starts_push_on_spawn() {
        let h = make(DriverMode::Push);
        assert_eq!(h.log.push_starts.load(Ordering::SeqCst), 1);

        // Push 驱动的 Read 不触发轮询。
        h.handle.submit_read(vec!["telemetry".to_string()]);
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(h.log.reads.load(Ordering::SeqCst), 0);

        h.handle.stop();
        wait_finished(&h.handle).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn successful_read_applies_values_to_driver_thing() {
        let mut h = make(DriverMode::Poll);

        h.handle.submit_read(vec!["telemetry".to_string()]);
        h.log.read_started.notified().await;
        h.release.send(()).unwrap();

        // 读数到达 sink 时 worker 已写回 Thing 并刷新 overview。
        let reading = h.sink_rx.recv().await.expect("reading should reach sink");
        assert_eq!(reading.values.first().and_then(|v| v.value.as_deref()), Some("42"));
        assert_eq!(h.handle.status().health.total_successes, 1);

        // 假驱动在下一次 execute 时自报它看到的属性值，证明 thing_mut 已写回。
        h.handle.submit_execute(cmd(0)).expect("queue has room");
        wait_until(|| h.log.executes.load(Ordering::SeqCst) == 1).await;
        assert_eq!(h.log.observed_value.lock().unwrap().clone(), Some("42".to_string()));

        h.handle.stop();
        wait_finished(&h.handle).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn command_and_connection_transitions_are_reported() {
        let mut h = make(DriverMode::Poll);

        // 首次成功读取：离线→在线迁移上报 Connected，随后读数进 sink。
        h.handle.submit_read(vec!["a".to_string()]);
        h.log.read_started.notified().await;
        h.release.send(()).unwrap();
        match h.result_rx.recv().await.expect("result") {
            WorkerResult::Connected { thing_id, .. } => assert_eq!(thing_id, "thing-1"),
            other => panic!("expected Connected, got {other:?}"),
        }

        // Execute：命令结果（成功/耗时）上报给 DataServer。
        h.handle.submit_execute(cmd(0)).expect("queue has room");
        loop {
            match h.result_rx.recv().await.expect("result") {
                WorkerResult::Command {
                    thing_id,
                    cmd: reported,
                    success,
                    error,
                    ..
                } => {
                    assert_eq!(thing_id, "thing-1");
                    assert_eq!(reported.id, "cmd-0");
                    assert!(success);
                    assert!(error.is_none());
                    break;
                }
                _ => continue,
            }
        }

        // 终态失败：在线→离线迁移上报 Disconnected，并附带 ReadFailed。
        h.log.fail_next_reads.store(1, Ordering::SeqCst);
        h.handle.submit_read(vec!["b".to_string()]);
        h.log.read_started.notified().await;
        h.release.send(()).unwrap();
        let mut kinds = Vec::new();
        for _ in 0..2 {
            match h.result_rx.recv().await.expect("result") {
                WorkerResult::Disconnected { .. } => kinds.push("disconnected"),
                WorkerResult::ReadFailed { .. } => kinds.push("read_failed"),
                other => panic!("unexpected result {other:?}"),
            }
        }
        assert_eq!(kinds, vec!["disconnected", "read_failed"]);

        h.handle.stop();
        wait_finished(&h.handle).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn full_value_sink_does_not_block_execute() {
        let mut h = make_with_cap(DriverMode::Poll, 1);
        h.sink_tx
            .try_send(DriverReading {
                thing_id: "filler".to_string(),
                values: vec![],
            })
            .expect("filler fills the only slot");

        h.handle.submit_read(vec!["a".to_string()]);
        h.log.read_started.notified().await;
        h.release.send(()).unwrap();

        // sink 已满：worker 的 try_send 丢弃读数并继续，Execute 不会被背压卡住。
        wait_until(|| h.log.reads.load(Ordering::SeqCst) == 1).await;
        h.handle.submit_execute(cmd(0)).expect("queue has room");
        wait_until(|| h.log.executes.load(Ordering::SeqCst) == 1).await;
        wait_until(|| h.handle.readings_dropped() == 1).await;

        assert_eq!(h.sink_rx.try_recv().unwrap().thing_id, "filler");
        assert!(h.sink_rx.try_recv().is_err(), "dropped reading must not appear");
        assert_eq!(h.handle.metrics().readings_dropped, 1);

        h.handle.stop();
        wait_finished(&h.handle).await;
    }

    /// 失败 streak：终态失败只上报一次 ReadFailed（不因 100ms 重试刷屏），
    /// 恢复时恰好一次 Connected + 正常读数。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn failure_streak_emits_read_failed_once_and_recovers() {
        let mut h = make(DriverMode::Poll);
        h.log.fail_next_reads.store(100, Ordering::SeqCst);

        // 首次终态失败：一次 ReadFailed（此前从未在线 → 无 Disconnected）。
        h.handle.submit_read(vec!["a".to_string()]);
        h.log.read_started.notified().await;
        h.release.send(()).unwrap();
        match h.result_rx.recv().await.expect("first result") {
            WorkerResult::ReadFailed { thing_id } => assert_eq!(thing_id, "thing-1"),
            other => panic!("expected ReadFailed, got {other:?}"),
        }

        // 同一 streak 内的下一次读取仍然执行（可恢复），但不再上报。
        tokio::time::sleep(Duration::from_millis(20)).await;
        h.handle.submit_read(vec!["a".to_string()]);
        h.log.read_started.notified().await;
        h.release.send(()).unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(150), h.result_rx.recv())
                .await
                .is_err(),
            "failure streak must emit ReadFailed only once"
        );

        // 恢复：一次 Connected + 一条读数进 sink。
        h.log.fail_next_reads.store(0, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(20)).await;
        h.handle.submit_read(vec!["a".to_string()]);
        h.log.read_started.notified().await;
        h.release.send(()).unwrap();
        match h.result_rx.recv().await.expect("recovery result") {
            WorkerResult::Connected { thing_id, .. } => assert_eq!(thing_id, "thing-1"),
            other => panic!("expected Connected, got {other:?}"),
        }
        let reading = h.sink_rx.recv().await.expect("recovered reading");
        assert_eq!(reading.values.first().and_then(|v| v.value.as_deref()), Some("42"));

        h.handle.stop();
        wait_finished(&h.handle).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn failed_read_sends_no_reading_and_worker_stays_live() {
        let mut h = make(DriverMode::Poll);
        h.log.fail_next_reads.store(1, Ordering::SeqCst);

        h.handle.submit_read(vec!["a".to_string()]);
        h.log.read_started.notified().await;
        h.release.send(()).unwrap();

        // 失败刷新 status，但不向 sink 发读数。
        wait_until(|| h.handle.status().health.total_failures == 1).await;
        assert!(h.sink_rx.try_recv().is_err(), "failed read must not emit");

        // 退避（假驱动 1ms）过后 worker 仍能读；随后 Execute 也仍可执行。
        tokio::time::sleep(Duration::from_millis(20)).await;
        h.handle.submit_read(vec!["b".to_string()]);
        h.log.read_started.notified().await;
        h.release.send(()).unwrap();
        let reading = h.sink_rx.recv().await.expect("second read should reach sink");
        assert_eq!(reading.values.first().and_then(|v| v.value.as_deref()), Some("42"));

        h.handle.submit_execute(cmd(0)).expect("queue has room");
        wait_until(|| h.log.executes.load(Ordering::SeqCst) == 1).await;

        h.handle.stop();
        wait_finished(&h.handle).await;
    }

    /// 协作式长读：cancel_current 命中在途调用，驱动退出后上报唯一结果
    /// `Cancelled`（不再产生读数/Connected/Disconnected/ReadFailed）。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancel_current_cancels_cooperative_read_within_grace() {
        let mut h = make_cancel(true, Duration::ZERO);

        // 先成功读一次 → 设备在线；若取消调用误走失败分支会产生 Disconnected。
        h.handle.submit_read(vec!["a".to_string()]);
        h.log.read_started.notified().await;
        h.release.send(()).unwrap();
        match h.result_rx.recv().await.expect("first read result") {
            WorkerResult::Connected { .. } => {}
            other => panic!("expected Connected, got {other:?}"),
        }
        let first = h.sink_rx.recv().await.expect("first reading");
        assert_eq!(first.values.first().and_then(|v| v.value.as_deref()), Some("42"));

        h.handle.submit_read(vec!["a".to_string()]);
        h.log.read_started.notified().await; // 驱动已进入长读
        assert!(h.handle.in_flight());
        assert!(h.handle.cancel_current(), "在途调用应报告 true");

        h.release.send(()).unwrap(); // 放行：驱动检查 token 后协作退出
        match h.result_rx.recv().await.expect("cancelled result") {
            WorkerResult::Cancelled {
                thing_id,
                within_grace,
                elapsed_ms,
            } => {
                assert_eq!(thing_id, "thing-1");
                assert!(within_grace, "cooperative driver must exit within grace");
                assert!(elapsed_ms <= 1000);
            }
            other => panic!("expected Cancelled, got {other:?}"),
        }
        wait_until(|| !h.handle.in_flight()).await;
        assert!(!h.handle.cancel_current(), "调用结束后不应报告在途");

        h.handle.stop();
        wait_finished(&h.handle).await;
        assert!(
            h.result_rx.try_recv().is_err(),
            "cancelled read must be the only outcome (no Disconnected/ReadFailed)"
        );
        assert!(h.sink_rx.try_recv().is_err(), "cancelled read must not emit a reading");
    }

    /// 不合作驱动阻塞超过宽限期：迟到返回后仍上报一次 cancelled_unknown，
    /// 且不再补发正常结果。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn non_cooperative_read_reports_cancelled_unknown_after_return() {
        let mut h = make_cancel(false, Duration::from_millis(1200));

        h.handle.submit_read(vec!["a".to_string()]);
        h.log.read_started.notified().await;
        assert!(h.handle.in_flight());
        assert!(h.handle.cancel_current());

        let result = tokio::time::timeout(Duration::from_secs(5), h.result_rx.recv())
            .await
            .expect("cancelled result must arrive after driver returns")
            .expect("result");
        match result {
            WorkerResult::Cancelled {
                within_grace,
                elapsed_ms,
                ..
            } => {
                assert!(!within_grace, "blocking driver exceeded grace");
                assert!(elapsed_ms >= 1000, "elapsed_ms={elapsed_ms}");
            }
            other => panic!("expected Cancelled, got {other:?}"),
        }
        wait_until(|| !h.handle.in_flight()).await;

        h.handle.stop();
        wait_finished(&h.handle).await;
        assert!(
            h.result_rx.try_recv().is_err(),
            "cancelled read must not also emit Connected/ReadFailed"
        );
        assert!(h.sink_rx.try_recv().is_err(), "cancelled read must not emit a reading");
    }

    /// 空闲时取消：返回 false，且后续正常调用前不得出现 Cancelled。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancel_current_when_idle_returns_false_and_emits_nothing() {
        let mut h = make_cancel(true, Duration::ZERO);
        assert!(!h.handle.in_flight());
        assert!(!h.handle.cancel_current());

        h.handle.submit_read(vec!["a".to_string()]);
        h.log.read_started.notified().await;
        h.release.send(()).unwrap();
        // 首次成功读取上报 Connected；空闲取消若误发了 Cancelled，会排在它前面。
        match h.result_rx.recv().await.expect("read result") {
            WorkerResult::Connected { thing_id, .. } => assert_eq!(thing_id, "thing-1"),
            other => panic!("idle cancel must not emit Cancelled, got {other:?}"),
        }
        assert!(h.result_rx.try_recv().is_err(), "unexpected extra result");

        h.handle.stop();
        wait_finished(&h.handle).await;
    }

    /// 未被取消的成功调用不产生 Cancelled。
    #[tokio::test]
    async fn uncancelled_execute_emits_no_cancelled_result() {
        let mut h = make_cancel(true, Duration::ZERO);

        h.handle.submit_execute(cmd(0)).expect("queue has room");
        match h.result_rx.recv().await.expect("command result") {
            WorkerResult::Command { success, error, .. } => {
                assert!(success);
                assert!(error.is_none());
            }
            other => panic!("expected Command, got {other:?}"),
        }
        assert!(h.result_rx.try_recv().is_err(), "unexpected extra result");

        h.handle.stop();
        wait_finished(&h.handle).await;
    }

    /// 在途命令取消：唯一结果是 Cancelled，不再发 Command。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancel_current_cancels_in_flight_execute_without_command_result() {
        let mut h = make_cancel_with(CancelOptions {
            gate_execute: true,
            ..CancelOptions::default()
        });

        h.handle.submit_execute(cmd(0)).expect("queue has room");
        h.log.execute_started.notified().await; // 命令已进入驱动
        assert!(h.handle.in_flight());
        assert!(h.handle.cancel_current(), "在途命令应报告 true");

        h.release.send(()).unwrap(); // 放行：驱动检查 token 后协作退出
        match h.result_rx.recv().await.expect("cancelled result") {
            WorkerResult::Cancelled {
                thing_id, within_grace, ..
            } => {
                assert_eq!(thing_id, "thing-1");
                assert!(within_grace);
            }
            other => panic!("expected Cancelled, got {other:?}"),
        }
        wait_until(|| !h.handle.in_flight()).await;

        h.handle.stop();
        wait_finished(&h.handle).await;
        assert!(
            h.result_rx.try_recv().is_err(),
            "cancelled execute must not emit a Command result"
        );
        assert_eq!(
            h.log.executes.load(Ordering::SeqCst),
            0,
            "command body must not complete"
        );
    }

    /// 调用完成后再取消不算取消：返回 false，也不补发 Cancelled。
    #[tokio::test]
    async fn late_cancel_after_call_completed_is_not_reported() {
        let mut h = make_cancel(true, Duration::ZERO);

        h.handle.submit_execute(cmd(0)).expect("queue has room");
        match h.result_rx.recv().await.expect("command result") {
            WorkerResult::Command { success, .. } => assert!(success),
            other => panic!("expected Command, got {other:?}"),
        }
        // Command 结果在 finish 之后才发送，此时槽位已原子取空。
        assert!(!h.handle.in_flight());
        assert!(!h.handle.cancel_current(), "已完成调用不应再接受取消");

        h.handle.stop();
        wait_finished(&h.handle).await;
        assert!(h.result_rx.try_recv().is_err(), "late cancel must not emit Cancelled");
    }

    /// 重复取消保留首次时刻：elapsed 从第一次 cancel 起算。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn repeated_cancel_preserves_first_cancel_instant() {
        let mut h = make_cancel(true, Duration::ZERO);

        h.handle.submit_read(vec!["a".to_string()]);
        h.log.read_started.notified().await;
        assert!(h.handle.cancel_current());
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(h.handle.cancel_current(), "第二次取消仍命中同一在途调用");

        h.release.send(()).unwrap();
        match h.result_rx.recv().await.expect("cancelled result") {
            WorkerResult::Cancelled {
                within_grace,
                elapsed_ms,
                ..
            } => {
                assert!(within_grace);
                assert!(
                    elapsed_ms >= 250,
                    "elapsed must count from the first cancel, got {elapsed_ms}ms"
                );
            }
            other => panic!("expected Cancelled, got {other:?}"),
        }

        h.handle.stop();
        wait_finished(&h.handle).await;
    }

    /// 可重试配置下的协作取消：取消是终态，驱动只被调用一次且仍在宽限期内。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cooperative_cancel_with_retrying_driver_stays_within_grace() {
        let mut h = make_cancel_with(CancelOptions {
            max_attempts: 5,
            retry_interval: Duration::from_millis(200),
            ..CancelOptions::default()
        });

        h.handle.submit_read(vec!["a".to_string()]);
        h.log.read_started.notified().await;
        assert!(h.handle.cancel_current());

        h.release.send(()).unwrap();
        match h.result_rx.recv().await.expect("cancelled result") {
            WorkerResult::Cancelled { within_grace, .. } => {
                assert!(within_grace, "cancellation must not enter retry backoff");
            }
            other => panic!("expected Cancelled, got {other:?}"),
        }
        assert_eq!(
            h.log.reads.load(Ordering::SeqCst),
            1,
            "cancelled driver must not be retried"
        );

        h.handle.stop();
        wait_finished(&h.handle).await;
        assert!(
            h.result_rx.try_recv().is_err(),
            "cancelled read must be the only outcome"
        );
    }
}
