use async_trait::async_trait;
use sqlx::{QueryBuilder, Row};

use crate::{
    domain::device::query_service::DeviceQueryService,
    dto::entity::device::{Device, DeviceStats},
    dto::response::{DeviceStatusDistribution, QuickDevice},
    infrastructure::persistence::Database,
    shared::error::Result,
};

/// SQLite implementation of DeviceQueryService
#[derive(Debug, Clone)]
pub struct SqliteDeviceQueryService {
    database: Database,
}

impl SqliteDeviceQueryService {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    const SELECT_COLUMNS: &str = r#"
        id, name, display_name, device_type, address, description, position,
        driver_name, device_model, protocol_type, factory_name, linked_data,
        driver_options, state, parent_id, product_id, tenant_id, workspace_id, created_at, updated_at
    "#;

    fn row_to_device(&self, row: sqlx::sqlite::SqliteRow) -> Result<Device> {
        Ok(Device {
            id: row.get("id"),
            name: row.get("name"),
            display_name: row.get("display_name"),
            device_type: row.get("device_type"),
            address: row.get("address"),
            description: row.get("description"),
            position: row.get("position"),
            driver_name: row.get("driver_name"),
            device_model: row.get("device_model"),
            protocol_type: row.get("protocol_type"),
            factory_name: row.get("factory_name"),
            linked_data: row.get("linked_data"),
            driver_options: row.get("driver_options"),
            state: row.get("state"),
            parent_id: row.get("parent_id"),
            product_id: row.get("product_id"),
            tenant_id: row.get("tenant_id"),
            workspace_id: row.get("workspace_id"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            tags: None,
            properties: None,
            commands: None,
            is_online: false,
            last_heartbeat: None,
        })
    }
}

#[async_trait]
impl DeviceQueryService for SqliteDeviceQueryService {
    async fn search(&self, keyword: &str, limit: Option<u32>) -> Result<Vec<Device>> {
        let search_pattern = format!("%{}%", keyword);
        let exact_pattern = format!("{}%", keyword);

        let mut builder = QueryBuilder::new("SELECT ");
        builder.push(Self::SELECT_COLUMNS);
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
            devices.push(self.row_to_device(row)?);
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
        builder.push(Self::SELECT_COLUMNS);
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
            devices.push(self.row_to_device(row)?);
        }
        Ok(devices)
    }

    async fn get_device_status_distribution(
        &self,
        workspace_id: Option<&str>,
    ) -> Result<DeviceStatusDistribution> {
        let online: i64 = if let Some(wid) = workspace_id {
            sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE state = 1 AND workspace_id = ?")
                .bind(wid)
                .fetch_one(self.database.pool())
                .await?
        } else {
            sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE state = 1")
                .fetch_one(self.database.pool())
                .await?
        };

        let offline: i64 = if let Some(wid) = workspace_id {
            sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE state = 0 AND workspace_id = ?")
                .bind(wid)
                .fetch_one(self.database.pool())
                .await?
        } else {
            sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE state = 0")
                .fetch_one(self.database.pool())
                .await?
        };

        let error: i64 = if let Some(wid) = workspace_id {
            sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE state < 0 AND workspace_id = ?")
                .bind(wid)
                .fetch_one(self.database.pool())
                .await?
        } else {
            sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE state < 0")
                .fetch_one(self.database.pool())
                .await?
        };

        let maintenance: i64 = if let Some(wid) = workspace_id {
            sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE state = 2 AND workspace_id = ?")
                .bind(wid)
                .fetch_one(self.database.pool())
                .await?
        } else {
            sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE state = 2")
                .fetch_one(self.database.pool())
                .await?
        };

        Ok(DeviceStatusDistribution { online, offline, error, maintenance })
    }

    async fn get_quick_devices_list(
        &self,
        limit: i32,
        workspace_id: Option<&str>,
    ) -> Result<Vec<QuickDevice>> {
        let devices: Vec<(String, String, Option<String>, i32, chrono::NaiveDateTime)> = if let Some(wid) = workspace_id {
            sqlx::query_as(
                r#"
                SELECT id, name, device_type, state, updated_at
                FROM devices WHERE workspace_id = ?
                ORDER BY
                    CASE
                        WHEN state = 1 THEN 0
                        WHEN state = 0 THEN 1
                        WHEN state < 0 THEN 2
                        ELSE 3
                    END,
                    updated_at DESC
                LIMIT ?"#,
            )
            .bind(wid)
            .bind(limit)
            .fetch_all(self.database.pool())
            .await?
        } else {
            sqlx::query_as(
                r#"
                SELECT id, name, device_type, state, updated_at
                FROM devices
                ORDER BY
                    CASE
                        WHEN state = 1 THEN 0
                        WHEN state = 0 THEN 1
                        WHEN state < 0 THEN 2
                        ELSE 3
                    END,
                    updated_at DESC
                LIMIT ?"#,
            )
            .bind(limit)
            .fetch_all(self.database.pool())
            .await?
        };

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
