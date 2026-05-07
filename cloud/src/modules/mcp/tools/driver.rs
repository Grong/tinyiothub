// Driver Tools Module
// MCP tools for driver management

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::modules::{
    device::driver,
    mcp::{
        handlers::get_mcp_context,
        tool_registry::{InputSchema, PropertySchema, ToolError, ToolHandler},
    },
};

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

/// Test driver request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TestDriverInput {
    driver_name: String,
    address: Option<String>,
    connection_config: Option<String>,
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
        let _claims = get_mcp_context()
            .ok_or_else(|| ToolError::Unauthorized("MCP context not initialized".to_string()))?;

        let all_names = driver::get_all_driver_names();

        let mut drivers = Vec::new();
        for name in &all_names {
            drivers.push(DriverInfo {
                name: name.clone(),
                version: "1.0.0".to_string(),
                description: Some(format!("{} driver", name)),
                is_dynamic: false,
                path: None,
                category: Some("protocol".to_string()),
            });
        }

        let total = drivers.len();
        Ok(serde_json::to_value(DriverListResponse { drivers, total }).unwrap())
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
        props.insert(
            "driverName".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Name of the driver to test (required)".to_string()),
            },
        );
        props.insert(
            "address".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Device address for connection test (optional)".to_string()),
            },
        );
        props.insert(
            "connectionConfig".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("JSON string with connection parameters (optional)".to_string()),
            },
        );
        InputSchema::object(vec!["driverName".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let _claims = get_mcp_context()
            .ok_or_else(|| ToolError::Unauthorized("MCP context not initialized".to_string()))?;
        let input: TestDriverInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let start = std::time::Instant::now();

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

        if input.driver_name == "SimulatedDriver" {
            let _state = crate::modules::mcp::get_app_state()
                .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;

            let test_device = tinyiothub_core::models::device::Device {
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
                status: tinyiothub_core::models::device::DeviceStatus::Offline,
                last_heartbeat: None,
                properties: None,
                commands: None,
                tags: None,
                parent_id: None,
                product_id: None,
                created_at: Some(chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()),
                updated_at: Some(chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()),
            };

            match driver::create_driver(&input.driver_name, &test_device) {
                Ok(mut driver_wrapper) => {
                    let result = driver_wrapper.read_data_once();
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
