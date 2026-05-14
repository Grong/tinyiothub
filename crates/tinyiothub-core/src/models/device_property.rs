use serde::{Deserialize, Serialize};

/// 设备属性实体
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceProperty {
    pub id: String,
    pub device_id: String,
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub data_type: Option<String>,
    pub unit: Option<String>,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    pub default_value: Option<String>,
    pub is_read_only: i32,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    /// 运行时属性（不存储在数据库中）
    pub current_value: Option<String>,
    /// 告警状态（不存储在数据库中）
    pub alarm_status: Option<i32>,
}

/// 设备属性查询参数
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct DevicePropertyQueryParams {
    pub device_id: Option<String>,
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub data_type: Option<String>,
    pub is_read_only: Option<i32>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// 创建设备属性请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateDevicePropertyRequest {
    pub device_id: String,
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub data_type: Option<String>,
    pub unit: Option<String>,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    pub default_value: Option<String>,
    pub is_read_only: Option<i32>,
}

/// 更新设备属性请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateDevicePropertyRequest {
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub data_type: Option<String>,
    pub unit: Option<String>,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    pub default_value: Option<String>,
    pub is_read_only: Option<i32>,
}

/// 设备属性值更新请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdatePropertyValueRequest {
    pub value: String,
    pub timestamp: Option<String>,
}

/// 设备属性统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DevicePropertyStats {
    pub total_properties: i64,
    pub read_only_properties: i64,
    pub writable_properties: i64,
    pub alarm_properties: i64,
}

/// Value label for enumeration properties
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ValueLabel {
    pub value: String,
    pub label: String,
}

impl ValueLabel {
    pub fn new(value: String, label: String) -> Self {
        Self { value, label }
    }
}

impl DeviceProperty {
    /// 设置属性当前值（运行时数据，不持久化）
    pub fn set_current_value(&mut self, value: String) {
        self.current_value = Some(value);
        self.updated_at = Some(chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string());
    }

    /// 设置当前值（可选）并更新时间戳
    pub fn set_current_value_option(&mut self, value: Option<String>) {
        if value.is_some() {
            self.updated_at = Some(chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string());
        }
        self.current_value = value;
    }

    /// 清除运行时数据（当前值、告警状态）
    /// 注意：不清除 updated_at，因为它是数据库字段
    pub fn clear_runtime_data(&mut self) {
        self.current_value = None;
        self.alarm_status = None;
    }

    /// 获取最后更新时间（使用 updated_at）
    pub fn get_last_update_time(&self) -> Option<&String> {
        self.updated_at.as_ref()
    }

    /// 设置告警状态（运行时数据，不持久化）
    pub fn set_alarm_status(&mut self, status: i32) {
        self.alarm_status = Some(status);
    }

    /// 验证属性值是否在范围内
    pub fn validate_value(&self, value: &str) -> Result<(), String> {
        // 根据数据类型验证值
        match self.data_type.as_deref() {
            Some("int") | Some("integer") => {
                let val: i64 = value.parse().map_err(|_| "无效的整数值".to_string())?;

                if let Some(min) = self.min_value
                    && (val as f64) < min
                {
                    return Err(format!("值 {} 小于最小值 {}", val, min));
                }

                if let Some(max) = self.max_value
                    && (val as f64) > max
                {
                    return Err(format!("值 {} 大于最大值 {}", val, max));
                }
            }
            Some("float") | Some("double") | Some("number") => {
                let val: f64 = value.parse().map_err(|_| "无效的数值".to_string())?;

                if let Some(min) = self.min_value
                    && val < min
                {
                    return Err(format!("值 {} 小于最小值 {}", val, min));
                }

                if let Some(max) = self.max_value
                    && val > max
                {
                    return Err(format!("值 {} 大于最大值 {}", val, max));
                }
            }
            Some("bool") | Some("boolean")
                if !matches!(value.to_lowercase().as_str(), "true" | "false" | "0" | "1") =>
            {
                return Err("无效的布尔值".to_string());
            }
            _ => {
                // 字符串类型或其他类型，暂不验证
            }
        }

        Ok(())
    }
}

impl Default for DeviceProperty {
    fn default() -> Self {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            device_id: String::new(),
            name: String::new(),
            display_name: None,
            description: None,
            data_type: Some("string".to_string()),
            unit: None,
            min_value: None,
            max_value: None,
            default_value: None,
            is_read_only: 0,
            created_at: Some(now.clone()),
            updated_at: Some(now),
            current_value: None,
            alarm_status: None,
        }
    }
}
