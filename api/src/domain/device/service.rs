use std::sync::Arc;

use crate::{
    domain::event::{
        entities::Event as DomainEvent,
        value_objects::{
            ContentElement, DeviceEventType, EventLevel, EventSource, RichContent, TextFormat,
        },
    },
    dto::{
        entity::{
            device::{CreateDeviceRequest, DeviceQueryParams, UpdateDeviceRequest},
            device_command::CreateDeviceCommandRequest,
            Device, DeviceCommand, DeviceProperty,
        },
        request::pagination::DataObjectWithPagination,
    },
    infrastructure::{event::EventBus, persistence::database::Database},
    shared::error::Error,
};

/// 设备服务
/// 负责设备的业务逻辑和事件发布
pub struct DeviceService {
    database: Arc<Database>,
    event_bus: Option<Arc<EventBus>>,
}

impl DeviceService {
    pub fn new(database: Arc<Database>) -> Self {
        Self { database, event_bus: None }
    }

    /// Create device service with event bus
    pub fn with_event_bus(database: Arc<Database>, event_bus: Arc<EventBus>) -> Self {
        Self { database, event_bus: Some(event_bus) }
    }

    /// 创建设备
    pub async fn create_device(&self, request: &CreateDeviceRequest) -> Result<Device, Error> {
        tracing::info!("Creating device: {}", request.name);

        // 验证设备名称唯一性
        if Device::exists_by_name(&self.database, &request.name).await.unwrap_or(false) {
            return Err(Error::ValidationError("设备名称已存在".to_string()));
        }

        // 创建设备
        let created_device = Device::create_with_tags(&self.database, request).await?;

        // 发布设备创建事件
        self.publish_device_created_event(&created_device).await;

        tracing::info!("Device {} created successfully", created_device.id);
        Ok(created_device)
    }

    /// 基于模板创建设备
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

        // 验证设备名称唯一性
        if Device::exists_by_name(&self.database, &device_input.name).await.unwrap_or(false) {
            return Err(Error::ValidationError("设备名称已存在".to_string()));
        }

        // 使用模板引擎应用模板
        let device_request = template_engine
            .apply_template(template_id, device_input)
            .await
            .map_err(|e| Error::ValidationError(format!("模板应用失败: {}", e)))?;

        // 创建设备
        let created_device = Device::create_with_tags(&self.database, &device_request).await?;

        // 获取模板
        let template = template_engine
            .get_repository()
            .find_by_id(template_id)
            .await
            .map_err(|e| Error::IOError(format!("Failed to get template: {}", e)))?
            .ok_or_else(|| Error::NotFound)?;

        // 生成并创建设备属性
        match template_engine
            .generate_device_properties(&template, device_input, &created_device.id)
            .await
        {
            Ok(properties) => {
                if !properties.is_empty() {
                    if let Err(e) = DeviceProperty::create_batch(&self.database, &properties).await
                    {
                        tracing::warn!("Failed to create device properties: {}", e);
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Failed to generate device properties: {}", e);
            }
        }

        // 生成并创建设备命令
        match template_engine
            .generate_device_commands(&template, device_input, &created_device.id)
            .await
        {
            Ok(commands) => {
                if !commands.is_empty() {
                    if let Err(e) = DeviceCommand::bulk_create(&self.database, &commands).await {
                        tracing::warn!("Failed to create device commands: {}", e);
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Failed to generate device commands: {}", e);
            }
        }

        // 发布设备创建事件
        self.publish_device_created_event(&created_device).await;

        tracing::info!(
            "Device created successfully from template: device_id={}",
            created_device.id
        );
        Ok(created_device)
    }

    /// 发布设备创建事件（提取为独立方法）
    async fn publish_device_created_event(&self, device: &Device) {
        if let Some(ref event_bus) = self.event_bus {
            let event = DomainEvent::new_device_event(
                DeviceEventType::DeviceCreated,
                EventLevel::Info,
                EventSource::device(device.id.clone(), Some("device_service".to_string())),
                RichContent::new(
                    format!("Device Created: {}", device.name),
                    vec![
                        ContentElement::Text {
                            content: format!(
                                "Device '{}' has been created successfully",
                                device.name
                            ),
                            format: TextFormat::Plain,
                        },
                        ContentElement::Text {
                            content: format!("Device ID: {}", device.id),
                            format: TextFormat::Plain,
                        },
                        ContentElement::Text {
                            content: format!(
                                "Device Type: {}",
                                device.device_type.as_deref().unwrap_or("N/A")
                            ),
                            format: TextFormat::Plain,
                        },
                    ],
                ),
                device.workspace_id.clone(),
            );

            if let Ok(event) = event {
                let event_bus_clone = event_bus.clone();
                crate::utils::publish_event_safe(event_bus_clone, event).await;
            }
        }
    }

    /// 更新设备
    pub async fn update_device(
        &self,
        device_id: &str,
        request: &UpdateDeviceRequest,
    ) -> Result<Device, Error> {
        tracing::info!("Updating device: {}", device_id);

        // 获取旧设备信息
        let old_device =
            Device::find_by_id(&self.database, device_id).await?.ok_or(Error::NotFound)?;

        // 更新设备
        let updated_device = Device::update_with_tags(&self.database, device_id, request).await?;

        // 发布设备更新事件
        if let Some(ref event_bus) = self.event_bus {
            let mut changes = Vec::new();

            // 检测变化的字段
            if let Some(ref new_name) = request.name {
                if new_name != &old_device.name {
                    changes.push(format!("Name: {} → {}", old_device.name, new_name));
                }
            }
            if let Some(ref new_type) = request.device_type {
                if Some(new_type) != old_device.device_type.as_ref() {
                    changes.push(format!(
                        "Type: {} → {}",
                        old_device.device_type.as_deref().unwrap_or("N/A"),
                        new_type
                    ));
                }
            }
            if let Some(ref new_address) = request.address {
                if Some(new_address) != old_device.address.as_ref() {
                    changes.push(format!(
                        "Address: {} → {}",
                        old_device.address.as_deref().unwrap_or("N/A"),
                        new_address
                    ));
                }
            }

            if !changes.is_empty() {
                let mut elements = vec![ContentElement::Text {
                    content: format!("Device '{}' has been updated", updated_device.name),
                    format: TextFormat::Plain,
                }];

                for change in changes {
                    elements
                        .push(ContentElement::Text { content: change, format: TextFormat::Plain });
                }

                let event = DomainEvent::new_device_event(
                    DeviceEventType::DeviceUpdated,
                    EventLevel::Info,
                    EventSource::device(
                        updated_device.id.clone(),
                        Some("device_service".to_string()),
                    ),
                    RichContent::new(format!("Device Updated: {}", updated_device.name), elements),
                    updated_device.workspace_id.clone(),
                )
                .map_err(|e| Error::IOError(format!("Failed to create event: {}", e)))?;

                let event_bus_clone = event_bus.clone();
                crate::utils::publish_event_safe(event_bus_clone, event).await;
            }
        }

        tracing::info!("Device {} updated successfully", device_id);
        Ok(updated_device)
    }

    /// 删除设备
    pub async fn delete_device(&self, device_id: &str) -> Result<bool, Error> {
        tracing::info!("Deleting device: {}", device_id);

        // 获取设备信息（用于事件）
        let device = Device::find_by_id(&self.database, device_id).await?.ok_or(Error::NotFound)?;

        // 删除设备
        let deleted_count = Device::delete(&self.database, device_id).await?;
        let success = deleted_count > 0;

        if success {
            // 发布设备删除事件
            if let Some(ref event_bus) = self.event_bus {
                let event = DomainEvent::new_device_event(
                    DeviceEventType::DeviceDeleted,
                    EventLevel::Warning,
                    EventSource::device(device.id.clone(), Some("device_service".to_string())),
                    RichContent::new(
                        format!("Device Deleted: {}", device.name),
                        vec![
                            ContentElement::Text {
                                content: format!("Device '{}' has been deleted", device.name),
                                format: TextFormat::Plain,
                            },
                            ContentElement::Text {
                                content: format!("Device ID: {}", device.id),
                                format: TextFormat::Plain,
                            },
                            ContentElement::Text {
                                content: format!(
                                    "Device Type: {}",
                                    device.device_type.as_deref().unwrap_or("N/A")
                                ),
                                format: TextFormat::Plain,
                            },
                        ],
                    ),
                    device.workspace_id.clone(),
                )
                .map_err(|e| Error::IOError(format!("Failed to create event: {}", e)))?;

                let event_bus_clone = event_bus.clone();
                crate::utils::publish_event_safe(event_bus_clone, event).await;
            }

            tracing::info!("Device {} deleted successfully", device_id);
        }

        Ok(success)
    }

    /// 更新设备状态
    pub async fn update_device_state(&self, device_id: &str, new_state: i32) -> Result<(), Error> {
        // 获取当前状态
        let device = Device::find_by_id(&self.database, device_id).await?.ok_or(Error::NotFound)?;

        let old_state = device.state.unwrap_or(0);

        if old_state != new_state {
            // 更新数据库中的状态
            Device::update_state(&self.database, device_id, new_state).await?;

            // 获取更新后的设备信息
            if let Ok(Some(updated_device)) = Device::find_by_id(&self.database, device_id).await {
                // 发布设备状态更新事件
                if let Some(ref event_bus) = self.event_bus {
                    let event = DomainEvent::new_device_event(
                        DeviceEventType::DeviceUpdated,
                        EventLevel::Info,
                        EventSource::device(
                            device_id.to_string(),
                            Some("device_service".to_string()),
                        ),
                        RichContent::new(
                            format!("Device State Updated: {}", updated_device.name),
                            vec![
                                ContentElement::Text {
                                    content: format!(
                                        "Device '{}' state changed from {} to {}",
                                        updated_device.name, old_state, new_state
                                    ),
                                    format: TextFormat::Plain,
                                },
                                ContentElement::Text {
                                    content: format!("Device ID: {}", device_id),
                                    format: TextFormat::Plain,
                                },
                            ],
                        ),
                        updated_device.workspace_id.clone(),
                    )
                    .map_err(|e| Error::IOError(format!("Failed to create event: {}", e)))?;

                    let event_bus_clone = event_bus.clone();
                    crate::utils::publish_event_safe(event_bus_clone, event).await;
                }

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

    // === 查询功能 ===

    /// 根据ID获取设备
    pub async fn get_device_by_id(&self, device_id: &str) -> Result<Option<Device>, Error> {
        Device::find_by_id(&self.database, device_id)
            .await
            .map_err(|e| Error::IOError(e.to_string()))
    }

    /// 根据ID获取设备（包含标签）
    pub async fn get_device_by_id_with_tags(
        &self,
        device_id: &str,
    ) -> Result<Option<Device>, Error> {
        Device::find_by_id_with_tags(&self.database, device_id)
            .await
            .map_err(|e| Error::IOError(e.to_string()))
    }

    /// 根据名称获取设备
    pub async fn get_device_by_name(&self, name: &str) -> Result<Option<Device>, Error> {
        Device::find_by_name(&self.database, name).await.map_err(|e| Error::IOError(e.to_string()))
    }

    /// 查询设备列表
    pub async fn get_devices(&self, params: &DeviceQueryParams) -> Result<Vec<Device>, Error> {
        Device::find_all(&self.database, params).await.map_err(|e| Error::IOError(e.to_string()))
    }

    /// 查询设备列表（包含标签）
    pub async fn get_devices_with_tags(
        &self,
        params: &DeviceQueryParams,
    ) -> Result<Vec<Device>, Error> {
        Device::find_all_with_tags(&self.database, params)
            .await
            .map_err(|e| Error::IOError(e.to_string()))
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

    /// 搜索设备（模糊匹配）
    pub async fn search_devices(
        &self,
        keyword: &str,
        limit: Option<u32>,
    ) -> Result<Vec<Device>, Error> {
        Device::search(&self.database, keyword, limit)
            .await
            .map_err(|e| Error::IOError(e.to_string()))
    }

    /// 根据父设备ID获取子设备
    pub async fn get_child_devices(&self, parent_id: &str) -> Result<Vec<Device>, Error> {
        Device::find_children(&self.database, parent_id)
            .await
            .map_err(|e| Error::IOError(e.to_string()))
    }

    /// 根据产品ID获取设备
    pub async fn get_devices_by_product(&self, product_id: &str) -> Result<Vec<Device>, Error> {
        Device::find_by_product_id(&self.database, product_id)
            .await
            .map_err(|e| Error::IOError(e.to_string()))
    }

    /// 根据驱动名称获取设备
    pub async fn get_devices_by_driver(&self, driver_name: &str) -> Result<Vec<Device>, Error> {
        Device::find_by_driver_name(&self.database, driver_name)
            .await
            .map_err(|e| Error::IOError(e.to_string()))
    }

    /// 获取设备树结构
    pub async fn get_device_tree(&self, root_id: Option<&str>) -> Result<Vec<Device>, Error> {
        Device::get_device_tree(&self.database, root_id)
            .await
            .map_err(|e| Error::IOError(e.to_string()))
    }

    // === 设备属性查询 ===

    /// 获取设备属性列表
    pub async fn get_device_properties(
        &self,
        device_id: &str,
    ) -> Result<Vec<DeviceProperty>, Error> {
        DeviceProperty::find_by_device_id(&self.database, device_id)
            .await
            .map_err(|e| Error::IOError(e.to_string()))
    }

    /// 根据名称获取设备属性
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
        DeviceCommand::find_by_device_id(&self.database, device_id)
            .await
            .map_err(|e| Error::IOError(e.to_string()))
    }

    /// 根据名称获取设备命令
    pub async fn get_device_command_by_name(
        &self,
        device_id: &str,
        command_name: &str,
    ) -> Result<Option<DeviceCommand>, Error> {
        let commands = self.get_device_commands(device_id).await?;
        Ok(commands.into_iter().find(|c| c.name == command_name))
    }

    // === 统计查询 ===

    /// 获取设备统计信息
    pub async fn get_device_stats(&self) -> Result<crate::dto::entity::device::DeviceStats, Error> {
        Device::get_stats(&self.database).await.map_err(|e| Error::IOError(e.to_string()))
    }

    /// 按类型统计设备
    pub async fn get_device_stats_by_type(&self) -> Result<Vec<(String, i64)>, Error> {
        Device::get_stats_by_type(&self.database).await.map_err(|e| Error::IOError(e.to_string()))
    }

    /// 按驱动统计设备
    pub async fn get_device_stats_by_driver(&self) -> Result<Vec<(String, i64)>, Error> {
        Device::get_stats_by_driver(&self.database).await.map_err(|e| Error::IOError(e.to_string()))
    }

    // === 批量操作 ===

    /// 批量创建设备
    pub async fn create_devices_batch(
        &self,
        requests: &[CreateDeviceRequest],
    ) -> Result<Vec<Device>, Error> {
        tracing::info!("Creating {} devices in batch", requests.len());

        // 验证设备名称唯一性
        for request in requests {
            if Device::exists_by_name(&self.database, &request.name).await.unwrap_or(false) {
                return Err(Error::ValidationError(format!("设备名称 '{}' 已存在", request.name)));
            }
        }

        // 批量创建设备
        let created_devices = Device::create_batch(&self.database, requests).await?;

        // 发布批量设备创建事件
        if let Some(ref event_bus) = self.event_bus {
            for device in &created_devices {
                let event = DomainEvent::new_device_event(
                    DeviceEventType::DeviceCreated,
                    EventLevel::Info,
                    EventSource::device(device.id.clone(), Some("device_service".to_string())),
                    RichContent::new(
                        format!("Device Created: {}", device.name),
                        vec![
                            ContentElement::Text {
                                content: format!(
                                    "Device '{}' created in batch operation",
                                    device.name
                                ),
                                format: TextFormat::Plain,
                            },
                            ContentElement::Text {
                                content: format!("Device ID: {}", device.id),
                                format: TextFormat::Plain,
                            },
                            ContentElement::Text {
                                content: format!(
                                    "Device Type: {}",
                                    device.device_type.as_deref().unwrap_or("N/A")
                                ),
                                format: TextFormat::Plain,
                            },
                        ],
                    ),
                    device.workspace_id.clone(),
                )
                .map_err(|e| Error::IOError(format!("Failed to create event: {}", e)))?;

                let event_bus_clone = event_bus.clone();
                crate::utils::publish_event_safe(event_bus_clone, event).await;
            }
        }

        tracing::info!("Successfully created {} devices in batch", created_devices.len());
        Ok(created_devices)
    }

    /// 批量删除设备
    pub async fn delete_devices_batch(&self, device_ids: &[String]) -> Result<u64, Error> {
        tracing::info!("Deleting {} devices in batch", device_ids.len());

        // 获取设备信息（用于事件）
        let devices = Device::find_by_ids(&self.database, device_ids).await?;

        // 批量删除设备
        let deleted_count = Device::delete_by_ids(&self.database, device_ids).await?;

        // 发布批量设备删除事件
        if let Some(ref event_bus) = self.event_bus {
            for device in &devices {
                let event = DomainEvent::new_device_event(
                    DeviceEventType::DeviceDeleted,
                    EventLevel::Warning,
                    EventSource::device(device.id.clone(), Some("device_service".to_string())),
                    RichContent::new(
                        format!("Device Deleted: {}", device.name),
                        vec![
                            ContentElement::Text {
                                content: format!(
                                    "Device '{}' deleted in batch operation",
                                    device.name
                                ),
                                format: TextFormat::Plain,
                            },
                            ContentElement::Text {
                                content: format!("Device ID: {}", device.id),
                                format: TextFormat::Plain,
                            },
                            ContentElement::Text {
                                content: format!(
                                    "Device Type: {}",
                                    device.device_type.as_deref().unwrap_or("N/A")
                                ),
                                format: TextFormat::Plain,
                            },
                        ],
                    ),
                    device.workspace_id.clone(),
                )
                .map_err(|e| Error::IOError(format!("Failed to create event: {}", e)))?;

                let event_bus_clone = event_bus.clone();
                crate::utils::publish_event_safe(event_bus_clone, event).await;
            }
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

        let updated_count = Device::update_states_batch(&self.database, updates).await?;

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
        Device::exists_by_name(&self.database, name)
            .await
            .map_err(|e| Error::IOError(e.to_string()))
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
        if device.driver_name.is_none() {
            errors.push("设备未配置驱动".to_string());
        }

        // 地址验证
        if device.address.as_ref().map_or(true, |addr| addr.is_empty()) {
            errors.push("设备地址未配置".to_string());
        }

        Ok(errors)
    }

    /// 加载完整的设备信息（包含属性和命令）
    pub async fn load_complete_device(&self, device_id: &str) -> Result<Option<Device>, Error> {
        // 从数据库加载设备基本信息
        let mut device = match Device::find_by_id(&self.database, device_id).await? {
            Some(device) => device,
            None => return Ok(None),
        };

        // 加载设备属性
        match DeviceProperty::find_by_device_id(&self.database, device_id).await {
            Ok(properties) => {
                device.properties = Some(properties);
            }
            Err(e) => {
                tracing::warn!("Failed to load properties for device {}: {}", device_id, e);
                device.properties = Some(Vec::new());
            }
        }

        // 加载设备指令
        match DeviceCommand::find_by_device_id(&self.database, device_id).await {
            Ok(commands) => {
                device.commands = Some(commands);
            }
            Err(e) => {
                tracing::warn!("Failed to load commands for device {}: {}", device_id, e);
                device.commands = Some(Vec::new());
            }
        }

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

    /// 发送设备命令（供自动化执行器使用）
    ///
    /// 创建设备命令记录（实际执行由 DataServer 通过事件驱动）
    pub async fn send_command(
        &self,
        device_id: &str,
        command_name: &str,
        command_type: &str,
        params: Option<String>,
    ) -> Result<String, Error> {
        // 验证设备存在
        let device = Device::find_by_id(&self.database, device_id).await?
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
        let create_request = CreateDeviceCommandRequest {
            device_id: device_id.to_string(),
            name: command_name.to_string(),
            display_name: Some(format!("{} ({})", command_name, command_type)),
            description: Some(format!("Automation command: {} via {}", command_name, command_type)),
            parameters: params,
        };
        let _ = DeviceCommand::create(&self.database, &create_request).await;

        // 发布命令执行事件（DataServer 会处理实际执行）
        if let Some(ref event_bus) = self.event_bus {
            let event = crate::domain::event::entities::Event::new_device_event(
                crate::domain::event::value_objects::DeviceEventType::CommandStarted,
                crate::domain::event::value_objects::EventLevel::Info,
                crate::domain::event::value_objects::EventSource::device(
                    device_id.to_string(),
                    Some("automation_service".to_string()),
                ),
                crate::domain::event::value_objects::RichContent::new(
                    format!("Command: {} ({})", command_name, command_type),
                    vec![
                        crate::domain::event::value_objects::ContentElement::Text {
                            content: format!("Device: {}", device.name),
                            format: crate::domain::event::value_objects::TextFormat::Plain,
                        },
                        crate::domain::event::value_objects::ContentElement::Text {
                            content: format!("Command ID: {}", command_id),
                            format: crate::domain::event::value_objects::TextFormat::Plain,
                        },
                    ],
                ),
                device.workspace_id.clone(),
            );
            if let Ok(event) = event {
                let event_bus_clone = event_bus.clone();
                crate::utils::publish_event_safe(event_bus_clone, event).await;
            }
        }

        Ok(command_id)
    }
}
