// cloud/src/shared/persistence/repositories/driver_installation.rs

use sqlx::FromRow;

use crate::shared::persistence::Database;

#[derive(Debug, Clone, FromRow)]
pub struct DriverInstallation {
    pub id: i64,
    pub workspace_id: String,
    pub driver_name: String,
    pub version: String,
    pub file_path: String,
    pub checksum: String,
    pub protocol_type: Option<String>,
    pub installed_at: String,
    pub updated_at: String,
}

pub struct DriverInstallationRepo {
    db: Database,
}

impl DriverInstallationRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub async fn create(
        &self,
        workspace_id: &str,
        driver_name: &str,
        version: &str,
        file_path: &str,
        checksum: &str,
        protocol_type: Option<&str>,
    ) -> Result<DriverInstallation, sqlx::Error> {
        let id = sqlx::query(
            r#"
            INSERT INTO driver_installations
                (workspace_id, driver_name, version, file_path, checksum, protocol_type)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(workspace_id)
        .bind(driver_name)
        .bind(version)
        .bind(file_path)
        .bind(checksum)
        .bind(protocol_type)
        .execute(self.db.pool())
        .await?
        .last_insert_rowid();

        self.find_by_id(id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    pub async fn find_by_id(&self, id: i64) -> Result<Option<DriverInstallation>, sqlx::Error> {
        sqlx::query_as::<_, DriverInstallation>("SELECT * FROM driver_installations WHERE id = ?")
            .bind(id)
            .fetch_optional(self.db.pool())
            .await
    }

    pub async fn find_by_workspace(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<DriverInstallation>, sqlx::Error> {
        sqlx::query_as::<_, DriverInstallation>(
            "SELECT * FROM driver_installations WHERE workspace_id = ? ORDER BY driver_name",
        )
        .bind(workspace_id)
        .fetch_all(self.db.pool())
        .await
    }

    pub async fn find_all(&self) -> Result<Vec<DriverInstallation>, sqlx::Error> {
        sqlx::query_as::<_, DriverInstallation>(
            "SELECT * FROM driver_installations ORDER BY workspace_id, driver_name",
        )
        .fetch_all(self.db.pool())
        .await
    }

    pub async fn delete(
        &self,
        workspace_id: &str,
        driver_name: &str,
        version: &str,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            "DELETE FROM driver_installations WHERE workspace_id = ? AND driver_name = ? AND version = ?"
        )
        .bind(workspace_id)
        .bind(driver_name)
        .bind(version)
        .execute(self.db.pool())
        .await?;

        Ok(result.rows_affected())
    }
}
