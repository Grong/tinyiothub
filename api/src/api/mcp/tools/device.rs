// Device Tools Module
// MCP tools for device management

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::mcp::tool_registry::{InputSchema, PropertySchema, ToolError, ToolHandler};
use crate::dto::entity::device::{CreateDeviceRequest, Device, DeviceQueryParams, UpdateDeviceRequest};
use crate::dto::entity::device_command::DeviceCommand;
use crate::dto::entity::device_property::DeviceProperty;
use crate::domain::device::monitoring_service::DeviceMetrics;
use crate::domain::device::performance_service::DevicePerformanceMetrics;

/// Tool input: List devices with pagination and filtering
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListDevicesInput {
    page: Option<u32>,
    page_size: Option<u32>,
    name: Option<String>,
    device_type: Option<String>,
    driver_name: Option<String>,
    state: Option<i32>,
    product_id: Option<String>,
    enabled: Option<bool>,
}

/// Tool input: Get single device
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetDeviceInput {
    id: String,
    include_properties: Option<bool>,
}

/// Tool input: Get device status
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetDeviceStatusInput {
    id: String,
}

/// Tool input: Read device properties
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReadPropertiesInput {
    device_id: String,
    property_names: Option<Vec<String>>, // Optional: if not provided, read all properties
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

/// Tool input: Create device
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateDeviceInput {
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
}

/// Tool input: Update device
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateDeviceInput {
    id: String,
    name: Option<String>,
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
}

/// Tool input: Delete device
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteDeviceInput {
    id: String,
}

/// Tool input: Get device history
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetDeviceHistoryInput {
    device_id: String,
    hours: Option<u32>, // History window in hours (default: 168 = 7 days)
}

/// Tool input: Get device metrics
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetDeviceMetricsInput {
    device_id: String,
}

/// Tool input: Export device report
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportDeviceReportInput {
    device_id: String,
    format: Option<String>, // "json" (default) or "summary"
}

/// Device status response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceStatusResponse {
    device_id: String,
    name: String,
    is_online: bool,
    state: Option<i32>,
    state_description: String,
    last_heartbeat: Option<String>,
    signal_strength: Option<i32>, // Placeholder, would come from device data
}

/// Property read response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PropertyReadResponse {
    device_id: String,
    properties: Vec<PropertyValue>,
}

/// Property value
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PropertyValue {
    name: String,
    value: Option<String>,
    unit: Option<String>,
    timestamp: Option<String>,
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

/// Device history response using performance metrics
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceHistoryResponse {
    device_id: String,
    hours: u32,
    records: Vec<DevicePerformanceMetrics>,
    total_count: usize,
}

/// Device metrics response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceMetricsResponse {
    device_id: String,
    total_properties: u32,
    online_properties: u32,
    offline_properties: u32,
    total_commands: u32,
    total_events: u32,
    active_alarms: u32,
    generated_at: String,
}

/// Device report response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceReportResponse {
    device: Device,
    properties: Vec<DeviceProperty>,
    commands: Vec<DeviceCommand>,
    status: DeviceStatusResponse,
    generated_at: String,
    report_type: String,
}

// === List Devices Handler ===
pub struct ListDevicesHandler;

#[async_trait]
impl ToolHandler for ListDevicesHandler {
    fn name(&self) -> &str {
        "list_devices"
    }

    fn description(&self) -> &str {
        "List devices with pagination and filtering support"
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert("page".to_string(), PropertySchema { prop_type: "integer".to_string(), description: Some("Page number (default: 1)".to_string()) });
        props.insert("pageSize".to_string(), PropertySchema { prop_type: "integer".to_string(), description: Some("Page size (default: 20, max: 100)".to_string()) });
        props.insert("name".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Filter by device name (partial match)".to_string()) });
        props.insert("deviceType".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Filter by device type".to_string()) });
        props.insert("driverName".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Filter by driver name".to_string()) });
        props.insert("state".to_string(), PropertySchema { prop_type: "integer".to_string(), description: Some("Filter by state (0=offline, 1=online, 2=alarm)".to_string()) });
        props.insert("productId".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Filter by product ID".to_string()) });
        props.insert("enabled".to_string(), PropertySchema { prop_type: "boolean".to_string(), description: Some("Filter by enabled status".to_string()) });
        InputSchema::object(vec![], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: ListDevicesInput = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let state = crate::api::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;

        let params = DeviceQueryParams {
            name: input.name,
            display_name: None,
            device_type: input.device_type,
            address: None,
            driver_name: input.driver_name,
            state: input.state,
            parent_id: None,
            product_id: input.product_id,
            page: input.page,
            page_size: input.page_size.or(Some(20)),
            tenant_id: None,
        };

        let devices = Device::find_all_with_tags(state.database(), &params)
            .await
            .map_err(|e| ToolError::Internal(e.to_string()))?;

        // Sync real-time state from DataContext
        let mut result = Vec::new();
        for mut device in devices {
            if let Some(cached) = state.data_context.get_device(&device.id) {
                device.state = cached.state;
                device.is_online = cached.is_online;
                device.last_heartbeat = cached.last_heartbeat;
            }
            result.push(device);
        }

        Ok(serde_json::to_value(result).unwrap())
    }
}

// === Get Device Handler ===
pub struct GetDeviceHandler;

#[async_trait]
impl ToolHandler for GetDeviceHandler {
    fn name(&self) -> &str {
        "get_device"
    }

    fn description(&self) -> &str {
        "Get detailed information about a single device"
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert("id".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Device ID (required)".to_string()) });
        props.insert("includeProperties".to_string(), PropertySchema { prop_type: "boolean".to_string(), description: Some("Include device properties (default: true)".to_string()) });
        InputSchema::object(vec!["id".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: GetDeviceInput = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let state = crate::api::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;

        let include_properties = input.include_properties.unwrap_or(true);

        let mut device = Device::find_by_id_with_tags(state.database(), &input.id)
            .await
            .map_err(|e| ToolError::Internal(e.to_string()))?
            .ok_or_else(|| ToolError::NotFound(format!("Device {} not found", input.id)))?;

        // Sync real-time state
        if let Some(cached) = state.data_context.get_device(&device.id) {
            device.state = cached.state;
            device.is_online = cached.is_online;
            device.last_heartbeat = cached.last_heartbeat;

            if include_properties {
                device.properties = cached.properties.clone();
                device.commands = cached.commands.clone();
            }
        }

        Ok(serde_json::to_value(device).unwrap())
    }
}

// === Get Device Status Handler ===
pub struct GetDeviceStatusHandler;

#[async_trait]
impl ToolHandler for GetDeviceStatusHandler {
    fn name(&self) -> &str {
        "get_device_status"
    }

    fn description(&self) -> &str {
        "Get device online/offline status and signal strength"
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert("id".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Device ID (required)".to_string()) });
        InputSchema::object(vec!["id".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: GetDeviceStatusInput = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let state = crate::api::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;

        let device = Device::find_by_id(state.database(), &input.id)
            .await
            .map_err(|e| ToolError::Internal(e.to_string()))?
            .ok_or_else(|| ToolError::NotFound(format!("Device {} not found", input.id)))?;

        let state_description = device.get_state_description().to_string();

        let (is_online, last_heartbeat) = if let Some(cached) = state.data_context.get_device(&device.id) {
            (cached.is_online, cached.last_heartbeat)
        } else {
            (device.is_online, device.last_heartbeat)
        };

        let status = DeviceStatusResponse {
            device_id: device.id.clone(),
            name: device.name.clone(),
            is_online,
            state: device.state,
            state_description,
            last_heartbeat,
            signal_strength: None, // Would need device data server to provide this
        };

        Ok(serde_json::to_value(status).unwrap())
    }
}

// === Read Properties Handler ===
pub struct ReadPropertiesHandler;

#[async_trait]
impl ToolHandler for ReadPropertiesHandler {
    fn name(&self) -> &str {
        "read_properties"
    }

    fn description(&self) -> &str {
        "Batch read device properties. If property_names is not provided, reads all properties."
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert("deviceId".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Device ID (required)".to_string()) });
        props.insert("propertyNames".to_string(), PropertySchema { prop_type: "array".to_string(), description: Some("List of property names to read. If not provided, reads all properties.".to_string()) });
        InputSchema::object(vec!["deviceId".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: ReadPropertiesInput = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let state = crate::api::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;

        // First check if device exists
        Device::find_by_id(state.database(), &input.device_id)
            .await
            .map_err(|e| ToolError::Internal(e.to_string()))?
            .ok_or_else(|| ToolError::NotFound(format!("Device {} not found", input.device_id)))?;

        // Get properties from DataContext (real-time values)
        let property_values = if let Some(cached) = state.data_context.get_device(&input.device_id) {
            cached.properties.unwrap_or_default()
        } else {
            // Fall back to database
            DeviceProperty::find_by_device_id(state.database(), &input.device_id)
                .await
                .map_err(|e| ToolError::Internal(e.to_string()))?
        };

        let properties: Vec<PropertyValue> = if let Some(names) = input.property_names {
            property_values
                .into_iter()
                .filter(|p| names.contains(&p.name))
                .map(|p| PropertyValue {
                    name: p.name.clone(),
                    value: p.current_value.or(p.default_value),
                    unit: p.unit,
                    timestamp: p.updated_at,
                })
                .collect()
        } else {
            property_values
                .into_iter()
                .map(|p| PropertyValue {
                    name: p.name.clone(),
                    value: p.current_value.or(p.default_value),
                    unit: p.unit,
                    timestamp: p.updated_at,
                })
                .collect()
        };

        let response = PropertyReadResponse {
            device_id: input.device_id,
            properties,
        };

        Ok(serde_json::to_value(response).unwrap())
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
        props.insert("deviceId".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Device ID (required)".to_string()) });
        props.insert("properties".to_string(), PropertySchema { prop_type: "object".to_string(), description: Some("Object mapping property names to values (required)".to_string()) });
        InputSchema::object(vec!["deviceId".to_string(), "properties".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: WritePropertiesInput = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let state = crate::api::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;

        // Check device exists
        Device::find_by_id(state.database(), &input.device_id)
            .await
            .map_err(|e| ToolError::Internal(e.to_string()))?
            .ok_or_else(|| ToolError::NotFound(format!("Device {} not found", input.device_id)))?;

        // Get device properties definition
        let device_properties = DeviceProperty::find_by_device_id(state.database(), &input.device_id)
            .await
            .map_err(|e| ToolError::Internal(e.to_string()))?;

        let mut updated_count = 0;
        let mut results = Vec::new();

        for (prop_name, value) in &input.properties {
            // Find property definition
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
                    // Validate value
                    if let Err(e) = def.validate_value(value) {
                        results.push(PropertyUpdateResult {
                            name: prop_name.clone(),
                            success: false,
                            message: Some(e),
                        });
                        continue;
                    }

                    // Update via DataContext
                    match state.data_context.update_device_property_value(
                        &input.device_id,
                        &def.id,
                        value,
                        Some(&state.event_bus),
                    ).await {
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

// === Send Command Handler ===
pub struct SendCommandHandler;

#[async_trait]
impl ToolHandler for SendCommandHandler {
    fn name(&self) -> &str {
        "send_command"
    }

    fn description(&self) -> &str {
        "Send a command to a device"
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert("deviceId".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Device ID (required)".to_string()) });
        props.insert("commandName".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Command name (required)".to_string()) });
        props.insert("parameters".to_string(), PropertySchema { prop_type: "object".to_string(), description: Some("Command parameters as key-value pairs".to_string()) });
        InputSchema::object(vec!["deviceId".to_string(), "commandName".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: SendCommandInput = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let state = crate::api::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;

        // Check device exists and is online
        let device = Device::find_by_id(state.database(), &input.device_id)
            .await
            .map_err(|e| ToolError::Internal(e.to_string()))?
            .ok_or_else(|| ToolError::NotFound(format!("Device {} not found", input.device_id)))?;

        // Check if device is online
        let is_online = state.data_context.get_device(&input.device_id)
            .map(|d| d.is_online)
            .unwrap_or(device.is_online);

        if !is_online {
            return Ok(serde_json::to_value(CommandResponse {
                device_id: input.device_id,
                command_name: input.command_name,
                success: false,
                message: Some("Device is offline".to_string()),
                execution_time: None,
            }).unwrap());
        }

        // Find command definition
        let command = DeviceCommand::find_by_device_and_name(
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
                }).unwrap());
            }
        };

        // Execute command via DataServer if available
        let start_time = std::time::Instant::now();

        // Create command with parameters
        let mut cmd = command_def.clone();
        if let Some(params) = input.parameters {
            cmd.parameters = Some(serde_json::to_string(&params).unwrap_or_default());
        }

        let result = if let Some(data_server) = state.data_server() {
            data_server.execute_command(cmd)
                .map_err(|e| ToolError::Internal(e.to_string()))
        } else {
            // No data server available - simulate success for testing
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
            }).unwrap()),
            Err(e) => Ok(serde_json::to_value(CommandResponse {
                device_id: input.device_id,
                command_name: input.command_name,
                success: false,
                message: Some(e.to_string()),
                execution_time: Some(execution_time),
            }).unwrap()),
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
        props.insert("name".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Device name (required)".to_string()) });
        props.insert("displayName".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Display name".to_string()) });
        props.insert("deviceType".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Device type".to_string()) });
        props.insert("address".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Device address".to_string()) });
        props.insert("description".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Device description".to_string()) });
        props.insert("position".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Device position/location".to_string()) });
        props.insert("driverName".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Driver name".to_string()) });
        props.insert("deviceModel".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Device model".to_string()) });
        props.insert("protocolType".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Protocol type (modbus, onvif, snmp, mqtt)".to_string()) });
        props.insert("factoryName".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Factory name".to_string()) });
        props.insert("linkedData".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Linked data".to_string()) });
        props.insert("connectionConfig".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Connection configuration (JSON)".to_string()) });
        props.insert("parentId".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Parent device ID".to_string()) });
        props.insert("productId".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Product ID".to_string()) });
        props.insert("organizationId".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Organization ID".to_string()) });
        InputSchema::object(vec!["name".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: CreateDeviceInput = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let state = crate::api::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;

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
            tenant_id: None,
        };

        let device_service = state.device_service.as_ref();

        match device_service.create_device(&request).await {
            Ok(device) => Ok(serde_json::to_value(device).unwrap()),
            Err(e) => Err(ToolError::Internal(format!("Failed to create device: {}", e))),
        }
    }
}

// === Update Device Handler ===
pub struct UpdateDeviceHandler;

#[async_trait]
impl ToolHandler for UpdateDeviceHandler {
    fn name(&self) -> &str {
        "update_device"
    }

    fn description(&self) -> &str {
        "Update device configuration"
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert("id".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Device ID (required)".to_string()) });
        props.insert("name".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Device name".to_string()) });
        props.insert("displayName".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Display name".to_string()) });
        props.insert("deviceType".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Device type".to_string()) });
        props.insert("address".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Device address".to_string()) });
        props.insert("description".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Device description".to_string()) });
        props.insert("position".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Device position/location".to_string()) });
        props.insert("driverName".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Driver name".to_string()) });
        props.insert("deviceModel".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Device model".to_string()) });
        props.insert("protocolType".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Protocol type".to_string()) });
        props.insert("factoryName".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Factory name".to_string()) });
        props.insert("linkedData".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Linked data".to_string()) });
        props.insert("connectionConfig".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Connection configuration".to_string()) });
        props.insert("parentId".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Parent device ID".to_string()) });
        props.insert("productId".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Product ID".to_string()) });
        props.insert("organizationId".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Organization ID".to_string()) });
        InputSchema::object(vec!["id".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: UpdateDeviceInput = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let state = crate::api::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;

        let request = UpdateDeviceRequest {
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
            state: None,
            parent_id: input.parent_id,
            product_id: input.product_id,
            tenant_id: None,
        };

        let device_service = state.device_service.as_ref();

        match device_service.update_device(&input.id, &request).await {
            Ok(device) => Ok(serde_json::to_value(device).unwrap()),
            Err(e) => Err(ToolError::Internal(format!("Failed to update device: {}", e))),
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
        props.insert("id".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Device ID (required)".to_string()) });
        InputSchema::object(vec!["id".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: DeleteDeviceInput = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let state = crate::api::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;

        let device_service = state.device_service.as_ref();

        match device_service.delete_device(&input.id).await {
            Ok(true) => Ok(serde_json::json!({"success": true, "device_id": input.id}).into()),
            Ok(false) => Err(ToolError::NotFound(format!("Device {} not found", input.id))),
            Err(e) => Err(ToolError::Internal(format!("Failed to delete device: {}", e))),
        }
    }
}

// === Get Device History Handler ===
pub struct GetDeviceHistoryHandler;

#[async_trait]
impl ToolHandler for GetDeviceHistoryHandler {
    fn name(&self) -> &str {
        "get_device_history"
    }

    fn description(&self) -> &str {
        "Query device performance history data. Returns time-series performance metrics."
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert("deviceId".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Device ID (required)".to_string()) });
        props.insert("hours".to_string(), PropertySchema { prop_type: "integer".to_string(), description: Some("History window in hours (default: 168 = 7 days)".to_string()) });
        InputSchema::object(vec!["deviceId".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: GetDeviceHistoryInput = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let state = crate::api::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;

        // Default to 7 days (168 hours)
        let hours = input.hours.unwrap_or(168);

        // Check device exists
        Device::find_by_id(state.database(), &input.device_id)
            .await
            .map_err(|e| ToolError::Internal(e.to_string()))?
            .ok_or_else(|| ToolError::NotFound(format!("Device {} not found", input.device_id)))?;

        // Get performance history from performance service
        let records = state.performance_service
            .get_device_performance_history(&input.device_id, hours)
            .await
            .map_err(|e| ToolError::Internal(format!("Failed to get history: {}", e)))?;

        let total_count = records.len();

        let response = DeviceHistoryResponse {
            device_id: input.device_id,
            hours,
            records,
            total_count,
        };

        Ok(serde_json::to_value(response).unwrap())
    }
}

// === Get Device Metrics Handler ===
pub struct GetDeviceMetricsHandler;

#[async_trait]
impl ToolHandler for GetDeviceMetricsHandler {
    fn name(&self) -> &str {
        "get_device_metrics"
    }

    fn description(&self) -> &str {
        "Get device metrics including property counts, command counts, events, and alarms."
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert("deviceId".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Device ID (required)".to_string()) });
        InputSchema::object(vec!["deviceId".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: GetDeviceMetricsInput = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let state = crate::api::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;

        // Check device exists
        Device::find_by_id(state.database(), &input.device_id)
            .await
            .map_err(|e| ToolError::Internal(e.to_string()))?
            .ok_or_else(|| ToolError::NotFound(format!("Device {} not found", input.device_id)))?;

        // Get metrics from monitoring service
        let metrics: DeviceMetrics = state.monitoring_service
            .get_device_metrics(&input.device_id)
            .await
            .unwrap_or_else(|| DeviceMetrics {
                total_properties: 0,
                online_properties: 0,
                offline_properties: 0,
                total_commands: 0,
                total_events: 0,
                active_alarms: 0,
            });

        let response = DeviceMetricsResponse {
            device_id: input.device_id,
            total_properties: metrics.total_properties,
            online_properties: metrics.online_properties,
            offline_properties: metrics.offline_properties,
            total_commands: metrics.total_commands,
            total_events: metrics.total_events,
            active_alarms: metrics.active_alarms,
            generated_at: chrono::Utc::now().to_rfc3339(),
        };

        Ok(serde_json::to_value(response).unwrap())
    }
}

// === Export Device Report Handler ===
pub struct ExportDeviceReportHandler;

#[async_trait]
impl ToolHandler for ExportDeviceReportHandler {
    fn name(&self) -> &str {
        "export_device_report"
    }

    fn description(&self) -> &str {
        "Generate a device operation report with full device details, properties, commands, and status"
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert("deviceId".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Device ID (required)".to_string()) });
        props.insert("format".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Report format: 'json' (default) or 'summary'".to_string()) });
        InputSchema::object(vec!["deviceId".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: ExportDeviceReportInput = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let state = crate::api::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;

        // Get device
        let mut device = Device::find_by_id_with_tags(state.database(), &input.device_id)
            .await
            .map_err(|e| ToolError::Internal(e.to_string()))?
            .ok_or_else(|| ToolError::NotFound(format!("Device {} not found", input.device_id)))?;

        // Get properties
        let properties = DeviceProperty::find_by_device_id(state.database(), &input.device_id)
            .await
            .map_err(|e| ToolError::Internal(e.to_string()))?;

        // Get commands
        let commands = DeviceCommand::find_by_device_id(state.database(), &input.device_id)
            .await
            .map_err(|e| ToolError::Internal(e.to_string()))?;

        // Sync real-time state
        if let Some(cached) = state.data_context.get_device(&device.id) {
            device.state = cached.state;
            device.is_online = cached.is_online;
            device.last_heartbeat = cached.last_heartbeat;
            device.properties = cached.properties;
            device.commands = cached.commands;
        }

        let status = DeviceStatusResponse {
            device_id: device.id.clone(),
            name: device.name.clone(),
            is_online: device.is_online,
            state: device.state,
            state_description: device.get_state_description().to_string(),
            last_heartbeat: device.last_heartbeat.clone(),
            signal_strength: None,
        };

        let report_type = input.format.as_deref().unwrap_or("json");

        let response = DeviceReportResponse {
            device,
            properties,
            commands,
            status,
            generated_at: chrono::Utc::now().to_rfc3339(),
            report_type: report_type.to_string(),
        };

        Ok(serde_json::to_value(response).unwrap())
    }
}