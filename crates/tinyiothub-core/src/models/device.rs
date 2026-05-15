use serde::{Deserialize, Serialize};

/// 设备状态枚举
///
/// 序列化输出小写字符串（"online"/"offline"/"error"），与前端统一。
/// 数据库存储为整数：Online=1, Offline=0, Error=2。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum DeviceStatus {
    #[default]
    Offline,
    Online,
    Error,
}

impl DeviceStatus {
    /// 检查是否在线
    pub fn is_online(&self) -> bool {
        *self == Self::Online
    }

    /// 检查是否可用（与 is_online 语义相同，保持兼容）
    pub fn is_available(&self) -> bool {
        *self == Self::Online
    }

    /// 检查是否为错误状态
    pub fn is_error(&self) -> bool {
        *self == Self::Error
    }

    /// 获取字符串表示
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Online => "online",
            Self::Offline => "offline",
            Self::Error => "error",
        }
    }

    /// 从字符串解析
    pub fn parse_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "online" => Some(Self::Online),
            "offline" => Some(Self::Offline),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

impl From<i32> for DeviceStatus {
    fn from(value: i32) -> Self {
        match value {
            1 => Self::Online,
            2 | 3 => Self::Error,
            _ => Self::Offline,
        }
    }
}

impl From<DeviceStatus> for i32 {
    fn from(status: DeviceStatus) -> Self {
        match status {
            DeviceStatus::Online => 1,
            DeviceStatus::Offline => 0,
            DeviceStatus::Error => 2,
        }
    }
}

impl std::fmt::Display for DeviceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Online => write!(f, "online"),
            Self::Offline => write!(f, "offline"),
            Self::Error => write!(f, "error"),
        }
    }
}

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
    pub status: DeviceStatus,
    pub parent_id: Option<String>,
    pub linked_gateway: Option<String>,
    pub fingerprint: Option<String>,
    pub product_id: Option<String>,
    pub workspace_id: Option<String>,
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
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
    pub linked_gateway: Option<String>,
    pub fingerprint: Option<String>,
    pub product_id: Option<String>,
    #[serde(default)]
    pub workspace_id: Option<String>,
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
    pub linked_gateway: Option<String>,
    pub fingerprint: Option<String>,
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
    pub last_heartbeat: Option<String>,
    pub updated_at: String,
}

impl DeviceStatusUpdate {
    pub fn is_online(&self) -> bool {
        self.state == 1
    }
}

impl Device {
    /// 检查设备是否在线
    pub fn is_online(&self) -> bool {
        self.status.is_online()
    }

    /// 获取设备状态描述
    pub fn get_state_description(&self) -> &'static str {
        match self.status {
            DeviceStatus::Online => "在线",
            DeviceStatus::Offline => "离线",
            DeviceStatus::Error => "故障",
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

        if let Some(display_name) = &self.display_name
            && display_name.len() > 200
        {
            return Err("显示名称长度不能超过200个字符".to_string());
        }

        if let Some(address) = &self.address
            && address.len() > 500
        {
            return Err("地址长度不能超过500个字符".to_string());
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
            status: DeviceStatus::Offline,
            parent_id: None,
            linked_gateway: None,
            fingerprint: None,
            product_id: None,
            workspace_id: None,
            created_at: Some(now.clone()),
            updated_at: Some(now),
            tags: None,           // 默认无标签
            properties: None,     // 默认无属性数据
            commands: None,       // 默认无指令数据
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_status_string_conversion() {
        assert_eq!(DeviceStatus::Online.as_str(), "online");
        assert_eq!(DeviceStatus::Offline.as_str(), "offline");
        assert_eq!(DeviceStatus::Error.as_str(), "error");

        assert_eq!(DeviceStatus::parse_str("online"), Some(DeviceStatus::Online));
        assert_eq!(DeviceStatus::parse_str("OFFLINE"), Some(DeviceStatus::Offline));
        assert_eq!(DeviceStatus::parse_str("invalid"), None);
    }

    #[test]
    fn test_device_status_properties() {
        assert!(DeviceStatus::Online.is_available());
        assert!(!DeviceStatus::Offline.is_available());
        assert!(!DeviceStatus::Error.is_available());

        assert!(!DeviceStatus::Online.is_error());
        assert!(!DeviceStatus::Offline.is_error());
        assert!(DeviceStatus::Error.is_error());

        assert!(DeviceStatus::Online.is_online());
        assert!(!DeviceStatus::Offline.is_online());
        assert!(!DeviceStatus::Error.is_online());
    }

    #[test]
    fn test_device_status_display() {
        assert_eq!(format!("{}", DeviceStatus::Online), "online");
        assert_eq!(format!("{}", DeviceStatus::Error), "error");
    }

    #[test]
    fn test_device_status_default() {
        assert_eq!(DeviceStatus::default(), DeviceStatus::Offline);
    }

    #[test]
    fn test_device_status_i32_conversion() {
        assert_eq!(i32::from(DeviceStatus::Online), 1);
        assert_eq!(i32::from(DeviceStatus::Offline), 0);
        assert_eq!(i32::from(DeviceStatus::Error), 2);

        assert_eq!(DeviceStatus::from(1), DeviceStatus::Online);
        assert_eq!(DeviceStatus::from(0), DeviceStatus::Offline);
        assert_eq!(DeviceStatus::from(2), DeviceStatus::Error);
        assert_eq!(DeviceStatus::from(3), DeviceStatus::Error);
    }

    #[test]
    fn test_device_default() {
        let device = Device::default();
        assert!(device.id.len() > 0);
        assert_eq!(device.name, "");
        assert_eq!(device.status, DeviceStatus::Offline);
        assert!(device.created_at.is_some());
        assert!(device.updated_at.is_some());
        assert!(device.tags.is_none());
        assert!(device.properties.is_none());
        assert!(device.commands.is_none());
        assert!(device.last_heartbeat.is_none());
    }

    #[test]
    fn test_device_is_online() {
        let mut device = Device::default();
        device.status = DeviceStatus::Online;
        assert!(device.is_online());
        assert!(device.enabled());

        device.status = DeviceStatus::Offline;
        assert!(!device.is_online());
        assert!(!device.enabled());
    }

    #[test]
    fn test_device_get_display_name() {
        let mut device = Device::default();
        device.name = "sensor-01".to_string();
        assert_eq!(device.get_display_name(), "sensor-01");

        device.display_name = Some("Temperature Sensor".to_string());
        assert_eq!(device.get_display_name(), "Temperature Sensor");
    }

    #[test]
    fn test_device_has_parent() {
        let mut device = Device::default();
        assert!(!device.has_parent());

        device.parent_id = Some("parent-001".to_string());
        assert!(device.has_parent());
    }

    #[test]
    fn test_device_has_product() {
        let mut device = Device::default();
        assert!(!device.has_product());

        device.product_id = Some("prod-001".to_string());
        assert!(device.has_product());
    }

    #[test]
    fn test_device_validate_success() {
        let mut device = Device::default();
        device.name = "valid-device".to_string();
        assert!(device.validate().is_ok());
    }

    #[test]
    fn test_device_validate_empty_name() {
        let device = Device::default();
        assert!(device.validate().is_err());
    }

    #[test]
    fn test_device_validate_whitespace_name() {
        let mut device = Device::default();
        device.name = "   ".to_string();
        assert!(device.validate().is_err());
    }

    #[test]
    fn test_device_validate_long_name() {
        let mut device = Device::default();
        device.name = "a".repeat(101);
        assert!(device.validate().is_err());
    }

    #[test]
    fn test_device_validate_long_display_name() {
        let mut device = Device::default();
        device.name = "valid".to_string();
        device.display_name = Some("b".repeat(201));
        assert!(device.validate().is_err());
    }

    #[test]
    fn test_device_validate_long_address() {
        let mut device = Device::default();
        device.name = "valid".to_string();
        device.address = Some("c".repeat(501));
        assert!(device.validate().is_err());
    }

    #[test]
    fn test_device_get_state_description() {
        let mut device = Device::default();
        device.status = DeviceStatus::Online;
        assert_eq!(device.get_state_description(), "在线");

        device.status = DeviceStatus::Offline;
        assert_eq!(device.get_state_description(), "离线");

        device.status = DeviceStatus::Error;
        assert_eq!(device.get_state_description(), "故障");
    }

    #[test]
    fn test_device_status_update_is_online() {
        let update = DeviceStatusUpdate {
            device_id: "d1".to_string(),
            state: 1,
            last_heartbeat: None,
            updated_at: "2024-01-01 00:00:00".to_string(),
        };
        assert!(update.is_online());

        let update_offline = DeviceStatusUpdate {
            device_id: "d1".to_string(),
            state: 0,
            last_heartbeat: None,
            updated_at: "2024-01-01 00:00:00".to_string(),
        };
        assert!(!update_offline.is_online());
    }

    #[test]
    fn test_device_serialization_roundtrip() {
        let device = Device {
            name: "test-device".to_string(),
            display_name: Some("Test Device".to_string()),
            status: DeviceStatus::Online,
            ..Device::default()
        };

        let json = serde_json::to_string(&device).unwrap();
        let deserialized: Device = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.name, "test-device");
        assert_eq!(deserialized.display_name, Some("Test Device".to_string()));
        assert_eq!(deserialized.status, DeviceStatus::Online);
    }
}
