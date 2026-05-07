use tinyiothub_web::response::ApiResponseBuilder;
use axum::{extract::State, routing::get, Json, Router};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::{
    shared::api_response::{ApiResponse},
    shared::app_state::AppState,
};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub struct PasswordPolicy {
    pub min_length: Option<u32>,
    pub require_uppercase: Option<bool>,
    pub require_lowercase: Option<bool>,
    pub require_numbers: Option<bool>,
    pub require_special_chars: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub struct SystemFeatures {
    // 系统版本信息
    pub version: Option<String>,
    pub edition: Option<String>,
    pub build_time: Option<String>,

    // 功能开关
    pub enable_device_management: Option<bool>,
    pub enable_alarm_system: Option<bool>,
    pub enable_monitoring: Option<bool>,
    pub enable_user_management: Option<bool>,
    pub enable_system_settings: Option<bool>,

    // API 配置
    pub api_prefix: Option<String>,
    pub public_api_prefix: Option<String>,

    // 系统限制
    pub max_devices: Option<u32>,
    pub max_users: Option<u32>,
    pub max_alarm_rules: Option<u32>,

    // 界面配置
    pub theme: Option<String>,
    pub language: Option<String>,
    pub timezone: Option<String>,

    // 高级功能
    pub enable_advanced_analytics: Option<bool>,
    pub enable_custom_dashboard: Option<bool>,
    pub enable_data_export: Option<bool>,
    pub enable_api_access: Option<bool>,

    // 安全配置
    pub enable_two_factor_auth: Option<bool>,
    pub session_timeout: Option<u32>,
    pub password_policy: Option<PasswordPolicy>,

    // 通知配置
    pub enable_email_notifications: Option<bool>,
    pub enable_sms_notifications: Option<bool>,
    pub enable_webhook_notifications: Option<bool>,

    // 系统状态
    pub system_status: Option<String>,
    pub last_health_check: Option<String>,

    // 许可证信息
    pub license_type: Option<String>,
    pub license_expiry: Option<String>,
    pub licensed_features: Option<Vec<String>>,
}

impl Default for SystemFeatures {
    fn default() -> Self {
        Self {
            version: Some("1.0.0".to_string()),
            edition: Some("Community".to_string()),
            build_time: Some(Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string()),

            // 基础功能默认开启
            enable_device_management: Some(true),
            enable_alarm_system: Some(true),
            enable_monitoring: Some(true),
            enable_user_management: Some(true),
            enable_system_settings: Some(true),

            // API 配置
            api_prefix: Some("/api/v1".to_string()),
            public_api_prefix: Some("/api/public".to_string()),

            // 系统限制 (社区版)
            max_devices: Some(100),
            max_users: Some(10),
            max_alarm_rules: Some(50),

            // 界面配置
            theme: Some("system".to_string()),
            language: Some("zh-Hans".to_string()),
            timezone: Some("Asia/Shanghai".to_string()),

            // 高级功能默认关闭 (需要专业版或企业版)
            enable_advanced_analytics: Some(false),
            enable_custom_dashboard: Some(false),
            enable_data_export: Some(false),
            enable_api_access: Some(false),

            // 安全配置
            enable_two_factor_auth: Some(false),
            session_timeout: Some(3600), // 1小时
            password_policy: Some(PasswordPolicy {
                min_length: Some(8),
                require_uppercase: Some(false),
                require_lowercase: Some(false),
                require_numbers: Some(false),
                require_special_chars: Some(false),
            }),

            // 通知配置
            enable_email_notifications: Some(false),
            enable_sms_notifications: Some(false),
            enable_webhook_notifications: Some(false),

            // 系统状态
            system_status: Some("healthy".to_string()),
            last_health_check: Some(Utc::now().to_rfc3339()),

            // 许可证信息
            license_type: Some("community".to_string()),
            license_expiry: None,
            licensed_features: Some(vec![
                "device-management".to_string(),
                "alarm-system".to_string(),
                "monitoring".to_string(),
                "user-management".to_string(),
                "system-settings".to_string(),
            ]),
        }
    }
}

pub fn create_router() -> Router<AppState> {
    Router::new().route("/features", get(get_system_features))
}

/// 获取系统功能特性
///
/// 返回系统支持的功能特性、版本信息、许可证信息等
/// 这个接口通常用于前端初始化时获取系统能力
async fn get_system_features(State(_state): State<AppState>) -> Json<ApiResponse<SystemFeatures>> {

    let features = SystemFeatures::default();

    tracing::debug!(
        "Retrieved system features: license_type={:?}, version={:?}",
        features.license_type,
        features.version
    );

    ApiResponseBuilder::success(features)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_features_default() {
        let features = SystemFeatures::default();

        assert_eq!(features.version, Some("1.0.0".to_string()));
        assert_eq!(features.edition, Some("Community".to_string()));
        assert_eq!(features.license_type, Some("community".to_string()));
        assert_eq!(features.enable_device_management, Some(true));
        assert_eq!(features.enable_advanced_analytics, Some(false));
        assert_eq!(features.max_devices, Some(100));
    }

    #[test]
    fn test_password_policy_default() {
        let features = SystemFeatures::default();
        let policy = features.password_policy.unwrap();

        assert_eq!(policy.min_length, Some(8));
        assert_eq!(policy.require_uppercase, Some(false));
    }
}
