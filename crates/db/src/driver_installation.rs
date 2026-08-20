//! Driver installation records (driver_installations table).

// cloud/src/shared/persistence/repositories/driver_installation.rs

use sqlx::{FromRow, SqlitePool};

use crate::Db;

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

pub(crate) async fn create_driver_installation(
    pool: &SqlitePool,
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
    .execute(pool)
    .await?
    .last_insert_rowid();

    find_driver_installation_by_id(pool, id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub(crate) async fn find_driver_installation_by_id(
    pool: &SqlitePool,
    id: i64,
) -> Result<Option<DriverInstallation>, sqlx::Error> {
    sqlx::query_as::<_, DriverInstallation>("SELECT * FROM driver_installations WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub(crate) async fn find_driver_installations_by_workspace(
    pool: &SqlitePool,
    workspace_id: &str,
) -> Result<Vec<DriverInstallation>, sqlx::Error> {
    sqlx::query_as::<_, DriverInstallation>(
        "SELECT * FROM driver_installations WHERE workspace_id = ? ORDER BY driver_name",
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await
}

pub(crate) async fn find_all_driver_installations(
    pool: &SqlitePool,
) -> Result<Vec<DriverInstallation>, sqlx::Error> {
    sqlx::query_as::<_, DriverInstallation>("SELECT * FROM driver_installations ORDER BY workspace_id, driver_name")
        .fetch_all(pool)
        .await
}

pub(crate) async fn delete_driver_installation(
    pool: &SqlitePool,
    workspace_id: &str,
    driver_name: &str,
    version: &str,
) -> Result<u64, sqlx::Error> {
    let result =
        sqlx::query("DELETE FROM driver_installations WHERE workspace_id = ? AND driver_name = ? AND version = ?")
            .bind(workspace_id)
            .bind(driver_name)
            .bind(version)
            .execute(pool)
            .await?;

    Ok(result.rows_affected())
}

impl Db {
    /// 记录一条驱动安装并回读（last_insert_rowid）。
    pub async fn create_driver_installation(
        &self,
        workspace_id: &str,
        driver_name: &str,
        version: &str,
        file_path: &str,
        checksum: &str,
        protocol_type: Option<&str>,
    ) -> Result<DriverInstallation, sqlx::Error> {
        create_driver_installation(
            self.pool(),
            workspace_id,
            driver_name,
            version,
            file_path,
            checksum,
            protocol_type,
        )
        .await
    }

    /// 按行 ID 查驱动安装记录。
    pub async fn find_driver_installation_by_id(
        &self,
        id: i64,
    ) -> Result<Option<DriverInstallation>, sqlx::Error> {
        find_driver_installation_by_id(self.pool(), id).await
    }

    /// 列出某 workspace 的驱动安装记录（按驱动名排序）。
    pub async fn find_driver_installations_by_workspace(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<DriverInstallation>, sqlx::Error> {
        find_driver_installations_by_workspace(self.pool(), workspace_id).await
    }

    /// 列出全部驱动安装记录（按 workspace、驱动名排序）。
    pub async fn find_all_driver_installations(&self) -> Result<Vec<DriverInstallation>, sqlx::Error> {
        find_all_driver_installations(self.pool()).await
    }

    /// 删除指定 (workspace, driver, version) 的安装记录，返回影响行数。
    pub async fn delete_driver_installation(
        &self,
        workspace_id: &str,
        driver_name: &str,
        version: &str,
    ) -> Result<u64, sqlx::Error> {
        delete_driver_installation(self.pool(), workspace_id, driver_name, version).await
    }
}
