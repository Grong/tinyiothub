//! TinyIoTHub MCP Server 单元测试

#[cfg(test)]
mod tests {
    use crate::client::{ApiResponse, Device, Alarm};
    use crate::config::McpConfig;
    use crate::tools::{get_all_tools, device, alarm};
    use crate::{McpServer, MethodCall, Id, Params};

    // ==================== 配置测试 ====================

    #[test]
    fn test_config_default() {
        let config = McpConfig::default();
        assert_eq!(config.tinyiothub.api_url, "http://localhost:3002");
    }

    #[test]
    fn test_api_response_success() {
        let response: ApiResponse<String> = ApiResponse {
            msg: "".to_string(),
            code: 0,
            result: Some("test".to_string()),
        };
        
        assert!(response.into_result().is_ok());
    }
    
    #[test]
    fn test_api_response_error() {
        let response: ApiResponse<String> = ApiResponse {
            msg: "Not found".to_string(),
            code: -1,
            result: None,
        };
        
        let result = response.into_result();
        assert!(result.is_err());
    }
    
    #[test]
    fn test_device_serde() {
        let json = r#"{
            "id": "test-id",
            "name": "test-device",
            "display_name": "测试设备",
            "device_type": "sensor",
            "is_online": true
        }"#;
        
        let device: Device = serde_json::from_str(json).unwrap();
        assert_eq!(device.id, "test-id");
        assert_eq!(device.name, "test-device");
    }
    
    #[test]
    fn test_alarm_serde() {
        let json = r#"{
            "id": "alarm-001",
            "device_id": "device-001",
            "alarm_type": "threshold",
            "alarm_level": "warning",
            "message": "温度过高",
            "status": "active",
            "is_acknowledged": false,
            "created_at": "2026-03-15T10:00:00Z"
        }"#;
        
        let alarm: Alarm = serde_json::from_str(json).unwrap();
        assert_eq!(alarm.id, "alarm-001");
    }

    // ==================== 工具定义测试 ====================

    #[test]
    fn test_get_all_tools() {
        let tools = get_all_tools();
        assert!(!tools.is_empty());
        
        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(tool_names.contains(&"list_devices"));
        assert!(tool_names.contains(&"get_device"));
        assert!(tool_names.contains(&"read_sensor_data"));
    }

    // ==================== 设备工具测试 ====================

    #[test]
    fn test_list_devices_schema() {
        let tool = device::list_devices();
        assert_eq!(tool.name, "list_devices");
    }
    
    #[test]
    fn test_get_device_schema() {
        let tool = device::get_device();
        assert_eq!(tool.name, "get_device");
    }
    
    #[test]
    fn test_send_command_schema() {
        let tool = device::send_command();
        assert_eq!(tool.name, "send_command");
    }

    // ==================== 告警工具测试 ====================

    #[test]
    fn test_list_alarms_schema() {
        let tool = alarm::list_alarms();
        assert_eq!(tool.name, "list_alarms");
    }
    
    #[test]
    fn test_acknowledge_alarm_schema() {
        let tool = alarm::acknowledge_alarm();
        assert_eq!(tool.name, "acknowledge_alarm");
    }

    // ==================== 集成测试 ====================

    #[tokio::test]
    async fn test_mcp_server_creation() {
        let config = McpConfig::default();
        let _server = McpServer::new(config);
    }

    #[tokio::test]
    async fn test_handle_initialize() {
        let config = McpConfig::default();
        let server = McpServer::new(config);
        
        let result = server.handle_initialize(Params::None).await.unwrap();
        
        assert!(result.get("protocolVersion").is_some());
    }

    #[tokio::test]
    async fn test_handle_tools_list() {
        let config = McpConfig::default();
        let server = McpServer::new(config);
        
        let result = server.handle_tools_list(Params::None).await.unwrap();
        
        assert!(result.get("tools").is_some());
    }

    #[tokio::test]
    async fn test_handle_unknown_method() {
        let config = McpConfig::default();
        let server = McpServer::new(config);
        
        let call = MethodCall {
            jsonrpc: Some(jsonrpc_core::Version::V2),
            id: Id::Null,
            method: "unknown_method".to_string(),
            params: Params::None,
        };
        
        let result = server.handle_call(call).await;
        assert!(result.is_err());
    }
}
