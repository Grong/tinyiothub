use crate::infrastructure::persistence::database::Database;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Row};

/// Operation record entity - 操作记录实体
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct OperationRecord {
    pub id: String,
    pub operation_type: String, // "create", "update", "delete", "login", "logout", etc.
    pub operation_time: String,
    pub operation_content: String, // JSON string with operation details
    pub user_id: Option<String>,
    pub user_name: Option<String>,
    pub target_type: Option<String>, // "device", "user", "role", etc.
    pub target_id: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub result: String, // "success", "failure", "partial"
    pub error_message: Option<String>,
    pub created_at: String,
}

/// Query parameters for operation record search
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct OperationRecordQuery {
    pub operation_type: Option<String>,
    pub user_id: Option<String>,
    pub user_name: Option<String>,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub result: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// Request for creating a new operation record
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateOperationRecordRequest {
    pub operation_type: String,
    pub operation_content: String,
    pub user_id: Option<String>,
    pub user_name: Option<String>,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub result: Option<String>,
    pub error_message: Option<String>,
}

/// Operation details for structured content
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct OperationDetails {
    pub action: String,
    pub resource: String,
    pub old_values: Option<serde_json::Value>,
    pub new_values: Option<serde_json::Value>,
    pub parameters: Option<serde_json::Value>,
    pub duration_ms: Option<u64>,
}

impl OperationRecord {
    /// Create a new operation record
    pub fn new(request: CreateOperationRecordRequest) -> Self {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            operation_type: request.operation_type,
            operation_time: now.clone(),
            operation_content: request.operation_content,
            user_id: request.user_id,
            user_name: request.user_name,
            target_type: request.target_type,
            target_id: request.target_id,
            ip_address: request.ip_address,
            user_agent: request.user_agent,
            result: request.result.unwrap_or_else(|| "success".to_string()),
            error_message: request.error_message,
            created_at: now,
        }
    }

    /// Find operation record by ID
    pub async fn find_by_id(
        db: &Database,
        id: &str,
    ) -> Result<Option<OperationRecord>, sqlx::Error> {
        let record = sqlx::query_as::<_, OperationRecord>(
            r#"
            SELECT id, operation_type, operation_time, operation_content, user_id, user_name,
                   target_type, target_id, ip_address, user_agent, result, error_message, created_at
            FROM OperationRecords WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(db.pool())
        .await?;

        Ok(record)
    }

    /// Create a new operation record in database
    pub async fn create(
        db: &Database,
        request: &CreateOperationRecordRequest,
    ) -> Result<OperationRecord, sqlx::Error> {
        let record = Self::new(request.clone());

        let mut tx = db.pool().begin().await?;

        sqlx::query(
            r#"
            INSERT INTO OperationRecords (
                id, operation_type, operation_time, operation_content, user_id, user_name,
                target_type, target_id, ip_address, user_agent, result, error_message, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&record.id)
        .bind(&record.operation_type)
        .bind(&record.operation_time)
        .bind(&record.operation_content)
        .bind(&record.user_id)
        .bind(&record.user_name)
        .bind(&record.target_type)
        .bind(&record.target_id)
        .bind(&record.ip_address)
        .bind(&record.user_agent)
        .bind(&record.result)
        .bind(&record.error_message)
        .bind(&record.created_at)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(record)
    }

    /// Find all operation records with filtering
    pub async fn find_all(
        db: &Database,
        query: &OperationRecordQuery,
    ) -> Result<Vec<OperationRecord>, sqlx::Error> {
        let mut sql_query = QueryBuilder::new(
            r#"
            SELECT id, operation_type, operation_time, operation_content, user_id, user_name,
                   target_type, target_id, ip_address, user_agent, result, error_message, created_at
            FROM OperationRecords WHERE 1=1
            "#,
        );

        if let Some(operation_type) = &query.operation_type {
            sql_query
                .push(" AND operation_type = ")
                .push_bind(operation_type);
        }

        if let Some(user_id) = &query.user_id {
            sql_query.push(" AND user_id = ").push_bind(user_id);
        }

        if let Some(user_name) = &query.user_name {
            sql_query
                .push(" AND user_name LIKE ")
                .push_bind(format!("%{}%", user_name));
        }

        if let Some(target_type) = &query.target_type {
            sql_query.push(" AND target_type = ").push_bind(target_type);
        }

        if let Some(target_id) = &query.target_id {
            sql_query.push(" AND target_id = ").push_bind(target_id);
        }

        if let Some(result) = &query.result {
            sql_query.push(" AND result = ").push_bind(result);
        }

        if let Some(start_time) = &query.start_time {
            sql_query
                .push(" AND operation_time >= ")
                .push_bind(start_time);
        }

        if let Some(end_time) = &query.end_time {
            sql_query
                .push(" AND operation_time <= ")
                .push_bind(end_time);
        }

        sql_query.push(" ORDER BY operation_time DESC");

        // Add pagination
        if let Some(page_size) = query.page_size {
            let offset = query.page.unwrap_or(1).saturating_sub(1) * page_size;
            sql_query.push(" LIMIT ").push_bind(page_size as i64);
            sql_query.push(" OFFSET ").push_bind(offset as i64);
        }

        let records = sql_query
            .build_query_as::<OperationRecord>()
            .fetch_all(db.pool())
            .await?;

        Ok(records)
    }

    /// Find records by user ID
    pub async fn find_by_user_id(
        db: &Database,
        user_id: &str,
        limit: Option<u32>,
    ) -> Result<Vec<OperationRecord>, sqlx::Error> {
        let mut query = QueryBuilder::new(
            r#"
            SELECT id, operation_type, operation_time, operation_content, user_id, user_name,
                   target_type, target_id, ip_address, user_agent, result, error_message, created_at
            FROM OperationRecords WHERE user_id = ?
            ORDER BY operation_time DESC
            "#,
        );

        if let Some(limit) = limit {
            query.push(" LIMIT ").push_bind(limit as i64);
        }

        let records = query
            .build()
            .bind(user_id)
            .fetch_all(db.pool())
            .await?
            .into_iter()
            .map(|row| OperationRecord {
                id: row.get("id"),
                operation_type: row.get("operation_type"),
                operation_time: row.get("operation_time"),
                operation_content: row.get("operation_content"),
                user_id: row.get("user_id"),
                user_name: row.get("user_name"),
                target_type: row.get("target_type"),
                target_id: row.get("target_id"),
                ip_address: row.get("ip_address"),
                user_agent: row.get("user_agent"),
                result: row.get("result"),
                error_message: row.get("error_message"),
                created_at: row.get("created_at"),
            })
            .collect();

        Ok(records)
    }

    /// Find records by operation type
    pub async fn find_by_operation_type(
        db: &Database,
        operation_type: &str,
        limit: Option<u32>,
    ) -> Result<Vec<OperationRecord>, sqlx::Error> {
        let mut query = QueryBuilder::new(
            r#"
            SELECT id, operation_type, operation_time, operation_content, user_id, user_name,
                   target_type, target_id, ip_address, user_agent, result, error_message, created_at
            FROM OperationRecords WHERE operation_type = ?
            ORDER BY operation_time DESC
            "#,
        );

        if let Some(limit) = limit {
            query.push(" LIMIT ").push_bind(limit as i64);
        }

        let records = query
            .build()
            .bind(operation_type)
            .fetch_all(db.pool())
            .await?
            .into_iter()
            .map(|row| OperationRecord {
                id: row.get("id"),
                operation_type: row.get("operation_type"),
                operation_time: row.get("operation_time"),
                operation_content: row.get("operation_content"),
                user_id: row.get("user_id"),
                user_name: row.get("user_name"),
                target_type: row.get("target_type"),
                target_id: row.get("target_id"),
                ip_address: row.get("ip_address"),
                user_agent: row.get("user_agent"),
                result: row.get("result"),
                error_message: row.get("error_message"),
                created_at: row.get("created_at"),
            })
            .collect();

        Ok(records)
    }

    /// Delete operation record
    pub async fn delete(db: &Database, id: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM OperationRecords WHERE id = ?")
            .bind(id)
            .execute(db.pool())
            .await?;

        Ok(result.rows_affected())
    }

    /// Delete old records (older than specified days)
    pub async fn delete_old_records(db: &Database, days: u32) -> Result<u64, sqlx::Error> {
        let cutoff_date = chrono::Utc::now() - chrono::Duration::days(days as i64);
        let cutoff_str = cutoff_date.format("%Y-%m-%d %H:%M:%S").to_string();

        let result = sqlx::query("DELETE FROM OperationRecords WHERE operation_time < ?")
            .bind(cutoff_str)
            .execute(db.pool())
            .await?;

        Ok(result.rows_affected())
    }

    /// Count operation records
    pub async fn count(db: &Database, query: &OperationRecordQuery) -> Result<i64, sqlx::Error> {
        let mut sql_query =
            QueryBuilder::new("SELECT COUNT(*) as count FROM OperationRecords WHERE 1=1");

        if let Some(operation_type) = &query.operation_type {
            sql_query
                .push(" AND operation_type = ")
                .push_bind(operation_type);
        }

        if let Some(user_id) = &query.user_id {
            sql_query.push(" AND user_id = ").push_bind(user_id);
        }

        if let Some(user_name) = &query.user_name {
            sql_query
                .push(" AND user_name LIKE ")
                .push_bind(format!("%{}%", user_name));
        }

        if let Some(target_type) = &query.target_type {
            sql_query.push(" AND target_type = ").push_bind(target_type);
        }

        if let Some(target_id) = &query.target_id {
            sql_query.push(" AND target_id = ").push_bind(target_id);
        }

        if let Some(result) = &query.result {
            sql_query.push(" AND result = ").push_bind(result);
        }

        if let Some(start_time) = &query.start_time {
            sql_query
                .push(" AND operation_time >= ")
                .push_bind(start_time);
        }

        if let Some(end_time) = &query.end_time {
            sql_query
                .push(" AND operation_time <= ")
                .push_bind(end_time);
        }

        let row = sql_query.build().fetch_one(db.pool()).await?;
        let count: i64 = row.get("count");

        Ok(count)
    }

    /// Get operation statistics by type
    pub async fn get_stats_by_type(
        db: &Database,
        days: Option<u32>,
    ) -> Result<Vec<(String, i64)>, sqlx::Error> {
        let mut query = QueryBuilder::new(
            "SELECT operation_type, COUNT(*) as count FROM OperationRecords WHERE 1=1",
        );

        if let Some(days) = days {
            let start_date = chrono::Utc::now() - chrono::Duration::days(days as i64);
            let start_str = start_date.format("%Y-%m-%d %H:%M:%S").to_string();
            query.push(" AND operation_time >= ").push_bind(start_str);
        }

        query.push(" GROUP BY operation_type ORDER BY count DESC");

        let rows = query.build().fetch_all(db.pool()).await?;

        let mut stats = Vec::new();
        for row in rows {
            let operation_type: String = row.get("operation_type");
            let count: i64 = row.get("count");
            stats.push((operation_type, count));
        }

        Ok(stats)
    }

    /// Get operation statistics by result
    pub async fn get_stats_by_result(
        db: &Database,
        days: Option<u32>,
    ) -> Result<Vec<(String, i64)>, sqlx::Error> {
        let mut query =
            QueryBuilder::new("SELECT result, COUNT(*) as count FROM OperationRecords WHERE 1=1");

        if let Some(days) = days {
            let start_date = chrono::Utc::now() - chrono::Duration::days(days as i64);
            let start_str = start_date.format("%Y-%m-%d %H:%M:%S").to_string();
            query.push(" AND operation_time >= ").push_bind(start_str);
        }

        query.push(" GROUP BY result ORDER BY count DESC");

        let rows = query.build().fetch_all(db.pool()).await?;

        let mut stats = Vec::new();
        for row in rows {
            let result: String = row.get("result");
            let count: i64 = row.get("count");
            stats.push((result, count));
        }

        Ok(stats)
    }

    /// Get recent failed operations
    pub async fn get_recent_failures(
        db: &Database,
        limit: Option<u32>,
    ) -> Result<Vec<OperationRecord>, sqlx::Error> {
        let mut query = QueryBuilder::new(
            r#"
            SELECT id, operation_type, operation_time, operation_content, user_id, user_name,
                   target_type, target_id, ip_address, user_agent, result, error_message, created_at
            FROM OperationRecords 
            WHERE result = 'failure'
            ORDER BY operation_time DESC
            "#,
        );

        if let Some(limit) = limit {
            query.push(" LIMIT ").push_bind(limit as i64);
        }

        let records = query
            .build_query_as::<OperationRecord>()
            .fetch_all(db.pool())
            .await?;

        Ok(records)
    }

    /// Log a successful operation
    pub async fn log_success(
        db: &Database,
        operation_type: String,
        operation_content: String,
        user_id: Option<String>,
        user_name: Option<String>,
        target_type: Option<String>,
        target_id: Option<String>,
    ) -> Result<OperationRecord, sqlx::Error> {
        let request = CreateOperationRecordRequest {
            operation_type,
            operation_content,
            user_id,
            user_name,
            target_type,
            target_id,
            ip_address: None,
            user_agent: None,
            result: Some("success".to_string()),
            error_message: None,
        };

        Self::create(db, &request).await
    }

    /// Log a failed operation
    pub async fn log_failure(
        db: &Database,
        operation_type: String,
        operation_content: String,
        error_message: String,
        user_id: Option<String>,
        user_name: Option<String>,
        target_type: Option<String>,
        target_id: Option<String>,
    ) -> Result<OperationRecord, sqlx::Error> {
        let request = CreateOperationRecordRequest {
            operation_type,
            operation_content,
            user_id,
            user_name,
            target_type,
            target_id,
            ip_address: None,
            user_agent: None,
            result: Some("failure".to_string()),
            error_message: Some(error_message),
        };

        Self::create(db, &request).await
    }

    /// Create a success operation record
    pub fn success(
        operation_type: String,
        operation_content: String,
        user_id: Option<String>,
        user_name: Option<String>,
    ) -> Self {
        Self::new(CreateOperationRecordRequest {
            operation_type,
            operation_content,
            user_id,
            user_name,
            target_type: None,
            target_id: None,
            ip_address: None,
            user_agent: None,
            result: Some("success".to_string()),
            error_message: None,
        })
    }

    /// Create a failure operation record
    pub fn failure(
        operation_type: String,
        operation_content: String,
        error_message: String,
        user_id: Option<String>,
        user_name: Option<String>,
    ) -> Self {
        Self::new(CreateOperationRecordRequest {
            operation_type,
            operation_content,
            user_id,
            user_name,
            target_type: None,
            target_id: None,
            ip_address: None,
            user_agent: None,
            result: Some("failure".to_string()),
            error_message: Some(error_message),
        })
    }

    /// Get operation details as structured data
    pub fn get_operation_details(&self) -> Option<OperationDetails> {
        serde_json::from_str(&self.operation_content).ok()
    }

    /// Set operation details from structured data
    pub fn set_operation_details(&mut self, details: OperationDetails) {
        if let Ok(json) = serde_json::to_string(&details) {
            self.operation_content = json;
        }
    }

    /// Check if operation was successful
    pub fn is_successful(&self) -> bool {
        self.result == "success"
    }

    /// Check if operation failed
    pub fn is_failed(&self) -> bool {
        self.result == "failure"
    }

    /// Get operation age in seconds
    pub fn get_operation_age_seconds(&self) -> i64 {
        let now = chrono::Utc::now();
        if let Ok(op_time) =
            chrono::NaiveDateTime::parse_from_str(&self.operation_time, "%Y-%m-%d %H:%M:%S")
        {
            let op_time_utc = chrono::DateTime::from_naive_utc_and_offset(op_time, chrono::Utc);
            (now - op_time_utc).num_seconds()
        } else {
            0
        }
    }

    /// Get user display name
    pub fn get_user_display_name(&self) -> String {
        self.user_name
            .clone()
            .or_else(|| self.user_id.clone())
            .unwrap_or_else(|| "Unknown".to_string())
    }
}

// Backward compatibility
pub type OptRecordDto = OperationRecord;
pub type OperationRecordQueryParams = OperationRecordQuery;
