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
//! **通道布局（有意为之）**：`WorkerCommand::Execute` 走 `mpsc`（有界 32 FIFO）；
//! `Read` / `SetDemand` / `Stop` **不经过** mpsc，分别走容量 1 的独立 `watch`
//! 槽位——这样 worker 忙时新请求仍能覆盖旧请求，且队列满不会阻塞 Stop 或采样器。
//! [`WorkerCommand`] 保留完整命令面以匹配 spec 契约。
//!
//! 调度用 `tokio::select! { biased; ... }`：Execute 优先于 Read。
//! ValueSink 使用非阻塞 `try_send`：满/关闭时丢弃本条读数（下一 tick 会重采），
//! 绝不因下游背压阻塞 worker 的 Execute/Stop。

use std::sync::Arc;

use parking_lot::RwLock;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

use tinyiothub_core::driver::{CancelToken, DriverMode, DriverReading, PropertyChangeSource, ResultValue, ValueSink};
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
    value_sink: ValueSink,
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
                let reading = read_once(&mut driver, &thing_id);
                *status.write() = driver.overview();
                if let Some(reading) = reading {
                    send_reading(&value_sink, reading);
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

    tracing::debug!(thing_id = %thing_id, "driver worker stopped");
}

/// 执行一次轮询读取：成功时先把读数写回驱动自己的 `Thing`，返回待上报的
/// [`DriverReading`]（上报由调用方在刷新 overview 之后做，保证 sink 消费者
/// 看到的状态不落后于共享 overview）。
fn read_once(driver: &mut DriverWrapper, thing_id: &str) -> Option<DriverReading> {
    let outcome = driver.read_data_once_with(&CancelToken::new());
    match outcome.result {
        Ok(values) => {
            apply_read_values(driver, &values);
            Some(DriverReading {
                thing_id: thing_id.to_string(),
                values,
            })
        }
        Err(err) => {
            tracing::warn!(
                thing_id = %thing_id,
                error = %err,
                elapsed_ms = outcome.elapsed.as_millis() as u64,
                "driver read failed"
            );
            None
        }
    }
}

/// 把一批读数写回驱动自己的 `Thing`（worker 是驱动单写者）。
///
/// 与 `DataServer::apply_property_values` 同语义：按属性名匹配、只写
/// `current_value`（`set_current_value` 同时刷新 `updated_at`）；另按轮询成功
/// 路径刷新 `last_heartbeat`。worker 不发布事件——事件仍由 DataServer 的共享
/// 管线在消费 ValueSink 时产生。
fn apply_read_values(driver: &mut DriverWrapper, values: &[ResultValue]) {
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
    thing.last_heartbeat = Some(chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string());
    *driver.thing_mut() = thing;
}

/// 非阻塞上报：sink 满/关闭时丢弃本条读数并告警（下一次采样会补）。
fn send_reading(value_sink: &ValueSink, reading: DriverReading) {
    let thing_id = reading.thing_id.clone();
    match value_sink.try_send(reading) {
        Ok(()) => {}
        Err(mpsc::error::TrySendError::Full(_)) => {
            tracing::warn!(thing_id = %thing_id, "driver worker value sink full; dropping reading");
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {
            tracing::warn!(thing_id = %thing_id, "driver worker value sink closed; dropping reading");
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
            self.log.executes.fetch_add(1, Ordering::SeqCst);
            self.log.events.lock().unwrap().push("execute".to_string());
            *self.log.observed_value.lock().unwrap() = self
                .thing
                .properties
                .as_ref()
                .and_then(|properties| properties.iter().find(|p| p.name == "temp"))
                .and_then(|p| p.current_value.clone());
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
        let handle = spawn_worker(thing, driver, sink_tx.clone(), Arc::new(FakeEvents::new()));
        Harness {
            handle,
            log,
            release,
            sink_tx,
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

        assert_eq!(h.sink_rx.try_recv().unwrap().thing_id, "filler");
        assert!(h.sink_rx.try_recv().is_err(), "dropped reading must not appear");

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
}
