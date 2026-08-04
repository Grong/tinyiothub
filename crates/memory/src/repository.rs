//! SqliteAgentMemoryRepository — SQLite implementation of MemoryStore.
use async_trait::async_trait;
use sqlx::SqlitePool;
use tinyiothub_core::error::Result;
use tinyiothub_core::memory::*;

/// SQLite-backed implementation of [`MemoryStore`].
pub struct SqliteAgentMemoryRepository {
    pool: SqlitePool,
}

impl SqliteAgentMemoryRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MemoryStore for SqliteAgentMemoryRepository {
    async fn put(&self, input: MemoryInput) -> Result<AgentMemory> {
        let id = uuid::Uuid::new_v4().to_string();
        let tags_json = serde_json::to_string(&input.tags).unwrap_or_default();

        sqlx::query(
            "INSERT INTO agent_memories (id, workspace_id, agent_id, zone, content, source, confidence, tags, supersedes, device_id, snapshot_data, snapshot_time, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))",
        )
        .bind(&id)
        .bind(&input.workspace_id)
        .bind(&input.agent_id)
        .bind(input.zone.as_str())
        .bind(&input.content)
        .bind(match input.source {
            MemorySource::User => "user",
            MemorySource::Reflection => "reflection",
            MemorySource::Import => "import",
            MemorySource::System => "system",
            MemorySource::DeviceSnapshot => "device_snapshot",
        })
        .bind(match input.confidence {
            Confidence::High => "high",
            Confidence::Medium => "medium",
            Confidence::Low => "low",
        })
        .bind(&tags_json)
        .bind(&input.supersedes)
        .bind(&input.device_id)
        .bind(&input.snapshot_data)
        .bind(input.snapshot_time)
        .execute(&self.pool)
        .await
        .map_err(|e| tinyiothub_core::error::Error::DatabaseError(e.to_string()))?;

        // Construct return value from input + generated fields (avoid read-after-write race)
        Ok(AgentMemory {
            id,
            workspace_id: input.workspace_id,
            agent_id: input.agent_id,
            zone: input.zone,
            content: input.content,
            source: input.source,
            confidence: input.confidence,
            tags: input.tags,
            pinned: false,
            supersedes: input.supersedes,
            device_id: input.device_id,
            snapshot_data: input.snapshot_data,
            snapshot_time: input.snapshot_time,
            effectiveness: 1.0,
            load_count: 0,
            reference_count: 0,
            created_at: String::new(),
            updated_at: String::new(),
        })
    }

    async fn get(&self, id: &str) -> Result<Option<AgentMemory>> {
        let row = sqlx::query_as::<_, MemoryRow>("SELECT * FROM agent_memories WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| tinyiothub_core::error::Error::DatabaseError(e.to_string()))?;
        Ok(row.map(|r| r.into()))
    }

    async fn get_all(&self, workspace_id: &str, agent_id: &str) -> Result<Vec<AgentMemory>> {
        let rows = sqlx::query_as::<_, MemoryRow>(
            "SELECT * FROM agent_memories WHERE workspace_id = ? AND agent_id = ? ORDER BY created_at DESC",
        )
        .bind(workspace_id)
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| tinyiothub_core::error::Error::DatabaseError(e.to_string()))?;
        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn list_active(&self, workspace_id: &str, agent_id: &str) -> Result<Vec<AgentMemory>> {
        // Push superseded filtering to SQL — avoids O(n²) in-memory transitive closure.
        // Memories whose id appears in any supersedes column are considered replaced.
        let rows = sqlx::query_as::<_, MemoryRow>(
            "SELECT * FROM agent_memories \
             WHERE workspace_id = ? AND agent_id = ? \
             AND source != 'device_snapshot' \
             AND id NOT IN (SELECT supersedes FROM agent_memories WHERE supersedes IS NOT NULL AND workspace_id = ? AND agent_id = ?) \
             ORDER BY pinned DESC, effectiveness DESC",
        )
        .bind(workspace_id)
        .bind(agent_id)
        .bind(workspace_id)
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| tinyiothub_core::error::Error::DatabaseError(e.to_string()))?;
        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn get_since(&self, workspace_id: &str, agent_id: &str, since: &str) -> Result<Vec<AgentMemory>> {
        let rows = sqlx::query_as::<_, MemoryRow>(
            "SELECT * FROM agent_memories WHERE workspace_id = ? AND agent_id = ? AND created_at > ? ORDER BY created_at DESC",
        )
        .bind(workspace_id)
        .bind(agent_id)
        .bind(since)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| tinyiothub_core::error::Error::DatabaseError(e.to_string()))?;
        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn set_pinned(&self, id: &str, pinned: bool) -> Result<()> {
        sqlx::query("UPDATE agent_memories SET pinned = ?, updated_at = datetime('now') WHERE id = ?")
            .bind(pinned as i32)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| tinyiothub_core::error::Error::DatabaseError(e.to_string()))?;
        Ok(())
    }

    async fn record_load(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE agent_memories SET load_count = load_count + 1, updated_at = datetime('now') WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| tinyiothub_core::error::Error::DatabaseError(e.to_string()))?;
        Ok(())
    }

    /// Atomic read-modify-write — no SELECT needed, no race condition.
    async fn record_reference(&self, id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE agent_memories SET \
             load_count = load_count + 1, \
             reference_count = reference_count + 1, \
             effectiveness = 0.5 + 0.5 * (CAST(reference_count + 1 AS REAL) / CAST(load_count + 1 AS REAL)), \
             updated_at = datetime('now') \
             WHERE id = ?",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| tinyiothub_core::error::Error::DatabaseError(e.to_string()))?;
        Ok(())
    }

    async fn get_pending_queue(&self, workspace_id: &str, agent_id: &str) -> Result<Vec<ReflectionQueueItem>> {
        let rows = sqlx::query_as::<_, QueueRow>(
            "SELECT id, workspace_id, agent_id, session_key, candidate_type, candidate_data, status, created_at \
             FROM reflection_queue WHERE workspace_id = ? AND agent_id = ? AND status = 'pending' \
             ORDER BY created_at DESC",
        )
        .bind(workspace_id)
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| tinyiothub_core::error::Error::DatabaseError(e.to_string()))?;
        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn resolve_queue_item(
        &self,
        id: &str,
        workspace_id: &str,
        approved: bool,
        reviewer_note: Option<&str>,
    ) -> Result<()> {
        let status = if approved { "approved" } else { "rejected" };
        sqlx::query(
            "UPDATE reflection_queue SET status = ?, reviewed_at = datetime('now'), reviewer_note = ? WHERE id = ? AND workspace_id = ?",
        )
        .bind(status)
        .bind(reviewer_note)
        .bind(id)
        .bind(workspace_id)
        .execute(&self.pool)
        .await
        .map_err(|e| tinyiothub_core::error::Error::DatabaseError(e.to_string()))?;
        Ok(())
    }

    async fn enqueue_candidate(&self, item: QueueCandidateInput) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO reflection_queue (id, workspace_id, agent_id, session_key, candidate_type, candidate_data) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&item.workspace_id)
        .bind(&item.agent_id)
        .bind(&item.session_key)
        .bind(&item.candidate_type)
        .bind(&item.candidate_data)
        .execute(&self.pool)
        .await
        .map_err(|e| tinyiothub_core::error::Error::DatabaseError(e.to_string()))?;
        Ok(id)
    }

    async fn count_by_source(&self, workspace_id: &str, agent_id: &str, source: MemorySource) -> Result<u64> {
        let source_str = match source {
            MemorySource::User => "user",
            MemorySource::Reflection => "reflection",
            MemorySource::Import => "import",
            MemorySource::System => "system",
            MemorySource::DeviceSnapshot => "device_snapshot",
        };
        let (count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM agent_memories WHERE workspace_id = ? AND agent_id = ? AND source = ?",
        )
        .bind(workspace_id)
        .bind(agent_id)
        .bind(source_str)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| tinyiothub_core::error::Error::DatabaseError(e.to_string()))?;
        Ok(count as u64)
    }
}

// ---------------------------------------------------------------------------
// SQLx row types
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct MemoryRow {
    id: String,
    workspace_id: String,
    agent_id: String,
    zone: String,
    content: String,
    source: String,
    confidence: String,
    tags: String,
    pinned: i32,
    supersedes: Option<String>,
    device_id: Option<String>,
    snapshot_data: Option<String>,
    snapshot_time: Option<i64>,
    effectiveness: f64,
    load_count: i64,
    reference_count: i64,
    created_at: String,
    updated_at: String,
}

impl From<MemoryRow> for AgentMemory {
    fn from(r: MemoryRow) -> Self {
        Self {
            id: r.id,
            workspace_id: r.workspace_id,
            agent_id: r.agent_id,
            zone: match r.zone.as_str() {
                "core" => MemoryZone::Core,
                "work" => MemoryZone::Work,
                "episode" => MemoryZone::Episode,
                _ => MemoryZone::General,
            },
            content: r.content,
            source: match r.source.as_str() {
                "user" => MemorySource::User,
                "reflection" => MemorySource::Reflection,
                "import" => MemorySource::Import,
                "system" => MemorySource::System,
                "device_snapshot" => MemorySource::DeviceSnapshot,
                _ => MemorySource::User,
            },
            confidence: match r.confidence.as_str() {
                "high" => Confidence::High,
                "low" => Confidence::Low,
                _ => Confidence::Medium,
            },
            tags: serde_json::from_str(&r.tags).unwrap_or_default(),
            pinned: r.pinned != 0,
            supersedes: r.supersedes,
            device_id: r.device_id,
            snapshot_data: r.snapshot_data,
            snapshot_time: r.snapshot_time,
            effectiveness: r.effectiveness,
            load_count: r.load_count as u32,
            reference_count: r.reference_count as u32,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct QueueRow {
    id: String,
    workspace_id: String,
    agent_id: String,
    session_key: String,
    candidate_type: String,
    candidate_data: String,
    status: String,
    created_at: String,
}

impl From<QueueRow> for ReflectionQueueItem {
    fn from(r: QueueRow) -> Self {
        Self {
            id: r.id,
            workspace_id: r.workspace_id,
            agent_id: r.agent_id,
            session_key: r.session_key,
            candidate_type: r.candidate_type,
            candidate_data: r.candidate_data,
            status: r.status,
            created_at: r.created_at,
        }
    }
}
