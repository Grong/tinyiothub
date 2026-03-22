use sqlx::{Error as SqlxError, Row, SqlitePool};

use sqlx::sqlite::SqliteRow;

use std::fmt::Display;

use serde::{de::DeserializeOwned, Serialize};

/// Database abstraction layer for SQLx
#[derive(Debug, Clone)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Execute a query and return multiple results

    pub async fn query<T, F>(&self, sql: &str, mapper: F) -> Result<Vec<T>, SqlxError>
    where
        F: Fn(&SqliteRow) -> Result<T, SqlxError>,
    {
        tracing::debug!("Executing query: {}", sql);

        let rows = sqlx::query(sql).fetch_all(&self.pool).await?;

        rows.iter().map(mapper).collect()
    }

    /// Execute a query and return the first result

    pub async fn query_first<T, F>(&self, sql: &str, mapper: F) -> Result<Option<T>, SqlxError>
    where
        F: Fn(&SqliteRow) -> Result<T, SqlxError>,
    {
        tracing::debug!("Executing query_first: {}", sql);

        let row = sqlx::query(sql).fetch_optional(&self.pool).await?;

        row.map(|r| mapper(&r)).transpose()
    }

    /// Execute a statement (INSERT, UPDATE, DELETE)

    pub async fn execute(&self, sql: &str) -> Result<u64, SqlxError> {
        tracing::debug!("Executing statement: {}", sql);

        let result = sqlx::query(sql).execute(&self.pool).await?;

        Ok(result.rows_affected())
    }

    /// Execute a statement with parameters

    pub async fn execute_with_params(&self, sql: &str, params: &[&str]) -> Result<u64, SqlxError> {
        tracing::debug!("Executing statement with params: {}", sql);

        let mut query = sqlx::query(sql);

        for param in params {
            query = query.bind(*param);
        }

        let result = query.execute(&self.pool).await?;

        Ok(result.rows_affected())
    }

    /// Begin a transaction

    pub async fn begin_transaction(
        &self,
    ) -> Result<sqlx::Transaction<'_, sqlx::Sqlite>, SqlxError> {
        self.pool.begin().await
    }
}

// Legacy compatibility functions - these maintain the old API for easier migration

pub async fn query<T, F>(pool: &SqlitePool, sql: &str, mut mapper: F) -> Result<Vec<T>, SqlxError>
where
    T: Serialize + DeserializeOwned,

    F: FnMut(&SqliteRow) -> Result<T, SqlxError>,
{
    tracing::debug!("Legacy query: {}", sql);

    let rows = sqlx::query(sql).fetch_all(pool).await?;

    let mut rst: Vec<T> = Vec::new();

    for row in rows {
        match mapper(&row) {
            Ok(item) => rst.push(item),

            Err(_) => continue,
        }
    }

    Ok(rst)
}

pub async fn query_first<T, F>(
    pool: &SqlitePool,
    sql: &str,
    mut mapper: F,
) -> Result<Option<T>, SqlxError>
where
    T: Serialize + DeserializeOwned,

    F: FnMut(&SqliteRow) -> Result<T, SqlxError>,
{
    tracing::debug!("Legacy query_first: {}", sql);

    let row = sqlx::query(sql).fetch_optional(pool).await?;

    match row {
        Some(r) => {
            let data = mapper(&r)?;

            Ok(Some(data))
        }

        None => Ok(None),
    }
}

pub async fn execute(pool: &SqlitePool, sql: &str) -> Result<u64, SqlxError> {
    tracing::debug!("Legacy execute: {}", sql);

    let result = sqlx::query(sql).execute(pool).await?;

    Ok(result.rows_affected())
}

// Utility functions

/// Escape single quotes in SQL string literals to prevent injection
fn escape_sql_string(s: &str) -> String {
    s.replace('\'', "''")
}

pub fn check_not_empty_equal(param: Option<String>, name: &str, list: &mut Vec<String>) {
    if let Some(s) = param {
        if !s.is_empty() {
            let str = format!(" {} = '{}'", name, escape_sql_string(&s));

            list.push(str);
        }
    }
}

pub fn check_not_empty_like(param: Option<String>, name: &str, list: &mut Vec<String>) {
    if let Some(s) = param {
        if !s.is_empty() {
            let escaped = escape_like(&s);
            let str = format!(" {} like '%{}%'", name, escaped);

            list.push(str);
        }
    }
}

/// Escape special characters in LIKE patterns to prevent SQL injection via LIKE wildcards
fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

pub fn check_not_empty_greater(param: Option<String>, name: &str, list: &mut Vec<String>) {
    if let Some(s) = param {
        if !s.is_empty() {
            let str = format!(" {} >= '{}'", name, escape_sql_string(&s));

            list.push(str);
        }
    }
}

pub fn check_not_empty_less(param: Option<String>, name: &str, list: &mut Vec<String>) {
    if let Some(s) = param {
        if !s.is_empty() {
            let str = format!(" {} <= '{}'", name, escape_sql_string(&s));

            list.push(str);
        }
    }
}

pub fn generate_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn get_value_or_default<T: Clone>(str: &Option<T>, def: T) -> T {
    match str {
        Some(s) => s.clone(),

        None => def,
    }
}

pub fn get_string_value_or_null<T: Clone + Display>(str: &Option<T>) -> String {
    match str {
        Some(s) => format!("'{}'", escape_sql_string(&s.to_string())),

        None => "null".to_string(),
    }
}

pub fn if_filed_null(str: &Option<String>) -> String {
    match str {
        Some(s) => s.clone(),

        None => "".to_string(),
    }
}

// Helper trait for easier row access

pub trait RowExt {
    fn try_get_string(&self, index: usize) -> Result<Option<String>, SqlxError>;

    fn try_get_i32(&self, index: usize) -> Result<i32, SqlxError>;

    fn try_get_i64(&self, index: usize) -> Result<i64, SqlxError>;

    fn try_get_f64(&self, index: usize) -> Result<f64, SqlxError>;
}

impl RowExt for SqliteRow {
    fn try_get_string(&self, index: usize) -> Result<Option<String>, SqlxError> {
        self.try_get(index)
    }

    fn try_get_i32(&self, index: usize) -> Result<i32, SqlxError> {
        self.try_get(index)
    }

    fn try_get_i64(&self, index: usize) -> Result<i64, SqlxError> {
        self.try_get(index)
    }

    fn try_get_f64(&self, index: usize) -> Result<f64, SqlxError> {
        self.try_get(index)
    }
}
