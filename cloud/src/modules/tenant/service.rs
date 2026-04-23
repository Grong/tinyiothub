use std::sync::Arc;

use super::repo::TenantRepository;
use super::types::{
    ApiKey, ApiUsageStats, CreateApiKeyRequest, CreateTenantRequest, SubscriptionPlan, Tenant,
    TenantUsage,
};
use crate::shared::error::{Error, Result};

// Resource type constants for quota checking
const RESOURCE_TYPE_DEVICE: &str = "device";
const RESOURCE_TYPE_API_CALL: &str = "api_call";
const RESOURCE_TYPE_USER: &str = "user";

/// Tenant domain service
pub struct TenantService {
    repository: Arc<dyn TenantRepository>,
}

impl TenantService {
    pub fn new(repository: Arc<dyn TenantRepository>) -> Self {
        Self { repository }
    }

    /// Find all subscription plans
    pub async fn find_all_plans(&self) -> Result<Vec<SubscriptionPlan>> {
        self.repository.find_all_plans().await
    }

    /// Find a subscription plan by ID
    pub async fn find_plan_by_id(&self, id: &str) -> Result<Option<SubscriptionPlan>> {
        self.repository.find_plan_by_id(id).await
    }

    /// Create a new tenant
    pub async fn create_tenant(&self, req: &CreateTenantRequest) -> Result<Tenant> {
        self.repository.create_tenant(req).await
    }

    /// Find a tenant by ID
    pub async fn find_tenant_by_id(&self, id: &str) -> Result<Option<Tenant>> {
        self.repository.find_tenant_by_id(id).await
    }

    /// Find a tenant by slug
    pub async fn find_tenant_by_slug(&self, slug: &str) -> Result<Option<Tenant>> {
        self.repository.find_tenant_by_slug(slug).await
    }

    /// Get tenant usage statistics
    pub async fn get_tenant_usage(&self, tenant_id: &str) -> Result<Option<TenantUsage>> {
        self.repository.get_tenant_usage(tenant_id).await
    }

    /// Check if tenant has quota for a resource
    pub async fn check_quota(&self, tenant_id: &str, resource: &str) -> Result<bool> {
        let tenant = self
            .repository
            .find_tenant_by_id(tenant_id)
            .await?
            .ok_or(Error::NotFound)?;
        let plan = self
            .repository
            .find_plan_by_id(&tenant.plan_id)
            .await?
            .ok_or(Error::NotFound)?;
        let usage = self
            .repository
            .get_tenant_usage(tenant_id)
            .await?
            .ok_or(Error::NotFound)?;

        match resource {
            RESOURCE_TYPE_DEVICE => Ok(self.check_resource_quota(plan.device_limit, usage.device_count)),
            RESOURCE_TYPE_API_CALL => Ok(self.check_resource_quota(plan.api_call_limit, usage.api_call_count)),
            RESOURCE_TYPE_USER => Ok(self.check_resource_quota(plan.user_limit, usage.user_count)),
            _ => Ok(false),
        }
    }

    /// Helper function to check if usage is under limit (limit = 0 means unlimited)
    fn check_resource_quota(&self, limit: i32, usage: i32) -> bool {
        if limit == 0 {
            return true;
        }
        usage < limit
    }

    /// Change tenant subscription plan
    pub async fn change_plan(&self, tenant_id: &str, plan_id: &str) -> Result<Tenant> {
        self.repository.change_plan(tenant_id, plan_id).await
    }

    /// Suspend a tenant
    pub async fn suspend_tenant(&self, tenant_id: &str) -> Result<Tenant> {
        self.repository.suspend_tenant(tenant_id).await
    }

    /// Activate a tenant
    pub async fn activate_tenant(&self, tenant_id: &str) -> Result<Tenant> {
        self.repository.activate_tenant(tenant_id).await
    }

    /// Create a new API key
    pub async fn create_api_key(
        &self,
        workspace_id: &str,
        req: &CreateApiKeyRequest,
    ) -> Result<(ApiKey, String)> {
        self.repository.create_api_key(workspace_id, req).await
    }

    /// Find an API key by ID
    pub async fn find_api_key_by_id(&self, id: &str) -> Result<Option<ApiKey>> {
        self.repository.find_api_key_by_id(id).await
    }

    /// Find an API key by prefix
    pub async fn find_api_key_by_prefix(&self, prefix: &str) -> Result<Option<ApiKey>> {
        self.repository.find_api_key_by_prefix(prefix).await
    }

    /// Find an API key by hash
    pub async fn find_api_key_by_hash(&self, key_hash: &str) -> Result<Option<ApiKey>> {
        self.repository.find_api_key_by_hash(key_hash).await
    }

    /// Find all API keys for a workspace
    pub async fn find_api_keys_by_workspace(&self, workspace_id: &str) -> Result<Vec<ApiKey>> {
        self.repository.find_api_keys_by_workspace(workspace_id).await
    }

    /// Revoke an API key
    pub async fn revoke_api_key(&self, id: &str) -> Result<()> {
        self.repository.revoke_api_key(id).await
    }

    /// Enable an API key
    pub async fn enable_api_key(&self, id: &str) -> Result<()> {
        self.repository.enable_api_key(id).await
    }

    /// Disable an API key
    pub async fn disable_api_key(&self, id: &str) -> Result<()> {
        self.repository.disable_api_key(id).await
    }

    /// Record API usage
    pub async fn record_api_usage(
        &self,
        workspace_id: &str,
        api_key_id: Option<&str>,
        method: &str,
        path: &str,
        status_code: i32,
        latency_ms: i32,
        ip_address: Option<&str>,
    ) -> Result<()> {
        self.repository
            .record_api_usage(workspace_id, api_key_id, method, path, status_code, latency_ms, ip_address)
            .await
    }

    /// Get API usage statistics
    pub async fn get_api_usage_stats(&self, tenant_id: &str, days: i32) -> Result<ApiUsageStats> {
        self.repository.get_api_usage_stats(tenant_id, days).await
    }
}
