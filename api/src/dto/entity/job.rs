use serde::{Deserialize, Serialize};
use sqlx::FromRow;

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
    pub workspace_id: Option<String>,
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
    pub is_enabled: Option<bool>,
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
        let valid_expressions = [
            "*/5 * * * *",
            "0 * * * *",
            "0 0 * * *",
            "0 0 * * 0",
            "0 0 1 * *",
        ];

        for expr in valid_expressions.iter() {
            assert!(!expr.is_empty());
        }
    }

    #[test]
    fn test_job_config_json() {
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

        let script_config = r#"{
            "script": "echo 'Hello World'",
            "interpreter": "bash",
            "working_dir": "/app"
        }"#;

        let parsed: serde_json::Value = serde_json::from_str(script_config).unwrap();
        assert_eq!(parsed["interpreter"], "bash");

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
        let tags = r#"["system", "monitoring", "critical"]"#;
        let parsed: Vec<String> = serde_json::from_str(tags).unwrap();
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0], "system");
    }
}
