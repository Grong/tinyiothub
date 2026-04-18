use async_trait::async_trait;
use sqlx::{QueryBuilder, Row};

use crate::domain::job::repository::{JobExecutionRepository, JobRepository};
use crate::dto::entity::job::{
    CreateJobRequest, Job, JobExecution, JobExecutionQueryParams, JobQueryParams, JobStatistics,
    UpdateJobRequest,
};
use crate::infrastructure::persistence::database::Database;
use crate::shared::error::Result;

pub struct SqliteJobRepository {
    database: Database,
}

impl SqliteJobRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }
}

fn map_job_row(row: &sqlx::sqlite::SqliteRow) -> std::result::Result<Job, sqlx::Error> {
    Ok(Job {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        description: row.try_get("description")?,
        job_type: row.try_get("job_type")?,
        cron_expression: row.try_get("cron_expression")?,
        config: row.try_get("config")?,
        timeout_seconds: row.try_get("timeout_seconds")?,
        retry_count: row.try_get("retry_count")?,
        retry_delay_seconds: row.try_get("retry_delay_seconds")?,
        concurrency: row.try_get("concurrency")?,
        target_device_id: row.try_get("target_device_id")?,
        target_command_name: row.try_get("target_command_name")?,
        target_command_params: row.try_get("target_command_params")?,
        is_enabled: row.try_get::<i32, _>("is_enabled")? != 0,
        is_running: row.try_get::<i32, _>("is_running")? != 0,
        last_run_at: row.try_get("last_run_at")?,
        last_run_status: row.try_get("last_run_status")?,
        last_run_error: row.try_get("last_run_error")?,
        next_run_at: row.try_get("next_run_at")?,
        run_count: row.try_get("run_count")?,
        success_count: row.try_get("success_count")?,
        fail_count: row.try_get("fail_count")?,
        tags: row.try_get("tags")?,
        alert_config: row.try_get("alert_config")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        created_by: row.try_get("created_by")?,
    })
}

#[async_trait]
impl JobRepository for SqliteJobRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<Job>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, description, job_type, cron_expression, config,
                   timeout_seconds, retry_count, retry_delay_seconds, concurrency,
                   target_device_id, target_command_name, target_command_params,
                   is_enabled, is_running, last_run_at, last_run_status, last_run_error,
                   next_run_at, run_count, success_count, fail_count, tags, alert_config,
                   created_at, updated_at, created_by
            FROM jobs WHERE id = ? LIMIT 1
            "#,
        )
        .bind(id)
        .fetch_optional(self.database.pool())
        .await?;

        Ok(row.map(|r| map_job_row(&r)).transpose()?)
    }

    async fn find_all(&self, params: &JobQueryParams) -> Result<Vec<Job>> {
        let mut query = QueryBuilder::<sqlx::Sqlite>::new(
            r#"
            SELECT id, name, description, job_type, cron_expression, config,
                   timeout_seconds, retry_count, retry_delay_seconds, concurrency,
                   target_device_id, target_command_name, target_command_params,
                   is_enabled, is_running, last_run_at, last_run_status, last_run_error,
                   next_run_at, run_count, success_count, fail_count, tags, alert_config,
                   created_at, updated_at, created_by
            FROM jobs WHERE 1=1
            "#,
        );

        if let Some(ref workspace_id) = params.workspace_id {
            query.push(" AND workspace_id = ");
            query.push_bind(workspace_id);
        }

        if let Some(ref name) = params.name {
            query.push(" AND name LIKE ");
            query.push_bind(format!("%{}%", name));
        }

        if let Some(ref job_type) = params.job_type {
            query.push(" AND job_type = ");
            query.push_bind(job_type);
        }

        if let Some(is_enabled) = params.is_enabled {
            query.push(" AND is_enabled = ");
            query.push_bind(if is_enabled { 1 } else { 0 });
        }

        query.push(" ORDER BY created_at DESC");

        let page = params.page.unwrap_or(1);
        let page_size = params.page_size.unwrap_or(20);
        let offset = (page - 1) * page_size;
        query.push(" LIMIT ").push_bind(page_size as i64);
        query.push(" OFFSET ").push_bind(offset as i64);

        let rows = query.build().fetch_all(self.database.pool()).await?;
        let mut jobs = Vec::new();
        for row in rows {
            jobs.push(map_job_row(&row)?);
        }
        Ok(jobs)
    }

    async fn create(&self, req: &CreateJobRequest) -> Result<Job> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        sqlx::query(
            r#"
            INSERT INTO jobs (
                id, name, description, job_type, cron_expression, config,
                timeout_seconds, retry_count, retry_delay_seconds, concurrency,
                target_device_id, target_command_name, target_command_params,
                is_enabled, is_running, tags, alert_config,
                created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, 0, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&req.name)
        .bind(req.description.as_deref().unwrap_or(""))
        .bind(&req.job_type)
        .bind(&req.cron_expression)
        .bind(&req.config)
        .bind(req.timeout_seconds.unwrap_or(300))
        .bind(req.retry_count.unwrap_or(0))
        .bind(req.retry_delay_seconds.unwrap_or(60))
        .bind(req.concurrency.unwrap_or(1))
        .bind(req.target_device_id.as_deref().unwrap_or(""))
        .bind(req.target_command_name.as_deref().unwrap_or(""))
        .bind(req.target_command_params.as_deref().unwrap_or(""))
        .bind(req.tags.as_deref().unwrap_or("[]"))
        .bind(req.alert_config.as_deref().unwrap_or("{}"))
        .bind(&now)
        .bind(&now)
        .execute(self.database.pool())
        .await?;

        self.find_by_id(&id).await?.ok_or(crate::shared::error::Error::NotFound)
    }

    async fn update(&self, id: &str, req: &UpdateJobRequest) -> Result<Job> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let mut query = QueryBuilder::<sqlx::Sqlite>::new("UPDATE jobs SET updated_at = ");
        query.push_bind(&now);
        let mut has_updates = false;

        if let Some(ref name) = req.name {
            query.push(", name = ");
            query.push_bind(name);
            has_updates = true;
        }

        if let Some(ref description) = req.description {
            query.push(", description = ");
            query.push_bind(description);
            has_updates = true;
        }

        if let Some(ref job_type) = req.job_type {
            query.push(", job_type = ");
            query.push_bind(job_type);
            has_updates = true;
        }

        if let Some(ref cron_expression) = req.cron_expression {
            query.push(", cron_expression = ");
            query.push_bind(cron_expression);
            has_updates = true;
        }

        if let Some(ref config) = req.config {
            query.push(", config = ");
            query.push_bind(config);
            has_updates = true;
        }

        if let Some(timeout_seconds) = req.timeout_seconds {
            query.push(", timeout_seconds = ");
            query.push_bind(timeout_seconds);
            has_updates = true;
        }

        if let Some(retry_count) = req.retry_count {
            query.push(", retry_count = ");
            query.push_bind(retry_count);
            has_updates = true;
        }

        if let Some(retry_delay_seconds) = req.retry_delay_seconds {
            query.push(", retry_delay_seconds = ");
            query.push_bind(retry_delay_seconds);
            has_updates = true;
        }

        if let Some(concurrency) = req.concurrency {
            query.push(", concurrency = ");
            query.push_bind(concurrency);
            has_updates = true;
        }

        if let Some(ref target_device_id) = req.target_device_id {
            query.push(", target_device_id = ");
            query.push_bind(target_device_id);
            has_updates = true;
        }

        if let Some(ref target_command_name) = req.target_command_name {
            query.push(", target_command_name = ");
            query.push_bind(target_command_name);
            has_updates = true;
        }

        if let Some(ref target_command_params) = req.target_command_params {
            query.push(", target_command_params = ");
            query.push_bind(target_command_params);
            has_updates = true;
        }

        if let Some(ref tags) = req.tags {
            query.push(", tags = ");
            query.push_bind(tags);
            has_updates = true;
        }

        if let Some(ref alert_config) = req.alert_config {
            query.push(", alert_config = ");
            query.push_bind(alert_config);
            has_updates = true;
        }

        if let Some(is_enabled) = req.is_enabled {
            query.push(", is_enabled = ");
            query.push_bind(if is_enabled { 1 } else { 0 });
            has_updates = true;
        }

        if !has_updates {
            return self.find_by_id(id).await?.ok_or(crate::shared::error::Error::NotFound);
        }

        query.push(" WHERE id = ").push_bind(id);

        let result = query.build().execute(self.database.pool()).await?;

        if result.rows_affected() == 0 {
            return Err(crate::shared::error::Error::NotFound);
        }

        self.find_by_id(id).await?.ok_or(crate::shared::error::Error::NotFound)
    }

    async fn delete(&self, id: &str) -> Result<u64> {
        let result = sqlx::query("DELETE FROM jobs WHERE id = ?")
            .bind(id)
            .execute(self.database.pool())
            .await?;
        Ok(result.rows_affected())
    }

    async fn set_enabled(&self, id: &str, is_enabled: bool) -> Result<Job> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        sqlx::query("UPDATE jobs SET is_enabled = ?, updated_at = ? WHERE id = ?")
            .bind(if is_enabled { 1 } else { 0 })
            .bind(&now)
            .bind(id)
            .execute(self.database.pool())
            .await?;

        self.find_by_id(id).await?.ok_or(crate::shared::error::Error::NotFound)
    }

    async fn set_running(&self, id: &str, is_running: bool) -> Result<()> {
        sqlx::query("UPDATE jobs SET is_running = ? WHERE id = ?")
            .bind(if is_running { 1 } else { 0 })
            .bind(id)
            .execute(self.database.pool())
            .await?;
        Ok(())
    }

    async fn update_run_stats(
        &self,
        id: &str,
        status: &str,
        error: Option<&str>,
    ) -> Result<()> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let success_inc = if status == "success" { 1 } else { 0 };
        let fail_inc = if status == "failed" || status == "timeout" { 1 } else { 0 };

        sqlx::query(
            r#"
            UPDATE jobs SET
                last_run_at = ?,
                last_run_status = ?,
                last_run_error = ?,
                run_count = run_count + 1,
                success_count = success_count + ?,
                fail_count = fail_count + ?,
                is_running = 0,
                updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&now)
        .bind(status)
        .bind(error.unwrap_or(""))
        .bind(success_inc)
        .bind(fail_inc)
        .bind(&now)
        .bind(id)
        .execute(self.database.pool())
        .await?;

        Ok(())
    }

    async fn get_statistics(&self) -> Result<JobStatistics> {
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs")
            .fetch_one(self.database.pool())
            .await?;

        let enabled: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE is_enabled = 1")
            .fetch_one(self.database.pool())
            .await?;

        let disabled = total - enabled;

        let running: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE is_running = 1")
            .fetch_one(self.database.pool())
            .await?;

        let total_exec: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM job_executions")
            .fetch_one(self.database.pool())
            .await?;

        let success_exec: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM job_executions WHERE status = 'success'")
                .fetch_one(self.database.pool())
                .await?;

        let failed_exec: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM job_executions WHERE status = 'failed'")
                .fetch_one(self.database.pool())
                .await?;

        let avg_duration: i64 = sqlx::query_scalar(
            "SELECT COALESCE(AVG(duration_ms), 0) FROM job_executions WHERE duration_ms IS NOT NULL",
        )
        .fetch_one(self.database.pool())
        .await?;

        Ok(JobStatistics {
            total_jobs: total,
            enabled_jobs: enabled,
            disabled_jobs: disabled,
            running_jobs: running,
            total_executions: total_exec,
            success_executions: success_exec,
            failed_executions: failed_exec,
            avg_duration_ms: avg_duration,
        })
    }
}

pub struct SqliteJobExecutionRepository {
    database: Database,
}

impl SqliteJobExecutionRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }
}

fn map_job_execution_row(
    row: &sqlx::sqlite::SqliteRow,
) -> std::result::Result<JobExecution, sqlx::Error> {
    Ok(JobExecution {
        id: row.try_get("id")?,
        job_id: row.try_get("job_id")?,
        started_at: row.try_get("started_at")?,
        ended_at: row.try_get("ended_at")?,
        duration_ms: row.try_get("duration_ms")?,
        status: row.try_get("status")?,
        result: row.try_get("result")?,
        error_message: row.try_get("error_message")?,
        error_trace: row.try_get("error_trace")?,
        trigger_type: row.try_get("trigger_type")?,
        triggered_by: row.try_get("triggered_by")?,
        worker_id: row.try_get("worker_id")?,
        memory_usage_bytes: row.try_get("memory_usage_bytes")?,
        cpu_time_ms: row.try_get("cpu_time_ms")?,
        created_at: row.try_get("created_at")?,
    })
}

#[async_trait]
impl JobExecutionRepository for SqliteJobExecutionRepository {
    async fn find_all(&self, params: &JobExecutionQueryParams) -> Result<Vec<JobExecution>> {
        let mut query = QueryBuilder::<sqlx::Sqlite>::new(
            r#"
            SELECT id, job_id, started_at, ended_at, duration_ms, status, result,
                   error_message, error_trace, trigger_type, triggered_by, worker_id,
                   memory_usage_bytes, cpu_time_ms, created_at
            FROM job_executions WHERE 1=1
            "#,
        );

        if let Some(ref job_id) = params.job_id {
            query.push(" AND job_id = ");
            query.push_bind(job_id);
        }

        if let Some(ref status) = params.status {
            query.push(" AND status = ");
            query.push_bind(status);
        }

        if let Some(ref trigger_type) = params.trigger_type {
            query.push(" AND trigger_type = ");
            query.push_bind(trigger_type);
        }

        query.push(" ORDER BY started_at DESC");

        let page = params.page.unwrap_or(1);
        let page_size = params.page_size.unwrap_or(20);
        let offset = (page - 1) * page_size;
        query.push(" LIMIT ").push_bind(page_size as i64);
        query.push(" OFFSET ").push_bind(offset as i64);

        let rows = query.build().fetch_all(self.database.pool()).await?;
        let mut executions = Vec::new();
        for row in rows {
            executions.push(map_job_execution_row(&row)?);
        }
        Ok(executions)
    }

    async fn count(&self, params: &JobExecutionQueryParams) -> Result<i64> {
        let mut query = QueryBuilder::<sqlx::Sqlite>::new(
            "SELECT COUNT(*) FROM job_executions WHERE 1=1",
        );

        if let Some(ref job_id) = params.job_id {
            query.push(" AND job_id = ");
            query.push_bind(job_id);
        }

        if let Some(ref status) = params.status {
            query.push(" AND status = ");
            query.push_bind(status);
        }

        if let Some(ref trigger_type) = params.trigger_type {
            query.push(" AND trigger_type = ");
            query.push_bind(trigger_type);
        }

        let row = query.build().fetch_one(self.database.pool()).await?;
        let count: i64 = row.get(0);
        Ok(count)
    }

    async fn create(
        &self,
        job_id: &str,
        trigger_type: &str,
        triggered_by: Option<&str>,
    ) -> Result<JobExecution> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        sqlx::query(
            r#"
            INSERT INTO job_executions (id, job_id, started_at, status, trigger_type, triggered_by, created_at)
            VALUES (?, ?, ?, 'running', ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(job_id)
        .bind(&now)
        .bind(trigger_type)
        .bind(triggered_by.unwrap_or("system"))
        .bind(&now)
        .execute(self.database.pool())
        .await?;

        self.find_by_id(&id).await?.ok_or(crate::shared::error::Error::NotFound)
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<JobExecution>> {
        let row = sqlx::query(
            r#"
            SELECT id, job_id, started_at, ended_at, duration_ms, status, result,
                   error_message, error_trace, trigger_type, triggered_by, worker_id,
                   memory_usage_bytes, cpu_time_ms, created_at
            FROM job_executions WHERE id = ? LIMIT 1
            "#,
        )
        .bind(id)
        .fetch_optional(self.database.pool())
        .await?;

        Ok(row.map(|r| map_job_execution_row(&r)).transpose()?)
    }

    async fn find_by_job(&self, job_id: &str, limit: i32) -> Result<Vec<JobExecution>> {
        let rows = sqlx::query(
            r#"
            SELECT id, job_id, started_at, ended_at, duration_ms, status, result,
                   error_message, error_trace, trigger_type, triggered_by, worker_id,
                   memory_usage_bytes, cpu_time_ms, created_at
            FROM job_executions WHERE job_id = ? ORDER BY started_at DESC LIMIT ?
            "#,
        )
        .bind(job_id)
        .bind(limit as i64)
        .fetch_all(self.database.pool())
        .await?;

        let mut executions = Vec::new();
        for row in rows {
            executions.push(map_job_execution_row(&row)?);
        }
        Ok(executions)
    }

    async fn update_status(
        &self,
        id: &str,
        status: &str,
        ended_at: Option<&str>,
        result: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE job_executions SET
                ended_at = ?,
                status = ?,
                result = ?,
                error_message = ?
            WHERE id = ?
            "#,
        )
        .bind(ended_at.unwrap_or(""))
        .bind(status)
        .bind(result.unwrap_or(""))
        .bind(error_message.unwrap_or(""))
        .bind(id)
        .execute(self.database.pool())
        .await?;

        Ok(())
    }

    async fn delete_by_job_id(&self, job_id: &str) -> Result<u64> {
        let result = sqlx::query("DELETE FROM job_executions WHERE job_id = ?")
            .bind(job_id)
            .execute(self.database.pool())
            .await?;
        Ok(result.rows_affected())
    }
}
