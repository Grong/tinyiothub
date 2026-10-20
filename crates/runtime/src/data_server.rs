use std::sync::Arc;

use dashmap::DashMap;
use moka::sync::Cache;
use parking_lot::RwLock;

use tinyiothub_core::driver::{DriverMode, DriverReading, ResultValue, ValueSink};
use tinyiothub_core::error::Error;
use tinyiothub_core::event::EventHandler;
use tinyiothub_core::models::event::{
    ContentElement, Event as DomainEvent, EventLevel, EventSource, RichContent, TextFormat, ThingEventType,
};
use tinyiothub_core::models::thing_property::ThingProperty;
use tinyiothub_core::models::{thing::Thing, thing_command::ThingCommand};

use crate::driver::{DriverWrapper, ThingOverview, create_driver};
use crate::event_bus::{EventBus, publish_event_safe};
use crate::ports::ThingCacheSource;

pub type DriverCache = Cache<String, Arc<RwLock<DriverWrapper>>>;
type CommandQueue = Arc<DashMap<String, Vec<ThingCommand>>>;

/// Thing data server — manages driver lifecycle and the polling loop.
pub struct DataServer {
    device_cache: Arc<dyn ThingCacheSource>,
    driver_cache: DriverCache,
    command_queue: CommandQueue,
    event_bus: Arc<EventBus>,
    /// 推送驱动上报入口（spec §一 ValueSink）：Task 7 的 Manager 注册推送驱动时经 `value_sink()` 注入。
    value_sink_tx: ValueSink,
    /// 消费端只能被 `run()` 取走一次（take 后为 None，重复 run 不再 spawn 消费任务）。
    value_sink_rx: Arc<parking_lot::Mutex<Option<tokio::sync::mpsc::Receiver<DriverReading>>>>,
}

impl DataServer {
    pub fn new(device_cache: Arc<dyn ThingCacheSource>, event_bus: Arc<EventBus>) -> Self {
        let driver_cache = Cache::new(10_000);
        let (value_sink_tx, value_sink_rx) = tokio::sync::mpsc::channel(256);
        Self::initialize_drivers(&driver_cache, &device_cache, &event_bus);
        Self {
            device_cache,
            driver_cache,
            command_queue: Arc::new(DashMap::new()),
            event_bus,
            value_sink_tx,
            value_sink_rx: Arc::new(parking_lot::Mutex::new(Some(value_sink_rx))),
        }
    }

    /// 推送驱动的上报句柄（Task 7 注册推送驱动时注入给 `start_push`）。
    pub fn value_sink(&self) -> ValueSink {
        self.value_sink_tx.clone()
    }

    /// 驱动缓存句柄（物理驱动 wrapper 表——轮询迭代按 mode() 过滤，sink 消费
    /// 侧按 thing_id 找 wrapper 走共享管线；X4 后派生引擎不再注册进此表，
    /// 无 wrapper 的读数走 device_cache 回退）。moka `Cache` 克隆是廉价句柄，
    /// 与 `value_sink()` 同级。
    pub fn driver_cache(&self) -> DriverCache {
        self.driver_cache.clone()
    }

    fn initialize_drivers(cache: &DriverCache, device_cache: &Arc<dyn ThingCacheSource>, event_bus: &Arc<EventBus>) {
        let devices = device_cache.all();
        for device in devices {
            if let Some(driver_name) = &device.driver_name {
                match create_driver(driver_name, &device) {
                    Ok(mut driver) => {
                        driver.set_event_bus(event_bus.clone());
                        cache.insert(device.id.clone(), Arc::new(RwLock::new(driver)));
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

    pub async fn run(&self, shutdown_rx: tokio::sync::broadcast::Receiver<()>) -> Result<(), Error> {
        let driver_cache = self.driver_cache.clone();
        let device_cache = self.device_cache.clone();
        let command_queue = self.command_queue.clone();
        let mut shutdown_rx_clone = shutdown_rx.resubscribe();
        tokio::spawn(async move {
            tokio::select! {
                _ = Self::process_all_drivers(driver_cache, device_cache, command_queue) => {
                    tracing::debug!("Driver polling task completed");
                }
                _ = shutdown_rx_clone.recv() => {
                    tracing::info!("Driver polling task received shutdown signal");
                }
            }
        });

        // 推送驱动 sink 独立消费任务：不经 100ms 轮询，读数到达即走共享管线（spec §一/E3）。
        if let Some(mut rx) = self.value_sink_rx.lock().take() {
            let device_cache2 = self.device_cache.clone();
            let driver_cache2 = self.driver_cache.clone();
            let event_bus2 = self.event_bus.clone();
            let mut shutdown_rx2 = shutdown_rx.resubscribe();
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = shutdown_rx2.recv() => break,
                        maybe = rx.recv() => {
                            let Some(reading) = maybe else { break };
                            Self::consume_driver_reading(reading, &driver_cache2, &device_cache2, event_bus2.clone()).await;
                        }
                    }
                }
            });
        }
        Ok(())
    }

    /// 消费一条推送读数：经 driver_cache 找到 owner 物的 wrapper，走共享管线
    /// （apply_property_values → cache_and_publish）。无 wrapper（空间物/无驱动物）
    /// 时回退 device_cache 直写（X4）；两处都无该物 → 丢弃并告警（物已删的迟到读数）。
    pub async fn consume_driver_reading(
        reading: DriverReading,
        driver_cache: &DriverCache,
        device_cache: &Arc<dyn ThingCacheSource>,
        event_bus: Arc<EventBus>,
    ) {
        let Some(driver_arc) = driver_cache.get(&reading.thing_id) else {
            // X4：无 wrapper → device_cache 回退（spec §一）
            let Some(mut device) = device_cache.get(&reading.thing_id) else {
                tracing::warn!(thing_id = %reading.thing_id, "sink 读数丢弃：物不在 wrapper 也不在缓存（已删？）");
                return;
            };
            let thing_name = device.name.clone();
            let events = Self::apply_property_values(&mut device, &thing_name, &reading.values);
            Self::cache_and_publish(device, events, device_cache, Some(event_bus)).await;
            return;
        };
        let (device, events) = {
            let mut driver = driver_arc.write();
            let mut device = driver.thing().clone();
            let thing_name = device.name.clone();
            let events = Self::apply_property_values(&mut device, &thing_name, &reading.values);
            *driver.thing_mut() = device.clone();
            (device, events)
        };
        Self::cache_and_publish(device, events, device_cache, Some(event_bus)).await;
    }

    async fn process_all_drivers(
        driver_cache: DriverCache,
        device_cache: Arc<dyn ThingCacheSource>,
        command_queue: CommandQueue,
    ) {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(100));
        loop {
            interval.tick().await;

            let mut drivers = Vec::new();
            for (_, driver_arc) in driver_cache.iter() {
                drivers.push(driver_arc.clone());
            }

            let mut all_commands = Vec::new();
            let keys: Vec<String> = command_queue.iter().map(|e| e.key().clone()).collect();
            for key in keys {
                if let Some((_, cmds)) = command_queue.remove(&key) {
                    all_commands.extend(cmds);
                }
            }

            for driver_arc in &drivers {
                let can_read = {
                    if let Some(driver) = driver_arc.try_read() {
                        // 推送驱动不经轮询拉取（spec §一）
                        driver.inner_driver().mode() == DriverMode::Poll && driver.can_read_now()
                    } else {
                        false
                    }
                };
                if !can_read {
                    continue;
                }

                let mut pending_events: Vec<DomainEvent> = Vec::new();
                let mut event_bus_ref: Option<std::sync::Arc<EventBus>> = None;
                let mut updated_device: Option<Thing> = None;

                if let Some(mut driver) = driver_arc.try_write() {
                    let thing_id = driver.thing().id.clone();
                    let read_result = driver.read_data_once();
                    let mut device = driver.thing().clone();
                    let was_online = device.is_online();
                    let device_address = device.address.clone();

                    event_bus_ref = driver.event_bus_ref().cloned();

                    match read_result.result {
                        Ok(values) => {
                            if !was_online && let Some(event) = driver.on_connected(device_address) {
                                pending_events.push(event);
                            }
                            device.status = tinyiothub_core::models::thing::ThingStatus::Online;
                            device.last_heartbeat = Some(chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string());
                            let thing_name = device.name.clone();
                            let events = Self::apply_property_values(&mut device, &thing_name, &values);
                            pending_events.extend(events);
                        }
                        Err(e) => {
                            if let Some(retry_info) = &read_result.retry_info
                                && retry_info.will_retry
                            {
                                continue;
                            }
                            if was_online && let Some(event) = driver.on_disconnected(Some(e.to_string())) {
                                pending_events.push(event);
                            }
                            device.status = tinyiothub_core::models::thing::ThingStatus::Offline;
                        }
                    }

                    for cmd in all_commands.iter().filter(|c| c.thing_id == thing_id) {
                        let cmd_result = driver.execute_command(cmd);
                        let execution_time_ms = cmd_result.elapsed.as_millis() as u64;
                        if let Some(event) = Self::build_command_event(
                            &device,
                            cmd,
                            cmd_result.result.is_ok(),
                            execution_time_ms,
                            cmd_result.result.as_ref().err().map(|e| e.to_string()),
                        ) {
                            pending_events.push(event);
                        }
                    }

                    *driver.thing_mut() = device.clone();
                    updated_device = Some(device);
                }

                if let Some(device) = updated_device {
                    Self::cache_and_publish(device, pending_events, &device_cache, event_bus_ref).await;
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
        if let Some(device) = self.device_cache.get(&cmd.thing_id) {
            let protocol = device.driver_name.unwrap_or_else(|| "unknown".to_string());
            self.command_queue.entry(protocol).or_default().push(cmd);
            Ok(())
        } else {
            Err(Error::NotFound)
        }
    }

    pub fn execute_commands(&self, cmds: Vec<ThingCommand>) -> Result<(), Error> {
        for cmd in cmds {
            self.execute_command(cmd)?;
        }
        Ok(())
    }

    pub fn remove_device(&self, thing_id: &str) {
        self.device_cache.remove(thing_id);
        self.driver_cache.invalidate(thing_id);
    }

    pub fn remove_things(&self, thing_ids: &[String]) {
        for thing_id in thing_ids {
            self.remove_device(thing_id);
        }
    }

    pub fn get_device_overview(&self, thing_id: &str) -> Option<ThingOverview> {
        self.driver_cache
            .get(thing_id)
            .and_then(|driver_arc| driver_arc.try_read().map(|driver| driver.overview()))
    }

    pub fn get_all_device_overview(&self) -> Vec<ThingOverview> {
        self.driver_cache
            .iter()
            .filter_map(|(_, driver_arc)| driver_arc.try_read().map(|driver| driver.overview()))
            .collect()
    }

    pub fn get_online_count(&self) -> usize {
        self.driver_cache
            .iter()
            .filter_map(|(_, driver_arc)| driver_arc.try_read().map(|driver| driver.is_online()))
            .filter(|&online| online)
            .count()
    }

    pub fn get_healthy_count(&self) -> usize {
        self.driver_cache
            .iter()
            .filter_map(|(_, driver_arc)| driver_arc.try_read().map(|driver| driver.is_healthy()))
            .filter(|&healthy| healthy)
            .count()
    }

    pub fn reset_device_driver(&self, thing_id: &str) -> bool {
        if let Some(driver_arc) = self.driver_cache.get(thing_id)
            && let Some(mut driver) = driver_arc.try_write()
        {
            driver.reset();
            return true;
        }
        false
    }

    pub fn set_device_offline(&self, thing_id: &str) -> bool {
        if let Some(driver_arc) = self.driver_cache.get(thing_id)
            && let Some(mut driver) = driver_arc.try_write()
        {
            driver.set_offline();
            return true;
        }
        false
    }

    pub fn get_driver_cache_size(&self) -> u64 {
        self.driver_cache.entry_count()
    }

    pub fn cleanup_driver_cache(&self) {
        let mut to_remove = Vec::new();
        for (thing_id, _) in self.driver_cache.iter() {
            if self.device_cache.get(&thing_id).is_none() {
                to_remove.push(thing_id.to_string());
            }
        }
        for thing_id in to_remove {
            self.driver_cache.invalidate(&thing_id);
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
                        // 创建驱动并加入采集循环
                        if let Some(driver_name) = &device.driver_name
                            && !self.driver_cache.contains_key(thing_id)
                        {
                            match create_driver(driver_name, &device) {
                                Ok(mut driver) => {
                                    driver.set_event_bus(self.event_bus.clone());
                                    self.driver_cache
                                        .insert(thing_id.to_string(), Arc::new(RwLock::new(driver)));
                                    tracing::info!(
                                        "Created driver for device '{}' and started data collection",
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
            driver_cache: self.driver_cache.clone(),
            command_queue: self.command_queue.clone(),
            event_bus: self.event_bus.clone(),
            value_sink_tx: self.value_sink_tx.clone(),
            value_sink_rx: self.value_sink_rx.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tinyiothub_core::driver::ThingDriver;
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

    // ── Task 7 台账追讨：Push 驱动跳过轮询的集成断言（Task 3 评审 ⚠️）──

    struct EmptyCache;

    impl ThingCacheSource for EmptyCache {
        fn all(&self) -> Vec<Thing> {
            vec![]
        }
        fn get(&self, _id: &str) -> Option<Thing> {
            None
        }
        fn get_by_name(&self, _name: &str) -> Option<Thing> {
            None
        }
        fn insert(&self, _device: Thing) {}
        fn update(&self, _device: Thing) {}
        fn remove(&self, _id: &str) {}
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
        fn read_data(&mut self) -> Result<Vec<ResultValue>, Error> {
            self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(vec![])
        }
        fn execute_command(&mut self, _cmd: &ThingCommand) -> Result<bool, Error> {
            Ok(true)
        }
    }

    /// 真实跑 `process_all_drivers` 循环：同缓存内一个 Push + 一个 Poll 驱动。
    /// Poll 计数 >0 证明循环确实在迭代；Push 计数 == 0 证明 read_data_once
    /// 路径按 mode() 跳过推送驱动（spec §一）。
    #[tokio::test]
    async fn push_mode_driver_is_never_polled() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let driver_cache: DriverCache = Cache::new(10);
        let push_reads = Arc::new(AtomicUsize::new(0));
        let poll_reads = Arc::new(AtomicUsize::new(0));
        for (id, mode, reads) in [
            ("push-1", DriverMode::Push, push_reads.clone()),
            ("poll-1", DriverMode::Poll, poll_reads.clone()),
        ] {
            let thing = Thing {
                id: id.to_string(),
                name: id.to_string(),
                ..Thing::default()
            };
            let wrapper = DriverWrapper::new(Box::new(CountingDriver { thing, mode, reads }));
            driver_cache.insert(id.to_string(), Arc::new(RwLock::new(wrapper)));
        }

        let device_cache: Arc<dyn ThingCacheSource> = Arc::new(EmptyCache);
        let command_queue: CommandQueue = Arc::new(DashMap::new());
        let handle = tokio::spawn(DataServer::process_all_drivers(
            driver_cache,
            device_cache,
            command_queue,
        ));
        // 100ms 间隔：350ms 至少 3 个 tick（首个 tick 立即触发）。
        tokio::time::sleep(tokio::time::Duration::from_millis(350)).await;
        handle.abort();

        assert!(
            poll_reads.load(Ordering::SeqCst) >= 2,
            "轮询循环必须在迭代（Poll 驱动计数 >= 2），got {}",
            poll_reads.load(Ordering::SeqCst)
        );
        assert_eq!(
            push_reads.load(Ordering::SeqCst),
            0,
            "mode()=Push 的驱动绝不经过 process_all_drivers 的 read_data_once 路径"
        );
    }
}
