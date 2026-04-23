use serde::{Deserialize, Serialize};

/// 设备实体 - 使用 snake_case 数据库字段
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub factory_name: Option<String>,
    pub linked_data: Option<String>,
    pub driver_options: Option<String>,
    pub state: Option<i32>,
    pub parent_id: Option<String>,
    pub product_id: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    /// 关联的标签列表 (不存储在数据库中，通过关联查询获取)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<serde_json::Value>>,
    /// 设备实时属性数据 (不存储在数据库中，由DataServer更新)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<Vec<super::device_property::DeviceProperty>>,
    /// 设备指令列表 (不存储在数据库中，由DataServer加载)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commands: Option<Vec<super::device_command::DeviceCommand>>,
    /// 设备在线状态 (不存储在数据库中，由DataServer更新)
    pub is_online: bool,
    /// 最后心跳时间 (不存储在数据库中，由DataServer更新)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_heartbeat: Option<String>,
}

/// 设备查询参数
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct DeviceQueryParams {
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub device_type: Option<String>,
    pub address: Option<String>,
    pub driver_name: Option<String>,
    pub state: Option<i32>,
    pub parent_id: Option<String>,
    pub product_id: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// 创建设备请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateDeviceRequest {
    pub name: String,
    pub display_name: Option<String>,
    pub device_type: Option<String>,
    pub address: Option<String>,
    pub description: Option<String>,
    pub position: Option<String>,
    pub driver_name: Option<String>,
    pub device_model: Option<String>,
    pub protocol_type: Option<String>,
    pub factory_name: Option<String>,
    pub linked_data: Option<String>,
    pub driver_options: Option<String>,
    pub parent_id: Option<String>,
    pub product_id: Option<String>,
}

/// 更新设备请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateDeviceRequest {
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub device_type: Option<String>,
    pub address: Option<String>,
    pub description: Option<String>,
    pub position: Option<String>,
    pub driver_name: Option<String>,
    pub device_model: Option<String>,
    pub protocol_type: Option<String>,
    pub factory_name: Option<String>,
    pub linked_data: Option<String>,
    pub driver_options: Option<String>,
    pub state: Option<i32>,
    pub parent_id: Option<String>,
    pub product_id: Option<String>,
}

/// 设备统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceStats {
    pub total_devices: i64,
    pub online_devices: i64,
    pub offline_devices: i64,
    pub alarm_devices: i64,
}

/// 设备状态更新记录
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceStatusUpdate {
    pub device_id: String,
    pub state: i32,
    pub is_online: bool,
    pub last_heartbeat: Option<String>,
    pub updated_at: String,
}

impl Device {
    /// 检查设备是否在线
    pub fn is_online(&self) -> bool {
        self.state.is_some_and(|s| s == 1)
    }

    /// 检查设备是否离线
    pub fn is_offline(&self) -> bool {
        self.state.is_none_or(|s| s == 0 || s == 3)
    }

    /// 检查设备是否有告警
    pub fn has_alarm(&self) -> bool {
        self.state.is_some_and(|s| s == 2)
    }

    /// 获取设备状态描述
    pub fn get_state_description(&self) -> &'static str {
        match self.state {
            Some(0) => "离线",
            Some(1) => "在线",
            Some(2) => "告警",
            Some(3) => "故障",
            _ => "未知",
        }
    }

    /// 获取设备显示名称（优先使用 DisplayName，否则使用 Name）
    pub fn get_display_name(&self) -> &str {
        self.display_name.as_ref().unwrap_or(&self.name)
    }

    /// 检查设备是否有父设备
    pub fn has_parent(&self) -> bool {
        self.parent_id.is_some()
    }

    /// 检查设备是否关联了产品
    pub fn has_product(&self) -> bool {
        self.product_id.is_some()
    }

    /// 验证设备配置
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("设备名称不能为空".to_string());
        }

        if self.name.len() > 100 {
            return Err("设备名称长度不能超过100个字符".to_string());
        }

        if let Some(display_name) = &self.display_name {
            if display_name.len() > 200 {
                return Err("显示名称长度不能超过200个字符".to_string());
            }
        }

        if let Some(address) = &self.address {
            if address.len() > 500 {
                return Err("地址长度不能超过500个字符".to_string());
            }
        }

        Ok(())
    }
}

impl Default for Device {
    fn default() -> Self {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: String::new(),
            display_name: None,
            device_type: None,
            address: None,
            description: None,
            position: None,
            driver_name: None,
            device_model: None,
            protocol_type: None,
            factory_name: None,
            linked_data: None,
            driver_options: None,
            state: Some(0), // 默认离线状态
            parent_id: None,
            product_id: None,
            created_at: Some(now.clone()),
            updated_at: Some(now),
            tags: None,           // 默认无标签
            properties: None,     // 默认无属性数据
            commands: None,       // 默认无指令数据
            is_online: false,     // 默认离线
            last_heartbeat: None, // 默认无心跳
        }
    }
}

impl Device {
    /// 获取设备创建时间 - 新API兼容方法
    pub fn created_at(&self) -> chrono::DateTime<chrono::Utc> {
        self.created_at
            .as_ref()
            .and_then(|s| {
                chrono::DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
                    .ok()
                    .map(|dt| dt.with_timezone(&chrono::Utc))
            })
            .unwrap_or_else(chrono::Utc::now)
    }

    /// 获取设备更新时间 - 新API兼容方法
    pub fn updated_at(&self) -> chrono::DateTime<chrono::Utc> {
        self.updated_at
            .as_ref()
            .and_then(|s| {
                chrono::DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
                    .ok()
                    .map(|dt| dt.with_timezone(&chrono::Utc))
            })
            .unwrap_or_else(|| self.created_at())
    }

    /// 检查设备是否启用 - 新API兼容方法
    pub fn enabled(&self) -> bool {
        self.is_online()
    }

    /// 获取设备连接配置 - 新API兼容方法
    pub fn connection_config(&self) -> Option<String> {
        self.driver_options.clone()
    }
}
