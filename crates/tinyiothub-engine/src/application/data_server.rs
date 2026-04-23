use std::{collections::HashMap, sync::Arc};

use dashmap::DashMap;
use moka::sync::Cache;
use parking_lot::RwLock;

use tinyiothub_core::error::Error;
use tinyiothub_core::models::{device::Device, device_command::DeviceCommand};

use tinyiothub_storage::cache::DeviceCache;
use crate::driver::{create_driver, DeviceOverview, DriverWrapper, ResultValue};
use crate::event_bus::{EventBus, EventHandler};

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
        for device in device_cache.all() {
            if let Some(driver_name) = &device.driver_name {
                match create_driver(driver_name, &device) {
                    Ok(mut driver) => {
                        driver.set_event_bus(event_bus.clone());
                        cache.insert(device.id.clone(), Arc::new(RwLock::new(driver)));
                        tracing::info!(
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
            }
        }
    }

    /// Core async loop. Spawns one task per protocol group.
    pub async fn run(
        &self,
        shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    ) -> Result<(), Error> {
        let driver_groups = self.group_drivers_by_protocol();
        for (protocol, _) in driver_groups {
            let driver_cache = self.driver_cache.clone();
            let device_cache = self.device_cache.clone();
            let command_queue = self.command_queue.clone();
            let mut shutdown_rx_clone = shutdown_rx.resubscribe();
            tokio::spawn(async move {
                tokio::select! {
                    _ = Self::process_protocol_drivers(protocol.clone(), driver_cache, device_cache, command_queue) => {
                        tracing::info!("Driver task for protocol {} completed", protocol);
                    }
                    _ = shutdown_rx_clone.recv() => {
                        tracing::info!("Driver task for protocol {} received shutdown signal", protocol);
                    }
                }
            });
        }
        Ok(())
    }

    async fn process_protocol_drivers(
        protocol: String,
        driver_cache: DriverCache,
        device_cache: Arc<DeviceCache>,
        command_queue: CommandQueue,
    ) {
        tracing::info!("Starting data server loop for protocol: {}", protocol);
        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(100));
        loop {
            interval.tick().await;

            let mut drivers = Vec::new();
            for (_, driver_arc) in driver_cache.iter() {
                if let Some(driver) = driver_arc.try_read() {
                    if driver.device().driver_name.as_deref() == Some(&protocol) {
                        drivers.push(driver_arc.clone());
                    }
                }
            }

            let commands = command_queue
                .remove(&protocol)
                .map(|(_, cmds)| cmds)
                .unwrap_or_default();

            for driver_arc in &drivers {
                let can_read = {
                    if let Some(driver) = driver_arc.try_read() {
                        driver.can_read_now()
                    } else {
                        false
                    }
                };
                if !can_read {
                    continue;
                }

                if let Some(mut driver) = driver_arc.try_write() {
                    let device_id = driver.device().id.clone();
                    let read_result = driver.read_data_once();
                    let mut device = driver.device().clone();
                    let was_online = device.is_online;
                    let device_display_name =
                        device.display_name.clone().unwrap_or_else(|| device.name.clone());
                    let device_address = device.address.clone();

                    match read_result.result {
                        Ok(values) => {
                            if !was_online {
                                driver.on_connected(device_address);
                                tracing::info!(
                                    "Device '{}' state changed: offline → online",
                                    device_display_name
                                );
                            }
                            device.is_online = true;
                            device.state = Some(1);
                            device.last_heartbeat =
                                Some(chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string());
                            Self::update_device_properties_with_events(
                                &mut device,
                                &values,
                                &mut driver,
                            );
                        }
                        Err(e) => {
                            if let Some(retry_info) = &read_result.retry_info {
                                if retry_info.will_retry {
                                    tracing::debug!(
                                        "Device {} read failed (attempt {}), will retry in {:?}: {}",
                                        device_display_name,
                                        retry_info.attempt,
                                        retry_info.next_retry_in.unwrap_or_default(),
                                        e
                                    );
                                    continue;
                                } else {
                                    if was_online {
                                        driver.on_disconnected(Some(e.to_string()));
                                        tracing::warn!(
                                            "Device '{}' state changed: online → offline after {} attempts",
                                            device_display_name,
                                            retry_info.attempt
                                        );
                                    }
                                    device.is_online = false;
                                    device.state = Some(0);
                                }
                            } else {
                                if was_online {
                                    driver.on_disconnected(Some(e.to_string()));
                                }
                                device.is_online = false;
                                device.state = Some(0);
                                tracing::warn!("Device {} read error: {}", device_display_name, e);
                            }
                        }
                    }

                    for cmd in commands.iter().filter(|c| c.device_id == device_id) {
                        let cmd_result = driver.execute_command(cmd);
                        let execution_time_ms = cmd_result.elapsed.as_millis() as u64;
                        match cmd_result.result {
                            Ok(_) => {
                                driver.publish_command_execution(
                                    cmd.name.clone(),
                                    true,
                                    execution_time_ms,
                                    None,
                                );
                            }
                            Err(e) => {
                                driver.publish_command_execution(
                                    cmd.name.clone(),
                                    false,
                                    execution_time_ms,
                                    Some(e.to_string()),
                                );
                            }
                        }
                    }

                    *driver.device_mut() = device.clone();
                    device_cache.update(device);
                }
            }
        }
    }

    fn update_device_properties_with_events(
        device: &mut Device,
        values: &[ResultValue],
        driver: &mut DriverWrapper,
    ) {
        if let Some(ref mut properties) = device.properties {
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
                            driver.publish_property_change(
                                property.id.clone(),
                                property.name.clone(),
                                old_value,
                                value_str.clone(),
                            );
                        }
                    }
                }
            }
        }
    }

    fn group_drivers_by_protocol(&self) -> HashMap<String, Vec<Arc<RwLock<DriverWrapper>>>> {
        let mut groups = HashMap::new();
        for (_, driver_arc) in self.driver_cache.iter() {
            if let Some(driver) = driver_arc.try_read() {
                let protocol = driver
                    .device()
                    .driver_name
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string());
                groups
                    .entry(protocol)
                    .or_insert_with(Vec::new)
                    .push(driver_arc.clone());
            }
        }
        groups
    }

    /// Handle a property-change event by updating the in-memory cache.
    ///
    /// The event is produced by `DriverWrapper::publish_property_change`:
    /// - `source.source_id` = "{device_id}:{property_id}"
    /// - `content.title`    = "Property Changed: {device_name} - {property_name}"
    /// - last text element  = "Current value: {new_value}"
    fn handle_property_change_event(
        &self,
        event: &tinyiothub_core::models::event::Event,
        device_id: &str,
    ) {
        use tinyiothub_core::models::event::ContentElement;

        let source_id = event.source().source_id();
        let property_id = source_id
            .split(':')
            .nth(1)
            .unwrap_or("");

        // Extract property name from title: "Property Changed: DeviceName - PropertyName"
        let title = event.content().title();
        let property_name = title
            .rfind(" - ")
            .map(|pos| &title[pos + 3..])
            .unwrap_or("");

        // Extract new value from the last "Current value: X" text element
        let new_value = event
            .content()
            .elements()
            .iter()
            .filter_map(|e| match e {
                ContentElement::Text { content, .. } => {
                    content.strip_prefix("Current value: ").map(|s| s.to_string())
                }
                _ => None,
            })
            .next();

        let Some(new_value) = new_value else {
            tracing::debug!(
                "PropertyChange event for device {} missing 'Current value' element",
                device_id
            );
            return;
        };

        if property_id.is_empty() && property_name.is_empty() {
            tracing::debug!(
                "PropertyChange event for device {} has no property identifier",
                device_id
            );
            return;
        }

        self.device_cache.update_property(device_id, property_id, |device| {
            if let Some(ref mut properties) = device.properties {
                for prop in properties.iter_mut() {
                    if prop.id == property_id || prop.name == property_name {
                        prop.set_current_value(new_value.clone());
                        tracing::debug!(
                            "Updated device cache: device={}, property={}, value={}",
                            device_id,
                            prop.name,
                            new_value
                        );
                        break;
                    }
                }
            }
        });
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
        for (device_id, driver_arc) in self.driver_cache.iter() {
            if driver_arc.try_read().is_none() {
                to_remove.push(device_id.to_string());
            }
        }
        for device_id in to_remove {
            self.driver_cache.invalidate(&device_id);
            tracing::info!("Removed invalid driver for device: {}", device_id);
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
                    self.handle_property_change_event(event, device_id);
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
