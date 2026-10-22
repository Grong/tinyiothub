use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use moka::sync::Cache;

use tinyiothub_core::driver::{DriverReading, ResultValue, ValueSink};
use tinyiothub_core::error::Error;
use tinyiothub_core::event::EventHandler;
use tinyiothub_core::models::event::{
    ContentElement, Event as DomainEvent, EventLevel, EventSource, RichContent, TextFormat, ThingEventType,
};
use tinyiothub_core::models::thing::ThingStatus;
use tinyiothub_core::models::thing_property::ThingProperty;
use tinyiothub_core::models::{thing::Thing, thing_command::ThingCommand};

use crate::access::sampler::{Sampler, SamplingWorkers};
use crate::arbitration::ResourceLease;
use crate::driver::worker::{CancelKind, CommandOutcome, WorkerHandle, WorkerResult, spawn_worker};
use crate::driver::{DriverWrapper, ThingOverview, create_driver};
use crate::event_bus::{EventBus, publish_event_safe};
use crate::ports::{HubLedger, ThingCacheSource};

/// worker 表：每个物一个独占驱动的 worker（E4b spec §1/§2.3）。
pub type WorkerCache = Cache<String, Arc<WorkerHandle>>;

/// 读数与 worker 结果的唯一消费端。
///
/// 两路输入共用**一个**消费任务（`tokio::select!` 顺序处理），避免两个消费者
/// 对 device_cache 做非原子的 get→mutate→update 而互相覆盖（Sticky Offline /
/// 派生值回滚）。
struct Consumers {
    readings: tokio::sync::mpsc::Receiver<DriverReading>,
    results: tokio::sync::mpsc::UnboundedReceiver<WorkerResult>,
}

/// Thing data server — manages driver worker lifecycle and the polling loop.
pub struct DataServer {
    device_cache: Arc<dyn ThingCacheSource>,
    workers: WorkerCache,
    event_bus: Arc<EventBus>,
    /// 推送驱动上报入口（spec §一 ValueSink）：Task 7 的 Manager 注册推送驱动时经 `value_sink()` 注入。
    value_sink_tx: ValueSink,
    /// 唯一消费端：`run()` take 一次（重复 run 不会 spawn 第二个消费者）。
    consumers: Arc<parking_lot::Mutex<Option<Consumers>>>,
    /// worker 结果上报入口（命令/连接迁移）。
    worker_result_tx: tokio::sync::mpsc::UnboundedSender<WorkerResult>,
    /// 采样任务的启动闸（重复 `run()` 不重复起轮询/sampler）。
    poll_started: Arc<AtomicBool>,
    /// 中央 sampler（由 ServiceManager 在 `run()` 前注入）。为 None 时保留
    /// 临时 100ms 全量轮询（旧测试/降级路径）。
    sampler: Arc<parking_lot::Mutex<Option<Arc<Sampler>>>>,
    /// E4c 取消留痕：可选账本（组合根注入）+ 进程内计数。
    cancellation: Arc<CancellationRecorder>,
}

/// 取消终态的留痕面：账本迟绑定（`run()` 的消费任务在 spawn 前持有 clone），
/// 计数用 atomics（getter 供指标/测试读取）。
#[derive(Default)]
struct CancellationRecorder {
    ledger: parking_lot::RwLock<Option<Arc<dyn HubLedger>>>,
    cancelled_total: AtomicU64,
    cancelled_unknown_total: AtomicU64,
}

impl CancellationRecorder {
    fn record(&self, within_grace: bool) {
        if within_grace {
            self.cancelled_total.fetch_add(1, Ordering::Relaxed);
        } else {
            self.cancelled_unknown_total.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn ledger(&self) -> Option<Arc<dyn HubLedger>> {
        self.ledger.read().clone()
    }
}

impl DataServer {
    pub fn new(device_cache: Arc<dyn ThingCacheSource>, event_bus: Arc<EventBus>) -> Self {
        let (value_sink_tx, readings) = tokio::sync::mpsc::channel(256);
        let (worker_result_tx, results) = tokio::sync::mpsc::unbounded_channel();
        let server = Self {
            device_cache,
            workers: Cache::new(10_000),
            event_bus,
            value_sink_tx,
            consumers: Arc::new(parking_lot::Mutex::new(Some(Consumers { readings, results }))),
            worker_result_tx,
            poll_started: Arc::new(AtomicBool::new(false)),
            sampler: Arc::new(parking_lot::Mutex::new(None)),
            cancellation: Arc::new(CancellationRecorder::default()),
        };
        server.initialize_drivers();
        server
    }

    /// 推送驱动的上报句柄（Task 7 注册推送驱动时注入给 `start_push`）。
    pub fn value_sink(&self) -> ValueSink {
        self.value_sink_tx.clone()
    }

    /// worker 表句柄（诊断/测试用）。moka `Cache` 克隆是廉价句柄。
    pub fn workers(&self) -> WorkerCache {
        self.workers.clone()
    }

    /// 给 Central sampler 用的 worker 面（读/需求/模式查询）。
    pub fn sampling_workers(&self) -> Arc<dyn SamplingWorkers> {
        Arc::new(CachedSamplingWorkers {
            workers: self.workers.clone(),
        })
    }

    /// 注入中央 sampler（必须在 `run()` 之前调用）。注入后 `run()` 不再启动
    /// 临时 100ms 全量轮询。
    pub fn set_sampler(&self, sampler: Arc<Sampler>) {
        *self.sampler.lock() = Some(sampler);
    }

    /// 注入账本（E4c 取消留痕；组合根在 `run()` 前后皆可调用）。None = 只发
    /// 事件/指标，不落账。
    pub fn set_hub_ledger(&self, ledger: Arc<dyn HubLedger>) {
        *self.cancellation.ledger.write() = Some(ledger);
    }

    /// E4c stop：取消该设备当前在途调用（协作式，不等待）。返回取消到的
    /// 在途调用数（0/1）。
    pub fn cancel_calls(&self, thing_id: &str) -> usize {
        self.workers.get(thing_id).is_some_and(|worker| worker.cancel_current()) as usize
    }

    /// 取消终态计数：宽限内退出（`cancelled`）与超时/迟到返回
    /// （`cancelled_unknown`）。
    pub fn cancelled_counts(&self) -> (u64, u64) {
        (
            self.cancellation.cancelled_total.load(Ordering::Relaxed),
            self.cancellation.cancelled_unknown_total.load(Ordering::Relaxed),
        )
    }

    /// 从外部 wrapper 注册一个 worker（测试装配 / 动态注册入口）。
    ///
    /// 同 key 已有 worker 时先停止旧 worker 再替换。
    pub fn register_worker(&self, thing_id: &str, driver: DriverWrapper) {
        self.spawn_driver_worker(thing_id.to_string(), driver);
    }

    fn initialize_drivers(&self) {
        for device in self.device_cache.all() {
            if let Some(driver_name) = &device.driver_name {
                match create_driver(driver_name, &device) {
                    Ok(driver) => {
                        self.spawn_driver_worker(device.id.clone(), driver);
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to create driver for device '{}': {}",
                            device.display_name.as_deref().unwrap_or(&device.name),
                            e
                        );
                    }
                }
            }
        }
    }

    fn spawn_driver_worker(&self, thing_id: String, mut driver: DriverWrapper) {
        if let Some(previous) = self.workers.get(&thing_id) {
            previous.stop();
        }
        driver.set_event_bus(self.event_bus.clone());
        let thing = driver.thing().clone();
        let handle = spawn_worker(
            thing,
            driver,
            self.value_sink_tx.clone(),
            self.event_bus.clone(),
            self.worker_result_tx.clone(),
        );
        self.workers.insert(thing_id, Arc::new(handle));
    }

    pub async fn run(&self, shutdown_rx: tokio::sync::broadcast::Receiver<()>) -> Result<(), Error> {
        // 启动闸：重复 `run()` 不得再起第二条采样/轮询任务。
        let sampler = self.sampler.lock().clone();
        if !self.poll_started.swap(true, Ordering::SeqCst) {
            if let Some(sampler) = sampler {
                // 中央 sampler 接管：50ms tick、需求门控/health/宽限。
                let sampler_shutdown_rx = shutdown_rx.resubscribe();
                tokio::spawn(async move {
                    sampler.run(sampler_shutdown_rx).await;
                });
            } else {
                // 兼容路径（未注入 sampler 的测试/降级）：临时 100ms 轮询向
                // 所有非 Push worker 提交全量 Read。worker 内部做 latest-wins
                // 合并与重试退避。
                let workers = self.workers.clone();
                let mut shutdown_rx_clone = shutdown_rx.resubscribe();
                tokio::spawn(async move {
                    tokio::select! {
                        _ = Self::poll_workers(workers) => {
                            tracing::debug!("Driver polling task completed");
                        }
                        _ = shutdown_rx_clone.recv() => {
                            tracing::info!("Driver polling task received shutdown signal");
                        }
                    }
                });
            }
        }

        // 单一消费任务：worker 结果与 ValueSink 读数顺序处理，device_cache 的
        // get→mutate→update 不跨任务并发（防 Connected 被旧读数快照覆盖）。
        if let Some(consumers) = self.consumers.lock().take() {
            let workers = self.workers.clone();
            let device_cache = self.device_cache.clone();
            let event_bus = self.event_bus.clone();
            let cancellation = self.cancellation.clone();
            let mut shutdown_rx_consumers = shutdown_rx.resubscribe();
            tokio::spawn(async move {
                Self::consume_loop(
                    consumers,
                    workers,
                    device_cache,
                    event_bus,
                    cancellation,
                    &mut shutdown_rx_consumers,
                )
                .await;
            });
        }
        Ok(())
    }

    async fn consume_loop(
        mut consumers: Consumers,
        workers: WorkerCache,
        device_cache: Arc<dyn ThingCacheSource>,
        event_bus: Arc<EventBus>,
        cancellation: Arc<CancellationRecorder>,
        shutdown_rx: &mut tokio::sync::broadcast::Receiver<()>,
    ) {
        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => break,
                maybe = consumers.readings.recv() => {
                    let Some(reading) = maybe else { break };
                    // 只有“有 worker 且当前健康”的读数才刷新 heartbeat/Online：
                    // 旧的成功读数不得覆盖随后发生的失败迁移（跨通道乱序防线）。
                    // Push 驱动不轮询，overview 永远不会转为 Online；其上报读数
                    // 本身就是在线证据（Task 4 评审结转）。
                    let worker_online = workers.get(&reading.thing_id).is_some_and(|worker| {
                        worker.is_push() || worker.status().health.is_online()
                    });
                    Self::consume_driver_reading_with(
                        reading,
                        worker_online,
                        &device_cache,
                        event_bus.clone(),
                    )
                    .await;
                }
                maybe = consumers.results.recv() => {
                    let Some(result) = maybe else { break };
                    Self::consume_worker_result(result, &device_cache, event_bus.clone(), &cancellation).await;
                }
            }
        }
    }

    /// 消费一条读数：走共享管线（apply_property_values → cache_and_publish）。
    ///
    /// worker 已把读数写回自己持有的 `Thing`；DataServer 只按 thing_id 从
    /// device_cache 取当前物（派生/空间物同样适用）。物已删 → 丢弃并告警。
    ///
    /// 测试/兼容入口：无 worker 关联（派生 owner / 空间物的注入读数），不写
    /// `last_heartbeat`。
    pub async fn consume_driver_reading(
        reading: DriverReading,
        device_cache: &Arc<dyn ThingCacheSource>,
        event_bus: Arc<EventBus>,
    ) {
        Self::consume_driver_reading_with(reading, false, device_cache, event_bus).await;
    }

    async fn consume_driver_reading_with(
        reading: DriverReading,
        heartbeat: bool,
        device_cache: &Arc<dyn ThingCacheSource>,
        event_bus: Arc<EventBus>,
    ) {
        let Some(mut device) = device_cache.get(&reading.thing_id) else {
            tracing::warn!(thing_id = %reading.thing_id, "sink 读数丢弃：物不在缓存（已删？）");
            return;
        };
        // 成功读数即心跳：与属性值在**同一次** cache write 中落盘（管理面 5 分钟
        // 在线判定不因仅在迁移时刷新而误报离线）。
        if heartbeat {
            device.status = ThingStatus::Online;
            device.last_heartbeat = Some(chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string());
        }
        let thing_name = device.name.clone();
        let events = Self::apply_property_values(&mut device, &thing_name, &reading.values);
        Self::cache_and_publish(device, events, device_cache, Some(event_bus)).await;
    }

    /// 消费一条 worker 结果：命令完成/失败与连接迁移翻译成领域事件，并刷新
    /// 缓存中的设备状态（事件构造与旧 `DriverWrapper::on_*` 保持一致）。
    async fn consume_worker_result(
        result: WorkerResult,
        device_cache: &Arc<dyn ThingCacheSource>,
        event_bus: Arc<EventBus>,
        cancellation: &CancellationRecorder,
    ) {
        match result {
            WorkerResult::Command {
                thing_id,
                cmd,
                success,
                elapsed_ms,
                error,
            } => {
                let Some(device) = device_cache.get(&thing_id) else {
                    return;
                };
                if let Some(event) = Self::build_command_event(&device, &cmd, success, elapsed_ms, error) {
                    publish_event_safe(event_bus, event).await;
                }
            }
            WorkerResult::Connected { thing_id, address } => {
                let Some(mut device) = device_cache.get(&thing_id) else {
                    return;
                };
                device.status = ThingStatus::Online;
                device.last_heartbeat = Some(chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string());
                let event = Self::build_connection_event(&device, true, address, None);
                device_cache.update(device);
                if let Some(event) = event {
                    publish_event_safe(event_bus, event).await;
                }
            }
            WorkerResult::Disconnected { thing_id, reason } => {
                let Some(mut device) = device_cache.get(&thing_id) else {
                    return;
                };
                device.status = ThingStatus::Offline;
                let event = Self::build_connection_event(&device, false, None, reason);
                device_cache.update(device);
                if let Some(event) = event {
                    publish_event_safe(event_bus, event).await;
                }
            }
            WorkerResult::ReadFailed { thing_id } => {
                if let Some(mut device) = device_cache.get(&thing_id) {
                    device.status = ThingStatus::Offline;
                    device_cache.update(device);
                }
            }
            // E4c stop 的异步终态：取消调用不再发 Command 结果，由这里补齐
            // 事件 + 账本 + 指标（一次调用一个 outcome）。
            WorkerResult::Cancelled {
                thing_id,
                kind,
                within_grace,
                elapsed_ms,
            } => {
                cancellation.record(within_grace);
                let Some(device) = device_cache.get(&thing_id) else {
                    tracing::warn!(thing_id = %thing_id, "取消终态丢弃：物不在缓存（已删？）");
                    return;
                };
                if let Some(event) = Self::build_command_cancelled_event(&device, kind, within_grace, elapsed_ms) {
                    publish_event_safe(event_bus, event).await;
                }
                if let Some(ledger) = cancellation.ledger() {
                    let Some(workspace_id) = device.workspace_id.as_deref() else {
                        tracing::debug!(thing_id = %thing_id, "取消终态无 workspace_id，跳过账本");
                        return;
                    };
                    let outcome = if within_grace { "cancelled" } else { "cancelled_unknown" };
                    // 账本 kind 保持 kind-agnostic（call_cancelled*），调用类型
                    // 在 payload.kind 里显式区分（Issue 8：读取消不得看起来
                    // 像命令取消）。
                    let ledger_kind = if within_grace {
                        "call_cancelled"
                    } else {
                        "call_cancelled_unknown"
                    };
                    let payload = serde_json::json!({
                        "v": 1,
                        "thingId": thing_id,
                        "outcome": outcome,
                        "kind": kind.as_str(),
                        "elapsedMs": elapsed_ms,
                    });
                    if let Err(e) = ledger.append(workspace_id, ledger_kind, Some(&thing_id), payload).await {
                        tracing::warn!(thing_id = %thing_id, kind = ledger_kind, error = %e, "取消终态账本写入失败");
                    }
                }
            }
        }
    }

    /// 临时轮询循环：每 100ms 向非 Push worker 提交一次 Read（source 集为空 =
    /// 读全部，与旧 `read_data_once` 语义一致）。
    async fn poll_workers(workers: WorkerCache) {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(100));
        loop {
            interval.tick().await;
            for (_, handle) in workers.iter() {
                if !handle.is_push() {
                    handle.submit_read(Vec::new());
                }
            }
        }
    }

    pub(crate) fn collect_property_change_events(device: &mut Thing, values: &[ResultValue]) -> Vec<DomainEvent> {
        let mut pending_events = Vec::new();
        let thing_id = device.id.clone();
        let thing_name = device.name.clone();
        if let Some(ref mut properties) = device.properties {
            for property in properties.iter_mut() {
                if let Some(result_value) = values.iter().find(|v| v.name == property.name)
                    && let Some(ref value_str) = result_value.value
                {
                    let old_value = property.current_value.clone();
                    let value_changed = match &old_value {
                        Some(old_val) => old_val != value_str,
                        None => true,
                    };
                    property.set_current_value(value_str.clone());
                    if value_changed
                        && let Some(event) = Self::build_property_change_event_static(
                            &thing_id,
                            &thing_name,
                            property,
                            old_value,
                            value_str,
                        )
                    {
                        pending_events.push(event);
                    }
                }
            }
        }
        pending_events
    }

    /// 应用一批属性读数：更新当前值并收集变更事件。轮询/推送共用（spec §一 共享下游管线）。
    pub(crate) fn apply_property_values(
        device: &mut Thing,
        thing_name: &str,
        values: &[ResultValue],
    ) -> Vec<DomainEvent> {
        let _ = thing_name;
        Self::collect_property_change_events(device, values)
    }

    /// 缓存更新 + 事件逐个发布（缓存先于发布——订阅者读缓存必见新值）。
    pub(crate) async fn cache_and_publish(
        device: Thing,
        events: Vec<DomainEvent>,
        device_cache: &Arc<dyn ThingCacheSource>,
        event_bus: Option<Arc<EventBus>>,
    ) {
        device_cache.update(device);
        if let Some(bus) = event_bus {
            for event in events {
                publish_event_safe(bus.clone(), event).await;
            }
        }
    }

    pub(crate) fn build_property_change_event_static(
        thing_id: &str,
        thing_name: &str,
        property: &ThingProperty,
        old_value: Option<String>,
        new_value: &str,
    ) -> Option<DomainEvent> {
        let mut elements = vec![ContentElement::Text {
            content: format!("Property '{}' value changed on device '{}'", property.name, thing_name),
            format: TextFormat::Plain,
        }];

        if let Some(ref old_val) = old_value {
            elements.push(ContentElement::Text {
                content: format!("Previous value: {}", old_val),
                format: TextFormat::Plain,
            });
        }

        elements.push(ContentElement::Text {
            content: format!("Current value: {}", new_value),
            format: TextFormat::Plain,
        });

        let mut content = RichContent::new(
            format!("Property Changed: {} - {}", thing_name, property.name),
            elements,
        )
        .with_metadata(
            "property_id".to_string(),
            serde_json::Value::String(property.id.clone()),
        )
        .with_metadata(
            "property_name".to_string(),
            serde_json::Value::String(property.display_name.clone().unwrap_or_else(|| property.name.clone())),
        )
        .with_metadata("value".to_string(), serde_json::Value::String(new_value.to_string()));
        if let Some(ref old) = old_value {
            content = content.with_metadata("old_value".to_string(), serde_json::Value::String(old.clone()));
        }

        DomainEvent::new_device_event(
            ThingEventType::PropertyChange,
            EventLevel::Info,
            EventSource::device_property(thing_id.to_string(), property.id.clone(), "data_collector".to_string()),
            content,
        )
        .ok()
    }

    fn build_command_event(
        device: &Thing,
        cmd: &ThingCommand,
        success: bool,
        execution_time_ms: u64,
        error_message: Option<String>,
    ) -> Option<DomainEvent> {
        let (event_type, level, status) = if success {
            (ThingEventType::CommandCompleted, EventLevel::Info, "success")
        } else {
            (ThingEventType::CommandFailed, EventLevel::Error, "failed")
        };

        let mut elements = vec![
            ContentElement::Text {
                content: format!("Command '{}' executed on device '{}'", cmd.name, device.name),
                format: TextFormat::Plain,
            },
            ContentElement::Text {
                content: format!("Status: {}, Time: {}ms", status, execution_time_ms),
                format: TextFormat::Plain,
            },
        ];

        if let Some(ref err) = error_message {
            elements.push(ContentElement::Text {
                content: format!("Error: {}", err),
                format: TextFormat::Plain,
            });
        }

        DomainEvent::new_device_event(
            event_type,
            level,
            EventSource::device(device.id.clone(), Some("driver".to_string())),
            RichContent::new(
                format!("Command '{}' on device '{}': {}", cmd.name, device.name, status),
                elements,
            ),
        )
        .ok()
    }

    /// E4c 取消终态事件：`outcome` 区分宽限内退出与迟到返回（spec §2.4）；
    /// `kind` 区分被取消的是采样读还是命令执行（Issue 8 审计保真）。
    fn build_command_cancelled_event(
        device: &Thing,
        kind: CancelKind,
        within_grace: bool,
        elapsed_ms: u64,
    ) -> Option<DomainEvent> {
        let outcome = if within_grace { "cancelled" } else { "cancelled_unknown" };
        let level = if within_grace {
            EventLevel::Info
        } else {
            EventLevel::Warning
        };
        let call_kind = kind.as_str();
        let content = RichContent::new(
            format!("Command on device '{}': {} ({call_kind})", device.name, outcome),
            vec![ContentElement::Text {
                content: format!(
                    "Cancellation outcome: {}, kind: {call_kind}, elapsed: {}ms",
                    outcome, elapsed_ms
                ),
                format: TextFormat::Plain,
            }],
        )
        .with_metadata("outcome".to_string(), serde_json::json!(outcome))
        .with_metadata("kind".to_string(), serde_json::json!(call_kind))
        .with_metadata("elapsedMs".to_string(), serde_json::json!(elapsed_ms))
        .with_metadata("thingId".to_string(), serde_json::json!(device.id));
        DomainEvent::new_device_event(
            ThingEventType::CommandCancelled,
            level,
            EventSource::device(device.id.clone(), Some("driver".to_string())),
            content,
        )
        .ok()
    }

    /// 连接迁移事件（等价于旧 `DriverWrapper::on_connected` / `on_disconnected`）。
    fn build_connection_event(
        device: &Thing,
        connected: bool,
        address: Option<String>,
        reason: Option<String>,
    ) -> Option<DomainEvent> {
        let source = EventSource::device(device.id.clone(), Some("driver".to_string()));
        if connected {
            DomainEvent::new_device_event(
                ThingEventType::Connection,
                EventLevel::Info,
                source,
                RichContent::new(
                    format!("Thing Online: {}", device.name),
                    vec![
                        ContentElement::Text {
                            content: format!("Thing '{}' is now online", device.name),
                            format: TextFormat::Plain,
                        },
                        ContentElement::Text {
                            content: format!("Protocol: {}", device.protocol_type.as_deref().unwrap_or("Unknown")),
                            format: TextFormat::Plain,
                        },
                        ContentElement::Text {
                            content: format!("Address: {}", address.as_deref().unwrap_or("N/A")),
                            format: TextFormat::Plain,
                        },
                    ],
                ),
            )
            .ok()
        } else {
            let mut elements = vec![ContentElement::Text {
                content: format!("Thing '{}' is now offline", device.name),
                format: TextFormat::Plain,
            }];
            if let Some(ref reason_text) = reason {
                elements.push(ContentElement::Text {
                    content: format!("Reason: {}", reason_text),
                    format: TextFormat::Plain,
                });
            }
            DomainEvent::new_device_event(
                ThingEventType::Connection,
                EventLevel::Warning,
                source,
                RichContent::new(format!("Thing Offline: {}", device.name), elements),
            )
            .ok()
        }
    }

    // === Public API ===

    pub fn get_things(&self) -> Vec<Thing> {
        self.device_cache.all()
    }

    pub fn get_device(&self, id: &str) -> Option<Thing> {
        self.device_cache.get(id)
    }

    pub fn get_device_by_name(&self, name: &str) -> Option<Thing> {
        self.device_cache.get_by_name(name)
    }

    pub fn execute_command(&self, cmd: ThingCommand) -> Result<(), Error> {
        self.execute_command_with_lease(cmd, None)
    }

    /// 提交命令并把调用方已持有的资源租约移交给 worker（E4c Issue 4）。
    ///
    /// worker 在命令真正结束（成功/失败/取消）后才 Drop 租约，因此 claim
    /// 覆盖排队 + 执行窗口；调用方不再需要在入队后立即释放。无法移交
    /// （无 worker 的物）时租约随本函数返回释放。
    pub fn execute_command_with_lease(&self, cmd: ThingCommand, lease: Option<ResourceLease>) -> Result<(), Error> {
        if self.device_cache.get(&cmd.thing_id).is_none() {
            return Err(Error::NotFound);
        }
        // 无 worker 的物（无驱动/派生 owner）沿用旧语义：命令被接受但无执行者。
        let Some(worker) = self.workers.get(&cmd.thing_id) else {
            tracing::debug!(thing_id = %cmd.thing_id, command = %cmd.name, "command accepted without a driver worker");
            return Ok(());
        };
        worker.submit_execute_with_lease(cmd, lease)
    }

    /// 提交命令并等待 worker 回报执行终态（E4c）。
    ///
    /// composite Call 步骤用它保证上一条命令真正离开驱动后才进入下一步；
    /// 运行期租约由 composite 全程持有，因此这里通常传 `None`。
    ///
    /// - 队列满：返回 `busy`；租约随被拒命令一起释放。
    /// - worker 已停止/任务提前退出：`done` 通道关闭，返回 `worker stopped`。
    /// - 无 worker 的物（无驱动/派生 owner）：命令视为已接受，返回成功终态
    ///   （与 [`Self::execute_command_with_lease`] 的旧语义一致）。
    pub async fn execute_command_with_lease_and_wait(
        &self,
        cmd: ThingCommand,
        lease: Option<ResourceLease>,
    ) -> Result<CommandOutcome, Error> {
        if self.device_cache.get(&cmd.thing_id).is_none() {
            return Err(Error::NotFound);
        }
        let Some(worker) = self.workers.get(&cmd.thing_id) else {
            tracing::debug!(thing_id = %cmd.thing_id, command = %cmd.name, "command accepted without a driver worker");
            drop(lease);
            return Ok(CommandOutcome {
                success: true,
                error: None,
                cancelled: false,
            });
        };
        let (done_tx, done_rx) = tokio::sync::oneshot::channel();
        worker.submit_execute_with_lease_and_done(cmd, lease, Some(done_tx))?;
        done_rx
            .await
            .map_err(|_| Error::DriverError("worker stopped".to_string()))
    }

    pub fn execute_commands(&self, cmds: Vec<ThingCommand>) -> Result<(), Error> {
        for cmd in cmds {
            self.execute_command(cmd)?;
        }
        Ok(())
    }

    pub fn remove_device(&self, thing_id: &str) {
        self.device_cache.remove(thing_id);
        if let Some(worker) = self.workers.get(thing_id) {
            worker.stop();
        }
        self.workers.invalidate(thing_id);
    }

    pub fn remove_things(&self, thing_ids: &[String]) {
        for thing_id in thing_ids {
            self.remove_device(thing_id);
        }
    }

    /// 设备驱动概览（worker 共享的 `ThingOverview` 快照）。
    pub fn get_driver_overview(&self, thing_id: &str) -> Option<ThingOverview> {
        self.workers.get(thing_id).map(|worker| worker.status())
    }

    /// 兼容旧名：与 [`Self::get_driver_overview`] 同义。
    pub fn get_device_overview(&self, thing_id: &str) -> Option<ThingOverview> {
        self.get_driver_overview(thing_id)
    }

    pub fn get_all_device_overview(&self) -> Vec<ThingOverview> {
        self.workers.iter().map(|(_, worker)| worker.status()).collect()
    }

    pub fn get_online_count(&self) -> usize {
        self.workers
            .iter()
            .filter(|(_, worker)| worker.status().health.is_online())
            .count()
    }

    pub fn get_healthy_count(&self) -> usize {
        self.workers
            .iter()
            .filter(|(_, worker)| worker.status().health.is_healthy())
            .count()
    }

    pub fn reset_device_driver(&self, thing_id: &str) -> bool {
        self.workers.get(thing_id).is_some_and(|worker| worker.reset())
    }

    pub fn set_device_offline(&self, thing_id: &str) -> bool {
        self.workers.get(thing_id).is_some_and(|worker| worker.set_offline())
    }

    pub fn get_driver_cache_size(&self) -> u64 {
        self.worker_count()
    }

    /// worker 数量（Task 4 接口名；`get_driver_cache_size` 的别名）。
    ///
    /// moka 的 `entry_count` 是最终一致的：先跑一次 pending tasks 保证
    /// insert/invalidate 已反映到计数（管理面/测试需要精确值）。
    pub fn worker_count(&self) -> u64 {
        self.workers.run_pending_tasks();
        self.workers.entry_count()
    }

    pub fn cleanup_driver_cache(&self) {
        let mut to_remove = Vec::new();
        for (thing_id, worker) in self.workers.iter() {
            if self.device_cache.get(&thing_id).is_none() {
                worker.stop();
                to_remove.push(thing_id.to_string());
            }
        }
        for thing_id in to_remove {
            self.workers.invalidate(&thing_id);
        }
    }
}

#[async_trait::async_trait]
impl EventHandler for DataServer {
    async fn handle(&self, event: &tinyiothub_core::models::event::Event) -> Result<(), Error> {
        use tinyiothub_core::models::event::{EventType, ThingEventType};

        let thing_id = match event.source().thing_id() {
            Some(id) => id,
            None => return Ok(()),
        };

        if let EventType::Device(device_event_type) = event.event_type() {
            match device_event_type {
                ThingEventType::DeviceCreated | ThingEventType::DeviceUpdated => {
                    tracing::info!("Handling {:?} event for device: {}", device_event_type, thing_id);
                    // 从事件 metadata 中提取完整设备信息
                    let device = event
                        .content()
                        .metadata()
                        .get("device")
                        .and_then(|v| serde_json::from_value::<Thing>(v.clone()).ok())
                        .or_else(|| self.device_cache.get(thing_id));

                    if let Some(device) = device {
                        // 插入/更新缓存（单一写入者）
                        self.device_cache.insert(device.clone());
                        // 创建 worker 并加入采集循环
                        if let Some(driver_name) = &device.driver_name
                            && !self.workers.contains_key(thing_id)
                        {
                            match create_driver(driver_name, &device) {
                                Ok(driver) => {
                                    self.spawn_driver_worker(thing_id.to_string(), driver);
                                    tracing::info!(
                                        "Created driver worker for device '{}' and started data collection",
                                        device.name
                                    );
                                }
                                Err(e) => {
                                    tracing::error!("Failed to create driver for device '{}': {}", device.name, e);
                                }
                            }
                        }
                    } else {
                        tracing::warn!("Thing not found in event metadata or cache: {}", thing_id);
                    }
                }
                ThingEventType::DeviceDeleted => {
                    self.remove_device(thing_id);
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn name(&self) -> &str {
        "DataServer"
    }

    fn should_handle(&self, event: &tinyiothub_core::models::event::Event) -> bool {
        matches!(event.event_type(), tinyiothub_core::models::event::EventType::Device(_))
    }

    fn priority(&self) -> u8 {
        10
    }
}

impl Clone for DataServer {
    fn clone(&self) -> Self {
        Self {
            device_cache: self.device_cache.clone(),
            workers: self.workers.clone(),
            event_bus: self.event_bus.clone(),
            value_sink_tx: self.value_sink_tx.clone(),
            consumers: self.consumers.clone(),
            worker_result_tx: self.worker_result_tx.clone(),
            poll_started: self.poll_started.clone(),
            sampler: self.sampler.clone(),
            cancellation: self.cancellation.clone(),
        }
    }
}

/// [`SamplingWorkers`] 的 worker 表适配器：没有 worker 的物（派生/空间物）
/// 静默跳过。
struct CachedSamplingWorkers {
    workers: WorkerCache,
}

impl SamplingWorkers for CachedSamplingWorkers {
    fn submit_read(&self, thing_id: &str, sources: Vec<String>) {
        if let Some(worker) = self.workers.get(thing_id) {
            worker.submit_read(sources);
        }
    }

    fn set_demand(&self, thing_id: &str, sources: Vec<String>) {
        if let Some(worker) = self.workers.get(thing_id) {
            worker.set_demand(sources);
        }
    }

    fn is_push(&self, thing_id: &str) -> bool {
        self.workers.get(thing_id).is_some_and(|worker| worker.is_push())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tinyiothub_core::driver::{CancelToken, DriverMode, ThingDriver};
    use tinyiothub_core::models::thing::Thing;
    use tinyiothub_core::models::thing_property::ThingProperty;

    fn make_thing_with_prop(name: &str, value: Option<&str>) -> Thing {
        let mut t = Thing {
            id: "thing-1".to_string(),
            name: "电表1".to_string(),
            ..Thing::default()
        };
        let mut p = ThingProperty {
            id: "prop-1".to_string(),
            thing_id: "thing-1".to_string(),
            name: name.to_string(),
            ..ThingProperty::default()
        };
        if let Some(v) = value {
            p.set_current_value(v.to_string());
        }
        t.properties = Some(vec![p]);
        t
    }

    #[test]
    fn property_change_emits_event_only_when_value_changes() {
        let mut thing = make_thing_with_prop("用电数", Some("100"));
        // 新值不同 → 产生 1 个 PropertyChange 事件，缓存值被更新
        let values = vec![ResultValue::new(
            "用电数".into(),
            "float".into(),
            Some("101.5".to_string()),
        )];
        let events = DataServer::collect_property_change_events(&mut thing, &values);
        assert_eq!(events.len(), 1);
        let prop = &thing.properties.as_ref().unwrap()[0];
        assert_eq!(prop.current_value.as_deref(), Some("101.5"));
        // 同值再来 → 0 事件
        let events2 = DataServer::collect_property_change_events(&mut thing, &values);
        assert_eq!(events2.len(), 0);
    }

    #[test]
    fn property_change_first_report_emits_event() {
        let mut thing = make_thing_with_prop("用电数", None);
        let values = vec![ResultValue::new(
            "用电数".into(),
            "float".into(),
            Some("42".to_string()),
        )];
        let events = DataServer::collect_property_change_events(&mut thing, &values);
        assert_eq!(events.len(), 1, "首次上报（old=None）必须产生事件");
    }

    #[test]
    fn property_change_event_metadata_carries_old_and_new() {
        let mut thing = make_thing_with_prop("用电数", Some("100"));
        let values = vec![ResultValue::new(
            "用电数".into(),
            "float".into(),
            Some("200".to_string()),
        )];
        let events = DataServer::collect_property_change_events(&mut thing, &values);
        let json = serde_json::to_value(&events[0]).unwrap();
        let s = json.to_string();
        assert!(s.contains("\"value\":\"200\""), "metadata.value 必须是新值: {s}");
        assert!(
            s.contains("\"old_value\":\"100\""),
            "metadata.old_value 必须是旧值: {s}"
        );
    }

    #[test]
    fn unknown_result_value_is_ignored() {
        let mut thing = make_thing_with_prop("用电数", Some("100"));
        let values = vec![ResultValue::new(
            "不存在的属性".into(),
            "float".into(),
            Some("1".to_string()),
        )];
        let events = DataServer::collect_property_change_events(&mut thing, &values);
        assert_eq!(events.len(), 0);
    }

    // ── Task 4：worker 路径（轮询跳过 / busy / overview / remove）──────

    struct VecCache(std::sync::Mutex<Vec<Thing>>);

    impl VecCache {
        fn new(devices: Vec<Thing>) -> Self {
            Self(std::sync::Mutex::new(devices))
        }
    }

    impl ThingCacheSource for VecCache {
        fn all(&self) -> Vec<Thing> {
            self.0.lock().unwrap().clone()
        }
        fn get(&self, id: &str) -> Option<Thing> {
            self.0.lock().unwrap().iter().find(|d| d.id == id).cloned()
        }
        fn get_by_name(&self, name: &str) -> Option<Thing> {
            self.0.lock().unwrap().iter().find(|d| d.name == name).cloned()
        }
        fn insert(&self, device: Thing) {
            let mut devices = self.0.lock().unwrap();
            if let Some(existing) = devices.iter_mut().find(|d| d.id == device.id) {
                *existing = device;
            } else {
                devices.push(device);
            }
        }
        fn update(&self, device: Thing) {
            self.insert(device);
        }
        fn remove(&self, id: &str) {
            self.0.lock().unwrap().retain(|d| d.id != id);
        }
    }

    fn worker_thing(id: &str) -> Thing {
        Thing {
            id: id.to_string(),
            name: id.to_string(),
            workspace_id: Some("ws-1".to_string()),
            ..Thing::default()
        }
    }

    fn ping_cmd(id: &str, thing_id: &str) -> ThingCommand {
        ThingCommand {
            id: id.to_string(),
            thing_id: thing_id.to_string(),
            name: "ping".to_string(),
            display_name: None,
            description: None,
            parameters: None,
            created_at: String::new(),
        }
    }

    async fn wait_until(mut condition: impl FnMut() -> bool) {
        tokio::time::timeout(tokio::time::Duration::from_secs(2), async {
            while !condition() {
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("condition should become true");
    }

    /// 计数 stub 驱动：read_data 被调用即计数（Push 驱动必须保持 0）。
    struct CountingDriver {
        thing: Thing,
        mode: DriverMode,
        reads: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    impl ThingDriver for CountingDriver {
        fn thing(&self) -> &Thing {
            &self.thing
        }
        fn thing_mut(&mut self) -> &mut Thing {
            &mut self.thing
        }
        fn mode(&self) -> DriverMode {
            self.mode
        }
        fn read_data(&mut self, cancel: &CancelToken) -> Result<Vec<ResultValue>, Error> {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(vec![])
        }
        fn execute_command(&mut self, _cmd: &ThingCommand, cancel: &CancelToken) -> Result<bool, Error> {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            Ok(true)
        }
    }

    /// 真实跑 `poll_workers` 循环：同缓存内一个 Push + 一个 Poll worker。
    /// Poll 计数 >0 证明循环确实在迭代；Push 计数 == 0 证明轮询跳过推送驱动。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn push_mode_driver_is_never_polled() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let workers: WorkerCache = Cache::new(10);
        let push_reads = Arc::new(AtomicUsize::new(0));
        let poll_reads = Arc::new(AtomicUsize::new(0));
        let (sink_tx, _sink_rx) = tokio::sync::mpsc::channel(16);
        let (result_tx, _result_rx) = tokio::sync::mpsc::unbounded_channel();
        for (id, mode, reads) in [
            ("push-1", DriverMode::Push, push_reads.clone()),
            ("poll-1", DriverMode::Poll, poll_reads.clone()),
        ] {
            let thing = worker_thing(id);
            let wrapper = DriverWrapper::new(Box::new(CountingDriver {
                thing: thing.clone(),
                mode,
                reads,
            }));
            let handle = spawn_worker(
                thing,
                wrapper,
                sink_tx.clone(),
                Arc::new(EventBus::new()),
                result_tx.clone(),
            );
            workers.insert(id.to_string(), Arc::new(handle));
        }

        let handle = tokio::spawn(DataServer::poll_workers(workers.clone()));
        // 100ms 间隔：350ms 至少 3 个 tick（首个 tick 立即触发）。
        tokio::time::sleep(tokio::time::Duration::from_millis(350)).await;
        handle.abort();
        for (_, worker) in workers.iter() {
            worker.stop();
        }

        assert!(
            poll_reads.load(Ordering::SeqCst) >= 2,
            "轮询循环必须在迭代（Poll 驱动计数 >= 2），got {}",
            poll_reads.load(Ordering::SeqCst)
        );
        assert_eq!(
            push_reads.load(Ordering::SeqCst),
            0,
            "Push worker 绝不经过 poll_workers 的 Read 路径"
        );
    }

    /// 无上报的 Push 驱动 stub（读数由测试经 `value_sink()` 注入）。
    struct IdlePushDriver {
        thing: Thing,
    }

    impl ThingDriver for IdlePushDriver {
        fn thing(&self) -> &Thing {
            &self.thing
        }
        fn thing_mut(&mut self) -> &mut Thing {
            &mut self.thing
        }
        fn mode(&self) -> DriverMode {
            DriverMode::Push
        }
        fn read_data(&mut self, _cancel: &CancelToken) -> Result<Vec<ResultValue>, Error> {
            Ok(vec![])
        }
        fn execute_command(&mut self, _cmd: &ThingCommand, _cancel: &CancelToken) -> Result<bool, Error> {
            Ok(true)
        }
    }

    /// Task 4 评审结转：Push worker 的 overview 永不转 Online（不轮询），
    /// 但上报读数必须刷新缓存 Online/last_heartbeat。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn push_reading_refreshes_heartbeat_without_worker_overview() {
        let thing = make_thing_with_prop("temp", None); // id = thing-1
        let cache = Arc::new(VecCache::new(vec![thing.clone()]));
        let event_bus = Arc::new(EventBus::new());
        let server = DataServer::new(cache.clone(), event_bus);
        let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel(1);
        server.run(shutdown_rx).await.expect("run");

        server.register_worker("thing-1", DriverWrapper::new(Box::new(IdlePushDriver { thing })));
        let overview = server.get_driver_overview("thing-1").expect("overview");
        assert!(
            !overview.health.is_online(),
            "Push worker 的 overview 不应被视为在线（前置条件）"
        );

        server
            .value_sink()
            .try_send(DriverReading {
                thing_id: "thing-1".to_string(),
                values: vec![ResultValue::new(
                    "temp".to_string(),
                    "float".to_string(),
                    Some("9".to_string()),
                )],
            })
            .expect("reading accepted");

        wait_until(|| {
            cache.get("thing-1").is_some_and(|device| {
                device
                    .properties
                    .as_ref()
                    .and_then(|props| props.iter().find(|p| p.name == "temp"))
                    .and_then(|p| p.current_value.as_deref())
                    == Some("9")
            })
        })
        .await;
        let cached = cache.get("thing-1").expect("cached thing");
        assert_eq!(
            cached.status,
            ThingStatus::Online,
            "Push 读数必须刷新缓存在线状态，即使 overview 尚未 Online"
        );
        assert!(cached.last_heartbeat.is_some(), "Push 读数必须刷新 last_heartbeat");
        assert!(
            !server
                .get_driver_overview("thing-1")
                .expect("overview")
                .health
                .is_online(),
            "overview 仍非 Online，证明心跳来自 Push 分支而非轮询成功"
        );

        let _ = shutdown_tx.send(());
    }

    /// 注入 sampler 后 `run()` 不得再起临时 100ms 全量轮询。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn sampler_replaces_temporary_poll_loop() {
        use std::sync::atomic::AtomicUsize;

        use crate::access::catalog::CachedManifestCatalog;
        use crate::access::demand::DemandTracker;
        use crate::access::registry::ManifestRegistry;

        let thing = worker_thing("dev-1");
        let cache = Arc::new(VecCache::new(vec![thing.clone()]));
        let server = DataServer::new(cache, Arc::new(EventBus::new()));
        let reads = Arc::new(AtomicUsize::new(0));
        server.register_worker(
            "dev-1",
            DriverWrapper::new(Box::new(CountingDriver {
                thing,
                mode: DriverMode::Poll,
                reads: reads.clone(),
            })),
        );

        let catalog = Arc::new(CachedManifestCatalog::new(
            Arc::new(VecCache::new(Vec::new())),
            Arc::new(ManifestRegistry::new()),
        ));
        let sampler = Arc::new(Sampler::new(
            catalog,
            Arc::new(DemandTracker::new()),
            server.sampling_workers(),
            None,
        ));
        server.set_sampler(sampler);

        let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel(1);
        server.run(shutdown_rx).await.expect("run");
        tokio::time::sleep(tokio::time::Duration::from_millis(250)).await;

        assert_eq!(
            reads.load(Ordering::SeqCst),
            0,
            "空 catalog 的 sampler 不得读；若 100ms 轮询仍存在则计数 >0"
        );
        let _ = shutdown_tx.send(());
        server.remove_device("dev-1");
    }

    /// 阻塞执行的假驱动：execute_command 通知测试后等待放行，用于灌满队列。
    struct BlockingExecDriver {
        thing: Thing,
        started: Arc<tokio::sync::Notify>,
        release: Arc<std::sync::atomic::AtomicBool>,
    }

    impl ThingDriver for BlockingExecDriver {
        fn thing(&self) -> &Thing {
            &self.thing
        }
        fn thing_mut(&mut self) -> &mut Thing {
            &mut self.thing
        }
        fn read_data(&mut self, _cancel: &CancelToken) -> Result<Vec<ResultValue>, Error> {
            Ok(vec![])
        }
        fn execute_command(&mut self, _cmd: &ThingCommand, _cancel: &CancelToken) -> Result<bool, Error> {
            self.started.notify_one();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !self.release.load(std::sync::atomic::Ordering::SeqCst) && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            Ok(true)
        }
    }

    /// `execute_command` 直投 worker；队列（32）满时返回 busy，不静默丢弃。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn execute_command_returns_busy_when_worker_queue_full() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let thing = worker_thing("dev-1");
        let cache = Arc::new(VecCache::new(vec![thing.clone()]));
        let server = DataServer::new(cache, Arc::new(EventBus::new()));
        let started = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(AtomicBool::new(false));
        server.register_worker(
            "dev-1",
            DriverWrapper::new(Box::new(BlockingExecDriver {
                thing,
                started: started.clone(),
                release: release.clone(),
            })),
        );

        // 第一条进入执行（阻塞），后续 32 条填满队列。
        server
            .execute_command(ping_cmd("cmd-0", "dev-1"))
            .expect("first command accepted");
        tokio::time::timeout(tokio::time::Duration::from_secs(2), started.notified())
            .await
            .expect("worker must start the first command");

        for i in 1..=32 {
            server
                .execute_command(ping_cmd(&format!("cmd-{i}"), "dev-1"))
                .expect("queue below cap must accept");
        }
        match server.execute_command(ping_cmd("cmd-overflow", "dev-1")) {
            Err(Error::DriverError(msg)) => assert_eq!(msg, "busy"),
            Err(other) => panic!("expected busy, got {other}"),
            Ok(()) => panic!("expected busy, got Ok"),
        }

        release.store(true, Ordering::SeqCst);
        server.remove_device("dev-1");
    }

    /// overview 读 worker 共享状态；remove_device 停止并移除 worker。
    #[tokio::test]
    async fn worker_backed_overview_and_remove() {
        use std::sync::atomic::AtomicUsize;

        let thing = worker_thing("dev-1");
        let cache = Arc::new(VecCache::new(vec![thing.clone()]));
        let server = DataServer::new(cache, Arc::new(EventBus::new()));
        server.register_worker(
            "dev-1",
            DriverWrapper::new(Box::new(CountingDriver {
                thing,
                mode: DriverMode::Poll,
                reads: Arc::new(AtomicUsize::new(0)),
            })),
        );

        assert_eq!(server.worker_count(), 1);
        let overview = server.get_driver_overview("dev-1").expect("overview");
        assert_eq!(overview.thing_id, "dev-1");
        assert_eq!(overview.health.total_successes, 0);

        // 命令执行成功后 worker 刷新共享 overview。
        server
            .execute_command(ping_cmd("cmd-1", "dev-1"))
            .expect("command accepted");
        wait_until(|| {
            server
                .get_driver_overview("dev-1")
                .is_some_and(|o| o.health.total_successes == 1)
        })
        .await;

        server.remove_device("dev-1");
        assert!(server.get_driver_overview("dev-1").is_none());
        wait_until(|| server.worker_count() == 0).await;
    }

    /// worker 的 Command 结果经 DataServer 结果消费任务发布为 CommandCompleted。
    #[tokio::test]
    async fn worker_command_result_publishes_command_event() {
        use std::sync::atomic::AtomicUsize;
        use tinyiothub_core::models::event::{EventType, ThingEventType};

        let thing = worker_thing("dev-1");
        let cache = Arc::new(VecCache::new(vec![thing.clone()]));
        let event_bus = Arc::new(EventBus::new());
        let server = DataServer::new(cache, event_bus.clone());
        let mut event_rx = event_bus.subscribe();
        let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel(1);
        server.run(shutdown_rx).await.expect("run");

        server.register_worker(
            "dev-1",
            DriverWrapper::new(Box::new(CountingDriver {
                thing,
                mode: DriverMode::Poll,
                reads: Arc::new(AtomicUsize::new(0)),
            })),
        );
        server
            .execute_command(ping_cmd("cmd-1", "dev-1"))
            .expect("command accepted");

        // 轮询也可能先发 Connection；只断言命令事件出现。
        let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(2);
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let event = tokio::time::timeout(remaining, event_rx.recv())
                .await
                .expect("command event within deadline")
                .expect("event channel open");
            if matches!(event.event_type(), EventType::Device(ThingEventType::CommandCompleted)) {
                assert_eq!(event.source().thing_id(), Some("dev-1"));
                break;
            }
        }

        let _ = shutdown_tx.send(());
    }

    /// 可切换成功/失败的 Poll 驱动（无阻塞，供 100ms 采样循环驱动 E2E）。
    struct FlakyPollDriver {
        thing: Thing,
        failing: Arc<AtomicBool>,
    }

    impl ThingDriver for FlakyPollDriver {
        fn thing(&self) -> &Thing {
            &self.thing
        }
        fn thing_mut(&mut self) -> &mut Thing {
            &mut self.thing
        }
        fn mode(&self) -> DriverMode {
            DriverMode::Poll
        }
        fn retry_config(&self) -> tinyiothub_core::driver::RetryConfig {
            tinyiothub_core::driver::RetryConfig {
                max_attempts: 1,
                base_interval: std::time::Duration::from_millis(1),
                max_interval: std::time::Duration::from_millis(1),
                timeout: std::time::Duration::from_millis(1),
                ..tinyiothub_core::driver::RetryConfig::default()
            }
        }
        fn read_data(&mut self, _cancel: &CancelToken) -> Result<Vec<ResultValue>, Error> {
            if self.failing.load(Ordering::SeqCst) {
                return Err(Error::IOError("boom".to_string()));
            }
            Ok(vec![ResultValue::new(
                "temp".to_string(),
                "float".to_string(),
                Some("42".to_string()),
            )])
        }
        fn execute_command(&mut self, _cmd: &ThingCommand, _cancel: &CancelToken) -> Result<bool, Error> {
            Ok(true)
        }
    }

    /// E2E（worker 路径）：成功轮询 → sink → device_cache/EventBus（Online +
    /// heartbeat + PropertyChange）；随后终态失败只发一次 offline 迁移。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn worker_poll_flows_to_cache_heartbeat_and_events() {
        use tinyiothub_core::models::event::{EventLevel, EventType, ThingEventType};

        let thing = make_thing_with_prop("temp", None); // id = thing-1
        let cache = Arc::new(VecCache::new(vec![thing.clone()]));
        let event_bus = Arc::new(EventBus::new());
        let mut events = event_bus.subscribe();
        let server = DataServer::new(cache.clone(), event_bus.clone());
        let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel(1);
        server.run(shutdown_rx).await.expect("run");

        let failing = Arc::new(AtomicBool::new(false));
        server.register_worker(
            "thing-1",
            DriverWrapper::new(Box::new(FlakyPollDriver {
                thing,
                failing: failing.clone(),
            })),
        );

        // 1) worker 成功读数落 device_cache：值 + Online + heartbeat。
        wait_until(|| {
            cache.get("thing-1").is_some_and(|device| {
                device
                    .properties
                    .as_ref()
                    .and_then(|props| props.iter().find(|p| p.name == "temp"))
                    .and_then(|p| p.current_value.as_deref())
                    == Some("42")
            })
        })
        .await;
        let cached = cache.get("thing-1").expect("cached thing");
        assert_eq!(cached.status, ThingStatus::Online, "成功读数必须保持 Online");
        assert!(
            cached.last_heartbeat.is_some(),
            "每次成功轮询都必须刷新 last_heartbeat（管理面 5 分钟在线判定）"
        );

        // 2) EventBus 上出现同一条 PropertyChange（value=42）。
        loop {
            let event = tokio::time::timeout(std::time::Duration::from_secs(2), events.recv())
                .await
                .expect("PropertyChange within deadline")
                .expect("event channel open");
            if matches!(event.event_type(), EventType::Device(ThingEventType::PropertyChange)) {
                assert_eq!(event.source().thing_id(), Some("thing-1"));
                assert_eq!(
                    event.content().metadata().get("value"),
                    Some(&serde_json::Value::String("42".to_string()))
                );
                break;
            }
        }

        // 3) 终态失败：Online → Offline，并发出一次 Connection(Warning)。
        failing.store(true, Ordering::SeqCst);
        wait_until(|| {
            cache
                .get("thing-1")
                .is_some_and(|device| device.status == ThingStatus::Offline)
        })
        .await;
        loop {
            let event = tokio::time::timeout(std::time::Duration::from_secs(2), events.recv())
                .await
                .expect("offline transition event within deadline")
                .expect("event channel open");
            if matches!(event.event_type(), EventType::Device(ThingEventType::Connection))
                && event.level() == EventLevel::Warning
            {
                break;
            }
        }

        // 4) 同一失败 streak 内（>=3 个 100ms tick）不得再发 offline 事件。
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(350);
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            match tokio::time::timeout(remaining, events.recv()).await {
                Ok(Ok(event)) => assert!(
                    !(matches!(event.event_type(), EventType::Device(ThingEventType::Connection))
                        && event.level() == EventLevel::Warning),
                    "失败 streak 内不得重复发 offline 迁移事件"
                ),
                Ok(Err(_)) => break,
                Err(_) => break,
            }
        }
        assert_eq!(cache.get("thing-1").expect("cached thing").status, ThingStatus::Offline);

        let _ = shutdown_tx.send(());
    }

    // ── E4c Task 4：stop 立即返回 + 取消异步留痕 ─────────────────────

    /// 协作式阻塞读：进入驱动后通知测试，放行后检查取消令牌。
    struct CooperativeGateDriver {
        thing: Thing,
        started: Arc<tokio::sync::Notify>,
        released: Arc<AtomicBool>,
        observed_cancel: Arc<AtomicBool>,
    }

    impl ThingDriver for CooperativeGateDriver {
        fn thing(&self) -> &Thing {
            &self.thing
        }
        fn thing_mut(&mut self) -> &mut Thing {
            &mut self.thing
        }
        fn mode(&self) -> DriverMode {
            DriverMode::Poll
        }
        fn read_data(&mut self, cancel: &CancelToken) -> Result<Vec<ResultValue>, Error> {
            self.started.notify_one();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !self.released.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            if cancel.is_cancelled() {
                self.observed_cancel.store(true, Ordering::SeqCst);
                return Err(Error::Cancelled);
            }
            Ok(vec![])
        }
        fn execute_command(&mut self, _cmd: &ThingCommand, cancel: &CancelToken) -> Result<bool, Error> {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            Ok(true)
        }
    }

    /// `cancel_calls` 置在途调用的取消令牌并返回命中数；空闲/结束后返回 0。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancel_calls_cancels_in_flight_and_returns_count() {
        let thing = worker_thing("dev-1");
        let cache = Arc::new(VecCache::new(vec![thing.clone()]));
        let server = DataServer::new(cache, Arc::new(EventBus::new()));
        let started = Arc::new(tokio::sync::Notify::new());
        let released = Arc::new(AtomicBool::new(false));
        let observed_cancel = Arc::new(AtomicBool::new(false));
        server.register_worker(
            "dev-1",
            DriverWrapper::new(Box::new(CooperativeGateDriver {
                thing,
                started: started.clone(),
                released: released.clone(),
                observed_cancel: observed_cancel.clone(),
            })),
        );
        let worker = server.workers().get("dev-1").expect("worker").clone();
        worker.submit_read(Vec::new());
        tokio::time::timeout(std::time::Duration::from_secs(2), started.notified())
            .await
            .expect("read must start");
        assert_eq!(server.cancel_calls("dev-1"), 1, "在途调用应被命中");
        assert_eq!(server.cancel_calls("missing"), 0, "无 worker 时返回 0");

        released.store(true, Ordering::SeqCst);
        wait_until(|| observed_cancel.load(Ordering::SeqCst) && !worker.in_flight()).await;
        assert_eq!(server.cancel_calls("dev-1"), 0, "调用已结束后不应再报告在途");
        server.remove_device("dev-1");
    }

    /// 假账本：记录 (workspace, kind, thing_id, payload)。
    #[derive(Default)]
    struct RecordingLedger(std::sync::Mutex<Vec<(String, String, Option<String>, serde_json::Value)>>);

    impl RecordingLedger {
        fn rows(&self) -> Vec<(String, String, Option<String>, serde_json::Value)> {
            self.0.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl HubLedger for RecordingLedger {
        async fn append(
            &self,
            workspace_id: &str,
            kind: &str,
            thing_id: Option<&str>,
            payload: serde_json::Value,
        ) -> Result<(), String> {
            self.0.lock().unwrap().push((
                workspace_id.to_string(),
                kind.to_string(),
                thing_id.map(str::to_string),
                payload,
            ));
            Ok(())
        }
    }

    /// 取消终态：事件 metadata 带 outcome/elapsedMs/thingId，账本每次调用一行；
    /// 计数按宽限内/未知分开。
    #[tokio::test]
    async fn cancelled_result_publishes_event_and_appends_ledger_row() {
        use tinyiothub_core::models::event::{EventType, ThingEventType};

        let thing = worker_thing("dev-1");
        let cache: Arc<dyn ThingCacheSource> = Arc::new(VecCache::new(vec![thing]));
        let bus = Arc::new(EventBus::new());
        let server = DataServer::new(cache.clone(), bus.clone());
        let ledger = Arc::new(RecordingLedger::default());
        server.set_hub_ledger(ledger.clone());
        let mut events = bus.subscribe();

        DataServer::consume_worker_result(
            WorkerResult::Cancelled {
                thing_id: "dev-1".to_string(),
                kind: CancelKind::Execute,
                within_grace: true,
                elapsed_ms: 42,
            },
            &cache,
            bus.clone(),
            &server.cancellation,
        )
        .await;

        let event = tokio::time::timeout(std::time::Duration::from_secs(2), events.recv())
            .await
            .expect("cancelled event within deadline")
            .expect("event channel open");
        assert_eq!(
            event.event_type(),
            &EventType::Device(ThingEventType::CommandCancelled),
            "取消终态必须发 CommandCancelled"
        );
        let metadata = event.content().metadata();
        assert_eq!(
            metadata.get("outcome"),
            Some(&serde_json::json!("cancelled")),
            "宽限内退出 outcome=cancelled"
        );
        assert_eq!(metadata.get("elapsedMs"), Some(&serde_json::json!(42)));
        assert_eq!(metadata.get("thingId"), Some(&serde_json::json!("dev-1")));
        assert_eq!(metadata.get("kind"), Some(&serde_json::json!("execute")));
        assert_eq!(event.source().thing_id(), Some("dev-1"));

        let rows = ledger.rows();
        assert_eq!(rows.len(), 1, "每次取消一条账本");
        assert_eq!(rows[0].0, "ws-1");
        assert_eq!(rows[0].1, "call_cancelled");
        assert_eq!(rows[0].2.as_deref(), Some("dev-1"));
        assert_eq!(
            rows[0].3,
            serde_json::json!({"v": 1, "thingId": "dev-1", "outcome": "cancelled", "kind": "execute", "elapsedMs": 42})
        );
        assert_eq!(server.cancelled_counts(), (1, 0));

        // 不合作驱动迟到返回：outcome/kind 都是 *_unknown。
        DataServer::consume_worker_result(
            WorkerResult::Cancelled {
                thing_id: "dev-1".to_string(),
                kind: CancelKind::Read,
                within_grace: false,
                elapsed_ms: 1500,
            },
            &cache,
            bus,
            &server.cancellation,
        )
        .await;
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), events.recv())
            .await
            .expect("unknown event within deadline")
            .expect("event channel open");
        assert_eq!(event.event_type(), &EventType::Device(ThingEventType::CommandCancelled));
        assert_eq!(
            event.content().metadata().get("outcome"),
            Some(&serde_json::json!("cancelled_unknown"))
        );
        // 读取消必须在审计里显式标记 kind=read（Issue 8）——账本 kind 仍是
        // call_cancelled_unknown（kind-agnostic 路径保留）。
        assert_eq!(event.content().metadata().get("kind"), Some(&serde_json::json!("read")));
        let rows = ledger.rows();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].1, "call_cancelled_unknown");
        assert_eq!(rows[1].3["outcome"], "cancelled_unknown");
        assert_eq!(rows[1].3["kind"], "read");
        assert_eq!(rows[1].3["elapsedMs"], 1500);
        assert_eq!(server.cancelled_counts(), (1, 1));
    }

    /// 驱动返回 `Error::Cancelled` 不计失败预算、不标离线、不进重试退避。
    struct AlwaysCancelledDriver {
        thing: Thing,
        calls: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl ThingDriver for AlwaysCancelledDriver {
        fn thing(&self) -> &Thing {
            &self.thing
        }
        fn thing_mut(&mut self) -> &mut Thing {
            &mut self.thing
        }
        fn read_data(&mut self, _cancel: &CancelToken) -> Result<Vec<ResultValue>, Error> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Err(Error::Cancelled)
        }
        fn execute_command(&mut self, _cmd: &ThingCommand, _cancel: &CancelToken) -> Result<bool, Error> {
            Err(Error::Cancelled)
        }
    }

    #[test]
    fn cancelled_error_does_not_count_as_driver_failure() {
        use std::sync::atomic::AtomicUsize;

        let calls = Arc::new(AtomicUsize::new(0));
        let mut wrapper = DriverWrapper::new(Box::new(AlwaysCancelledDriver {
            thing: worker_thing("dev-1"),
            calls: calls.clone(),
        }));

        assert!(wrapper.read_data_once().result.is_err());
        let health = wrapper.overview().health;
        assert_eq!(health.total_failures, 0, "取消不得计失败");
        assert_eq!(health.consecutive_failures, 0);
        assert_eq!(health.total_attempts, 0);

        // 未记录失败 → 下次调用不进入退避，驱动再次被调用。
        assert!(wrapper.read_data_once().result.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 2, "取消后不得进入重试退避");
    }
}
