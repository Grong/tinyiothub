use sqlx::Row;

use crate::sqlite::database::Database;
use tinyiothub_core::generate_id;
use tinyiothub_core::models::notification_channel::*;

/// 根据 ID 查询通知渠道
pub async fn find_notification_channel_by_id(
    db: &Database,
    id: &str,
) -> Result<Option<NotificationChannel>, sqlx::Error> {
    let row = sqlx::query("SELECT * FROM notification_channels WHERE id = ? LIMIT 1")
        .bind(id)
        .fetch_optional(db.pool())
        .await?;

    if let Some(row) = row {
        Ok(Some(NotificationChannel {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            channel_type: row.try_get("channel_type")?,
            config: row.try_get("config")?,
            is_enabled: row.try_get::<i32, _>("is_enabled")? != 0,
            description: row.try_get("description")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
            workspace_id: row.try_get("workspace_id").ok(),
        }))
    } else {
        Ok(None)
    }
}

/// Count channels with filters
pub async fn count_notification_channels(
    db: &Database,
    params: &NotificationChannelQueryParams,
) -> Result<i64, sqlx::Error> {
    let mut query_builder =
        sqlx::query_builder::QueryBuilder::new("SELECT COUNT(*) FROM notification_channels WHERE 1=1");

    if let Some(ref channel_type) = params.channel_type {
        query_builder.push(" AND channel_type = ");
        query_builder.push_bind(channel_type);
    }
    if let Some(is_enabled) = params.is_enabled {
        query_builder.push(" AND is_enabled = ");
        query_builder.push_bind(if is_enabled { 1 } else { 0 });
    }
    if let Some(ref ws_id) = params.workspace_id {
        query_builder.push(" AND workspace_id = ");
        query_builder.push_bind(ws_id);
    }

    let row = query_builder.build().fetch_one(db.pool()).await?;
    let count: i64 = row.try_get(0)?;
    Ok(count)
}

/// 查询所有通知渠道
pub async fn find_all_notification_channels(
    db: &Database,
    params: &NotificationChannelQueryParams,
) -> Result<Vec<NotificationChannel>, sqlx::Error> {
    let page = params.page.unwrap_or(1);
    let page_size = params.page_size.unwrap_or(20);
    let offset = (page - 1) * page_size;

    let mut query_builder = sqlx::query_builder::QueryBuilder::new("SELECT * FROM notification_channels WHERE 1=1");

    if let Some(ref channel_type) = params.channel_type {
        query_builder.push(" AND channel_type = ");
        query_builder.push_bind(channel_type);
    }
    if let Some(is_enabled) = params.is_enabled {
        query_builder.push(" AND is_enabled = ");
        query_builder.push_bind(if is_enabled { 1 } else { 0 });
    }
    if let Some(ref ws_id) = params.workspace_id {
        query_builder.push(" AND workspace_id = ");
        query_builder.push_bind(ws_id);
    }

    query_builder.push(" ORDER BY created_at DESC");
    query_builder.push(" LIMIT ");
    query_builder.push_bind(page_size as i64);
    query_builder.push(" OFFSET ");
    query_builder.push_bind(offset as i64);

    let query = query_builder.build();
    let rows = query.fetch_all(db.pool()).await?;

    rows.into_iter()
        .map(|row| {
            Ok(NotificationChannel {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                channel_type: row.try_get("channel_type")?,
                config: row.try_get("config")?,
                is_enabled: row.try_get::<i32, _>("is_enabled")? != 0,
                description: row.try_get("description")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
                workspace_id: row.try_get("workspace_id").ok(),
            })
        })
        .collect()
}

/// 创建通知渠道
pub async fn create_notification_channel(
    db: &Database,
    req: &CreateNotificationChannelRequest,
    workspace_id: Option<&str>,
) -> Result<NotificationChannel, sqlx::Error> {
    let id = generate_id();
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    sqlx::query(
        r#"
        INSERT INTO notification_channels (id, name, channel_type, config, is_enabled, description, created_at, updated_at, workspace_id)
        VALUES (?, ?, ?, ?, 1, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(&req.name)
    .bind(&req.channel_type)
    .bind(&req.config)
    .bind(req.description.as_deref().unwrap_or(""))
    .bind(&now)
    .bind(&now)
    .bind(workspace_id)
    .execute(db.pool())
    .await?;

    find_notification_channel_by_id(db, &id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

/// 更新通知渠道；workspace_id 用于 WHERE 子句确保租户隔离
pub async fn update_notification_channel(
    db: &Database,
    id: &str,
    req: &UpdateNotificationChannelRequest,
    workspace_id: Option<&str>,
) -> Result<NotificationChannel, sqlx::Error> {
    let now = chrono::Utc::now().to_rfc3339();

    let mut query_builder = sqlx::query_builder::QueryBuilder::new("UPDATE notification_channels SET updated_at = ");
    query_builder.push_bind(&now);

    if let Some(ref name) = req.name {
        query_builder.push(", name = ");
        query_builder.push_bind(name);
    }
    if let Some(ref channel_type) = req.channel_type {
        query_builder.push(", channel_type = ");
        query_builder.push_bind(channel_type);
    }
    if let Some(ref config) = req.config {
        query_builder.push(", config = ");
        query_builder.push_bind(config);
    }
    if let Some(ref description) = req.description {
        query_builder.push(", description = ");
        query_builder.push_bind(description);
    }

    query_builder.push(" WHERE id = ");
    query_builder.push_bind(id);
    if let Some(ws) = workspace_id {
        query_builder.push(" AND workspace_id = ");
        query_builder.push_bind(ws);
    }

    query_builder.build().execute(db.pool()).await?;

    find_notification_channel_by_id(db, id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

/// 删除通知渠道；workspace_id 用于 WHERE 子句确保租户隔离
pub async fn delete_notification_channel(
    db: &Database,
    id: &str,
    workspace_id: Option<&str>,
) -> Result<u64, sqlx::Error> {
    let query = if workspace_id.is_some() {
        "DELETE FROM notification_channels WHERE id = ? AND workspace_id = ?"
    } else {
        "DELETE FROM notification_channels WHERE id = ?"
    };
    let mut sqlx_query = sqlx::query(query).bind(id);
    if let Some(ws) = workspace_id {
        sqlx_query = sqlx_query.bind(ws);
    }
    let result = sqlx_query.execute(db.pool()).await?;
    Ok(result.rows_affected())
}

/// 设置启用/禁用
pub async fn set_notification_channel_enabled(
    db: &Database,
    id: &str,
    is_enabled: bool,
) -> Result<NotificationChannel, sqlx::Error> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE notification_channels SET is_enabled = ?, updated_at = ? WHERE id = ?")
        .bind(if is_enabled { 1 } else { 0 })
        .bind(&now)
        .bind(id)
        .execute(db.pool())
        .await?;

    find_notification_channel_by_id(db, id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

/// 获取统计
pub async fn get_notification_channel_statistics(
    db: &Database,
    workspace_id: Option<&str>,
) -> Result<ChannelStatistics, sqlx::Error> {
    let total: i64 = if let Some(ws_id) = workspace_id {
        sqlx::query_scalar("SELECT COUNT(*) FROM notification_channels WHERE workspace_id = ?")
            .bind(ws_id)
            .fetch_one(db.pool())
            .await?
    } else {
        sqlx::query_scalar("SELECT COUNT(*) FROM notification_channels")
            .fetch_one(db.pool())
            .await?
    };

    let enabled: i64 = if let Some(ws_id) = workspace_id {
        sqlx::query_scalar("SELECT COUNT(*) FROM notification_channels WHERE is_enabled = 1 AND workspace_id = ?")
            .bind(ws_id)
            .fetch_one(db.pool())
            .await?
    } else {
        sqlx::query_scalar("SELECT COUNT(*) FROM notification_channels WHERE is_enabled = 1")
            .fetch_one(db.pool())
            .await?
    };

    let sms: i64 = if let Some(ws_id) = workspace_id {
        sqlx::query_scalar("SELECT COUNT(*) FROM notification_channels WHERE channel_type = 'sms' AND workspace_id = ?")
            .bind(ws_id)
            .fetch_one(db.pool())
            .await?
    } else {
        sqlx::query_scalar("SELECT COUNT(*) FROM notification_channels WHERE channel_type = 'sms'")
            .fetch_one(db.pool())
            .await?
    };

    let email: i64 = if let Some(ws_id) = workspace_id {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM notification_channels WHERE channel_type = 'email' AND workspace_id = ?",
        )
        .bind(ws_id)
        .fetch_one(db.pool())
        .await?
    } else {
        sqlx::query_scalar("SELECT COUNT(*) FROM notification_channels WHERE channel_type = 'email'")
            .fetch_one(db.pool())
            .await?
    };

    let webhook: i64 = if let Some(ws_id) = workspace_id {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM notification_channels WHERE channel_type = 'webhook' AND workspace_id = ?",
        )
        .bind(ws_id)
        .fetch_one(db.pool())
        .await?
    } else {
        sqlx::query_scalar("SELECT COUNT(*) FROM notification_channels WHERE channel_type = 'webhook'")
            .fetch_one(db.pool())
            .await?
    };

    Ok(ChannelStatistics {
        total_channels: total,
        enabled_channels: enabled,
        sms_channels: sms,
        email_channels: email,
        webhook_channels: webhook,
    })
}
