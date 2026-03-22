//! TinyIoTHub MCP Server
//!
//! MCP Server 实现，用于连接 AI Agent 与 TinyIoTHub 物联网平台

pub mod client;
pub mod config;
pub mod tools;

mod tests;

use std::io::{BufRead, BufReader, Write};
use std::sync::Arc;

use jsonrpc_core::{Error, ErrorCode, Id, MethodCall, Params, Value, Version};
use serde::Deserialize;
use tokio::sync::RwLock;
use tracing::{error, info};

use client::TinyIoTHubClient;
use config::McpConfig;

/// MCP 调用上下文，包含请求 ID 用于响应回传
#[derive(Debug)]
pub struct MethodCallWithId {
    pub id: Id,
    pub call: MethodCall,
}

/// MCP 工具调用错误，携带结构化信息用于生成准确的 JSON-RPC 错误码
#[derive(Debug)]
pub enum ToolError {
    /// 参数解析失败
    InvalidParams(String),
    /// API 返回 401 未授权
    Unauthorized(String),
    /// API 返回 404 资源不存在
    NotFound(String),
    /// API 返回 429 速率限制
    RateLimited(String),
    /// API 返回其他错误
    ApiError(i32, String),
    /// 网络/超时错误
    NetworkError(String),
    /// 内部错误
    Internal(String),
}

pub struct McpServer {
    client: TinyIoTHubClient,
    #[allow(dead_code)]
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
            "list_devices" => self.tool_list_devices(call.params).await,
            "get_device" => self.tool_get_device(call.params).await,
            "get_device_status" => self.tool_get_device_status(call.params).await,
            "read_sensor_data" => self.tool_read_sensor_data(call.params).await,
            "send_command" => self.tool_send_command(call.params).await,
            "list_alarms" => self.tool_list_alarms(call.params).await,
            "acknowledge_alarm" => self.tool_acknowledge_alarm(call.params).await,
            "get_alarm_statistics" => self.tool_get_alarm_statistics(call.params).await,
            "list_drivers" => self.tool_list_drivers(call.params).await,

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
                error!("MCP call error: {:?}", e);
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
    async fn tool_list_devices(&self, params: Params) -> Result<Value, Error> {
        #[derive(serde::Deserialize)]
        struct ListDevicesParams {
            page: Option<u32>,
            page_size: Option<u32>,
            include_properties: Option<bool>,
        }

        let params: ListDevicesParams = params
            .parse()
            .map_err(|e| Error {
                code: ErrorCode::InvalidParams,
                message: format!("Invalid params for list_devices: {}", e),
                data: None,
            })?;

        let response = self
            .client
            .list_devices(
                params.page.unwrap_or(1),
                params.page_size.unwrap_or(20),
                params.include_properties.unwrap_or(false),
            )
            .await
            .map_err(|e| map_client_error(e))?;

        Ok(serde_json::to_value(response).map_err(|e| Error {
            code: ErrorCode::InternalError,
            message: format!("Serialization error: {}", e),
            data: None,
        })?)
    }

    /// get_device - 获取设备详情
    async fn tool_get_device(&self, params: Params) -> Result<Value, Error> {
        #[derive(serde::Deserialize)]
        struct GetDeviceParams {
            device_id: String,
            include_properties: Option<bool>,
        }

        let params: GetDeviceParams = params.parse().map_err(|e| Error {
            code: ErrorCode::InvalidParams,
            message: format!("Invalid params for get_device: {}", e),
            data: None,
        })?;

        let response = self
            .client
            .get_device(&params.device_id, params.include_properties.unwrap_or(true))
            .await
            .map_err(|e| map_client_error(e))?;

        Ok(serde_json::to_value(response).map_err(|e| Error {
            code: ErrorCode::InternalError,
            message: format!("Serialization error: {}", e),
            data: None,
        })?)
    }

    /// get_device_status - 获取设备状态
    async fn tool_get_device_status(&self, params: Params) -> Result<Value, Error> {
        #[derive(serde::Deserialize)]
        struct GetDeviceStatusParams {
            device_id: String,
        }

        let params: GetDeviceStatusParams = params.parse().map_err(|e| Error {
            code: ErrorCode::InvalidParams,
            message: format!("Invalid params for get_device_status: {}", e),
            data: None,
        })?;

        let device = self
            .client
            .get_device(&params.device_id, false)
            .await
            .map_err(|e| map_client_error(e))?;

        Ok(serde_json::json!({
            "device_id": device.id,
            "name": device.name,
            "state": device.state,
            "is_online": device.is_online,
            "last_heartbeat": device.last_heartbeat,
        }))
    }

    /// read_sensor_data - 读取传感器数据
    async fn tool_read_sensor_data(&self, params: Params) -> Result<Value, Error> {
        #[derive(serde::Deserialize)]
        struct ReadSensorParams {
            device_id: String,
            properties: Option<Vec<String>>,
        }

        let params: ReadSensorParams = params.parse().map_err(|e| Error {
            code: ErrorCode::InvalidParams,
            message: format!("Invalid params for read_sensor_data: {}", e),
            data: None,
        })?;

        let response = self
            .client
            .read_device_properties(&params.device_id, params.properties)
            .await
            .map_err(|e| map_client_error(e))?;

        Ok(serde_json::to_value(response).map_err(|e| Error {
            code: ErrorCode::InternalError,
            message: format!("Serialization error: {}", e),
            data: None,
        })?)
    }

    /// send_command - 发送控制命令
    async fn tool_send_command(&self, params: Params) -> Result<Value, Error> {
        #[derive(serde::Deserialize)]
        struct SendCommandParams {
            device_id: String,
            command: String,
            parameters: Option<serde_json::Value>,
        }

        let params: SendCommandParams = params.parse().map_err(|e| Error {
            code: ErrorCode::InvalidParams,
            message: format!("Invalid params for send_command: {}", e),
            data: None,
        })?;

        let response = self
            .client
            .send_command(&params.device_id, &params.command, params.parameters)
            .await
            .map_err(|e| map_client_error(e))?;

        Ok(serde_json::to_value(response).map_err(|e| Error {
            code: ErrorCode::InternalError,
            message: format!("Serialization error: {}", e),
            data: None,
        })?)
    }

    /// list_alarms - 获取告警列表
    async fn tool_list_alarms(&self, params: Params) -> Result<Value, Error> {
        #[derive(serde::Deserialize)]
        struct ListAlarmsParams {
            status: Option<String>,
            device_id: Option<String>,
            limit: Option<u32>,
        }

        let params: ListAlarmsParams = params.parse().map_err(|e| Error {
            code: ErrorCode::InvalidParams,
            message: format!("Invalid params for list_alarms: {}", e),
            data: None,
        })?;

        let response = self
            .client
            .list_alarms(
                params.status.as_deref().unwrap_or("active"),
                params.device_id.as_deref(),
                params.limit.unwrap_or(20),
            )
            .await
            .map_err(|e| map_client_error(e))?;

        Ok(serde_json::to_value(response).map_err(|e| Error {
            code: ErrorCode::InternalError,
            message: format!("Serialization error: {}", e),
            data: None,
        })?)
    }

    /// acknowledge_alarm - 确认告警
    async fn tool_acknowledge_alarm(&self, params: Params) -> Result<Value, Error> {
        #[derive(serde::Deserialize)]
        struct AckAlarmParams {
            alarm_id: String,
            comment: Option<String>,
        }

        let params: AckAlarmParams = params.parse().map_err(|e| Error {
            code: ErrorCode::InvalidParams,
            message: format!("Invalid params for acknowledge_alarm: {}", e),
            data: None,
        })?;

        let response = self
            .client
            .acknowledge_alarm(&params.alarm_id, params.comment.as_deref())
            .await
            .map_err(|e| map_client_error(e))?;

        Ok(serde_json::to_value(response).map_err(|e| Error {
            code: ErrorCode::InternalError,
            message: format!("Serialization error: {}", e),
            data: None,
        })?)
    }

    /// get_alarm_statistics - 获取告警统计
    async fn tool_get_alarm_statistics(&self, _params: Params) -> Result<Value, Error> {
        let response = self
            .client
            .get_alarm_statistics()
            .await
            .map_err(|e| map_client_error(e))?;

        Ok(serde_json::to_value(response).map_err(|e| Error {
            code: ErrorCode::InternalError,
            message: format!("Serialization error: {}", e),
            data: None,
        })?)
    }

    /// list_drivers - 获取驱动列表
    async fn tool_list_drivers(&self, _params: Params) -> Result<Value, Error> {
        let response = self
            .client
            .list_drivers()
            .await
            .map_err(|e| map_client_error(e))?;

        Ok(serde_json::to_value(response).map_err(|e| Error {
            code: ErrorCode::InternalError,
            message: format!("Serialization error: {}", e),
            data: None,
        })?)
    }
}

/// 将 client 的错误映射为 JSON-RPC 错误码
fn map_client_error(err: client::ClientError) -> Error {
    match err {
        client::ClientError::Unauthorized(msg) => Error {
            code: ErrorCode::ServerError(401),
            message: msg,
            data: None,
        },
        client::ClientError::NotFound(msg) => Error {
            code: ErrorCode::ServerError(404),
            message: msg,
            data: None,
        },
        client::ClientError::RateLimited(msg) => Error {
            code: ErrorCode::ServerError(429),
            message: msg,
            data: None,
        },
        client::ClientError::ApiError(status, msg) => Error {
            code: ErrorCode::ServerError(status as i64),
            message: msg,
            data: None,
        },
        client::ClientError::NetworkError(msg) => Error {
            code: ErrorCode::InternalError,
            message: format!("Network error: {}", msg),
            data: None,
        },
        client::ClientError::Timeout => Error {
            code: ErrorCode::InternalError,
            message: "Request timed out after 30 seconds".to_string(),
            data: None,
        },
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

        // 解析 JSON-RPC 请求（同时提取 ID）
        let parse_result: Result<MethodCallWithId, _> =
            serde_json::from_str::<JsonRpcRequest>(&line)
                .map(|req| MethodCallWithId {
                    id: req.id,
                    call: req.method,
                });

        let response_json = match parse_result {
            Ok(ctx) => {
                let MethodCallWithId { id, call } = ctx;
                let server = server.read().await;
                match server.handle_call(call).await {
                    Ok(result) => serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": result
                    }),
                    Err(e) => serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {
                            "code": e.code.code(),
                            "message": e.message
                        }
                    }),
                }
            }
            Err(e) => serde_json::json!({
                "jsonrpc": "2.0",
                "id": Id::Null,
                "error": {
                    "code": -32700,
                    "message": format!("Parse error: {}", e)
                }
            }),
        };

        // 输出响应
        let _ = writeln!(stdout, "{}", response_json);
        let _ = stdout.flush();
    }
}

/// JSON-RPC 请求结构（用于同时提取 id 和 method）
#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: Version,
    id: Id,
    #[serde(flatten)]
    method: MethodCall,
}

fn _assert_deserialize_flatten() {
    // Verify JsonRpcRequest can be parsed from a JSON-RPC request
    let _ = serde_json::from_str::<JsonRpcRequest>(r#"{"jsonrpc":"2.0","id":1,"method":"foo","params":{}}"#);
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
