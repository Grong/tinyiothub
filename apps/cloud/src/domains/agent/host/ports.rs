// 数据实现，留 cloud（D2）
//! Composition ports — the seams where the composition layer (cloud) plugs
//! capabilities into the agent host without the agent crate depending on
//! not-yet-extracted or downstream domains.
//!
//! Task 14 后：通用缝（ExternalTool* traits / default_model / minimax
//! settings）已迁入 `tinyiothub_agent`（`tools::external` / `config` /
//! `pool::provider`）；本文件只剩带数据/下游依赖的组合缝：
//!
//! - [`external_tool_registry`] — 从 MCP registry 按需派生
//!   `tinyiothub_agent::tools::ExternalToolRegistry` 适配器（G3 —— 无注册
//!   静态，MCP_REGISTRY 是单一事实源）。
//! - [`WorkspaceAccess`] — workspace existence + tenant ownership lookup used
//!   by HTTP handlers. Implemented in cloud over
//!   `crate::domains::tenant::WorkspaceService` (tenant → agent edge stays one-way;
//!   agent must not depend on the tenant crate).
//! - [`StorageAutonomyPolicyReader`] — 桥 `db` crate 的 `PolicyRepository`
//!   到 crates/agent 的自治策略读取端口（Task 13）。

use std::sync::Arc;

use async_trait::async_trait;

use tinyiothub_agent::tools::ExternalToolRegistry;

// ---------------------------------------------------------------------------
// External tool registry (MCP seam)
// ---------------------------------------------------------------------------

/// The external tool registry, derived on demand from the MCP tool registry
/// (G3 — OnceLock eliminated; MCP_REGISTRY is the single source of truth).
pub fn external_tool_registry() -> Option<Arc<dyn ExternalToolRegistry>> {
    crate::domains::mcp::get_mcp_registry().map(|registry| {
        Arc::new(crate::domains::mcp::agent_bridge::McpExternalToolRegistry::new(
            registry,
        )) as Arc<dyn ExternalToolRegistry>
    })
}

// ---------------------------------------------------------------------------
// Workspace access (tenant seam)
// ---------------------------------------------------------------------------

/// Workspace existence + tenant ownership lookup for HTTP handlers.
#[async_trait]
pub trait WorkspaceAccess: Send + Sync {
    /// The owning tenant_id of the workspace, `Ok(None)` when it does not
    /// exist, `Err` on lookup failure.
    async fn workspace_tenant_id(&self, workspace_id: &str) -> Result<Option<String>, String>;
}

/// Verify the workspace exists and belongs to the current tenant. Mirrors the
/// semantics of the tenant crate's `verify_workspace_access!` macro (which the
/// agent crate cannot use without a tenant dependency edge).
///
/// Expands to an early `return` with the same error responses as the original.
#[macro_export]
macro_rules! verify_workspace_access_port {
    ($state:expr, $claims:expr, $id:expr) => {{
        match $state.workspace_access.workspace_tenant_id(&$id).await {
            Ok(Some(tenant_id)) => {
                if tenant_id != $claims.tenant_id {
                    return ApiResponseBuilder::error_with_code(403, "无权访问此工作空间");
                }
            }
            Ok(None) => return ApiResponseBuilder::error_with_code(404, "工作空间不存在"),
            Err(e) => {
                tracing::error!("Failed to get workspace: {}", e);
                return ApiResponseBuilder::error("获取工作空间失败");
            }
        }
    }};
}

// ---------------------------------------------------------------------------
// Autonomy policy reader (storage seam)
// ---------------------------------------------------------------------------

/// `AutonomyPolicyReader` 的 cloud 适配器（Task 13）—— 桥 db crate 的
/// `PolicyRepository` 到 crates/agent 的读取端口：运行时 crate 不再直接
/// 依赖存储实现，组合层在此接线。
pub struct StorageAutonomyPolicyReader {
    repo: Arc<tinyiothub_storage::policy::PolicyRepository>,
}

impl StorageAutonomyPolicyReader {
    pub fn new(repo: Arc<tinyiothub_storage::policy::PolicyRepository>) -> Self {
        Self { repo }
    }
}

#[async_trait]
impl tinyiothub_agent::runtime::thing_agent::traits::AutonomyPolicyReader for StorageAutonomyPolicyReader {
    async fn load_autonomy(
        &self,
        workspace_id: &str,
    ) -> anyhow::Result<Option<tinyiothub_core::policy::AutonomyPolicy>> {
        Ok(self.repo.load_autonomy(workspace_id).await?)
    }
}
