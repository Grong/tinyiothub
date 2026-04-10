// Device Memory Repository 实现

use sqlx::SqlitePool;

use crate::domain::agent::device_memory::DeviceMemory;

/// SQLite 实现
pub struct SqliteDeviceMemoryRepository {
    pool: SqlitePool,
}

impl SqliteDeviceMemoryRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// 保存设备快照
    pub async fn save(&self, memory: &DeviceMemory) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO device_memory (workspace_id, agent_id, device_id, snapshot_data, snapshot_time)
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(workspace_id, agent_id, device_id) DO UPDATE SET
                snapshot_data = excluded.snapshot_data,
                snapshot_time = excluded.snapshot_time,
                created_at = datetime('now')
            "#,
        )
        .bind(&memory.workspace_id)
        .bind(&memory.agent_id)
        .bind(&memory.device_id)
        .bind(&memory.snapshot_data)
        .bind(memory.snapshot_time)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// 获取设备的最新快照
    pub async fn get_latest(
        &self,
        workspace_id: &str,
        agent_id: &str,
        device_id: &str,
    ) -> Result<Option<DeviceMemory>, sqlx::Error> {
        let row: Option<(i64, String, String, String, String, i64, String)> = sqlx::query_as(
            r#"
            SELECT id, workspace_id, agent_id, device_id, snapshot_data, snapshot_time, created_at
            FROM device_memory
            WHERE workspace_id = ? AND agent_id = ? AND device_id = ?
            ORDER BY snapshot_time DESC
            LIMIT 1
            "#,
        )
        .bind(workspace_id)
        .bind(agent_id)
        .bind(device_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| DeviceMemory {
            id: Some(r.0),
            workspace_id: r.1,
            agent_id: r.2,
            device_id: r.3,
            snapshot_data: r.4,
            snapshot_time: r.5,
            created_at: Some(r.6),
        }))
    }

    /// 获取 Agent 的所有设备快照
    pub async fn get_all_for_agent(
        &self,
        workspace_id: &str,
        agent_id: &str,
    ) -> Result<Vec<DeviceMemory>, sqlx::Error> {
        let rows: Vec<(i64, String, String, String, String, i64, String)> = sqlx::query_as(
            r#"
            SELECT id, workspace_id, agent_id, device_id, snapshot_data, snapshot_time, created_at
            FROM device_memory
            WHERE workspace_id = ? AND agent_id = ?
            ORDER BY snapshot_time DESC
            "#,
        )
        .bind(workspace_id)
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| DeviceMemory {
                id: Some(r.0),
                workspace_id: r.1,
                agent_id: r.2,
                device_id: r.3,
                snapshot_data: r.4,
                snapshot_time: r.5,
                created_at: Some(r.6),
            })
            .collect())
    }

    /// 删除旧快照
    pub async fn delete_old(
        &self,
        workspace_id: &str,
        agent_id: &str,
        device_id: &str,
        keep_count: i64,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            DELETE FROM device_memory
            WHERE workspace_id = ? AND agent_id = ? AND device_id = ?
              AND id NOT IN (
                  SELECT id FROM device_memory
                  WHERE workspace_id = ? AND agent_id = ? AND device_id = ?
                  ORDER BY snapshot_time DESC
                  LIMIT ?
              )
            "#,
        )
        .bind(workspace_id)
        .bind(agent_id)
        .bind(device_id)
        .bind(workspace_id)
        .bind(agent_id)
        .bind(device_id)
        .bind(keep_count)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }
}
