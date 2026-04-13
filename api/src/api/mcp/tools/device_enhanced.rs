// Device Enhanced Tools Module
// MCP tools for device comparison, diagnosis, and serial port scanning

use std::collections::HashMap;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;

use crate::api::mcp::handlers::get_mcp_context;
use crate::api::mcp::tool_registry::{InputSchema, PropertySchema, ToolError, ToolHandler};
use crate::infrastructure::diagnostics::DiagnosticsService;

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
    // No workspace_id needed - serial scanning is hardware-level
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

        let claims = get_mcp_context().ok_or_else(|| {
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

        // SECURITY: Verify all devices belong to authenticated workspace
        let db = state.database();
        for device_id in &input.device_ids {
            let device = crate::dto::entity::device::Device::find_by_id(db, device_id)
                .await
                .map_err(|e| ToolError::Internal(format!("failed to verify device: {}", e)))?
                .ok_or_else(|| ToolError::NotFound(format!("device {} not found", device_id)))?;

            if device.workspace_id.as_ref() != Some(&claims.workspace_id) {
                tracing::warn!("MCP compare_devices: access denied to device {} for workspace {}", device_id, claims.workspace_id);
                return Err(ToolError::Forbidden(
                    "Access denied: one or more devices do not belong to authenticated workspace".to_string()
                ));
            }
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

        let claims = get_mcp_context().ok_or_else(|| {
            ToolError::Unauthorized("MCP context not initialized".to_string())
        })?;

        let state = crate::api::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;

        // SECURITY: Verify device belongs to authenticated workspace
        let db = state.database();
        let device = crate::dto::entity::device::Device::find_by_id(db, &input.device_id)
            .await
            .map_err(|e| ToolError::Internal(format!("failed to verify device: {}", e)))?
            .ok_or_else(|| ToolError::NotFound(format!("device {} not found", input.device_id)))?;

        if device.workspace_id.as_ref() != Some(&claims.workspace_id) {
            tracing::warn!("MCP diagnose_device: access denied to device {} for workspace {}", input.device_id, claims.workspace_id);
            return Err(ToolError::Forbidden(
                "Access denied: device does not belong to authenticated workspace".to_string()
            ));
        }

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
        // Serial scanning is hardware-level, no workspace scoping needed
        InputSchema::object(vec![], HashMap::new())
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let _input: ScanSerialInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        // SECURITY: Verify authentication (system-level operation, no workspace check needed)
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