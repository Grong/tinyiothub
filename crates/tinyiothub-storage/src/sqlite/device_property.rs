use sqlx::FromRow;

use crate::sqlite::database::Database;
use tinyiothub_core::models::device_property::*;
use tinyiothub_core::{generate_id, now_string};

/// Internal row type for sqlx mapping
#[derive(Debug, Clone, FromRow)]
struct DevicePropertyRow {
    id: String,
    device_id: String,
    name: String,
    display_name: Option<String>,
    description: Option<String>,
    data_type: Option<String>,
    unit: Option<String>,
    min_value: Option<f64>,
    max_value: Option<f64>,
    default_value: Option<String>,
    is_read_only: i32,
    created_at: Option<String>,
    updated_at: Option<String>,
}

impl From<DevicePropertyRow> for DeviceProperty {
    fn from(row: DevicePropertyRow) -> Self {
        Self {
            id: row.id,
            device_id: row.device_id,
            name: row.name,
            display_name: row.display_name,
            description: row.description,
            data_type: row.data_type,
            unit: row.unit,
            min_value: row.min_value,
            max_value: row.max_value,
            default_value: row.default_value,
            is_read_only: row.is_read_only,
            created_at: row.created_at,
            updated_at: row.updated_at,
            current_value: None,
            alarm_status: None,
        }
    }
}

/// Find a device property by ID
pub async fn find_device_property_by_id(
    db: &Database,
    id: &str,
) -> Result<Option<DeviceProperty>, sqlx::Error> {
    let row = sqlx::query_as::<_, DevicePropertyRow>(
        r#"
        SELECT id, device_id, name, display_name, description, data_type, unit,
               min_value, max_value, default_value, is_read_only, created_at, updated_at
        FROM device_properties WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(db.pool())
    .await?;

    let mut property: Option<DeviceProperty> = row.map(Into::into);
    if let Some(ref mut prop) = property {
        prop.clear_runtime_data();
    }

    Ok(property)
}

/// Find properties by device ID
pub async fn find_device_properties_by_device_id(
    db: &Database,
    device_id: &str,
) -> Result<Vec<DeviceProperty>, sqlx::Error> {
    let rows = sqlx::query_as::<_, DevicePropertyRow>(
        r#"
        SELECT id, device_id, name, display_name, description, data_type, unit,
               min_value, max_value, default_value, is_read_only, created_at, updated_at
        FROM device_properties WHERE device_id = ?
        ORDER BY name
        "#,
    )
    .bind(device_id)
    .fetch_all(db.pool())
    .await?;

    let mut properties: Vec<DeviceProperty> = rows.into_iter().map(Into::into).collect();
    for prop in &mut properties {
        prop.clear_runtime_data();
    }

    Ok(properties)
}

/// Batch create device properties
pub async fn create_device_properties_batch(
    db: &Database,
    requests: &[CreateDevicePropertyRequest],
) -> Result<Vec<DeviceProperty>, sqlx::Error> {
    let mut tx = db.pool().begin().await?;
    let mut created_ids = Vec::new();

    for request in requests {
        let id = generate_id();
        let now = now_string();
        let is_read_only = request.is_read_only.unwrap_or(0);

        sqlx::query(
            r#"
            INSERT INTO device_properties (
                id, device_id, name, display_name, description, data_type, unit,
                min_value, max_value, default_value, is_read_only, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&request.device_id)
        .bind(&request.name)
        .bind(&request.display_name)
        .bind(&request.description)
        .bind(&request.data_type)
        .bind(&request.unit)
        .bind(request.min_value)
        .bind(request.max_value)
        .bind(&request.default_value)
        .bind(is_read_only)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await?;

        created_ids.push(id);
    }

    tx.commit().await?;

    let mut results = Vec::new();
    for id in created_ids {
        if let Some(property) = find_device_property_by_id(db, &id).await? {
            results.push(property);
        }
    }

    Ok(results)
}
