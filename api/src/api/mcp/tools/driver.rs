// Driver Tools Module
// MCP tools for driver management

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::mcp::tool_registry::{InputSchema, PropertySchema, ToolError, ToolHandler};
use crate::domain::device::driver;
use crate::dto::entity::component::ComponentOption;

/// Driver list response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DriverListResponse {
    drivers: Vec<DriverInfo>,
    total: usize,
}

/// Driver information
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DriverInfo {
    name: String,
    version: String,
    description: Option<String>,
    is_dynamic: bool,
    path: Option<String>,
    category: Option<String>,
}

/// Driver config schema response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DriverConfigSchemaResponse {
    driver_name: String,
    config_options: Vec<ConfigOption>,
    default_config: HashMap<String, String>,
}

/// Config option for driver
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfigOption {
    label: String,
    name: String,
    default_value: String,
    option_type: String,
    required: bool,
    description: Option<String>,
}

/// Match driver request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MatchDriverInput {
    manufacturer: Option<String>, // Factory name / brand
    model: Option<String>,        // Device model
    protocol: Option<String>,     // Protocol type (Modbus, SNMP, ONVIF, MQTT)
    device_type: Option<String>, // Optional device type
}

/// Match driver response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MatchDriverResponse {
    matched_driver: Option<String>,
    confidence: f32,        // 0.0 to 1.0
    match_reason: String,
    available_drivers: Vec<String>,
}

/// Load driver request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoadDriverInput {
    path: String, // Path to the dynamic driver library (.so, .dll, .dylib)
}

/// Load driver response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LoadDriverResponse {
    driver_name: String,
    success: bool,
    message: String,
}

/// Unload driver request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UnloadDriverInput {
    name: String, // Driver name to unload
}

/// Unload driver response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UnloadDriverResponse {
    name: String,
    success: bool,
    message: String,
}

/// Test driver request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TestDriverInput {
    driver_name: String,
    address: Option<String>,    // Device address (e.g., /dev/ttyS0, 192.168.1.100)
    connection_config: Option<String>, // JSON string with connection parameters
}

/// Test driver response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TestDriverResponse {
    driver_name: String,
    success: bool,
    message: String,
    test_data: Option<Vec<TestDataPoint>>,
    execution_time_ms: u64,
}

/// Test data point
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TestDataPoint {
    property_name: String,
    value: Option<String>,
    value_type: String,
}

// === List Drivers Handler ===
pub struct ListDriversHandler;

#[async_trait]
impl ToolHandler for ListDriversHandler {
    fn name(&self) -> &str {
        "list_drivers"
    }

    fn description(&self) -> &str {
        "List all available drivers (static and dynamic) with their basic information"
    }

    fn input_schema(&self) -> InputSchema {
        InputSchema::object(Vec::new(), HashMap::new())
    }

    async fn execute(&self, _args: Value) -> Result<Value, ToolError> {
        let all_names = driver::get_all_driver_names();
        let registry = driver::dynamic::registry::get_global_registry();

        let mut drivers = Vec::new();
        for name in &all_names {
            let is_dynamic = !driver::is_driver_supported(name);
            let path = if is_dynamic { registry.get_driver_path(name) } else { None };

            drivers.push(DriverInfo {
                name: name.clone(),
                version: "1.0.0".to_string(),
                description: Some(format!("{} driver", name)),
                is_dynamic,
                path,
                category: Some("protocol".to_string()),
            });
        }

        let total = drivers.len();
        Ok(serde_json::to_value(DriverListResponse { drivers, total }).unwrap())
    }
}

// === Get Driver Config Schema Handler ===
pub struct GetDriverConfigSchemaHandler;

#[async_trait]
impl ToolHandler for GetDriverConfigSchemaHandler {
    fn name(&self) -> &str {
        "get_driver_config_schema"
    }

    fn description(&self) -> &str {
        "Get the configuration parameters schema for a specific driver"
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert(
            "driverName".to_string(),
            PropertySchema { prop_type: "string".to_string(), description: Some("Driver name (required)".to_string()) },
        );
        InputSchema::object(vec!["driverName".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let driver_name = args["driverName"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("driverName is required".to_string()))?;

        // Get driver list (static drivers)
        let static_drivers = driver::get_driver_list();

        // Try to find the driver in static list first
        let config_options: Vec<ComponentOption> = if let Some(driver_info) =
            static_drivers.iter().find(|d| d.name == driver_name)
        {
            serde_json::from_str(&driver_info.options_descriptors).unwrap_or_default()
        } else {
            // Check dynamic drivers
            let registry = driver::dynamic::registry::get_global_registry();
            if let Ok(info) = registry.get_dynamic_driver_info(driver_name) {
                serde_json::from_str(&info.options_descriptors).unwrap_or_default()
            } else {
                return Err(ToolError::NotFound(format!("Driver '{}' not found", driver_name)));
            }
        };

        let mut default_config = HashMap::new();
        let config_opts: Vec<ConfigOption> = config_options
            .into_iter()
            .map(|opt| {
                default_config.insert(opt.name.clone(), opt.default_value.clone());
                ConfigOption {
                    label: opt.label,
                    name: opt.name,
                    default_value: opt.default_value,
                    option_type: opt.option_type,
                    required: opt.required,
                    description: opt.description,
                }
            })
            .collect();

        Ok(serde_json::to_value(DriverConfigSchemaResponse {
            driver_name: driver_name.to_string(),
            config_options: config_opts,
            default_config,
        })
        .unwrap())
    }
}

// === Match Driver Handler ===
pub struct MatchDriverHandler;

#[async_trait]
impl ToolHandler for MatchDriverHandler {
    fn name(&self) -> &str {
        "match_driver"
    }

    fn description(&self) -> &str {
        "Automatically match the appropriate driver based on device brand/model/protocol"
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert("manufacturer".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Device manufacturer/brand name (optional)".to_string()) });
        props.insert("model".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Device model identifier (optional)".to_string()) });
        props.insert("protocol".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Protocol type: Modbus, SNMP, ONVIF, MQTT (optional)".to_string()) });
        props.insert("deviceType".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Device type classification (optional)".to_string()) });
        InputSchema::object(vec![], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: MatchDriverInput = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let all_drivers = driver::get_all_driver_names();
        let mut matched_driver: Option<String> = None;
        let mut confidence: f32 = 0.0;
        let mut match_reason = String::new();

        // Protocol-based matching (highest priority)
        if let Some(ref protocol) = input.protocol {
            let protocol_lower = protocol.to_lowercase();
            let (driver, conf, reason) = match protocol_lower.as_str() {
                p if p.contains("modbus") => ("ModbusDriver".to_string(), 0.95, "Protocol matches Modbus".to_string()),
                p if p.contains("snmp") => ("SnmpDriver".to_string(), 0.95, "Protocol matches SNMP".to_string()),
                p if p.contains("onvif") => ("OnvifDriver".to_string(), 0.95, "Protocol matches ONVIF".to_string()),
                p if p.contains("mqtt") => ("MqttDriver".to_string(), 0.95, "Protocol matches MQTT".to_string()),
                p if p.contains("bacnet") => ("BacnetDriver".to_string(), 0.95, "Protocol matches BACnet".to_string()),
                p if p.contains("opcua") || p.contains("opc-ua") => ("OpcUaDriver".to_string(), 0.95, "Protocol matches OPC UA".to_string()),
                p if p.contains("http") || p.contains("rest") || p.contains("api") => ("HttpDriver".to_string(), 0.8, "Protocol suggests HTTP/REST".to_string()),
                _ => {
                    // Check if there's a driver with the protocol name
                    let driver_name = format!("{}Driver", capitalized(&protocol_lower));
                    if all_drivers.contains(&driver_name) {
                        (driver_name, 0.7, format!("Protocol '{}' matches driver name", protocol))
                    } else {
                        (String::new(), 0.0, String::new())
                    }
                }
            };

            if conf > confidence && !driver.is_empty() {
                confidence = conf;
                matched_driver = Some(driver);
                match_reason = reason;
            }
        }

        // Manufacturer/brand-based matching
        if let Some(ref manufacturer) = input.manufacturer {
            let mfg_lower = manufacturer.to_lowercase();
            let (driver, conf, reason) = match mfg_lower.as_str() {
                m if m.contains("siemens") => ("S7Driver".to_string(), 0.85, "Manufacturer Siemens detected".to_string()),
                m if m.contains("schneider") || m.contains("se") => ("ModbusDriver".to_string(), 0.7, "Schneider Electric typically uses Modbus".to_string()),
                m if m.contains("abb") => ("ModbusDriver".to_string(), 0.7, "ABB often uses Modbus".to_string()),
                m if m.contains("honeywell") => ("BacnetDriver".to_string(), 0.75, "Honeywell often uses BACnet".to_string()),
                m if m.contains("johnson") || m.contains("johnson controls") => ("BacnetDriver".to_string(), 0.75, "Johnson Controls typically uses BACnet".to_string()),
                m if m.contains("hikvision") || m.contains("dahua") || m.contains("axis") => ("OnvifDriver".to_string(), 0.9, "IP Camera manufacturer detected".to_string()),
                m if m.contains("APC") || m.contains("apc") => ("SnmpDriver".to_string(), 0.8, "APC UPS typically uses SNMP".to_string()),
                m if m.contains("emerson") || m.contains("rosemount") => ("ModbusDriver".to_string(), 0.7, "Emerson/rosemount typically uses Modbus".to_string()),
                _ => (String::new(), 0.0, String::new()),
            };

            if conf > confidence {
                confidence = conf;
                matched_driver = Some(driver);
                match_reason = reason;
            }
        }

        // Model-based matching
        if let Some(ref model) = input.model {
            let model_lower = model.to_lowercase();
            // Check for specific model patterns
            if model_lower.contains("S7") || model_lower.contains("1200") || model_lower.contains("1500") {
                if confidence < 0.8 {
                    confidence = 0.8;
                    matched_driver = Some("S7Driver".to_string());
                    match_reason = "Siemens S7 PLC model detected".to_string();
                }
            } else if model_lower.contains("UPS") || model_lower.contains("smart") {
                if confidence < 0.7 {
                    confidence = 0.7;
                    matched_driver = Some("SnmpDriver".to_string());
                    match_reason = "UPS/Smart device model pattern detected".to_string();
                }
            }
        }

        // Verify the matched driver exists
        let matched_driver = matched_driver.filter(|d| driver::has_driver(d));

        if matched_driver.is_none() && confidence > 0.0 {
            // Driver not available, provide suggestions
            match_reason = format!("{} but driver not available", match_reason);
        }

        Ok(serde_json::to_value(MatchDriverResponse {
            matched_driver,
            confidence,
            match_reason,
            available_drivers: all_drivers,
        })
        .unwrap())
    }
}

// === Generate Driver Handler (Stub - Phase 3) ===
pub struct GenerateDriverHandler;

#[async_trait]
impl ToolHandler for GenerateDriverHandler {
    fn name(&self) -> &str {
        "generate_driver"
    }

    fn description(&self) -> &str {
        "Generate a new driver from natural language description using cloud LLM (Phase 3)"
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert("description".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Natural language description of the driver to generate".to_string()) });
        props.insert("protocol".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Communication protocol (Modbus, SNMP, etc.)".to_string()) });
        InputSchema::object(vec!["description".to_string()], props)
    }

    async fn execute(&self, _args: Value) -> Result<Value, ToolError> {
        Err(ToolError::NotImplemented(
            "Phase 3: Cloud LLM driver generation - This feature requires cloud-side LLM integration".to_string(),
        ))
    }
}

// === Load Driver Handler ===
pub struct LoadDriverHandler;

#[async_trait]
impl ToolHandler for LoadDriverHandler {
    fn name(&self) -> &str {
        "load_driver"
    }

    fn description(&self) -> &str {
        "Load a dynamic driver (.so/.dll/.dylib) into the gateway"
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert("path".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Path to the dynamic driver library file".to_string()) });
        InputSchema::object(vec!["path".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: LoadDriverInput = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        match driver::load_dynamic_driver(&input.path) {
            Ok(driver_name) => Ok(serde_json::to_value(LoadDriverResponse {
                driver_name,
                success: true,
                message: "Driver loaded successfully".to_string(),
            })
            .unwrap()),
            Err(e) => Ok(serde_json::to_value(LoadDriverResponse {
                driver_name: String::new(),
                success: false,
                message: format!("Failed to load driver: {}", e),
            })
            .unwrap()),
        }
    }
}

// === Unload Driver Handler ===
pub struct UnloadDriverHandler;

#[async_trait]
impl ToolHandler for UnloadDriverHandler {
    fn name(&self) -> &str {
        "unload_driver"
    }

    fn description(&self) -> &str {
        "Unload a previously loaded dynamic driver from the gateway"
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert("name".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Name of the driver to unload".to_string()) });
        InputSchema::object(vec!["name".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: UnloadDriverInput = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        // Check if driver is a static driver (cannot be unloaded)
        if driver::is_driver_supported(&input.name) {
            return Ok(serde_json::to_value(UnloadDriverResponse {
                name: input.name,
                success: false,
                message: "Cannot unload static driver - only dynamic drivers can be unloaded".to_string(),
            })
            .unwrap());
        }

        match driver::unload_dynamic_driver(&input.name) {
            Ok(_) => Ok(serde_json::to_value(UnloadDriverResponse {
                name: input.name.clone(),
                success: true,
                message: format!("Driver '{}' unloaded successfully", input.name),
            })
            .unwrap()),
            Err(e) => Ok(serde_json::to_value(UnloadDriverResponse {
                name: input.name,
                success: false,
                message: format!("Failed to unload driver: {}", e),
            })
            .unwrap()),
        }
    }
}

// === Test Driver Handler ===
pub struct TestDriverHandler;

#[async_trait]
impl ToolHandler for TestDriverHandler {
    fn name(&self) -> &str {
        "test_driver"
    }

    fn description(&self) -> &str {
        "Perform a smoke test on a driver to verify it can connect and read data"
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert("driverName".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Name of the driver to test (required)".to_string()) });
        props.insert("address".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("Device address for connection test (optional)".to_string()) });
        props.insert("connectionConfig".to_string(), PropertySchema { prop_type: "string".to_string(), description: Some("JSON string with connection parameters (optional)".to_string()) });
        InputSchema::object(vec!["driverName".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: TestDriverInput = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let start = std::time::Instant::now();

        // Check if driver exists
        if !driver::has_driver(&input.driver_name) {
            return Ok(serde_json::to_value(TestDriverResponse {
                driver_name: input.driver_name.clone(),
                success: false,
                message: format!("Driver '{}' not found", input.driver_name),
                test_data: None,
                execution_time_ms: start.elapsed().as_millis() as u64,
            })
            .unwrap());
        }

        // For SimulatedDriver, we can actually run a test
        if input.driver_name == "SimulatedDriver" {
            let state = crate::api::mcp::get_app_state()
                .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;

            // Create a test device
            let test_device = crate::dto::entity::device::Device {
                id: uuid::Uuid::new_v4().to_string(),
                name: "test_device".to_string(),
                display_name: Some("Test Device".to_string()),
                device_type: Some("test".to_string()),
                address: input.address.or(Some("test".to_string())),
                description: Some("Test device for driver smoke test".to_string()),
                position: None,
                driver_name: Some(input.driver_name.clone()),
                device_model: None,
                protocol_type: Some("simulated".to_string()),
                factory_name: Some("Test".to_string()),
                linked_data: None,
                driver_options: input.connection_config,
                state: Some(0),
                is_online: false,
                last_heartbeat: None,
                properties: None,
                commands: None,
                tags: None,
                parent_id: None,
                product_id: None,
                tenant_id: None,
                created_at: Some(chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()),
                updated_at: Some(chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()),
            };

            // Create driver instance
            match driver::create_driver(&input.driver_name, &test_device, Arc::clone(&state.data_context)) {
                Ok(mut driver_wrapper) => {
                    // Try to read data
                    let result = driver_wrapper.read_data();

                    let execution_time_ms = start.elapsed().as_millis() as u64;

                    match result.result {
                        Ok(data) => {
                            let test_data: Vec<TestDataPoint> = data
                                .into_iter()
                                .map(|rv| TestDataPoint {
                                    property_name: rv.name,
                                    value: rv.value,
                                    value_type: rv.value_type,
                                })
                                .collect();

                            Ok(serde_json::to_value(TestDriverResponse {
                                driver_name: input.driver_name,
                                success: true,
                                message: "Driver smoke test passed successfully".to_string(),
                                test_data: Some(test_data),
                                execution_time_ms,
                            })
                            .unwrap())
                        }
                        Err(e) => Ok(serde_json::to_value(TestDriverResponse {
                            driver_name: input.driver_name,
                            success: false,
                            message: format!("Driver read failed: {}", e),
                            test_data: None,
                            execution_time_ms,
                        })
                        .unwrap()),
                    }
                }
                Err(e) => Ok(serde_json::to_value(TestDriverResponse {
                    driver_name: input.driver_name,
                    success: false,
                    message: format!("Failed to create driver instance: {}", e),
                    test_data: None,
                    execution_time_ms: start.elapsed().as_millis() as u64,
                })
                .unwrap()),
            }
        } else {
            // For other drivers, just verify the driver is available
            let execution_time_ms = start.elapsed().as_millis() as u64;
            Ok(serde_json::to_value(TestDriverResponse {
                driver_name: input.driver_name.clone(),
                success: true,
                message: format!("Driver '{}' is available and registered", input.driver_name),
                test_data: None,
                execution_time_ms,
            })
            .unwrap())
        }
    }
}

/// Helper function to capitalize a string
fn capitalized(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}
