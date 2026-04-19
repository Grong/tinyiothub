use std::sync::Arc;

use tinyiothub_core::models::tenant::{
    ApiKey, ApiUsageStats, CreateApiKeyRequest, CreateTenantRequest, SubscriptionPlan, Tenant,
    TenantUsage,
};
use crate::shared::error::{Error, Result};

use super::repository::TenantRepository;

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
            "device" => {
                if plan.device_limit == 0 {
                    return Ok(true);
                }
                Ok(usage.device_count < plan.device_limit)
            }
            "api_call" => {
                if plan.api_call_limit == 0 {
                    return Ok(true);
                }
                Ok(usage.api_call_count < plan.api_call_limit)
            }
            "user" => {
                if plan.user_limit == 0 {
                    return Ok(true);
                }
                Ok(usage.user_count < plan.user_limit)
            }
            _ => Ok(false),
        }
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
