use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Row, Sqlite};

use crate::infrastructure::persistence::Database;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceCommand {
    pub id: String,
    pub device_id: String,
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub parameters: Option<String>, // JSON string
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateDeviceCommandRequest {
    pub device_id: String,
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub parameters: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateDeviceCommandRequest {
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub parameters: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct DeviceCommandQueryParams {
    pub device_id: Option<String>,
    pub name: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct CommandQueryParams {
    pub device_id: Option<String>,
    pub name: Option<String>,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceCommandStatistics {
    pub total_commands: i64,
    pub devices_with_commands: i64,
}

impl DeviceCommand {
    /// Find a device command by ID
    pub async fn find_by_id(db: &Database, id: &str) -> Result<Option<DeviceCommand>, sqlx::Error> {
        let command = sqlx::query_as::<_, DeviceCommand>(
            r#"
            SELECT id, device_id, name, display_name, description, parameters, created_at
            FROM device_commands WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(db.pool())
        .await?;

        Ok(command)
    }

    /// Create a new device command using type-safe operations
    pub async fn create(
        db: &Database,
        request: &CreateDeviceCommandRequest,
    ) -> Result<DeviceCommand, sqlx::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        // Use transaction for data consistency
        let mut tx = db.pool().begin().await?;

        sqlx::query(
            r#"
            INSERT INTO device_commands (id, device_id, name, display_name, description, parameters, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#
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

        // Return the created command
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

    /// Update a device command
    pub async fn update(
        db: &Database,
        id: &str,
        request: &UpdateDeviceCommandRequest,
    ) -> Result<DeviceCommand, sqlx::Error> {
        let mut query_builder = QueryBuilder::<Sqlite>::new("UPDATE device_commands SET ");
        let mut has_updates = false;

        if let Some(name) = &request.name {
            if has_updates {
                query_builder.push(", ");
            }
            query_builder.push("name = ");
            query_builder.push_bind(name);
            has_updates = true;
        }

        if let Some(display_name) = &request.display_name {
            if has_updates {
                query_builder.push(", ");
            }
            query_builder.push("display_name = ");
            query_builder.push_bind(display_name);
            has_updates = true;
        }

        if let Some(description) = &request.description {
            if has_updates {
                query_builder.push(", ");
            }
            query_builder.push("description = ");
            query_builder.push_bind(description);
            has_updates = true;
        }

        if let Some(parameters) = &request.parameters {
            if has_updates {
                query_builder.push(", ");
            }
            query_builder.push("parameters = ");
            query_builder.push_bind(parameters);
            has_updates = true;
        }

        if !has_updates {
            return Err(sqlx::Error::RowNotFound);
        }

        query_builder.push(" WHERE id = ");
        query_builder.push_bind(id);

        let query = query_builder.build();
        query.execute(db.pool()).await?;

        // Return the updated command
        Self::find_by_id(db, id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    /// Delete a device command
    pub async fn delete(db: &Database, id: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM device_commands WHERE id = ?")
            .bind(id)
            .execute(db.pool())
            .await?;

        Ok(result.rows_affected())
    }

    /// Find all device commands with optional filtering
    pub async fn find_all(
        db: &Database,
        params: &DeviceCommandQueryParams,
    ) -> Result<Vec<DeviceCommand>, sqlx::Error> {
        let mut query_builder = QueryBuilder::<Sqlite>::new(
            r#"
            SELECT id, device_id, name, display_name, description, parameters, created_at
            FROM device_commands WHERE 1=1
            "#,
        );

        if let Some(device_id) = &params.device_id {
            query_builder.push(" AND device_id = ");
            query_builder.push_bind(device_id);
        }

        if let Some(name) = &params.name {
            query_builder.push(" AND name LIKE ");
            query_builder.push_bind(format!("%{}%", name));
        }

        query_builder.push(" ORDER BY name ASC");

        if let Some(page_size) = params.page_size {
            query_builder.push(" LIMIT ");
            query_builder.push_bind(page_size);

            if let Some(page) = params.page {
                let offset = (page - 1) * page_size;
                query_builder.push(" OFFSET ");
                query_builder.push_bind(offset);
            }
        }

        let query = query_builder.build_query_as::<DeviceCommand>();
        let commands = query.fetch_all(db.pool()).await?;

        Ok(commands)
    }

    /// Count device commands with optional filtering
    pub async fn count(
        db: &Database,
        params: &DeviceCommandQueryParams,
    ) -> Result<i64, sqlx::Error> {
        let mut query_builder =
            QueryBuilder::<Sqlite>::new("SELECT COUNT(*) FROM device_commands WHERE 1=1");

        if let Some(device_id) = &params.device_id {
            query_builder.push(" AND device_id = ");
            query_builder.push_bind(device_id);
        }

        if let Some(name) = &params.name {
            query_builder.push(" AND name LIKE ");
            query_builder.push_bind(format!("%{}%", name));
        }

        let query = query_builder.build_query_scalar::<i64>();
        let count = query.fetch_one(db.pool()).await?;

        Ok(count)
    }

    /// Find commands by device ID
    pub async fn find_by_device_id(
        db: &Database,
        device_id: &str,
    ) -> Result<Vec<DeviceCommand>, sqlx::Error> {
        let commands = sqlx::query_as::<_, DeviceCommand>(
            r#"
            SELECT id, device_id, name, display_name, description, parameters, created_at
            FROM device_commands WHERE device_id = ?
            ORDER BY name ASC
            "#,
        )
        .bind(device_id)
        .fetch_all(db.pool())
        .await?;

        Ok(commands)
    }

    /// Check if a command exists by device ID and name
    pub async fn exists_by_device_and_name(
        db: &Database,
        device_id: &str,
        name: &str,
    ) -> Result<bool, sqlx::Error> {
        let row =
            sqlx::query("SELECT COUNT(*) FROM device_commands WHERE device_id = ? AND name = ?")
                .bind(device_id)
                .bind(name)
                .fetch_one(db.pool())
                .await?;

        Ok(row.get::<i64, _>(0) > 0)
    }

    /// Delete all commands for a device
    pub async fn delete_by_device_id(db: &Database, device_id: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM device_commands WHERE device_id = ?")
            .bind(device_id)
            .execute(db.pool())
            .await?;

        Ok(result.rows_affected())
    }

    /// Get command statistics
    pub async fn get_statistics(db: &Database) -> Result<DeviceCommandStatistics, sqlx::Error> {
        let total_row =
            sqlx::query("SELECT COUNT(*) FROM device_commands").fetch_one(db.pool()).await?;

        let devices_with_commands_row =
            sqlx::query("SELECT COUNT(DISTINCT device_id) FROM device_commands")
                .fetch_one(db.pool())
                .await?;

        Ok(DeviceCommandStatistics {
            total_commands: total_row.get(0),
            devices_with_commands: devices_with_commands_row.get(0),
        })
    }

    /// Bulk create device commands
    pub async fn bulk_create(
        db: &Database,
        requests: &[CreateDeviceCommandRequest],
    ) -> Result<Vec<DeviceCommand>, sqlx::Error> {
        let mut tx = db.pool().begin().await?;
        let mut created_commands = Vec::new();

        for request in requests {
            let id = uuid::Uuid::new_v4().to_string();
            let created_at = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

            sqlx::query(
                r#"
                INSERT INTO device_commands (id, device_id, name, display_name, description, parameters, created_at)
                VALUES (?, ?, ?, ?, ?, ?, ?)
                "#
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

    /// Find commands with advanced filtering and sorting
    pub async fn find_with_params(
        db: &Database,
        params: &CommandQueryParams,
    ) -> Result<Vec<DeviceCommand>, sqlx::Error> {
        let mut query_builder = QueryBuilder::<Sqlite>::new(
            r#"
            SELECT id, device_id, name, display_name, description, parameters, created_at
            FROM device_commands WHERE 1=1
            "#,
        );

        if let Some(device_id) = &params.device_id {
            query_builder.push(" AND device_id = ");
            query_builder.push_bind(device_id);
        }

        if let Some(name) = &params.name {
            query_builder.push(" AND name LIKE ");
            query_builder.push_bind(format!("%{}%", name));
        }

        // Add sorting
        let sort_column = match params.sort_by.as_deref() {
            Some("name") => "name",
            Some("displayName") => "display_name",
            Some("createdAt") => "created_at",
            _ => "created_at",
        };

        let sort_order = match params.sort_order.as_deref() {
            Some("desc") => "DESC",
            _ => "ASC",
        };

        query_builder.push(format!(" ORDER BY {} {}", sort_column, sort_order));

        if let Some(page_size) = params.page_size {
            query_builder.push(" LIMIT ");
            query_builder.push_bind(page_size);

            if let Some(page) = params.page {
                let offset = (page - 1) * page_size;
                query_builder.push(" OFFSET ");
                query_builder.push_bind(offset);
            }
        }

        let query = query_builder.build_query_as::<DeviceCommand>();
        let commands = query.fetch_all(db.pool()).await?;

        Ok(commands)
    }

    /// Find command by device ID and name (legacy method for compatibility)
    pub async fn get_commands_with_name(
        _device_id: &str,
        _name: &str,
    ) -> Result<Option<DeviceCommand>, sqlx::Error> {
        // This is a legacy method that needs to be updated to use Database parameter
        // For now, return None to avoid compilation errors
        // TODO: Update callers to use find_by_device_and_name with proper Database parameter
        Ok(None)
    }

    /// Find command by device ID and name
    pub async fn find_by_device_and_name(
        db: &Database,
        device_id: &str,
        name: &str,
    ) -> Result<Option<DeviceCommand>, sqlx::Error> {
        let command = sqlx::query_as::<_, DeviceCommand>(
            r#"
            SELECT id, device_id, name, display_name, description, parameters, created_at
            FROM device_commands WHERE device_id = ? AND name = ?
            "#,
        )
        .bind(device_id)
        .bind(name)
        .fetch_optional(db.pool())
        .await?;

        Ok(command)
    }
}
