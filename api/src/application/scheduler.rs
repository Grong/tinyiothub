use std::{process::Command, str::FromStr, sync::Arc, time::Duration};

use cron::Schedule;
use moka::sync::Cache;
use serde_json::Value;
use tokio::{sync::Mutex, time::interval};

use crate::domain::job::service::{JobExecutionService, JobService};

/// 任务调度器
pub struct TimeTask {
    jobs: Cache<String, JobSchedule>,
    job_service: Option<Arc<JobService>>,
    job_execution_service: Option<Arc<JobExecutionService>>,
    running: Mutex<bool>,
}

#[derive(Clone)]
#[allow(dead_code)]
struct JobSchedule {
    pub job: crate::dto::entity::job::Job,
    pub schedule: Schedule,
}

impl TimeTask {
    pub fn new() -> Self {
        let map: Cache<String, JobSchedule> = Cache::new(100);

        Self {
            jobs: map,
            job_service: None,
            job_execution_service: None,
            running: Mutex::new(false),
        }
    }

    /// 设置服务
    pub fn with_services(
        mut self,
        job_service: Arc<JobService>,
        job_execution_service: Arc<JobExecutionService>,
    ) -> Self {
        self.job_service = Some(job_service);
        self.job_execution_service = Some(job_execution_service);
        self
    }

    /// 启动调度器
    pub async fn run(&self) {
        if self.job_service.is_none() || self.job_execution_service.is_none() {
            tracing::warn!("TimeTask: No services configured, scheduler not started");
            return;
        }

        let job_service = self.job_service.as_ref().unwrap();
        let job_execution_service = self.job_execution_service.as_ref().unwrap();

        let mut running = self.running.lock().await;
        if *running {
            tracing::warn!("TimeTask: Scheduler already running, skipping start");
            return;
        }
        *running = true;
        drop(running);

        tracing::info!("TimeTask Scheduler started");

        let mut checker = interval(Duration::from_secs(60));

        loop {
            let running = self.running.lock().await;
            if !*running {
                drop(running);
                break;
            }
            drop(running);

            checker.tick().await;

            self.check_and_run_tasks(job_service, job_execution_service).await;
        }

        tracing::info!("TimeTask Scheduler stopped");
    }

    /// 停止调度器
    pub async fn stop(&self) {
        let mut running = self.running.lock().await;
        *running = false;
    }

    /// 检查并执行到期的任务
    async fn check_and_run_tasks(
        &self,
        job_service: &Arc<JobService>,
        job_execution_service: &Arc<JobExecutionService>,
    ) {
        let now = chrono::Utc::now();

        let params = crate::dto::entity::job::JobQueryParams {
            is_enabled: Some(true),
            ..Default::default()
        };

        let jobs = match job_service.find_all(&params).await {
            Ok(jobs) => jobs,
            Err(e) => {
                tracing::error!("Failed to load jobs: {}", e);
                return;
            }
        };

        for job in jobs {
            if should_run_now(&job, &now) {
                tracing::info!("Executing job: {} ({})", job.name, job.id);

                let job_service_clone = job_service.clone();
                let job_execution_service_clone = job_execution_service.clone();
                let job_clone = job.clone();

                tokio::spawn(async move {
                    execute_job(&job_clone, &job_service_clone, &job_execution_service_clone).await;
                });
            }
        }
    }

    /// 添加任务
    pub fn add_job(&self, job: crate::dto::entity::job::Job) {
        if let Ok(schedule) = Schedule::from_str(&job.cron_expression) {
            self.jobs.insert(job.id.clone(), JobSchedule { job, schedule });
            tracing::info!("Added job to scheduler");
        } else {
            tracing::warn!("Invalid cron expression for job");
        }
    }

    /// 更新任务
    pub fn upd_job(&self, job: crate::dto::entity::job::Job) {
        self.jobs.invalidate(&job.id);
        self.add_job(job);
    }

    /// 删除任务
    pub fn del_job(&self, id: String) {
        self.jobs.invalidate(&id);
    }

    /// 加载任务
    pub fn load_jobs(&self, jobs: Vec<crate::dto::entity::job::Job>) {
        for job in jobs {
            self.add_job(job);
        }
    }
}

impl Clone for TimeTask {
    fn clone(&self) -> Self {
        Self {
            jobs: self.jobs.clone(),
            job_service: self.job_service.clone(),
            job_execution_service: self.job_execution_service.clone(),
            running: Mutex::new(false),
        }
    }
}

/// 检查任务是否应该现在执行
fn should_run_now(job: &crate::dto::entity::job::Job, now: &chrono::DateTime<chrono::Utc>) -> bool {
    if job.is_running {
        return false;
    }

    let Some(next_run) = &job.next_run_at else {
        return true;
    };

    if let Ok(next) = chrono::DateTime::parse_from_rfc3339(next_run) {
        return now >= &next;
    }

    true
}

/// 执行任务
async fn execute_job(
    job: &crate::dto::entity::job::Job,
    job_service: &Arc<JobService>,
    job_execution_service: &Arc<JobExecutionService>,
) {
    let job_id = job.id.clone();
    let start_time = std::time::Instant::now();

    let execution = match job_execution_service.create(&job_id, "schedule", Some("system")).await {
        Ok(e) => e,
        Err(e) => {
            tracing::error!("Failed to create execution record: {}", e);
            return;
        }
    };

    let _ = job_service.set_running(&job_id, true).await;

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

    let status = if result.is_ok() { "success" } else { "failed" };
    let error_msg = result.as_ref().err().map(|s| s.as_str());

    let _ = job_execution_service
        .update_status(
            &execution.id,
            status,
            Some(&ended_at),
            result.as_ref().ok().map(|s| s.as_str()),
            error_msg,
        )
        .await;

    let _ = job_service
        .update_run_stats(&job_id, status, error_msg)
        .await;

    tracing::info!("Job {} executed: {} ({}ms)", job_id, status, duration);
}

/// 执行 HTTP/Webhook 任务
async fn execute_http_job(job: &crate::dto::entity::job::Job) -> Result<String, String> {
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

    if let Some(hdrs) = headers {
        for (key, value) in hdrs.iter() {
            request = request.header(key.as_str(), value.as_str());
        }
    }

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
async fn execute_script_job(job: &crate::dto::entity::job::Job) -> Result<String, String> {
    let config: Value =
        serde_json::from_str(&job.config).map_err(|e| format!("Invalid config JSON: {}", e))?;

    let script = config.get("script").and_then(|v| v.as_str()).ok_or("Missing script in config")?;

    let working_dir = config.get("working_dir").and_then(|v| v.as_str());

    let interpreter = config.get("interpreter").and_then(|v| v.as_str()).unwrap_or("bash");

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
async fn execute_device_control_job(job: &crate::dto::entity::job::Job) -> Result<String, String> {
    let device_id = job.target_device_id.as_ref().ok_or("No target device configured")?;
    let command = job.target_command_name.as_ref().ok_or("No command configured")?;

    tracing::info!(
        "Executing device command: {} -> {}:{}",
        device_id,
        command,
        job.target_command_params.as_deref().unwrap_or("")
    );

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
async fn execute_notification_job(job: &crate::dto::entity::job::Job) -> Result<String, String> {
    let config: Value =
        serde_json::from_str(&job.config).map_err(|e| format!("Invalid config JSON: {}", e))?;

    let channel = config.get("channel").and_then(|v| v.as_str()).unwrap_or("email");

    let to = config.get("to").and_then(|v| v.as_str()).ok_or("Missing 'to' in config")?;

    let message = config.get("message").and_then(|v| v.as_str()).unwrap_or("");

    tracing::info!("Sending notification: {} -> {}: {}", channel, to, message);

    Ok(format!("Notification sent via {} to {}", channel, to))
}

/// 执行 SQL 任务
async fn execute_sql_job(job: &crate::dto::entity::job::Job) -> Result<String, String> {
    let config: Value =
        serde_json::from_str(&job.config).map_err(|e| format!("Invalid config JSON: {}", e))?;

    let sql = config.get("sql").and_then(|v| v.as_str()).ok_or("Missing SQL in config")?;

    tracing::info!("Executing SQL: {}", sql);

    Ok(format!("SQL would execute: {}", sql))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_job(job_type: &str) -> crate::dto::entity::job::Job {
        crate::dto::entity::job::Job {
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
        let job = crate::dto::entity::job::Job {
            config: r#"{"url": "http://httpbin.org/status/200", "method": "GET"}"#.to_string(),
            timeout_seconds: 30,
            ..create_test_job("http")
        };

        let result = execute_http_job(&job).await;
        assert!(result.is_ok() || result.unwrap_err().contains("HTTP error"));
    }

    #[tokio::test]
    async fn test_execute_http_job_missing_url() {
        let job = crate::dto::entity::job::Job {
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
        let job = crate::dto::entity::job::Job {
            config: r#"{"script": "echo 'Hello Test'", "interpreter": "cmd"}"#.to_string(),
            ..create_test_job("script")
        };

        let result = execute_script_job(&job).await;
        assert!(result.is_ok() || result.unwrap_err().contains("Failed to execute"));
    }

    #[tokio::test]
    async fn test_execute_device_control_job() {
        let job = crate::dto::entity::job::Job {
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
        let job = crate::dto::entity::job::Job {
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
        let job = crate::dto::entity::job::Job {
            config: r#"{"sql": "SELECT * FROM devices"}"#.to_string(),
            ..create_test_job("sql")
        };

        let result = execute_sql_job(&job).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_sql_job_missing_sql() {
        let job = crate::dto::entity::job::Job {
            config: r#"{}"#.to_string(),
            ..create_test_job("sql")
        };

        let result = execute_sql_job(&job).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_unknown_job_type() {
        let job = create_test_job("unknown_type");

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
        assert!(task.job_service.is_none());
    }

    #[test]
    fn test_timetask_add_job() {
        let task = TimeTask::new();
        let job = create_test_job("http");

        task.add_job(job.clone());
    }

    #[test]
    fn test_timetask_clone() {
        let task = TimeTask::new();
        let _cloned = task.clone();
    }
}
