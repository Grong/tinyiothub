use serde::Deserialize;

/// 确认报警请求
#[derive(Debug, Clone, Deserialize)]
pub struct AcknowledgeAlarmRequest {
    pub note: Option<String>,
}

/// 解决报警请求
#[derive(Debug, Clone, Deserialize)]
pub struct ResolveAlarmRequest {
    pub resolution_type: String,
    pub note: Option<String>,
}

/// 批量确认请求
#[derive(Debug, Clone, Deserialize)]
pub struct BatchAcknowledgeRequest {
    pub alarm_ids: Vec<String>,
}

/// 批量解决请求
#[derive(Debug, Clone, Deserialize)]
pub struct BatchResolveRequest {
    pub alarm_ids: Vec<String>,
    pub resolution_type: String,
}

/// 报警查询参数
#[derive(Debug, Clone, Deserialize)]
pub struct AlarmQueryParams {
    pub device_ids: Option<Vec<String>>,
    pub levels: Option<Vec<String>>,
    pub statuses: Option<Vec<String>>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// 创建报警规则请求
#[derive(Debug, Clone, Deserialize)]
pub struct CreateAlarmRuleRequest {
    pub name: String,
    pub description: Option<String>,
    pub device_id: Option<String>,
    pub property_id: Option<String>,
    pub rule_type: crate::domain::alarm::RuleType,
    pub condition: serde_json::Value,
    pub alarm_level: String,
    pub notification_config: serde_json::Value,
}

/// 更新报警规则请求
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateAlarmRuleRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub condition: Option<serde_json::Value>,
    pub alarm_level: Option<String>,
    pub notification_config: Option<serde_json::Value>,
}

/// 切换规则状态请求
#[derive(Debug, Clone, Deserialize)]
pub struct ToggleRuleRequest {
    pub enabled: bool,
}

/// 统计查询参数
#[derive(Debug, Clone, Deserialize)]
pub struct StatisticsQueryParams {
    pub start_time: Option<String>,
    pub end_time: Option<String>,
}
