// Memory Service - Memory context building for AI Agent
//
// This module provides:
// - AgentMemoryService for building context from device snapshots
// - MemoryContext for organizing memory items
// - DeviceSnapshot for individual device state tracking

use std::sync::Arc;
use thiserror::Error;

use crate::domain::agent::device_memory::DeviceMemory;
use crate::infrastructure::persistence::repositories::device_memory_repository_impl::SqliteDeviceMemoryRepository;

/// Errors that can occur during memory operations
#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("Repository error: {0}")]
    RepositoryError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    #[error("Context build failed: {0}")]
    ContextBuildFailed(String),
}

/// A snapshot of device state at a point in time
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeviceSnapshot {
    /// Device ID
    pub device_id: String,
    /// Workspace ID
    pub workspace_id: String,
    /// Agent ID that captured this snapshot
    pub agent_id: String,
    /// Snapshot data as JSON
    pub snapshot_data: serde_json::Value,
    /// Snapshot timestamp (Unix millis)
    pub snapshot_time: i64,
    /// Human-readable timestamp
    pub timestamp_formatted: String,
}

impl DeviceSnapshot {
    /// Create a new device snapshot from domain DeviceMemory
    pub fn from_domain(memory: &DeviceMemory) -> Result<Self, MemoryError> {
        let snapshot_data = memory
            .parse_snapshot()
            .ok_or_else(|| MemoryError::SerializationError(
                format!("Failed to parse snapshot for device {}", memory.device_id)
            ))?;

        let timestamp_formatted = chrono::DateTime::from_timestamp_millis(memory.snapshot_time)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_default();

        Ok(Self {
            device_id: memory.device_id.clone(),
            workspace_id: memory.workspace_id.clone(),
            agent_id: memory.agent_id.clone(),
            snapshot_data,
            snapshot_time: memory.snapshot_time,
            timestamp_formatted,
        })
    }

    /// Format the snapshot for inclusion in a prompt
    pub fn to_prompt_fragment(&self) -> String {
        format!(
            "[{}] Device {}: {}",
            self.timestamp_formatted,
            self.device_id,
            serde_json::to_string(&self.snapshot_data).unwrap_or_default()
        )
    }

    /// Get a specific field from the snapshot data
    pub fn get_field(&self, field: &str) -> Option<&serde_json::Value> {
        self.snapshot_data.get(field)
    }

    /// Check if this snapshot contains data for a specific property
    pub fn has_property(&self, property: &str) -> bool {
        self.snapshot_data.get(property).is_some()
    }
}

/// An individual memory item for the agent
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentMemoryItem {
    /// Memory type: "device_snapshot", "user_preference", "conversation_summary"
    pub item_type: String,
    /// Memory content/key
    pub key: String,
    /// Memory value/data
    pub value: serde_json::Value,
    /// When this memory was created/updated
    pub timestamp: i64,
    /// Relevance score (0.0 - 1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relevance: Option<f32>,
}

impl AgentMemoryItem {
    /// Create a device snapshot memory item
    pub fn device_snapshot(snapshot: &DeviceSnapshot) -> Self {
        Self {
            item_type: "device_snapshot".to_string(),
            key: snapshot.device_id.clone(),
            value: serde_json::json!({
                "workspace_id": snapshot.workspace_id,
                "agent_id": snapshot.agent_id,
                "snapshot": snapshot.snapshot_data,
                "timestamp": snapshot.snapshot_time,
            }),
            timestamp: snapshot.snapshot_time,
            relevance: None,
        }
    }

    /// Create a user preference memory item
    pub fn user_preference(key: impl Into<String>, value: serde_json::Value) -> Self {
        Self {
            item_type: "user_preference".to_string(),
            key: key.into(),
            value,
            timestamp: chrono::Utc::now().timestamp_millis(),
            relevance: Some(1.0),
        }
    }

    /// Create a conversation summary memory item
    pub fn conversation_summary(summary: impl Into<String>, topics: Vec<String>) -> Self {
        Self {
            item_type: "conversation_summary".to_string(),
            key: "latest_summary".to_string(),
            value: serde_json::json!({
                "summary": summary.into(),
                "topics": topics,
            }),
            timestamp: chrono::Utc::now().timestamp_millis(),
            relevance: Some(0.8),
        }
    }
}

/// Complete memory context for an agent session
#[derive(Debug, Clone, Default)]
pub struct MemoryContext {
    /// Device state snapshots
    pub device_snapshots: Vec<DeviceSnapshot>,
    /// User preferences
    pub user_preferences: Vec<AgentMemoryItem>,
    /// Conversation summaries
    pub conversation_summaries: Vec<AgentMemoryItem>,
    /// Other memory items
    pub other_items: Vec<AgentMemoryItem>,
}

impl MemoryContext {
    /// Create an empty memory context
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a device snapshot
    pub fn add_device_snapshot(&mut self, snapshot: DeviceSnapshot) {
        self.device_snapshots.push(snapshot);
    }

    /// Add a memory item
    pub fn add_item(&mut self, item: AgentMemoryItem) {
        match item.item_type.as_str() {
            "device_snapshot" => self.device_snapshots.push(
                DeviceSnapshot::from_domain(&DeviceMemory {
                    id: None,
                    workspace_id: item.value.get("workspace_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    agent_id: item.value.get("agent_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    device_id: item.key.clone(),
                    snapshot_data: item.value.get("snapshot")
                        .map(|v| v.to_string())
                        .unwrap_or_default(),
                    snapshot_time: item.timestamp,
                    created_at: None,
                }).unwrap_or_else(|_| DeviceSnapshot {
                    device_id: item.key.clone(),
                    workspace_id: String::new(),
                    agent_id: String::new(),
                    snapshot_data: item.value.clone(),
                    snapshot_time: item.timestamp,
                    timestamp_formatted: String::new(),
                })
            ),
            "user_preference" => self.user_preferences.push(item),
            "conversation_summary" => self.conversation_summaries.push(item),
            _ => self.other_items.push(item),
        }
    }

    /// Check if context is empty
    pub fn is_empty(&self) -> bool {
        self.device_snapshots.is_empty()
            && self.user_preferences.is_empty()
            && self.conversation_summaries.is_empty()
            && self.other_items.is_empty()
    }

    /// Get total item count
    pub fn total_items(&self) -> usize {
        self.device_snapshots.len()
            + self.user_preferences.len()
            + self.conversation_summaries.len()
            + self.other_items.len()
    }

    /// Build a prompt fragment from all memory
    pub fn to_prompt_fragment(&self) -> String {
        if self.is_empty() {
            return String::new();
        }

        let mut fragments = vec!["\n\n## Context from Memory\n".to_string()];

        // Add device snapshots
        if !self.device_snapshots.is_empty() {
            fragments.push("### Device States\n".to_string());
            for snapshot in &self.device_snapshots {
                fragments.push(snapshot.to_prompt_fragment());
                fragments.push("\n".to_string());
            }
        }

        // Add user preferences
        if !self.user_preferences.is_empty() {
            fragments.push("### User Preferences\n".to_string());
            for pref in &self.user_preferences {
                fragments.push(format!("- {}: {}\n", pref.key, pref.value));
            }
        }

        // Add conversation summaries
        if !self.conversation_summaries.is_empty() {
            fragments.push("### Previous Conversations\n".to_string());
            for summary in &self.conversation_summaries {
                if let Some(summary_text) = summary.value.get("summary").and_then(|v| v.as_str()) {
                    fragments.push(format!("- {}\n", summary_text));
                }
            }
        }

        fragments.concat()
    }

    /// Get snapshots for a specific device
    pub fn get_device_snapshots(&self, device_id: &str) -> Vec<&DeviceSnapshot> {
        self.device_snapshots
            .iter()
            .filter(|s| s.device_id == device_id)
            .collect()
    }

    /// Get the most recent snapshot for a device
    pub fn get_latest_device_snapshot(&self, device_id: &str) -> Option<&DeviceSnapshot> {
        self.device_snapshots
            .iter()
            .filter(|s| s.device_id == device_id)
            .max_by_key(|s| s.snapshot_time)
    }
}

/// Service for managing agent memory and building context
pub struct AgentMemoryService {
    repo: Arc<SqliteDeviceMemoryRepository>,
}

impl AgentMemoryService {
    /// Create a new memory service
    pub fn new(repo: Arc<SqliteDeviceMemoryRepository>) -> Self {
        Self { repo }
    }

    /// Build memory context for a workspace/agent
    pub async fn build_context(
        &self,
        workspace_id: &str,
        agent_id: &str,
    ) -> Result<MemoryContext, MemoryError> {
        let mut context = MemoryContext::new();

        // Load device snapshots
        let memories = self
            .repo
            .get_all_for_agent(workspace_id, agent_id)
            .await
            .map_err(|e| MemoryError::RepositoryError(e.to_string()))?;

        for memory in memories {
            match DeviceSnapshot::from_domain(&memory) {
                Ok(snapshot) => context.add_device_snapshot(snapshot),
                Err(e) => {
                    tracing::warn!("Failed to parse device snapshot: {}", e);
                }
            }
        }

        Ok(context)
    }

    /// Save a device snapshot
    pub async fn save_device_snapshot(
        &self,
        workspace_id: &str,
        agent_id: &str,
        device_id: &str,
        snapshot_data: serde_json::Value,
    ) -> Result<(), MemoryError> {
        let memory = DeviceMemory::new(
            workspace_id.to_string(),
            agent_id.to_string(),
            device_id.to_string(),
            snapshot_data,
        );

        self.repo
            .save(&memory)
            .await
            .map_err(|e| MemoryError::RepositoryError(e.to_string()))
    }

    /// Get the latest snapshot for a device
    pub async fn get_latest_device(
        &self,
        workspace_id: &str,
        agent_id: &str,
        device_id: &str,
    ) -> Result<Option<DeviceSnapshot>, MemoryError> {
        let memory = self
            .repo
            .get_latest(workspace_id, agent_id, device_id)
            .await
            .map_err(|e| MemoryError::RepositoryError(e.to_string()))?;

        match memory {
            Some(m) => Ok(Some(DeviceSnapshot::from_domain(&m)?)),
            None => Ok(None),
        }
    }

    /// Build a memory prompt fragment for injection into system prompt
    pub async fn build_memory_prompt(
        &self,
        workspace_id: &str,
        agent_id: &str,
    ) -> Result<String, MemoryError> {
        let context = self.build_context(workspace_id, agent_id).await?;
        Ok(context.to_prompt_fragment())
    }

    /// Prune old snapshots for a device (keep only N most recent)
    pub async fn prune_old_snapshots(
        &self,
        workspace_id: &str,
        agent_id: &str,
        device_id: &str,
        keep_count: i64,
    ) -> Result<u64, MemoryError> {
        self.repo
            .delete_old(workspace_id, agent_id, device_id, keep_count)
            .await
            .map_err(|e| MemoryError::RepositoryError(e.to_string()))
    }

    /// Get all device IDs that have snapshots for this agent
    pub async fn get_tracked_devices(
        &self,
        workspace_id: &str,
        agent_id: &str,
    ) -> Result<Vec<String>, MemoryError> {
        let memories = self
            .repo
            .get_all_for_agent(workspace_id, agent_id)
            .await
            .map_err(|e| MemoryError::RepositoryError(e.to_string()))?;

        let device_ids: std::collections::HashSet<String> = memories
            .into_iter()
            .map(|m| m.device_id)
            .collect();

        Ok(device_ids.into_iter().collect())
    }

    /// Clear all memory for an agent
    pub async fn clear_agent_memory(
        &self,
        workspace_id: &str,
        agent_id: &str,
    ) -> Result<u64, MemoryError> {
        let devices = self.get_tracked_devices(workspace_id, agent_id).await?;
        let mut total_deleted = 0u64;

        for device_id in devices {
            let count = self
                .repo
                .delete_old(workspace_id, agent_id, &device_id, 0)
                .await
                .map_err(|e| MemoryError::RepositoryError(e.to_string()))?;
            total_deleted += count;
        }

        Ok(total_deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_snapshot_from_domain() {
        let memory = DeviceMemory::new(
            "ws-123".to_string(),
            "agent-456".to_string(),
            "device-789".to_string(),
            serde_json::json!({"temperature": 25.5, "status": "online"}),
        );

        let snapshot = DeviceSnapshot::from_domain(&memory).unwrap();

        assert_eq!(snapshot.device_id, "device-789");
        assert_eq!(snapshot.workspace_id, "ws-123");
        assert_eq!(snapshot.agent_id, "agent-456");
        assert_eq!(snapshot.snapshot_data.get("temperature").unwrap().as_f64().unwrap(), 25.5);
    }

    #[test]
    fn test_device_snapshot_to_prompt_fragment() {
        let snapshot = DeviceSnapshot {
            device_id: "dev-1".to_string(),
            workspace_id: "ws".to_string(),
            agent_id: "agent".to_string(),
            snapshot_data: serde_json::json!({"temp": 25}),
            snapshot_time: 1234567890000,
            timestamp_formatted: "2009-02-13 23:31:30".to_string(),
        };

        let fragment = snapshot.to_prompt_fragment();
        assert!(fragment.contains("dev-1"));
        assert!(fragment.contains("2009-02-13"));
        assert!(fragment.contains("temp"));
    }

    #[test]
    fn test_memory_context_empty() {
        let context = MemoryContext::new();
        assert!(context.is_empty());
        assert_eq!(context.total_items(), 0);
    }

    #[test]
    fn test_memory_context_add_snapshot() {
        let mut context = MemoryContext::new();
        let snapshot = DeviceSnapshot {
            device_id: "dev-1".to_string(),
            workspace_id: "ws".to_string(),
            agent_id: "agent".to_string(),
            snapshot_data: serde_json::json!({"temp": 25}),
            snapshot_time: 1234567890000,
            timestamp_formatted: "2009-02-13 23:31:30".to_string(),
        };

        context.add_device_snapshot(snapshot);

        assert!(!context.is_empty());
        assert_eq!(context.total_items(), 1);
        assert_eq!(context.device_snapshots.len(), 1);
    }

    #[test]
    fn test_memory_context_get_device_snapshots() {
        let mut context = MemoryContext::new();

        context.add_device_snapshot(DeviceSnapshot {
            device_id: "dev-1".to_string(),
            workspace_id: "ws".to_string(),
            agent_id: "agent".to_string(),
            snapshot_data: serde_json::json!({"temp": 25}),
            snapshot_time: 1000,
            timestamp_formatted: "time-1".to_string(),
        });

        context.add_device_snapshot(DeviceSnapshot {
            device_id: "dev-2".to_string(),
            workspace_id: "ws".to_string(),
            agent_id: "agent".to_string(),
            snapshot_data: serde_json::json!({"temp": 30}),
            snapshot_time: 2000,
            timestamp_formatted: "time-2".to_string(),
        });

        let dev1_snapshots = context.get_device_snapshots("dev-1");
        assert_eq!(dev1_snapshots.len(), 1);
        assert_eq!(dev1_snapshots[0].device_id, "dev-1");
    }

    #[test]
    fn test_agent_memory_item_helpers() {
        let snapshot_item = AgentMemoryItem::device_snapshot(&DeviceSnapshot {
            device_id: "dev-1".to_string(),
            workspace_id: "ws".to_string(),
            agent_id: "agent".to_string(),
            snapshot_data: serde_json::json!({"temp": 25}),
            snapshot_time: 1000,
            timestamp_formatted: "time".to_string(),
        });
        assert_eq!(snapshot_item.item_type, "device_snapshot");
        assert_eq!(snapshot_item.key, "dev-1");

        let pref_item = AgentMemoryItem::user_preference("theme", serde_json::json!("dark"));
        assert_eq!(pref_item.item_type, "user_preference");
        assert_eq!(pref_item.key, "theme");

        let summary_item = AgentMemoryItem::conversation_summary(
            "Talked about devices",
            vec!["devices".to_string()]
        );
        assert_eq!(summary_item.item_type, "conversation_summary");
        assert_eq!(summary_item.key, "latest_summary");
    }

    #[test]
    fn test_memory_error_display() {
        let err = MemoryError::DeviceNotFound("dev-123".to_string());
        assert!(err.to_string().contains("dev-123"));

        let err = MemoryError::ContextBuildFailed("test".to_string());
        assert!(err.to_string().contains("Context build failed"));
    }
}
