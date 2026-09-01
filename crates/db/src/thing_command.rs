use sqlx::{FromRow, SqlitePool};

use crate::database::Db;
use tinyiothub_core::models::thing_command::*;
use tinyiothub_core::{generate_id, now_string};

/// Internal row type for sqlx mapping
#[derive(Debug, Clone, FromRow)]
struct ThingCommandRow {
    id: String,
    thing_id: String,
    name: String,
    display_name: Option<String>,
    description: Option<String>,
    parameters: Option<String>,
    created_at: String,
}

impl From<ThingCommandRow> for ThingCommand {
    fn from(row: ThingCommandRow) -> Self {
        Self {
            id: row.id,
            thing_id: row.thing_id,
            name: row.name,
            display_name: row.display_name,
            description: row.description,
            parameters: row.parameters,
            created_at: row.created_at,
        }
    }
}

/// Find a device command by ID
pub(crate) async fn find_thing_command_by_id(pool: &SqlitePool, id: &str) -> Result<Option<ThingCommand>, sqlx::Error> {
    let row = sqlx::query_as::<_, ThingCommandRow>(
        r#"
        SELECT id, thing_id, name, display_name, description, parameters, created_at
        FROM thing_actions WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(Into::into))
}

/// Create a new device command
pub(crate) async fn create_thing_command(
    pool: &SqlitePool,
    request: &CreateThingCommandRequest,
) -> Result<ThingCommand, sqlx::Error> {
    let id = generate_id();
    let created_at = now_string();

    let mut tx = pool.begin().await?;

    sqlx::query(
        r#"
        INSERT INTO thing_actions (id, thing_id, name, display_name, description, parameters, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(&request.thing_id)
    .bind(&request.name)
    .bind(&request.display_name)
    .bind(&request.description)
    .bind(&request.parameters)
    .bind(&created_at)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(ThingCommand {
        id,
        thing_id: request.thing_id.clone(),
        name: request.name.clone(),
        display_name: request.display_name.clone(),
        description: request.description.clone(),
        parameters: request.parameters.clone(),
        created_at,
    })
}

/// Find commands by device ID
pub(crate) async fn find_thing_commands_by_thing_id(
    pool: &SqlitePool,
    thing_id: &str,
) -> Result<Vec<ThingCommand>, sqlx::Error> {
    let rows = sqlx::query_as::<_, ThingCommandRow>(
        r#"
        SELECT id, thing_id, name, display_name, description, parameters, created_at
        FROM thing_actions WHERE thing_id = ?
        ORDER BY name ASC
        "#,
    )
    .bind(thing_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(Into::into).collect())
}

/// Find command by device ID and name
pub(crate) async fn find_thing_command_by_thing_and_name(
    pool: &SqlitePool,
    thing_id: &str,
    name: &str,
) -> Result<Option<ThingCommand>, sqlx::Error> {
    let row = sqlx::query_as::<_, ThingCommandRow>(
        r#"
        SELECT id, thing_id, name, display_name, description, parameters, created_at
        FROM thing_actions WHERE thing_id = ? AND name = ?
        "#,
    )
    .bind(thing_id)
    .bind(name)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(Into::into))
}

/// 在调用方事务内批量插入设备指令（场景实例化器专用），返回新建指令列表。
/// 不自行 commit/rollback；公开入口 bulk_create_thing_commands 是薄包装。
pub(crate) async fn bulk_create_thing_commands_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    requests: &[CreateThingCommandRequest],
) -> Result<Vec<ThingCommand>, sqlx::Error> {
    let mut created_commands = Vec::new();

    for request in requests {
        let id = generate_id();
        let created_at = now_string();

        sqlx::query(
            r#"
            INSERT INTO thing_actions (id, thing_id, name, display_name, description, parameters, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&request.thing_id)
        .bind(&request.name)
        .bind(&request.display_name)
        .bind(&request.description)
        .bind(&request.parameters)
        .bind(&created_at)
        .execute(&mut **tx)
        .await?;

        created_commands.push(ThingCommand {
            id,
            thing_id: request.thing_id.clone(),
            name: request.name.clone(),
            display_name: request.display_name.clone(),
            description: request.description.clone(),
            parameters: request.parameters.clone(),
            created_at,
        });
    }

    Ok(created_commands)
}

/// Bulk create device commands
pub(crate) async fn bulk_create_thing_commands(
    pool: &SqlitePool,
    requests: &[CreateThingCommandRequest],
) -> Result<Vec<ThingCommand>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let created_commands = bulk_create_thing_commands_tx(&mut tx, requests).await?;
    tx.commit().await?;
    Ok(created_commands)
}

impl Db {
    /// 按 ID 查设备指令。
    pub async fn find_thing_command_by_id(&self, id: &str) -> Result<Option<ThingCommand>, sqlx::Error> {
        find_thing_command_by_id(self.pool(), id).await
    }

    /// 场景实例化器：在调用方事务内批量插入设备指令。
    pub async fn bulk_create_thing_commands_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        requests: &[CreateThingCommandRequest],
    ) -> Result<Vec<ThingCommand>, sqlx::Error> {
        bulk_create_thing_commands_tx(tx, requests).await
    }

    /// 创建一条设备指令（内部事务）。
    pub async fn create_thing_command(&self, request: &CreateThingCommandRequest) -> Result<ThingCommand, sqlx::Error> {
        create_thing_command(self.pool(), request).await
    }

    /// 按设备 ID 列出指令（按名称升序）。
    pub async fn find_thing_commands_by_thing_id(&self, thing_id: &str) -> Result<Vec<ThingCommand>, sqlx::Error> {
        find_thing_commands_by_thing_id(self.pool(), thing_id).await
    }

    /// 按设备 ID + 指令名查指令。
    pub async fn find_thing_command_by_thing_and_name(
        &self,
        thing_id: &str,
        name: &str,
    ) -> Result<Option<ThingCommand>, sqlx::Error> {
        find_thing_command_by_thing_and_name(self.pool(), thing_id, name).await
    }

    /// 批量创建设备指令（内部事务）。
    pub async fn bulk_create_thing_commands(
        &self,
        requests: &[CreateThingCommandRequest],
    ) -> Result<Vec<ThingCommand>, sqlx::Error> {
        bulk_create_thing_commands(self.pool(), requests).await
    }
}
