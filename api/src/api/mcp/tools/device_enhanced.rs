// Device Enhanced Tools Module
// MCP tools for device comparison, diagnosis, and serial port scanning

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::mcp::handlers::get_mcp_context;
use crate::api::mcp::tool_registry::{InputSchema, PropertySchema, ToolError, ToolHandler};
use crate::infrastructure::diagnostics::{
    DiagnosticsService, DeviceDiagnosis, PropertyComparison, SerialPortInfo,
};

/// Tool input: Compare devices
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompareDevicesInput {
    device_ids: Vec<String>,
    property: String,
}

/// Tool input: Diagnose device
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiagnoseDeviceInput {
    device_id: String,
}

/// Tool input: Scan serial ports
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScanSerialInput {
    workspace_id: Option<String>,
}

/// Compare devices tool handler
pub struct CompareDevicesHandler;

#[async_trait]
impl ToolHandler for CompareDevicesHandler {
    fn name(&self) -> &str {
        "compare_devices"
    }

    fn description(&self) -> &str {
        "Compare current property values across multiple devices in a workspace."
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert(
            "deviceIds".to_string(),
            PropertySchema {
                prop_type: "array".to_string(),
                description: Some("Array of device IDs to compare".to_string()),
            },
        );
        props.insert(
            "property".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Property name to compare (e.g., temperature, voltage)".to_string()),
            },
        );
        InputSchema::object(vec!["deviceIds".to_string(), "property".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: CompareDevicesInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let _claims = get_mcp_context().ok_or_else(|| {
            ToolError::Unauthorized("MCP context not initialized".to_string())
        })?;

        let state = crate::api::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;

        if input.device_ids.is_empty() {
            return Err(ToolError::InvalidParams("device_ids cannot be empty".to_string()));
        }

        if input.device_ids.len() < 2 {
            return Err(ToolError::InvalidParams(
                "Need at least 2 devices to compare".to_string(),
            ));
        }

        let comparison = DiagnosticsService::compare_properties(
            &state,
            &input.device_ids,
            &input.property,
        )
        .await
        .map_err(|e| ToolError::Internal(format!("Failed to compare devices: {}", e)))?;

        Ok(serde_json::to_value(comparison).unwrap())
    }
}

/// Diagnose device tool handler
pub struct DiagnoseDeviceHandler;

#[async_trait]
impl ToolHandler for DiagnoseDeviceHandler {
    fn name(&self) -> &str {
        "diagnose_device"
    }

    fn description(&self) -> &str {
        "Analyze a device for common fault patterns including offline status, reconnect history, and error rates."
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert(
            "deviceId".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Device ID to diagnose".to_string()),
            },
        );
        InputSchema::object(vec!["deviceId".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: DiagnoseDeviceInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let _claims = get_mcp_context().ok_or_else(|| {
            ToolError::Unauthorized("MCP context not initialized".to_string())
        })?;

        let state = crate::api::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;

        let diagnosis = DiagnosticsService::diagnose_device(&state, &input.device_id)
            .await
            .map_err(|e| ToolError::Internal(format!("Failed to diagnose device: {}", e)))?;

        Ok(serde_json::to_value(diagnosis).unwrap())
    }
}

/// Scan serial port tool handler
pub struct ScanSerialHandler;

#[async_trait]
impl ToolHandler for ScanSerialHandler {
    fn name(&self) -> &str {
        "scan_serial"
    }

    fn description(&self) -> &str {
        "Scan for available serial ports on the gateway device."
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert(
            "workspaceId".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Workspace ID (optional, for scoping)".to_string()),
            },
        );
        InputSchema::object(vec![], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let _input: ScanSerialInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let _claims = get_mcp_context().ok_or_else(|| {
            ToolError::Unauthorized("MCP context not initialized".to_string())
        })?;

        let ports = DiagnosticsService::scan_serial_ports()
            .map_err(|e| ToolError::Internal(format!("Failed to scan serial ports: {}", e)))?;

        Ok(serde_json::json!({
            "ports": ports,
            "count": ports.len()
        }).into())
    }
}