use async_trait::async_trait;
use sqlx::{QueryBuilder, Row};

use crate::sqlite::database::Database;
use crate::traits::cron::CronRunRepository;
use tinyiothub_core::error::Result;
use tinyiothub_core::models::cron_job::{CronRun, CronRunQuery};
use tinyiothub_core::{generate_id, now_string};

pub struct SqliteCronRunRepository {
    database: Database,
}

impl SqliteCronRunRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }
}

fn map_cron_run_row(row: &sqlx::sqlite::SqliteRow) -> std::result::Result<CronRun, sqlx::Error> {
    Ok(CronRun {
        id: row.try_get("id")?,
        job_id: row.try_get("job_id")?,
        workspace_id: row.try_get("workspace_id")?,
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
        workspace_id: &str,
        trigger_type: &str,
        triggered_by: Option<&str>,
    ) -> Result<CronRun> {
        let id = generate_id();
        let now = now_string();

        sqlx::query(
            r#"
            INSERT INTO cron_runs (
                id, job_id, workspace_id, started_at, status,
                trigger_type, triggered_by, created_at
            ) VALUES (?, ?, ?, ?, 'running', ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(job_id)
        .bind(workspace_id)
        .bind(&now)
        .bind(trigger_type)
        .bind(triggered_by)
        .bind(&now)
        .execute(self.database.pool())
        .await?;

        self.find_by_id(&id, workspace_id)
            .await?
            .ok_or(tinyiothub_core::error::Error::NotFound)
    }

    async fn complete(
        &self,
        id: &str,
        workspace_id: &str,
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
            WHERE id = ? AND workspace_id = ?
            "#,
        )
        .bind(status)
        .bind(output)
        .bind(error)
        .bind(duration_ms)
        .bind(&now)
        .bind(id)
        .bind(workspace_id)
        .execute(self.database.pool())
        .await?;

        if result.rows_affected() == 0 {
            return Err(tinyiothub_core::error::Error::NotFound);
        }

        // Fetch the run back to return the updated entity
        let row = sqlx::query(
            r#"
            SELECT id, job_id, workspace_id, started_at, ended_at, duration_ms, status,
                   output, error_message, trigger_type, triggered_by, created_at
            FROM cron_runs WHERE id = ? AND workspace_id = ? LIMIT 1
            "#,
        )
        .bind(id)
        .bind(workspace_id)
        .fetch_one(self.database.pool())
        .await?;

        Ok(map_cron_run_row(&row)?)
    }

    async fn find_by_job_id(&self, job_id: &str, workspace_id: &str, query: &CronRunQuery) -> Result<Vec<CronRun>> {
        let mut builder = QueryBuilder::<sqlx::Sqlite>::new(
            r#"
            SELECT id, job_id, workspace_id, started_at, ended_at, duration_ms, status,
                   output, error_message, trigger_type, triggered_by, created_at
            FROM cron_runs WHERE job_id =
            "#,
        );
        builder.push_bind(job_id);
        builder.push(" AND workspace_id = ");
        builder.push_bind(workspace_id);

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

    async fn find_by_id(&self, id: &str, workspace_id: &str) -> Result<Option<CronRun>> {
        let row = sqlx::query(
            r#"
            SELECT id, job_id, workspace_id, started_at, ended_at, duration_ms, status,
                   output, error_message, trigger_type, triggered_by, created_at
            FROM cron_runs WHERE id = ? AND workspace_id = ? LIMIT 1
            "#,
        )
        .bind(id)
        .bind(workspace_id)
        .fetch_optional(self.database.pool())
        .await?;

        Ok(row.map(|r| map_cron_run_row(&r)).transpose()?)
    }

    async fn delete_by_job_id(&self, job_id: &str, workspace_id: &str) -> Result<u64> {
        let result = sqlx::query("DELETE FROM cron_runs WHERE job_id = ? AND workspace_id = ?")
            .bind(job_id)
            .bind(workspace_id)
            .execute(self.database.pool())
            .await?;

        Ok(result.rows_affected())
    }

    async fn count_by_job_id(&self, job_id: &str, workspace_id: &str) -> Result<i64> {
        let row = sqlx::query("SELECT COUNT(*) as count FROM cron_runs WHERE job_id = ? AND workspace_id = ?")
            .bind(job_id)
            .bind(workspace_id)
            .fetch_one(self.database.pool())
            .await?;

        let count: i64 = row.get("count");
        Ok(count)
    }

    async fn count_by_status(&self, workspace_id: &str, status: &str) -> Result<i64> {
        let row = sqlx::query("SELECT COUNT(*) as count FROM cron_runs WHERE workspace_id = ? AND status = ?")
            .bind(workspace_id)
            .bind(status)
            .fetch_one(self.database.pool())
            .await?;

        let count: i64 = row.get("count");
        Ok(count)
    }

    async fn find_all(&self, workspace_id: &str, query: &CronRunQuery) -> Result<Vec<CronRun>> {
        let mut builder = QueryBuilder::<sqlx::Sqlite>::new(
            r#"
            SELECT id, job_id, workspace_id, started_at, ended_at, duration_ms, status,
                   output, error_message, trigger_type, triggered_by, created_at
            FROM cron_runs WHERE workspace_id =
            "#,
        );
        builder.push_bind(workspace_id);

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

    async fn avg_duration_ms(&self, workspace_id: &str) -> Result<i64> {
        let row = sqlx::query(
            "SELECT CAST(COALESCE(AVG(duration_ms), 0) AS REAL) as avg FROM cron_runs WHERE workspace_id = ? AND duration_ms IS NOT NULL",
        )
        .bind(workspace_id)
        .fetch_one(self.database.pool())
        .await?;

        let avg: f64 = row.get("avg");
        Ok(avg as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::database::Database;
    use crate::traits::cron::CronRunRepository;

    async fn setup_repo() -> SqliteCronRunRepository {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("Failed to create in-memory pool");

        sqlx::query(
            r#"
            CREATE TABLE cron_runs (
                id            TEXT PRIMARY KEY,
                job_id        TEXT NOT NULL,
                workspace_id  TEXT NOT NULL,
                started_at    TEXT NOT NULL,
                ended_at      TEXT,
                duration_ms   INTEGER,
                status        TEXT NOT NULL CHECK (status IN ('pending', 'running', 'success', 'failed')),
                output        TEXT,
                error_message TEXT,
                trigger_type  TEXT NOT NULL DEFAULT 'schedule',
                triggered_by  TEXT,
                created_at    TEXT NOT NULL DEFAULT (datetime('now'))
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create cron_runs table");

        let database = Database::new(pool);
        SqliteCronRunRepository::new(database)
    }

    #[tokio::test]
    async fn test_create_run() {
        let repo = setup_repo().await;
        let run = repo.create("job-1", "ws-1", "schedule", None).await.unwrap();
        assert_eq!(run.job_id, "job-1");
        assert_eq!(run.workspace_id, "ws-1");
        assert_eq!(run.status, "running");
        assert_eq!(run.trigger_type, "schedule");
        assert!(run.duration_ms.is_none());
    }

    #[tokio::test]
    async fn test_complete_run() {
        let repo = setup_repo().await;
        let run = repo.create("job-1", "ws-1", "schedule", None).await.unwrap();

        let completed = repo
            .complete(&run.id, "ws-1", "success", Some("output ok"), None, 1500)
            .await
            .unwrap();

        assert_eq!(completed.status, "success");
        assert_eq!(completed.output, Some("output ok".to_string()));
        assert_eq!(completed.duration_ms, Some(1500));
        assert!(completed.ended_at.is_some());
    }

    #[tokio::test]
    async fn test_complete_run_not_found() {
        let repo = setup_repo().await;
        let result = repo.complete("nonexistent", "ws-1", "success", None, None, 100).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_find_by_id() {
        let repo = setup_repo().await;
        let run = repo.create("job-1", "ws-1", "manual", Some("user-1")).await.unwrap();

        let found = repo.find_by_id(&run.id, "ws-1").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, run.id);
    }

    #[tokio::test]
    async fn test_find_by_id_not_found() {
        let repo = setup_repo().await;
        let found = repo.find_by_id("nonexistent", "ws-1").await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_find_by_job_id() {
        let repo = setup_repo().await;
        repo.create("job-1", "ws-1", "schedule", None).await.unwrap();
        repo.create("job-1", "ws-1", "manual", None).await.unwrap();
        repo.create("job-2", "ws-1", "schedule", None).await.unwrap();

        let query = CronRunQuery::default();
        let runs = repo.find_by_job_id("job-1", "ws-1", &query).await.unwrap();
        assert_eq!(runs.len(), 2);
    }

    #[tokio::test]
    async fn test_find_by_job_id_with_status_filter() {
        let repo = setup_repo().await;
        let run = repo.create("job-1", "ws-1", "schedule", None).await.unwrap();
        repo.complete(&run.id, "ws-1", "success", None, None, 100)
            .await
            .unwrap();
        repo.create("job-1", "ws-1", "schedule", None).await.unwrap();

        let query = CronRunQuery {
            status: Some("running".to_string()),
            ..Default::default()
        };
        let runs = repo.find_by_job_id("job-1", "ws-1", &query).await.unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, "running");
    }

    #[tokio::test]
    async fn test_delete_by_job_id() {
        let repo = setup_repo().await;
        repo.create("job-1", "ws-1", "schedule", None).await.unwrap();
        repo.create("job-1", "ws-1", "schedule", None).await.unwrap();

        let deleted = repo.delete_by_job_id("job-1", "ws-1").await.unwrap();
        assert_eq!(deleted, 2);

        let query = CronRunQuery::default();
        let runs = repo.find_by_job_id("job-1", "ws-1", &query).await.unwrap();
        assert_eq!(runs.len(), 0);
    }

    #[tokio::test]
    async fn test_count_by_job_id() {
        let repo = setup_repo().await;
        repo.create("job-1", "ws-1", "schedule", None).await.unwrap();
        repo.create("job-1", "ws-1", "schedule", None).await.unwrap();
        repo.create("job-2", "ws-1", "schedule", None).await.unwrap();

        assert_eq!(repo.count_by_job_id("job-1", "ws-1").await.unwrap(), 2);
        assert_eq!(repo.count_by_job_id("job-2", "ws-1").await.unwrap(), 1);
        assert_eq!(repo.count_by_job_id("job-3", "ws-1").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_count_by_status() {
        let repo = setup_repo().await;
        let run1 = repo.create("job-1", "ws-1", "schedule", None).await.unwrap();
        repo.complete(&run1.id, "ws-1", "success", None, None, 100)
            .await
            .unwrap();
        repo.create("job-2", "ws-1", "schedule", None).await.unwrap();

        assert_eq!(repo.count_by_status("ws-1", "success").await.unwrap(), 1);
        assert_eq!(repo.count_by_status("ws-1", "running").await.unwrap(), 1);
        assert_eq!(repo.count_by_status("ws-1", "failed").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_find_all() {
        let repo = setup_repo().await;
        repo.create("job-1", "ws-1", "schedule", None).await.unwrap();
        repo.create("job-2", "ws-1", "manual", None).await.unwrap();

        let query = CronRunQuery::default();
        let runs = repo.find_all("ws-1", &query).await.unwrap();
        assert_eq!(runs.len(), 2);
    }

    #[tokio::test]
    async fn test_avg_duration_ms_no_runs() {
        let repo = setup_repo().await;
        let avg = repo.avg_duration_ms("ws-1").await.unwrap();
        assert_eq!(avg, 0);
    }

    #[tokio::test]
    async fn test_avg_duration_ms_with_runs() {
        let repo = setup_repo().await;

        let run1 = repo.create("job-1", "ws-1", "schedule", None).await.unwrap();
        repo.complete(&run1.id, "ws-1", "success", None, None, 100)
            .await
            .unwrap();

        let run2 = repo.create("job-1", "ws-1", "schedule", None).await.unwrap();
        repo.complete(&run2.id, "ws-1", "success", None, None, 300)
            .await
            .unwrap();

        let avg = repo.avg_duration_ms("ws-1").await.unwrap();
        // AVG(100, 300) = 200
        assert_eq!(avg, 200);
    }

    #[tokio::test]
    async fn test_avg_duration_ms_ignores_null() {
        let repo = setup_repo().await;

        // Running run has NULL duration_ms
        repo.create("job-1", "ws-1", "schedule", None).await.unwrap();

        let run2 = repo.create("job-1", "ws-1", "schedule", None).await.unwrap();
        repo.complete(&run2.id, "ws-1", "success", None, None, 500)
            .await
            .unwrap();

        let avg = repo.avg_duration_ms("ws-1").await.unwrap();
        // Only the completed run (500) should be counted
        assert_eq!(avg, 500);
    }
}
