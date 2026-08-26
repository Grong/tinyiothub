use chrono::Utc;
use cron::Schedule;
use sqlx::{QueryBuilder, Row, SqlitePool};
use std::str::FromStr;

use crate::database::Db;
use tinyiothub_core::error::Result;
use tinyiothub_core::models::cron_job::{CreateCronJobRequest, CronJob, CronJobQuery, UpdateCronJobRequest};
use tinyiothub_core::{generate_id, now_string};

fn map_cron_job_row(row: &sqlx::sqlite::SqliteRow) -> std::result::Result<CronJob, sqlx::Error> {
    Ok(CronJob {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        description: row.try_get("description")?,
        job_type: row.try_get("job_type")?,
        cron_expression: row.try_get("cron_expression")?,
        config: row.try_get("config")?,
        timeout_seconds: row.try_get("timeout_seconds")?,
        max_retries: row.try_get("max_retries")?,
        is_enabled: row.try_get::<i32, _>("is_enabled")? != 0,
        is_running: row.try_get::<i32, _>("is_running")? != 0,
        last_run_at: row.try_get("last_run_at")?,
        last_run_status: row.try_get("last_run_status")?,
        last_run_error: row.try_get("last_run_error")?,
        next_run_at: row.try_get("next_run_at")?,
        run_count: row.try_get("run_count")?,
        success_count: row.try_get("success_count")?,
        fail_count: row.try_get("fail_count")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        created_by: row.try_get("created_by")?,
        workspace_id: row.try_get("workspace_id").ok(),
    })
}

fn compute_next_run_at(cron_expression: &str) -> Option<String> {
    let normalized = {
        let fields: Vec<&str> = cron_expression.split_whitespace().collect();
        if fields.len() == 5 {
            format!("0 {}", cron_expression)
        } else {
            cron_expression.to_string()
        }
    };
    let schedule = Schedule::from_str(&normalized).ok()?;
    let next = schedule.upcoming(Utc).next()?;
    Some(next.format("%Y-%m-%d %H:%M:%S").to_string())
}

pub(crate) async fn find_job_by_id(pool: &SqlitePool, id: &str) -> Result<Option<CronJob>> {
    let row = sqlx::query(
        r#"
        SELECT id, workspace_id, name, description, job_type, cron_expression, config,
               timeout_seconds, max_retries, is_enabled, is_running,
               last_run_at, last_run_status, last_run_error, next_run_at,
               run_count, success_count, fail_count,
               created_at, updated_at, created_by
        FROM cron_jobs WHERE id = ? LIMIT 1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| map_cron_job_row(&r)).transpose()?)
}

pub(crate) async fn list_jobs(pool: &SqlitePool, query: &CronJobQuery) -> Result<Vec<CronJob>> {
    let mut builder = QueryBuilder::<sqlx::Sqlite>::new(
        r#"
        SELECT id, workspace_id, name, description, job_type, cron_expression, config,
               timeout_seconds, max_retries, is_enabled, is_running,
               last_run_at, last_run_status, last_run_error, next_run_at,
               run_count, success_count, fail_count,
               created_at, updated_at, created_by
        FROM cron_jobs WHERE 1=1
        "#,
    );

    if let Some(ref name) = query.name {
        builder.push(" AND name LIKE ");
        builder.push_bind(format!("%{}%", name));
    }

    if let Some(ref job_type) = query.job_type {
        builder.push(" AND job_type = ");
        builder.push_bind(job_type);
    }

    if let Some(is_enabled) = query.is_enabled {
        builder.push(" AND is_enabled = ");
        builder.push_bind(if is_enabled { 1 } else { 0 });
    }

    if let Some(ref ws_id) = query.workspace_id {
        builder.push(" AND workspace_id = ");
        builder.push_bind(ws_id);
    }

    builder.push(" ORDER BY created_at DESC");

    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(20).min(100);
    let offset = (page.saturating_sub(1)) * page_size;
    builder.push(" LIMIT ").push_bind(page_size as i64);
    builder.push(" OFFSET ").push_bind(offset as i64);

    let rows = builder.build().fetch_all(pool).await?;
    let mut jobs = Vec::new();
    for row in rows {
        jobs.push(map_cron_job_row(&row)?);
    }
    Ok(jobs)
}

pub(crate) async fn create_job(
    pool: &SqlitePool,
    job: &CreateCronJobRequest,
    created_by: Option<&str>,
) -> Result<CronJob> {
    let id = generate_id();
    let now = now_string();
    let timeout_seconds = job.timeout_seconds.unwrap_or(300);
    let max_retries = job.max_retries.unwrap_or(3);
    let next_run_at = compute_next_run_at(&job.cron_expression);

    sqlx::query(
        r#"
        INSERT INTO cron_jobs (
            id, workspace_id, name, description, job_type, cron_expression, config,
            timeout_seconds, max_retries, is_enabled, is_running,
            next_run_at, run_count, success_count, fail_count,
            created_at, updated_at, created_by
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 1, 0, ?, 0, 0, 0, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(&job.workspace_id)
    .bind(&job.name)
    .bind(job.description.as_deref().unwrap_or(""))
    .bind(&job.job_type)
    .bind(&job.cron_expression)
    .bind(&job.config)
    .bind(timeout_seconds)
    .bind(max_retries)
    .bind(next_run_at.as_deref())
    .bind(&now)
    .bind(&now)
    .bind(created_by)
    .execute(pool)
    .await?;

    find_job_by_id(pool, &id)
        .await?
        .ok_or(tinyiothub_core::error::Error::NotFound)
}

pub(crate) async fn update_job(pool: &SqlitePool, id: &str, req: &UpdateCronJobRequest) -> Result<CronJob> {
    let now = now_string();

    let mut builder = QueryBuilder::<sqlx::Sqlite>::new("UPDATE cron_jobs SET updated_at = ");
    builder.push_bind(&now);
    let mut has_updates = false;

    if let Some(ref name) = req.name {
        builder.push(", name = ");
        builder.push_bind(name);
        has_updates = true;
    }

    if let Some(ref description) = req.description {
        builder.push(", description = ");
        builder.push_bind(description);
        has_updates = true;
    }

    if let Some(ref job_type) = req.job_type {
        builder.push(", job_type = ");
        builder.push_bind(job_type);
        has_updates = true;
    }

    if let Some(ref cron_expression) = req.cron_expression {
        builder.push(", cron_expression = ");
        builder.push_bind(cron_expression);
        if let Some(next) = compute_next_run_at(cron_expression) {
            builder.push(", next_run_at = ");
            builder.push_bind(next);
        }
        has_updates = true;
    }

    if let Some(ref config) = req.config {
        builder.push(", config = ");
        builder.push_bind(config);
        has_updates = true;
    }

    if let Some(timeout_seconds) = req.timeout_seconds {
        builder.push(", timeout_seconds = ");
        builder.push_bind(timeout_seconds);
        has_updates = true;
    }

    if let Some(max_retries) = req.max_retries {
        builder.push(", max_retries = ");
        builder.push_bind(max_retries);
        has_updates = true;
    }

    if let Some(is_enabled) = req.is_enabled {
        builder.push(", is_enabled = ");
        builder.push_bind(if is_enabled { 1 } else { 0 });
        has_updates = true;
    }

    if !has_updates {
        return find_job_by_id(pool, id)
            .await?
            .ok_or(tinyiothub_core::error::Error::NotFound);
    }

    builder.push(" WHERE id = ").push_bind(id);

    let result = builder.build().execute(pool).await?;

    if result.rows_affected() == 0 {
        return Err(tinyiothub_core::error::Error::NotFound);
    }

    find_job_by_id(pool, id)
        .await?
        .ok_or(tinyiothub_core::error::Error::NotFound)
}

pub(crate) async fn delete_job(pool: &SqlitePool, id: &str) -> Result<bool> {
    let result = sqlx::query("DELETE FROM cron_jobs WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

pub(crate) async fn update_job_run_stats(
    pool: &SqlitePool,
    id: &str,
    status: &str,
    error: Option<&str>,
) -> Result<bool> {
    let now = now_string();
    let success_inc = if status == "success" { 1 } else { 0 };
    let fail_inc = if status == "failed" { 1 } else { 0 };

    let result = sqlx::query(
        r#"
        UPDATE cron_jobs SET
            last_run_at = ?,
            last_run_status = ?,
            last_run_error = ?,
            run_count = run_count + 1,
            success_count = success_count + ?,
            fail_count = fail_count + ?,
            updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(&now)
    .bind(status)
    .bind(error)
    .bind(success_inc)
    .bind(fail_inc)
    .bind(&now)
    .bind(id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub(crate) async fn set_job_running(pool: &SqlitePool, id: &str, running: bool) -> Result<bool> {
    let now = now_string();

    let result = sqlx::query("UPDATE cron_jobs SET is_running = ?, updated_at = ? WHERE id = ?")
        .bind(if running { 1 } else { 0 })
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

pub(crate) async fn find_due_jobs(pool: &SqlitePool) -> Result<Vec<CronJob>> {
    let mut builder = QueryBuilder::<sqlx::Sqlite>::new(
        r#"
        SELECT id, workspace_id, name, description, job_type, cron_expression, config,
               timeout_seconds, max_retries, is_enabled, is_running,
               last_run_at, last_run_status, last_run_error, next_run_at,
               run_count, success_count, fail_count,
               created_at, updated_at, created_by
        FROM cron_jobs
        WHERE is_enabled = 1 AND is_running = 0
          AND (next_run_at IS NULL OR next_run_at <= datetime('now'))
        "#,
    );

    builder.push(" ORDER BY next_run_at ASC");

    let rows = builder.build().fetch_all(pool).await?;
    let mut jobs = Vec::new();
    for row in rows {
        jobs.push(map_cron_job_row(&row)?);
    }
    Ok(jobs)
}

pub(crate) async fn claim_job(pool: &SqlitePool, id: &str) -> Result<bool> {
    let now = now_string();
    let result = sqlx::query("UPDATE cron_jobs SET is_running = 1, updated_at = ? WHERE id = ? AND is_running = 0")
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

pub(crate) async fn clear_all_running_jobs(pool: &SqlitePool) -> Result<u64> {
    let now = now_string();
    let mut builder = QueryBuilder::<sqlx::Sqlite>::new("UPDATE cron_jobs SET is_running = 0, updated_at = ");
    builder.push_bind(&now);
    builder.push(" WHERE is_running = 1");

    let result = builder.build().execute(pool).await?;
    Ok(result.rows_affected())
}

pub(crate) async fn count_jobs(pool: &SqlitePool, workspace_id: &str) -> Result<i64> {
    let row = sqlx::query("SELECT COUNT(*) as count FROM cron_jobs WHERE workspace_id = ?")
        .bind(workspace_id)
        .fetch_one(pool)
        .await?;

    let count: i64 = row.get("count");
    Ok(count)
}

pub(crate) async fn count_jobs_by_enabled(pool: &SqlitePool, workspace_id: &str, is_enabled: bool) -> Result<i64> {
    let row = sqlx::query("SELECT COUNT(*) as count FROM cron_jobs WHERE workspace_id = ? AND is_enabled = ?")
        .bind(workspace_id)
        .bind(if is_enabled { 1 } else { 0 })
        .fetch_one(pool)
        .await?;

    let count: i64 = row.get("count");
    Ok(count)
}

pub(crate) async fn count_running_jobs(pool: &SqlitePool, workspace_id: &str) -> Result<i64> {
    let row = sqlx::query("SELECT COUNT(*) as count FROM cron_jobs WHERE workspace_id = ? AND is_running = 1")
        .bind(workspace_id)
        .fetch_one(pool)
        .await?;

    let count: i64 = row.get("count");
    Ok(count)
}

pub(crate) async fn update_job_next_run_at(pool: &SqlitePool, id: &str, next_run_at: Option<&str>) -> Result<bool> {
    let now = now_string();

    let result = sqlx::query("UPDATE cron_jobs SET next_run_at = ?, updated_at = ? WHERE id = ?")
        .bind(next_run_at)
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

impl Db {
    /// 按 ID 查询 cron 任务。
    pub async fn find_cron_job_by_id(&self, id: &str) -> Result<Option<CronJob>> {
        find_job_by_id(self.pool(), id).await
    }

    /// 分页查询 cron 任务列表。
    pub async fn list_cron_jobs(&self, query: &CronJobQuery) -> Result<Vec<CronJob>> {
        list_jobs(self.pool(), query).await
    }

    /// 创建 cron 任务（计算下次运行时间）。
    pub async fn create_cron_job(&self, job: &CreateCronJobRequest, created_by: Option<&str>) -> Result<CronJob> {
        create_job(self.pool(), job, created_by).await
    }

    /// 更新 cron 任务。
    pub async fn update_cron_job(&self, id: &str, req: &UpdateCronJobRequest) -> Result<CronJob> {
        update_job(self.pool(), id, req).await
    }

    /// 删除 cron 任务。
    pub async fn delete_cron_job(&self, id: &str) -> Result<bool> {
        delete_job(self.pool(), id).await
    }

    /// 更新 cron 任务的运行统计。
    pub async fn update_cron_job_run_stats(&self, id: &str, status: &str, error: Option<&str>) -> Result<bool> {
        update_job_run_stats(self.pool(), id, status, error).await
    }

    /// 设置 cron 任务的运行中标志。
    pub async fn set_cron_job_running(&self, id: &str, running: bool) -> Result<bool> {
        set_job_running(self.pool(), id, running).await
    }

    /// 查询到期的 cron 任务。
    pub async fn find_due_cron_jobs(&self) -> Result<Vec<CronJob>> {
        find_due_jobs(self.pool()).await
    }

    /// 原子认领 cron 任务（防止并发触发）。
    pub async fn claim_cron_job(&self, id: &str) -> Result<bool> {
        claim_job(self.pool(), id).await
    }

    /// 清除所有运行中标志（启动时崩溃恢复）。
    pub async fn clear_all_running_cron_jobs(&self) -> Result<u64> {
        clear_all_running_jobs(self.pool()).await
    }

    /// 统计工作空间的 cron 任务数。
    pub async fn count_cron_jobs(&self, workspace_id: &str) -> Result<i64> {
        count_jobs(self.pool(), workspace_id).await
    }

    /// 按启用状态统计 cron 任务数。
    pub async fn count_cron_jobs_by_enabled(&self, workspace_id: &str, is_enabled: bool) -> Result<i64> {
        count_jobs_by_enabled(self.pool(), workspace_id, is_enabled).await
    }

    /// 统计运行中的 cron 任务数。
    pub async fn count_running_cron_jobs(&self, workspace_id: &str) -> Result<i64> {
        count_running_jobs(self.pool(), workspace_id).await
    }

    /// 更新 cron 任务的下次运行时间。
    pub async fn update_cron_job_next_run_at(&self, id: &str, next_run_at: Option<&str>) -> Result<bool> {
        update_job_next_run_at(self.pool(), id, next_run_at).await
    }
}
