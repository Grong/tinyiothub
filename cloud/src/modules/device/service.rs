// Device service — migrated from domain/device/service.rs

use std::sync::Arc;

use tinyiothub_core::models::{
    device::{CreateDeviceRequest, Device, DeviceQueryParams, UpdateDeviceRequest},
    device_command::{CreateDeviceCommandRequest, DeviceCommand},
    device_property::DeviceProperty,
};

use crate::shared::persistence::repositories::{
    bulk_create_device_commands, create_device_command, create_device_properties_batch,
    find_device_commands_by_device_id, find_device_properties_by_device_id,
};

const ERROR_DEVICE_NAME_EXISTS: &str = "Device name already exists";
const ERROR_TEMPLATE_APPLICATION_FAILED: &str = "Template application failed";
const ERROR_DEVICE_DRIVER_NOT_CONFIGURED: &str = "Device driver not configured";
const ERROR_DEVICE_ADDRESS_NOT_CONFIGURED: &str = "Device address not configured";
const MSG_DEVICE_TYPE_VALUE_NA: &str = "N/A";

use crate::{
    modules::{
        device::repository::{DeviceCriteria, DeviceRepository},
        event::{
            entities::Event as DomainEvent,
            value_objects::{
                ContentElement, DeviceEventType, EventLevel, EventSource, RichContent, TextFormat,
            },
        },
        tag::TagRepository,
    },
    shared::{
        error::Error, event::EventBus, pagination::DataObjectWithPagination, persistence::Database,
    },
};

pub struct DeviceService {
    repository: Arc<dyn DeviceRepository>,
    database: Arc<Database>,
    event_bus: Option<Arc<EventBus>>,
    tag_repository: Option<Arc<dyn TagRepository>>,
}

impl DeviceService {
    pub fn new(repository: Arc<dyn DeviceRepository>, database: Arc<Database>) -> Self {
        Self { repository, database, event_bus: None, tag_repository: None }
    }

    pub fn with_event_bus(
        repository: Arc<dyn DeviceRepository>,
        database: Arc<Database>,
        event_bus: Arc<EventBus>,
    ) -> Self {
        Self { repository, database, event_bus: Some(event_bus), tag_repository: None }
    }

    pub fn with_tag_repository(mut self, tag_repository: Arc<dyn TagRepository>) -> Self {
        self.tag_repository = Some(tag_repository);
        self
    }

    pub async fn create_device(&self, request: &CreateDeviceRequest) -> Result<Device, Error> {
        tracing::info!("Creating device: {}", request.name);
        if self.repository.exists_by_name(&request.name).await.unwrap_or(false) {
            return Err(Error::ValidationError(ERROR_DEVICE_NAME_EXISTS.to_string()));
        }
        let created_device = self.repository.create(request).await?;
        // 加载完整设备信息（含属性、指令）再发布事件
        let complete_device =
            self.load_complete_device(&created_device.id).await?.unwrap_or(created_device.clone());
        self.publish_device_created_event(&complete_device).await;
        tracing::info!("Device {} created successfully", created_device.id);
        Ok(created_device)
    }

    pub async fn create_device_from_template(
        &self,
        template_engine: &crate::modules::template::TemplateEngine,
        template_id: &str,
        device_input: &crate::modules::template::types::DeviceCreationInput,
    ) -> Result<Device, Error> {
        tracing::info!(
            "Creating device from template: template_id={}, device_name={}",
            template_id,
            device_input.name
        );
        let created_device = self
            .apply_template_and_create_device(template_engine, template_id, device_input)
            .await?;
        let template = self.get_template(template_engine, template_id).await?;
        self.generate_and_create_properties(
            template_engine,
            &template,
            device_input,
            &created_device.id,
        )
        .await;
        self.generate_and_create_commands(
            template_engine,
            &template,
            device_input,
            &created_device.id,
        )
        .await;
        // 加载完整设备信息（含属性、指令）再发布事件
        let complete_device =
            self.load_complete_device(&created_device.id).await?.unwrap_or(created_device.clone());
        self.publish_device_created_event(&complete_device).await;
        tracing::info!(
            "Device created successfully from template: device_id={}",
            created_device.id
        );
        Ok(created_device)
    }

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

    fn create_text_element(&self, content: String) -> ContentElement {
        ContentElement::Text { content, format: TextFormat::Plain }
    }

    fn io_error(&self, e: impl std::fmt::Display) -> Error {
        Error::IOError(e.to_string())
    }

    async fn publish_device_created_event(&self, device: &Device) {
        let content_elements = self.build_device_info_elements(
            device,
            format!("Device '{}' has been created successfully", device.name),
        );
        // 将完整设备信息序列化到 metadata，供 DataServer 使用
        let device_json = serde_json::to_value(device).unwrap_or(serde_json::Value::Null);
        let _ = self
            .publish_device_event_with_metadata(
                DeviceEventType::DeviceCreated,
                EventLevel::Info,
                device,
                format!("Device Created: {}", device.name),
                content_elements,
                device_json,
            )
            .await;
    }

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

    async fn publish_device_event_with_metadata(
        &self,
        event_type: DeviceEventType,
        level: EventLevel,
        device: &Device,
        title: String,
        content_elements: Vec<ContentElement>,
        metadata_value: serde_json::Value,
    ) -> Result<(), Error> {
        if let Some(ref event_bus) = self.event_bus {
            let content = RichContent::new(title, content_elements)
                .with_metadata("device".to_string(), metadata_value);
            let event = DomainEvent::new_device_event(
                event_type,
                level,
                EventSource::device(device.id.clone(), Some("device_service".to_string())),
                content,
            )
            .map_err(|e| Error::IOError(format!("Failed to create event: {}", e)))?;
            let event_bus_clone = event_bus.clone();
            crate::shared::utils::publish_event_safe(event_bus_clone, event).await;
        }
        Ok(())
    }

    async fn get_template(
        &self,
        template_engine: &crate::modules::template::TemplateEngine,
        template_id: &str,
    ) -> Result<crate::modules::template::types::DeviceTemplate, Error> {
        template_engine
            .get_repository()
            .find_by_id(template_id)
            .await
            .map_err(|e| Error::IOError(format!("Failed to get template: {}", e)))?
            .ok_or(Error::NotFound)
    }

    async fn apply_template_and_create_device(
        &self,
        template_engine: &crate::modules::template::TemplateEngine,
        template_id: &str,
        device_input: &crate::modules::template::types::DeviceCreationInput,
    ) -> Result<Device, Error> {
        if self.repository.exists_by_name(&device_input.name).await.unwrap_or(false) {
            return Err(Error::ValidationError(ERROR_DEVICE_NAME_EXISTS.to_string()));
        }
        let device_request =
            template_engine.apply_template(template_id, device_input).await.map_err(|e| {
                Error::ValidationError(format!("{}: {}", ERROR_TEMPLATE_APPLICATION_FAILED, e))
            })?;
        self.repository.create(&device_request).await
    }

    async fn generate_and_create_properties(
        &self,
        template_engine: &crate::modules::template::TemplateEngine,
        template: &crate::modules::template::types::DeviceTemplate,
        device_input: &crate::modules::template::types::DeviceCreationInput,
        device_id: &str,
    ) {
        match template_engine.generate_device_properties(template, device_input, device_id).await {
            Ok(properties) => {
                if !properties.is_empty() {
                    let db = self.database.clone();
                    if let Err(e) = create_device_properties_batch(&db, &properties).await {
                        tracing::warn!("{}", e);
                    }
                }
            }
            Err(e) => tracing::warn!("Failed to generate device properties: {}", e),
        }
    }

    async fn generate_and_create_commands(
        &self,
        template_engine: &crate::modules::template::TemplateEngine,
        template: &crate::modules::template::types::DeviceTemplate,
        device_input: &crate::modules::template::types::DeviceCreationInput,
        device_id: &str,
    ) {
        match template_engine.generate_device_commands(template, device_input, device_id).await {
            Ok(commands) => {
                if !commands.is_empty() {
                    let db = self.database.clone();
                    if let Err(e) = bulk_create_device_commands(&db, &commands).await {
                        tracing::warn!("Failed to create device commands: {}", e);
                    }
                }
            }
            Err(e) => tracing::warn!("Failed to generate device commands: {}", e),
        }
    }

    pub async fn update_device(
        &self,
        device_id: &str,
        request: &UpdateDeviceRequest,
    ) -> Result<Device, Error> {
        tracing::info!("Updating device: {}", device_id);
        let old_device = self.repository.find_by_id(device_id).await?.ok_or(Error::NotFound)?;
        let updated_device = self.repository.update(device_id, request).await?;
        self.publish_device_updated_event(&old_device, request, &updated_device).await;
        tracing::info!("Device {} updated successfully", device_id);
        Ok(updated_device)
    }

    async fn publish_device_updated_event(
        &self,
        old_device: &Device,
        request: &UpdateDeviceRequest,
        updated_device: &Device,
    ) {
        let mut changes = Vec::new();
        if let Some(ref new_name) = request.name
            && new_name != &old_device.name
        {
            changes.push(format!("Name: {} → {}", old_device.name, new_name));
        }
        if let Some(ref new_type) = request.device_type
            && Some(new_type) != old_device.device_type.as_ref()
        {
            changes.push(format!(
                "Type: {} → {}",
                old_device.device_type.as_deref().unwrap_or("N/A"),
                new_type
            ));
        }
        if let Some(ref new_address) = request.address
            && Some(new_address) != old_device.address.as_ref()
        {
            changes.push(format!(
                "Address: {} → {}",
                old_device.address.as_deref().unwrap_or("N/A"),
                new_address
            ));
        }
        if !changes.is_empty() {
            let mut elements =
                vec![self.create_text_element(format!(
                    "Device '{}' has been updated",
                    updated_device.name
                ))];
            for change in changes {
                elements.push(self.create_text_element(change));
            }
            let _ = self
                .publish_device_event(
                    DeviceEventType::DeviceUpdated,
                    EventLevel::Info,
                    updated_device,
                    format!("Device Updated: {}", updated_device.name),
                    elements,
                )
                .await;
        }
    }

    pub async fn delete_device(&self, device_id: &str) -> Result<bool, Error> {
        tracing::info!("Deleting device: {}", device_id);
        let device = self.repository.find_by_id(device_id).await?.ok_or(Error::NotFound)?;

        // Cascade: delete all sub-devices linked to this gateway
        if let Ok(sub_devices) = self.repository.find_by_linked_gateway(device_id).await {
            let sub_ids: Vec<String> = sub_devices.iter().map(|d| d.id.clone()).collect();
            if !sub_ids.is_empty() {
                tracing::info!(
                    gateway_id = %device_id,
                    sub_device_count = sub_ids.len(),
                    "Cascade deleting sub-devices"
                );
                self.repository.delete_by_ids(&sub_ids).await?;
            }
        }

        let deleted_count = self.repository.delete(device_id).await?;
        let success = deleted_count > 0;
        if success {
            self.publish_device_deleted_event(&device).await;
            tracing::info!("Device {} deleted successfully", device_id);
        }
        Ok(success)
    }

    async fn publish_device_deleted_event(&self, device: &Device) {
        let content_elements = self.build_device_info_elements(
            device,
            format!("Device '{}' has been deleted", device.name),
        );
        let _ = self
            .publish_device_event(
                DeviceEventType::DeviceDeleted,
                EventLevel::Warning,
                device,
                format!("Device Deleted: {}", device.name),
                content_elements,
            )
            .await;
    }

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
        let _ = self
            .publish_device_event(
                DeviceEventType::CommandStarted,
                EventLevel::Info,
                device,
                format!("Command: {} ({})", command_name, command_type),
                content_elements,
            )
            .await;
    }

    pub async fn update_device_state(&self, device_id: &str, new_state: i32) -> Result<(), Error> {
        let device = self.repository.find_by_id(device_id).await?.ok_or(Error::NotFound)?;
        let old_state: i32 = device.status.into();
        if old_state != new_state {
            self.repository.update_state(device_id, new_state).await?;
            if let Ok(Some(updated_device)) = self.repository.find_by_id(device_id).await {
                self.publish_device_state_updated_event(&updated_device, old_state, new_state)
                    .await;
            }
        }
        Ok(())
    }

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
        let _ = self
            .publish_device_event(
                DeviceEventType::DeviceUpdated,
                EventLevel::Info,
                device,
                format!("Device State Updated: {}", device.name),
                content_elements,
            )
            .await;
    }

    pub async fn get_device_by_id(&self, device_id: &str) -> Result<Option<Device>, Error> {
        self.repository.find_by_id(device_id).await.map_err(|e| self.io_error(e))
    }

    pub async fn get_device_by_id_with_tags(
        &self,
        device_id: &str,
    ) -> Result<Option<Device>, Error> {
        self.repository.find_by_id(device_id).await.map_err(|e| self.io_error(e))
    }

    pub async fn get_device_by_name(&self, name: &str) -> Result<Option<Device>, Error> {
        self.repository.find_by_name(name).await.map_err(|e| self.io_error(e))
    }

    pub async fn get_devices(&self, params: &DeviceQueryParams) -> Result<Vec<Device>, Error> {
        let criteria = params_to_criteria(params);
        self.repository.find_all(&criteria).await.map_err(|e| self.io_error(e))
    }

    pub async fn get_devices_with_tags(
        &self,
        params: &DeviceQueryParams,
        tenant_id: &str,
    ) -> Result<Vec<Device>, Error> {
        let mut devices = self.get_devices(params).await?;
        if let Some(tag_repo) = &self.tag_repository {
            for device in &mut devices {
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

    pub async fn count_devices(&self, params: &DeviceQueryParams) -> Result<i64, Error> {
        let criteria = params_to_criteria(params);
        self.repository.count(&criteria).await.map_err(|e| self.io_error(e))
    }

    pub async fn update_device_enabled_status(
        &self,
        device_id: &str,
        enabled: bool,
    ) -> Result<bool, Error> {
        self.repository
            .update_enabled_status(device_id, enabled)
            .await
            .map_err(|e| self.io_error(e))
    }

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

    pub async fn get_child_devices(&self, parent_id: &str) -> Result<Vec<Device>, Error> {
        self.repository.find_children(parent_id).await.map_err(|e| self.io_error(e))
    }

    pub async fn get_devices_by_product(&self, product_id: &str) -> Result<Vec<Device>, Error> {
        self.repository.find_by_product_id(product_id).await.map_err(|e| self.io_error(e))
    }

    pub async fn get_devices_by_driver(&self, driver_name: &str) -> Result<Vec<Device>, Error> {
        self.repository.find_by_driver_name(driver_name).await.map_err(|e| self.io_error(e))
    }

    pub async fn get_device_properties(
        &self,
        device_id: &str,
    ) -> Result<Vec<DeviceProperty>, Error> {
        let db = self.database.clone();
        find_device_properties_by_device_id(&db, device_id).await.map_err(|e| self.io_error(e))
    }

    pub async fn get_device_property_by_name(
        &self,
        device_id: &str,
        property_name: &str,
    ) -> Result<Option<DeviceProperty>, Error> {
        let properties = self.get_device_properties(device_id).await?;
        Ok(properties.into_iter().find(|p| p.name == property_name))
    }

    pub async fn get_device_properties_page(
        &self,
        device_id: &str,
        property_name: Option<String>,
        page_no: u32,
        page_size: u32,
    ) -> Result<DataObjectWithPagination<DeviceProperty>, Error> {
        let mut properties = self.get_device_properties(device_id).await?;
        if let Some(name) = property_name {
            properties.retain(|p| p.name.contains(&name));
        }
        Ok(DataObjectWithPagination::new(&properties, page_no, page_size))
    }

    pub async fn get_device_commands(&self, device_id: &str) -> Result<Vec<DeviceCommand>, Error> {
        let db = self.database.clone();
        find_device_commands_by_device_id(&db, device_id).await.map_err(|e| self.io_error(e))
    }

    pub async fn get_device_command_by_name(
        &self,
        device_id: &str,
        command_name: &str,
    ) -> Result<Option<DeviceCommand>, Error> {
        let commands = self.get_device_commands(device_id).await?;
        Ok(commands.into_iter().find(|c| c.name == command_name))
    }

    pub async fn create_devices_batch(
        &self,
        requests: &[CreateDeviceRequest],
    ) -> Result<Vec<Device>, Error> {
        for request in requests {
            if self.repository.exists_by_name(&request.name).await.unwrap_or(false) {
                return Err(Error::ValidationError(format!(
                    "{}: '{}'",
                    ERROR_DEVICE_NAME_EXISTS, request.name
                )));
            }
        }
        let created_devices = self.repository.create_batch(requests).await?;
        for device in &created_devices {
            self.publish_batch_device_created_event(device).await;
        }
        Ok(created_devices)
    }

    async fn publish_batch_device_created_event(&self, device: &Device) {
        let content_elements = self.build_device_info_elements(
            device,
            format!("Device '{}' created in batch operation", device.name),
        );
        let _ = self
            .publish_device_event(
                DeviceEventType::DeviceCreated,
                EventLevel::Info,
                device,
                format!("Device Created: {}", device.name),
                content_elements,
            )
            .await;
    }

    async fn publish_batch_device_deleted_event(&self, device: &Device) {
        let content_elements = self.build_device_info_elements(
            device,
            format!("Device '{}' deleted in batch operation", device.name),
        );
        let _ = self
            .publish_device_event(
                DeviceEventType::DeviceDeleted,
                EventLevel::Warning,
                device,
                format!("Device Deleted: {}", device.name),
                content_elements,
            )
            .await;
    }

    pub async fn delete_devices_batch(&self, device_ids: &[String]) -> Result<u64, Error> {
        let devices = self.repository.find_by_ids(device_ids).await?;
        let deleted_count = self.repository.delete_by_ids(device_ids).await?;
        for device in &devices {
            self.publish_batch_device_deleted_event(device).await;
        }
        Ok(deleted_count)
    }

    pub async fn update_device_states_batch(
        &self,
        updates: &[(String, i32)],
    ) -> Result<u64, Error> {
        self.repository.update_states_batch(updates).await
    }

    pub async fn device_exists(&self, device_id: &str) -> Result<bool, Error> {
        match self.get_device_by_id(device_id).await? {
            Some(_) => Ok(true),
            None => Ok(false),
        }
    }

    pub async fn device_name_exists(&self, name: &str) -> Result<bool, Error> {
        self.repository.exists_by_name(name).await.map_err(|e| self.io_error(e))
    }

    fn validate_driver_configuration(&self, device: &Device) -> Option<String> {
        if device.driver_name.is_none() {
            Some(ERROR_DEVICE_DRIVER_NOT_CONFIGURED.to_string())
        } else {
            None
        }
    }

    fn validate_address_configuration(&self, device: &Device) -> Option<String> {
        if device.address.as_ref().is_none_or(|addr| addr.is_empty()) {
            Some(ERROR_DEVICE_ADDRESS_NOT_CONFIGURED.to_string())
        } else {
            None
        }
    }

    pub async fn validate_device(&self, device_id: &str) -> Result<Vec<String>, Error> {
        let device = self.get_device_by_id(device_id).await?.ok_or(Error::NotFound)?;
        let mut errors = Vec::new();
        if let Err(e) = device.validate() {
            errors.push(e);
        }
        if let Some(error) = self.validate_driver_configuration(&device) {
            errors.push(error);
        }
        if let Some(error) = self.validate_address_configuration(&device) {
            errors.push(error);
        }
        Ok(errors)
    }

    async fn load_properties_with_fallback(&self, device_id: &str) -> Option<Vec<DeviceProperty>> {
        match self.get_device_properties(device_id).await {
            Ok(properties) => Some(properties),
            Err(e) => {
                tracing::warn!("Failed to load properties for device {}: {}", device_id, e);
                Some(Vec::new())
            }
        }
    }

    async fn load_commands_with_fallback(&self, device_id: &str) -> Option<Vec<DeviceCommand>> {
        match self.get_device_commands(device_id).await {
            Ok(commands) => Some(commands),
            Err(e) => {
                tracing::warn!("Failed to load commands for device {}: {}", device_id, e);
                Some(Vec::new())
            }
        }
    }

    pub async fn load_complete_device(&self, device_id: &str) -> Result<Option<Device>, Error> {
        let mut device = match self.repository.find_by_id(device_id).await? {
            Some(device) => device,
            None => return Ok(None),
        };
        device.properties = self.load_properties_with_fallback(device_id).await;
        device.commands = self.load_commands_with_fallback(device_id).await;
        device.status = tinyiothub_core::models::device::DeviceStatus::Offline;
        device.last_heartbeat = None;
        Ok(Some(device))
    }

    pub async fn load_complete_devices(&self, device_ids: &[String]) -> Result<Vec<Device>, Error> {
        let mut devices = Vec::new();
        for device_id in device_ids {
            if let Some(device) = self.load_complete_device(device_id).await? {
                devices.push(device);
            }
        }
        Ok(devices)
    }

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

    pub async fn send_command(
        &self,
        device_id: &str,
        command_name: &str,
        command_type: &str,
        params: Option<String>,
    ) -> Result<String, Error> {
        let device = self.repository.find_by_id(device_id).await?.ok_or(Error::NotFound)?;
        let command_id = uuid::Uuid::new_v4().to_string();
        let create_request =
            self.build_device_command_request(device_id, command_name, command_type, params);
        let db = self.database.clone();
        let _ = create_device_command(&db, &create_request).await;
        self.publish_command_started_event(&device, command_name, command_type, &command_id).await;
        Ok(command_id)
    }
}

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
        workspace_id: None,
        search_text: None,
        tag_name: None,
        sort_by: tinyiothub_storage::traits::device::DeviceSortBy::CreatedAt,
        sort_order: tinyiothub_storage::traits::device::DeviceSortOrder::Descending,
        limit: params.page_size,
        offset: params.page.map(|p| p.saturating_sub(1) * params.page_size.unwrap_or(0)),
    }
}
