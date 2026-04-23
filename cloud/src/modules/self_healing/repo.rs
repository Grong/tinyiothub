// Self-healing repository
// Migrated from domain/self_healing/repository.rs

use std::sync::Arc;

use sqlx::Row;

use crate::shared::persistence::Database;

use super::types::{ExecutionResult, HealingExecution, RecoveryActionType, SeverityLevel};

/// Repository for storing and retrieving healing executions
pub struct HealingExecutionRepository {
    db: Arc<Database>,
}

impl HealingExecutionRepository {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    pub fn database(&self) -> &Arc<Database> {
        &self.db
    }

    pub async fn save(&self, execution: &HealingExecution) -> std::result::Result<(), sqlx::Error> {
        let sql = r#"
            INSERT INTO healing_executions (
                id, tenant_id, timestamp, level, action_type, target, result, logs
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#;

        let logs_json = serde_json::to_string(&execution.logs).unwrap_or_else(|_| "[]".to_string());

        self.db
            .execute_with_params(
                sql,
                &[
                    &execution.id,
                    &execution.tenant_id,
                    &execution.timestamp.to_rfc3339(),
                    execution.level.as_str(),
                    execution.action_type.as_str(),
                    &execution.target,
                    execution.result.as_str(),
                    &logs_json,
                ],
            )
            .await?;

        Ok(())
    }

    pub async fn get_recent(
        &self,
        _tenant_id: &str,
        _limit: u32,
        _offset: u32,
    ) -> std::result::Result<Vec<HealingExecution>, sqlx::Error> {
        let sql = r#"
            SELECT id, tenant_id, timestamp, level, action_type, target, result, logs
            FROM healing_executions
            WHERE tenant_id = ?
            ORDER BY timestamp DESC
            LIMIT ? OFFSET ?
        "#;

        self.db.query(sql, |row| {
            let id: String = row.try_get("id")?;
            let tenant_id: String = row.try_get("tenant_id")?;
            let timestamp_str: String = row.try_get("timestamp")?;
            let level_str: String = row.try_get("level")?;
            let action_type_str: String = row.try_get("action_type")?;
            let target: String = row.try_get("target")?;
            let result_str: String = row.try_get("result")?;
            let logs_str: String = row.try_get("logs")?;

            let timestamp = chrono::DateTime::parse_from_rfc3339(&timestamp_str)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());

            let level = match level_str.as_str() {
                "L0" => SeverityLevel::L0,
                "L1" => SeverityLevel::L1,
                "L2" => SeverityLevel::L2,
                "L3" => SeverityLevel::L3,
                _ => SeverityLevel::L0,
            };

            let action_type = match action_type_str.as_str() {
                "logOnly" => RecoveryActionType::LogOnly,
                "restartDriver" => RecoveryActionType::RestartDriver,
                "rejoinLora" => RecoveryActionType::RejoinLora,
                "reconnectDevice" => RecoveryActionType::ReconnectDevice,
                "cleanLogs" => RecoveryActionType::CleanLogs,
                "reportCloud" => RecoveryActionType::ReportCloud,
                "createTicket" => RecoveryActionType::CreateTicket,
                _ => RecoveryActionType::LogOnly,
            };

            let result = match result_str.as_str() {
                "Success" => ExecutionResult::Success,
                "Failed" => ExecutionResult::Failed,
                "PendingApproval" => ExecutionResult::PendingApproval,
                "Skipped" => ExecutionResult::Skipped,
                _ => ExecutionResult::Failed,
            };

            let logs: Vec<String> =
                serde_json::from_str(&logs_str).unwrap_or_else(|_| Vec::new());

            Ok(HealingExecution {
                id,
                tenant_id,
                timestamp,
                level,
                action_type,
                target,
                result,
                logs,
            })
        }).await
    }

    pub async fn count(&self, _tenant_id: &str) -> std::result::Result<u32, sqlx::Error> {
        let sql = r#"
            SELECT COUNT(*) as cnt FROM healing_executions WHERE tenant_id = ?
        "#;

        self.db
            .query_first(sql, |row| row.try_get::<i64, _>("cnt"))
            .await
            .map(|cnt| cnt.unwrap_or(0) as u32)
    }

    pub async fn ensure_table(&self) -> std::result::Result<(), sqlx::Error> {
        let sql = r#"
            CREATE TABLE IF NOT EXISTS healing_executions (
                id TEXT PRIMARY KEY,
                tenant_id TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                level TEXT NOT NULL,
                action_type TEXT NOT NULL,
                target TEXT NOT NULL,
                result TEXT NOT NULL,
                logs TEXT NOT NULL
            )
        "#;

        self.db.execute(sql).await?;
        Ok(())
    }
}
