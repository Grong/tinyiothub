use sqlx::FromRow;

use crate::sqlite::database::Database;
use tinyiothub_core::models::device_command::*;
use tinyiothub_core::{generate_id, now_string};

/// Internal row type for sqlx mapping
#[derive(Debug, Clone, FromRow)]
struct DeviceCommandRow {
    id: String,
    device_id: String,
    name: String,
    display_name: Option<String>,
    description: Option<String>,
    parameters: Option<String>,
    created_at: String,
}

impl From<DeviceCommandRow> for DeviceCommand {
    fn from(row: DeviceCommandRow) -> Self {
        Self {
            id: row.id,
            device_id: row.device_id,
            name: row.name,
            display_name: row.display_name,
            description: row.description,
            parameters: row.parameters,
            created_at: row.created_at,
        }
    }
}

/// Find a device command by ID
pub async fn find_device_command_by_id(
    db: &Database,
    id: &str,
) -> Result<Option<DeviceCommand>, sqlx::Error> {
    let row = sqlx::query_as::<_, DeviceCommandRow>(
        r#"
        SELECT id, device_id, name, display_name, description, parameters, created_at
        FROM device_commands WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(db.pool())
    .await?;

    Ok(row.map(Into::into))
}

/// Create a new device command
pub async fn create_device_command(
    db: &Database,
    request: &CreateDeviceCommandRequest,
) -> Result<DeviceCommand, sqlx::Error> {
    let id = generate_id();
    let created_at = now_string();

    let mut tx = db.pool().begin().await?;

    sqlx::query(
        r#"
        INSERT INTO device_commands (id, device_id, name, display_name, description, parameters, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(&request.device_id)
    .bind(&request.name)
    .bind(&request.display_name)
    .bind(&request.description)
    .bind(&request.parameters)
    .bind(&created_at)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(DeviceCommand {
        id,
        device_id: request.device_id.clone(),
        name: request.name.clone(),
        display_name: request.display_name.clone(),
        description: request.description.clone(),
        parameters: request.parameters.clone(),
        created_at,
    })
}

/// Find commands by device ID
pub async fn find_device_commands_by_device_id(
    db: &Database,
    device_id: &str,
) -> Result<Vec<DeviceCommand>, sqlx::Error> {
    let rows = sqlx::query_as::<_, DeviceCommandRow>(
        r#"
        SELECT id, device_id, name, display_name, description, parameters, created_at
        FROM device_commands WHERE device_id = ?
        ORDER BY name ASC
        "#,
    )
    .bind(device_id)
    .fetch_all(db.pool())
    .await?;

    Ok(rows.into_iter().map(Into::into).collect())
}

/// Find command by device ID and name
pub async fn find_device_command_by_device_and_name(
    db: &Database,
    device_id: &str,
    name: &str,
) -> Result<Option<DeviceCommand>, sqlx::Error> {
    let row = sqlx::query_as::<_, DeviceCommandRow>(
        r#"
        SELECT id, device_id, name, display_name, description, parameters, created_at
        FROM device_commands WHERE device_id = ? AND name = ?
        "#,
    )
    .bind(device_id)
    .bind(name)
    .fetch_optional(db.pool())
    .await?;

    Ok(row.map(Into::into))
}

/// Bulk create device commands
pub async fn bulk_create_device_commands(
    db: &Database,
    requests: &[CreateDeviceCommandRequest],
) -> Result<Vec<DeviceCommand>, sqlx::Error> {
    let mut tx = db.pool().begin().await?;
    let mut created_commands = Vec::new();

    for request in requests {
        let id = generate_id();
        let created_at = now_string();

        sqlx::query(
            r#"
            INSERT INTO device_commands (id, device_id, name, display_name, description, parameters, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&request.device_id)
        .bind(&request.name)
        .bind(&request.display_name)
        .bind(&request.description)
        .bind(&request.parameters)
        .bind(&created_at)
        .execute(&mut *tx)
        .await?;

        created_commands.push(DeviceCommand {
            id,
            device_id: request.device_id.clone(),
            name: request.name.clone(),
            display_name: request.display_name.clone(),
            description: request.description.clone(),
            parameters: request.parameters.clone(),
            created_at,
        });
    }

    tx.commit().await?;
    Ok(created_commands)
}
