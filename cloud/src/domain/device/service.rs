use tinyiothub_core::models::device::{CreateDeviceRequest, Device, DeviceQueryParams, UpdateDeviceRequest};
use tinyiothub_core::models::device_command::DeviceCommand;
use tinyiothub_core::models::device_property::DeviceProperty;
use crate::infrastructure::persistence::repositories::{
    bulk_create_device_commands, create_device_command, find_device_commands_by_device_id,
    create_device_properties_batch, find_device_properties_by_device_id,
};
use tinyiothub_core::models::device_command::CreateDeviceCommandRequest;
use std::sync::Arc;

// Error message constants for internationalization
const ERROR_DEVICE_NAME_EXISTS: &str = "Device name already exists";
const ERROR_TEMPLATE_APPLICATION_FAILED: &str = "Template application failed";
const ERROR_DEVICE_DRIVER_NOT_CONFIGURED: &str = "Device driver not configured";
const ERROR_DEVICE_ADDRESS_NOT_CONFIGURED: &str = "Device address not configured";

// Event message constants for consistency
const MSG_DEVICE_TYPE_VALUE_NA: &str = "N/A";

use crate::{
    domain::{
        device::repository::{DeviceCriteria, DeviceRepository},
        event::{
            entities::Event as DomainEvent,
            value_objects::{
                ContentElement, DeviceEventType, EventLevel, EventSource, RichContent, TextFormat,
            },
        },
        tag::repository::TagRepository,
    },
    dto::{
        request::pagination::DataObjectWithPagination
    },
    infrastructure::{event::EventBus, persistence::Database},
    shared::error::Error
};

/// Device service
/// Handles device business logic and event publishing
pub struct DeviceService {
    repository: Arc<dyn DeviceRepository>,
    /// Temporarily retained: for Property/Command/Tag calls not yet migrated to Repository
    /// TODO: Phase 3 完成后移除
    database: Arc<Database>,
    event_bus: Option<Arc<EventBus>>,
    tag_repository: Option<Arc<dyn TagRepository>>,
}

impl DeviceService {
    pub fn new(repository: Arc<dyn DeviceRepository>, database: Arc<Database>) -> Self {
        Self { repository, database, event_bus: None, tag_repository: None }
    }

    /// Create device service with event bus
    pub fn with_event_bus(repository: Arc<dyn DeviceRepository>, database: Arc<Database>, event_bus: Arc<EventBus>) -> Self {
        Self { repository, database, event_bus: Some(event_bus), tag_repository: None }
    }

    /// Set tag repository for loading device tags
    pub fn with_tag_repository(mut self, tag_repository: Arc<dyn TagRepository>) -> Self {
        self.tag_repository = Some(tag_repository);
        self
    }

    /// Create device
    pub async fn create_device(&self, request: &CreateDeviceRequest) -> Result<Device, Error> {
        tracing::info!("Creating device: {}", request.name);

        // Validate device name uniqueness
        if self.repository.exists_by_name(&request.name).await.unwrap_or(false) {
            return Err(Error::ValidationError(ERROR_DEVICE_NAME_EXISTS.to_string()));
        }

        // Create device
        let created_device = self.repository.create(request).await?;

        // TODO: Load tags (current repository does not handle tags, needs to be extended at repository layer or manually loaded here)
        // publish_device_created_event does not need tags

        // Publish device creation event
        self.publish_device_created_event(&created_device).await;

        tracing::info!("Device {} created successfully", created_device.id);
        Ok(created_device)
    }

    /// Create device from template
    pub async fn create_device_from_template(
        &self,
        template_engine: &crate::domain::template::engine::TemplateEngine,
        template_id: &str,
        device_input: &crate::dto::entity::device_template::DeviceCreationInput,
    ) -> Result<Device, Error> {
        tracing::info!(
            "Creating device from template: template_id={}, device_name={}",
            template_id,
            device_input.name
        );

        // Apply template and create device
        let created_device = self
            .apply_template_and_create_device(template_engine, template_id, device_input)
            .await?;

        // Get template for property and command generation
        let template = self.get_template(template_engine, template_id).await?;

        // Generate and create device properties
        self.generate_and_create_properties(
            template_engine,
            &template,
            device_input,
            &created_device.id,
        )
        .await;

        // Generate and create device commands
        self.generate_and_create_commands(
            template_engine,
            &template,
            device_input,
            &created_device.id,
        )
        .await;

        // Publish device creation event
        self.publish_device_created_event(&created_device).await;

        tracing::info!(
            "Device created successfully from template: device_id={}",
            created_device.id
        );
        Ok(created_device)
    }

    /// Build common device info content elements
    fn build_device_info_elements(&self, device: &Device, title: String) -> Vec<ContentElement> {
        vec![
            self.create_text_element(title),
            self.create_text_element(format!("Device ID: {}", device.id)),
            self.create_text_element(format!(
                "Device Type: {}",
                device.device_type.as_deref().unwrap_or(MSG_DEVICE_TYPE_VALUE_NA)
            )),
        ]
    }

    /// Create a text content element
    fn create_text_element(&self, content: String) -> ContentElement {
        ContentElement::Text {
            content,
            format: TextFormat::Plain,
        }
    }

    /// Convert repository error to IOError
    fn io_error(&self, e: impl std::fmt::Display) -> Error {
        Error::IOError(e.to_string())
    }

    /// Publish device creation event (extracted as separate method)
    async fn publish_device_created_event(&self, device: &Device) {
        let content_elements = self.build_device_info_elements(
            device,
            format!("Device '{}' has been created successfully", device.name),
        );

        // Ignore errors for backward compatibility
        let _ = self.publish_device_event(
            DeviceEventType::DeviceCreated,
            EventLevel::Info,
            device,
            format!("Device Created: {}", device.name),
            content_elements,
        ).await;
    }

    /// Publish device event with common format
    async fn publish_device_event(
        &self,
        event_type: DeviceEventType,
        level: EventLevel,
        device: &Device,
        title: String,
        content_elements: Vec<ContentElement>,
    ) -> Result<(), Error> {
        if let Some(ref event_bus) = self.event_bus {
            let event = DomainEvent::new_device_event(
                event_type,
                level,
                EventSource::device(device.id.clone(), Some("device_service".to_string())),
                RichContent::new(title, content_elements),
            )
            .map_err(|e| Error::IOError(format!("Failed to create event: {}", e)))?;

            let event_bus_clone = event_bus.clone();
            crate::shared::utils::publish_event_safe(event_bus_clone, event).await;
        }
        Ok(())
    }

    /// Get template by ID
    async fn get_template(
        &self,
        template_engine: &crate::domain::template::engine::TemplateEngine,
        template_id: &str,
    ) -> Result<crate::dto::entity::device_template::DeviceTemplate, Error> {
        template_engine
            .get_repository()
            .find_by_id(template_id)
            .await
            .map_err(|e| Error::IOError(format!("Failed to get template: {}", e)))?
            .ok_or(Error::NotFound)
    }

    /// Apply template and create device
    async fn apply_template_and_create_device(
        &self,
        template_engine: &crate::domain::template::engine::TemplateEngine,
        template_id: &str,
        device_input: &crate::dto::entity::device_template::DeviceCreationInput,
    ) -> Result<tinyiothub_core::models::device::Device, Error> {
        // Validate device name uniqueness
        if self.repository.exists_by_name(&device_input.name).await.unwrap_or(false) {
            return Err(Error::ValidationError(ERROR_DEVICE_NAME_EXISTS.to_string()));
        }

        // Apply template using template engine
        let device_request = template_engine
            .apply_template(template_id, device_input)
            .await
            .map_err(|e| Error::ValidationError(format!("{}: {}", ERROR_TEMPLATE_APPLICATION_FAILED, e)))?;

        // Create device
        self.repository.create(&device_request).await
    }

    /// Generate and create device properties
    async fn generate_and_create_properties(
        &self,
        template_engine: &crate::domain::template::engine::TemplateEngine,
        template: &crate::dto::entity::device_template::DeviceTemplate,
        device_input: &crate::dto::entity::device_template::DeviceCreationInput,
        device_id: &str,
    ) {
        match template_engine
            .generate_device_properties(template, device_input, device_id)
            .await
        {
            Ok(properties) => {
                if !properties.is_empty() {
                    // TODO: Bulk creation of DeviceProperty still directly depends on Database, should be extracted to repository later
                    let db = self.database.clone();
                    if let Err(e) = create_device_properties_batch(&db, &properties).await {
                        tracing::warn!("{}", e);
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Failed to generate device properties: {}", e);
            }
        }
    }

    /// Generate and create device commands
    async fn generate_and_create_commands(
        &self,
        template_engine: &crate::domain::template::engine::TemplateEngine,
        template: &crate::dto::entity::device_template::DeviceTemplate,
        device_input: &crate::dto::entity::device_template::DeviceCreationInput,
        device_id: &str,
    ) {
        match template_engine
            .generate_device_commands(template, device_input, device_id)
            .await
        {
            Ok(commands) => {
                if !commands.is_empty() {
                    // TODO: Same as above, temporarily using Database directly
                    let db = self.database.clone();
                    if let Err(e) = bulk_create_device_commands(&db, &commands).await {
                        tracing::warn!("Failed to create device commands: {}", e);
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Failed to generate device commands: {}", e);
            }
        }
    }

    /// Update device
    pub async fn update_device(
        &self,
        device_id: &str,
        request: &UpdateDeviceRequest,
    ) -> Result<Device, Error> {
        tracing::info!("Updating device: {}", device_id);

        // 获取旧设备信息
        let old_device =
            self.repository.find_by_id(device_id).await?.ok_or(Error::NotFound)?;

        // 更新设备
        let updated_device = self.repository.update(device_id, request).await?;

        // TODO: 标签处理暂由调用方负责，当前 repository 不处理标签

        // 发布设备更新事件
        self.publish_device_updated_event(&old_device, request, &updated_device).await;

        tracing::info!("Device {} updated successfully", device_id);
        Ok(updated_device)
    }

    /// Publish device update event with field change detection
    async fn publish_device_updated_event(
        &self,
        old_device: &Device,
        request: &UpdateDeviceRequest,
        updated_device: &Device,
    ) {
        let mut changes = Vec::new();

        // 检测变化的字段
        if let Some(ref new_name) = request.name
            && new_name != &old_device.name {
                changes.push(format!("Name: {} → {}", old_device.name, new_name));
            }
        if let Some(ref new_type) = request.device_type
            && Some(new_type) != old_device.device_type.as_ref() {
                changes.push(format!(
                    "Type: {} → {}",
                    old_device.device_type.as_deref().unwrap_or("N/A"),
                    new_type
                ));
            }
        if let Some(ref new_address) = request.address
            && Some(new_address) != old_device.address.as_ref() {
                changes.push(format!(
                    "Address: {} → {}",
                    old_device.address.as_deref().unwrap_or("N/A"),
                    new_address
                ));
            }

        if !changes.is_empty() {
            let mut elements = vec![self.create_text_element(format!("Device '{}' has been updated", updated_device.name))];

            for change in changes {
                elements.push(self.create_text_element(change));
            }

            // 使用通用的 publish_device_event 方法
            let _ = self.publish_device_event(
                DeviceEventType::DeviceUpdated,
                EventLevel::Info,
                updated_device,
                format!("Device Updated: {}", updated_device.name),
                elements,
            ).await;
        }
    }

    /// Delete device
    pub async fn delete_device(&self, device_id: &str) -> Result<bool, Error> {
        tracing::info!("Deleting device: {}", device_id);

        // 获取设备信息（用于事件）
        let device = self.repository.find_by_id(device_id).await?.ok_or(Error::NotFound)?;

        // 删除设备
        let deleted_count = self.repository.delete(device_id).await?;
        let success = deleted_count > 0;

        if success {
            // 发布设备删除事件
            self.publish_device_deleted_event(&device).await;

            tracing::info!("Device {} deleted successfully", device_id);
        }

        Ok(success)
    }

    /// Publish device deleted event
    async fn publish_device_deleted_event(&self, device: &Device) {
        let content_elements = self.build_device_info_elements(
            device,
            format!("Device '{}' has been deleted", device.name),
        );

        // 使用通用的 publish_device_event 方法
        let _ = self.publish_device_event(
            DeviceEventType::DeviceDeleted,
            EventLevel::Warning,
            device,
            format!("Device Deleted: {}", device.name),
            content_elements,
        ).await;
    }

    /// Publish command started event
    async fn publish_command_started_event(
        &self,
        device: &Device,
        command_name: &str,
        command_type: &str,
        command_id: &str,
    ) {
        let content_elements = vec![
            self.create_text_element(format!("Device: {}", device.name)),
            self.create_text_element(format!("Command ID: {}", command_id)),
        ];

        // 使用通用的 publish_device_event 方法
        let _ = self.publish_device_event(
            DeviceEventType::CommandStarted,
            EventLevel::Info,
            device,
            format!("Command: {} ({})", command_name, command_type),
            content_elements,
        ).await;
    }

    /// Update device state
    pub async fn update_device_state(&self, device_id: &str, new_state: i32) -> Result<(), Error> {
        // 获取当前状态
        let device = self.repository.find_by_id(device_id).await?.ok_or(Error::NotFound)?;

        let old_state = device.state.unwrap_or(0);

        if old_state != new_state {
            // 更新数据库中的状态
            self.repository.update_state(device_id, new_state).await?;

            // 获取更新后的设备信息
            if let Ok(Some(updated_device)) = self.repository.find_by_id(device_id).await {
                // 发布设备状态更新事件
                self.publish_device_state_updated_event(&updated_device, old_state, new_state).await;

                tracing::debug!(
                    "Device {} state updated from {} to {}",
                    device_id,
                    old_state,
                    new_state
                );
            }
        }

        Ok(())
    }

    /// Publish device state updated event
    async fn publish_device_state_updated_event(
        &self,
        device: &Device,
        old_state: i32,
        new_state: i32,
    ) {
        let content_elements = vec![
            self.create_text_element(format!(
                "Device '{}' state changed from {} to {}",
                device.name, old_state, new_state
            )),
            self.create_text_element(format!("Device ID: {}", device.id)),
        ];

        // 使用通用的 publish_device_event 方法
        let _ = self.publish_device_event(
            DeviceEventType::DeviceUpdated,
            EventLevel::Info,
            device,
            format!("Device State Updated: {}", device.name),
            content_elements,
        ).await;
    }

    // === 查询功能 ===

    /// Get device by ID
    pub async fn get_device_by_id(&self, device_id: &str) -> Result<Option<Device>, Error> {
        self.repository
            .find_by_id(device_id)
            .await
            .map_err(|e| self.io_error(e))
    }

    /// Get device by ID with tags
    pub async fn get_device_by_id_with_tags(
        &self,
        device_id: &str,
    ) -> Result<Option<Device>, Error> {
        // TODO: repository 当前不处理标签，先调用 find_by_id，后续扩展 repository 或在此手动加载标签
        self.repository
            .find_by_id(device_id)
            .await
            .map_err(|e| self.io_error(e))
    }

    /// Get device by name
    pub async fn get_device_by_name(&self, name: &str) -> Result<Option<Device>, Error> {
        self.repository.find_by_name(name).await.map_err(|e| self.io_error(e))
    }

    /// Query device list
    pub async fn get_devices(&self, params: &DeviceQueryParams) -> Result<Vec<Device>, Error> {
        let criteria = params_to_criteria(params);
        self.repository.find_all(&criteria).await.map_err(|e| self.io_error(e))
    }

    /// Query device list (including tags)
    pub async fn get_devices_with_tags(
        &self,
        params: &DeviceQueryParams,
        tenant_id: &str,
    ) -> Result<Vec<Device>, Error> {
        let mut devices = self.get_devices(params).await?;

        if let Some(tag_repo) = &self.tag_repository {
            for device in &mut devices {
                let tenant_id = tenant_id;
                match tag_repo.find_by_target_id(&device.id, tenant_id).await {
                    Ok(tags) => {
                        let tag_values: Vec<serde_json::Value> = tags
                            .into_iter()
                            .map(|t| serde_json::to_value(t).unwrap_or_default())
                            .collect();
                        device.tags = Some(tag_values);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to load tags for device {}: {}", device.id, e);
                        device.tags = Some(vec![]);
                    }
                }
            }
        }

        Ok(devices)
    }

    /// Count devices
    pub async fn count_devices(&self, params: &DeviceQueryParams) -> Result<i64, Error> {
        let criteria = params_to_criteria(params);
        self.repository.count(&criteria).await.map_err(|e| self.io_error(e))
    }

    /// Update device enabled status
    pub async fn update_device_enabled_status(&self, device_id: &str, enabled: bool) -> Result<bool, Error> {
        tracing::info!("Updating device enabled status: device_id={}, enabled={}", device_id, enabled);
        self.repository.update_enabled_status(device_id, enabled).await.map_err(|e| self.io_error(e))
    }

    /// 分页查询设备
    pub async fn get_devices_page(
        &self,
        params: &DeviceQueryParams,
        page_no: u32,
        page_size: u32,
    ) -> Result<DataObjectWithPagination<Device>, Error> {
        let mut query_params = params.clone();
        query_params.page = Some(page_no);
        query_params.page_size = Some(page_size);

        let devices = self.get_devices(&query_params).await?;
        Ok(DataObjectWithPagination::new(&devices, page_no, page_size))
    }

    /// Get child devices by parent ID
    pub async fn get_child_devices(&self, parent_id: &str) -> Result<Vec<Device>, Error> {
        self.repository
            .find_children(parent_id)
            .await
            .map_err(|e| self.io_error(e))
    }

    /// Get devices by product ID
    pub async fn get_devices_by_product(&self, product_id: &str) -> Result<Vec<Device>, Error> {
        self.repository
            .find_by_product_id(product_id)
            .await
            .map_err(|e| self.io_error(e))
    }

    /// Get devices by driver name
    pub async fn get_devices_by_driver(&self, driver_name: &str) -> Result<Vec<Device>, Error> {
        self.repository
            .find_by_driver_name(driver_name)
            .await
            .map_err(|e| self.io_error(e))
    }

    // === 设备属性查询 ===

    /// 获取设备属性列表
    pub async fn get_device_properties(
        &self,
        device_id: &str,
    ) -> Result<Vec<DeviceProperty>, Error> {
        // TODO: DeviceProperty 尚未提取到 repository，暂时仍直接调用 Database
        let db = self.database.clone();
        find_device_properties_by_device_id(&db, device_id)
            .await
            .map_err(|e| self.io_error(e))
    }

    /// Get device property by name
    pub async fn get_device_property_by_name(
        &self,
        device_id: &str,
        property_name: &str,
    ) -> Result<Option<DeviceProperty>, Error> {
        let properties = self.get_device_properties(device_id).await?;
        Ok(properties.into_iter().find(|p| p.name == property_name))
    }

    /// 分页查询设备属性
    pub async fn get_device_properties_page(
        &self,
        device_id: &str,
        property_name: Option<String>,
        page_no: u32,
        page_size: u32,
    ) -> Result<DataObjectWithPagination<DeviceProperty>, Error> {
        let mut properties = self.get_device_properties(device_id).await?;

        // 如果指定了属性名称，进行过滤
        if let Some(name) = property_name {
            properties.retain(|p| p.name.contains(&name));
        }

        Ok(DataObjectWithPagination::new(&properties, page_no, page_size))
    }

    // === 设备命令查询 ===

    /// 获取设备命令列表
    pub async fn get_device_commands(&self, device_id: &str) -> Result<Vec<DeviceCommand>, Error> {
        // TODO: DeviceCommand 尚未提取到 repository，暂时仍直接调用 Database
        let db = self.database.clone();
        find_device_commands_by_device_id(&db, device_id)
            .await
            .map_err(|e| self.io_error(e))
    }

    /// Get device command by name
    pub async fn get_device_command_by_name(
        &self,
        device_id: &str,
        command_name: &str,
    ) -> Result<Option<DeviceCommand>, Error> {
        let commands = self.get_device_commands(device_id).await?;
        Ok(commands.into_iter().find(|c| c.name == command_name))
    }

    // === 批量操作 ===

    /// 批量创建设备
    pub async fn create_devices_batch(
        &self,
        requests: &[CreateDeviceRequest],
    ) -> Result<Vec<Device>, Error> {
        tracing::info!("Creating {} devices in batch", requests.len());

        // Validate device name uniqueness
        for request in requests {
            if self.repository.exists_by_name(&request.name).await.unwrap_or(false) {
                return Err(Error::ValidationError(format!("{}: '{}'", ERROR_DEVICE_NAME_EXISTS, request.name)));
            }
        }

        // 批量创建设备
        let created_devices = self.repository.create_batch(requests).await?;

        // 发布批量设备创建事件
        for device in &created_devices {
            self.publish_batch_device_created_event(device).await;
        }

        tracing::info!("Successfully created {} devices in batch", created_devices.len());
        Ok(created_devices)
    }

    /// Publish batch device created event
    async fn publish_batch_device_created_event(&self, device: &Device) {
        let content_elements = self.build_device_info_elements(
            device,
            format!("Device '{}' created in batch operation", device.name),
        );

        // 使用通用的 publish_device_event 方法
        let _ = self.publish_device_event(
            DeviceEventType::DeviceCreated,
            EventLevel::Info,
            device,
            format!("Device Created: {}", device.name),
            content_elements,
        ).await;
    }

    /// Publish batch device deleted event
    async fn publish_batch_device_deleted_event(&self, device: &Device) {
        let content_elements = self.build_device_info_elements(
            device,
            format!("Device '{}' deleted in batch operation", device.name),
        );

        // 使用通用的 publish_device_event 方法
        let _ = self.publish_device_event(
            DeviceEventType::DeviceDeleted,
            EventLevel::Warning,
            device,
            format!("Device Deleted: {}", device.name),
            content_elements,
        ).await;
    }

    /// 批量删除设备
    pub async fn delete_devices_batch(&self, device_ids: &[String]) -> Result<u64, Error> {
        tracing::info!("Deleting {} devices in batch", device_ids.len());

        // 获取设备信息（用于事件）
        let devices = self.repository.find_by_ids(device_ids).await?;

        // 批量删除设备
        let deleted_count = self.repository.delete_by_ids(device_ids).await?;

        // 发布批量设备删除事件
        for device in &devices {
            self.publish_batch_device_deleted_event(device).await;
        }

        tracing::info!("Successfully deleted {} devices in batch", deleted_count);
        Ok(deleted_count)
    }

    /// 批量更新设备状态
    pub async fn update_device_states_batch(
        &self,
        updates: &[(String, i32)],
    ) -> Result<u64, Error> {
        tracing::info!("Updating {} device states in batch", updates.len());

        let updated_count = self.repository.update_states_batch(updates).await?;

        // 发布设备更新事件（简化版，只记录日志）
        for (device_id, new_state) in updates {
            tracing::debug!("Device {} state updated to {}", device_id, new_state);
        }

        tracing::info!("Successfully updated {} device states in batch", updated_count);
        Ok(updated_count)
    }

    // === 验证和工具方法 ===

    /// 检查设备是否存在
    pub async fn device_exists(&self, device_id: &str) -> Result<bool, Error> {
        match self.get_device_by_id(device_id).await? {
            Some(_) => Ok(true),
            None => Ok(false),
        }
    }

    /// 检查设备名称是否存在
    pub async fn device_name_exists(&self, name: &str) -> Result<bool, Error> {
        self.repository
            .exists_by_name(name)
            .await
            .map_err(|e| self.io_error(e))
    }

    /// 验证设备驱动配置
    fn validate_driver_configuration(&self, device: &Device) -> Option<String> {
        if device.driver_name.is_none() {
            Some(ERROR_DEVICE_DRIVER_NOT_CONFIGURED.to_string())
        } else {
            None
        }
    }

    /// 验证设备地址配置
    fn validate_address_configuration(&self, device: &Device) -> Option<String> {
        if device.address.as_ref().is_none_or(|addr| addr.is_empty()) {
            Some(ERROR_DEVICE_ADDRESS_NOT_CONFIGURED.to_string())
        } else {
            None
        }
    }

    /// 验证设备配置
    pub async fn validate_device(&self, device_id: &str) -> Result<Vec<String>, Error> {
        let device = self.get_device_by_id(device_id).await?.ok_or(Error::NotFound)?;

        let mut errors = Vec::new();

        // 基本验证
        if let Err(e) = device.validate() {
            errors.push(e);
        }

        // 驱动验证
        if let Some(error) = self.validate_driver_configuration(&device) {
            errors.push(error);
        }

        // 地址验证
        if let Some(error) = self.validate_address_configuration(&device) {
            errors.push(error);
        }

        Ok(errors)
    }

    /// 加载设备属性，失败时返回空向量并记录警告
    async fn load_properties_with_fallback(&self, device_id: &str) -> Option<Vec<DeviceProperty>> {
        match self.get_device_properties(device_id).await {
            Ok(properties) => Some(properties),
            Err(e) => {
                tracing::warn!("Failed to load properties for device {}: {}", device_id, e);
                Some(Vec::new())
            }
        }
    }

    /// 加载设备命令，失败时返回空向量并记录警告
    async fn load_commands_with_fallback(&self, device_id: &str) -> Option<Vec<DeviceCommand>> {
        match self.get_device_commands(device_id).await {
            Ok(commands) => Some(commands),
            Err(e) => {
                tracing::warn!("Failed to load commands for device {}: {}", device_id, e);
                Some(Vec::new())
            }
        }
    }

    /// 加载完整的设备信息（包含属性和命令）
    pub async fn load_complete_device(&self, device_id: &str) -> Result<Option<Device>, Error> {
        // 从数据库加载设备基本信息
        let mut device = match self.repository.find_by_id(device_id).await? {
            Some(device) => device,
            None => return Ok(None)
};

        // 加载设备属性
        device.properties = self.load_properties_with_fallback(device_id).await;

        // 加载设备指令
        device.commands = self.load_commands_with_fallback(device_id).await;

        // 初始化运行时状态
        device.is_online = false; // 默认离线，由DataServer更新
        device.last_heartbeat = None; // 默认无心跳，由DataServer更新

        Ok(Some(device))
    }

    /// 批量加载完整的设备信息
    pub async fn load_complete_devices(&self, device_ids: &[String]) -> Result<Vec<Device>, Error> {
        let mut devices = Vec::new();

        for device_id in device_ids {
            if let Some(device) = self.load_complete_device(device_id).await? {
                devices.push(device);
            } else {
                tracing::warn!("Device not found in database: {}", device_id);
            }
        }

        Ok(devices)
    }

    // === 设备命令执行 ===

    /// 构建设备命令请求
    fn build_device_command_request(
        &self,
        device_id: &str,
        command_name: &str,
        command_type: &str,
        params: Option<String>,
    ) -> CreateDeviceCommandRequest {
        CreateDeviceCommandRequest {
            device_id: device_id.to_string(),
            name: command_name.to_string(),
            display_name: Some(format!("{} ({})", command_name, command_type)),
            description: Some(format!("Automation command: {} via {}", command_name, command_type)),
            parameters: params,
        }
    }

    /// 发送设备命令（供自动化执行器使用）
    ///
    /// Create device命令记录（实际执行由 DataServer 通过事件驱动）
    pub async fn send_command(
        &self,
        device_id: &str,
        command_name: &str,
        command_type: &str,
        params: Option<String>,
    ) -> Result<String, Error> {
        // 验证设备存在
        let device = self.repository.find_by_id(device_id).await?
            .ok_or(Error::NotFound)?;

        let command_id = uuid::Uuid::new_v4().to_string();

        tracing::info!(
            "Automation sent command '{}' ({}) to device '{}' (command_id: {})",
            command_name,
            command_type,
            device.name,
            command_id
        );

        // 存储命令到数据库（用于历史记录和 DataServer 轮询）
        let create_request = self.build_device_command_request(device_id, command_name, command_type, params);
        // TODO: DeviceCommand 尚未提取到 repository，暂时仍直接调用 Database
        let db = self.database.clone();
        let _ = create_device_command(&db, &create_request).await;

        // 发布命令执行事件（DataServer 会处理实际执行）
        self.publish_command_started_event(&device, command_name, command_type, &command_id).await;

        Ok(command_id)
    }
}

/// 将 DeviceQueryParams 转换为 DeviceCriteria
fn params_to_criteria(params: &DeviceQueryParams) -> DeviceCriteria {
    DeviceCriteria {
        name: params.name.clone(),
        display_name: params.display_name.clone(),
        device_type: params.device_type.clone(),
        address: params.address.clone(),
        driver_name: params.driver_name.clone(),
        state: params.state,
        parent_id: params.parent_id.clone(),
        product_id: params.product_id.clone(),
        search_text: None,
        sort_by: tinyiothub_storage::traits::device::DeviceSortBy::CreatedAt,
        sort_order: tinyiothub_storage::traits::device::DeviceSortOrder::Descending,
        limit: params.page_size,
        offset: params.page.map(|p| p.saturating_sub(1) * params.page_size.unwrap_or(0)),
    }
}
