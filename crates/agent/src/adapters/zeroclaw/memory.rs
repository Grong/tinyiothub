//! port Memory → zeroclaw Memory 桥。

use std::sync::Arc;

use crate::port::attribution::Attributable;
use crate::port::memory::{Memory, MemoryCategory, MemoryEntry};

use super::tools::zc_role;

/// MemoryEntry 两形状 vendored 时逐字段一致——serde 中转转换。
pub(crate) fn zc_entry(entry: MemoryEntry) -> zeroclaw_api::memory_traits::MemoryEntry {
    let json = serde_json::to_string(&entry).expect("MemoryEntry serde");
    serde_json::from_str(&json).expect("MemoryEntry shape parity with zeroclaw-api")
}

/// MemoryCategory 两形状 serde 表示一致。
fn port_category(category: zeroclaw_api::memory_traits::MemoryCategory) -> MemoryCategory {
    port_category_ref(&category)
}

fn port_category_ref(category: &zeroclaw_api::memory_traits::MemoryCategory) -> MemoryCategory {
    let json = serde_json::to_string(category).expect("MemoryCategory serde");
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
