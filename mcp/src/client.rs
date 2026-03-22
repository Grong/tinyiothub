//! TinyIoTHub API Client
//!
//! 用于 MCP Server 调用 TinyIoTHub REST API

use std::time::Duration;

use reqwest::Client;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, error};

/// 客户端错误类型，携带结构化信息用于精确的 JSON-RPC 错误码映射
#[derive(Debug, Error)]
pub enum ClientError {
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Rate limited: {0}")]
    RateLimited(String),

    #[error("API error {0}: {1}")]
    ApiError(i32, String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Request timed out")]
    Timeout,
}

/// API 统一响应格式（复用 TinyIoTHub 现有结构）
#[derive(Debug, Deserialize)]
pub struct ApiResponse<T> {
    pub msg: String,
    pub code: i32,
    pub result: Option<T>,
}

impl<T> ApiResponse<T> {
    pub fn into_result(self) -> Result<T, ClientError> {
        if self.code == 0 {
            self.result.ok_or_else(|| ClientError::ApiError(0, "Empty result".to_string()))
        } else {
            Err(ClientError::ApiError(self.code, self.msg))
        }
    }
}

/// 设备（复用现有 DTO 结构）
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct Device {
    pub id: String,
    pub name: String,
    pub display_name: Option<String>,
    pub device_type: Option<String>,
    pub address: Option<String>,
    pub description: Option<String>,
    pub position: Option<String>,
    pub driver_name: Option<String>,
    pub device_model: Option<String>,
    pub protocol_type: Option<String>,
    pub state: Option<i32>,
    pub is_online: bool,
    pub last_heartbeat: Option<String>,
    pub properties: Option<Vec<DeviceProperty>>,
}

/// 设备属性（复用现有 DTO）
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceProperty {
    pub id: Option<String>,
    pub device_id: Option<String>,
    pub name: String,
    pub display_name: Option<String>,
    pub data_type: Option<String>,
    pub unit: Option<String>,
    pub current_value: Option<String>,
    pub alarm_status: Option<i32>,
}

/// 设备命令执行结果
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct CommandResult {
    pub success: bool,
    pub message: Option<String>,
}

/// 告警（复用现有 DTO）
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct Alarm {
    pub id: String,
    pub device_id: String,
    pub device_name: Option<String>,
    pub alarm_type: String,
    pub alarm_level: String,
    pub message: String,
    pub status: String,
    pub is_acknowledged: bool,
    pub created_at: String,
}

/// 告警统计（复用现有 DTO）
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AlarmStatistics {
    pub total_count: u64,
    pub active_count: u64,
    pub acknowledged_count: u64,
    pub resolved_count: u64,
}

/// 驱动信息
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct Driver {
    pub name: String,
    pub display_name: Option<String>,
    pub protocol_type: Option<String>,
    pub description: Option<String>,
}

/// TinyIoTHub API 客户端
pub struct TinyIoTHubClient {
    client: Client,
    base_url: String,
    api_key: String,
}

impl TinyIoTHubClient {
    pub fn new(base_url: &str, api_key: &str) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("reqwest client must build");
        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
        }
    }

    /// 发起 API 请求
    async fn request<T: DeserializeOwned + Serialize>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<T, ClientError> {
        let url = format!("{}/api/v1{}", self.base_url, path);

        debug!("API Request: {} {}", method, url);

        let mut request = self
            .client
            .request(method, &url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json");

        if let Some(body) = body {
            request = request.body(body.to_string());
        }

        let response = request.send().await.map_err(|e| {
            if e.is_timeout() {
                ClientError::Timeout
            } else {
                ClientError::NetworkError(e.to_string())
            }
        })?;

        let status = response.status();

        if status.as_u16() == 401 {
            let text = response.text().await.unwrap_or_default();
            error!("API Unauthorized: {}", text);
            return Err(ClientError::Unauthorized(text));
        }

        if status.as_u16() == 404 {
            let text = response.text().await.unwrap_or_default();
            error!("API Not Found: {}", text);
            return Err(ClientError::NotFound(text));
        }

        if status.as_u16() == 429 {
            let text = response.text().await.unwrap_or_default();
            error!("API Rate Limited: {}", text);
            return Err(ClientError::RateLimited(text));
        }

        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            error!("API Error: {} - {}", status, text);
            return Err(ClientError::ApiError(status.as_u16() as i32, text));
        }

        let api_response: ApiResponse<T> = response.json().await.map_err(|e| {
            ClientError::NetworkError(format!("Failed to parse response: {}", e))
        })?;
        api_response.into_result()
    }

    // ==================== 设备 API ====================

    /// 获取设备列表
    pub async fn list_devices(
        &self,
        page: u32,
        page_size: u32,
        include_properties: bool,
    ) -> Result<Vec<Device>, ClientError> {
        let path = format!(
            "/devices?page={}&page_size={}&include_properties={}",
            page, page_size, include_properties
        );
        self.request(reqwest::Method::GET, &path, None).await
    }

    /// 获取设备详情
    pub async fn get_device(
        &self,
        device_id: &str,
        include_properties: bool,
    ) -> Result<Device, ClientError> {
        let path = format!(
            "/devices/{}?include_properties={}",
            device_id, include_properties
        );
        self.request(reqwest::Method::GET, &path, None).await
    }

    /// 读取设备属性（需要新增 API）
    pub async fn read_device_properties(
        &self,
        device_id: &str,
        properties: Option<Vec<String>>,
    ) -> Result<Vec<DeviceProperty>, ClientError> {
        let path = format!("/devices/{}/properties/read", device_id);

        let body = serde_json::json!({
            "properties": properties,
            "timeout_ms": 5000
        });

        self.request(reqwest::Method::POST, &path, Some(body)).await
    }

    /// 发送设备命令
    pub async fn send_command(
        &self,
        device_id: &str,
        command: &str,
        parameters: Option<serde_json::Value>,
    ) -> Result<CommandResult, ClientError> {
        let path = format!("/devices/{}/commands/execute", device_id);

        let mut body = serde_json::json!({
            "command": command
        });

        if let Some(params) = parameters {
            if let Some(obj) = body.as_object_mut() {
                obj.insert("parameters".to_string(), params);
            }
        }

        self.request(reqwest::Method::POST, &path, Some(body)).await
    }

    // ==================== 告警 API ====================

    /// 获取告警列表
    pub async fn list_alarms(
        &self,
        status: &str,
        device_id: Option<&str>,
        limit: u32,
    ) -> Result<Vec<Alarm>, ClientError> {
        let mut path = format!("/alarms?status={}&limit={}", status, limit);

        if let Some(did) = device_id {
            path.push_str(&format!("&device_id={}", did));
        }

        self.request(reqwest::Method::GET, &path, None).await
    }

    /// 确认告警
    pub async fn acknowledge_alarm(
        &self,
        alarm_id: &str,
        comment: Option<&str>,
    ) -> Result<Alarm, ClientError> {
        let path = format!("/alarms/{}/acknowledge", alarm_id);

        let body = serde_json::json!({
            "comment": comment.unwrap_or("")
        });

        self.request(reqwest::Method::POST, &path, Some(body)).await
    }

    /// 获取告警统计
    pub async fn get_alarm_statistics(&self) -> Result<AlarmStatistics, ClientError> {
        self.request(reqwest::Method::GET, "/alarms/statistics", None).await
    }

    // ==================== 驱动 API ====================

    /// 获取驱动列表
    pub async fn list_drivers(&self) -> Result<Vec<Driver>, ClientError> {
        self.request(reqwest::Method::GET, "/drivers", None).await
    }
}
