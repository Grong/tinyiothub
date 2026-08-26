//! Batch command 持久化：batch_commands / batch_command_items 表
//!（自 cloud domains/admin/batch/batch_command.rs 迁入，Task 12）。
//!
//! 类型随 repo 住 db：BatchCommand/BatchCommandItem/CreateBatchCommandRequest/
//! BatchCommandWithItems/BatchCommandError，cloud 侧直接引用本模块路径。

use sqlx::SqlitePool;
use thiserror::Error;
use uuid::Uuid;

use crate::database::Db;

// ──────────────────────────────────────────────
// 持久化类型 + 契约错误 — 自 cloud batch_command.rs 迁入
// ──────────────────────────────────────────────

#[derive(Error, Debug)]
pub enum BatchCommandError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Thing service error: {0}")]
    ThingService(String),
    #[error("Batch not found: {0}")]
    NotFound(String),
    #[error("Idempotency conflict: batch {0} already exists")]
    IdempotencyConflict(String),
}

pub type BatchCommandResult<T> = Result<T, BatchCommandError>;

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct BatchCommand {
    pub id: String,
    pub workspace_id: String,
    pub idempotency_key: String,
    pub command_name: String,
    pub command_type: String,
    pub parameters: Option<String>,
    pub total_devices: i32,
    pub status: String,
    pub submitted_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct BatchCommandItem {
    pub id: String,
    pub batch_id: String,
    pub thing_id: String,
    pub device_name: Option<String>,
    pub status: String,
    pub result_message: Option<String>,
    pub command_id: Option<String>,
    pub executed_at: Option<String>,
    pub completed_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CreateBatchCommandRequest {
    pub workspace_id: String,
    pub idempotency_key: String,
    pub command_name: String,
    pub command_type: Option<String>,
    pub parameters: Option<String>,
    pub device_ids: Vec<String>,
    pub submitted_by: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BatchCommandWithItems {
    pub batch: BatchCommand,
    pub items: Vec<BatchCommandItem>,
}

// ──────────────────────────────────────────────
// 持久化函数（SQLite）
// ──────────────────────────────────────────────

/// Find existing batch by workspace_id + idempotency_key
pub(crate) async fn find_batch_command_by_idempotency_key(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
) -> BatchCommandResult<Option<BatchCommand>> {
    let result = sqlx::query_as::<_, BatchCommand>(
        r#"
            SELECT id, workspace_id, idempotency_key, command_name, command_type,
                   parameters, total_devices, status, submitted_by,
                   created_at, updated_at, completed_at
            FROM batch_commands
            WHERE workspace_id = ? AND idempotency_key = ?
            "#,
    )
    .bind(workspace_id)
    .bind(idempotency_key)
    .fetch_optional(pool)
    .await?;

    Ok(result)
}

/// Find batch by ID
pub(crate) async fn find_batch_command_by_id(
    pool: &SqlitePool,
    batch_id: &str,
) -> BatchCommandResult<Option<BatchCommand>> {
    let result = sqlx::query_as::<_, BatchCommand>(
        r#"
            SELECT id, workspace_id, idempotency_key, command_name, command_type,
                   parameters, total_devices, status, submitted_by,
                   created_at, updated_at, completed_at
            FROM batch_commands
            WHERE id = ?
            "#,
    )
    .bind(batch_id)
    .fetch_optional(pool)
    .await?;

    Ok(result)
}

/// Create a new batch command with items
pub(crate) async fn create_batch_command(
    pool: &SqlitePool,
    request: &CreateBatchCommandRequest,
) -> BatchCommandResult<BatchCommandWithItems> {
    let batch_id = Uuid::new_v4().to_string();
    let command_type = request.command_type.clone().unwrap_or_else(|| "custom".to_string());
    let total_devices = request.device_ids.len() as i32;

    // Insert batch command
    sqlx::query(
        r#"
            INSERT INTO batch_commands (id, workspace_id, idempotency_key, command_name, command_type,
                                       parameters, total_devices, status, submitted_by)
            VALUES (?, ?, ?, ?, ?, ?, ?, 'pending', ?)
            "#,
    )
    .bind(&batch_id)
    .bind(&request.workspace_id)
    .bind(&request.idempotency_key)
    .bind(&request.command_name)
    .bind(&command_type)
    .bind(&request.parameters)
    .bind(total_devices)
    .bind(&request.submitted_by)
    .execute(pool)
    .await?;

    // Insert batch items
    let mut items = Vec::new();
    for thing_id in &request.device_ids {
        let item_id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
                INSERT INTO batch_command_items (id, batch_id, thing_id, status)
                VALUES (?, ?, ?, 'pending')
                "#,
        )
        .bind(&item_id)
        .bind(&batch_id)
        .bind(thing_id)
        .execute(pool)
        .await?;

        items.push(BatchCommandItem {
            id: item_id,
            batch_id: batch_id.clone(),
            thing_id: thing_id.clone(),
            device_name: None,
            status: "pending".to_string(),
            result_message: None,
            command_id: None,
            executed_at: None,
            completed_at: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        });
    }

    let batch = find_batch_command_by_id(pool, &batch_id)
        .await?
        .expect("Batch just created");

    Ok(BatchCommandWithItems { batch, items })
}

/// Update batch status
pub(crate) async fn update_batch_command_status(
    pool: &SqlitePool,
    batch_id: &str,
    status: &str,
) -> BatchCommandResult<()> {
    sqlx::query(
        r#"
            UPDATE batch_commands
            SET status = ?, updated_at = CURRENT_TIMESTAMP
            WHERE id = ?
            "#,
    )
    .bind(status)
    .bind(batch_id)
    .execute(pool)
    .await?;

    Ok(())
}

/// Mark batch as completed
pub(crate) async fn mark_batch_command_completed(
    pool: &SqlitePool,
    batch_id: &str,
    status: &str,
) -> BatchCommandResult<()> {
    sqlx::query(
        r#"
            UPDATE batch_commands
            SET status = ?, completed_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP
            WHERE id = ?
            "#,
    )
    .bind(status)
    .bind(batch_id)
    .execute(pool)
    .await?;

    Ok(())
}

/// Update item status
pub(crate) async fn update_batch_command_item_status(
    pool: &SqlitePool,
    item_id: &str,
    status: &str,
    result_message: Option<&str>,
    command_id: Option<&str>,
) -> BatchCommandResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        r#"
            UPDATE batch_command_items
            SET status = ?, result_message = ?, command_id = ?,
                executed_at = CASE WHEN status = 'sent' THEN ? ELSE executed_at END,
                completed_at = CASE WHEN status IN ('success', 'failure', 'timeout') THEN ? ELSE completed_at END
            WHERE id = ?
            "#,
    )
    .bind(status)
    .bind(result_message)
    .bind(command_id)
    .bind(&now)
    .bind(&now)
    .bind(item_id)
    .execute(pool)
    .await?;

    Ok(())
}

/// Get items by batch ID
pub(crate) async fn get_batch_command_items(
    pool: &SqlitePool,
    batch_id: &str,
) -> BatchCommandResult<Vec<BatchCommandItem>> {
    let items = sqlx::query_as::<_, BatchCommandItem>(
        r#"
            SELECT id, batch_id, thing_id, device_name, status, result_message,
                   command_id, executed_at, completed_at, created_at
            FROM batch_command_items
            WHERE batch_id = ?
            ORDER BY created_at ASC
            "#,
    )
    .bind(batch_id)
    .fetch_all(pool)
    .await?;

    Ok(items)
}

/// Get batch with items
pub(crate) async fn get_batch_command_with_items(
    pool: &SqlitePool,
    batch_id: &str,
) -> BatchCommandResult<Option<BatchCommandWithItems>> {
    let batch = find_batch_command_by_id(pool, batch_id).await?;
    match batch {
        Some(batch) => {
            let items = get_batch_command_items(pool, batch_id).await?;
            Ok(Some(BatchCommandWithItems { batch, items }))
        }
        None => Ok(None),
    }
}

/// List batches by workspace
pub(crate) async fn list_batch_commands_by_workspace(
    pool: &SqlitePool,
    workspace_id: &str,
    limit: i32,
) -> BatchCommandResult<Vec<BatchCommand>> {
    let batches = sqlx::query_as::<_, BatchCommand>(
        r#"
            SELECT id, workspace_id, idempotency_key, command_name, command_type,
                   parameters, total_devices, status, submitted_by,
                   created_at, updated_at, completed_at
            FROM batch_commands
            WHERE workspace_id = ?
            ORDER BY created_at DESC
            LIMIT ?
            "#,
    )
    .bind(workspace_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(batches)
}

// ──────────────────────────────────────────────
// Db 委托
// ──────────────────────────────────────────────

impl Db {
    /// 按 workspace_id + idempotency_key 查既有批次。
    pub async fn find_batch_command_by_idempotency_key(
        &self,
        workspace_id: &str,
        idempotency_key: &str,
    ) -> BatchCommandResult<Option<BatchCommand>> {
        find_batch_command_by_idempotency_key(self.pool(), workspace_id, idempotency_key).await
    }

    /// 按 ID 查批次。
    pub async fn find_batch_command_by_id(&self, batch_id: &str) -> BatchCommandResult<Option<BatchCommand>> {
        find_batch_command_by_id(self.pool(), batch_id).await
    }

    /// 创建批次（含 items）。
    pub async fn create_batch_command(
        &self,
        request: &CreateBatchCommandRequest,
    ) -> BatchCommandResult<BatchCommandWithItems> {
        create_batch_command(self.pool(), request).await
    }

    /// 更新批次状态。
    pub async fn update_batch_command_status(&self, batch_id: &str, status: &str) -> BatchCommandResult<()> {
        update_batch_command_status(self.pool(), batch_id, status).await
    }

    /// 标记批次完成。
    pub async fn mark_batch_command_completed(&self, batch_id: &str, status: &str) -> BatchCommandResult<()> {
        mark_batch_command_completed(self.pool(), batch_id, status).await
    }

    /// 更新批次 item 状态。
    pub async fn update_batch_command_item_status(
        &self,
        item_id: &str,
        status: &str,
        result_message: Option<&str>,
        command_id: Option<&str>,
    ) -> BatchCommandResult<()> {
        update_batch_command_item_status(self.pool(), item_id, status, result_message, command_id).await
    }

    /// 按批次 ID 取 items。
    pub async fn get_batch_command_items(&self, batch_id: &str) -> BatchCommandResult<Vec<BatchCommandItem>> {
        get_batch_command_items(self.pool(), batch_id).await
    }

    /// 取批次 + items。
    pub async fn get_batch_command_with_items(
        &self,
        batch_id: &str,
    ) -> BatchCommandResult<Option<BatchCommandWithItems>> {
        get_batch_command_with_items(self.pool(), batch_id).await
    }

    /// 按 workspace 列出批次。
    pub async fn list_batch_commands_by_workspace(
        &self,
        workspace_id: &str,
        limit: i32,
    ) -> BatchCommandResult<Vec<BatchCommand>> {
        list_batch_commands_by_workspace(self.pool(), workspace_id, limit).await
    }
}
