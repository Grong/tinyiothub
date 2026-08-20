//! Event audit log 持久化：audit_logs 表（自 cloud event/security/audit_log.rs
//! 迁入，Task 12）。
//!
//! 注意：audit_logs 表由 `init_audit_log_storage` 惰性创建（CREATE TABLE IF NOT
//! EXISTS），沿用 cloud 原有运行时建表语义。
//!
//! 类型随 repo 住 db：AuditLogEntry，cloud 侧直接引用本模块路径。

use sqlx::SqlitePool;

use crate::database::Db;

// ──────────────────────────────────────────────
// 持久化类型 — 自 cloud audit_log.rs 迁入
// ──────────────────────────────────────────────

/// Audit log entry
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuditLogEntry {
    pub id: String,
    pub action: String,
    pub user_id: Option<String>,
    pub event_id: Option<String>,
    pub event_type: Option<String>,
    pub event_level: Option<String>,
    pub result: Option<String>,
    pub details: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub created_at: String,
}

impl AuditLogEntry {
    pub fn new(action: String, user_id: Option<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            action,
            user_id,
            event_id: None,
            event_type: None,
            event_level: None,
            result: Some("success".to_string()),
            details: None,
            ip_address: None,
            user_agent: None,
            created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        }
    }

    pub fn with_event_id(mut self, event_id: String) -> Self {
        self.event_id = Some(event_id);
        self
    }

    pub fn with_event_type(mut self, event_type: String) -> Self {
        self.event_type = Some(event_type);
        self
    }

    pub fn with_event_level(mut self, event_level: String) -> Self {
        self.event_level = Some(event_level);
        self
    }

    pub fn with_result(mut self, result: String) -> Self {
        self.result = Some(result);
        self
    }

    pub fn with_details(mut self, details: String) -> Self {
        self.details = Some(details);
        self
    }

    pub fn with_ip_address(mut self, ip_address: String) -> Self {
        self.ip_address = Some(ip_address);
        self
    }

    pub fn with_user_agent(mut self, user_agent: String) -> Self {
        self.user_agent = Some(user_agent);
        self
    }
}

// ──────────────────────────────────────────────
// 持久化函数（SQLite）
// ──────────────────────────────────────────────

/// Create audit log table + indexes if they don't exist.
pub(crate) async fn init_audit_log_storage(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    // Create audit log table if it doesn't exist
    let create_table_sql = r#"
            CREATE TABLE IF NOT EXISTS audit_logs (
                id TEXT PRIMARY KEY,
                action TEXT NOT NULL,
                user_id TEXT,
                event_id TEXT,
                event_type TEXT,
                event_level TEXT,
                result TEXT,
                details TEXT,
                ip_address TEXT,
                user_agent TEXT,
                created_at TEXT NOT NULL,
                FOREIGN KEY (event_id) REFERENCES events(id) ON DELETE SET NULL
            )
        "#;

    sqlx::query(create_table_sql).execute(pool).await?;

    // Create indexes for better query performance
    let create_indexes_sql = vec![
        "CREATE INDEX IF NOT EXISTS idx_audit_logs_user_id ON audit_logs(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_audit_logs_event_id ON audit_logs(event_id)",
        "CREATE INDEX IF NOT EXISTS idx_audit_logs_created_at ON audit_logs(created_at)",
        "CREATE INDEX IF NOT EXISTS idx_audit_logs_action ON audit_logs(action)",
    ];

    for sql in create_indexes_sql {
        sqlx::query(sql).execute(pool).await?;
    }

    Ok(())
}

/// 插入一条审计日志。
pub(crate) async fn insert_audit_log(pool: &SqlitePool, entry: &AuditLogEntry) -> Result<(), sqlx::Error> {
    let sql = r#"
            INSERT INTO audit_logs (
                id, action, user_id, event_id, event_type, event_level,
                result, details, ip_address, user_agent, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#;

    sqlx::query(sql)
        .bind(&entry.id)
        .bind(&entry.action)
        .bind(&entry.user_id)
        .bind(&entry.event_id)
        .bind(&entry.event_type)
        .bind(&entry.event_level)
        .bind(&entry.result)
        .bind(&entry.details)
        .bind(&entry.ip_address)
        .bind(&entry.user_agent)
        .bind(&entry.created_at)
        .execute(pool)
        .await?;

    Ok(())
}

type AuditLogTuple = (
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
);

fn tuple_to_audit_log_entry(
    (id, action, user_id, event_id, event_type, event_level, result, details, ip_address, user_agent, created_at): AuditLogTuple,
) -> AuditLogEntry {
    AuditLogEntry {
        id,
        action,
        user_id,
        event_id,
        event_type,
        event_level,
        result,
        details,
        ip_address,
        user_agent,
        created_at,
    }
}

/// 按用户查询审计日志（新的在前）。
pub(crate) async fn list_audit_logs_by_user(
    pool: &SqlitePool,
    user_id: &str,
    limit: i64,
) -> Result<Vec<AuditLogEntry>, sqlx::Error> {
    let sql = r#"
            SELECT id, action, user_id, event_id, event_type, event_level,
                   result, details, ip_address, user_agent, created_at
            FROM audit_logs
            WHERE user_id = ?
            ORDER BY created_at DESC
            LIMIT ?
        "#;

    let rows = sqlx::query_as::<_, AuditLogTuple>(sql)
        .bind(user_id)
        .bind(limit)
        .fetch_all(pool)
        .await?;

    Ok(rows.into_iter().map(tuple_to_audit_log_entry).collect())
}

/// 按事件查询审计日志（新的在前）。
pub(crate) async fn list_audit_logs_by_event(
    pool: &SqlitePool,
    event_id: &str,
    limit: i64,
) -> Result<Vec<AuditLogEntry>, sqlx::Error> {
    let sql = r#"
            SELECT id, action, user_id, event_id, event_type, event_level,
                   result, details, ip_address, user_agent, created_at
            FROM audit_logs
            WHERE event_id = ?
            ORDER BY created_at DESC
            LIMIT ?
        "#;

    let rows = sqlx::query_as::<_, AuditLogTuple>(sql)
        .bind(event_id)
        .bind(limit)
        .fetch_all(pool)
        .await?;

    Ok(rows.into_iter().map(tuple_to_audit_log_entry).collect())
}

/// 查询全部审计日志（分页，新的在前）。
pub(crate) async fn list_all_audit_logs(
    pool: &SqlitePool,
    limit: i64,
    offset: i64,
) -> Result<Vec<AuditLogEntry>, sqlx::Error> {
    let sql = r#"
            SELECT id, action, user_id, event_id, event_type, event_level,
                   result, details, ip_address, user_agent, created_at
            FROM audit_logs
            ORDER BY created_at DESC
            LIMIT ? OFFSET ?
        "#;

    let rows = sqlx::query_as::<_, AuditLogTuple>(sql)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?;

    Ok(rows.into_iter().map(tuple_to_audit_log_entry).collect())
}

/// 删除早于 cutoff 的审计日志，返回删除条数。
pub(crate) async fn delete_old_audit_logs(pool: &SqlitePool, cutoff: &str) -> Result<u64, sqlx::Error> {
    let sql = "DELETE FROM audit_logs WHERE created_at < ?";

    let result = sqlx::query(sql).bind(cutoff).execute(pool).await?;

    Ok(result.rows_affected())
}

// ──────────────────────────────────────────────
// Db 委托
// ──────────────────────────────────────────────

impl Db {
    /// 惰性创建 audit_logs 表与索引。
    pub async fn init_audit_log_storage(&self) -> Result<(), sqlx::Error> {
        init_audit_log_storage(self.pool()).await
    }

    /// 插入一条审计日志。
    pub async fn insert_audit_log(&self, entry: &AuditLogEntry) -> Result<(), sqlx::Error> {
        insert_audit_log(self.pool(), entry).await
    }

    /// 按用户查询审计日志（新的在前）。
    pub async fn list_audit_logs_by_user(&self, user_id: &str, limit: i64) -> Result<Vec<AuditLogEntry>, sqlx::Error> {
        list_audit_logs_by_user(self.pool(), user_id, limit).await
    }

    /// 按事件查询审计日志（新的在前）。
    pub async fn list_audit_logs_by_event(
        &self,
        event_id: &str,
        limit: i64,
    ) -> Result<Vec<AuditLogEntry>, sqlx::Error> {
        list_audit_logs_by_event(self.pool(), event_id, limit).await
    }

    /// 查询全部审计日志（分页，新的在前）。
    pub async fn list_all_audit_logs(&self, limit: i64, offset: i64) -> Result<Vec<AuditLogEntry>, sqlx::Error> {
        list_all_audit_logs(self.pool(), limit, offset).await
    }

    /// 删除早于 cutoff 的审计日志，返回删除条数。
    pub async fn delete_old_audit_logs(&self, cutoff: &str) -> Result<u64, sqlx::Error> {
        delete_old_audit_logs(self.pool(), cutoff).await
    }
}
