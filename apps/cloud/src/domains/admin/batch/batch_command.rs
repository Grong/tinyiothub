// Batch Command Infrastructure
// Handles batch command execution with idempotency
//
// SQL 与行类型已迁入 tinyiothub_storage::batch_command（Task 12）；
// 本文件保留执行器（设备侧调用）并 re-export 既有类型路径。

use std::sync::Arc;

use tinyiothub_storage::Db;

pub use tinyiothub_storage::batch_command::{
    BatchCommand, BatchCommandError, BatchCommandItem, BatchCommandResult, BatchCommandWithItems,
    CreateBatchCommandRequest,
};

use crate::domains::driver::legacy::DeviceService;

/// Execute batch commands
pub struct BatchCommandExecutor;

impl BatchCommandExecutor {
    /// Execute a batch command - send commands to all pending devices
    pub async fn execute(
        db: &Db,
        device_service: Arc<DeviceService>,
        batch_id: &str,
    ) -> BatchCommandResult<BatchCommandWithItems> {
        // Get batch with items
        let batch_with_items = db.get_batch_command_with_items(batch_id)
            .await?
            .ok_or_else(|| BatchCommandError::NotFound(batch_id.to_string()))?;

        // Update batch status to running
        db.update_batch_command_status(batch_id, "running").await?;

        let command_type = batch_with_items.batch.command_type.clone();
        let parameters = batch_with_items.batch.parameters.clone();

        // Process each pending item
        for item in &batch_with_items.items {
            if item.status != "pending" {
                continue;
            }

            // Update item to sent
            if let Err(e) =
                db.update_batch_command_item_status(&item.id, "sent", Some("Command sent to device"), None)
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
                    let _ = db.update_batch_command_item_status(
                        &item.id,
                        "success",
                        Some(&format!("Command sent successfully: {}", command_id)),
                        Some(&command_id),
                    )
                    .await;
                }
                Err(e) => {
                    // Update item as failure
                    let _ = db.update_batch_command_item_status(
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
        let updated = db.get_batch_command_with_items(batch_id)
            .await?
            .expect("Batch must exist");

        // Check if all items are done
        let all_done = updated
            .items
            .iter()
            .all(|i| i.status != "pending" && i.status != "sent");
        let has_failures = updated.items.iter().any(|i| i.status == "failure");

        let final_status = if all_done {
            if has_failures { "partial_failure" } else { "completed" }
        } else {
            "running"
        };

        db.mark_batch_command_completed(batch_id, final_status).await?;

        // Return final state
        db.get_batch_command_with_items(batch_id)
            .await?
            .ok_or_else(|| BatchCommandError::NotFound(batch_id.to_string()))
    }
}
