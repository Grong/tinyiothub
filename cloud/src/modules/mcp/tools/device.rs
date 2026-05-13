// Device Tools Module
// MCP tools for device management

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tinyiothub_core::models::device::CreateDeviceRequest;
use tinyiothub_storage::traits::device::{DeviceCriteria, DeviceSortBy, DeviceSortOrder};

use crate::{
    modules::{
        mcp::tool_registry::{InputSchema, PropertySchema, ToolError, ToolHandler},
        template::types::{CreateDeviceFromTemplateRequest, DeviceCreationInput},
    },
    shared::persistence::repositories::{
        find_device_by_id, find_device_by_id_with_tags, find_device_command_by_device_and_name,
        find_device_properties_by_device_id,
    },
};

/// Tool input: Get single device
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetDeviceInput {
    id: String,
    include_properties: Option<bool>,
}

/// Tool input: Write device properties
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WritePropertiesInput {
    device_id: String,
    properties: HashMap<String, String>, // property_name -> value
}

/// Tool input: Send command to device
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SendCommandInput {
    device_id: String,
    command_name: String,
    parameters: Option<HashMap<String, String>>,
}

/// Tool input: Create device from template
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateDeviceInput {
    template_id: Option<String>,
    name: String,
    display_name: Option<String>,
    device_type: Option<String>,
    address: Option<String>,
    description: Option<String>,
    position: Option<String>,
    driver_name: Option<String>,
    device_model: Option<String>,
    protocol_type: Option<String>,
    factory_name: Option<String>,
    linked_data: Option<String>,
    connection_config: Option<String>,
    parent_id: Option<String>,
    product_id: Option<String>,
    property_values: Option<HashMap<String, String>>,
    enabled_commands: Option<Vec<String>>,
}

/// Tool input: Delete device
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteDeviceInput {
    id: String,
}

/// Property write response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PropertyWriteResponse {
    device_id: String,
    updated_count: usize,
    properties: Vec<PropertyUpdateResult>,
}

/// Property update result
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PropertyUpdateResult {
    name: String,
    success: bool,
    message: Option<String>,
}

/// Command response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CommandResponse {
    device_id: String,
    command_name: String,
    success: bool,
    message: Option<String>,
    execution_time: Option<String>,
}

// === Get Device Profile Handler ===
pub struct DeviceProfileHandler;

#[async_trait]
impl ToolHandler for DeviceProfileHandler {
    fn name(&self) -> &str {
        "get_device"
    }

    fn description(&self) -> &str {
        "Get detailed information about a single device, including its property definitions, current values, status, and metrics"
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert(
            "id".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Device ID (required)".to_string()),
            },
        );
        props.insert(
            "includeProperties".to_string(),
            PropertySchema {
                prop_type: "boolean".to_string(),
                description: Some("Include device properties (default: true)".to_string()),
            },
        );
        InputSchema::object(vec!["id".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: GetDeviceInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let state = crate::modules::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;

        let include_properties = input.include_properties.unwrap_or(true);

        let _workspace_id = crate::modules::mcp::handlers::get_mcp_context()
            .ok_or_else(|| ToolError::Unauthorized("MCP context not initialized".to_string()))?
            .workspace_id;

        let mut device = find_device_by_id_with_tags(state.database(), &input.id, "")
            .await
            .map_err(|e| ToolError::Internal(e.to_string()))?
            .ok_or_else(|| ToolError::NotFound(format!("Device {} not found", input.id)))?;

        // Sync real-time state
        if let Some(cached) = state.device_cache.get(&device.id) {
            device.status = cached.status.clone();
            device.last_heartbeat = cached.last_heartbeat.clone();

            if include_properties {
                device.properties = cached.properties.clone();
                device.commands = cached.commands.clone();
            }
        }

        Ok(serde_json::to_value(device).unwrap())
    }
}

// === Device Property Get Handler ===
pub struct DevicePropertyGetHandler;

#[async_trait]
impl ToolHandler for DevicePropertyGetHandler {
    fn name(&self) -> &str {
        "read_properties"
    }

    fn description(&self) -> &str {
        "Get the definition and current value of a specific property on a device"
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert(
            "deviceId".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Device ID (required)".to_string()),
            },
        );
        props.insert(
            "propertyName".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Property name (required)".to_string()),
            },
        );
        InputSchema::object(vec!["deviceId".to_string(), "propertyName".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Input {
            device_id: String,
            property_name: String,
        }

        let input: Input =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let state = crate::modules::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;

        let _workspace_id = crate::modules::mcp::handlers::get_mcp_context()
            .ok_or_else(|| ToolError::Unauthorized("MCP context not initialized".to_string()))?
            .workspace_id;

        let _device = find_device_by_id(state.database(), &input.device_id)
            .await
            .map_err(|e| ToolError::Internal(e.to_string()))?
            .ok_or_else(|| ToolError::NotFound(format!("Device {} not found", input.device_id)))?;

        let all_properties =
            find_device_properties_by_device_id(state.database(), &input.device_id)
                .await
                .map_err(|e| ToolError::Internal(e.to_string()))?;

        let prop =
            all_properties.iter().find(|p| p.name == input.property_name).ok_or_else(|| {
                ToolError::NotFound(format!(
                    "Property '{}' not found on device {}",
                    input.property_name, input.device_id
                ))
            })?;

        let current_value = state
            .device_cache
            .get(&input.device_id)
            .and_then(|d| d.properties)
            .and_then(|props| props.into_iter().find(|p| p.name == input.property_name))
            .and_then(|p| p.current_value);

        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct PropertyDetailResponse {
            device_id: String,
            property_name: String,
            display_name: Option<String>,
            description: Option<String>,
            data_type: Option<String>,
            unit: Option<String>,
            min_value: Option<f64>,
            max_value: Option<f64>,
            default_value: Option<String>,
            is_read_only: bool,
            current_value: Option<String>,
        }

        Ok(serde_json::to_value(PropertyDetailResponse {
            device_id: input.device_id,
            property_name: prop.name.clone(),
            display_name: prop.display_name.clone(),
            description: prop.description.clone(),
            data_type: prop.data_type.clone(),
            unit: prop.unit.clone(),
            min_value: prop.min_value,
            max_value: prop.max_value,
            default_value: prop.default_value.clone(),
            is_read_only: prop.is_read_only == 1,
            current_value,
        })
        .unwrap())
    }
}

// === Write Properties Handler ===
pub struct WritePropertiesHandler;

#[async_trait]
impl ToolHandler for WritePropertiesHandler {
    fn name(&self) -> &str {
        "write_properties"
    }

    fn description(&self) -> &str {
        "Batch write device properties. Only writable properties can be updated."
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert(
            "deviceId".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Device ID (required)".to_string()),
            },
        );
        props.insert(
            "properties".to_string(),
            PropertySchema {
                prop_type: "object".to_string(),
                description: Some("Object mapping property names to values (required)".to_string()),
            },
        );
        InputSchema::object(vec!["deviceId".to_string(), "properties".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: WritePropertiesInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let state = crate::modules::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;

        let workspace_id = crate::modules::mcp::handlers::get_mcp_context()
            .ok_or_else(|| ToolError::Unauthorized("MCP context not initialized".to_string()))?
            .workspace_id;

        let _device = find_device_by_id(state.database(), &input.device_id)
            .await
            .map_err(|e| ToolError::Internal(e.to_string()))?
            .ok_or_else(|| ToolError::NotFound(format!("Device {} not found", input.device_id)))?;

        let device_properties =
            find_device_properties_by_device_id(state.database(), &input.device_id)
                .await
                .map_err(|e| ToolError::Internal(e.to_string()))?;

        let mut updated_count = 0;
        let mut results = Vec::new();

        for (prop_name, value) in &input.properties {
            let prop_def = device_properties.iter().find(|p| &p.name == prop_name);

            match prop_def {
                Some(def) if def.is_read_only == 1 => {
                    results.push(PropertyUpdateResult {
                        name: prop_name.clone(),
                        success: false,
                        message: Some("Property is read-only".to_string()),
                    });
                }
                Some(def) => {
                    if let Err(e) = def.validate_value(value) {
                        results.push(PropertyUpdateResult {
                            name: prop_name.clone(),
                            success: false,
                            message: Some(e),
                        });
                        continue;
                    }

                    match state
                        .update_device_property_value(
                            &workspace_id,
                            &input.device_id,
                            &def.id,
                            value,
                        )
                        .await
                    {
                        Ok(_) => {
                            updated_count += 1;
                            results.push(PropertyUpdateResult {
                                name: prop_name.clone(),
                                success: true,
                                message: None,
                            });
                        }
                        Err(e) => {
                            results.push(PropertyUpdateResult {
                                name: prop_name.clone(),
                                success: false,
                                message: Some(format!("Update failed: {}", e)),
                            });
                        }
                    }
                }
                None => {
                    results.push(PropertyUpdateResult {
                        name: prop_name.clone(),
                        success: false,
                        message: Some("Property not found".to_string()),
                    });
                }
            }
        }

        let response = PropertyWriteResponse {
            device_id: input.device_id,
            updated_count,
            properties: results,
        };

        Ok(serde_json::to_value(response).unwrap())
    }
}

// === Device Command Handler ===
pub struct DeviceCommandHandler;

#[async_trait]
impl ToolHandler for DeviceCommandHandler {
    fn name(&self) -> &str {
        "send_command"
    }

    fn description(&self) -> &str {
        "Send a command to a device"
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert(
            "deviceId".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Device ID (required)".to_string()),
            },
        );
        props.insert(
            "commandName".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Command name (required)".to_string()),
            },
        );
        props.insert(
            "parameters".to_string(),
            PropertySchema {
                prop_type: "object".to_string(),
                description: Some("Command parameters as key-value pairs".to_string()),
            },
        );
        InputSchema::object(vec!["deviceId".to_string(), "commandName".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: SendCommandInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let state = crate::modules::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;

        let _workspace_id = crate::modules::mcp::handlers::get_mcp_context()
            .ok_or_else(|| ToolError::Unauthorized("MCP context not initialized".to_string()))?
            .workspace_id;

        let device = find_device_by_id(state.database(), &input.device_id)
            .await
            .map_err(|e| ToolError::Internal(e.to_string()))?
            .ok_or_else(|| ToolError::NotFound(format!("Device {} not found", input.device_id)))?;

        let is_online = state
            .device_cache
            .get(&input.device_id)
            .map(|d| d.is_online())
            .unwrap_or(device.is_online());

        if !is_online {
            return Ok(serde_json::to_value(CommandResponse {
                device_id: input.device_id,
                command_name: input.command_name,
                success: false,
                message: Some("Device is offline".to_string()),
                execution_time: None,
            })
            .unwrap());
        }

        let command = find_device_command_by_device_and_name(
            state.database(),
            &input.device_id,
            &input.command_name,
        )
        .await
        .map_err(|e| ToolError::Internal(e.to_string()))?;

        let command_def = match command {
            Some(c) => c,
            None => {
                return Ok(serde_json::to_value(CommandResponse {
                    device_id: input.device_id.clone(),
                    command_name: input.command_name.clone(),
                    success: false,
                    message: Some(format!("Command '{}' not found on device", input.command_name)),
                    execution_time: None,
                })
                .unwrap());
            }
        };

        let start_time = std::time::Instant::now();

        let mut cmd = command_def.clone();
        if let Some(params) = input.parameters {
            cmd.parameters = Some(serde_json::to_string(&params).unwrap_or_default());
        }

        let result = if let Some(data_server) = state.data_server() {
            data_server.execute_command(cmd).map_err(|e| ToolError::Internal(e.to_string()))
        } else {
            tracing::warn!("DataServer not available, command execution simulated");
            Ok(())
        };

        let execution_time = format!("{:?}", start_time.elapsed());

        match result {
            Ok(_) => Ok(serde_json::to_value(CommandResponse {
                device_id: input.device_id,
                command_name: input.command_name,
                success: true,
                message: None,
                execution_time: Some(execution_time),
            })
            .unwrap()),
            Err(e) => Ok(serde_json::to_value(CommandResponse {
                device_id: input.device_id,
                command_name: input.command_name,
                success: false,
                message: Some(e.to_string()),
                execution_time: Some(execution_time),
            })
            .unwrap()),
        }
    }
}

// === Create Device Handler ===
pub struct CreateDeviceHandler;

#[async_trait]
impl ToolHandler for CreateDeviceHandler {
    fn name(&self) -> &str {
        "create_device"
    }

    fn description(&self) -> &str {
        "Create a new device from structured input"
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert(
            "templateId".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some(
                    "Device template ID (required for template-based creation)".to_string(),
                ),
            },
        );
        props.insert(
            "name".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Device name (required)".to_string()),
            },
        );
        props.insert(
            "displayName".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Display name".to_string()),
            },
        );
        props.insert(
            "address".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Device address".to_string()),
            },
        );
        props.insert(
            "description".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Device description".to_string()),
            },
        );
        props.insert(
            "position".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Device position/location".to_string()),
            },
        );
        props.insert(
            "propertyValues".to_string(),
            PropertySchema {
                prop_type: "object".to_string(),
                description: Some(
                    "Property values to set at creation (property name -> value)".to_string(),
                ),
            },
        );
        props.insert(
            "enabledCommands".to_string(),
            PropertySchema {
                prop_type: "array".to_string(),
                description: Some("Commands to enable at creation".to_string()),
            },
        );
        InputSchema::object(vec!["name".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: CreateDeviceInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let state = crate::modules::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;

        let workspace_id = crate::modules::mcp::handlers::get_mcp_context()
            .map(|c| c.workspace_id)
            .ok_or_else(|| ToolError::Unauthorized("MCP context not available".to_string()))?;

        let tenant_device_service = state.tenant_device_service_str(&workspace_id);

        if let Some(template_id) = &input.template_id {
            let device_input = DeviceCreationInput {
                name: input.name,
                display_name: input.display_name,
                description: input.description,
                position: input.position,
                address: input.address,
                driver_name: input.driver_name,
                driver_options: input.connection_config,
                parent_id: input.parent_id,
                product_id: input.product_id,
                property_values: input.property_values.unwrap_or_default(),
                enabled_commands: input.enabled_commands.unwrap_or_default(),
                tenant_id: None,
                workspace_id: None,
            };
            let request =
                CreateDeviceFromTemplateRequest { template_id: template_id.clone(), device_input };
            match tenant_device_service
                .create_device_from_template(
                    state.template_engine(),
                    &request.template_id,
                    &request.device_input,
                )
                .await
            {
                Ok(device) => Ok(serde_json::to_value(device).unwrap()),
                Err(e) => Err(ToolError::Internal(format!(
                    "Failed to create device from template: {}",
                    e
                ))),
            }
        } else {
            let request = CreateDeviceRequest {
                name: input.name,
                display_name: input.display_name,
                device_type: input.device_type,
                address: input.address,
                description: input.description,
                position: input.position,
                driver_name: input.driver_name,
                device_model: input.device_model,
                protocol_type: input.protocol_type,
                factory_name: input.factory_name,
                linked_data: input.linked_data,
                driver_options: input.connection_config,
                parent_id: input.parent_id,
                product_id: input.product_id,
                linked_gateway: None,
                fingerprint: None,
            };
            match tenant_device_service.create_device(&request).await {
                Ok(device) => Ok(serde_json::to_value(device).unwrap()),
                Err(e) => Err(ToolError::Internal(format!("Failed to create device: {}", e))),
            }
        }
    }
}

// === Delete Device Handler ===
pub struct DeleteDeviceHandler;

#[async_trait]
impl ToolHandler for DeleteDeviceHandler {
    fn name(&self) -> &str {
        "delete_device"
    }

    fn description(&self) -> &str {
        "Delete a device"
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert(
            "id".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Device ID (required)".to_string()),
            },
        );
        InputSchema::object(vec!["id".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: DeleteDeviceInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let state = crate::modules::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;

        let workspace_id = crate::modules::mcp::handlers::get_mcp_context()
            .map(|c| c.workspace_id)
            .ok_or_else(|| ToolError::Unauthorized("MCP context not available".to_string()))?;

        let tenant_device_service = state.tenant_device_service_str(&workspace_id);

        match tenant_device_service.delete_device(&input.id).await {
            Ok(true) => Ok(serde_json::json!({"success": true, "device_id": input.id})),
            Ok(false) => Err(ToolError::NotFound(format!(
                "Device {} not found or does not belong to workspace",
                input.id
            ))),
            Err(e) => Err(ToolError::Internal(format!("Failed to delete device: {}", e))),
        }
    }
}

// === Search Devices Handler ===

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchDevicesInput {
    keyword: String,
    tag: Option<String>,
    limit: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchDeviceResult {
    id: String,
    name: String,
    display_name: Option<String>,
    device_type: Option<String>,
    status: String,
    driver_name: Option<String>,
    address: Option<String>,
    last_heartbeat: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchDevicesResponse {
    keyword: String,
    total: usize,
    devices: Vec<SearchDeviceResult>,
}

pub struct SearchDevicesHandler;

#[async_trait]
impl ToolHandler for SearchDevicesHandler {
    fn name(&self) -> &str {
        "search_devices"
    }

    fn description(&self) -> &str {
        "Search devices by keyword across name, display name, address, and description. Returns a concise list of matching devices."
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert(
            "keyword".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some(
                    "Search keyword (partial match on name, display name, address, description)"
                        .to_string(),
                ),
            },
        );
        props.insert(
            "tag".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Filter by tag name (partial match)".to_string()),
            },
        );
        props.insert(
            "limit".to_string(),
            PropertySchema {
                prop_type: "integer".to_string(),
                description: Some("Max results to return (default: 20, max: 50)".to_string()),
            },
        );
        InputSchema::object(vec!["keyword".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: SearchDevicesInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        if input.keyword.trim().is_empty() {
            return Err(ToolError::InvalidParams("keyword cannot be empty".to_string()));
        }

        let limit = input.limit.unwrap_or(20).min(50);

        let state = crate::modules::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;

        let workspace_id = crate::modules::mcp::handlers::get_mcp_context()
            .ok_or_else(|| ToolError::Unauthorized("MCP context not initialized".to_string()))?
            .workspace_id;

        let repository = state.device_repository_factory.create_for_workspace(workspace_id.clone());

        let criteria = DeviceCriteria {
            workspace_id: Some(workspace_id),
            search_text: Some(input.keyword.clone()),
            tag_name: input.tag,
            limit: Some(limit),
            sort_by: DeviceSortBy::Name,
            sort_order: DeviceSortOrder::Ascending,
            ..Default::default()
        };

        let mut devices = repository
            .find_all(&criteria)
            .await
            .map_err(|e| ToolError::Internal(format!("Search failed: {}", e)))?;

        let mut results = Vec::with_capacity(devices.len());
        for device in &mut devices {
            if let Some(cached) = state.device_cache.get(&device.id) {
                device.status = cached.status.clone();
                device.last_heartbeat = cached.last_heartbeat.clone();
            }
            results.push(SearchDeviceResult {
                id: device.id.clone(),
                name: device.name.clone(),
                display_name: device.display_name.clone(),
                device_type: device.device_type.clone(),
                status: device.status.to_string(),
                driver_name: device.driver_name.clone(),
                address: device.address.clone(),
                last_heartbeat: device.last_heartbeat.clone(),
            });
        }

        let response = SearchDevicesResponse {
            keyword: input.keyword,
            total: results.len(),
            devices: results,
        };

        Ok(serde_json::to_value(response).unwrap())
    }
}
