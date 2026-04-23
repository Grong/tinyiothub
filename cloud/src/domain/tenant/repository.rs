use async_trait::async_trait;

use crate::dto::entity::tenant::{
    ApiKey, ApiUsageStats, CreateApiKeyRequest, CreateTenantRequest, SubscriptionPlan, Tenant,
    TenantUsage,
};
use tinyiothub_core::error::Result;

/// Repository interface for tenant persistence (defined in domain layer)
#[async_trait]
pub trait TenantRepository: Send + Sync {
    /// Find all subscription plans
    async fn find_all_plans(&self) -> Result<Vec<SubscriptionPlan>>;

    /// Find a subscription plan by ID
    async fn find_plan_by_id(&self, id: &str) -> Result<Option<SubscriptionPlan>>;

    /// Create a new tenant
    async fn create_tenant(&self, req: &CreateTenantRequest) -> Result<Tenant>;

    /// Find a tenant by ID
    async fn find_tenant_by_id(&self, id: &str) -> Result<Option<Tenant>>;

    /// Find a tenant by slug
    async fn find_tenant_by_slug(&self, slug: &str) -> Result<Option<Tenant>>;

    /// Get tenant usage statistics
    async fn get_tenant_usage(&self, tenant_id: &str) -> Result<Option<TenantUsage>>;

    /// Change tenant subscription plan
    async fn change_plan(&self, tenant_id: &str, plan_id: &str) -> Result<Tenant>;

    /// Suspend a tenant
    async fn suspend_tenant(&self, tenant_id: &str) -> Result<Tenant>;

    /// Activate a tenant
    async fn activate_tenant(&self, tenant_id: &str) -> Result<Tenant>;

    /// Create a new API key (returns the key and the raw key string)
    async fn create_api_key(
        &self,
        workspace_id: &str,
        req: &CreateApiKeyRequest,
    ) -> Result<(ApiKey, String)>;

    /// Find an API key by ID
    async fn find_api_key_by_id(&self, id: &str) -> Result<Option<ApiKey>>;

    /// Find an API key by prefix
    async fn find_api_key_by_prefix(&self, prefix: &str) -> Result<Option<ApiKey>>;

    /// Find an API key by hash
    async fn find_api_key_by_hash(&self, key_hash: &str) -> Result<Option<ApiKey>>;

    /// Find all API keys for a workspace
    async fn find_api_keys_by_workspace(&self, workspace_id: &str) -> Result<Vec<ApiKey>>;

    /// Revoke an API key
    async fn revoke_api_key(&self, id: &str) -> Result<()>;

    /// Enable an API key
    async fn enable_api_key(&self, id: &str) -> Result<()>;

    /// Disable an API key
    async fn disable_api_key(&self, id: &str) -> Result<()>;

    /// Record API usage
    async fn record_api_usage(
        &self,
        workspace_id: &str,
        api_key_id: Option<&str>,
        method: &str,
        path: &str,
        status_code: i32,
        latency_ms: i32,
        ip_address: Option<&str>,
    ) -> Result<()>;

    /// Get API usage statistics
    async fn get_api_usage_stats(&self, tenant_id: &str, days: i32) -> Result<ApiUsageStats>;
}
