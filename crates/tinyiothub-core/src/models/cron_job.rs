use serde::{Deserialize, Serialize};

/// CronJob entity
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CronJob {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub job_type: String,
    pub cron_expression: String,
    pub config: String,
    pub timeout_seconds: i32,
    pub max_retries: i32,
    pub is_enabled: bool,
    pub is_running: bool,
    pub last_run_at: Option<String>,
    pub last_run_status: Option<String>,
    pub last_run_error: Option<String>,
    pub next_run_at: Option<String>,
    pub run_count: i64,
    pub success_count: i64,
    pub fail_count: i64,
    pub created_at: String,
    pub updated_at: String,
    pub created_by: Option<String>,
}

impl CronJob {
    fn parsed_config(&self) -> Option<serde_json::Value> {
        if self.job_type != "device_command" {
            return None;
        }
        serde_json::from_str(&self.config).ok()
    }

    /// Parse config JSON and return device_id if job_type == "device_command"
    pub fn target_device_id(&self) -> Option<String> {
        self.parsed_config()
            .and_then(|v| v.get("device_id").and_then(|d| d.as_str()).map(String::from))
    }

    /// Return command_name from config if job_type == "device_command"
    pub fn target_command_name(&self) -> Option<String> {
        self.parsed_config()
            .and_then(|v| v.get("command_name").and_then(|c| c.as_str()).map(String::from))
    }

    /// Return params from config if job_type == "device_command"
    /// Handles both string params and JSON object params.
    pub fn target_command_params(&self) -> Option<String> {
        self.parsed_config().and_then(|v| {
            v.get("params").and_then(|p| {
                p.as_str()
                    .map(String::from)
                    .or_else(|| serde_json::to_string(p).ok())
            })
        })
    }
}

/// CronJob query parameters
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct CronJobQuery {
    pub name: Option<String>,
    pub job_type: Option<String>,
    pub is_enabled: Option<bool>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// Create CronJob request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateCronJobRequest {
    pub name: String,
    pub description: Option<String>,
    pub job_type: String,
    pub cron_expression: String,
    pub config: String,
    pub workspace_id: String,
    pub timeout_seconds: Option<i32>,
    pub max_retries: Option<i32>,
}

/// Update CronJob request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateCronJobRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub job_type: Option<String>,
    pub cron_expression: Option<String>,
    pub config: Option<String>,
    pub timeout_seconds: Option<i32>,
    pub max_retries: Option<i32>,
    pub is_enabled: Option<bool>,
}

/// CronRun entity
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CronRun {
    pub id: String,
    pub job_id: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub duration_ms: Option<i64>,
    pub status: String,
    pub output: Option<String>,
    pub error_message: Option<String>,
    pub trigger_type: String,
    pub triggered_by: Option<String>,
    pub created_at: String,
}

/// CronRun query parameters
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct CronRunQuery {
    pub job_id: Option<String>,
    pub status: Option<String>,
    pub trigger_type: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// Cron statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CronStatistics {
    pub total_jobs: i64,
    pub enabled_jobs: i64,
    pub disabled_jobs: i64,
    pub running_jobs: i64,
    pub total_runs: i64,
    pub success_runs: i64,
    pub failed_runs: i64,
    pub avg_duration_ms: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cron_job_config_extraction() {
        let job = CronJob {
            id: "job-001".to_string(),
            name: "Test Device Command".to_string(),
            description: None,
            job_type: "device_command".to_string(),
            cron_expression: "*/5 * * * *".to_string(),
            config: r#"{"device_id":"dev-123","command_name":"restart","params":"{\"delay\":5}"}"#.to_string(),
            timeout_seconds: 300,
            max_retries: 3,
            is_enabled: true,
            is_running: false,
            last_run_at: None,
            last_run_status: None,
            last_run_error: None,
            next_run_at: None,
            run_count: 0,
            success_count: 0,
            fail_count: 0,
            created_at: "2026-04-18 08:00:00".to_string(),
            updated_at: "2026-04-18 08:00:00".to_string(),
            created_by: Some("admin".to_string()),
        };

        assert_eq!(job.target_device_id(), Some("dev-123".to_string()));
        assert_eq!(job.target_command_name(), Some("restart".to_string()));
        assert_eq!(job.target_command_params(), Some(r#"{"delay":5}"#.to_string()));
    }

    #[test]
    fn test_cron_job_config_extraction_object_params() {
        let job = CronJob {
            id: "job-003".to_string(),
            name: "Test Object Params".to_string(),
            description: None,
            job_type: "device_command".to_string(),
            cron_expression: "*/5 * * * *".to_string(),
            config: r#"{"device_id":"dev-456","command_name":"set_config","params":{"delay":5,"mode":"fast"}}"#.to_string(),
            timeout_seconds: 300,
            max_retries: 3,
            is_enabled: true,
            is_running: false,
            last_run_at: None,
            last_run_status: None,
            last_run_error: None,
            next_run_at: None,
            run_count: 0,
            success_count: 0,
            fail_count: 0,
            created_at: "2026-04-18 08:00:00".to_string(),
            updated_at: "2026-04-18 08:00:00".to_string(),
            created_by: None,
        };

        assert_eq!(job.target_device_id(), Some("dev-456".to_string()));
        assert_eq!(job.target_command_name(), Some("set_config".to_string()));
        assert_eq!(
            job.target_command_params(),
            Some(r#"{"delay":5,"mode":"fast"}"#.to_string())
        );
    }

    #[test]
    fn test_cron_job_config_extraction_invalid_json() {
        let job = CronJob {
            id: "job-004".to_string(),
            name: "Invalid Config".to_string(),
            description: None,
            job_type: "device_command".to_string(),
            cron_expression: "*/5 * * * *".to_string(),
            config: "not-json-at-all".to_string(),
            timeout_seconds: 300,
            max_retries: 3,
            is_enabled: true,
            is_running: false,
            last_run_at: None,
            last_run_status: None,
            last_run_error: None,
            next_run_at: None,
            run_count: 0,
            success_count: 0,
            fail_count: 0,
            created_at: "2026-04-18 08:00:00".to_string(),
            updated_at: "2026-04-18 08:00:00".to_string(),
            created_by: None,
        };

        assert_eq!(job.target_device_id(), None);
        assert_eq!(job.target_command_name(), None);
        assert_eq!(job.target_command_params(), None);
    }

    #[test]
    fn test_cron_job_config_extraction_wrong_type() {
        let job = CronJob {
            id: "job-002".to_string(),
            name: "Test HTTP Job".to_string(),
            description: None,
            job_type: "http".to_string(),
            cron_expression: "0 * * * *".to_string(),
            config: r#"{"url":"http://example.com"}"#.to_string(),
            timeout_seconds: 60,
            max_retries: 0,
            is_enabled: true,
            is_running: false,
            last_run_at: None,
            last_run_status: None,
            last_run_error: None,
            next_run_at: None,
            run_count: 0,
            success_count: 0,
            fail_count: 0,
            created_at: "2026-04-18 08:00:00".to_string(),
            updated_at: "2026-04-18 08:00:00".to_string(),
            created_by: None,
        };

        assert_eq!(job.target_device_id(), None);
        assert_eq!(job.target_command_name(), None);
        assert_eq!(job.target_command_params(), None);
    }

    #[test]
    fn test_cron_job_query_default() {
        let query = CronJobQuery::default();
        assert_eq!(query.page, None);
        assert_eq!(query.page_size, None);
    }

    #[test]
    fn test_create_cron_job_request() {
        let req = CreateCronJobRequest {
            name: "Test Job".to_string(),
            description: Some("desc".to_string()),
            job_type: "device_command".to_string(),
            cron_expression: "*/5 * * * *".to_string(),
            config: "{}".to_string(),
            workspace_id: "ws-test".to_string(),
            timeout_seconds: Some(300),
            max_retries: Some(3),
        };
        assert_eq!(req.name, "Test Job");
        assert_eq!(req.timeout_seconds, Some(300));
    }

    #[test]
    fn test_update_cron_job_request() {
        let req = UpdateCronJobRequest {
            name: Some("Updated".to_string()),
            description: None,
            job_type: None,
            cron_expression: Some("0 0 * * *".to_string()),
            config: None,
            timeout_seconds: Some(600),
            max_retries: None,
            is_enabled: Some(false),
        };
        assert_eq!(req.name, Some("Updated".to_string()));
        assert_eq!(req.is_enabled, Some(false));
    }

    #[test]
    fn test_cron_run_fields() {
        let run = CronRun {
            id: "run-001".to_string(),
            job_id: "job-001".to_string(),
            started_at: "2026-04-18 08:00:00".to_string(),
            ended_at: Some("2026-04-18 08:00:05".to_string()),
            duration_ms: Some(5000),
            status: "success".to_string(),
            output: Some("OK".to_string()),
            error_message: None,
            trigger_type: "scheduled".to_string(),
            triggered_by: None,
            created_at: "2026-04-18 08:00:00".to_string(),
        };
        assert_eq!(run.status, "success");
        assert_eq!(run.duration_ms, Some(5000));
    }

    #[test]
    fn test_cron_run_query_default() {
        let query = CronRunQuery::default();
        assert_eq!(query.job_id, None);
        assert_eq!(query.status, None);
    }

    #[test]
    fn test_cron_statistics() {
        let stats = CronStatistics {
            total_jobs: 10,
            enabled_jobs: 8,
            disabled_jobs: 2,
            running_jobs: 1,
            total_runs: 100,
            success_runs: 95,
            failed_runs: 5,
            avg_duration_ms: 1200,
        };
        assert_eq!(stats.total_jobs, 10);
        assert_eq!(stats.success_runs, 95);
        assert_eq!(stats.avg_duration_ms, 1200);
    }
}
