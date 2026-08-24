// Agent memory plane — HTTP handler + PromptMemorySource 接线（组合层）。
pub mod handler;
pub mod types;

/// `PromptMemorySource` 适配器（Task 14）—— newtype 包装存储层
/// `MemoryStore`（orphan rule：trait 与类型都在外侧 crate，组合层以
/// newtype 接线）。prompt 组装只读 list_active / record_load。
pub struct PromptMemoryStoreAdapter(pub std::sync::Arc<tinyiothub_storage::memory::MemoryStore>);

#[async_trait::async_trait]
impl tinyiothub_agent::prompt::PromptMemorySource for PromptMemoryStoreAdapter {
    async fn list_active(
        &self,
        workspace_id: &str,
        agent_id: &str,
    ) -> anyhow::Result<Vec<tinyiothub_core::memory::AgentMemory>> {
        Ok(self.0.list_active_memory_entries(workspace_id, agent_id).await?)
    }

    async fn record_load(&self, id: &str) -> anyhow::Result<()> {
        Ok(self.0.record_load(id).await?)
    }
}
