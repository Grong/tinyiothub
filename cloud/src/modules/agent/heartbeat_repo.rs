//! SQLite implementations of AI crate heartbeat repository traits.

use async_trait::async_trait;
use sqlx::SqlitePool;
use tinyiothub_ai::heartbeat::{
    repo::{HeartbeatTaskRepository, RepoError},
    types::{HeartbeatResult, HeartbeatStatus, HeartbeatTask, NewHeartbeatTask},
};

/// DB row struct with sqlx::FromRow — maps to domain HeartbeatTask.
#[derive(Debug, Clone, sqlx::FromRow)]
struct HeartbeatTaskRow {
    pub id: i64,
    pub workspace_id: String,
    pub priority: String,
    pub text: String,
    pub paused: bool,
    pub version: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<HeartbeatTaskRow> for HeartbeatTask {
    fn from(r: HeartbeatTaskRow) -> Self {
        Self {
            id: r.id,
            workspace_id: r.workspace_id,
            priority: r.priority,
            text: r.text,
            paused: r.paused,
            version: r.version,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

pub struct SqliteHeartbeatTaskRepository {
    pool: SqlitePool,
}

impl SqliteHeartbeatTaskRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HeartbeatTaskRepository for SqliteHeartbeatTaskRepository {
    async fn list_by_workspace(&self, workspace_id: &str) -> Result<Vec<HeartbeatTask>, RepoError> {
        let rows = sqlx::query_as::<_, HeartbeatTaskRow>(
            "SELECT id, workspace_id, priority, text, paused, version,
                    created_at, updated_at
             FROM heartbeat_tasks WHERE workspace_id = ? ORDER BY priority DESC, id ASC",
        )
        .bind(workspace_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::Database(e.to_string()))?;

        Ok(rows.into_iter().map(HeartbeatTask::from).collect())
    }

    async fn upsert(
        &self,
        workspace_id: &str,
        task: &HeartbeatTask,
        expected_version: i64,
    ) -> Result<bool, RepoError> {
        let result = sqlx::query(
            "UPDATE heartbeat_tasks
             SET priority = ?, text = ?, paused = ?, version = version + 1,
                 updated_at = CURRENT_TIMESTAMP
             WHERE workspace_id = ? AND id = ? AND version = ?",
        )
        .bind(&task.priority)
        .bind(&task.text)
        .bind(task.paused)
        .bind(workspace_id)
        .bind(task.id)
        .bind(expected_version)
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::Database(e.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    async fn insert(
        &self,
        workspace_id: &str,
        priority: &str,
        text: &str,
    ) -> Result<HeartbeatTask, RepoError> {
        let row = sqlx::query_as::<_, HeartbeatTaskRow>(
            "INSERT INTO heartbeat_tasks (workspace_id, priority, text)
             VALUES (?, ?, ?)
             RETURNING id, workspace_id, priority, text, paused, version,
                       created_at, updated_at",
        )
        .bind(workspace_id)
        .bind(priority)
        .bind(text)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RepoError::Database(e.to_string()))?;

        Ok(HeartbeatTask::from(row))
    }

    async fn set_paused(
        &self,
        workspace_id: &str,
        task_id: i64,
        paused: bool,
    ) -> Result<(), RepoError> {
        sqlx::query(
            "UPDATE heartbeat_tasks SET paused = ?, updated_at = CURRENT_TIMESTAMP
             WHERE workspace_id = ? AND id = ?",
        )
        .bind(paused)
        .bind(workspace_id)
        .bind(task_id)
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::Database(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, workspace_id: &str, task_id: i64) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM heartbeat_tasks WHERE workspace_id = ? AND id = ?")
            .bind(workspace_id)
            .bind(task_id)
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::Database(e.to_string()))?;
        Ok(())
    }

    async fn replace_all(&self, workspace_id: &str, tasks: &[NewHeartbeatTask]) -> Result<(), RepoError> {
        let mut tx = self.pool.begin().await.map_err(|e| RepoError::Database(e.to_string()))?;
        sqlx::query("DELETE FROM heartbeat_tasks WHERE workspace_id = ?")
            .bind(workspace_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| RepoError::Database(e.to_string()))?;
        for task in tasks {
            sqlx::query(
                "INSERT INTO heartbeat_tasks (workspace_id, priority, text, paused) VALUES (?, ?, ?, ?)",
            )
            .bind(workspace_id)
            .bind(&task.priority)
            .bind(&task.text)
            .bind(task.paused)
            .execute(&mut *tx)
            .await
            .map_err(|e| RepoError::Database(e.to_string()))?;
        }
        tx.commit().await.map_err(|e| RepoError::Database(e.to_string()))?;
        Ok(())
    }

    async fn load_trust_config(
        &self,
        workspace_id: &str,
    ) -> Result<Option<tinyiothub_ai::tool::trust::TrustConfig>, RepoError> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT heartbeat_trust_config FROM workspaces WHERE id = ?")
                .bind(workspace_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| RepoError::Database(e.to_string()))?;
        Ok(row.and_then(|(json,)| {
            if json.trim().is_empty() {
                None
            } else {
                Some(tinyiothub_ai::tool::trust::TrustConfig::from_db_json(Some(&json)))
            }
        }))
    }

    async fn insert_result(
        &self,
        workspace_id: &str,
        result: &HeartbeatResult,
    ) -> Result<(), RepoError> {
        // Row format must match the readers in workspace/handler/heartbeat.rs:
        // one summary|error row per tick, one auto_executed row per action,
        // one proposal row per proposal, all sharing one created_at so the
        // log query can group details under their tick.
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let agent_id = format!("__heartbeat__:{workspace_id}");
        let mut tx = self.pool.begin().await.map_err(|e| RepoError::Database(e.to_string()))?;

        let (action_type, content) = if result.status == HeartbeatStatus::Error {
            let message = result.error.as_deref().unwrap_or(&result.summary);
            ("error", serde_json::json!({"taskCount": result.task_count, "error": message}))
        } else {
            ("summary", serde_json::json!({"taskCount": result.task_count, "result": result.summary}))
        };
        insert_action_row(&mut tx, workspace_id, &agent_id, action_type, &content.to_string(), &now).await?;

        for action in &result.executed_actions {
            let content = serde_json::json!({
                "tool": action.tool_name,
                "deviceId": action.device_id,
                "summary": action.details,
            });
            insert_action_row(&mut tx, workspace_id, &agent_id, "auto_executed", &content.to_string(), &now)
                .await?;
        }

        for proposal in &result.proposals {
            let proposal_id = if proposal.id.is_empty() {
                uuid::Uuid::new_v4().to_string()
            } else {
                proposal.id.clone()
            };
            let content = serde_json::json!({
                "proposalId": proposal_id,
                "status": proposal.status.to_string(),
                "toolName": proposal.tool_name,
                "deviceId": proposal.device_id,
                "deviceName": "",
                "summary": proposal.summary,
                "reason": proposal.reason,
                "risk": proposal.risk,
            });
            insert_action_row(&mut tx, workspace_id, &agent_id, "proposal", &content.to_string(), &now)
                .await?;
        }

        tx.commit().await.map_err(|e| RepoError::Database(e.to_string()))?;
        Ok(())
    }
}

async fn insert_action_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
    agent_id: &str,
    action_type: &str,
    content: &str,
    created_at: &str,
) -> Result<(), RepoError> {
    sqlx::query(
        "INSERT INTO agent_actions (id, workspace_id, agent_id, event_type, action_type, content, created_at)
         VALUES (?, ?, ?, 'heartbeat', ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(workspace_id)
    .bind(agent_id)
    .bind(action_type)
    .bind(content)
    .bind(created_at)
    .execute(&mut **tx)
    .await
    .map_err(|e| RepoError::Database(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use tinyiothub_ai::heartbeat::types::{ExecutedAction, HeartbeatStatus, NewHeartbeatTask};
    use tinyiothub_ai::proposal::{Proposal, ProposalStatus};

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .expect("create in-memory sqlite");
        for migration in [
            include_str!("../../../migrations/20260615120000_agent_actions.sql"),
            include_str!("../../../migrations/20260629000001_create_heartbeat_tasks.sql"),
        ] {
            for stmt in migration.split(';') {
                let stmt = stmt.trim();
                if !stmt.is_empty() {
                    sqlx::query(stmt).execute(&pool).await.expect("apply migration");
                }
            }
        }
        pool
    }

    #[tokio::test]
    async fn replace_all_replaces_task_set() {
        let pool = test_pool().await;
        let repo = SqliteHeartbeatTaskRepository::new(pool.clone());

        let initial = vec![
            NewHeartbeatTask { priority: "high".into(), text: "task A".into(), paused: false },
            NewHeartbeatTask { priority: "low".into(), text: "task B".into(), paused: true },
        ];
        repo.replace_all("ws_1", &initial).await.expect("seed tasks");

        let tasks = repo.list_by_workspace("ws_1").await.expect("list");
        assert_eq!(tasks.len(), 2);
        assert!(tasks.iter().any(|t| t.text == "task A" && !t.paused));
        assert!(tasks.iter().any(|t| t.text == "task B" && t.paused));

        let replacement =
            vec![NewHeartbeatTask { priority: "medium".into(), text: "task C".into(), paused: false }];
        repo.replace_all("ws_1", &replacement).await.expect("replace");

        let tasks = repo.list_by_workspace("ws_1").await.expect("list after replace");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].text, "task C");
        assert_eq!(tasks[0].priority, "medium");

        // Empty replacement is allowed and means "no tasks"
        repo.replace_all("ws_1", &[]).await.expect("clear");
        assert!(repo.list_by_workspace("ws_1").await.expect("list empty").is_empty());
    }

    fn sample_result() -> HeartbeatResult {
        HeartbeatResult {
            workspace_id: "ws_1".to_string(),
            status: HeartbeatStatus::Partial,
            summary: "checked 2 devices".to_string(),
            task_count: 3,
            executed_actions: vec![ExecutedAction {
                tool_name: "device_control".to_string(),
                device_id: Some("dev_1".to_string()),
                success: true,
                details: "restarted".to_string(),
            }],
            proposals: vec![Proposal {
                id: String::new(),
                workspace_id: "ws_1".to_string(),
                agent_id: String::new(),
                tool_name: "firmware_update".to_string(),
                device_id: Some("dev_2".to_string()),
                summary: "update firmware".to_string(),
                reason: "security patch".to_string(),
                risk: "high".to_string(),
                parameters: None,
                created_at: String::new(),
                status: ProposalStatus::Pending,
            }],
            error: None,
        }
    }

    #[tokio::test]
    async fn insert_result_roundtrips_through_log_and_approval_queries() {
        let pool = test_pool().await;
        let repo = SqliteHeartbeatTaskRepository::new(pool.clone());

        repo.insert_result("ws_1", &sample_result())
            .await
            .expect("insert_result must succeed against the real schema");

        // Same query as workspace/handler/heartbeat.rs get_logs
        let rows: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT action_type, content, created_at FROM agent_actions \
             WHERE workspace_id = ? AND event_type = 'heartbeat' \
             ORDER BY created_at DESC LIMIT 200",
        )
        .bind("ws_1")
        .fetch_all(&pool)
        .await
        .expect("log query");

        let mut summary_rows = 0;
        let mut auto_rows = 0;
        let mut proposal_rows = 0;
        let mut proposal_id = String::new();
        for (action_type, content, _created_at) in &rows {
            let parsed: serde_json::Value = serde_json::from_str(content).expect("content is JSON");
            match action_type.as_str() {
                "summary" => {
                    summary_rows += 1;
                    assert_eq!(parsed["taskCount"], 3);
                    assert_eq!(parsed["result"], "checked 2 devices");
                }
                "auto_executed" => {
                    auto_rows += 1;
                    assert_eq!(parsed["tool"], "device_control");
                    assert_eq!(parsed["deviceId"], "dev_1");
                    assert_eq!(parsed["summary"], "restarted");
                }
                "proposal" => {
                    proposal_rows += 1;
                    proposal_id = parsed["proposalId"].as_str().unwrap_or("").to_string();
                    assert!(!proposal_id.is_empty(), "proposalId must be server-generated");
                    assert_eq!(parsed["status"], "pending");
                    assert_eq!(parsed["toolName"], "firmware_update");
                    assert_eq!(parsed["risk"], "high");
                }
                other => panic!("unexpected action_type: {other}"),
            }
        }
        assert_eq!((summary_rows, auto_rows, proposal_rows), (1, 1, 1));

        // Same query as update_proposal_status
        let found: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM agent_actions \
             WHERE workspace_id = ? AND action_type = 'proposal' \
             AND json_extract(content, '$.proposalId') = ?",
        )
        .bind("ws_1")
        .bind(&proposal_id)
        .fetch_optional(&pool)
        .await
        .expect("approval lookup query");
        assert!(found.is_some(), "approval query must find the proposal by proposalId");
    }

    #[tokio::test]
    async fn insert_result_error_status_writes_error_row() {        let pool = test_pool().await;
        let repo = SqliteHeartbeatTaskRepository::new(pool.clone());

        let result = HeartbeatResult {
            workspace_id: "ws_1".to_string(),
            status: HeartbeatStatus::Error,
            summary: String::new(),
            task_count: 0,
            executed_actions: vec![],
            proposals: vec![],
            error: Some("llm timeout".to_string()),
        };
        repo.insert_result("ws_1", &result).await.expect("insert error result");

        let (action_type, content): (String, String) = sqlx::query_as(
            "SELECT action_type, content FROM agent_actions WHERE workspace_id = 'ws_1'",
        )
        .fetch_one(&pool)
        .await
        .expect("one row");
        assert_eq!(action_type, "error");
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["error"], "llm timeout");
    }

    async fn create_workspaces_table(pool: &SqlitePool) {
        for stmt in [
            "CREATE TABLE workspaces (id TEXT PRIMARY KEY, name TEXT NOT NULL, description TEXT, tenant_id TEXT NOT NULL, agent_id TEXT, agent_config TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
            "ALTER TABLE workspaces ADD COLUMN heartbeat_trust_config TEXT NOT NULL DEFAULT ''",
        ] {
            sqlx::query(stmt).execute(pool).await.expect("create workspaces table");
        }
    }

    #[tokio::test]
    async fn load_trust_config_reads_workspace_column() {
        let pool = test_pool().await;
        create_workspaces_table(&pool).await;
        let repo = SqliteHeartbeatTaskRepository::new(pool.clone());

        let config = tinyiothub_ai::tool::trust::TrustConfig {
            trust_level: tinyiothub_ai::tool::trust::TrustLevel::FullAuto,
            ..Default::default()
        };
        sqlx::query(
            "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at, heartbeat_trust_config)
             VALUES ('ws_full', 'ws', 't1', 'now', 'now', ?)",
        )
        .bind(config.to_db_json())
        .execute(&pool)
        .await
        .expect("insert workspace");
        sqlx::query(
            "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at)
             VALUES ('ws_empty', 'ws', 't1', 'now', 'now')",
        )
        .execute(&pool)
        .await
        .expect("insert workspace with default column");

        let loaded = repo.load_trust_config("ws_full").await.expect("load");
        assert_eq!(
            loaded.map(|c| c.trust_level),
            Some(tinyiothub_ai::tool::trust::TrustLevel::FullAuto)
        );

        // Empty column and unknown workspace both mean "no persisted config".
        assert!(repo.load_trust_config("ws_empty").await.expect("load").is_none());
        assert!(repo.load_trust_config("ws_missing").await.expect("load").is_none());
    }
}
