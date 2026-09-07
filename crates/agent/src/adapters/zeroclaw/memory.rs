//! port Memory → zeroclaw Memory 桥 + zeroclaw → port 反向桥。

use std::sync::Arc;

use crate::port::attribution::Attributable;
use crate::port::memory::{Memory, MemoryCategory, MemoryEntry};

use super::tools::{port_role, zc_role};

/// MemoryEntry 两形状 vendored 时逐字段一致——serde 中转转换。
pub(crate) fn zc_entry(entry: MemoryEntry) -> zeroclaw_api::memory_traits::MemoryEntry {
    let json = serde_json::to_string(&entry).expect("MemoryEntry serde");
    serde_json::from_str(&json).expect("MemoryEntry shape parity with zeroclaw-api")
}

/// zeroclaw MemoryEntry → port MemoryEntry（[`zc_entry`] 的反向）。
pub(crate) fn port_entry(entry: zeroclaw_api::memory_traits::MemoryEntry) -> MemoryEntry {
    let json = serde_json::to_string(&entry).expect("MemoryEntry serde");
    serde_json::from_str(&json).expect("MemoryEntry shape parity with zeroclaw-api")
}

/// MemoryCategory 两形状 serde 表示一致。
pub(crate) fn port_category(category: zeroclaw_api::memory_traits::MemoryCategory) -> MemoryCategory {
    port_category_ref(&category)
}

pub(crate) fn port_category_ref(category: &zeroclaw_api::memory_traits::MemoryCategory) -> MemoryCategory {
    let json = serde_json::to_string(category).expect("MemoryCategory serde");
    serde_json::from_str(&json).expect("MemoryCategory shape parity with zeroclaw-api")
}

/// port MemoryCategory → zeroclaw MemoryCategory。
pub(crate) fn zc_category(category: MemoryCategory) -> zeroclaw_api::memory_traits::MemoryCategory {
    let json = serde_json::to_string(&category).expect("MemoryCategory serde");
    serde_json::from_str(&json).expect("MemoryCategory shape parity with zeroclaw-api")
}

/// port Memory 包装为 zeroclaw Memory（zeroclaw loop 的 builder 接收它）。
///
/// 只实现 port trait 有的 11 个方法；zeroclaw trait 的其余方法（带默认实现）
/// 继承其默认体。
pub struct PortMemoryAsZeroclaw(pub Arc<dyn Memory>);

impl Attributable for PortMemoryAsZeroclaw {
    fn role(&self) -> crate::port::attribution::Role {
        self.0.role()
    }
    fn alias(&self) -> &str {
        self.0.alias()
    }
}

impl zeroclaw_api::attribution::Attributable for PortMemoryAsZeroclaw {
    fn role(&self) -> zeroclaw_api::attribution::Role {
        zc_role(&self.0.role())
    }
    fn alias(&self) -> &str {
        self.0.alias()
    }
}

#[async_trait::async_trait]
impl zeroclaw::memory::Memory for PortMemoryAsZeroclaw {
    fn name(&self) -> &str {
        self.0.name()
    }

    async fn store(
        &self,
        key: &str,
        content: &str,
        category: zeroclaw_api::memory_traits::MemoryCategory,
        session_id: Option<&str>,
    ) -> anyhow::Result<()> {
        self.0.store(key, content, port_category(category), session_id).await
    }

    async fn recall(
        &self,
        query: &str,
        limit: usize,
        session_id: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
    ) -> anyhow::Result<Vec<zeroclaw_api::memory_traits::MemoryEntry>> {
        let entries = self.0.recall(query, limit, session_id, since, until).await?;
        Ok(entries.into_iter().map(zc_entry).collect())
    }

    async fn get(&self, key: &str) -> anyhow::Result<Option<zeroclaw_api::memory_traits::MemoryEntry>> {
        Ok(self.0.get(key).await?.map(zc_entry))
    }

    async fn list(
        &self,
        category: Option<&zeroclaw_api::memory_traits::MemoryCategory>,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<zeroclaw_api::memory_traits::MemoryEntry>> {
        let category = category.map(port_category_ref);
        let entries = self.0.list(category.as_ref(), session_id).await?;
        Ok(entries.into_iter().map(zc_entry).collect())
    }

    async fn forget(&self, key: &str) -> anyhow::Result<bool> {
        self.0.forget(key).await
    }

    async fn forget_for_agent(&self, key: &str, agent_id: &str) -> anyhow::Result<bool> {
        self.0.forget_for_agent(key, agent_id).await
    }

    async fn count(&self) -> anyhow::Result<usize> {
        self.0.count().await
    }

    async fn health_check(&self) -> bool {
        self.0.health_check().await
    }

    async fn store_with_agent(
        &self,
        key: &str,
        content: &str,
        category: zeroclaw_api::memory_traits::MemoryCategory,
        session_id: Option<&str>,
        namespace: Option<&str>,
        importance: Option<f64>,
        agent_id: Option<&str>,
    ) -> anyhow::Result<()> {
        self.0
            .store_with_agent(
                key,
                content,
                port_category(category),
                session_id,
                namespace,
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
    ) -> anyhow::Result<Vec<zeroclaw_api::memory_traits::MemoryEntry>> {
        let entries = self
            .0
            .recall_for_agents(allowed_agent_ids, query, limit, session_id, since, until)
            .await?;
        Ok(entries.into_iter().map(zc_entry).collect())
    }
}

/// zeroclaw Memory 包装为 port Memory（组合层把 zeroclaw 存储实现注入
/// crates/agent 的 port 缝；crates/agent 保持零存储实现不变量）。
///
/// 只直通 port trait 的 11 个方法。
pub struct ZeroclawMemoryAsPort(pub Arc<dyn zeroclaw::memory::Memory>);

impl Attributable for ZeroclawMemoryAsPort {
    fn role(&self) -> crate::port::attribution::Role {
        port_role(self.0.role())
    }
    fn alias(&self) -> &str {
        self.0.alias()
    }
}

#[async_trait::async_trait]
impl Memory for ZeroclawMemoryAsPort {
    fn name(&self) -> &str {
        self.0.name()
    }

    async fn store(
        &self,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
    ) -> anyhow::Result<()> {
        self.0.store(key, content, zc_category(category), session_id).await
    }

    async fn recall(
        &self,
        query: &str,
        limit: usize,
        session_id: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let entries = self.0.recall(query, limit, session_id, since, until).await?;
        Ok(entries.into_iter().map(port_entry).collect())
    }

    async fn get(&self, key: &str) -> anyhow::Result<Option<MemoryEntry>> {
        Ok(self.0.get(key).await?.map(port_entry))
    }

    async fn list(
        &self,
        category: Option<&MemoryCategory>,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let category = category.map(|c| zc_category(c.clone()));
        let entries = self.0.list(category.as_ref(), session_id).await?;
        Ok(entries.into_iter().map(port_entry).collect())
    }

    async fn forget(&self, key: &str) -> anyhow::Result<bool> {
        self.0.forget(key).await
    }

    async fn forget_for_agent(&self, key: &str, agent_id: &str) -> anyhow::Result<bool> {
        self.0.forget_for_agent(key, agent_id).await
    }

    async fn count(&self) -> anyhow::Result<usize> {
        self.0.count().await
    }

    async fn health_check(&self) -> bool {
        self.0.health_check().await
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
        self.0
            .store_with_agent(
                key,
                content,
                zc_category(category),
                session_id,
                namespace,
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
        let entries = self
            .0
            .recall_for_agents(allowed_agent_ids, query, limit, session_id, since, until)
            .await?;
        Ok(entries.into_iter().map(port_entry).collect())
    }
}

/// 用 zeroclaw 存储实现建一个 port Memory（组合层注入入口；行为同
/// 迁移前 pool 自建的 memory：auto_save/hygiene 开，response_cache 不建）。
pub fn create_memory(backend: &str, workspace_dir: &std::path::Path) -> anyhow::Result<Arc<dyn Memory>> {
    let memory_config = zeroclaw::config::schema::MemoryConfig {
        backend: backend.to_string(),
        auto_save: true,
        hygiene_enabled: true,
        response_cache_enabled: true,
        ..Default::default()
    };
    let memory = zeroclaw::memory::create_memory(&memory_config, workspace_dir, None)
        .map_err(|e| anyhow::anyhow!("Failed to create memory backend '{}': {}", backend, e))?;
    Ok(Arc::new(ZeroclawMemoryAsPort(Arc::from(memory))))
}
