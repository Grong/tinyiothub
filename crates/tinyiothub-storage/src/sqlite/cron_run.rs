use async_trait::async_trait;
use sqlx::{QueryBuilder, Row};

use crate::traits::cron::CronRunRepository;
use tinyiothub_core::models::cron_job::{CronRun, CronRunQuery};
use crate::sqlite::database::Database;
use tinyiothub_core::error::Result;
use tinyiothub_core::{generate_id, now_string};

pub struct SqliteCronRunRepository {
    database: Database,
}

impl SqliteCronRunRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }
}

fn map_cron_run_row(
    row: &sqlx::sqlite::SqliteRow,
) -> std::result::Result<CronRun, sqlx::Error> {
    Ok(CronRun {
        id: row.try_get("id")?,
        job_id: row.try_get("job_id")?,
        started_at: row.try_get("started_at")?,
        ended_at: row.try_get("ended_at")?,
        duration_ms: row.try_get("duration_ms")?,
        status: row.try_get("status")?,
        output: row.try_get("output")?,
        error_message: row.try_get("error_message")?,
        trigger_type: row.try_get("trigger_type")?,
        triggered_by: row.try_get("triggered_by")?,
        created_at: row.try_get("created_at")?,
    })
}

#[async_trait]
impl CronRunRepository for SqliteCronRunRepository {
    async fn create(
        &self,
        job_id: &str,
        trigger_type: &str,
        triggered_by: Option<&str>,
    ) -> Result<CronRun> {
        let id = generate_id();
        let now = now_string();

        sqlx::query(
            r#"
            INSERT INTO cron_runs (
                id, job_id, started_at, status,
                trigger_type, triggered_by, created_at
            ) VALUES (?, ?, ?, 'running', ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(job_id)
        .bind(&now)
        .bind(trigger_type)
        .bind(triggered_by)
        .bind(&now)
        .execute(self.database.pool())
        .await?;

        self.find_by_id(&id)
            .await?
            .ok_or(tinyiothub_core::error::Error::NotFound)
    }

    async fn complete(
        &self,
        id: &str,
        status: &str,
        output: Option<&str>,
        error: Option<&str>,
        duration_ms: i64,
    ) -> Result<CronRun> {
        let now = now_string();

        let result = sqlx::query(
            r#"
            UPDATE cron_runs SET
                status = ?,
                output = ?,
                error_message = ?,
                duration_ms = ?,
                ended_at = ?
            WHERE id = ?
            "#,
        )
        .bind(status)
        .bind(output)
        .bind(error)
        .bind(duration_ms)
        .bind(&now)
        .bind(id)
        .execute(self.database.pool())
        .await?;

        if result.rows_affected() == 0 {
            return Err(tinyiothub_core::error::Error::NotFound);
        }

        // Fetch the run back to return the updated entity
        let row = sqlx::query(
            r#"
            SELECT id, job_id, started_at, ended_at, duration_ms, status,
                   output, error_message, trigger_type, triggered_by, created_at
            FROM cron_runs WHERE id = ? LIMIT 1
            "#,
        )
        .bind(id)
        .fetch_one(self.database.pool())
        .await?;

        Ok(map_cron_run_row(&row)?)
    }

    async fn find_by_job_id(
        &self,
        job_id: &str,
        query: &CronRunQuery,
    ) -> Result<Vec<CronRun>> {
        let mut builder = QueryBuilder::<sqlx::Sqlite>::new(
            r#"
            SELECT id, job_id, started_at, ended_at, duration_ms, status,
                   output, error_message, trigger_type, triggered_by, created_at
            FROM cron_runs WHERE job_id = ?
            "#,
        );
        builder.push_bind(job_id);

        if let Some(ref status) = query.status {
            builder.push(" AND status = ");
            builder.push_bind(status);
        }

        if let Some(ref trigger_type) = query.trigger_type {
            builder.push(" AND trigger_type = ");
            builder.push_bind(trigger_type);
        }

        builder.push(" ORDER BY started_at DESC");

        let page = query.page.unwrap_or(1);
        let page_size = query.page_size.unwrap_or(20).min(100);
        let offset = (page.saturating_sub(1)) * page_size;
        builder.push(" LIMIT ").push_bind(page_size as i64);
        builder.push(" OFFSET ").push_bind(offset as i64);

        let rows = builder.build().fetch_all(self.database.pool()).await?;
        let mut runs = Vec::new();
        for row in rows {
            runs.push(map_cron_run_row(&row)?);
        }
        Ok(runs)
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<CronRun>> {
        let row = sqlx::query(
            r#"
            SELECT id, job_id, started_at, ended_at, duration_ms, status,
                   output, error_message, trigger_type, triggered_by, created_at
            FROM cron_runs WHERE id = ? LIMIT 1
            "#,
        )
        .bind(id)
        .fetch_optional(self.database.pool())
        .await?;

        Ok(row.map(|r| map_cron_run_row(&r)).transpose()?)
    }

    async fn delete_by_job_id(&self, job_id: &str) -> Result<u64> {
        let result = sqlx::query(
            "DELETE FROM cron_runs WHERE job_id = ?",
        )
        .bind(job_id)
        .execute(self.database.pool())
        .await?;

        Ok(result.rows_affected())
    }

    async fn count_by_job_id(&self, job_id: &str) -> Result<i64> {
        let row = sqlx::query(
            "SELECT COUNT(*) as count FROM cron_runs WHERE job_id = ?",
        )
        .bind(job_id)
        .fetch_one(self.database.pool())
        .await?;

        let count: i64 = row.get("count");
        Ok(count)
    }

    async fn count_by_status(&self, status: &str) -> Result<i64> {
        let row = sqlx::query(
            "SELECT COUNT(*) as count FROM cron_runs WHERE status = ?",
        )
        .bind(status)
        .fetch_one(self.database.pool())
        .await?;

        let count: i64 = row.get("count");
        Ok(count)
    }

    async fn find_all(
        &self,
        query: &CronRunQuery,
    ) -> Result<Vec<CronRun>> {
        let mut builder = QueryBuilder::<sqlx::Sqlite>::new(
            r#"
            SELECT id, job_id, started_at, ended_at, duration_ms, status,
                   output, error_message, trigger_type, triggered_by, created_at
            FROM cron_runs WHERE 1=1
            "#,
        );

        if let Some(ref status) = query.status {
            builder.push(" AND status = ");
            builder.push_bind(status);
        }

        if let Some(ref trigger_type) = query.trigger_type {
            builder.push(" AND trigger_type = ");
            builder.push_bind(trigger_type);
        }

        builder.push(" ORDER BY started_at DESC");

        let page = query.page.unwrap_or(1);
        let page_size = query.page_size.unwrap_or(20).min(100);
        let offset = (page.saturating_sub(1)) * page_size;
        builder.push(" LIMIT ").push_bind(page_size as i64);
        builder.push(" OFFSET ").push_bind(offset as i64);

        let rows = builder.build().fetch_all(self.database.pool()).await?;
        let mut runs = Vec::new();
        for row in rows {
            runs.push(map_cron_run_row(&row)?);
        }
        Ok(runs)
    }

    async fn avg_duration_ms(&self) -> Result<i64> {
        let row = sqlx::query(
            "SELECT COALESCE(AVG(duration_ms), 0) as avg FROM cron_runs WHERE duration_ms IS NOT NULL",
        )
        .fetch_one(self.database.pool())
        .await?;

        let avg: f64 = row.get("avg");
        Ok(avg as i64)
    }
}
