use std::{path::PathBuf, sync::Arc};

use dashmap::DashMap;
use sqlx::SqlitePool;

use crate::{
    domain::template::{
        engine::TemplateEngine, repository::TemplateRepository, service::TemplateService,
        validator::TemplateValidator,
    },
    dto::entity::Device,
    infrastructure::{
        event::EventBus,
        persistence::{create_pool, Database, DatabaseConfig},
    },
    shared::error::Error,
};

/// 重构后的数据上下文 - 专注于内存缓存管理
#[derive(Debug)]
pub struct DataContext {
    // 核心缓存
    devices: DashMap<String, Arc<Device>>, // 设备基本信息缓存
    name_to_id: DashMap<String, String>,   // 设备名称到ID的映射

    // 基础设施
    pub db_pool: SqlitePool, // 数据库连接池

    // 模板系统
    template_engine: Arc<TemplateEngine>,   // 模板引擎
    template_service: Arc<TemplateService>, // 模板服务
}

impl DataContext {
    /// 创建新的数据上下文
    pub async fn new(
        db_config: DatabaseConfig,
    ) -> Result<Arc<Self>, Box<dyn std::error::Error + Send + Sync>> {
        let db_pool = create_pool(&db_config).await?;
        Self::new_with_pool(db_pool).await
    }

    /// 使用现有数据库连接池创建数据上下文
    pub async fn new_with_pool(
        db_pool: SqlitePool,
    ) -> Result<Arc<Self>, Box<dyn std::error::Error + Send + Sync>> {
        // 初始化模板系统
        let template_system = Self::init_template_system(&db_pool).await?;

        let context = Arc::new(Self {
            devices: DashMap::new(),
            name_to_id: DashMap::new(),
            db_pool,
            template_engine: template_system.0,
            template_service: template_system.1,
        });

        // 初始化设备缓存
        if let Err(e) = context.init_device_cache().await {
            tracing::error!("Failed to initialize device cache: {}", e);
        }

        Ok(context)
    }

    /// 创建用于测试的模拟 DataContext
    #[cfg(test)]
    pub async fn new_mock() -> Self {
        let db_pool = sqlx::SqlitePool::connect(":memory:")
            .await
            .expect("Failed to create in-memory database for testing");

        let template_system = Self::init_template_system(&db_pool)
            .await
            .expect("Failed to initialize template system for testing");

        Self {
            devices: DashMap::new(),
            name_to_id: DashMap::new(),
            db_pool,
            template_engine: template_system.0,
            template_service: template_system.1,
        }
    }

    /// 初始化模板系统
    async fn init_template_system(
        db_pool: &SqlitePool,
    ) -> Result<(Arc<TemplateEngine>, Arc<TemplateService>), Box<dyn std::error::Error + Send + Sync>>
    {
        let database = Database::new(db_pool.clone());
        let template_repository =
            Arc::new(TemplateRepository::new(Arc::new(database), PathBuf::from("templates")));
        let template_validator = Arc::new(TemplateValidator::new());
        let template_engine =
            Arc::new(TemplateEngine::new(template_repository.clone(), template_validator));
        let template_service = Arc::new(TemplateService::new(template_repository));

        // 初始化模板系统
        if let Err(e) = template_service.initialize().await {
            tracing::warn!("Failed to initialize template system: {}", e);
        }

        Ok((template_engine, template_service))
    }

    /// 初始化设备缓存
    async fn init_device_cache(&self) -> Result<(), Error> {
        // 使用 DeviceService 加载设备
        let device_service =
            crate::domain::device::service::DeviceService::new(Arc::new(self.database()));

        match device_service.get_devices(&Default::default()).await {
            Ok(devices) => {
                tracing::info!("Loading {} devices into cache", devices.len());

                for device in devices {
                    // 加载完整设备信息（包含属性和命令）
                    match device_service.load_complete_device(&device.id).await {
                        Ok(Some(full_device)) => {
                            self.set_device(full_device);
                        }
                        Ok(None) => {
                            tracing::warn!(
                                "Device {} not found when loading complete info",
                                device.id
                            );
                        }
                        Err(e) => {
                            tracing::error!("Failed to load complete device {}: {}", device.id, e);
                            // 至少缓存基本设备信息
                            self.set_device(device);
                        }
                    }
                }

                tracing::info!("Device cache initialized successfully");
                Ok(())
            }
            Err(e) => {
                tracing::error!("Failed to load devices: {}", e);
                Err(e)
            }
        }
    }

    // === 数据库访问 ===

    /// 获取数据库实例
    pub fn database(&self) -> Database {
        Database::new(self.db_pool.clone())
    }

    // === 模板系统访问 ===

    /// 获取模板引擎引用
    pub fn template_engine(&self) -> &TemplateEngine {
        &self.template_engine
    }

    /// 获取模板服务引用
    pub fn template_service(&self) -> &TemplateService {
        &self.template_service
    }

    // === 设备缓存管理 ===

    /// 获取设备（从缓存）
    pub fn get_device(&self, id: &str) -> Option<Device> {
        self.devices.get(id).map(|device| Device::clone(&device))
    }

    /// 根据名称获取设备（从缓存）
    pub fn get_device_by_name(&self, name: &str) -> Option<Device> {
        self.name_to_id.get(name).and_then(|id| self.get_device(&id))
    }

    /// 根据设备名称和属性名称获取属性
    pub fn get_device_prop_by_name(
        &self,
        dev_name: &str,
        prop_name: &str,
    ) -> Option<crate::dto::entity::DeviceProperty> {
        if let Some(device) = self.get_device_by_name(dev_name) {
            if let Some(properties) = &device.properties {
                return properties.iter().find(|prop| prop.name == prop_name).cloned();
            }
        }

        tracing::debug!("Property '{}' not found for device '{}'", prop_name, dev_name);
        None
    }

    /// 设置设备到缓存
    pub fn set_device(&self, device: Device) {
        let id = device.id.clone();
        let name = device.name.clone();

        self.devices.insert(id.clone(), Arc::new(device));
        self.name_to_id.insert(name, id);
    }

    /// 更新设备值（保持引用）
    pub fn update_device_value(&self, device: Device) {
        let id = device.id.clone();

        if let Some(mut cached_device) = self.devices.get_mut(&id) {
            *cached_device.value_mut() = Arc::new(device);
        } else {
            // 如果设备不存在，直接添加
            self.set_device(device);
        }
    }

    /// 批量更新设备值
    pub fn update_devices_value(&self, devices: Vec<Device>) {
        for device in devices {
            self.update_device_value(device);
        }
    }

    /// 从缓存中移除设备
    pub fn remove_device(&self, id: &str) {
        if let Some((_, device)) = self.devices.remove(id) {
            let name = device.name.clone();
            if !name.is_empty() {
                self.name_to_id.remove(&name);
            }
        }
    }

    /// 获取所有设备（从缓存）
    pub fn get_all_devices(&self) -> Vec<Device> {
        self.devices.iter().map(|entry| Device::clone(entry.value())).collect()
    }

    /// 尝试获取所有设备（带错误处理）
    pub fn try_get_all_devices(&self) -> Result<Vec<Device>, Error> {
        Ok(self.get_all_devices())
    }

    /// 更新设备属性（使用闭包）
    pub fn update_device_property(
        &self,
        device_id: &str,
        _property_id: &str,
        update_fn: impl FnOnce(&mut Device),
    ) {
        if let Some(mut device_ref) = self.devices.get_mut(device_id) {
            let mut device = Device::clone(&device_ref);
            update_fn(&mut device);
            *device_ref.value_mut() = Arc::new(device);
        }
    }

    /// 更新设备属性值
    pub async fn update_device_property_value(
        &self,
        device_id: &str,
        property_id: &str,
        value: &str,
        event_bus: Option<&Arc<EventBus>>,
    ) -> Result<(), Error> {
        // 验证设备是否存在
        if self.get_device(device_id).is_none() {
            return Err(Error::NotFound);
        }

        let db = self.database();

        // 验证属性是否存在并获取旧值
        let property = crate::dto::entity::DeviceProperty::find_by_id(&db, property_id)
            .await
            .map_err(|e| Error::IOError(format!("Failed to find property: {}", e)))?
            .ok_or_else(|| Error::IOError("Property not found".to_string()))?;

        // 验证属性是否属于该设备
        if property.device_id != device_id {
            return Err(Error::IOError("Property does not belong to device".to_string()));
        }

        let old_value = property.current_value.clone();

        // 直接更新数据库中的属性值
        let update_query = "UPDATE device_properties SET current_value = ?, updated_at = datetime('now') WHERE id = ?";
        sqlx::query(update_query)
            .bind(value)
            .bind(property_id)
            .execute(db.pool())
            .await
            .map_err(|e| Error::IOError(format!("Failed to update property value: {}", e)))?;

        // 更新缓存中的设备属性
        self.update_device_property(device_id, property_id, |device| {
            if let Some(properties) = &mut device.properties {
                for prop in properties.iter_mut() {
                    if prop.id == property_id {
                        prop.set_current_value(value.to_string());
                        break;
                    }
                }
            }
        });

        // 发布属性变化事件
        if let Some(event_bus) = event_bus {
            use crate::domain::event::{
                entities::Event as DomainEvent,
                value_objects::{
                    ContentElement, DeviceEventType, EventLevel, EventSource, RichContent,
                    TextFormat,
                },
            };

            let mut elements = vec![ContentElement::Text {
                content: format!("Property '{}' value changed", property.name),
                format: TextFormat::Plain,
            }];

            if let Some(ref old_val) = old_value {
                elements.push(ContentElement::Text {
                    content: format!("Previous value: {}", old_val),
                    format: TextFormat::Plain,
                });
            }

            elements.push(ContentElement::Text {
                content: format!("Current value: {}", value),
                format: TextFormat::Plain,
            });

            let event = DomainEvent::new_device_event(
                DeviceEventType::PropertyChange,
                EventLevel::Debug,
                EventSource::device_property(
                    device_id.to_string(),
                    property_id.to_string(),
                    "context".to_string(),
                ),
                RichContent::new(format!("Property Changed: {}", property.name), elements),
            );

            if let Ok(event) = event {
                let event_bus_clone = event_bus.clone();
                crate::utils::publish_event_safe(event_bus_clone, event).await;
            }
        }

        tracing::info!(
            "Updated property value: device={}, property={}, value={}",
            device_id,
            property_id,
            value
        );

        Ok(())
    }

    /// 获取事件总线（已移除，由 AppState 统一管理）
    #[deprecated(note = "Use AppState.event_bus() instead")]
    pub fn event_bus(&self) -> Option<Arc<EventBus>> {
        None
    }

    /// 清空所有缓存
    pub fn clear(&self) {
        self.devices.clear();
        self.name_to_id.clear();
    }

    /// 获取缓存统计信息
    pub fn get_cache_stats(&self) -> CacheStats {
        CacheStats { device_count: self.devices.len(), name_mapping_count: self.name_to_id.len() }
    }

    /// 刷新设备缓存（从数据库重新加载）
    pub async fn refresh_device_cache(&self) -> Result<(), Error> {
        tracing::info!("Refreshing device cache from database");

        // 清空现有缓存
        self.clear();

        // 重新初始化缓存
        self.init_device_cache().await?;

        tracing::info!("Device cache refreshed successfully");
        Ok(())
    }

    /// 刷新单个设备缓存
    pub async fn refresh_device(&self, device_id: &str) -> Result<(), Error> {
        let device_service =
            crate::domain::device::service::DeviceService::new(Arc::new(self.database()));

        match device_service.load_complete_device(device_id).await? {
            Some(device) => {
                self.set_device(device);
                tracing::debug!("Refreshed device {} in cache", device_id);
                Ok(())
            }
            None => {
                // 设备不存在，从缓存中移除
                self.remove_device(device_id);
                tracing::debug!("Removed non-existent device {} from cache", device_id);
                Err(Error::NotFound)
            }
        }
    }

    /// 重新加载设备（别名方法，用于API层）
    pub async fn reload_device(&self, device_id: &str) -> Result<(), Error> {
        self.refresh_device(device_id).await
    }
}

/// 缓存统计信息
#[derive(Debug, Clone, serde::Serialize)]
pub struct CacheStats {
    pub device_count: usize,
    pub name_mapping_count: usize,
}
