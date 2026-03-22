//! TinyIoTHub MCP Server
//! 
//! MCP Server 实现，用于连接 AI Agent 与 TinyIoTHub 物联网平台

pub mod client;
pub mod config;
pub mod tools;

mod tests;

use std::io::{BufRead, BufReader, Write};
use std::sync::Arc;

use jsonrpc_core::{Error, ErrorCode, Id, MethodCall, Params, Value};
use tokio::sync::RwLock;
use tracing::{error, info};

use client::TinyIoTHubClient;
use config::McpConfig;

pub struct McpServer {
    client: TinyIoTHubClient,
    config: McpConfig,
}

impl McpServer {
    pub fn new(config: McpConfig) -> Self {
        let client = TinyIoTHubClient::new(
            &config.tinyiothub.api_url,
            &config.tinyiothub.api_key,
        );
        
        Self { client, config }
    }
    
    /// 处理 MCP 方法调用
    pub async fn handle_call(&self, call: MethodCall) -> Result<Value, Error> {
        let method = call.method.clone();
        
        info!("Handling MCP call: {}", method);
        
        let result: Result<Value, Error> = match method.as_str() {
            // 工具调用
            "list_devices" => self.tool_list_devices(call.params).await.map_err(|e| Error {
                code: ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            }),
            "get_device" => self.tool_get_device(call.params).await.map_err(|e| Error {
                code: ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            }),
            "get_device_status" => self.tool_get_device_status(call.params).await.map_err(|e| Error {
                code: ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            }),
            "read_sensor_data" => self.tool_read_sensor_data(call.params).await.map_err(|e| Error {
                code: ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            }),
            "send_command" => self.tool_send_command(call.params).await.map_err(|e| Error {
                code: ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            }),
            "list_alarms" => self.tool_list_alarms(call.params).await.map_err(|e| Error {
                code: ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            }),
            "acknowledge_alarm" => self.tool_acknowledge_alarm(call.params).await.map_err(|e| Error {
                code: ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            }),
            "get_alarm_statistics" => self.tool_get_alarm_statistics(call.params).await.map_err(|e| Error {
                code: ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            }),
            "list_drivers" => self.tool_list_drivers(call.params).await.map_err(|e| Error {
                code: ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            }),
            "get_driver_info" => self.tool_get_driver_info(call.params).await.map_err(|e| Error {
                code: ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            }),
            "list_templates" => self.tool_list_templates(call.params).await.map_err(|e| Error {
                code: ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            }),
            "get_template" => self.tool_get_template(call.params).await.map_err(|e| Error {
                code: ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            }),
            "get_current_user" => self.tool_get_current_user(call.params).await.map_err(|e| Error {
                code: ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            }),
            
            // MCP 协议方法
            "initialize" => self.handle_initialize(call.params).await,
            "tools/list" => self.handle_tools_list(call.params).await,
            
            // 未知方法
            _ => Err(Error {
                code: ErrorCode::MethodNotFound,
                message: format!("Unknown method: {}", method),
                data: None,
            }),
        };
        
        match result {
            Ok(value) => Ok(value),
            Err(e) => {
                error!("MCP call error: {}", e);
                Err(e)
            }
        }
    }
    
    /// handle_initialize
    async fn handle_initialize(&self, _params: Params) -> Result<Value, Error> {
        Ok(serde_json::json!({
            "protocolVersion": "2024-11-05",
            "serverInfo": {
                "name": "tinyiothub",
                "version": "1.0.0"
            },
            "capabilities": {
                "tools": {}
            }
        }))
    }
    
    /// handle_tools/list
    async fn handle_tools_list(&self, _params: Params) -> Result<Value, Error> {
        Ok(serde_json::json!({
            "tools": tools::get_all_tools_json()
        }))
    }
}

// ==================== 工具实现 ====================

impl McpServer {
    /// list_devices - 获取设备列表
    async fn tool_list_devices(&self, params: Params) -> Result<Value, anyhow::Error> {
        #[derive(serde::Deserialize)]
        struct ListDevicesParams {
            page: Option<u32>,
            page_size: Option<u32>,
            include_properties: Option<bool>,
        }
        
        let params: ListDevicesParams = params.parse().unwrap_or(ListDevicesParams {
            page: Some(1),
            page_size: Some(20),
            include_properties: Some(false),
        });
        
        let response = self.client
            .list_devices(
                params.page.unwrap_or(1),
                params.page_size.unwrap_or(20),
                params.include_properties.unwrap_or(false),
            )
            .await?;
            
        Ok(serde_json::to_value(response)?)
    }
    
    /// get_device - 获取设备详情
    async fn tool_get_device(&self, params: Params) -> Result<Value, anyhow::Error> {
        #[derive(serde::Deserialize)]
        struct GetDeviceParams {
            device_id: String,
            include_properties: Option<bool>,
        }
        
        let params: GetDeviceParams = params.parse()?;
        let response = self.client
            .get_device(&params.device_id, params.include_properties.unwrap_or(true))
            .await?;
            
        Ok(serde_json::to_value(response)?)
    }
    
    /// get_device_status - 获取设备状态
    async fn tool_get_device_status(&self, params: Params) -> Result<Value, anyhow::Error> {
        #[derive(serde::Deserialize)]
        struct GetDeviceStatusParams {
            device_id: String,
        }
        
        let params: GetDeviceStatusParams = params.parse()?;
        let device = self.client.get_device(&params.device_id, false).await?;
        
        let result = serde_json::json!({
            "device_id": device.id,
            "name": device.name,
            "state": device.state,
            "is_online": device.is_online,
            "last_heartbeat": device.last_heartbeat,
        });
        
        Ok(result)
    }
    
    /// read_sensor_data - 读取传感器数据
    async fn tool_read_sensor_data(&self, params: Params) -> Result<Value, anyhow::Error> {
        #[derive(serde::Deserialize)]
        struct ReadSensorParams {
            device_id: String,
            properties: Option<Vec<String>>,
        }
        
        let params: ReadSensorParams = params.parse()?;
        
        // 调用 API 读取属性
        let response = self.client
            .read_device_properties(&params.device_id, params.properties)
            .await?;
            
        Ok(serde_json::to_value(response)?)
    }
    
    /// send_command - 发送控制命令
    async fn tool_send_command(&self, params: Params) -> Result<Value, anyhow::Error> {
        #[derive(serde::Deserialize)]
        struct SendCommandParams {
            device_id: String,
            command: String,
            parameters: Option<serde_json::Value>,
        }
        
        let params: SendCommandParams = params.parse()?;
        
        let response = self.client
            .send_command(
                &params.device_id,
                &params.command,
                params.parameters,
            )
            .await?;
            
        Ok(serde_json::to_value(response)?)
    }
    
    /// list_alarms - 获取告警列表
    async fn tool_list_alarms(&self, params: Params) -> Result<Value, anyhow::Error> {
        #[derive(serde::Deserialize)]
        struct ListAlarmsParams {
            status: Option<String>,
            device_id: Option<String>,
            limit: Option<u32>,
        }
        
        let params: ListAlarmsParams = params.parse().unwrap_or(ListAlarmsParams {
            status: Some("active".to_string()),
            device_id: None,
            limit: Some(20),
        });
        
        let response = self.client
            .list_alarms(
                params.status.as_deref().unwrap_or("active"),
                params.device_id.as_deref(),
                params.limit.unwrap_or(20),
            )
            .await?;
            
        Ok(serde_json::to_value(response)?)
    }
    
    /// acknowledge_alarm - 确认告警
    async fn tool_acknowledge_alarm(&self, params: Params) -> Result<Value, anyhow::Error> {
        #[derive(serde::Deserialize)]
        struct AckAlarmParams {
            alarm_id: String,
            comment: Option<String>,
        }
        
        let params: AckAlarmParams = params.parse()?;
        let response = self.client
            .acknowledge_alarm(&params.alarm_id, params.comment.as_deref())
            .await?;
            
        Ok(serde_json::to_value(response)?)
    }
    
    /// get_alarm_statistics - 获取告警统计
    async fn tool_get_alarm_statistics(&self, _params: Params) -> Result<Value, anyhow::Error> {
        let response = self.client.get_alarm_statistics().await?;
        Ok(serde_json::to_value(response)?)
    }
    
    /// list_drivers - 获取驱动列表
    async fn tool_list_drivers(&self, _params: Params) -> Result<Value, anyhow::Error> {
        let response = self.client.list_drivers().await?;
        Ok(serde_json::to_value(response)?)
    }
    
    /// get_driver_info - 获取驱动详情
    async fn tool_get_driver_info(&self, params: Params) -> Result<Value, anyhow::Error> {
        #[derive(serde::Deserialize)]
        struct GetDriverInfoParams {
            name: String,
        }
        
        let params: GetDriverInfoParams = params.parse()?;
        let response = self.client.get_driver(&params.name).await?;
        Ok(serde_json::to_value(response)?)
    }
    
    /// list_templates - 获取模板列表
    async fn tool_list_templates(&self, params: Params) -> Result<Value, anyhow::Error> {
        #[derive(serde::Deserialize)]
        struct ListTemplatesParams {
            page: Option<u32>,
            page_size: Option<u32>,
        }
        
        let params: ListTemplatesParams = params.parse().unwrap_or(ListTemplatesParams {
            page: Some(1),
            page_size: Some(20),
        });
        
        let response: Vec<client::Template> = self.client
            .list_templates(
                params.page.unwrap_or(1),
                params.page_size.unwrap_or(20),
            )
            .await?;
        Ok(serde_json::to_value(response)?)
    }
    
    /// get_template - 获取模板详情
    async fn tool_get_template(&self, params: Params) -> Result<Value, anyhow::Error> {
        #[derive(serde::Deserialize)]
        struct GetTemplateParams {
            id: String,
        }
        
        let params: GetTemplateParams = params.parse()?;
        let response: client::Template = self.client.get_template(&params.id).await?;
        Ok(serde_json::to_value(response)?)
    }
    
    /// get_current_user - 获取当前用户信息
    async fn tool_get_current_user(&self, _params: Params) -> Result<Value, anyhow::Error> {
        let response: client::User = self.client.get_current_user().await?;
        Ok(serde_json::to_value(response)?)
    }
}

// ==================== STDIO 处理 ====================

/// 处理 STDIO 输入
async fn process_stdio(server: Arc<RwLock<McpServer>>) {
    let stdin = BufReader::new(std::io::stdin());
    let mut stdout = std::io::stdout();
    
    for line in stdin.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        
        // 跳过空行
        if line.trim().is_empty() {
            continue;
        }
        
        // 解析 JSON-RPC 请求
        let request: Result<MethodCall, _> = serde_json::from_str(&line);
        
        let response = match request {
            Ok(call) => {
                let server = server.read().await;
                match server.handle_call(call).await {
                    Ok(result) => {
                        serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": Id::Null,
                            "result": result
                        })
                    }
                    Err(e) => {
                        serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": Id::Null,
                            "error": {
                                "code": e.code.code(),
                                "message": e.message
                            }
                        })
                    }
                }
            }
            Err(e) => {
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": Id::Null,
                    "error": {
                        "code": -32700,
                        "message": format!("Parse error: {}", e)
                    }
                })
            }
        };
        
        // 输出响应
        let _ = writeln!(stdout, "{}", response);
        let _ = stdout.flush();
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();
    
    info!("TinyIoTHub MCP Server starting...");
    
    // 加载配置
    let config = config::load_config()?;
    info!("Config loaded: {}", config.tinyiothub.api_url);
    
    // 创建 MCP Server
    let server = Arc::new(RwLock::new(McpServer::new(config)));
    
    // 处理 STDIO
    process_stdio(server).await;
    
    Ok(())
}
