use serde::{Deserialize, Serialize};

/// 订阅计划
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SubscriptionPlan {
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub description: Option<String>,
    pub device_limit: i32,
    pub api_call_limit: i32,
    pub storage_mb: i32,
    pub user_limit: i32,
    pub price_monthly: f64,
    pub price_yearly: f64,
    pub features: String,
    pub sort_order: i32,
    pub created_at: String,
    pub updated_at: String,
}

/// 租户
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Tenant {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub status: String,
    pub plan_id: String,
    pub subscription_status: String,
    pub trial_expires_at: Option<String>,
    pub billing_email: Option<String>,
    pub billing_contact: Option<String>,
    pub timezone: String,
    pub locale: String,
    pub custom_logo: Option<String>,
    pub custom_theme: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 租户查询参数
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct TenantQueryParams {
    pub status: Option<String>,
    pub plan_id: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// 创建租户请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateTenantRequest {
    pub name: String,
    pub slug: String,
    pub billing_email: Option<String>,
    pub billing_contact: Option<String>,
    pub timezone: Option<String>,
    pub locale: Option<String>,
}

/// 更新租户请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateTenantRequest {
    pub name: Option<String>,
    pub billing_email: Option<String>,
    pub billing_contact: Option<String>,
    pub timezone: Option<String>,
    pub locale: Option<String>,
    pub custom_logo: Option<String>,
    pub custom_theme: Option<String>,
}

/// 租户使用量
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TenantUsage {
    pub id: String,
    pub tenant_id: String,
    pub device_count: i32,
    pub api_call_count: i32,
    pub api_call_reset_at: Option<String>,
    pub storage_used_bytes: i64,
    pub user_count: i32,
    pub total_api_calls: i64,
    pub total_api_errors: i64,
    pub updated_at: String,
}

/// API Key 实体
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ApiKey {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub key_hash: String,
    pub prefix: String,
    pub permissions: String,
    pub rate_limit: i32,
    pub is_enabled: bool,
    pub is_revoked: bool,
    pub last_used_at: Option<String>,
    pub last_used_ip: Option<String>,
    pub request_count: i64,
    pub expires_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 创建 API Key 请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateApiKeyRequest {
    pub workspace_id: String,
    pub name: String,
    pub permissions: Option<Vec<String>>,
    pub rate_limit: Option<i32>,
    pub expires_in_days: Option<i32>,
}

/// API 使用统计
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ApiUsageStats {
    pub total_calls: i64,
    pub success_calls: i64,
    pub error_calls: i64,
    pub avg_latency_ms: f64,
    pub period_start: String,
    pub period_end: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subscription_plan_fields() {
        let plan = SubscriptionPlan {
            id: "plan_basic".to_string(),
            name: "basic".to_string(),
            display_name: "基础版".to_string(),
            description: Some("适合小型项目".to_string()),
            device_limit: 100,
            api_call_limit: 10000,
            storage_mb: 1024,
            user_limit: 5,
            price_monthly: 99.0,
            price_yearly: 990.0,
            features: r#"{"webhook": true}"#.to_string(),
            sort_order: 2,
            created_at: "2026-03-12 00:00:00".to_string(),
            updated_at: "2026-03-12 00:00:00".to_string(),
        };

        assert_eq!(plan.device_limit, 100);
        assert_eq!(plan.price_monthly, 99.0);
    }

    #[test]
    fn test_tenant_fields() {
        let tenant = Tenant {
            id: "tenant-001".to_string(),
            name: "Test Company".to_string(),
            slug: "test-company".to_string(),
            status: "active".to_string(),
            plan_id: "plan_pro".to_string(),
            subscription_status: "active".to_string(),
            trial_expires_at: Some("2026-03-26 00:00:00".to_string()),
            billing_email: Some("billing@test.com".to_string()),
            billing_contact: Some("John Doe".to_string()),
            timezone: "Asia/Shanghai".to_string(),
            locale: "zh-CN".to_string(),
            custom_logo: None,
            custom_theme: None,
            created_at: "2026-03-12 00:00:00".to_string(),
            updated_at: "2026-03-12 00:00:00".to_string(),
        };

        assert_eq!(tenant.slug, "test-company");
        assert_eq!(tenant.status, "active");
    }

    #[test]
    fn test_create_tenant_request() {
        let req = CreateTenantRequest {
            name: "New Company".to_string(),
            slug: "new-company".to_string(),
            billing_email: Some("admin@newcompany.com".to_string()),
            billing_contact: Some("Jane Doe".to_string()),
            timezone: Some("America/New_York".to_string()),
            locale: Some("en-US".to_string()),
        };

        assert_eq!(req.name, "New Company");
        assert_eq!(req.slug, "new-company");
    }

    #[test]
    fn test_api_key_fields() {
        let key = ApiKey {
            id: "key-001".to_string(),
            workspace_id: "ws-001".to_string(),
            name: "Production API".to_string(),
            key_hash: "abc123".to_string(),
            prefix: "sk_live_xxxx".to_string(),
            permissions: r#"["read", "write"]"#.to_string(),
            rate_limit: 60,
            is_enabled: true,
            is_revoked: false,
            last_used_at: Some("2026-03-12 10:00:00".to_string()),
            last_used_ip: Some("192.168.1.1".to_string()),
            request_count: 1000,
            expires_at: None,
            created_at: "2026-03-12 00:00:00".to_string(),
            updated_at: "2026-03-12 00:00:00".to_string(),
        };

        assert_eq!(key.rate_limit, 60);
        assert!(key.is_enabled);
    }

    #[test]
    fn test_create_api_key_request() {
        let req = CreateApiKeyRequest {
            workspace_id: "ws-001".to_string(),
            name: "Test Key".to_string(),
            permissions: Some(vec!["read".to_string()]),
            rate_limit: Some(100),
            expires_in_days: Some(90),
        };

        assert_eq!(req.name, "Test Key");
        assert_eq!(req.rate_limit, Some(100));
    }

    #[test]
    fn test_tenant_usage_fields() {
        let usage = TenantUsage {
            id: "usage-001".to_string(),
            tenant_id: "tenant-001".to_string(),
            device_count: 50,
            api_call_count: 5000,
            api_call_reset_at: Some("2026-04-01 00:00:00".to_string()),
            storage_used_bytes: 1024000000,
            user_count: 5,
            total_api_calls: 50000,
            total_api_errors: 100,
            updated_at: "2026-03-12 12:00:00".to_string(),
        };

        assert_eq!(usage.device_count, 50);
        assert_eq!(usage.api_call_count, 5000);
    }

    #[test]
    fn test_api_usage_stats() {
        let stats = ApiUsageStats {
            total_calls: 10000,
            success_calls: 9900,
            error_calls: 100,
            avg_latency_ms: 45.5,
            period_start: "2026-03-01 00:00:00".to_string(),
            period_end: "2026-03-12 23:59:59".to_string(),
        };

        assert_eq!(stats.total_calls, 10000);
        assert!(stats.avg_latency_ms > 0.0);
    }

    #[test]
    fn test_plan_features_json() {
        let features = r#"{
            "webhook": true,
            "sms": true,
            "email": false,
            "api_access": true,
            "custom_brand": false
        }"#;

        let parsed: serde_json::Value = serde_json::from_str(features).unwrap();
        assert!(parsed["webhook"].as_bool().unwrap_or(false));
        assert!(parsed["sms"].as_bool().unwrap_or(false));
    }
}
