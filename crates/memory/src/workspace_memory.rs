// WorkspaceScopedMemory — namespace-based memory isolation for workspaces
//
// Replaces zeroclaw's NamespacedMemory (removed in v0.8.1).
// Wraps a Memory backend and scopes all operations to a workspace_id namespace.
//
// Design:
//   - store() delegates to store_with_metadata(namespace=workspace_id)
//   - recall() delegates to recall_namespaced(namespace=workspace_id)
//   - get() and list() post-filter by namespace
//   - Other methods delegate to inner

use std::sync::Arc;

use async_trait::async_trait;
use zeroclaw::memory::{Memory, MemoryCategory, MemoryEntry};
use zeroclaw_api::attribution::{Attributable, Role};

/// Decorator that wraps a `Memory` backend with namespace isolation by workspace_id.
pub struct WorkspaceScopedMemory {
    inner: Arc<dyn Memory>,
    namespace: String,
}

impl WorkspaceScopedMemory {
    /// Create a new WorkspaceScopedMemory wrapping an existing memory backend.
    pub fn new(inner: Arc<dyn Memory>, workspace_id: String) -> Self {
        Self { inner, namespace: workspace_id }
    }
}

#[async_trait]
impl Memory for WorkspaceScopedMemory {
    fn name(&self) -> &str {
        self.inner.name()
    }

    async fn store(
        &self,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
    ) -> anyhow::Result<()> {
        self.inner
            .store_with_metadata(key, content, category, session_id, Some(&self.namespace), None)
            .await
    }

    async fn recall(
        &self,
        query: &str,
        limit: usize,
        session_id: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        self.inner.recall_namespaced(&self.namespace, query, limit, session_id, since, until).await
    }

    async fn get(&self, key: &str) -> anyhow::Result<Option<MemoryEntry>> {
        let entry = self.inner.get(key).await?;
        Ok(entry.filter(|e| e.namespace == self.namespace))
    }

    async fn list(
        &self,
        category: Option<&MemoryCategory>,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let entries = self.inner.list(category, session_id).await?;
        Ok(entries.into_iter().filter(|e| e.namespace == self.namespace).collect())
    }

    async fn forget(&self, key: &str) -> anyhow::Result<bool> {
        self.inner.forget(key).await
    }

    async fn forget_for_agent(&self, key: &str, agent_id: &str) -> anyhow::Result<bool> {
        self.inner.forget_for_agent(key, agent_id).await
    }

    async fn store_with_agent(
        &self,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
        namespace: Option<&str>,
        importance: Option<f64>,
        agent_id: Option<&str>,
    ) -> anyhow::Result<()> {
        // Always use our workspace namespace, merging with caller's if provided
        let ns = Some(&*self.namespace);
        self.inner
            .store_with_agent(
                key,
                content,
                category,
                session_id,
                namespace.or(ns),
                importance,
                agent_id,
            )
            .await
    }

    async fn recall_for_agents(
        &self,
        allowed_agent_ids: &[&str],
        query: &str,
        limit: usize,
        session_id: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        self.inner
            .recall_for_agents(allowed_agent_ids, query, limit, session_id, since, until)
            .await
    }

    async fn count(&self) -> anyhow::Result<usize> {
        // Approximate: count across all namespaces. The trait has no
        // namespaced count; this is conservative for limits.
        self.inner.count().await
    }

    async fn health_check(&self) -> bool {
        self.inner.health_check().await
    }
}

// Delegate Attributable to the inner memory
impl Attributable for WorkspaceScopedMemory {
    fn role(&self) -> Role {
        self.inner.role()
    }
    fn alias(&self) -> &str {
        self.inner.alias()
    }
}
