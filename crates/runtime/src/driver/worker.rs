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
//! - **Stop**：`watch`。worker 在操作边界退出（在途调用在 E4c 接取消，本期
//!   只停止后续）。
//!
//! 调度用 `tokio::select! { biased; ... }`：Execute 优先于 Read。

use std::sync::Arc;

use parking_lot::RwLock;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

use tinyiothub_core::driver::{CancelToken, DriverMode, DriverReading, PropertyChangeSource, ValueSink};
use tinyiothub_core::error::Error;
use tinyiothub_core::models::thing::Thing;
use tinyiothub_core::models::thing_command::ThingCommand;

use super::status::ThingOverview;
use super::wrapper::DriverWrapper;

/// Execute 队列容量（spec §2.3 队列上限 32）。
const EXEC_QUEUE_CAP: usize = 32;

/// worker 命令面（契约见 spec §2.3）。
///
/// `Read` / `SetDemand` / `Stop` 走各自的无阻塞 `watch` 槽位；`Execute` 走
/// 有界 FIFO 队列。枚举保留完整命令面供外部按契约引用。
pub enum WorkerCommand {
    Read { sources: Vec<String> },
    Execute { cmd: ThingCommand },
    SetDemand { sources: Vec<String> },
    Stop,
}

/// 每设备 worker 句柄：所有提交方法都不阻塞（Execute 满时返回 busy）。
pub struct WorkerHandle {
    exec_tx: mpsc::Sender<WorkerCommand>,
    read_tx: watch::Sender<Option<Vec<String>>>,
    demand_tx: watch::Sender<Vec<String>>,
    stop_tx: watch::Sender<bool>,
    status: Arc<RwLock<ThingOverview>>,
    last_read_sources: Arc<RwLock<Vec<String>>>,
    join: JoinHandle<()>,
}

impl WorkerHandle {
    /// 提交一次轮询读取。忙碌期间多次提交只保留最后一次的 source 集。
    ///
    /// 使用 `watch::send_replace`，接收端已退出也不 panic、不阻塞。
    pub fn submit_read(&self, sources: Vec<String>) {
        self.read_tx.send_replace(Some(sources));
    }

    /// 提交命令执行。队列（32）已满时返回 `DriverError("busy")`。
    pub fn submit_execute(&self, cmd: ThingCommand) -> Result<(), Error> {
        match self.exec_tx.try_send(WorkerCommand::Execute { cmd }) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => Err(Error::DriverError("busy".to_string())),
            Err(mpsc::error::TrySendError::Closed(_)) => Err(Error::DriverError("worker stopped".to_string())),
        }
    }

    /// 更新 Push 驱动的活跃 source 集（latest-wins，不阻塞）。
    pub fn set_demand(&self, sources: Vec<String>) {
        self.demand_tx.send_replace(sources);
    }

    /// 请求停止：worker 不再接受后续操作；在途调用返回后退出。
    pub fn stop(&self) {
        self.stop_tx.send_replace(true);
    }

    /// 当前设备状态快照。
    pub fn status(&self) -> ThingOverview {
        self.status.read().clone()
    }

    /// 最近一次实际执行的 Read 的 source 集（诊断/指标用）。
    pub fn last_read_sources(&self) -> Vec<String> {
        self.last_read_sources.read().clone()
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
        status.clone(),
        last_read_sources.clone(),
    ));

    WorkerHandle {
        exec_tx,
        read_tx,
        demand_tx,
        stop_tx,
        status,
        last_read_sources,
        join,
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_worker(
    mut driver: DriverWrapper,
    thing_id: String,
    mut value_sink: ValueSink,
    is_push: bool,
    mut exec_rx: mpsc::Receiver<WorkerCommand>,
    mut read_rx: watch::Receiver<Option<Vec<String>>>,
    mut demand_rx: watch::Receiver<Vec<String>>,
    mut stop_rx: watch::Receiver<bool>,
    status: Arc<RwLock<ThingOverview>>,
    last_read_sources: Arc<RwLock<Vec<String>>>,
) {
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
                match cmd {
                    Some(WorkerCommand::Execute { cmd }) => {
                        execute_command(&mut driver, &cmd);
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
                let sources = read_rx.borrow_and_update().clone().unwrap_or_default();
                // Push 驱动由 start_push 上报，Read 不触发轮询。
                if is_push {
                    continue;
                }
                last_read_sources.write().clone_from(&sources);
                read_once(&mut driver, &thing_id, &mut value_sink).await;
                *status.write() = driver.overview();
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

    tracing::debug!(thing_id = %thing_id, "driver worker stopped");
}

/// 执行一次轮询读取：成功时把读数交给 ValueSink，状态写回共享 overview。
async fn read_once(driver: &mut DriverWrapper, thing_id: &str, value_sink: &mut ValueSink) {
    let outcome = driver.read_data_once_with(&CancelToken::new());
    match outcome.result {
        Ok(values) => {
            let reading = DriverReading {
                thing_id: thing_id.to_string(),
                values,
            };
            if value_sink.send(reading).await.is_err() {
                tracing::warn!(thing_id = %thing_id, "driver worker value sink closed");
            }
        }
        Err(err) => {
            tracing::warn!(
                thing_id = %thing_id,
                error = %err,
                elapsed_ms = outcome.elapsed.as_millis() as u64,
                "driver read failed"
            );
        }
    }
}

/// 执行一条命令；失败只记日志（E4b 命令流仍为 fire-and-forget）。
fn execute_command(driver: &mut DriverWrapper, cmd: &ThingCommand) {
    let outcome = driver.execute_command_with(cmd, &CancelToken::new());
    if let Err(err) = outcome.result {
        tracing::warn!(
            thing_id = %driver.thing().id,
            command = %cmd.name,
            error = %err,
            elapsed_ms = outcome.elapsed.as_millis() as u64,
            "driver command failed"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Mutex, mpsc as std_mpsc};
    use std::time::Duration;

    use tokio::sync::Notify;

    use tinyiothub_core::driver::{ResultValue, ThingDriver};

    #[derive(Default)]
    struct FakeLog {
        reads: AtomicUsize,
        executes: AtomicUsize,
        push_starts: AtomicUsize,
        events: Mutex<Vec<String>>,
        read_started: Notify,
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

        fn read_data(&mut self, cancel: &CancelToken) -> Result<Vec<ResultValue>, Error> {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            self.log.reads.fetch_add(1, Ordering::SeqCst);
            self.log.events.lock().unwrap().push("read".to_string());
            self.log.read_started.notify_one();
            // 放行令牌未到就阻塞；sender 已 drop 时视为放行。
            let _ = self.release_rx.lock().unwrap().recv();
            Ok(vec![ResultValue::integer("value".to_string(), 1)])
        }

        fn execute_command(&mut self, _cmd: &ThingCommand, cancel: &CancelToken) -> Result<bool, Error> {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            self.log.executes.fetch_add(1, Ordering::SeqCst);
            self.log.events.lock().unwrap().push("execute".to_string());
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
        sink_rx: mpsc::Receiver<DriverReading>,
    }

    fn make(mode: DriverMode) -> Harness {
        let (release, release_rx) = std_mpsc::channel();
        let log = Arc::new(FakeLog::default());
        let thing = Thing {
            id: "thing-1".to_string(),
            name: "thing-1".to_string(),
            ..Thing::default()
        };
        let driver = DriverWrapper::new(Box::new(FakeDriver {
            thing: thing.clone(),
            mode,
            log: log.clone(),
            release_rx: Mutex::new(release_rx),
        }));
        let (sink_tx, sink_rx) = mpsc::channel(16);
        let handle = spawn_worker(thing, driver, sink_tx, Arc::new(FakeEvents::new()));
        Harness {
            handle,
            log,
            release,
            sink_rx,
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
        tokio::time::timeout(Duration::from_secs(2), async {
            while !handle.is_finished() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("worker should stop");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn two_reads_while_busy_coalesce_to_latest_source_set() {
        let h = make(DriverMode::Poll);
        let _sink_rx = &h.sink_rx;

        h.handle.submit_read(vec!["a".to_string()]);
        h.log.read_started.notified().await; // 第一次读已开始并阻塞

        h.handle.submit_read(vec!["b".to_string()]);
        h.handle.submit_read(vec!["c".to_string()]);
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
        let _sink_rx = &h.sink_rx;

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
        let _sink_rx = &h.sink_rx;

        h.handle.submit_read(vec!["a".to_string()]);
        h.log.read_started.notified().await; // worker 忙，Execute 只能排队

        for i in 0..EXEC_QUEUE_CAP {
            h.handle.submit_execute(cmd(i)).expect("queue below cap must accept");
        }
        match h.handle.submit_execute(cmd(EXEC_QUEUE_CAP)) {
            Err(Error::DriverError(msg)) => assert_eq!(msg, "busy"),
            Err(other) => panic!("expected busy, got {other}"),
            Ok(()) => panic!("expected busy, got Ok"),
        }

        // 放行并让 32 条命令全部执行完。
        h.release.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while h.log.executes.load(Ordering::SeqCst) < EXEC_QUEUE_CAP {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("queued commands should all run");

        h.handle.stop();
        wait_finished(&h.handle).await;
    }

    #[tokio::test]
    async fn stop_ends_task_and_late_sends_do_not_panic() {
        let h = make(DriverMode::Poll);
        let _sink_rx = &h.sink_rx;

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
}
