use sqlx::{QueryBuilder, Row, SqlitePool};

use crate::database::Db;
use tinyiothub_core::error::Result;
use tinyiothub_core::models::cron_job::{CronRun, CronRunQuery};
use tinyiothub_core::{generate_id, now_string};

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

pub(crate) async fn create_run(
    pool: &SqlitePool,
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
    .execute(pool)
    .await?;

    find_run_by_id(pool, &id, workspace_id)
        .await?
        .ok_or(tinyiothub_core::error::Error::NotFound)
}

pub(crate) async fn complete_run(
    pool: &SqlitePool,
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
    .execute(pool)
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
    .fetch_one(pool)
    .await?;

    Ok(map_cron_run_row(&row)?)
}

pub(crate) async fn find_runs_by_job_id(
    pool: &SqlitePool,
    job_id: &str,
    workspace_id: &str,
    query: &CronRunQuery,
) -> Result<Vec<CronRun>> {
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

    let rows = builder.build().fetch_all(pool).await?;
    let mut runs = Vec::new();
    for row in rows {
        runs.push(map_cron_run_row(&row)?);
    }
    Ok(runs)
}

pub(crate) async fn find_run_by_id(pool: &SqlitePool, id: &str, workspace_id: &str) -> Result<Option<CronRun>> {
    let row = sqlx::query(
        r#"
        SELECT id, job_id, workspace_id, started_at, ended_at, duration_ms, status,
               output, error_message, trigger_type, triggered_by, created_at
        FROM cron_runs WHERE id = ? AND workspace_id = ? LIMIT 1
        "#,
    )
    .bind(id)
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| map_cron_run_row(&r)).transpose()?)
}

pub(crate) async fn delete_runs_by_job_id(pool: &SqlitePool, job_id: &str, workspace_id: &str) -> Result<u64> {
    let result = sqlx::query("DELETE FROM cron_runs WHERE job_id = ? AND workspace_id = ?")
        .bind(job_id)
        .bind(workspace_id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected())
}

pub(crate) async fn count_runs_by_job_id(pool: &SqlitePool, job_id: &str, workspace_id: &str) -> Result<i64> {
    let row = sqlx::query("SELECT COUNT(*) as count FROM cron_runs WHERE job_id = ? AND workspace_id = ?")
        .bind(job_id)
        .bind(workspace_id)
        .fetch_one(pool)
        .await?;

    let count: i64 = row.get("count");
    Ok(count)
}

pub(crate) async fn count_runs_by_status(pool: &SqlitePool, workspace_id: &str, status: &str) -> Result<i64> {
    let row = sqlx::query("SELECT COUNT(*) as count FROM cron_runs WHERE workspace_id = ? AND status = ?")
        .bind(workspace_id)
        .bind(status)
        .fetch_one(pool)
        .await?;

    let count: i64 = row.get("count");
    Ok(count)
}

pub(crate) async fn list_runs(pool: &SqlitePool, workspace_id: &str, query: &CronRunQuery) -> Result<Vec<CronRun>> {
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

    let rows = builder.build().fetch_all(pool).await?;
    let mut runs = Vec::new();
    for row in rows {
        runs.push(map_cron_run_row(&row)?);
    }
    Ok(runs)
}

pub(crate) async fn avg_run_duration_ms(pool: &SqlitePool, workspace_id: &str) -> Result<i64> {
    let row = sqlx::query(
        "SELECT CAST(COALESCE(AVG(duration_ms), 0) AS REAL) as avg FROM cron_runs WHERE workspace_id = ? AND duration_ms IS NOT NULL",
    )
    .bind(workspace_id)
    .fetch_one(pool)
    .await?;

    let avg: f64 = row.get("avg");
    Ok(avg as i64)
}

impl Db {
    /// 创建 cron 执行记录（状态 running）。
    pub async fn create_cron_run(
        &self,
        job_id: &str,
        workspace_id: &str,
        trigger_type: &str,
        triggered_by: Option<&str>,
    ) -> Result<CronRun> {
        create_run(self.pool(), job_id, workspace_id, trigger_type, triggered_by).await
    }

    /// 完成 cron 执行记录（写入结果与耗时）。
    pub async fn complete_cron_run(
        &self,
        id: &str,
        workspace_id: &str,
        status: &str,
        output: Option<&str>,
        error: Option<&str>,
        duration_ms: i64,
    ) -> Result<CronRun> {
        complete_run(self.pool(), id, workspace_id, status, output, error, duration_ms).await
    }

    /// 分页查询某 cron 任务的执行记录。
    pub async fn find_cron_runs_by_job_id(
        &self,
        job_id: &str,
        workspace_id: &str,
        query: &CronRunQuery,
    ) -> Result<Vec<CronRun>> {
        find_runs_by_job_id(self.pool(), job_id, workspace_id, query).await
    }

    /// 按 ID 查询 cron 执行记录。
    pub async fn find_cron_run_by_id(&self, id: &str, workspace_id: &str) -> Result<Option<CronRun>> {
        find_run_by_id(self.pool(), id, workspace_id).await
    }

    /// 删除某 cron 任务的全部执行记录。
    pub async fn delete_cron_runs_by_job_id(&self, job_id: &str, workspace_id: &str) -> Result<u64> {
        delete_runs_by_job_id(self.pool(), job_id, workspace_id).await
    }

    /// 统计某 cron 任务的执行记录数。
    pub async fn count_cron_runs_by_job_id(&self, job_id: &str, workspace_id: &str) -> Result<i64> {
        count_runs_by_job_id(self.pool(), job_id, workspace_id).await
    }

    /// 按状态统计 cron 执行记录数。
    pub async fn count_cron_runs_by_status(&self, workspace_id: &str, status: &str) -> Result<i64> {
        count_runs_by_status(self.pool(), workspace_id, status).await
    }

    /// 分页查询工作空间的 cron 执行记录。
    pub async fn list_cron_runs(&self, workspace_id: &str, query: &CronRunQuery) -> Result<Vec<CronRun>> {
        list_runs(self.pool(), workspace_id, query).await
    }

    /// 工作空间 cron 执行的平均耗时（毫秒）。
    pub async fn avg_cron_run_duration_ms(&self, workspace_id: &str) -> Result<i64> {
        avg_run_duration_ms(self.pool(), workspace_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_db() -> Db {
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

        Db::new(pool)
    }

    #[tokio::test]
    async fn test_create_run() {
        let db = setup_db().await;
        let run = db.create_cron_run("job-1", "ws-1", "schedule", None).await.unwrap();
        assert_eq!(run.job_id, "job-1");
        assert_eq!(run.workspace_id, "ws-1");
        assert_eq!(run.status, "running");
        assert_eq!(run.trigger_type, "schedule");
        assert!(run.duration_ms.is_none());
    }

    #[tokio::test]
    async fn test_complete_run() {
        let db = setup_db().await;
        let run = db.create_cron_run("job-1", "ws-1", "schedule", None).await.unwrap();

        let completed = db
            .complete_cron_run(&run.id, "ws-1", "success", Some("output ok"), None, 1500)
            .await
            .unwrap();

        assert_eq!(completed.status, "success");
        assert_eq!(completed.output, Some("output ok".to_string()));
        assert_eq!(completed.duration_ms, Some(1500));
        assert!(completed.ended_at.is_some());
    }

    #[tokio::test]
    async fn test_complete_run_not_found() {
        let db = setup_db().await;
        let result = db.complete_cron_run("nonexistent", "ws-1", "success", None, None, 100).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_find_by_id() {
        let db = setup_db().await;
        let run = db.create_cron_run("job-1", "ws-1", "manual", Some("user-1")).await.unwrap();

        let found = db.find_cron_run_by_id(&run.id, "ws-1").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, run.id);
    }

    #[tokio::test]
    async fn test_find_by_id_not_found() {
        let db = setup_db().await;
        let found = db.find_cron_run_by_id("nonexistent", "ws-1").await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_find_by_job_id() {
        let db = setup_db().await;
        db.create_cron_run("job-1", "ws-1", "schedule", None).await.unwrap();
        db.create_cron_run("job-1", "ws-1", "manual", None).await.unwrap();
        db.create_cron_run("job-2", "ws-1", "schedule", None).await.unwrap();

        let query = CronRunQuery::default();
        let runs = db.find_cron_runs_by_job_id("job-1", "ws-1", &query).await.unwrap();
        assert_eq!(runs.len(), 2);
    }

    #[tokio::test]
    async fn test_find_by_job_id_with_status_filter() {
        let db = setup_db().await;
        let run = db.create_cron_run("job-1", "ws-1", "schedule", None).await.unwrap();
        db.complete_cron_run(&run.id, "ws-1", "success", None, None, 100)
            .await
            .unwrap();
        db.create_cron_run("job-1", "ws-1", "schedule", None).await.unwrap();

        let query = CronRunQuery {
            status: Some("running".to_string()),
            ..Default::default()
        };
        let runs = db.find_cron_runs_by_job_id("job-1", "ws-1", &query).await.unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, "running");
    }

    #[tokio::test]
    async fn test_delete_by_job_id() {
        let db = setup_db().await;
        db.create_cron_run("job-1", "ws-1", "schedule", None).await.unwrap();
        db.create_cron_run("job-1", "ws-1", "schedule", None).await.unwrap();

        let deleted = db.delete_cron_runs_by_job_id("job-1", "ws-1").await.unwrap();
        assert_eq!(deleted, 2);

        let query = CronRunQuery::default();
        let runs = db.find_cron_runs_by_job_id("job-1", "ws-1", &query).await.unwrap();
        assert_eq!(runs.len(), 0);
    }

    #[tokio::test]
    async fn test_count_by_job_id() {
        let db = setup_db().await;
        db.create_cron_run("job-1", "ws-1", "schedule", None).await.unwrap();
        db.create_cron_run("job-1", "ws-1", "schedule", None).await.unwrap();
        db.create_cron_run("job-2", "ws-1", "schedule", None).await.unwrap();

        assert_eq!(db.count_cron_runs_by_job_id("job-1", "ws-1").await.unwrap(), 2);
        assert_eq!(db.count_cron_runs_by_job_id("job-2", "ws-1").await.unwrap(), 1);
        assert_eq!(db.count_cron_runs_by_job_id("job-3", "ws-1").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_count_by_status() {
        let db = setup_db().await;
        let run1 = db.create_cron_run("job-1", "ws-1", "schedule", None).await.unwrap();
        db.complete_cron_run(&run1.id, "ws-1", "success", None, None, 100)
            .await
            .unwrap();
        db.create_cron_run("job-2", "ws-1", "schedule", None).await.unwrap();

        assert_eq!(db.count_cron_runs_by_status("ws-1", "success").await.unwrap(), 1);
        assert_eq!(db.count_cron_runs_by_status("ws-1", "running").await.unwrap(), 1);
        assert_eq!(db.count_cron_runs_by_status("ws-1", "failed").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_find_all() {
        let db = setup_db().await;
        db.create_cron_run("job-1", "ws-1", "schedule", None).await.unwrap();
        db.create_cron_run("job-2", "ws-1", "manual", None).await.unwrap();

        let query = CronRunQuery::default();
        let runs = db.list_cron_runs("ws-1", &query).await.unwrap();
        assert_eq!(runs.len(), 2);
    }

    #[tokio::test]
    async fn test_avg_duration_ms_no_runs() {
        let db = setup_db().await;
        let avg = db.avg_cron_run_duration_ms("ws-1").await.unwrap();
        assert_eq!(avg, 0);
    }

    #[tokio::test]
    async fn test_avg_duration_ms_with_runs() {
        let db = setup_db().await;

        let run1 = db.create_cron_run("job-1", "ws-1", "schedule", None).await.unwrap();
        db.complete_cron_run(&run1.id, "ws-1", "success", None, None, 100)
            .await
            .unwrap();

        let run2 = db.create_cron_run("job-1", "ws-1", "schedule", None).await.unwrap();
        db.complete_cron_run(&run2.id, "ws-1", "success", None, None, 300)
            .await
            .unwrap();

        let avg = db.avg_cron_run_duration_ms("ws-1").await.unwrap();
        // AVG(100, 300) = 200
        assert_eq!(avg, 200);
    }

    #[tokio::test]
    async fn test_avg_duration_ms_ignores_null() {
        let db = setup_db().await;

        // Running run has NULL duration_ms
        db.create_cron_run("job-1", "ws-1", "schedule", None).await.unwrap();

        let run2 = db.create_cron_run("job-1", "ws-1", "schedule", None).await.unwrap();
        db.complete_cron_run(&run2.id, "ws-1", "success", None, None, 500)
            .await
            .unwrap();

        let avg = db.avg_cron_run_duration_ms("ws-1").await.unwrap();
        // Only the completed run (500) should be counted
        assert_eq!(avg, 500);
    }
}
