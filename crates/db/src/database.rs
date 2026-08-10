use sqlx::{AssertSqlSafe, Error as SqlxError, SqlitePool, sqlite::SqliteRow};

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
        tracing::debug!("Executing query");

        let rows = sqlx::query(AssertSqlSafe(sql)).fetch_all(&self.pool).await?;

        rows.iter().map(mapper).collect()
    }

    /// Execute a query and return the first result
    pub async fn query_first<T, F>(&self, sql: &str, mapper: F) -> Result<Option<T>, SqlxError>
    where
        F: Fn(&SqliteRow) -> Result<T, SqlxError>,
    {
        tracing::debug!("Executing query_first");

        let row = sqlx::query(AssertSqlSafe(sql)).fetch_optional(&self.pool).await?;

        row.map(|r| mapper(&r)).transpose()
    }

    /// Execute a statement (INSERT, UPDATE, DELETE)
    pub async fn execute(&self, sql: &str) -> Result<u64, SqlxError> {
        tracing::debug!("Executing statement");

        let result = sqlx::query(AssertSqlSafe(sql)).execute(&self.pool).await?;

        Ok(result.rows_affected())
    }

    /// Execute a statement with parameters
    pub async fn execute_with_params(&self, sql: &str, params: &[&str]) -> Result<u64, SqlxError> {
        tracing::debug!("Executing statement with params");

        let mut query = sqlx::query(AssertSqlSafe(sql));

        for param in params {
            query = query.bind(*param);
        }

        let result = query.execute(&self.pool).await?;

        Ok(result.rows_affected())
    }

    /// Begin a transaction
    pub async fn begin_transaction(&self) -> Result<sqlx::Transaction<'_, sqlx::Sqlite>, SqlxError> {
        self.pool.begin().await
    }
}
