//! Edge 网关本地持久化（Task 13 自 apps/edge 裸 SQL 收编）。
//!
//! 表归属：offline_buffer（断网消息缓冲，FIFO 淘汰）、config_meta
//! （本地配置版本）。两表为 edge 专有，cloud 基线迁移不含。

use sqlx::SqlitePool;

use crate::database::Db;

// ──────────────────────────────────────────────
// 行类型
// ──────────────────────────────────────────────

/// offline_buffer 行（flush 批次用）。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct OfflineBufferRow {
    pub id: i64,
    pub msg_type: String,
    pub topic: String,
    pub payload: Vec<u8>,
}

/// offline_buffer 统计（get_status 用）。
#[derive(Debug, Clone, Default, sqlx::FromRow)]
pub struct OfflineBufferStatusRow {
    pub total_telemetry: i64,
    pub total_alarms: i64,
    pub oldest: Option<i64>,
    pub newest: Option<i64>,
}

// ──────────────────────────────────────────────
// 持久化函数（pub(crate) 自由函数 + Db 委托）
// ──────────────────────────────────────────────

/// 写入一条缓冲消息。
pub(crate) async fn insert_offline_message(
    pool: &SqlitePool,
    msg_type: &str,
    topic: &str,
    payload: &[u8],
    created_at: i64,
    priority: i32,
) -> std::result::Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO offline_buffer (msg_type, topic, payload, created_at, priority) VALUES (?, ?, ?, ?, ?)")
        .bind(msg_type)
        .bind(topic)
        .bind(payload)
        .bind(created_at)
        .bind(priority)
        .execute(pool)
        .await?;

    Ok(())
}

/// 普通优先级（priority = 0）缓冲消息总数。
pub(crate) async fn count_normal_priority_offline(pool: &SqlitePool) -> std::result::Result<i64, sqlx::Error> {
    let count = sqlx::query_scalar("SELECT COUNT(*) FROM offline_buffer WHERE priority = 0")
        .fetch_one(pool)
        .await?;

    Ok(count)
}

/// FIFO 淘汰最老的 n 条普通优先级消息。
pub(crate) async fn evict_oldest_normal_offline(
    pool: &SqlitePool,
    excess: i64,
) -> std::result::Result<(), sqlx::Error> {
    sqlx::query(
        "DELETE FROM offline_buffer WHERE id IN (
            SELECT id FROM offline_buffer WHERE priority = 0 ORDER BY created_at ASC LIMIT ?
        )",
    )
    .bind(excess)
    .execute(pool)
    .await?;

    Ok(())
}

/// 按创建时间正序取一批缓冲消息。
pub(crate) async fn fetch_offline_batch(
    pool: &SqlitePool,
    limit: i64,
) -> std::result::Result<Vec<OfflineBufferRow>, sqlx::Error> {
    let rows = sqlx::query_as::<_, OfflineBufferRow>(
        "SELECT id, msg_type, topic, payload FROM offline_buffer ORDER BY created_at ASC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// 删除一条缓冲消息（确认发布成功后）。
pub(crate) async fn delete_offline_message(pool: &SqlitePool, id: i64) -> std::result::Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM offline_buffer WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    Ok(())
}

/// 发布失败：retry_count + 1，保留行。
pub(crate) async fn increment_offline_retry(pool: &SqlitePool, id: i64) -> std::result::Result<(), sqlx::Error> {
    sqlx::query("UPDATE offline_buffer SET retry_count = retry_count + 1 WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    Ok(())
}

/// 缓冲统计：telemetry/alarm 计数 + 最老/最新时间戳。
pub(crate) async fn offline_buffer_status(
    pool: &SqlitePool,
) -> std::result::Result<OfflineBufferStatusRow, sqlx::Error> {
    let row = sqlx::query_as::<_, OfflineBufferStatusRow>(
        "SELECT COALESCE(SUM(CASE WHEN msg_type = 'telemetry' THEN 1 ELSE 0 END), 0) AS total_telemetry,
                COALESCE(SUM(CASE WHEN msg_type = 'alarm' THEN 1 ELSE 0 END), 0) AS total_alarms,
                MIN(created_at) AS oldest,
                MAX(created_at) AS newest
         FROM offline_buffer",
    )
    .fetch_one(pool)
    .await?;

    Ok(row)
}

/// 取本地配置版本（无记录时 None）。
pub(crate) async fn find_config_local_version(pool: &SqlitePool) -> std::result::Result<Option<String>, sqlx::Error> {
    let version = sqlx::query_scalar("SELECT local_version FROM config_meta WHERE key = 'main'")
        .fetch_optional(pool)
        .await?;

    Ok(version)
}

/// 写入本地配置版本（last-write-wins）。
pub(crate) async fn upsert_config_local_version(
    pool: &SqlitePool,
    version: &str,
    updated_at: i64,
) -> std::result::Result<(), sqlx::Error> {
    sqlx::query("INSERT OR REPLACE INTO config_meta (key, local_version, updated_at) VALUES ('main', ?, ?)")
        .bind(version)
        .bind(updated_at)
        .execute(pool)
        .await?;

    Ok(())
}

// ──────────────────────────────────────────────
// Db 委托（Edge 领域）
// ──────────────────────────────────────────────

impl Db {
    /// 写入一条缓冲消息。
    pub async fn insert_offline_message(
        &self,
        msg_type: &str,
        topic: &str,
        payload: &[u8],
        created_at: i64,
        priority: i32,
    ) -> std::result::Result<(), sqlx::Error> {
        insert_offline_message(self.pool(), msg_type, topic, payload, created_at, priority).await
    }

    /// 普通优先级缓冲消息总数。
    pub async fn count_normal_priority_offline(&self) -> std::result::Result<i64, sqlx::Error> {
        count_normal_priority_offline(self.pool()).await
    }

    /// FIFO 淘汰最老的 n 条普通优先级消息。
    pub async fn evict_oldest_normal_offline(&self, excess: i64) -> std::result::Result<(), sqlx::Error> {
        evict_oldest_normal_offline(self.pool(), excess).await
    }

    /// 按创建时间正序取一批缓冲消息。
    pub async fn fetch_offline_batch(&self, limit: i64) -> std::result::Result<Vec<OfflineBufferRow>, sqlx::Error> {
        fetch_offline_batch(self.pool(), limit).await
    }

    /// 删除一条缓冲消息（确认发布成功后）。
    pub async fn delete_offline_message(&self, id: i64) -> std::result::Result<(), sqlx::Error> {
        delete_offline_message(self.pool(), id).await
    }

    /// 发布失败：retry_count + 1，保留行。
    pub async fn increment_offline_retry(&self, id: i64) -> std::result::Result<(), sqlx::Error> {
        increment_offline_retry(self.pool(), id).await
    }

    /// 缓冲统计：telemetry/alarm 计数 + 最老/最新时间戳。
    pub async fn offline_buffer_status(&self) -> std::result::Result<OfflineBufferStatusRow, sqlx::Error> {
        offline_buffer_status(self.pool()).await
    }

    /// 取本地配置版本（无记录时 None）。
    pub async fn find_config_local_version(&self) -> std::result::Result<Option<String>, sqlx::Error> {
        find_config_local_version(self.pool()).await
    }

    /// 写入本地配置版本（last-write-wins）。
    pub async fn upsert_config_local_version(
        &self,
        version: &str,
        updated_at: i64,
    ) -> std::result::Result<(), sqlx::Error> {
        upsert_config_local_version(self.pool(), version, updated_at).await
    }
}
