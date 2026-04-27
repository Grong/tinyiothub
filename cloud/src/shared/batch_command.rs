// Batch Command Infrastructure
// Handles batch command execution with idempotency

use std::sync::Arc;

use thiserror::Error;
use uuid::Uuid;

use crate::domain::device::service::DeviceService;
use crate::infrastructure::persistence::database::Database;

#[derive(Error, Debug)]
pub enum BatchCommandError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Device service error: {0}")]
    DeviceService(String),
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
    pub device_id: String,
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

pub struct BatchCommandRepository;

impl BatchCommandRepository {
    /// Find existing batch by workspace_id + idempotency_key
    pub async fn find_by_idempotency_key(
        db: &Database,
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
        .fetch_optional(db.pool())
        .await?;

        Ok(result)
    }

    /// Find batch by ID
    pub async fn find_by_id(
        db: &Database,
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
        .fetch_optional(db.pool())
        .await?;

        Ok(result)
    }

    /// Create a new batch command with items
    pub async fn create(
        db: &Database,
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
        .execute(db.pool())
        .await?;

        // Insert batch items
        let mut items = Vec::new();
        for device_id in &request.device_ids {
            let item_id = Uuid::new_v4().to_string();
            sqlx::query(
                r#"
                INSERT INTO batch_command_items (id, batch_id, device_id, status)
                VALUES (?, ?, ?, 'pending')
                "#,
            )
            .bind(&item_id)
            .bind(&batch_id)
            .bind(device_id)
            .execute(db.pool())
            .await?;

            items.push(BatchCommandItem {
                id: item_id,
                batch_id: batch_id.clone(),
                device_id: device_id.clone(),
                device_name: None,
                status: "pending".to_string(),
                result_message: None,
                command_id: None,
                executed_at: None,
                completed_at: None,
                created_at: chrono::Utc::now().to_rfc3339(),
            });
        }

        let batch = Self::find_by_id(db, &batch_id)
            .await?
            .expect("Batch just created");

        Ok(BatchCommandWithItems { batch, items })
    }

    /// Update batch status
    pub async fn update_status(
        db: &Database,
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
        .execute(db.pool())
        .await?;

        Ok(())
    }

    /// Mark batch as completed
    pub async fn mark_completed(
        db: &Database,
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
        .execute(db.pool())
        .await?;

        Ok(())
    }

    /// Update item status
    pub async fn update_item_status(
        db: &Database,
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
        .execute(db.pool())
        .await?;

        Ok(())
    }

    /// Get items by batch ID
    pub async fn get_items_by_batch_id(
        db: &Database,
        batch_id: &str,
    ) -> BatchCommandResult<Vec<BatchCommandItem>> {
        let items = sqlx::query_as::<_, BatchCommandItem>(
            r#"
            SELECT id, batch_id, device_id, device_name, status, result_message,
                   command_id, executed_at, completed_at, created_at
            FROM batch_command_items
            WHERE batch_id = ?
            ORDER BY created_at ASC
            "#,
        )
        .bind(batch_id)
        .fetch_all(db.pool())
        .await?;

        Ok(items)
    }

    /// Get batch with items
    pub async fn get_batch_with_items(
        db: &Database,
        batch_id: &str,
    ) -> BatchCommandResult<Option<BatchCommandWithItems>> {
        let batch = Self::find_by_id(db, batch_id).await?;
        match batch {
            Some(batch) => {
                let items = Self::get_items_by_batch_id(db, batch_id).await?;
                Ok(Some(BatchCommandWithItems { batch, items }))
            }
            None => Ok(None),
        }
    }

    /// List batches by workspace
    pub async fn list_by_workspace(
        db: &Database,
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
        .fetch_all(db.pool())
        .await?;

        Ok(batches)
    }
}

/// Execute batch commands
pub struct BatchCommandExecutor;

impl BatchCommandExecutor {
    /// Execute a batch command - send commands to all pending devices
    pub async fn execute(
        db: &Database,
        device_service: Arc<DeviceService>,
        batch_id: &str,
    ) -> BatchCommandResult<BatchCommandWithItems> {
        // Get batch with items
        let batch_with_items = BatchCommandRepository::get_batch_with_items(db, batch_id)
            .await?
            .ok_or_else(|| BatchCommandError::NotFound(batch_id.to_string()))?;

        // Update batch status to running
        BatchCommandRepository::update_status(db, batch_id, "running").await?;

        let command_type = batch_with_items.batch.command_type.clone();
        let parameters = batch_with_items.batch.parameters.clone();

        // Process each pending item
        for item in &batch_with_items.items {
            if item.status != "pending" {
                continue;
            }

            // Update item to sent
            if let Err(e) = BatchCommandRepository::update_item_status(
                db,
                &item.id,
                "sent",
                Some("Command sent to device"),
                None,
            )
            .await
            {
                tracing::error!("Failed to update item {} status: {}", item.id, e);
            }

            // Send command to device
            match device_service
                .send_command(
                    &item.device_id,
                    &batch_with_items.batch.command_name,
                    &command_type,
                    parameters.clone(),
                )
                .await
            {
                Ok(command_id) => {
                    // Update item as success
                    let _ = BatchCommandRepository::update_item_status(
                        db,
                        &item.id,
                        "success",
                        Some(&format!("Command sent successfully: {}", command_id)),
                        Some(&command_id),
                    )
                    .await;
                }
                Err(e) => {
                    // Update item as failure
                    let _ = BatchCommandRepository::update_item_status(
                        db,
                        &item.id,
                        "failure",
                        Some(&format!("Failed to send command: {}", e)),
                        None,
                    )
                    .await;
                }
            }
        }

        // Refresh batch with updated items
        let updated = BatchCommandRepository::get_batch_with_items(db, batch_id)
            .await?
            .expect("Batch must exist");

        // Check if all items are done
        let all_done = updated.items.iter().all(|i| i.status != "pending" && i.status != "sent");
        let has_failures = updated.items.iter().any(|i| i.status == "failure");

        let final_status = if all_done {
            if has_failures {
                "partial_failure"
            } else {
                "completed"
            }
        } else {
            "running"
        };

        BatchCommandRepository::mark_completed(db, batch_id, final_status).await?;

        // Return final state
        BatchCommandRepository::get_batch_with_items(db, batch_id)
            .await?
            .ok_or_else(|| BatchCommandError::NotFound(batch_id.to_string()))
    }
}
