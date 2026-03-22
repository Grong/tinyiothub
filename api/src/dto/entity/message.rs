use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Row};

use crate::infrastructure::persistence::database::Database;

/// Message entity for system messages, events, and alarms
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Message {
    pub id: String,
    pub level: i32,
    pub create_date_time: String,
    pub title: String,
    pub content: Option<String>, // JSON string
    pub message_type: Option<String>,
    pub device_type: Option<String>,
    pub is_disabled: i32,
    pub confirmor: Option<String>,
    pub confirm_time: Option<String>,
    pub confirm_result: Option<String>,
    pub child_object: Option<String>,
    pub false_positive: i32,
}

/// Query parameters for message search
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct MessageQueryParams {
    pub level: Option<i32>,
    pub message_type: Option<String>,
    pub device_type: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub is_disabled: Option<i32>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// Request for creating a new message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateMessageRequest {
    pub level: i32,
    pub title: String,
    pub content: Option<String>,
    pub message_type: Option<String>,
    pub device_type: Option<String>,
    pub child_object: Option<String>,
}

/// Request for updating a message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateMessageRequest {
    pub title: Option<String>,
    pub content: Option<String>,
    pub message_type: Option<String>,
    pub device_type: Option<String>,
    pub confirmor: Option<String>,
    pub confirm_time: Option<String>,
    pub confirm_result: Option<String>,
    pub child_object: Option<String>,
    pub false_positive: Option<i32>,
    pub is_disabled: Option<i32>,
}

impl Message {
    /// Find a message by ID
    pub async fn find_by_id(db: &Database, id: &str) -> Result<Option<Message>, sqlx::Error> {
        let message = sqlx::query_as::<_, Message>(
            r#"
            SELECT id as id, Level as level, create_date_time as create_date_time, 
                   Title as title, Content as content, type as message_type, 
                   device_type as device_type, is_disabled as is_disabled,
                   Confirmor as confirmor, confirm_time as confirm_time, 
                   confirm_result as confirm_result, child_object as child_object, 
                   false_positive as false_positive
            FROM Messages WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(db.pool())
        .await?;

        Ok(message)
    }

    /// Create a new message
    pub async fn create(
        db: &Database,
        request: &CreateMessageRequest,
    ) -> Result<Message, sqlx::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let create_date_time = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        sqlx::query(
            r#"
            INSERT INTO Messages (
                id, Level, create_date_time, Title, Content, type, device_type, is_disabled,
                Confirmor, confirm_time, confirm_result, child_object, false_positive
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(request.level)
        .bind(&create_date_time)
        .bind(&request.title)
        .bind(&request.content)
        .bind(&request.message_type)
        .bind(&request.device_type)
        .bind(0) // is_disabled
        .bind(None::<String>) // confirmor
        .bind(None::<String>) // confirm_time
        .bind(None::<String>) // confirm_result
        .bind(&request.child_object)
        .bind(0) // false_positive
        .execute(db.pool())
        .await?;

        // Return the created message
        Message::find_by_id(db, &id).await?.ok_or_else(|| sqlx::Error::RowNotFound)
    }

    /// Update a message
    pub async fn update(
        db: &Database,
        id: &str,
        request: &UpdateMessageRequest,
    ) -> Result<Message, sqlx::Error> {
        let mut query = QueryBuilder::new("UPDATE Messages SET ");
        let mut has_updates = false;

        if let Some(title) = &request.title {
            if has_updates {
                query.push(", ");
            }
            query.push("Title = ").push_bind(title);
            has_updates = true;
        }

        if let Some(content) = &request.content {
            if has_updates {
                query.push(", ");
            }
            query.push("Content = ").push_bind(content);
            has_updates = true;
        }

        if let Some(message_type) = &request.message_type {
            if has_updates {
                query.push(", ");
            }
            query.push("type = ").push_bind(message_type);
            has_updates = true;
        }

        if let Some(device_type) = &request.device_type {
            if has_updates {
                query.push(", ");
            }
            query.push("device_type = ").push_bind(device_type);
            has_updates = true;
        }

        if let Some(confirmor) = &request.confirmor {
            if has_updates {
                query.push(", ");
            }
            query.push("Confirmor = ").push_bind(confirmor);
            has_updates = true;
        }

        if let Some(confirm_time) = &request.confirm_time {
            if has_updates {
                query.push(", ");
            }
            query.push("confirm_time = ").push_bind(confirm_time);
            has_updates = true;
        }

        if let Some(confirm_result) = &request.confirm_result {
            if has_updates {
                query.push(", ");
            }
            query.push("confirm_result = ").push_bind(confirm_result);
            has_updates = true;
        }

        if let Some(child_object) = &request.child_object {
            if has_updates {
                query.push(", ");
            }
            query.push("child_object = ").push_bind(child_object);
            has_updates = true;
        }

        if let Some(false_positive) = request.false_positive {
            if has_updates {
                query.push(", ");
            }
            query.push("false_positive = ").push_bind(false_positive);
            has_updates = true;
        }

        if let Some(is_disabled) = request.is_disabled {
            if has_updates {
                query.push(", ");
            }
            query.push("is_disabled = ").push_bind(is_disabled);
            has_updates = true;
        }

        if !has_updates {
            return Err(sqlx::Error::RowNotFound);
        }

        query.push(" WHERE id = ").push_bind(id);

        query.build().execute(db.pool()).await?;

        // Return the updated message
        Message::find_by_id(db, id).await?.ok_or_else(|| sqlx::Error::RowNotFound)
    }

    /// Delete a message
    pub async fn delete(db: &Database, id: &str) -> Result<u64, sqlx::Error> {
        let result =
            sqlx::query("DELETE FROM Messages WHERE id = ?").bind(id).execute(db.pool()).await?;

        Ok(result.rows_affected())
    }

    /// Find all messages with optional filtering
    pub async fn find_all(
        db: &Database,
        params: &MessageQueryParams,
    ) -> Result<Vec<Message>, sqlx::Error> {
        let mut query = QueryBuilder::new(
            r#"
            SELECT id as id, Level as level, create_date_time as create_date_time, 
                   Title as title, Content as content, type as message_type, 
                   device_type as device_type, is_disabled as is_disabled,
                   Confirmor as confirmor, confirm_time as confirm_time, 
                   confirm_result as confirm_result, child_object as child_object, 
                   false_positive as false_positive
            FROM Messages WHERE 1=1
            "#,
        );

        if let Some(level) = params.level {
            query.push(" AND Level = ").push_bind(level);
        }

        if let Some(message_type) = &params.message_type {
            query.push(" AND type = ").push_bind(message_type);
        }

        if let Some(device_type) = &params.device_type {
            query.push(" AND device_type = ").push_bind(device_type);
        }

        if let Some(start_date) = &params.start_date {
            query.push(" AND create_date_time >= ").push_bind(start_date);
        }

        if let Some(end_date) = &params.end_date {
            query.push(" AND create_date_time <= ").push_bind(end_date);
        }

        if let Some(is_disabled) = params.is_disabled {
            query.push(" AND is_disabled = ").push_bind(is_disabled);
        }

        query.push(" ORDER BY create_date_time DESC");

        if let Some(page_size) = params.page_size {
            let offset = params.page.unwrap_or(1).saturating_sub(1) * page_size;
            query.push(" LIMIT ").push_bind(page_size as i64);
            query.push(" OFFSET ").push_bind(offset as i64);
        }

        let messages = query.build_query_as::<Message>().fetch_all(db.pool()).await?;

        Ok(messages)
    }

    /// Count messages with optional filtering
    pub async fn count(db: &Database, params: &MessageQueryParams) -> Result<i64, sqlx::Error> {
        let mut query = QueryBuilder::new("SELECT COUNT(*) as count FROM Messages WHERE 1=1");

        if let Some(level) = params.level {
            query.push(" AND Level = ").push_bind(level);
        }

        if let Some(message_type) = &params.message_type {
            query.push(" AND type = ").push_bind(message_type);
        }

        if let Some(device_type) = &params.device_type {
            query.push(" AND device_type = ").push_bind(device_type);
        }

        if let Some(start_date) = &params.start_date {
            query.push(" AND create_date_time >= ").push_bind(start_date);
        }

        if let Some(end_date) = &params.end_date {
            query.push(" AND create_date_time <= ").push_bind(end_date);
        }

        if let Some(is_disabled) = params.is_disabled {
            query.push(" AND is_disabled = ").push_bind(is_disabled);
        }

        let row = query.build().fetch_one(db.pool()).await?;
        let count: i64 = row.get("count");

        Ok(count)
    }

    /// Get messages by level
    pub async fn find_by_level(db: &Database, level: i32) -> Result<Vec<Message>, sqlx::Error> {
        let messages = sqlx::query_as::<_, Message>(
            r#"
            SELECT id as id, Level as level, create_date_time as create_date_time, 
                   Title as title, Content as content, type as message_type, 
                   device_type as device_type, is_disabled as is_disabled,
                   Confirmor as confirmor, confirm_time as confirm_time, 
                   confirm_result as confirm_result, child_object as child_object, 
                   false_positive as false_positive
            FROM Messages WHERE Level = ? AND is_disabled = 0
            ORDER BY create_date_time DESC
            "#,
        )
        .bind(level)
        .fetch_all(db.pool())
        .await?;

        Ok(messages)
    }

    /// Get recent messages (last 24 hours)
    pub async fn find_recent(db: &Database) -> Result<Vec<Message>, sqlx::Error> {
        let messages = sqlx::query_as::<_, Message>(
            r#"
            SELECT id as id, Level as level, create_date_time as create_date_time, 
                   Title as title, Content as content, type as message_type, 
                   device_type as device_type, is_disabled as is_disabled,
                   Confirmor as confirmor, confirm_time as confirm_time, 
                   confirm_result as confirm_result, child_object as child_object, 
                   false_positive as false_positive
            FROM Messages 
            WHERE create_date_time >= datetime('now', '-1 day') AND is_disabled = 0
            ORDER BY create_date_time DESC
            "#,
        )
        .fetch_all(db.pool())
        .await?;

        Ok(messages)
    }

    /// Get message statistics
    pub async fn get_statistics(db: &Database) -> Result<MessageStatistics, sqlx::Error> {
        let total_row = sqlx::query("SELECT COUNT(*) as count FROM Messages WHERE is_disabled = 0")
            .fetch_one(db.pool())
            .await?;

        let unconfirmed_row = sqlx::query(
            "SELECT COUNT(*) as count FROM Messages WHERE is_disabled = 0 AND Confirmor IS NULL",
        )
        .fetch_one(db.pool())
        .await?;

        let recent_row = sqlx::query("SELECT COUNT(*) as count FROM Messages WHERE is_disabled = 0 AND create_date_time >= datetime('now', '-1 day')")
            .fetch_one(db.pool())
            .await?;

        Ok(MessageStatistics {
            total_count: total_row.get("count"),
            unconfirmed_count: unconfirmed_row.get("count"),
            recent_count: recent_row.get("count"),
        })
    }

    // Backward compatibility methods
    pub async fn add_message(
        db: &Database,
        level: i32,
        title: String,
        content: Option<String>,
        message_type: Option<String>,
    ) -> Result<Message, sqlx::Error> {
        let request = CreateMessageRequest {
            level,
            title,
            content,
            message_type,
            device_type: None,
            child_object: None,
        };
        Self::create(db, &request).await
    }

    pub async fn clear_limit_messages(db: &Database, limit: i64) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            "DELETE FROM Messages WHERE id IN (SELECT id FROM Messages ORDER BY create_date_time ASC LIMIT ?)"
        )
        .bind(limit)
        .execute(db.pool())
        .await?;

        Ok(result.rows_affected())
    }

    pub fn new_with_text(level: i32, title: String, content: String) -> CreateMessageRequest {
        CreateMessageRequest {
            level,
            title,
            content: Some(content),
            message_type: None,
            device_type: None,
            child_object: None,
        }
    }
}

/// Message statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MessageStatistics {
    pub total_count: i64,
    pub unconfirmed_count: i64,
    pub recent_count: i64,
}

/// Message content structure for event handlers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MessageContent {
    pub content_type: Option<i32>,
    pub content: Option<String>,
}

/// DTO for backward compatibility
pub type MessageDto = Message;

impl Default for Message {
    fn default() -> Self {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            level: 0,
            create_date_time: now,
            title: String::new(),
            content: None,
            message_type: None,
            device_type: None,
            is_disabled: 0,
            confirmor: None,
            confirm_time: None,
            confirm_result: None,
            child_object: None,
            false_positive: 0,
        }
    }
}
