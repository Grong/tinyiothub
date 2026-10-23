//! E4a driver/device declarations + projection writes.
//!
//! 两张事实源表（`driver_declarations` / `device_declarations`）、验收报告表，
//! 以及声明到 `thing_properties` / `thing_actions` 的物化写入。
//! 物化只 upsert + 标 stale，从不删除（spec §三：模板/手工能力行保留）。

use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};

use crate::Db;
use tinyiothub_core::driver::declaration::{AcceptanceReportRecord, DeviceDeclarationRecord, DriverDeclarationRecord};
use tinyiothub_core::driver::manifest::{ManifestTool, StateFieldDecl};

const DRIVER_COLUMNS: &str = "workspace_id, driver_name, driver_version, manifest_version, \
                              manifest_json, manifest_hash, binary_hash, status, installed_at";
const DEVICE_COLUMNS: &str = "workspace_id, thing_id, driver_name, driver_version, \
                              declaration_json, declaration_hash, status, updated_at";

fn driver_record_from_row(row: &SqliteRow) -> Result<DriverDeclarationRecord, sqlx::Error> {
    Ok(DriverDeclarationRecord {
        workspace_id: row.try_get("workspace_id")?,
        driver_name: row.try_get("driver_name")?,
        driver_version: row.try_get("driver_version")?,
        manifest_version: row.try_get("manifest_version")?,
        manifest_json: row.try_get("manifest_json")?,
        manifest_hash: row.try_get("manifest_hash")?,
        binary_hash: row.try_get("binary_hash")?,
        status: row.try_get("status")?,
        installed_at: row.try_get("installed_at")?,
    })
}

fn device_record_from_row(row: &SqliteRow) -> Result<DeviceDeclarationRecord, sqlx::Error> {
    Ok(DeviceDeclarationRecord {
        workspace_id: row.try_get("workspace_id")?,
        thing_id: row.try_get("thing_id")?,
        driver_name: row.try_get("driver_name")?,
        driver_version: row.try_get("driver_version")?,
        declaration_json: row.try_get("declaration_json")?,
        declaration_hash: row.try_get("declaration_hash")?,
        status: row.try_get("status")?,
        updated_at: row.try_get("updated_at")?,
    })
}

/// 写入包级声明；同 (workspace, driver) 下的其他版本置 retired。
///
/// retire UPDATE 与 active INSERT 在同一事务内：并发安装不会留下两条
/// active，崩溃也不会停在"零 active"的中间态。
pub(crate) async fn upsert_driver_declaration(
    pool: &SqlitePool,
    r: &DriverDeclarationRecord,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE driver_declarations SET status='retired' \
         WHERE workspace_id=? AND driver_name=? AND driver_version<>?",
    )
    .bind(&r.workspace_id)
    .bind(&r.driver_name)
    .bind(&r.driver_version)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        r#"INSERT INTO driver_declarations
             (workspace_id, driver_name, driver_version, manifest_version,
              manifest_json, manifest_hash, binary_hash, status, installed_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
           ON CONFLICT(workspace_id, driver_name, driver_version) DO UPDATE SET
             manifest_version=excluded.manifest_version, manifest_json=excluded.manifest_json,
             manifest_hash=excluded.manifest_hash, binary_hash=excluded.binary_hash,
             status=excluded.status, installed_at=excluded.installed_at"#,
    )
    .bind(&r.workspace_id)
    .bind(&r.driver_name)
    .bind(&r.driver_version)
    .bind(&r.manifest_version)
    .bind(&r.manifest_json)
    .bind(&r.manifest_hash)
    .bind(&r.binary_hash)
    .bind(&r.status)
    .bind(&r.installed_at)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// 某 workspace 下当前生效（status='active'）的包级声明。
pub(crate) async fn list_active_driver_declarations(
    pool: &SqlitePool,
    workspace_id: &str,
) -> Result<Vec<DriverDeclarationRecord>, sqlx::Error> {
    let sql = format!(
        "SELECT {DRIVER_COLUMNS} FROM driver_declarations \
         WHERE workspace_id=? AND status='active' ORDER BY driver_name"
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(workspace_id)
        .fetch_all(pool)
        .await?;
    rows.iter().map(driver_record_from_row).collect()
}

/// 全量包级声明（含 retired；启动重织/审计用）。
pub(crate) async fn list_all_driver_declarations(
    pool: &SqlitePool,
) -> Result<Vec<DriverDeclarationRecord>, sqlx::Error> {
    let sql =
        format!("SELECT {DRIVER_COLUMNS} FROM driver_declarations ORDER BY workspace_id, driver_name, installed_at");
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str())).fetch_all(pool).await?;
    rows.iter().map(driver_record_from_row).collect()
}

/// 写入设备级声明（UNIQUE(thing_id, driver_name) 幂等 upsert）。
pub(crate) async fn upsert_device_declaration(
    pool: &SqlitePool,
    r: &DeviceDeclarationRecord,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO device_declarations
             (workspace_id, thing_id, driver_name, driver_version,
              declaration_json, declaration_hash, status, updated_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?)
           ON CONFLICT(thing_id, driver_name) DO UPDATE SET
             workspace_id=excluded.workspace_id, driver_version=excluded.driver_version,
             declaration_json=excluded.declaration_json,
             declaration_hash=excluded.declaration_hash,
             status=excluded.status, updated_at=excluded.updated_at"#,
    )
    .bind(&r.workspace_id)
    .bind(&r.thing_id)
    .bind(&r.driver_name)
    .bind(&r.driver_version)
    .bind(&r.declaration_json)
    .bind(&r.declaration_hash)
    .bind(&r.status)
    .bind(&r.updated_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// 某 workspace 下的全部设备声明（启动重织用）。
pub(crate) async fn list_device_declarations(
    pool: &SqlitePool,
    workspace_id: &str,
) -> Result<Vec<DeviceDeclarationRecord>, sqlx::Error> {
    let sql =
        format!("SELECT {DEVICE_COLUMNS} FROM device_declarations WHERE workspace_id=? ORDER BY thing_id, driver_name");
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(workspace_id)
        .fetch_all(pool)
        .await?;
    rows.iter().map(device_record_from_row).collect()
}

/// 单物设备声明查找（cache miss 时回源）。
pub(crate) async fn find_device_declaration(
    pool: &SqlitePool,
    workspace_id: &str,
    thing_id: &str,
) -> Result<Option<DeviceDeclarationRecord>, sqlx::Error> {
    let sql = format!(
        "SELECT {DEVICE_COLUMNS} FROM device_declarations \
         WHERE workspace_id=? AND thing_id=? ORDER BY driver_name LIMIT 1"
    );
    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(workspace_id)
        .bind(thing_id)
        .fetch_optional(pool)
        .await?;
    row.as_ref().map(device_record_from_row).transpose()
}

/// 追加一条验收报告（append-only）。
pub(crate) async fn insert_acceptance_report(pool: &SqlitePool, r: &AcceptanceReportRecord) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO acceptance_reports
             (workspace_id, driver_name, driver_version, profile, level, report_json, created_at)
           VALUES (?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(&r.workspace_id)
    .bind(&r.driver_name)
    .bind(&r.driver_version)
    .bind(&r.profile)
    .bind(&r.level)
    .bind(&r.report_json)
    .bind(&r.created_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// 物化声明字段 → thing_properties（source='driver'，不删除手工/模板行）。
pub(crate) async fn upsert_declared_properties(
    pool: &SqlitePool,
    thing_id: &str,
    fields: &[StateFieldDecl],
) -> Result<(), sqlx::Error> {
    for f in fields {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        sqlx::query(
            r#"INSERT INTO thing_properties
                 (id, thing_id, name, display_name, description, data_type, unit,
                  min_value, max_value, default_value, is_read_only, source, created_at, updated_at)
               VALUES (?, ?, ?, ?, NULL, ?, ?, ?, ?, NULL, ?, 'driver', ?, ?)
               ON CONFLICT(thing_id, name) DO UPDATE SET
                 display_name=excluded.display_name, data_type=excluded.data_type,
                 unit=excluded.unit, min_value=excluded.min_value,
                 max_value=excluded.max_value, is_read_only=excluded.is_read_only,
                 source='driver', updated_at=excluded.updated_at"#,
        )
        .bind(&id)
        .bind(thing_id)
        .bind(&f.name)
        .bind(&f.name)
        .bind(&f.data_type)
        .bind(&f.unit)
        .bind(f.range.as_ref().and_then(|r| r.min))
        .bind(f.range.as_ref().and_then(|r| r.max))
        .bind(if f.read_only { 1i32 } else { 0i32 })
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// 物化声明工具 → thing_actions（source='driver'）。
pub(crate) async fn upsert_declared_commands(
    pool: &SqlitePool,
    thing_id: &str,
    tools: &[ManifestTool],
) -> Result<(), sqlx::Error> {
    for t in tools {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let params = serde_json::to_string(&t.parameters).unwrap_or_else(|_| "{}".into());
        sqlx::query(
            r#"INSERT INTO thing_actions
                 (id, thing_id, name, display_name, description, parameters, created_at, updated_at, source)
               VALUES (?, ?, ?, ?, NULL, ?, ?, ?, 'driver')
               ON CONFLICT(thing_id, name) DO UPDATE SET
                 parameters=excluded.parameters, source='driver', updated_at=excluded.updated_at"#,
        )
        .bind(&id)
        .bind(thing_id)
        .bind(&t.name)
        .bind(&t.name)
        .bind(&params)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// 声明中已消失的能力标 source='stale'（保留行，不删除）。
pub(crate) async fn mark_stale_declared_capabilities(
    pool: &SqlitePool,
    thing_id: &str,
    keep_properties: &[String],
    keep_commands: &[String],
) -> Result<(), sqlx::Error> {
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    mark_stale(pool, "thing_properties", thing_id, keep_properties, now.clone()).await?;
    mark_stale(pool, "thing_actions", thing_id, keep_commands, now).await
}

async fn mark_stale(
    pool: &SqlitePool,
    table: &str,
    thing_id: &str,
    keep: &[String],
    now: String,
) -> Result<(), sqlx::Error> {
    if keep.is_empty() {
        let sql = format!("UPDATE {table} SET source='stale', updated_at=? WHERE thing_id=? AND source='driver'");
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(&now)
            .bind(thing_id)
            .execute(pool)
            .await?;
    } else {
        let placeholders = vec!["?"; keep.len()].join(",");
        let sql = format!(
            "UPDATE {table} SET source='stale', updated_at=? WHERE thing_id=? AND source='driver' AND name NOT IN ({placeholders})"
        );
        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql.as_str())).bind(&now).bind(thing_id);
        for name in keep {
            q = q.bind(name);
        }
        q.execute(pool).await?;
    }
    Ok(())
}

impl Db {
    pub async fn upsert_driver_declaration(&self, r: &DriverDeclarationRecord) -> Result<(), sqlx::Error> {
        upsert_driver_declaration(self.pool(), r).await
    }

    pub async fn list_active_driver_declarations(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<DriverDeclarationRecord>, sqlx::Error> {
        list_active_driver_declarations(self.pool(), workspace_id).await
    }

    pub async fn list_all_driver_declarations(&self) -> Result<Vec<DriverDeclarationRecord>, sqlx::Error> {
        list_all_driver_declarations(self.pool()).await
    }

    pub async fn upsert_device_declaration(&self, r: &DeviceDeclarationRecord) -> Result<(), sqlx::Error> {
        upsert_device_declaration(self.pool(), r).await
    }

    pub async fn list_device_declarations(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<DeviceDeclarationRecord>, sqlx::Error> {
        list_device_declarations(self.pool(), workspace_id).await
    }

    pub async fn find_device_declaration(
        &self,
        workspace_id: &str,
        thing_id: &str,
    ) -> Result<Option<DeviceDeclarationRecord>, sqlx::Error> {
        find_device_declaration(self.pool(), workspace_id, thing_id).await
    }

    pub async fn insert_acceptance_report(&self, r: &AcceptanceReportRecord) -> Result<(), sqlx::Error> {
        insert_acceptance_report(self.pool(), r).await
    }

    pub async fn upsert_declared_properties(
        &self,
        thing_id: &str,
        fields: &[StateFieldDecl],
    ) -> Result<(), sqlx::Error> {
        upsert_declared_properties(self.pool(), thing_id, fields).await
    }

    pub async fn upsert_declared_commands(&self, thing_id: &str, tools: &[ManifestTool]) -> Result<(), sqlx::Error> {
        upsert_declared_commands(self.pool(), thing_id, tools).await
    }

    pub async fn mark_stale_declared_capabilities(
        &self,
        thing_id: &str,
        keep_properties: &[String],
        keep_commands: &[String],
    ) -> Result<(), sqlx::Error> {
        mark_stale_declared_capabilities(self.pool(), thing_id, keep_properties, keep_commands).await
    }

    /// 按 id 加载物 + 属性（无 workspace 过滤：调用方已完成鉴权）。
    pub async fn find_thing_with_properties(
        &self,
        thing_id: &str,
    ) -> Result<Option<tinyiothub_core::models::thing::Thing>, sqlx::Error> {
        let Some(mut thing) = crate::thing::find_thing_by_id(self.pool(), None, thing_id)
            .await
            .map_err(|e| sqlx::Error::Protocol(e.to_string()))?
        else {
            return Ok(None);
        };
        thing.properties = Some(crate::thing_property::find_thing_properties_by_thing_id(self.pool(), thing_id).await?);
        Ok(Some(thing))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::test_pool;
    use tinyiothub_core::driver::declaration::{DeviceDeclarationRecord, DriverDeclarationRecord};
    use tinyiothub_core::driver::manifest::{NumericRange, StateFieldDecl, ToolKindDecl};

    fn driver_rec() -> DriverDeclarationRecord {
        DriverDeclarationRecord {
            workspace_id: "ws1".into(),
            driver_name: "demo".into(),
            driver_version: "1.0.0".into(),
            manifest_version: "1.0".into(),
            manifest_json: "{}".into(),
            manifest_hash: "h1".into(),
            binary_hash: Some("b1".into()),
            status: "active".into(),
            installed_at: "2026-10-21T00:00:00Z".into(),
        }
    }

    /// things.workspace_id 有 FK → workspaces(id)：先补租户/工作区链。
    async fn setup_thing(db: &Db) {
        crate::seed::seed_system(db).await.unwrap();
        sqlx::query(
            "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) \
             VALUES ('ws1','ws1','tenant-default-001','2026-10-21','2026-10-21')",
        )
        .execute(db.pool())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO things (id, name, thing_type, state, workspace_id, created_at, updated_at) \
             VALUES ('t1','t1','device',0,'ws1','2026-10-21','2026-10-21')",
        )
        .execute(db.pool())
        .await
        .unwrap();
    }

    fn field(name: &str) -> StateFieldDecl {
        StateFieldDecl {
            name: name.into(),
            data_type: "integer".into(),
            unit: Some("%".into()),
            range: Some(NumericRange {
                min: Some(0.0),
                max: Some(100.0),
            }),
            read_only: false,
            required: true,
            source: Some("telemetry".into()),
        }
    }

    fn tool(name: &str) -> ManifestTool {
        ManifestTool {
            name: name.into(),
            kind: ToolKindDecl::Action,
            target_field: None,
            parameters: serde_json::json!({"type": "object"}),
            timeout: Default::default(),
            safety: Default::default(),
            resources: vec![],
            cancellable: true,
        }
    }

    async fn sources(db: &Db, sql: &'static str) -> Vec<(String, String)> {
        sqlx::query_as(sql).fetch_all(db.pool()).await.unwrap()
    }

    #[tokio::test]
    async fn upserts_and_retires_old_versions() {
        let db = Db::new(test_pool().await);
        db.upsert_driver_declaration(&driver_rec()).await.unwrap();
        let mut r2 = driver_rec();
        r2.driver_version = "1.1.0".into();
        r2.manifest_hash = "h2".into();
        db.upsert_driver_declaration(&r2).await.unwrap();
        let active = db.list_active_driver_declarations("ws1").await.unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].driver_version, "1.1.0");
        assert_eq!(active[0].manifest_hash, "h2");
    }

    #[tokio::test]
    async fn upserts_declared_properties_and_marks_stale() {
        let db = Db::new(test_pool().await);
        setup_thing(&db).await;
        db.upsert_declared_properties("t1", &[field("level")]).await.unwrap();
        db.mark_stale_declared_capabilities("t1", &[], &[]).await.unwrap();
        let props = db.find_thing_properties_by_thing_id("t1").await.unwrap();
        assert_eq!(props[0].name, "level");
        assert_eq!(props[0].source.as_deref(), Some("stale"));
    }

    #[tokio::test]
    async fn mark_stale_keeps_named_capabilities_and_stales_the_rest() {
        let db = Db::new(test_pool().await);
        setup_thing(&db).await;
        db.upsert_declared_properties("t1", &[field("level"), field("humidity")])
            .await
            .unwrap();
        db.upsert_declared_commands("t1", &[tool("set_level"), tool("reboot")])
            .await
            .unwrap();

        db.mark_stale_declared_capabilities("t1", &["level".into()], &["set_level".into()])
            .await
            .unwrap();

        let props = sources(
            &db,
            "SELECT name, source FROM thing_properties WHERE thing_id='t1' ORDER BY name",
        )
        .await;
        assert_eq!(
            props,
            vec![
                ("humidity".to_string(), "stale".to_string()),
                ("level".to_string(), "driver".to_string()),
            ]
        );
        let cmds = sources(
            &db,
            "SELECT name, source FROM thing_actions WHERE thing_id='t1' ORDER BY name",
        )
        .await;
        assert_eq!(
            cmds,
            vec![
                ("reboot".to_string(), "stale".to_string()),
                ("set_level".to_string(), "driver".to_string()),
            ]
        );
    }

    #[tokio::test]
    async fn mark_stale_command_only_keeps_named_command() {
        let db = Db::new(test_pool().await);
        setup_thing(&db).await;
        db.upsert_declared_commands("t1", &[tool("reboot"), tool("set_level")])
            .await
            .unwrap();

        db.mark_stale_declared_capabilities("t1", &[], &["reboot".into()])
            .await
            .unwrap();

        let props = sources(
            &db,
            "SELECT name, source FROM thing_properties WHERE thing_id='t1' ORDER BY name",
        )
        .await;
        assert!(props.is_empty(), "command-only thing must have no props: {props:?}");
        let cmds = sources(
            &db,
            "SELECT name, source FROM thing_actions WHERE thing_id='t1' ORDER BY name",
        )
        .await;
        assert_eq!(
            cmds,
            vec![
                ("reboot".to_string(), "driver".to_string()),
                ("set_level".to_string(), "stale".to_string()),
            ]
        );
    }

    #[tokio::test]
    async fn device_declaration_round_trip() {
        let db = Db::new(test_pool().await);
        let rec = DeviceDeclarationRecord {
            workspace_id: "ws1".into(),
            thing_id: "t1".into(),
            driver_name: "demo".into(),
            driver_version: "1.0.0".into(),
            declaration_json: "{}".into(),
            declaration_hash: "dh".into(),
            status: "active".into(),
            updated_at: "2026-10-21".into(),
        };
        db.upsert_device_declaration(&rec).await.unwrap();
        let got = db.find_device_declaration("ws1", "t1").await.unwrap().unwrap();
        assert_eq!(got.declaration_hash, "dh");
    }
}
