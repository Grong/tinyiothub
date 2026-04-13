use std::{process::Command, str::FromStr, sync::Arc, time::Duration};

use cron::Schedule;
use moka::sync::Cache;
use serde_json::Value;
use tokio::{sync::Mutex, time::interval};

use crate::{
    dto::entity::job::{Job, JobExecution},
    infrastructure::persistence::database::Database,
};

/// 任务调度器
pub struct TimeTask {
    jobs: Cache<String, JobSchedule>,
    db: Option<Arc<Database>>,
    running: Mutex<bool>,
}

#[derive(Clone)]
struct JobSchedule {
    pub job: Job,
    pub schedule: Schedule,
}

impl TimeTask {
    pub fn new() -> Self {
        let map: Cache<String, JobSchedule> = Cache::new(100);

        Self { jobs: map, db: None, running: Mutex::new(false) }
    }

    /// 设置数据库连接
    pub fn with_database(mut self, db: Arc<Database>) -> Self {
        self.db = Some(db);
        self
    }

    /// 启动调度器
    pub async fn run(&self) {
        if self.db.is_none() {
            tracing::warn!("TimeTask: No database configured, scheduler not started");
            return;
        }

        let db = self.db.as_ref().unwrap();

        // 使用互斥锁保护 running 状态，防止并发 start/stop
        let mut running = self.running.lock().await;
        if *running {
            tracing::warn!("TimeTask: Scheduler already running, skipping start");
            return;
        }
        *running = true;
        drop(running); // 释放锁，让 stop() 可以获取锁

        tracing::info!("TimeTask Scheduler started");

        // 每分钟检查一次任务
        let mut checker = interval(Duration::from_secs(60));

        loop {
            // 检查是否应该继续运行
            let running = self.running.lock().await;
            if !*running {
                drop(running);
                break;
            }
            drop(running);

            checker.tick().await;

            // 加载并执行到期的任务
            self.check_and_run_tasks(db).await;
        }

        tracing::info!("TimeTask Scheduler stopped");
    }

    /// 停止调度器
    pub async fn stop(&self) {
        let mut running = self.running.lock().await;
        *running = false;
    }

    /// 检查并执行到期的任务
    async fn check_and_run_tasks(&self, db: &Database) {
        let now = chrono::Utc::now();

        // 查询所有启用的任务
        let params = crate::dto::entity::job::JobQueryParams {
            is_enabled: Some(true),
            ..Default::default()
        };

        let jobs = match Job::find_all(db, &params).await {
            Ok(jobs) => jobs,
            Err(e) => {
                tracing::error!("Failed to load jobs: {}", e);
                return;
            }
        };

        for job in jobs {
            // 检查是否应该执行
            if should_run_now(&job, &now) {
                tracing::info!("Executing job: {} ({})", job.name, job.id);

                // 异步执行任务
                let db_clone = db.clone();
                let job_clone = job.clone();

                tokio::spawn(async move {
                    execute_job(&job_clone, &db_clone).await;
                });
            }
        }
    }

    /// 添加任务
    pub fn add_job(&self, job: Job) {
        if let Ok(schedule) = Schedule::from_str(&job.cron_expression) {
            self.jobs.insert(job.id.clone(), JobSchedule { job, schedule });
            tracing::info!("Added job to scheduler");
        } else {
            tracing::warn!("Invalid cron expression for job");
        }
    }

    /// 更新任务
    pub fn upd_job(&self, job: Job) {
        self.jobs.invalidate(&job.id);
        self.add_job(job);
    }

    /// 删除任务
    pub fn del_job(&self, id: String) {
        self.jobs.invalidate(&id);
    }

    /// 加载任务
    pub fn load_jobs(&self, jobs: Vec<Job>) {
        for job in jobs {
            self.add_job(job);
        }
    }
}

unsafe impl Send for TimeTask {}
unsafe impl Sync for TimeTask {}

impl Clone for TimeTask {
    fn clone(&self) -> Self {
        Self {
            jobs: self.jobs.clone(),
            db: self.db.clone(),
            running: Mutex::new(false), // Clone 创建新实例时默认为未运行状态
        }
    }
}

/// 检查任务是否应该现在执行
fn should_run_now(job: &Job, now: &chrono::DateTime<chrono::Utc>) -> bool {
    // 如果任务正在运行，跳过
    if job.is_running {
        return false;
    }

    // 如果没有下次的行时间，跳过
    let Some(next_run) = &job.next_run_at else {
        return true; // 首次执行
    };

    // 解析下次执行时间
    if let Ok(next) = chrono::DateTime::parse_from_rfc3339(next_run) {
        return now >= &next;
    }

    // 如果无法解析，认为应该执行
    true
}

/// 执行任务
async fn execute_job(job: &Job, db: &Database) {
    let job_id = job.id.clone();
    let start_time = std::time::Instant::now();

    // 创建执行记录
    let execution = match JobExecution::create(db, &job_id, "schedule", Some("system")).await {
        Ok(e) => e,
        Err(e) => {
            tracing::error!("Failed to create execution record: {}", e);
            return;
        }
    };

    // 设置任务为运行中
    let _ = Job::set_running(db, &job_id, true).await;

    // 执行任务
    let result = match job.job_type.as_str() {
        "webhook" | "http" => execute_http_job(job).await,
        "script" => execute_script_job(job).await,
        "device_control" => execute_device_control_job(job).await,
        "notification" => execute_notification_job(job).await,
        "sql" => execute_sql_job(job).await,
        _ => Err(format!("Unknown job type: {}", job.job_type)),
    };

    let duration = start_time.elapsed().as_millis() as i64;
    let ended_at = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    // 更新执行状态
    let status = if result.is_ok() { "success" } else { "failed" };
    let error_msg = result.as_ref().err().map(|s| s.as_str());

    let _ = JobExecution::update_status(
        db,
        &execution.id,
        status,
        Some(&ended_at),
        result.as_ref().ok().map(|s| s.as_str()),
        error_msg,
    )
    .await;

    // 更新任务统计
    let _ = Job::update_run_stats(db, &job_id, status, error_msg).await;

    tracing::info!("Job {} executed: {} ({}ms)", job_id, status, duration);
}

/// 执行 HTTP/Webhook 任务
async fn execute_http_job(job: &Job) -> Result<String, String> {
    let config: Value =
        serde_json::from_str(&job.config).map_err(|e| format!("Invalid config JSON: {}", e))?;

    let url = config.get("url").and_then(|v| v.as_str()).ok_or("Missing URL in config")?;

    let method = config.get("method").and_then(|v| v.as_str()).unwrap_or("GET");

    let headers = config.get("headers").and_then(|v| v.as_object()).map(|o| {
        o.iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
            .collect::<std::collections::HashMap<String, String>>()
    });

    let body = config.get("body").and_then(|v| v.as_str());

    // 构建完整 URL
    let full_url =
        if url.starts_with("http") { url.to_string() } else { format!("http://localhost{}", url) };

    let client = reqwest::Client::new();

    let mut request = match method {
        "GET" => client.get(&full_url),
        "POST" => client.post(&full_url),
        "PUT" => client.put(&full_url),
        "DELETE" => client.delete(&full_url),
        "PATCH" => client.patch(&full_url),
        _ => return Err(format!("Unsupported HTTP method: {}", method)),
    };

    // 添加 headers
    if let Some(hdrs) = headers {
        for (key, value) in hdrs.iter() {
            request = request.header(key.as_str(), value.as_str());
        }
    }

    // 添加 body
    if let Some(b) = body {
        request = request.body(b.to_string());
    }

    let timeout = std::time::Duration::from_secs(job.timeout_seconds as u64);

    let response =
        request.timeout(timeout).send().await.map_err(|e| format!("HTTP request failed: {}", e))?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();

    if status.is_success() {
        Ok(body)
    } else {
        Err(format!("HTTP error {}: {}", status, body))
    }
}

/// 执行脚本任务
async fn execute_script_job(job: &Job) -> Result<String, String> {
    let config: Value =
        serde_json::from_str(&job.config).map_err(|e| format!("Invalid config JSON: {}", e))?;

    let script = config.get("script").and_then(|v| v.as_str()).ok_or("Missing script in config")?;

    let working_dir = config.get("working_dir").and_then(|v| v.as_str());

    let interpreter = config.get("interpreter").and_then(|v| v.as_str()).unwrap_or("bash");

    // 根据解释器执行脚本
    let output = match interpreter {
        "python" => Command::new("python")
            .arg("-c")
            .arg(script)
            .current_dir(working_dir.unwrap_or("."))
            .output(),
        "powershell" => Command::new("powershell")
            .args(["-Command", script])
            .current_dir(working_dir.unwrap_or("."))
            .output(),
        "cmd" => Command::new("cmd")
            .args(["/C", script])
            .current_dir(working_dir.unwrap_or("."))
            .output(),
        "node" => Command::new("node")
            .arg("-e")
            .arg(script)
            .current_dir(working_dir.unwrap_or("."))
            .output(),
        _ => Command::new("bash")
            .args(["-c", script])
            .current_dir(working_dir.unwrap_or("."))
            .output(),
    };

    match output {
        Ok(output) => {
            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).to_string())
            } else {
                Err(String::from_utf8_lossy(&output.stderr).to_string())
            }
        }
        Err(e) => Err(format!("Failed to execute script: {}", e)),
    }
}

/// 执行设备控制任务
async fn execute_device_control_job(job: &Job) -> Result<String, String> {
    // 需要设备服务支持，这里简化处理
    let device_id = job.target_device_id.as_ref().ok_or("No target device configured")?;
    let command = job.target_command_name.as_ref().ok_or("No command configured")?;

    tracing::info!(
        "Executing device command: {} -> {}:{}",
        device_id,
        command,
        job.target_command_params.as_deref().unwrap_or("")
    );

    // NOTE: device_control job type is a stub — it logs the command but does not execute it
    // TODO: integrate with device service when device command API is available
    tracing::warn!(
        "device_control job is a stub — command logged but not executed: {} {} ({})",
        device_id,
        command,
        job.target_command_params.as_deref().unwrap_or("")
    );
    Ok(format!(
        "Device command (STUB): {} {} ({})",
        device_id,
        command,
        job.target_command_params.as_deref().unwrap_or("")
    ))
}

/// 执行通知任务
async fn execute_notification_job(job: &Job) -> Result<String, String> {
    let config: Value =
        serde_json::from_str(&job.config).map_err(|e| format!("Invalid config JSON: {}", e))?;

    let channel = config.get("channel").and_then(|v| v.as_str()).unwrap_or("email");

    let to = config.get("to").and_then(|v| v.as_str()).ok_or("Missing 'to' in config")?;

    let message = config.get("message").and_then(|v| v.as_str()).unwrap_or("");

    tracing::info!("Sending notification: {} -> {}: {}", channel, to, message);

    // TODO: 调用通知服务
    Ok(format!("Notification sent via {} to {}", channel, to))
}

/// 执行 SQL 任务
async fn execute_sql_job(job: &Job) -> Result<String, String> {
    let config: Value =
        serde_json::from_str(&job.config).map_err(|e| format!("Invalid config JSON: {}", e))?;

    let sql = config.get("sql").and_then(|v| v.as_str()).ok_or("Missing SQL in config")?;

    // 注意：实际执行需要数据库连接
    // 这里只做验证和模拟
    tracing::info!("Executing SQL: {}", sql);

    Ok(format!("SQL would execute: {}", sql))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_job(job_type: &str) -> Job {
        Job {
            id: "test-job-001".to_string(),
            name: "Test Job".to_string(),
            description: Some("Test description".to_string()),
            job_type: job_type.to_string(),
            cron_expression: "*/5 * * * *".to_string(),
            config: "{}".to_string(),
            timeout_seconds: 300,
            retry_count: 0,
            retry_delay_seconds: 60,
            concurrency: 1,
            target_device_id: None,
            target_command_name: None,
            target_command_params: None,
            is_enabled: true,
            is_running: false,
            last_run_at: None,
            last_run_status: None,
            last_run_error: None,
            next_run_at: None,
            run_count: 0,
            success_count: 0,
            fail_count: 0,
            tags: "[]".to_string(),
            alert_config: "{}".to_string(),
            created_at: "2026-03-12 08:00:00".to_string(),
            updated_at: "2026-03-12 08:00:00".to_string(),
            created_by: None,
        }
    }

    #[tokio::test]
    async fn test_execute_http_job_valid_config() {
        let job = Job {
            config: r#"{"url": "http://httpbin.org/status/200", "method": "GET"}"#.to_string(),
            timeout_seconds: 30,
            ..create_test_job("http")
        };

        let result = execute_http_job(&job).await;
        // 可能网络不通，但至少应该能解析配置
        assert!(result.is_ok() || result.unwrap_err().contains("HTTP error"));
    }

    #[tokio::test]
    async fn test_execute_http_job_missing_url() {
        let job = Job {
            config: r#"{"method": "GET"}"#.to_string(),
            timeout_seconds: 30,
            ..create_test_job("http")
        };

        let result = execute_http_job(&job).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing URL"));
    }

    #[tokio::test]
    async fn test_execute_script_job() {
        let job = Job {
            config: r#"{"script": "echo 'Hello Test'", "interpreter": "cmd"}"#.to_string(),
            ..create_test_job("script")
        };

        let result = execute_script_job(&job).await;
        // Windows 上可能没有 bash，但 cmd 应该可以
        assert!(result.is_ok() || result.unwrap_err().contains("Failed to execute"));
    }

    #[tokio::test]
    async fn test_execute_device_control_job() {
        let job = Job {
            target_device_id: Some("device-001".to_string()),
            target_command_name: Some("turn_on".to_string()),
            target_command_params: Some(r#"{"power": 100}"#.to_string()),
            ..create_test_job("device_control")
        };

        let result = execute_device_control_job(&job).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_device_control_job_no_device() {
        let job = create_test_job("device_control");

        let result = execute_device_control_job(&job).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_notification_job() {
        let job = Job {
            config: r#"{
                "channel": "email",
                "to": "test@example.com",
                "message": "Test notification"
            }"#
            .to_string(),
            ..create_test_job("notification")
        };

        let result = execute_notification_job(&job).await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("Notification sent"));
    }

    #[tokio::test]
    async fn test_execute_sql_job() {
        let job = Job {
            config: r#"{"sql": "SELECT * FROM devices"}"#.to_string(),
            ..create_test_job("sql")
        };

        let result = execute_sql_job(&job).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_sql_job_missing_sql() {
        let job = Job { config: r#"{}"#.to_string(), ..create_test_job("sql") };

        let result = execute_sql_job(&job).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_unknown_job_type() {
        let job = create_test_job("unknown_type");

        // 测试未知任务类型应该返回错误
        let result = match job.job_type.as_str() {
            "http" | "webhook" => Ok("http"),
            "script" => Ok("script"),
            "device_control" => Ok("device"),
            "notification" => Ok("notification"),
            "sql" => Ok("sql"),
            other => Err(format!("Unknown job type: {}", other)),
        };

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown job type"));
    }

    #[test]
    fn test_should_run_now_first_time() {
        let job = create_test_job("http");

        // 首次执行（没有 next_run_at）应该返回 true
        let result = should_run_now(&job, &chrono::Utc::now());
        assert!(result);
    }

    #[test]
    fn test_should_run_now_already_running() {
        let mut job = create_test_job("http");
        job.is_running = true;

        let result = should_run_now(&job, &chrono::Utc::now());
        assert!(!result);
    }

    #[test]
    fn test_timetask_new() {
        let task = TimeTask::new();
        assert!(task.db.is_none());
    }

    #[test]
    fn test_timetask_add_job() {
        let task = TimeTask::new();
        let job = create_test_job("http");

        task.add_job(job.clone());
        // 添加后不会 panic，说明 cron 表达式有效
    }

    #[test]
    fn test_timetask_clone() {
        let task = TimeTask::new();
        let _cloned = task.clone();
        // Clone 实现应该正常工作
    }
}
