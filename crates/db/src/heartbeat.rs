//! Heartbeat 持久化：巡检任务/结果/信任配置（P-集中化 E6b，自 agent crate 迁入）。
//!
//! 值类型归位 core（Task 1），本模块经 glob re-export 组织 db 内部路径；
//! WorkspaceHeartbeatConfig（DB 行序列化格式）与全部 SQL 留在本文件，
//! 经 `Db` 门面委托暴露（Task 9）。

// 领域值类型住 core（tinyiothub_core::heartbeat）；此处 re-export 仅为 db
// 内部模块组织，非跨 crate 摆渡层。
pub use tinyiothub_core::heartbeat::*;

/// Per-workspace heartbeat settings, persisted as JSON on the workspace row.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceHeartbeatConfig {
    pub enabled: bool,
    pub interval_minutes: u32,
}

impl WorkspaceHeartbeatConfig {
    pub fn validated(enabled: bool, interval_minutes: u32) -> Result<Self, String> {
        if interval_minutes < MIN_HEARTBEAT_INTERVAL_MINUTES {
            return Err(format!(
                "interval_minutes must be >= {}",
                MIN_HEARTBEAT_INTERVAL_MINUTES
            ));
        }
        Ok(Self {
            enabled,
            interval_minutes,
        })
    }

    pub fn from_db_json(json: Option<&str>) -> Option<Self> {
        let json = json?.trim();
        if json.is_empty() {
            return None;
        }
        serde_json::from_str(json).ok()
    }

    pub fn to_db_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    #[error("Database error: {0}")]
    Database(String),
    #[error("Not found")]
    NotFound,
    #[error("Serialization error: {0}")]
    Serialization(String),
}

// ──────────────────────────────────────────────
// 持久化函数（pool 参数）+ Db 门面委托
// ──────────────────────────────────────────────

use sqlx::SqlitePool;

use crate::database::Db;

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

pub(crate) async fn list_by_workspace(pool: &SqlitePool, workspace_id: &str) -> Result<Vec<HeartbeatTask>, RepoError> {
    let rows = sqlx::query_as::<_, HeartbeatTaskRow>(
        "SELECT id, workspace_id, priority, text, paused, version,
                    created_at, updated_at
             FROM heartbeat_tasks WHERE workspace_id = ? ORDER BY priority DESC, id ASC",
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await
    .map_err(|e| RepoError::Database(e.to_string()))?;

    Ok(rows.into_iter().map(HeartbeatTask::from).collect())
}

pub(crate) async fn upsert(
    pool: &SqlitePool,
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
    .execute(pool)
    .await
    .map_err(|e| RepoError::Database(e.to_string()))?;

    Ok(result.rows_affected() > 0)
}

pub(crate) async fn insert(
    pool: &SqlitePool,
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
    .fetch_one(pool)
    .await
    .map_err(|e| RepoError::Database(e.to_string()))?;

    Ok(HeartbeatTask::from(row))
}

pub(crate) async fn set_paused(
    pool: &SqlitePool,
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
    .execute(pool)
    .await
    .map_err(|e| RepoError::Database(e.to_string()))?;
    Ok(())
}

pub(crate) async fn delete(pool: &SqlitePool, workspace_id: &str, task_id: i64) -> Result<(), RepoError> {
    sqlx::query("DELETE FROM heartbeat_tasks WHERE workspace_id = ? AND id = ?")
        .bind(workspace_id)
        .bind(task_id)
        .execute(pool)
        .await
        .map_err(|e| RepoError::Database(e.to_string()))?;
    Ok(())
}

pub(crate) async fn replace_all(
    pool: &SqlitePool,
    workspace_id: &str,
    tasks: &[NewHeartbeatTask],
) -> Result<(), RepoError> {
    let mut tx = pool.begin().await.map_err(|e| RepoError::Database(e.to_string()))?;
    sqlx::query("DELETE FROM heartbeat_tasks WHERE workspace_id = ?")
        .bind(workspace_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| RepoError::Database(e.to_string()))?;
    for task in tasks {
        sqlx::query("INSERT INTO heartbeat_tasks (workspace_id, priority, text, paused) VALUES (?, ?, ?, ?)")
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

pub(crate) async fn load_trust_config(
    pool: &SqlitePool,
    workspace_id: &str,
) -> Result<Option<crate::heartbeat::TrustConfig>, RepoError> {
    let row: Option<(String,)> = sqlx::query_as("SELECT heartbeat_trust_config FROM workspaces WHERE id = ?")
        .bind(workspace_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| RepoError::Database(e.to_string()))?;
    Ok(row.and_then(|(json,)| {
        if json.trim().is_empty() {
            None
        } else {
            Some(crate::heartbeat::TrustConfig::from_db_json(Some(&json)))
        }
    }))
}

/// fencing 时间戳的统一格式化（review M1）：fencing 契约依赖所有写入方
/// 产出字节一致、字典序可比的 RFC3339-millis-Z 串——格式只能有一个家，
/// 任何调用点漂移都会静默打破 fencing。
pub fn fencing_timestamp(ts: chrono::DateTime<chrono::Utc>) -> String {
    ts.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub(crate) async fn save_trust_config(
    pool: &SqlitePool,
    workspace_id: &str,
    config: &crate::heartbeat::TrustConfig,
) -> Result<(), RepoError> {
    // CEO review T2：同步维护 fencing 时间戳——handler 先写路径（D11-⑤）是
    // 权威写，事件路径以 occurred_at 与该列比较，旧事件无法覆盖本写。
    // 格式统一走 fencing_timestamp（M1：单一事实源）。
    sqlx::query("UPDATE workspaces SET heartbeat_trust_config = ?, heartbeat_trust_config_updated_at = ? WHERE id = ?")
        .bind(config.to_db_json())
        .bind(fencing_timestamp(chrono::Utc::now()))
        .bind(workspace_id)
        .execute(pool)
        .await
        .map_err(|e| RepoError::Database(e.to_string()))?;
    Ok(())
}

/// fencing upsert（CEO review T2）：仅当事件的 occurred_at 不早于已应用
/// 时间戳时写入；乱序/回放的旧事件无法覆盖新配置。返回是否实际写入
/// （false = 被 fencing 拦截或工作区不存在）。
pub(crate) async fn save_trust_config_fenced(
    pool: &SqlitePool,
    workspace_id: &str,
    config: &crate::heartbeat::TrustConfig,
    occurred_at: &str,
) -> Result<bool, RepoError> {
    let result = sqlx::query(
        "UPDATE workspaces SET heartbeat_trust_config = ?, heartbeat_trust_config_updated_at = ?
         WHERE id = ? AND (heartbeat_trust_config_updated_at IS NULL OR heartbeat_trust_config_updated_at <= ?)",
    )
    .bind(config.to_db_json())
    .bind(occurred_at)
    .bind(workspace_id)
    .bind(occurred_at)
    .execute(pool)
    .await
    .map_err(|e| RepoError::Database(e.to_string()))?;
    Ok(result.rows_affected() > 0)
}

pub(crate) async fn load_heartbeat_config(
    pool: &SqlitePool,
    workspace_id: &str,
) -> Result<Option<crate::heartbeat::WorkspaceHeartbeatConfig>, RepoError> {
    let row: Option<(String,)> = sqlx::query_as("SELECT heartbeat_config FROM workspaces WHERE id = ?")
        .bind(workspace_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| RepoError::Database(e.to_string()))?;
    Ok(row.and_then(|(json,)| crate::heartbeat::WorkspaceHeartbeatConfig::from_db_json(Some(&json))))
}

pub(crate) async fn save_heartbeat_config(
    pool: &SqlitePool,
    workspace_id: &str,
    config: &crate::heartbeat::WorkspaceHeartbeatConfig,
) -> Result<(), RepoError> {
    sqlx::query("UPDATE workspaces SET heartbeat_config = ? WHERE id = ?")
        .bind(config.to_db_json())
        .bind(workspace_id)
        .execute(pool)
        .await
        .map_err(|e| RepoError::Database(e.to_string()))?;
    Ok(())
}

pub(crate) async fn insert_result(
    pool: &SqlitePool,
    workspace_id: &str,
    result: &HeartbeatResult,
) -> Result<(), RepoError> {
    // Row format must match the readers in workspace/handler/heartbeat.rs:
    // one summary|error row per tick, one auto_executed row per action,
    // one proposal row per proposal, all sharing one created_at so the
    // log query can group details under their tick.
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let agent_id = format!("__heartbeat__:{workspace_id}");
    let mut tx = pool.begin().await.map_err(|e| RepoError::Database(e.to_string()))?;

    let (action_type, content) = if result.status == HeartbeatStatus::Error {
        let message = result.error.as_deref().unwrap_or(&result.summary);
        (
            "error",
            serde_json::json!({"taskCount": result.task_count, "error": message}),
        )
    } else {
        (
            "summary",
            serde_json::json!({"taskCount": result.task_count, "result": result.summary}),
        )
    };
    insert_action_row(
        &mut tx,
        workspace_id,
        &agent_id,
        action_type,
        &content.to_string(),
        &now,
    )
    .await?;

    for action in &result.executed_actions {
        let content = serde_json::json!({
            "tool": action.tool_name,
            "deviceId": action.device_id,
            "summary": action.details,
        });
        insert_action_row(
            &mut tx,
            workspace_id,
            &agent_id,
            "auto_executed",
            &content.to_string(),
            &now,
        )
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
            "parameters": proposal.parameters,
        });
        insert_action_row(&mut tx, workspace_id, &agent_id, "proposal", &content.to_string(), &now).await?;
    }

    tx.commit().await.map_err(|e| RepoError::Database(e.to_string()))?;
    Ok(())
}

// ──────────────────────────────────────────────
// Db 门面委托
// ──────────────────────────────────────────────

impl Db {
    /// 列出工作区的巡检任务（priority 降序、id 升序）。
    pub async fn list_heartbeat_tasks(&self, workspace_id: &str) -> Result<Vec<HeartbeatTask>, RepoError> {
        list_by_workspace(self.pool(), workspace_id).await
    }

    /// 乐观锁更新巡检任务（version 不匹配返回 false）。
    pub async fn upsert_heartbeat_task(
        &self,
        workspace_id: &str,
        task: &HeartbeatTask,
        expected_version: i64,
    ) -> Result<bool, RepoError> {
        upsert(self.pool(), workspace_id, task, expected_version).await
    }

    /// 新增巡检任务，返回完整行。
    pub async fn insert_heartbeat_task(
        &self,
        workspace_id: &str,
        priority: &str,
        text: &str,
    ) -> Result<HeartbeatTask, RepoError> {
        insert(self.pool(), workspace_id, priority, text).await
    }

    /// 设置巡检任务暂停标志。
    pub async fn set_heartbeat_task_paused(
        &self,
        workspace_id: &str,
        task_id: i64,
        paused: bool,
    ) -> Result<(), RepoError> {
        set_paused(self.pool(), workspace_id, task_id, paused).await
    }

    /// 删除巡检任务。
    pub async fn delete_heartbeat_task(&self, workspace_id: &str, task_id: i64) -> Result<(), RepoError> {
        delete(self.pool(), workspace_id, task_id).await
    }

    /// 事务化整体替换工作区的巡检任务集。
    pub async fn replace_heartbeat_tasks(
        &self,
        workspace_id: &str,
        tasks: &[NewHeartbeatTask],
    ) -> Result<(), RepoError> {
        replace_all(self.pool(), workspace_id, tasks).await
    }

    /// 读取工作区心跳信任配置（空串/缺行视为 None）。
    pub async fn load_heartbeat_trust_config(
        &self,
        workspace_id: &str,
    ) -> Result<Option<crate::heartbeat::TrustConfig>, RepoError> {
        load_trust_config(self.pool(), workspace_id).await
    }

    /// 幂等写入工作区心跳信任配置。
    pub async fn save_heartbeat_trust_config(
        &self,
        workspace_id: &str,
        config: &crate::heartbeat::TrustConfig,
    ) -> Result<(), RepoError> {
        save_trust_config(self.pool(), workspace_id, config).await
    }

    /// fencing 写入（CEO review T2）：旧事件（occurred_at 早于已应用
    /// 时间戳）不覆盖新配置；返回是否实际写入。
    pub async fn save_heartbeat_trust_config_fenced(
        &self,
        workspace_id: &str,
        config: &crate::heartbeat::TrustConfig,
        occurred_at: &str,
    ) -> Result<bool, RepoError> {
        save_trust_config_fenced(self.pool(), workspace_id, config, occurred_at).await
    }

    /// 读取工作区心跳开关/间隔配置（空串/缺行视为 None）。
    pub async fn load_heartbeat_config(
        &self,
        workspace_id: &str,
    ) -> Result<Option<crate::heartbeat::WorkspaceHeartbeatConfig>, RepoError> {
        load_heartbeat_config(self.pool(), workspace_id).await
    }

    /// 写入工作区心跳开关/间隔配置。
    pub async fn save_heartbeat_config(
        &self,
        workspace_id: &str,
        config: &crate::heartbeat::WorkspaceHeartbeatConfig,
    ) -> Result<(), RepoError> {
        save_heartbeat_config(self.pool(), workspace_id, config).await
    }

    /// 一次心跳 tick 的结果落库（summary/error + auto_executed + proposal
    /// 行，同事务共享 created_at）。
    pub async fn insert_heartbeat_result(&self, workspace_id: &str, result: &HeartbeatResult) -> Result<(), RepoError> {
        insert_result(self.pool(), workspace_id, result).await
    }
}

pub(crate) async fn insert_action_row(
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
    use crate::heartbeat::{ExecutedAction, HeartbeatStatus, NewHeartbeatTask};
    use crate::policy::{Proposal, ProposalStatus};

    use super::*;

    pub async fn test_pool() -> SqlitePool {
        crate::test_helpers::test_pool().await
    }

    #[test]
    fn workspace_config_roundtrips_json() {
        let cfg = WorkspaceHeartbeatConfig {
            enabled: true,
            interval_minutes: 30,
        };
        let json = cfg.to_db_json();
        let loaded = WorkspaceHeartbeatConfig::from_db_json(Some(&json)).expect("parse");
        assert_eq!(loaded.interval_minutes, 30);
        assert!(loaded.enabled);
    }

    #[test]
    fn workspace_config_empty_is_none() {
        assert!(WorkspaceHeartbeatConfig::from_db_json(Some("")).is_none());
        assert!(WorkspaceHeartbeatConfig::from_db_json(Some("  ")).is_none());
        assert!(WorkspaceHeartbeatConfig::from_db_json(None).is_none());
    }

    #[test]
    fn workspace_config_rejects_sub_minimum_interval() {
        use tinyiothub_core::heartbeat::MIN_HEARTBEAT_INTERVAL_MINUTES;
        assert!(WorkspaceHeartbeatConfig::validated(true, MIN_HEARTBEAT_INTERVAL_MINUTES - 1).is_err());
        assert!(WorkspaceHeartbeatConfig::validated(true, MIN_HEARTBEAT_INTERVAL_MINUTES).is_ok());
    }

    #[tokio::test]
    pub async fn replace_all_replaces_task_set() {
        let pool = test_pool().await;
        let db = Db::new(pool.clone());

        let initial = vec![
            NewHeartbeatTask {
                priority: "high".into(),
                text: "task A".into(),
                paused: false,
            },
            NewHeartbeatTask {
                priority: "low".into(),
                text: "task B".into(),
                paused: true,
            },
        ];
        db.replace_heartbeat_tasks("ws_1", &initial).await.expect("seed tasks");

        let tasks = db.list_heartbeat_tasks("ws_1").await.expect("list");
        assert_eq!(tasks.len(), 2);
        assert!(tasks.iter().any(|t| t.text == "task A" && !t.paused));
        assert!(tasks.iter().any(|t| t.text == "task B" && t.paused));

        let replacement = vec![NewHeartbeatTask {
            priority: "medium".into(),
            text: "task C".into(),
            paused: false,
        }];
        db.replace_heartbeat_tasks("ws_1", &replacement).await.expect("replace");

        let tasks = db.list_heartbeat_tasks("ws_1").await.expect("list after replace");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].text, "task C");
        assert_eq!(tasks[0].priority, "medium");

        // Empty replacement is allowed and means "no tasks"
        db.replace_heartbeat_tasks("ws_1", &[]).await.expect("clear");
        assert!(db.list_heartbeat_tasks("ws_1").await.expect("list empty").is_empty());
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
    pub async fn insert_result_roundtrips_through_log_and_approval_queries() {
        let pool = test_pool().await;
        let db = Db::new(pool.clone());

        db.insert_heartbeat_result("ws_1", &sample_result())
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
    pub async fn insert_result_persists_proposal_parameters() {
        // Approve-and-execute needs the tool arguments back out of the DB.
        let pool = test_pool().await;
        let db = Db::new(pool.clone());

        let mut result = sample_result();
        result.proposals[0].parameters = Some(serde_json::json!({"device_id": "dev_2", "version": "1.2.3"}));
        db.insert_heartbeat_result("ws_1", &result)
            .await
            .expect("insert_result");

        let (content,): (String,) = sqlx::query_as(
            "SELECT content FROM agent_actions WHERE workspace_id = 'ws_1' AND action_type = 'proposal'",
        )
        .fetch_one(&pool)
        .await
        .expect("proposal row");
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["parameters"]["version"], "1.2.3");
    }

    #[tokio::test]
    pub async fn insert_result_error_status_writes_error_row() {
        let pool = test_pool().await;
        let db = Db::new(pool.clone());

        let result = HeartbeatResult {
            workspace_id: "ws_1".to_string(),
            status: HeartbeatStatus::Error,
            summary: String::new(),
            task_count: 0,
            executed_actions: vec![],
            proposals: vec![],
            error: Some("llm timeout".to_string()),
        };
        db.insert_heartbeat_result("ws_1", &result)
            .await
            .expect("insert error result");

        let (action_type, content): (String, String) =
            sqlx::query_as("SELECT action_type, content FROM agent_actions WHERE workspace_id = 'ws_1'")
                .fetch_one(&pool)
                .await
                .expect("one row");
        assert_eq!(action_type, "error");
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["error"], "llm timeout");
    }

    /// Seed the plan + tenant rows the baseline `workspaces.tenant_id` /
    /// `tenants.plan_id` foreign keys require.
    pub async fn seed_tenant(pool: &SqlitePool) {
        sqlx::query(
            "INSERT OR IGNORE INTO subscription_plans (id, name, display_name) VALUES ('plan_free','free','Free')",
        )
        .execute(pool)
        .await
        .expect("seed plan");
        sqlx::query("INSERT OR IGNORE INTO tenants (id, name, slug) VALUES ('t1','t','t1')")
            .execute(pool)
            .await
            .expect("seed tenant");
    }

    #[tokio::test]
    pub async fn heartbeat_config_save_and_load_roundtrip() {
        let pool = test_pool().await;
        seed_tenant(&pool).await;
        sqlx::query(
            "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) \
             VALUES ('ws_c', 'ws', 't1', 'now', 'now')",
        )
        .execute(&pool)
        .await
        .expect("insert workspace");
        let db = Db::new(pool.clone());

        assert!(db.load_heartbeat_config("ws_c").await.expect("load").is_none());

        let cfg = crate::heartbeat::WorkspaceHeartbeatConfig {
            enabled: true,
            interval_minutes: 30,
        };
        db.save_heartbeat_config("ws_c", &cfg).await.expect("save");
        let loaded = db
            .load_heartbeat_config("ws_c")
            .await
            .expect("load")
            .expect("persisted");
        assert_eq!(loaded.interval_minutes, 30);
        assert!(loaded.enabled);
    }

    #[tokio::test]
    pub async fn save_trust_config_persists_to_workspace_column() {
        let pool = test_pool().await;
        seed_tenant(&pool).await;
        sqlx::query(
            "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) \
             VALUES ('ws_t', 'ws', 't1', 'now', 'now')",
        )
        .execute(&pool)
        .await
        .expect("insert workspace");
        let db = Db::new(pool.clone());

        let cfg = crate::heartbeat::TrustConfig {
            trust_level: crate::heartbeat::TrustLevel::FullAuto,
            ..Default::default()
        };
        db.save_heartbeat_trust_config("ws_t", &cfg).await.expect("save");
        let loaded = db
            .load_heartbeat_trust_config("ws_t")
            .await
            .expect("load")
            .expect("persisted");
        assert_eq!(loaded.trust_level, crate::heartbeat::TrustLevel::FullAuto);
    }

    #[tokio::test]
    pub async fn load_trust_config_reads_workspace_column() {
        let pool = test_pool().await;
        seed_tenant(&pool).await;
        let db = Db::new(pool.clone());

        let config = crate::heartbeat::TrustConfig {
            trust_level: crate::heartbeat::TrustLevel::FullAuto,
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

        let loaded = db.load_heartbeat_trust_config("ws_full").await.expect("load");
        assert_eq!(
            loaded.map(|c| c.trust_level),
            Some(crate::heartbeat::TrustLevel::FullAuto)
        );

        // Empty column and unknown workspace both mean "no persisted config".
        assert!(
            db.load_heartbeat_trust_config("ws_empty")
                .await
                .expect("load")
                .is_none()
        );
        assert!(
            db.load_heartbeat_trust_config("ws_missing")
                .await
                .expect("load")
                .is_none()
        );
    }

    /// CEO review T2：fencing upsert——occurred_at 早于已应用时间戳的旧事件
    /// 不得覆盖新配置；不早于的才写入。handler 先写路径（save_trust_config）
    /// 同时维护 fencing 时间戳。
    #[tokio::test]
    pub async fn fenced_save_rejects_stale_occurred_at() {
        let pool = test_pool().await;
        seed_tenant(&pool).await;
        sqlx::query(
            "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at)
             VALUES ('ws_f', 'ws', 't1', 'now', 'now')",
        )
        .execute(&pool)
        .await
        .expect("insert workspace");
        let db = Db::new(pool.clone());

        let hardened = crate::heartbeat::TrustConfig {
            max_auto_actions_per_tick: 1,
            ..Default::default()
        };
        let relaxed = crate::heartbeat::TrustConfig {
            max_auto_actions_per_tick: 99,
            ..Default::default()
        };

        // 权威写（handler 路径）：写入加固配置并打上当前时间戳。
        db.save_heartbeat_trust_config("ws_f", &hardened)
            .await
            .expect("authoritative save");

        // 旧事件回放（occurred_at 早 1 小时）：必须被 fencing 拦截。
        let stale_ts =
            (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let applied = db
            .save_heartbeat_trust_config_fenced("ws_f", &relaxed, &stale_ts)
            .await
            .expect("fenced save");
        assert!(!applied, "stale event must be fenced");
        let current = db
            .load_heartbeat_trust_config("ws_f")
            .await
            .expect("load")
            .expect("config present");
        assert_eq!(current.max_auto_actions_per_tick, 1, "stale event must not overwrite");

        // 新事件（occurred_at 晚 1 小时）：允许写入。
        let fresh_ts =
            (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let applied = db
            .save_heartbeat_trust_config_fenced("ws_f", &relaxed, &fresh_ts)
            .await
            .expect("fenced save");
        assert!(applied, "newer event must apply");
        let current = db
            .load_heartbeat_trust_config("ws_f")
            .await
            .expect("load")
            .expect("config present");
        assert_eq!(current.max_auto_actions_per_tick, 99);
    }

    /// fencing 边界（testing specialist T4）：从未写过的行（updated_at IS
    /// NULL）首次 fenced 写入必须成功；不存在的工作区返回 Ok(false)。
    #[tokio::test]
    pub async fn fenced_save_first_write_and_missing_workspace() {
        let pool = test_pool().await;
        seed_tenant(&pool).await;
        sqlx::query(
            "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at)
             VALUES ('ws_n', 'ws', 't1', 'now', 'now')",
        )
        .execute(&pool)
        .await
        .expect("insert workspace");
        let db = Db::new(pool.clone());
        let cfg = crate::heartbeat::TrustConfig::default();
        let ts = fencing_timestamp(chrono::Utc::now());

        // IS NULL 分支：首次 fenced 写入成功。
        let applied = db
            .save_heartbeat_trust_config_fenced("ws_n", &cfg, &ts)
            .await
            .expect("first fenced write");
        assert!(applied, "first fenced write on NULL column must apply");
        assert!(db.load_heartbeat_trust_config("ws_n").await.expect("load").is_some());

        // 不存在的工作区：UPDATE 命中 0 行 → Ok(false)，不得报错。
        let applied = db
            .save_heartbeat_trust_config_fenced("ws_nonexistent", &cfg, &ts)
            .await
            .expect("fenced save on missing workspace");
        assert!(!applied);
    }
}
