use std::sync::Arc;

use dashmap::DashMap;
use moka::sync::Cache;
use parking_lot::RwLock;

use tinyiothub_core::error::Error;
use tinyiothub_core::models::device_property::DeviceProperty;
use tinyiothub_core::models::{device::Device, device_command::DeviceCommand};
use tinyiothub_core::models::event::{
    ContentElement, DeviceEventType, Event as DomainEvent, EventLevel, EventSource, RichContent,
    TextFormat,
};

use tinyiothub_storage::cache::DeviceCache;
use crate::driver::{create_driver, DeviceOverview, DriverWrapper, ResultValue};
use crate::event_bus::{publish_event_safe, EventBus, EventHandler};

// Type aliases
type DriverCache = Cache<String, Arc<RwLock<DriverWrapper>>>;
type CommandQueue = Arc<DashMap<String, Vec<DeviceCommand>>>;

/// Device data server — manages driver lifecycle and the polling loop.
///
/// Lives in `tinyiothub-engine` so it can be reused by both cloud and edge
/// binaries without dragging in cloud-specific infrastructure.
pub struct DataServer {
    device_cache: Arc<DeviceCache>,
    driver_cache: DriverCache,
    command_queue: CommandQueue,
    event_bus: Arc<EventBus>,
}

impl DataServer {
    pub fn new(device_cache: Arc<DeviceCache>, event_bus: Arc<EventBus>) -> Self {
        let driver_cache = Cache::new(10_000);
        Self::initialize_drivers(&driver_cache, &device_cache, &event_bus);
        Self {
            device_cache,
            driver_cache,
            command_queue: Arc::new(DashMap::new()),
            event_bus,
        }
    }

    fn initialize_drivers(
        cache: &DriverCache,
        device_cache: &Arc<DeviceCache>,
        event_bus: &Arc<EventBus>,
    ) {
        let devices = device_cache.all();
        tracing::info!("initialize_drivers: {} device(s) in cache", devices.len());
        for device in devices {
            tracing::debug!(
                "Device {}: driver_name={:?}, props={}",
                device.name, device.driver_name,
                device.properties.as_ref().map(|p| p.len()).unwrap_or(0)
            );
            if let Some(driver_name) = &device.driver_name {
                match create_driver(driver_name, &device) {
                    Ok(mut driver) => {
                        driver.set_event_bus(event_bus.clone());
                        cache.insert(device.id.clone(), Arc::new(RwLock::new(driver)));
                        tracing::debug!(
                            "Loaded driver for device: {}",
                            device.display_name.as_deref().unwrap_or(&device.name)
                        );
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to create driver for device '{}': {}",
                            device.display_name.as_deref().unwrap_or(&device.name),
                            e
                        );
                    }
                }
            } else {
                tracing::warn!("Device '{}' has no driver_name, skipping", device.name);
            }
        }
        tracing::info!("Driver cache now has {} entries", cache.entry_count());
    }

    /// Core async loop. Spawns a single task that polls all drivers.
    pub async fn run(
        &self,
        shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    ) -> Result<(), Error> {
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
        Ok(())
    }

    async fn process_all_drivers(
        driver_cache: DriverCache,
        device_cache: Arc<DeviceCache>,
        command_queue: CommandQueue,
    ) {
        tracing::info!("Starting data server polling loop for all drivers");
        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(100));
        loop {
            interval.tick().await;

            let mut drivers = Vec::new();
            for (_, driver_arc) in driver_cache.iter() {
                drivers.push(driver_arc.clone());
            }

            if drivers.is_empty() {
                tracing::debug!("No drivers found in cache");
            }

            // Collect commands from all protocol queues.
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
                        driver.can_read_now()
                    } else {
                        false
                    }
                };
                if !can_read {
                    tracing::debug!("Driver not ready for read (retry backoff)");
                    continue;
                }

                // Collect events INSIDE the write lock, publish OUTSIDE to avoid deadlock.
                let mut pending_events: Vec<DomainEvent> = Vec::new();
                let mut event_bus_ref: Option<std::sync::Arc<EventBus>> = None;
                let mut device_id = String::new();
                let mut updated_device: Option<Device> = None;

                if let Some(mut driver) = driver_arc.try_write() {
                    device_id = driver.device().id.clone();
                    let read_result = driver.read_data_once();
                    let mut device = driver.device().clone();
                    let was_online = device.is_online();
                    let device_display_name =
                        device.display_name.clone().unwrap_or_else(|| device.name.clone());
                    let device_address = device.address.clone();
                    let prop_count = device.properties.as_ref().map(|p| p.len()).unwrap_or(0);

                    event_bus_ref = driver.event_bus_ref().cloned();

                    match read_result.result {
                        Ok(values) => {
                            tracing::debug!(
                                "Device '{}' read {} values ({} properties)",
                                device_display_name, values.len(), prop_count
                            );
                            if !was_online {
                                if let Some(event) = driver.on_connected(device_address) {
                                    pending_events.push(event);
                                }
                                tracing::info!(
                                    "Device '{}' state changed: offline → online",
                                    device_display_name
                                );
                            }
                            device.status = tinyiothub_core::models::device::DeviceStatus::Online;
                            device.last_heartbeat =
                                Some(chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string());
                            let events = Self::collect_property_change_events(
                                &mut device,
                                &values,
                                event_bus_ref.as_deref(),
                            );
                            tracing::debug!(
                                "Device '{}' generated {} property change event(s)",
                                device_display_name, events.len()
                            );
                            pending_events.extend(events);
                        }
                        Err(e) => {
                            if let Some(retry_info) = &read_result.retry_info {
                                if retry_info.will_retry {
                                    continue;
                                } else {
                                    if was_online {
                                        if let Some(event) = driver.on_disconnected(Some(e.to_string())) {
                                            pending_events.push(event);
                                        }
                                    }
                                    device.status = tinyiothub_core::models::device::DeviceStatus::Offline;
                                }
                            } else {
                                if was_online {
                                    if let Some(event) = driver.on_disconnected(Some(e.to_string())) {
                                        pending_events.push(event);
                                    }
                                }
                                device.status = tinyiothub_core::models::device::DeviceStatus::Offline;
                            }
                        }
                    }

                    for cmd in all_commands.iter().filter(|c| c.device_id == device_id) {
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

                    *driver.device_mut() = device.clone();
                    updated_device = Some(device);
                } else {
                    tracing::warn!("Driver write lock contended, skipping this tick");
                }
                // Write lock released — update cache and publish events
                if let Some(device) = updated_device {
                    device_cache.update(device);
                }
                if let Some(bus) = event_bus_ref {
                    if !pending_events.is_empty() {
                        tracing::debug!("Publishing {} events for device {}", pending_events.len(), device_id);
                    }
                    for event in pending_events {
                        publish_event_safe(bus.clone(), event).await;
                    }
                }
            }
        }
    }

    /// Collect property change events without publishing.
    ///
    /// Returns events that need to be published AFTER the driver write lock is released.
    /// This avoids a deadlock: publish_property_change calls self.device() which
    /// tries to read-lock the same RwLock that the caller holds as a write lock.
    fn collect_property_change_events(
        device: &mut Device,
        values: &[ResultValue],
        _event_bus: Option<&EventBus>,
    ) -> Vec<DomainEvent> {
        let mut pending_events = Vec::new();
        let device_id = device.id.clone();
        let device_name = device.name.clone();
        if let Some(ref mut properties) = device.properties {
            let value_names: Vec<&str> = values.iter().map(|v| v.name.as_str()).collect();
            let prop_names: Vec<&str> = properties.iter().map(|p| p.name.as_str()).collect();
            tracing::debug!(
                "Device '{}' properties: {:?}, driver values: {:?}",
                device_name, prop_names, value_names
            );
            for property in properties.iter_mut() {
                if let Some(result_value) = values.iter().find(|v| v.name == property.name) {
                    if let Some(ref value_str) = result_value.value {
                        let old_value = property.current_value.clone();
                        let value_changed = match &old_value {
                            Some(old_val) => old_val != value_str,
                            None => true,
                        };
                        property.set_current_value(value_str.clone());
                        if value_changed {
                            tracing::debug!(
                                "Property '{}' changed on device '{}': {:?} -> {}",
                                property.name, device_name, old_value, value_str
                            );
                            if let Some(event) = Self::build_property_change_event_static(
                                &device_id, &device_name, property, old_value, value_str,
                            ) {
                                pending_events.push(event);
                            }
                        }
                    }
                } else {
                    tracing::debug!(
                        "No driver value for property '{}' on device '{}'",
                        property.name, device_name
                    );
                }
            }
        }
        pending_events
    }

    /// Build a property change event without acquiring any locks.
    /// Takes pre-extracted device fields to avoid borrow conflicts.
    fn build_property_change_event_static(
        device_id: &str,
        device_name: &str,
        property: &DeviceProperty,
        old_value: Option<String>,
        new_value: &str,
    ) -> Option<DomainEvent> {
        let mut elements = vec![ContentElement::Text {
            content: format!(
                "Property '{}' value changed on device '{}'",
                property.name, device_name
            ),
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

        DomainEvent::new_device_event(
            DeviceEventType::PropertyChange,
            EventLevel::Info,
            EventSource::device_property(
                device_id.to_string(),
                property.id.clone(),
                "data_collector".to_string(),
            ),
            RichContent::new(
                format!("Property Changed: {} - {}", device_name, property.name),
                elements,
            ),
        )
        .ok()
    }

    /// Build a command execution event without acquiring any locks.
    fn build_command_event(
        device: &Device,
        cmd: &DeviceCommand,
        success: bool,
        execution_time_ms: u64,
        error_message: Option<String>,
    ) -> Option<DomainEvent> {
        let (event_type, level, status) = if success {
            (DeviceEventType::CommandCompleted, EventLevel::Info, "success")
        } else {
            (DeviceEventType::CommandFailed, EventLevel::Error, "failed")
        };

        let mut elements = vec![
            ContentElement::Text {
                content: format!(
                    "Command '{}' executed on device '{}'",
                    cmd.name, device.name
                ),
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

    pub fn get_devices(&self) -> Vec<Device> {
        self.device_cache.all()
    }

    pub fn get_device(&self, id: &str) -> Option<Device> {
        self.device_cache.get(id)
    }

    pub fn get_device_by_name(&self, name: &str) -> Option<Device> {
        self.device_cache.get_by_name(name)
    }

    pub fn execute_command(&self, cmd: DeviceCommand) -> Result<(), Error> {
        if let Some(device) = self.device_cache.get(&cmd.device_id) {
            let protocol = device.driver_name.unwrap_or_else(|| "unknown".to_string());
            self.command_queue.entry(protocol).or_default().push(cmd);
            Ok(())
        } else {
            Err(Error::NotFound)
        }
    }

    pub fn execute_commands(&self, cmds: Vec<DeviceCommand>) -> Result<(), Error> {
        for cmd in cmds {
            self.execute_command(cmd)?;
        }
        Ok(())
    }

    pub fn remove_device(&self, device_id: &str) {
        self.device_cache.remove(device_id);
        self.driver_cache.invalidate(device_id);
        tracing::info!("Removed device and driver: {}", device_id);
    }

    pub fn remove_devices(&self, device_ids: &[String]) {
        for device_id in device_ids {
            self.remove_device(device_id);
        }
    }

    pub fn get_device_overview(&self, device_id: &str) -> Option<DeviceOverview> {
        self.driver_cache
            .get(device_id)
            .and_then(|driver_arc| driver_arc.try_read().map(|driver| driver.overview()))
    }

    pub fn get_all_device_overview(&self) -> Vec<DeviceOverview> {
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

    pub fn reset_device_driver(&self, device_id: &str) -> bool {
        if let Some(driver_arc) = self.driver_cache.get(device_id) {
            if let Some(mut driver) = driver_arc.try_write() {
                driver.reset();
                tracing::info!("Reset driver for device: {}", device_id);
                return true;
            }
        }
        false
    }

    pub fn set_device_offline(&self, device_id: &str) -> bool {
        if let Some(driver_arc) = self.driver_cache.get(device_id) {
            if let Some(mut driver) = driver_arc.try_write() {
                driver.set_offline();
                tracing::info!("Set device offline: {}", device_id);
                return true;
            }
        }
        false
    }

    pub fn get_driver_cache_size(&self) -> u64 {
        self.driver_cache.entry_count()
    }

    pub fn cleanup_driver_cache(&self) {
        let mut to_remove = Vec::new();
        for (device_id, _) in self.driver_cache.iter() {
            // 只清理设备已从 device_cache 中删除的孤儿驱动
            if self.device_cache.get(&device_id).is_none() {
                to_remove.push(device_id.to_string());
            }
        }
        for device_id in to_remove {
            self.driver_cache.invalidate(&device_id);
            tracing::info!("Removed orphan driver for deleted device: {}", device_id);
        }
    }
}

#[async_trait::async_trait]
impl EventHandler for DataServer {
    async fn handle(
        &self,
        event: &tinyiothub_core::models::event::Event,
    ) -> Result<(), Error> {
        use tinyiothub_core::models::event::{DeviceEventType, EventType};

        let device_id = match event.source().device_id() {
            Some(id) => id,
            None => return Ok(()),
        };

        match event.event_type() {
            EventType::Device(device_event_type) => match device_event_type {
                DeviceEventType::DeviceCreated | DeviceEventType::DeviceUpdated => {
                    tracing::info!(
                        "Handling {:?} event for device: {}",
                        device_event_type,
                        device_id
                    );
                    // NOTE: reload_device was removed because it depends on
                    // cloud-specific DeviceService / Repository.
                    // The cloud crate should call DataServer::reload_device
                    // after updating the DeviceCache.
                }
                DeviceEventType::DeviceDeleted => {
                    tracing::info!("Handling DeviceDeleted event for device: {}", device_id);
                    self.remove_device(device_id);
                }
                DeviceEventType::PropertyChange => {
                    // DataServer does NOT handle PropertyChange events.
                    // The device cache is already updated inline by the polling
                    // loop (device_cache.update()). Consuming our own events
                    // here would be redundant and introduces deadlock risk
                    // (update_property contends for the same DashMap shard).
                }
                _ => {}
            },
            _ => {}
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
        }
    }
}
