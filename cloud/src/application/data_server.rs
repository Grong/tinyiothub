use std::{collections::HashMap, sync::Arc};

use dashmap::DashMap;
use moka::sync::Cache;
use parking_lot::RwLock;

use super::data_context::DataContext;
use crate::{
    domain::device::driver::{create_driver, DeviceOverview, DriverWrapper},
    dto::entity::{Device, DeviceCommand},
    shared::error::Error,
};

// 精简的类型别名
type DriverCache = Cache<String, Arc<RwLock<DriverWrapper>>>;
type CommandQueue = Arc<DashMap<String, Vec<DeviceCommand>>>;

/// 重构后的数据服务器 - 专注于驱动管理和数据处理
pub struct DataServer {
    context: Arc<DataContext>,
    driver_cache: DriverCache,
    command_queue: CommandQueue,
    event_bus: Arc<crate::infrastructure::event::EventBus>,
}

impl DataServer {
    pub fn new(
        context: Arc<DataContext>,
        event_bus: Arc<crate::infrastructure::event::EventBus>,
    ) -> Self {
        let driver_cache = Cache::new(10_000);

        // 初始化驱动
        Self::initialize_drivers(&driver_cache, &context, &event_bus);

        Self { context, driver_cache, command_queue: Arc::new(DashMap::new()), event_bus }
    }

    /// 初始化驱动（提取为独立方法）
    fn initialize_drivers(
        cache: &DriverCache,
        context: &Arc<DataContext>,
        event_bus: &Arc<crate::infrastructure::event::EventBus>,
    ) {
        for device in context.get_all_devices() {
            if let Some(driver_name) = &device.driver_name {
                match create_driver(driver_name, &device, context.clone()) {
                    Ok(mut driver) => {
                        driver.set_event_bus(event_bus.clone());

                        cache.insert(device.id.clone(), Arc::new(RwLock::new(driver)));
                        tracing::info!("Loaded driver for device: {}", device.get_display_name());
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to create driver for device '{}': {}",
                            device.get_display_name(),
                            e
                        );
                    }
                }
            }
        }
    }

    /// 核心数据处理循环（简化版）
    pub async fn run(
        &self,
        shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    ) -> Result<(), Error> {
        // 按协议类型分组启动处理任务
        let driver_groups = self.group_drivers_by_protocol();

        for (protocol, _) in driver_groups {
            let driver_cache = self.driver_cache.clone();
            let context = self.context.clone();
            let command_queue = self.command_queue.clone();
            let mut shutdown_rx_clone = shutdown_rx.resubscribe();

            tokio::spawn(async move {
                tokio::select! {
                    _ = Self::process_protocol_drivers(protocol.clone(), driver_cache, context, command_queue) => {
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

    /// 处理特定协议的驱动（修复版 - 动态获取驱动列表）
    async fn process_protocol_drivers(
        protocol: String,
        driver_cache: DriverCache,
        context: Arc<DataContext>,
        command_queue: CommandQueue,
    ) {
        tracing::info!("Starting data server loop for protocol: {}", protocol);

        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(100));

        loop {
            interval.tick().await;

            // 动态获取当前协议的所有驱动
            let mut drivers = Vec::new();
            for (_, driver_arc) in driver_cache.iter() {
                if let Some(driver) = driver_arc.try_read() {
                    if driver.device().driver_name.as_deref() == Some(&protocol) {
                        drivers.push(driver_arc.clone());
                    }
                }
            }

            // 获取待执行命令
            let commands =
                command_queue.remove(&protocol).map(|(_, cmds)| cmds).unwrap_or_default();

            // 处理每个驱动
            for driver_arc in &drivers {
                // 先检查是否可以读取（避免在重试间隔内获取写锁）
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

                    // 读取数据（非阻塞版，由 tick 间隔替代 sleep）
                    let read_result = driver.read_data_once();

                    // 克隆设备信息，避免借用冲突
                    let mut device = driver.device().clone();

                    // 记录设备之前的在线状态（在克隆后立即获取）
                    let was_online = device.is_online;
                    let device_display_name =
                        device.display_name.clone().unwrap_or_else(|| device.name.clone());
                    let device_address = device.address.clone();

                    match read_result.result {
                        Ok(values) => {
                            // 检查状态是否改变：离线 → 在线
                            if !was_online {
                                // 状态改变：设备从离线变为在线
                                driver.on_connected(device_address);
                                tracing::info!(
                                    "Device '{}' state changed: offline → online",
                                    device_display_name
                                );
                            }

                            // 更新设备状态
                            device.is_online = true;
                            device.state = Some(1);
                            device.last_heartbeat =
                                Some(chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string());

                            // 更新属性值并发布变化事件
                            Self::update_device_properties_with_events(
                                &mut device,
                                &values,
                                &mut driver,
                            );

                            tracing::debug!(
                                "Device {} data read successful, {} values, took {:?}",
                                device_display_name,
                                values.len(),
                                read_result.elapsed
                            );
                        }
                        Err(e) => {
                            // 检查是否是最终失败（重试已完成）
                            if let Some(retry_info) = &read_result.retry_info {
                                if retry_info.will_retry {
                                    // 还在重试中，不改变状态，只记录日志
                                    tracing::debug!(
                                        "Device {} read failed (attempt {}), will retry in {:?}: {}",
                                        device_display_name,
                                        retry_info.attempt,
                                        retry_info.next_retry_in.unwrap_or_default(),
                                        e
                                    );
                                    // 不更新设备状态，继续处理下一个驱动
                                    continue;
                                } else {
                                    // 重试已完成，检查状态是否改变：在线 → 离线
                                    if was_online {
                                        // 状态改变：设备从在线变为离线
                                        driver.on_disconnected(Some(e.to_string()));
                                        tracing::warn!(
                                            "Device '{}' state changed: online → offline after {} attempts",
                                            device_display_name,
                                            retry_info.attempt
                                        );
                                    } else {
                                        // 设备本来就离线，只记录日志
                                        tracing::debug!(
                                            "Device {} read failed after {} attempts (already offline): {}",
                                            device_display_name,
                                            retry_info.attempt,
                                            e
                                        );
                                    }

                                    // 更新设备状态为离线
                                    device.is_online = false;
                                    device.state = Some(0);
                                }
                            } else {
                                // 没有重试信息，直接处理错误
                                if was_online {
                                    driver.on_disconnected(Some(e.to_string()));
                                }
                                device.is_online = false;
                                device.state = Some(0);
                                tracing::warn!("Device {} read error: {}", device_display_name, e);
                            }
                        }
                    }

                    // 执行命令并发布事件
                    for cmd in commands.iter().filter(|c| c.device_id == device_id) {
                        let cmd_result = driver.execute_command(cmd);
                        let execution_time_ms = cmd_result.elapsed.as_millis() as u64;

                        match cmd_result.result {
                            Ok(_) => {
                                // 发布命令执行成功事件
                                driver.publish_command_execution(
                                    cmd.name.clone(),
                                    true,
                                    execution_time_ms,
                                    None,
                                );

                                tracing::info!(
                                    "Device {} command '{}' executed successfully in {:?}",
                                    device_display_name,
                                    cmd.name,
                                    cmd_result.elapsed
                                );
                            }
                            Err(e) => {
                                // 发布命令执行失败事件
                                driver.publish_command_execution(
                                    cmd.name.clone(),
                                    false,
                                    execution_time_ms,
                                    Some(e.to_string()),
                                );

                                tracing::error!(
                                    "Device {} command '{}' failed in {:?}: {}",
                                    device_display_name,
                                    cmd.name,
                                    cmd_result.elapsed,
                                    e
                                );
                            }
                        }
                    }

                    // 更新驱动和上下文
                    *driver.device_mut() = device.clone();
                    context.update_device_value(device);
                }
            }
        }
    }

    /// 更新设备属性值并发布变化事件
    fn update_device_properties_with_events(
        device: &mut Device,
        values: &[crate::domain::device::driver::ResultValue],
        driver: &mut DriverWrapper,
    ) {
        if let Some(ref mut properties) = device.properties {
            for property in properties.iter_mut() {
                if let Some(result_value) = values.iter().find(|v| v.name == property.name) {
                    if let Some(ref value_str) = result_value.value {
                        // 记录旧值（克隆以避免借用问题）
                        let old_value = property.current_value.clone();

                        // 检查值是否真的改变了
                        let value_changed = match &old_value {
                            Some(old_val) => old_val != value_str,
                            None => true, // 如果之前没有值，认为是改变
                        };

                        // 更新属性值
                        property.set_current_value(value_str.clone());

                        // 只有值真正改变时才发布事件
                        if value_changed {
                            // 克隆旧值用于日志
                            let old_value_for_log = old_value.clone();

                            driver.publish_property_change(
                                property.id.clone(),
                                property.name.clone(),
                                old_value,
                                value_str.clone(),
                            );

                            tracing::debug!(
                                "Property '{}' changed on device '{}': {:?} → {}",
                                property.name,
                                device.name,
                                old_value_for_log.as_deref().unwrap_or("None"),
                                value_str
                            );
                        }
                    }
                }
            }
        }
    }

    /// 按协议分组驱动
    fn group_drivers_by_protocol(&self) -> HashMap<String, Vec<Arc<RwLock<DriverWrapper>>>> {
        let mut groups = HashMap::new();

        for (_, driver_arc) in self.driver_cache.iter() {
            if let Some(driver) = driver_arc.try_read() {
                let protocol =
                    driver.device().driver_name.clone().unwrap_or_else(|| "unknown".to_string());

                groups.entry(protocol).or_insert_with(Vec::new).push(driver_arc.clone());
            }
        }

        groups
    }
    // === 简化的公共API ===

    /// 获取设备列表（从内存缓存）
    pub fn get_devices(&self) -> Vec<Device> {
        self.context.get_all_devices()
    }

    /// 获取设备（从内存缓存）
    pub fn get_device(&self, id: &str) -> Option<Device> {
        self.context.get_device(id)
    }

    /// 根据名称获取设备（从内存缓存）
    pub fn get_device_by_name(&self, name: &str) -> Option<Device> {
        self.context.get_device_by_name(name)
    }

    /// 执行命令
    pub fn execute_command(&self, cmd: DeviceCommand) -> Result<(), Error> {
        if let Some(device) = self.context.get_device(&cmd.device_id) {
            let protocol = device.driver_name.unwrap_or_else(|| "unknown".to_string());

            // 将命令加入队列
            self.command_queue.entry(protocol).or_default().push(cmd);
            Ok(())
        } else {
            Err(Error::NotFound)
        }
    }

    /// 批量执行命令
    pub fn execute_commands(&self, cmds: Vec<DeviceCommand>) -> Result<(), Error> {
        for cmd in cmds {
            self.execute_command(cmd)?;
        }
        Ok(())
    }

    /// 重新加载设备（从数据库加载最新数据并更新context和驱动）
    pub async fn reload_device(&self, ids: Vec<String>) -> Result<(), Error> {
        // 使用DeviceService加载完整设备信息
        let db = self.context.database();
        let device_repository: Arc<dyn crate::domain::device::repository::DeviceRepository> =
            Arc::new(crate::infrastructure::persistence::repositories::SqliteDeviceRepository::new(db.clone()));
        let device_service = crate::domain::device::service::DeviceService::new(device_repository, Arc::new(db));

        let devices = device_service.load_complete_devices(&ids).await?;

        for device in devices {
            let device_id = device.id.clone();

            // 更新到上下文
            self.context.set_device(device.clone());

            // 重新创建驱动
            if let Some(driver_name) = &device.driver_name {
                match create_driver(driver_name, &device, self.context.clone()) {
                    Ok(mut driver) => {
                        // 设置事件总线
                        driver.set_event_bus(self.event_bus.clone());

                        self.driver_cache.insert(device_id.clone(), Arc::new(RwLock::new(driver)));
                        tracing::info!("Reloaded driver for device: {}", device.get_display_name());
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to reload driver for device '{}': {}",
                            device.get_display_name(),
                            e
                        );
                    }
                }
            }
        }

        Ok(())
    }

    /// 删除设备（从context和驱动缓存中移除）
    pub fn remove_device(&self, device_id: &str) {
        // 从上下文移除
        self.context.remove_device(device_id);
        // 从驱动缓存移除
        self.driver_cache.invalidate(device_id);
        tracing::info!("Removed device and driver: {}", device_id);
    }

    /// 批量删除设备
    pub fn remove_devices(&self, device_ids: &[String]) {
        for device_id in device_ids {
            self.remove_device(device_id);
        }
    }

    // === 驱动管理和统计 ===

    /// 获取驱动统计
    pub fn get_device_overview(&self, device_id: &str) -> Option<DeviceOverview> {
        self.driver_cache
            .get(device_id)
            .and_then(|driver_arc| driver_arc.try_read().map(|driver| driver.overview()))
    }

    /// 获取所有设备统计
    pub fn get_all_device_overview(&self) -> Vec<DeviceOverview> {
        self.driver_cache
            .iter()
            .filter_map(|(_, driver_arc)| driver_arc.try_read().map(|driver| driver.overview()))
            .collect()
    }

    /// 获取在线设备数量
    pub fn get_online_count(&self) -> usize {
        self.driver_cache
            .iter()
            .filter_map(|(_, driver_arc)| driver_arc.try_read().map(|driver| driver.is_online()))
            .filter(|&online| online)
            .count()
    }

    /// 获取健康设备数量
    pub fn get_healthy_count(&self) -> usize {
        self.driver_cache
            .iter()
            .filter_map(|(_, driver_arc)| driver_arc.try_read().map(|driver| driver.is_healthy()))
            .filter(|&healthy| healthy)
            .count()
    }

    /// 重置设备驱动状态
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

    /// 强制设备离线
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

    /// 获取驱动缓存大小
    pub fn get_driver_cache_size(&self) -> u64 {
        self.driver_cache.entry_count()
    }

    /// 清理驱动缓存
    pub fn cleanup_driver_cache(&self) {
        // 移除无效的驱动
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

// === 实现 EventHandler trait ===

#[async_trait::async_trait]
impl crate::infrastructure::event::EventHandler for DataServer {
    async fn handle(
        &self,
        event: &crate::domain::event::entities::Event,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use crate::domain::event::value_objects::DeviceEventType;

        // 提取设备ID
        let device_id = match event.source().device_id() {
            Some(id) => id,
            None => return Ok(()),
        };

        match event.event_type() {
            crate::domain::event::value_objects::EventType::Device(device_event_type) => {
                match device_event_type {
                    DeviceEventType::DeviceCreated | DeviceEventType::DeviceUpdated => {
                        tracing::info!(
                            "Handling {:?} event for device: {}",
                            device_event_type,
                            device_id
                        );
                        if let Err(e) = self.reload_device(vec![device_id.to_string()]).await {
                            tracing::error!("Failed to reload device {}: {}", device_id, e);
                        }
                    }
                    DeviceEventType::DeviceDeleted => {
                        tracing::info!("Handling DeviceDeleted event for device: {}", device_id);
                        self.remove_device(device_id);
                    }
                    _ => {}
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn name(&self) -> &str {
        "DataServer"
    }

    fn should_handle(&self, event: &crate::domain::event::entities::Event) -> bool {
        matches!(event.event_type(), crate::domain::event::value_objects::EventType::Device(_))
    }

    fn priority(&self) -> u8 {
        10
    }
}

impl Clone for DataServer {
    fn clone(&self) -> Self {
        Self {
            context: self.context.clone(),
            driver_cache: self.driver_cache.clone(),
            command_queue: self.command_queue.clone(),
            event_bus: self.event_bus.clone(),
        }
    }
}
