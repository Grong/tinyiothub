use tinyiothub_core::models::device::{Device, DeviceStats};
use async_trait::async_trait;
use sqlx::{QueryBuilder, Row};

use crate::{
    modules::device::query_service::DeviceQueryService,
    modules::monitoring::types::{DeviceStatusDistribution, QuickDevice},
    shared::persistence::Database,
    shared::error::Result,
};

use tinyiothub_storage::sqlite::device_row_mapper;

/// SQLite implementation of DeviceQueryService
#[derive(Debug, Clone)]
pub struct SqliteDeviceQueryService {
    database: Database,
}

impl SqliteDeviceQueryService {
    pub fn new(database: Database) -> Self {
        Self { database }
    }
}

#[async_trait]
impl DeviceQueryService for SqliteDeviceQueryService {
    async fn search(&self, keyword: &str, limit: Option<u32>) -> Result<Vec<Device>> {
        let search_pattern = format!("%{}%", keyword);
        let exact_pattern = format!("{}%", keyword);

        let mut builder = QueryBuilder::new("SELECT ");
        builder.push(device_row_mapper::SELECT_COLUMNS);
        builder.push(
            " FROM devices WHERE name LIKE ? OR display_name LIKE ? OR address LIKE ? OR description LIKE ?
             ORDER BY CASE
                WHEN name LIKE ? THEN 1
                WHEN display_name LIKE ? THEN 2
                WHEN address LIKE ? THEN 3
                ELSE 4
             END, name",
        );

        builder.push_bind(&search_pattern);
        builder.push_bind(&search_pattern);
        builder.push_bind(&search_pattern);
        builder.push_bind(&search_pattern);
        builder.push_bind(&exact_pattern);
        builder.push_bind(&exact_pattern);
        builder.push_bind(&exact_pattern);

        if let Some(limit) = limit {
            builder.push(" LIMIT ").push_bind(limit as i64);
        }

        let rows = builder.build().fetch_all(self.database.pool()).await?;
        let mut devices = Vec::new();
        for row in rows {
            devices.push(device_row_mapper::row_to_device(row)?);
        }
        Ok(devices)
    }

    async fn get_stats(&self) -> Result<DeviceStats> {
        let row = sqlx::query(
            r#"
            SELECT
                COUNT(*) as total_devices,
                COUNT(CASE WHEN state = 1 THEN 1 END) as online_devices,
                COUNT(CASE WHEN state = 0 OR state = 3 THEN 1 END) as offline_devices,
                COUNT(CASE WHEN state = 2 THEN 1 END) as alarm_devices
            FROM devices
            "#,
        )
        .fetch_one(self.database.pool())
        .await?;

        Ok(DeviceStats {
            total_devices: row.get("total_devices"),
            online_devices: row.get("online_devices"),
            offline_devices: row.get("offline_devices"),
            alarm_devices: row.get("alarm_devices"),
        })
    }

    async fn get_stats_by_type(&self) -> Result<Vec<(String, i64)>> {
        let rows = sqlx::query(
            r#"
            SELECT COALESCE(device_type, 'Unknown') as device_type, COUNT(*) as count
            FROM devices
            GROUP BY device_type
            ORDER BY count DESC
            "#,
        )
        .fetch_all(self.database.pool())
        .await?;

        let mut stats = Vec::new();
        for row in rows {
            let device_type: String = row.get("device_type");
            let count: i64 = row.get("count");
            stats.push((device_type, count));
        }
        Ok(stats)
    }

    async fn get_stats_by_driver(&self) -> Result<Vec<(String, i64)>> {
        let rows = sqlx::query(
            r#"
            SELECT COALESCE(driver_name, 'Unknown') as driver_name, COUNT(*) as count
            FROM devices
            GROUP BY driver_name
            ORDER BY count DESC
            "#,
        )
        .fetch_all(self.database.pool())
        .await?;

        let mut stats = Vec::new();
        for row in rows {
            let driver_name: String = row.get("driver_name");
            let count: i64 = row.get("count");
            stats.push((driver_name, count));
        }
        Ok(stats)
    }

    async fn get_device_tree(&self, root_id: Option<&str>) -> Result<Vec<Device>> {
        let mut builder = QueryBuilder::new("SELECT ");
        builder.push(device_row_mapper::SELECT_COLUMNS);
        builder.push(" FROM devices WHERE ");

        if let Some(root_id) = root_id {
            builder.push("parent_id = ").push_bind(root_id);
        } else {
            builder.push("parent_id IS NULL");
        }

        builder.push(" ORDER BY name");

        let rows = builder.build().fetch_all(self.database.pool()).await?;
        let mut devices = Vec::new();
        for row in rows {
            devices.push(device_row_mapper::row_to_device(row)?);
        }
        Ok(devices)
    }

    async fn get_device_status_distribution(
        &self,
        workspace_id: Option<&str>,
    ) -> Result<DeviceStatusDistribution> {
        let mut builder = QueryBuilder::new(
            "SELECT
                SUM(CASE WHEN state = 1 THEN 1 ELSE 0 END) as online,
                SUM(CASE WHEN state = 0 THEN 1 ELSE 0 END) as offline,
                SUM(CASE WHEN state < 0 THEN 1 ELSE 0 END) as error_count,
                SUM(CASE WHEN state = 2 THEN 1 ELSE 0 END) as maintenance
            FROM devices",
        );

        if let Some(wid) = workspace_id {
            builder.push(" WHERE workspace_id = ").push_bind(wid);
        }

        let row = builder.build().fetch_one(self.database.pool()).await?;

        Ok(DeviceStatusDistribution {
            online: row.get("online"),
            offline: row.get("offline"),
            error: row.get("error_count"),
            maintenance: row.get("maintenance"),
        })
    }

    async fn get_quick_devices_list(
        &self,
        limit: i32,
        workspace_id: Option<&str>,
    ) -> Result<Vec<QuickDevice>> {
        let mut builder = QueryBuilder::new(
            "SELECT id, name, device_type, state, updated_at FROM devices",
        );

        if let Some(wid) = workspace_id {
            builder.push(" WHERE workspace_id = ").push_bind(wid);
        }

        builder.push(
            " ORDER BY
                CASE
                    WHEN state = 1 THEN 0
                    WHEN state = 0 THEN 1
                    WHEN state < 0 THEN 2
                    ELSE 3
                END,
                updated_at DESC
            LIMIT ",
        );
        builder.push_bind(limit);

        let devices: Vec<(String, String, Option<String>, i32, chrono::NaiveDateTime)> =
            builder.build_query_as().fetch_all(self.database.pool()).await?;

        let quick_devices = devices
            .into_iter()
            .map(|(id, name, device_type, state, updated_at)| {
                let status = match state {
                    1 => "online",
                    0 => "offline",
                    2 => "maintenance",
                    _ => "error",
                };

                QuickDevice {
                    id,
                    name,
                    status: status.to_string(),
                    last_seen: updated_at.and_utc(),
                    device_type: device_type.unwrap_or_else(|| "unknown".to_string()),
                }
            })
            .collect();

        Ok(quick_devices)
    }
}
