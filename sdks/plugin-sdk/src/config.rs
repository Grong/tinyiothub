//! 驱动配置管理

use crate::Device;
use std::collections::HashMap;

/// 驱动配置管理器
#[derive(Debug, Clone)]
pub struct DriverConfig {
    config: HashMap<String, String>,
}

impl DriverConfig {
    /// 从设备信息创建配置管理器
    pub fn from_device(device: &Device) -> Self {
        let mut config = HashMap::new();

        if let Some(ref driver_options) = device.driver_options {
            if let Ok(parsed) = serde_json::from_str::<HashMap<String, serde_json::Value>>(driver_options) {
                for (key, value) in parsed {
                    config.insert(key, value.to_string().trim_matches('"').to_string());
                }
            }
        }

        Self { config }
    }

    /// 获取字符串类型配置参数
    pub fn get_string(&self, key: &str, default: &str) -> String {
        self.config.get(key).cloned().unwrap_or_else(|| default.to_string())
    }

    /// 获取整数类型配置参数
    pub fn get_integer(&self, key: &str, default: i64) -> i64 {
        self.config
            .get(key)
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(default)
    }

    /// 获取浮点数类型配置参数
    pub fn get_float(&self, key: &str, default: f64) -> f64 {
        self.config
            .get(key)
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(default)
    }

    /// 获取布尔类型配置参数
    pub fn get_boolean(&self, key: &str, default: bool) -> bool {
        self.config
            .get(key)
            .and_then(|v| match v.to_lowercase().as_str() {
                "true" | "1" | "yes" | "on" => Some(true),
                "false" | "0" | "no" | "off" => Some(false),
                _ => v.parse::<bool>().ok(),
            })
            .unwrap_or(default)
    }

    /// 获取配置参数值
    pub fn get_value(&self, key: &str) -> Option<&String> {
        self.config.get(key)
    }

    /// 检查是否包含指定的配置参数
    pub fn contains_key(&self, key: &str) -> bool {
        self.config.contains_key(key)
    }
}
