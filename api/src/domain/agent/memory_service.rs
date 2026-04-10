// Memory Service - 设备状态快照服务
// 用于 AI Agent 记忆设备状态，支持上下文理解

use std::sync::Arc;

use crate::domain::agent::device_memory::DeviceMemory;
use crate::infrastructure::persistence::repositories::device_memory_repository_impl::DeviceMemoryRepository;

/// Memory Service - 管理设备状态快照
pub struct MemoryService {
    repo: Arc<dyn DeviceMemoryRepository>,
}

impl MemoryService {
    pub fn new(repo: Arc<dyn DeviceMemoryRepository>) -> Self {
        Self { repo }
    }

    /// 保存设备状态快照
    pub async fn save_device_snapshot(
        &self,
        workspace_id: &str,
        agent_id: &str,
        device_id: &str,
        snapshot_data: serde_json::Value,
    ) -> Result<(), String> {
        let memory = DeviceMemory::new(
            workspace_id.to_string(),
            agent_id.to_string(),
            device_id.to_string(),
            snapshot_data,
        );
        self.repo
            .save(&memory)
            .await
            .map_err(|e| e.to_string())
    }

    /// 获取设备的最新快照
    pub async fn get_latest_device(
        &self,
        workspace_id: &str,
        agent_id: &str,
        device_id: &str,
    ) -> Result<Option<serde_json::Value>, String> {
        let memory = self
            .repo
            .get_latest(workspace_id, agent_id, device_id)
            .await
            .map_err(|e| e.to_string())?;
        Ok(memory.and_then(|m| serde_json::from_str(&m.snapshot_data).ok()))
    }

    /// 构建 Memory Prompt 片段（用于注入 system prompt）
    pub async fn build_memory_prompt(
        &self,
        workspace_id: &str,
        agent_id: &str,
    ) -> Result<String, String> {
        let memories = self
            .repo
            .get_all_for_agent(workspace_id, agent_id)
            .await
            .map_err(|e| e.to_string())?;

        if memories.is_empty() {
            return Ok(String::new());
        }

        let mut prompt = String::from("\n\n## 设备状态记忆\n");
        for mem in memories {
            if let Ok(data) =
                serde_json::from_str::<serde_json::Value>(&mem.snapshot_data)
            {
                let time = chrono::DateTime::from_timestamp_millis(mem.snapshot_time)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_default();
                prompt.push_str(&format!(
                    "\n[{}] 设备 {}: {}\n",
                    time,
                    mem.device_id,
                    data
                ));
            }
        }
        Ok(prompt)
    }

    /// 清理旧快照（保留最近 N 条）
    pub async fn prune_old_snapshots(
        &self,
        workspace_id: &str,
        agent_id: &str,
        device_id: &str,
        keep_count: i64,
    ) -> Result<u64, String> {
        self.repo
            .delete_old(workspace_id, agent_id, device_id, keep_count)
            .await
            .map_err(|e| e.to_string())
    }
}
