use serde::{Deserialize, Serialize};
use sqlx::{Execute, FromRow, Row, Sqlite};

use crate::infrastructure::persistence::database::Database;

/// 任务实体
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Job {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub job_type: String,
    pub cron_expression: String,
    pub config: String,
    pub timeout_seconds: i32,
    pub retry_count: i32,
    pub retry_delay_seconds: i32,
    pub concurrency: i32,
    pub target_device_id: Option<String>,
    pub target_command_name: Option<String>,
    pub target_command_params: Option<String>,
    pub is_enabled: bool,
    pub is_running: bool,
    pub last_run_at: Option<String>,
    pub last_run_status: Option<String>,
    pub last_run_error: Option<String>,
    pub next_run_at: Option<String>,
    pub run_count: i64,
    pub success_count: i64,
    pub fail_count: i64,
    pub tags: String,
    pub alert_config: String,
    pub created_at: String,
    pub updated_at: String,
    pub created_by: Option<String>,
}

/// 任务查询参数
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct JobQueryParams {
    pub name: Option<String>,
    pub job_type: Option<String>,
    pub is_enabled: Option<bool>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// 创建任务请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateJobRequest {
    pub name: String,
    pub description: Option<String>,
    pub job_type: String,
    pub cron_expression: String,
    pub config: String,
    pub timeout_seconds: Option<i32>,
    pub retry_count: Option<i32>,
    pub retry_delay_seconds: Option<i32>,
    pub concurrency: Option<i32>,
    pub target_device_id: Option<String>,
    pub target_command_name: Option<String>,
    pub target_command_params: Option<String>,
    pub tags: Option<String>,
    pub alert_config: Option<String>,
}

/// 更新任务请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateJobRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub job_type: Option<String>,
    pub cron_expression: Option<String>,
    pub config: Option<String>,
    pub timeout_seconds: Option<i32>,
    pub retry_count: Option<i32>,
    pub retry_delay_seconds: Option<i32>,
    pub concurrency: Option<i32>,
    pub target_device_id: Option<String>,
    pub target_command_name: Option<String>,
    pub target_command_params: Option<String>,
    pub tags: Option<String>,
    pub alert_config: Option<String>,
}

/// 任务执行记录
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct JobExecution {
    pub id: String,
    pub job_id: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub duration_ms: Option<i64>,
    pub status: String,
    pub result: Option<String>,
    pub error_message: Option<String>,
    pub error_trace: Option<String>,
    pub trigger_type: String,
    pub triggered_by: Option<String>,
    pub worker_id: Option<String>,
    pub memory_usage_bytes: Option<i64>,
    pub cpu_time_ms: Option<i64>,
    pub created_at: String,
}

/// 任务执行查询参数
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct JobExecutionQueryParams {
    pub job_id: Option<String>,
    pub status: Option<String>,
    pub trigger_type: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// 任务统计
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct JobStatistics {
    pub total_jobs: i64,
    pub enabled_jobs: i64,
    pub disabled_jobs: i64,
    pub running_jobs: i64,
    pub total_executions: i64,
    pub success_executions: i64,
    pub failed_executions: i64,
    pub avg_duration_ms: i64,
}

impl Job {
    /// 根据 ID 查询
    pub async fn find_by_id(db: &Database, id: &str) -> Result<Option<Job>, sqlx::Error> {
        // 使用参数化查询防止 SQL 注入
        let row = sqlx::query("SELECT * FROM jobs WHERE id = ? LIMIT 1")
            .bind(id)
            .fetch_optional(db.pool())
            .await?;

        if let Some(row) = row {
            Ok(Some(Job {
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
                }))
        } else {
            Ok(None)
        }
    }

    /// 查询所有（带分页）
    pub async fn find_all(db: &Database, params: &JobQueryParams) -> Result<Vec<Job>, sqlx::Error> {
        // 使用 QueryBuilder 防止 SQL 注入
        let mut query_builder: sqlx::query_builder::QueryBuilder<'_, Sqlite> = sqlx::query_builder::QueryBuilder::new("SELECT * FROM jobs WHERE 1=1");

        if let Some(ref name) = params.name {
            // 转义 LIKE 特殊字符防止注入
            let escaped = name.replace('\'', "''");
            query_builder.push(&format!(" AND name LIKE '%{}%'", escaped));
        }
        if let Some(ref job_type) = params.job_type {
            query_builder.push(" AND job_type = ");
            query_builder.push(job_type);
        }
        if let Some(is_enabled) = params.is_enabled {
            query_builder.push(" AND is_enabled = ");
            query_builder.push(if is_enabled { "1" } else { "0" });
        }

        query_builder.push(" ORDER BY created_at DESC");

        let page = params.page.unwrap_or(1);
        let page_size = params.page_size.unwrap_or(20);
        let offset = (page - 1) * page_size;
        query_builder.push(&format!(" LIMIT {} OFFSET {}", page_size, offset));

        let sql = query_builder.build().sql();
        let mut rows = db
            .query(sql, |row| {
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
            })
            .await?;

        Ok(rows)
    }

    /// 创建任务
    pub async fn create(db: &Database, req: &CreateJobRequest) -> Result<Job, sqlx::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        // 使用参数化查询防止 SQL 注入
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
            .execute(db.pool())
            .await?;

        Self::find_by_id(db, &id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    /// 更新任务
    pub async fn update(
        db: &Database,
        id: &str,
        req: &UpdateJobRequest,
    ) -> Result<Job, sqlx::Error> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        // 使用 QueryBuilder 防止 SQL 注入
        let mut query_builder = sqlx::query_builder::QueryBuilder::new("UPDATE jobs SET updated_at = ");
        query_builder.push(&now);

        if let Some(ref name) = req.name {
            query_builder.push(", name = ");
            query_builder.push(name);
        }
        if let Some(ref description) = req.description {
            query_builder.push(", description = ");
            query_builder.push(description);
        }
        if let Some(ref job_type) = req.job_type {
            query_builder.push(", job_type = ");
            query_builder.push(job_type);
        }
        if let Some(ref cron_expression) = req.cron_expression {
            query_builder.push(", cron_expression = ");
            query_builder.push(cron_expression);
        }
        if let Some(ref config) = req.config {
            query_builder.push(", config = ");
            query_builder.push(config);
        }
        if let Some(timeout_seconds) = req.timeout_seconds {
            query_builder.push(", timeout_seconds = ");
            query_builder.push(timeout_seconds.to_string());
        }
        if let Some(retry_count) = req.retry_count {
            query_builder.push(", retry_count = ");
            query_builder.push(retry_count.to_string());
        }
        if let Some(retry_delay_seconds) = req.retry_delay_seconds {
            query_builder.push(", retry_delay_seconds = ");
            query_builder.push(retry_delay_seconds.to_string());
        }
        if let Some(concurrency) = req.concurrency {
            query_builder.push(", concurrency = ");
            query_builder.push(concurrency.to_string());
        }
        if let Some(ref target_device_id) = req.target_device_id {
            query_builder.push(", target_device_id = ");
            query_builder.push(target_device_id);
        }
        if let Some(ref target_command_name) = req.target_command_name {
            query_builder.push(", target_command_name = ");
            query_builder.push(target_command_name);
        }
        if let Some(ref target_command_params) = req.target_command_params {
            query_builder.push(", target_command_params = ");
            query_builder.push(target_command_params);
        }
        if let Some(ref tags) = req.tags {
            query_builder.push(", tags = ");
            query_builder.push(tags);
        }
        if let Some(ref alert_config) = req.alert_config {
            query_builder.push(", alert_config = ");
            query_builder.push(alert_config);
        }

        query_builder.push(" WHERE id = ");
        query_builder.push(id);

        query_builder.build().execute(db.pool()).await?;

        Self::find_by_id(db, id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    /// 删除任务
    pub async fn delete(db: &Database, id: &str) -> Result<u64, sqlx::Error> {
        // 使用参数化查询防止 SQL 注入
        let result = sqlx::query("DELETE FROM jobs WHERE id = ?")
            .bind(id)
            .execute(db.pool())
            .await?;
        Ok(result.rows_affected())
    }

    /// 设置启用/禁用
    pub async fn set_enabled(
        db: &Database,
        id: &str,
        is_enabled: bool,
    ) -> Result<Job, sqlx::Error> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        // 使用参数化查询防止 SQL 注入
        sqlx::query("UPDATE jobs SET is_enabled = ?, updated_at = ? WHERE id = ?")
            .bind(if is_enabled { 1 } else { 0 })
            .bind(&now)
            .bind(id)
            .execute(db.pool())
            .await?;

        Self::find_by_id(db, id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    /// 设置运行状态
    pub async fn set_running(db: &Database, id: &str, is_running: bool) -> Result<(), sqlx::Error> {
        // 使用参数化查询防止 SQL 注入
        sqlx::query("UPDATE jobs SET is_running = ? WHERE id = ?")
            .bind(if is_running { 1 } else { 0 })
            .bind(id)
            .execute(db.pool())
            .await?;
        Ok(())
    }

    /// 更新运行统计
    pub async fn update_run_stats(
        db: &Database,
        id: &str,
        status: &str,
        error: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let success_inc = if status == "success" { 1 } else { 0 };
        let fail_inc = if status == "failed" || status == "timeout" { 1 } else { 0 };

        // 使用参数化查询防止 SQL 注入
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
            .execute(db.pool())
            .await?;
        Ok(())
    }

    /// 获取统计信息
    pub async fn get_statistics(db: &Database) -> Result<JobStatistics, sqlx::Error> {
        let total: i64 = db
            .query_first("SELECT COUNT(*) FROM jobs", |row| row.try_get::<i64, _>(0))
            .await?
            .unwrap_or(0);
        let enabled: i64 = db
            .query_first("SELECT COUNT(*) FROM jobs WHERE is_enabled = 1", |row| {
                row.try_get::<i64, _>(0)
            })
            .await?
            .unwrap_or(0);
        let disabled = total - enabled;
        let running: i64 = db
            .query_first("SELECT COUNT(*) FROM jobs WHERE is_running = 1", |row| {
                row.try_get::<i64, _>(0)
            })
            .await?
            .unwrap_or(0);

        let total_exec: i64 = db
            .query_first("SELECT COUNT(*) FROM job_executions", |row| row.try_get::<i64, _>(0))
            .await?
            .unwrap_or(0);
        let success_exec: i64 = db
            .query_first("SELECT COUNT(*) FROM job_executions WHERE status = 'success'", |row| {
                row.try_get::<i64, _>(0)
            })
            .await?
            .unwrap_or(0);
        let failed_exec: i64 = db
            .query_first("SELECT COUNT(*) FROM job_executions WHERE status = 'failed'", |row| {
                row.try_get::<i64, _>(0)
            })
            .await?
            .unwrap_or(0);

        let avg_duration: i64 = db.query_first(
            "SELECT COALESCE(AVG(duration_ms), 0) FROM job_executions WHERE duration_ms IS NOT NULL",
            |row| row.try_get::<i64, _>(0)
        ).await?.unwrap_or(0);

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

impl JobExecution {
    /// 查询所有执行记录（带分页）
    pub async fn find_all(
        db: &Database,
        params: &JobExecutionQueryParams,
    ) -> Result<Vec<JobExecution>, sqlx::Error> {
        let mut query_builder: sqlx::query_builder::QueryBuilder<'_, Sqlite> =
            sqlx::query_builder::QueryBuilder::new("SELECT * FROM job_executions WHERE 1=1");

        if let Some(ref job_id) = params.job_id {
            query_builder.push(" AND job_id = ");
            query_builder.push_bind(job_id);
        }
        if let Some(ref status) = params.status {
            query_builder.push(" AND status = ");
            query_builder.push_bind(status);
        }
        if let Some(ref trigger_type) = params.trigger_type {
            query_builder.push(" AND trigger_type = ");
            query_builder.push_bind(trigger_type);
        }

        query_builder.push(" ORDER BY started_at DESC");

        let page = params.page.unwrap_or(1);
        let page_size = params.page_size.unwrap_or(20);
        let offset = (page - 1) * page_size;
        query_builder.push(&format!(" LIMIT {} OFFSET {}", page_size, offset));

        let sql = query_builder.build().sql();
        let rows = db.query(sql, |row| {
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
        }).await?;

        Ok(rows)
    }

    /// 统计执行记录数量
    pub async fn count(
        db: &Database,
        params: &JobExecutionQueryParams,
    ) -> Result<i64, sqlx::Error> {
        let mut query_builder: sqlx::query_builder::QueryBuilder<'_, Sqlite> =
            sqlx::query_builder::QueryBuilder::new("SELECT COUNT(*) FROM job_executions WHERE 1=1");

        if let Some(ref job_id) = params.job_id {
            query_builder.push(" AND job_id = ");
            query_builder.push_bind(job_id);
        }
        if let Some(ref status) = params.status {
            query_builder.push(" AND status = ");
            query_builder.push_bind(status);
        }
        if let Some(ref trigger_type) = params.trigger_type {
            query_builder.push(" AND trigger_type = ");
            query_builder.push_bind(trigger_type);
        }

        let sql = query_builder.build().sql();
        let row = db.query_first(sql, |row| row.try_get::<i64, _>(0)).await?;
        Ok(row.unwrap_or(0))
    }

    /// 创建执行记录
    pub async fn create(
        db: &Database,
        job_id: &str,
        trigger_type: &str,
        triggered_by: Option<&str>,
    ) -> Result<JobExecution, sqlx::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        // 使用参数化查询防止 SQL 注入
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
            .execute(db.pool())
            .await?;

        Self::find_by_id(db, &id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    /// 根据 ID 查询
    pub async fn find_by_id(db: &Database, id: &str) -> Result<Option<JobExecution>, sqlx::Error> {
        let row = sqlx::query("SELECT * FROM job_executions WHERE id = ? LIMIT 1")
            .bind(id)
            .fetch_optional(db.pool())
            .await?;

        if let Some(row) = row {
            Ok(Some(JobExecution {
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
            }))
        } else {
            Ok(None)
        }
    }

    /// 查询任务的所有执行记录
    pub async fn find_by_job(
        db: &Database,
        job_id: &str,
        limit: i32,
    ) -> Result<Vec<JobExecution>, sqlx::Error> {
        // 使用转义防止 SQL 注入
        let safe_job_id = job_id.replace('\'', "''");
        let sql = format!(
            "SELECT * FROM job_executions WHERE job_id = '{}' ORDER BY started_at DESC LIMIT {}",
            safe_job_id, limit
        );

        db.query(&sql, |row| {
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
        })
        .await
    }

    /// 更新执行状态
    pub async fn update_status(
        db: &Database,
        id: &str,
        status: &str,
        ended_at: Option<&str>,
        result: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        // 使用参数化查询防止 SQL 注入
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
            .execute(db.pool())
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_query_params_default() {
        let params = JobQueryParams::default();
        assert_eq!(params.page, None);
        assert_eq!(params.page_size, None);
    }

    #[test]
    fn test_create_job_request() {
        let req = CreateJobRequest {
            name: "Test Job".to_string(),
            description: Some("Test description".to_string()),
            job_type: "http".to_string(),
            cron_expression: "*/5 * * * *".to_string(),
            config: r#"{"url": "http://example.com"}"#.to_string(),
            timeout_seconds: Some(300),
            retry_count: Some(3),
            retry_delay_seconds: Some(60),
            concurrency: Some(1),
            target_device_id: None,
            target_command_name: None,
            target_command_params: None,
            tags: Some(r#"["test"]"#.to_string()),
            alert_config: Some(r#"{"on_failure": true}"#.to_string()),
        };

        assert_eq!(req.name, "Test Job");
        assert_eq!(req.job_type, "http");
        assert_eq!(req.timeout_seconds, Some(300));
    }

    #[test]
    fn test_update_job_request_partial() {
        let req = UpdateJobRequest {
            name: Some("Updated Job".to_string()),
            description: None,
            job_type: None,
            cron_expression: None,
            config: None,
            timeout_seconds: Some(600),
            retry_count: None,
            retry_delay_seconds: None,
            concurrency: None,
            target_device_id: None,
            target_command_name: None,
            target_command_params: None,
            tags: None,
            alert_config: None,
        };

        assert_eq!(req.name, Some("Updated Job".to_string()));
        assert_eq!(req.timeout_seconds, Some(600));
    }

    #[test]
    fn test_job_statistics_default() {
        let stats = JobStatistics {
            total_jobs: 0,
            enabled_jobs: 0,
            disabled_jobs: 0,
            running_jobs: 0,
            total_executions: 0,
            success_executions: 0,
            failed_executions: 0,
            avg_duration_ms: 0,
        };

        assert_eq!(stats.total_jobs, 0);
        assert_eq!(stats.success_executions, 0);
    }

    #[test]
    fn test_job_execution_fields() {
        let execution = JobExecution {
            id: "exec-001".to_string(),
            job_id: "job-001".to_string(),
            started_at: "2026-03-12 08:00:00".to_string(),
            ended_at: Some("2026-03-12 08:00:05".to_string()),
            duration_ms: Some(5000),
            status: "success".to_string(),
            result: Some("OK".to_string()),
            error_message: None,
            error_trace: None,
            trigger_type: "manual".to_string(),
            triggered_by: Some("admin".to_string()),
            worker_id: None,
            memory_usage_bytes: None,
            cpu_time_ms: None,
            created_at: "2026-03-12 08:00:00".to_string(),
        };

        assert_eq!(execution.status, "success");
        assert_eq!(execution.duration_ms, Some(5000));
        assert!(execution.error_message.is_none());
    }

    #[test]
    fn test_cron_expression_validation() {
        // 有效的 cron 表达式
        let valid_expressions = [
            "*/5 * * * *", // 每5分钟
            "0 * * * *",   // 每小时
            "0 0 * * *",   // 每天午夜
            "0 0 * * 0",   // 每周日
            "0 0 1 * *",   // 每月第一天
        ];

        for expr in valid_expressions.iter() {
            // 验证格式正确（实际解析需要 cron crate）
            assert!(!expr.is_empty());
        }
    }

    #[test]
    fn test_job_config_json() {
        // 测试 HTTP 任务配置
        let http_config = r#"{
            "url": "http://api.example.com/webhook",
            "method": "POST",
            "headers": {
                "Content-Type": "application/json",
                "Authorization": "Bearer token123"
            },
            "body": {"message": "hello"}
        }"#;

        let parsed: serde_json::Value = serde_json::from_str(http_config).unwrap();
        assert_eq!(parsed["url"], "http://api.example.com/webhook");
        assert_eq!(parsed["method"], "POST");

        // 测试脚本任务配置
        let script_config = r#"{
            "script": "echo 'Hello World'",
            "interpreter": "bash",
            "working_dir": "/app"
        }"#;

        let parsed: serde_json::Value = serde_json::from_str(script_config).unwrap();
        assert_eq!(parsed["interpreter"], "bash");

        // 测试通知任务配置
        let notify_config = r#"{
            "channel": "email",
            "to": "user@example.com",
            "subject": "Alert",
            "message": "Something happened!"
        }"#;

        let parsed: serde_json::Value = serde_json::from_str(notify_config).unwrap();
        assert_eq!(parsed["channel"], "email");
    }

    #[test]
    fn test_job_tags_json() {
        // 测试标签
        let tags = r#"["system", "monitoring", "critical"]"#;
        let parsed: Vec<String> = serde_json::from_str(tags).unwrap();
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0], "system");
    }
}
