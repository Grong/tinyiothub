use chrono::{DateTime, Utc};
use serde_json::json;
use sha2::Digest;
use sqlx::{SqlSafeStr, SqliteConnection, SqlitePool};
use tinyiothub_core::harness::{CommandKind, CommandState, ExecutionMode, HarnessCommand, HarnessEventInput};

use crate::{
    Db,
    error::{DbError, Result},
};

const COMMAND_SELECT: &str = "SELECT command_id, kind, target_task_id, target_run_id, actor, payload_json, created_at, expires_at, state FROM commands";

#[derive(Debug, sqlx::FromRow)]
struct CommandRow {
    command_id: String,
    kind: String,
    target_task_id: String,
    target_run_id: String,
    actor: String,
    payload_json: String,
    created_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
    state: String,
}

impl TryFrom<CommandRow> for HarnessCommand {
    type Error = DbError;

    fn try_from(row: CommandRow) -> Result<Self> {
        Ok(Self {
            command_id: row.command_id,
            kind: CommandKind::from_db(&row.kind)
                .ok_or_else(|| DbError::Internal(format!("unknown command kind: {}", row.kind)))?,
            target_task_id: row.target_task_id,
            target_run_id: row.target_run_id,
            actor: row.actor,
            payload: serde_json::from_str(&row.payload_json)?,
            created_at: row.created_at,
            expires_at: row.expires_at,
            state: CommandState::from_db(&row.state)
                .ok_or_else(|| DbError::Internal(format!("unknown command state: {}", row.state)))?,
        })
    }
}

pub(crate) fn migration() -> sqlx::migrate::Migration {
    sqlx::migrate::Migration::new(
        20261103000004,
        "harness_commands".into(),
        sqlx::migrate::MigrationType::Simple,
        include_str!("../migrations/20261103000004_harness_commands.sql").into_sql_str(),
        false,
    )
}

async fn load_command(connection: &mut SqliteConnection, command_id: &str) -> Result<Option<HarnessCommand>> {
    sqlx::query_as::<_, CommandRow>(sqlx::AssertSqlSafe(format!("{COMMAND_SELECT} WHERE command_id = ?")))
        .bind(command_id)
        .fetch_optional(connection)
        .await?
        .map(HarnessCommand::try_from)
        .transpose()
}

fn event_for(
    workspace_id: &str,
    command: &HarnessCommand,
    occurred_at: DateTime<Utc>,
    reason: Option<&str>,
) -> Result<HarnessEventInput> {
    let payload = json!({
        "commandId": command.command_id,
        "kind": command.kind.as_str(),
        "targetTaskId": command.target_task_id,
        "targetRunId": command.target_run_id,
        "actor": command.actor,
        "state": command.state.as_str(),
        "payload": command.payload,
        "reason": reason,
    });
    let payload_json = serde_json::to_string(&payload)?;
    Ok(HarnessEventInput {
        event_id: format!("command:{}:{}", command.command_id, command.state.as_str()),
        workspace_id: workspace_id.to_string(),
        session_id: format!("task:{}", command.target_task_id),
        execution_mode: ExecutionMode::Production,
        run_id: Some(command.target_run_id.clone()),
        task_id: Some(command.target_task_id.clone()),
        event_type: command.state.event_type().to_string(),
        occurred_at,
        cause_id: None,
        schema_v: 1,
        payload_digest: format!("{:x}", sha2::Sha256::digest(payload_json.as_bytes())),
        payload_json: payload,
    })
}

async fn write_event(
    connection: &mut SqliteConnection,
    workspace_id: &str,
    command: &HarnessCommand,
    occurred_at: DateTime<Utc>,
    reason: Option<&str>,
) -> Result<()> {
    crate::harness_effect::insert_event(connection, &event_for(workspace_id, command, occurred_at, reason)?).await
}

fn same_request(stored: &HarnessCommand, requested: &HarnessCommand) -> bool {
    stored.kind == requested.kind
        && stored.target_task_id == requested.target_task_id
        && stored.target_run_id == requested.target_run_id
        && stored.actor == requested.actor
        && stored.payload == requested.payload
        && stored.created_at == requested.created_at
        && stored.expires_at == requested.expires_at
}

async fn transition_command(
    connection: &mut SqliteConnection,
    workspace_id: &str,
    command: &HarnessCommand,
    next: CommandState,
    now: DateTime<Utc>,
    reason: &str,
) -> Result<()> {
    let updated = sqlx::query("UPDATE commands SET state = ? WHERE command_id = ? AND state = 'requested'")
        .bind(next.as_str())
        .bind(&command.command_id)
        .execute(&mut *connection)
        .await?
        .rows_affected();
    if updated != 1 {
        return Err(DbError::CasConflict {
            message: format!("command {} is no longer requested", command.command_id),
        });
    }
    let mut transitioned = command.clone();
    transitioned.state = next;
    write_event(connection, workspace_id, &transitioned, now, Some(reason)).await
}

async fn insert_wake(connection: &mut SqliteConnection, command: &HarnessCommand, now: DateTime<Utc>) -> Result<()> {
    let wake_id = format!("command:{}:wake", command.command_id);
    let signal = json!({
        "commandId": command.command_id,
        "kind": command.kind.as_str(),
        "payload": command.payload,
    })
    .to_string();
    let inserted = sqlx::query(
        "INSERT INTO task_wake_outbox
            (task_id, event_id, signal_json, dispatch_state, created_at, dispatched_at, resolved_at)
         VALUES (?, ?, ?, 'pending', ?, NULL, NULL)
         ON CONFLICT(task_id, event_id) DO NOTHING",
    )
    .bind(&command.target_task_id)
    .bind(&wake_id)
    .bind(&signal)
    .bind(now.to_rfc3339())
    .execute(&mut *connection)
    .await?
    .rows_affected();
    if inserted == 0 {
        let stored = sqlx::query_scalar::<_, String>(
            "SELECT signal_json FROM task_wake_outbox WHERE task_id = ? AND event_id = ?",
        )
        .bind(&command.target_task_id)
        .bind(&wake_id)
        .fetch_one(&mut *connection)
        .await?;
        if stored != signal {
            return Err(DbError::Conflict {
                message: format!("wake {wake_id} already exists with another signal"),
            });
        }
    }
    Ok(())
}

fn terminal_task(state: &str) -> bool {
    matches!(
        state,
        "completed" | "done" | "succeeded" | "failed" | "cancelled" | "cancelled_unknown"
    )
}

fn terminal_run(state: &str) -> bool {
    matches!(
        state,
        "completed" | "done" | "succeeded" | "failed" | "cancelled" | "cancelled_unknown" | "interrupted"
    )
}

async fn consume_once(
    pool: &SqlitePool,
    workspace_id: &str,
    target_task_id: &str,
    now: DateTime<Utc>,
) -> Result<Option<HarnessCommand>> {
    let mut connection = pool.acquire().await?.detach();
    sqlx::query("BEGIN IMMEDIATE").execute(&mut connection).await?;
    let result = async {
        loop {
            let selected = sqlx::query_as::<_, CommandRow>(sqlx::AssertSqlSafe(format!(
                "{COMMAND_SELECT} WHERE target_task_id = ? AND state = 'requested' ORDER BY seq LIMIT 1"
            )))
            .bind(target_task_id)
            .fetch_optional(&mut connection)
            .await?
            .map(HarnessCommand::try_from)
            .transpose()?;
            let Some(mut selected) = selected else {
                return Ok(None);
            };
            if selected.expires_at.is_some_and(|expires| expires <= now) {
                transition_command(&mut connection, workspace_id, &selected, CommandState::Expired, now, "expired")
                    .await?;
                continue;
            }
            let task_state = sqlx::query_scalar::<_, String>("SELECT state FROM agent_tasks WHERE id = ?")
                .bind(target_task_id)
                .fetch_optional(&mut connection)
                .await?;
            let Some(task_state) = task_state else {
                transition_command(
                    &mut connection,
                    workspace_id,
                    &selected,
                    CommandState::Rejected,
                    now,
                    "missing_target_task",
                )
                .await?;
                continue;
            };
            let run_state = sqlx::query_scalar::<_, String>(
                "SELECT state FROM agent_runs WHERE id = ? AND task_id = ?",
            )
            .bind(&selected.target_run_id)
            .bind(target_task_id)
            .fetch_optional(&mut connection)
            .await?;
            let Some(run_state) = run_state else {
                transition_command(
                    &mut connection,
                    workspace_id,
                    &selected,
                    CommandState::Rejected,
                    now,
                    "missing_target_run",
                )
                .await?;
                continue;
            };
            if terminal_task(&task_state) || task_state == "cancelling" {
                transition_command(
                    &mut connection,
                    workspace_id,
                    &selected,
                    CommandState::Superseded,
                    now,
                    "target_not_active",
                )
                .await?;
                continue;
            }
            if task_state == "paused" && selected.kind != CommandKind::Cancel {
                return Ok(None);
            }
            if selected.kind != CommandKind::Cancel && terminal_run(&run_state) {
                transition_command(
                    &mut connection,
                    workspace_id,
                    &selected,
                    CommandState::Rejected,
                    now,
                    "terminal_target_run",
                )
                .await?;
                continue;
            }
            match selected.kind {
                CommandKind::Interrupt if run_state != "model_calling" => return Ok(None),
                CommandKind::Steer | CommandKind::Queue if run_state == "model_calling" => return Ok(None),
                _ => {}
            }

            transition_command(
                &mut connection,
                workspace_id,
                &selected,
                CommandState::Consumed,
                now,
                "consumed",
            )
            .await?;
            match selected.kind {
                CommandKind::Interrupt => {
                    let updated = sqlx::query(
                        "UPDATE agent_runs SET state = 'interrupted' WHERE id = ? AND task_id = ? AND state = 'model_calling'",
                    )
                    .bind(&selected.target_run_id)
                    .bind(target_task_id)
                    .execute(&mut connection)
                    .await?
                    .rows_affected();
                    if updated != 1 {
                        return Err(DbError::CasConflict {
                            message: format!("run {} left its model call", selected.target_run_id),
                        });
                    }
                    let last_persisted_seq = sqlx::query_scalar::<_, i64>(
                        "SELECT COALESCE(MAX(seq), 0) FROM session_events WHERE run_id = ?",
                    )
                    .bind(&selected.target_run_id)
                    .fetch_one(&mut connection)
                    .await?;
                    let payload = json!({
                        "commandId": selected.command_id,
                        "runId": selected.target_run_id,
                        "state": "interrupted",
                        "incomplete_tail": true,
                        "last_persisted_seq": last_persisted_seq,
                    });
                    crate::harness_effect::insert_event(
                        &mut connection,
                        &HarnessEventInput {
                            event_id: format!("command:{}:interrupted", selected.command_id),
                            workspace_id: workspace_id.to_string(),
                            session_id: format!("task:{target_task_id}"),
                            execution_mode: ExecutionMode::Production,
                            run_id: Some(selected.target_run_id.clone()),
                            task_id: Some(target_task_id.to_string()),
                            event_type: "run.interrupted".to_string(),
                            occurred_at: now,
                            cause_id: None,
                            schema_v: 1,
                            payload_digest: format!("{:x}", sha2::Sha256::digest(payload.to_string().as_bytes())),
                            payload_json: payload,
                        },
                    )
                    .await?;
                }
                CommandKind::Pause => {
                    let updated = sqlx::query(
                        "UPDATE agent_tasks SET state = 'paused', state_version = state_version + 1 WHERE id = ? AND state = ?",
                    )
                    .bind(target_task_id)
                    .bind(&task_state)
                    .execute(&mut connection)
                    .await?
                    .rows_affected();
                    if updated != 1 {
                        return Err(DbError::CasConflict {
                            message: format!("task {target_task_id} changed during pause"),
                        });
                    }
                }
                CommandKind::Cancel => {
                    let updated = sqlx::query(
                        "UPDATE agent_tasks SET state = 'cancelling', state_version = state_version + 1 WHERE id = ? AND state = ?",
                    )
                    .bind(target_task_id)
                    .bind(&task_state)
                    .execute(&mut connection)
                    .await?
                    .rows_affected();
                    if updated != 1 {
                        return Err(DbError::CasConflict {
                            message: format!("task {target_task_id} changed during cancel"),
                        });
                    }
                    if !terminal_run(&run_state) {
                        sqlx::query("UPDATE agent_runs SET state = 'cancelling' WHERE id = ? AND task_id = ?")
                            .bind(&selected.target_run_id)
                            .bind(target_task_id)
                            .execute(&mut connection)
                            .await?;
                    }
                    let remaining = sqlx::query_as::<_, CommandRow>(sqlx::AssertSqlSafe(format!(
                        "{COMMAND_SELECT} WHERE target_task_id = ? AND state = 'requested' AND command_id != ? ORDER BY seq"
                    )))
                    .bind(target_task_id)
                    .bind(&selected.command_id)
                    .fetch_all(&mut connection)
                    .await?
                    .into_iter()
                    .map(HarnessCommand::try_from)
                    .collect::<Result<Vec<_>>>()?;
                    for command in remaining {
                        let (next, reason) = if command.expires_at.is_some_and(|expires| expires <= now) {
                            (CommandState::Expired, "expired")
                        } else {
                            (CommandState::Superseded, "cancelled")
                        };
                        transition_command(&mut connection, workspace_id, &command, next, now, reason).await?;
                    }
                }
                CommandKind::Steer | CommandKind::Queue => {}
            }
            if selected.kind != CommandKind::Interrupt {
                insert_wake(&mut connection, &selected, now).await?;
            }
            selected.state = CommandState::Consumed;
            return Ok(Some(selected));
        }
    }
    .await;
    match result {
        Ok(command) => {
            sqlx::query("COMMIT").execute(&mut connection).await?;
            Ok(command)
        }
        Err(error) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut connection).await;
            Err(error)
        }
    }
}

impl Db {
    pub async fn request_harness_command(
        &self,
        workspace_id: &str,
        command: &HarnessCommand,
    ) -> Result<HarnessCommand> {
        if command.command_id.is_empty()
            || command.target_task_id.is_empty()
            || command.target_run_id.is_empty()
            || command.actor.is_empty()
            || command.state != CommandState::Requested
        {
            return Err(DbError::Validation {
                message: "command ID, explicit target, actor and requested state are required".to_string(),
            });
        }
        let pool = self.workspace_pool(workspace_id).await?;
        let mut tx = pool.begin().await?;
        let inserted = sqlx::query(
            "INSERT INTO commands
                (command_id, kind, target_task_id, target_run_id, actor, payload_json, created_at, expires_at, state)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'requested')
             ON CONFLICT(command_id) DO NOTHING",
        )
        .bind(&command.command_id)
        .bind(command.kind.as_str())
        .bind(&command.target_task_id)
        .bind(&command.target_run_id)
        .bind(&command.actor)
        .bind(serde_json::to_string(&command.payload)?)
        .bind(command.created_at.to_rfc3339())
        .bind(command.expires_at.map(|value| value.to_rfc3339()))
        .execute(&mut *tx)
        .await?
        .rows_affected();
        let stored = load_command(&mut tx, &command.command_id)
            .await?
            .ok_or_else(|| DbError::Internal("inserted command disappeared".to_string()))?;
        if !same_request(&stored, command) {
            return Err(DbError::Conflict {
                message: format!("command {} already exists with different content", command.command_id),
            });
        }
        if inserted == 1 {
            write_event(&mut tx, workspace_id, &stored, command.created_at, None).await?;
        }
        tx.commit().await?;
        Ok(stored)
    }

    pub async fn harness_command(&self, workspace_id: &str, command_id: &str) -> Result<Option<HarnessCommand>> {
        let pool = self.workspace_pool(workspace_id).await?;
        let mut connection = pool.acquire().await?.detach();
        load_command(&mut connection, command_id).await
    }

    pub async fn list_requested_harness_commands(&self, workspace_id: &str) -> Result<Vec<HarnessCommand>> {
        let pool = self.workspace_pool(workspace_id).await?;
        sqlx::query_as::<_, CommandRow>(sqlx::AssertSqlSafe(format!(
            "{COMMAND_SELECT} WHERE state = 'requested' ORDER BY target_task_id, seq"
        )))
        .fetch_all(&pool)
        .await?
        .into_iter()
        .map(HarnessCommand::try_from)
        .collect()
    }

    pub async fn expire_harness_commands(&self, workspace_id: &str, now: DateTime<Utc>) -> Result<u64> {
        let pool = self.workspace_pool(workspace_id).await?;
        let mut tx = pool.begin().await?;
        let due = sqlx::query_as::<_, CommandRow>(sqlx::AssertSqlSafe(format!(
            "{COMMAND_SELECT} WHERE state = 'requested' AND expires_at IS NOT NULL AND expires_at <= ? ORDER BY seq"
        )))
        .bind(now.to_rfc3339())
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .map(HarnessCommand::try_from)
        .collect::<Result<Vec<_>>>()?;
        let count = due.len() as u64;
        for command in due {
            transition_command(&mut tx, workspace_id, &command, CommandState::Expired, now, "expired").await?;
        }
        tx.commit().await?;
        Ok(count)
    }

    pub async fn reject_harness_command(
        &self,
        workspace_id: &str,
        command_id: &str,
        now: DateTime<Utc>,
        reason: &str,
    ) -> Result<Option<HarnessCommand>> {
        let pool = self.workspace_pool(workspace_id).await?;
        let mut tx = pool.begin().await?;
        let Some(command) = load_command(&mut tx, command_id).await? else {
            tx.commit().await?;
            return Ok(None);
        };
        if command.state != CommandState::Requested {
            tx.commit().await?;
            return Ok(Some(command));
        }
        let next = if command.expires_at.is_some_and(|expires| expires <= now) {
            CommandState::Expired
        } else {
            CommandState::Rejected
        };
        transition_command(&mut tx, workspace_id, &command, next, now, reason).await?;
        let mut transitioned = command;
        transitioned.state = next;
        tx.commit().await?;
        Ok(Some(transitioned))
    }

    pub async fn consume_next_command(
        &self,
        workspace_id: &str,
        target_task_id: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<HarnessCommand>> {
        self.guard_recovery_matrix(workspace_id).await?;
        let pool = self.workspace_pool(workspace_id).await?;
        consume_once(&pool, workspace_id, target_task_id, now).await
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::{DateTime, Duration, Utc};
    use serde_json::{Value, json};
    use tinyiothub_core::harness::{CommandKind, CommandState, HarnessCommand};

    use crate::{Db, DbError};

    const WS: &str = "ws-commands";

    struct Fixture {
        root: PathBuf,
        db: Db,
    }

    impl Fixture {
        async fn new() -> Self {
            let root = std::env::temp_dir().join(format!("tinyiothub-harness-command-{}", uuid::Uuid::new_v4()));
            let pool = crate::test_helpers::test_pool().await;
            let mut db = Db::new(pool);
            db.workspace_store_root = root.join("data/agents");
            crate::seed::seed_system(&db).await.expect("seed platform registry");
            sqlx::query(
                "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) VALUES (?, 'workspace', 'tenant-default-001', 'now', 'now')",
            )
            .bind(WS)
            .execute(db.pool())
            .await
            .expect("insert workspace");
            db.create_workspace_store(WS).await.expect("create workspace store");
            Self { root, db }
        }

        async fn pool(&self) -> sqlx::SqlitePool {
            self.db.workspace_pool(WS).await.expect("workspace pool")
        }

        async fn seed_target(&self, task_id: &str, task_state: &str, run_state: &str) {
            let pool = self.pool().await;
            sqlx::query("INSERT INTO agent_tasks (id, run_id, state, state_version) VALUES (?, ?, ?, 1)")
                .bind(task_id)
                .bind(format!("run-{task_id}"))
                .bind(task_state)
                .execute(&pool)
                .await
                .expect("task");
            sqlx::query("INSERT INTO agent_runs (id, task_id, state) VALUES (?, ?, ?)")
                .bind(format!("run-{task_id}"))
                .bind(task_id)
                .bind(run_state)
                .execute(&pool)
                .await
                .expect("run");
        }

        async fn request(&self, command: HarnessCommand) -> HarnessCommand {
            self.db
                .request_harness_command(WS, &command)
                .await
                .expect("request command")
        }

        async fn state(&self, command_id: &str) -> CommandState {
            self.db
                .harness_command(WS, command_id)
                .await
                .expect("load command")
                .expect("command exists")
                .state
        }

        async fn close(self) {
            self.db.close_workspace_store(WS).await.expect("close workspace");
            self.db.pool().close().await;
            let _ = tokio::fs::remove_dir_all(&self.root).await;
        }
    }

    fn at() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-03T00:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc)
    }

    fn command(id: &str, kind: CommandKind, task_id: &str, payload: Value) -> HarnessCommand {
        HarnessCommand {
            command_id: id.to_string(),
            kind,
            target_task_id: task_id.to_string(),
            target_run_id: format!("run-{task_id}"),
            actor: "operator-1".to_string(),
            payload,
            created_at: at(),
            expires_at: Some(at() + Duration::hours(1)),
            state: CommandState::Requested,
        }
    }

    async fn wake_count(fixture: &Fixture, task_id: &str) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM task_wake_outbox WHERE task_id = ?")
            .bind(task_id)
            .fetch_one(&fixture.pool().await)
            .await
            .expect("wake count")
    }

    #[tokio::test]
    async fn harness_command_request_deduplicates_stable_id_and_rejects_changed_payload() {
        let fixture = Fixture::new().await;
        fixture.seed_target("task-dedupe", "running", "running").await;
        let original = command(
            "command-dedupe",
            CommandKind::Queue,
            "task-dedupe",
            json!({"input":"one"}),
        );
        let stored = fixture.request(original.clone()).await;
        assert_eq!(stored.state, CommandState::Requested);
        assert_eq!(fixture.request(original.clone()).await, stored);
        let requested_events: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM session_events WHERE type = 'command.requested'")
                .fetch_one(&fixture.pool().await)
                .await
                .expect("requested events");
        assert_eq!(requested_events, 1);

        let changed = HarnessCommand {
            payload: json!({"input":"two"}),
            ..original
        };
        let error = fixture
            .db
            .request_harness_command(WS, &changed)
            .await
            .expect_err("same ID with different payload must conflict");
        assert!(matches!(error, DbError::Conflict { .. }));
        fixture.close().await;
    }

    #[tokio::test]
    async fn harness_command_consume_is_fifo_and_idempotent_across_targets() {
        let fixture = Fixture::new().await;
        fixture.seed_target("task-a", "running", "running").await;
        fixture.seed_target("task-b", "running", "running").await;
        for command in [
            command("queue-a1", CommandKind::Queue, "task-a", json!({"input":1})),
            command("queue-a2", CommandKind::Queue, "task-a", json!({"input":2})),
            command("queue-b1", CommandKind::Queue, "task-b", json!({"input":3})),
        ] {
            fixture.request(command).await;
        }

        let first = fixture
            .db
            .consume_next_command(WS, "task-a", at())
            .await
            .expect("consume first")
            .expect("first command");
        assert_eq!(first.command_id, "queue-a1");
        assert_eq!(first.state, CommandState::Consumed);
        let second = fixture
            .db
            .consume_next_command(WS, "task-a", at())
            .await
            .expect("consume second")
            .expect("second command");
        assert_eq!(second.command_id, "queue-a2");
        assert!(
            fixture
                .db
                .consume_next_command(WS, "task-a", at())
                .await
                .expect("duplicate consume")
                .is_none()
        );
        let other = fixture
            .db
            .consume_next_command(WS, "task-b", at())
            .await
            .expect("other target")
            .expect("other command");
        assert_eq!(other.command_id, "queue-b1");
        assert_eq!(wake_count(&fixture, "task-a").await, 2);
        assert_eq!(wake_count(&fixture, "task-b").await, 1);
        fixture.close().await;
    }

    #[tokio::test]
    async fn harness_command_expiry_scanner_and_consume_never_execute_expired_heads() {
        let fixture = Fixture::new().await;
        fixture.seed_target("task-expiry", "running", "running").await;
        fixture.seed_target("task-scanner", "running", "running").await;
        fixture
            .request(HarnessCommand {
                expires_at: Some(at() - Duration::seconds(1)),
                ..command(
                    "queue-expired",
                    CommandKind::Queue,
                    "task-expiry",
                    json!({"input":"old"}),
                )
            })
            .await;
        fixture
            .request(command(
                "queue-valid",
                CommandKind::Queue,
                "task-expiry",
                json!({"input":"new"}),
            ))
            .await;
        fixture
            .request(HarnessCommand {
                expires_at: Some(at() - Duration::seconds(1)),
                ..command(
                    "queue-scanner",
                    CommandKind::Queue,
                    "task-scanner",
                    json!({"input":"old"}),
                )
            })
            .await;

        let consumed = fixture
            .db
            .consume_next_command(WS, "task-expiry", at())
            .await
            .expect("consume with expired head")
            .expect("next valid command");
        assert_eq!(consumed.command_id, "queue-valid");
        assert_eq!(fixture.state("queue-expired").await, CommandState::Expired);
        assert_eq!(wake_count(&fixture, "task-expiry").await, 1);

        assert_eq!(
            fixture.db.expire_harness_commands(WS, at()).await.expect("scan expiry"),
            1
        );
        assert_eq!(fixture.state("queue-scanner").await, CommandState::Expired);
        assert_eq!(
            fixture.db.expire_harness_commands(WS, at()).await.expect("repeat scan"),
            0
        );
        assert_eq!(wake_count(&fixture, "task-scanner").await, 0);
        fixture.close().await;
    }

    #[tokio::test]
    async fn harness_command_invalid_and_terminal_targets_close_without_wakes() {
        let fixture = Fixture::new().await;
        fixture.seed_target("task-terminal", "running", "completed").await;
        fixture.seed_target("task-cancelled", "cancelled", "cancelled").await;
        fixture
            .request(command(
                "steer-missing",
                CommandKind::Steer,
                "task-missing",
                json!({"guidance":"x"}),
            ))
            .await;
        fixture
            .request(command(
                "steer-terminal",
                CommandKind::Steer,
                "task-terminal",
                json!({"guidance":"x"}),
            ))
            .await;
        fixture
            .request(command(
                "queue-cancelled",
                CommandKind::Queue,
                "task-cancelled",
                json!({"input":"x"}),
            ))
            .await;

        for task_id in ["task-missing", "task-terminal", "task-cancelled"] {
            assert!(
                fixture
                    .db
                    .consume_next_command(WS, task_id, at())
                    .await
                    .expect("closed target consume")
                    .is_none()
            );
        }
        assert_eq!(fixture.state("steer-missing").await, CommandState::Rejected);
        assert_eq!(fixture.state("steer-terminal").await, CommandState::Rejected);
        assert_eq!(fixture.state("queue-cancelled").await, CommandState::Superseded);
        assert_eq!(wake_count(&fixture, "task-missing").await, 0);
        assert_eq!(wake_count(&fixture, "task-terminal").await, 0);
        assert_eq!(wake_count(&fixture, "task-cancelled").await, 0);
        fixture.close().await;
    }

    #[tokio::test]
    async fn harness_command_pause_holds_pending_and_cancel_supersedes_unconsumed() {
        let fixture = Fixture::new().await;
        fixture.seed_target("task-pause", "running", "running").await;
        for command in [
            command("pause-1", CommandKind::Pause, "task-pause", json!({})),
            command(
                "queue-paused",
                CommandKind::Queue,
                "task-pause",
                json!({"input":"held"}),
            ),
        ] {
            fixture.request(command).await;
        }
        let paused = fixture
            .db
            .consume_next_command(WS, "task-pause", at())
            .await
            .expect("consume pause")
            .expect("pause command");
        assert_eq!(paused.kind, CommandKind::Pause);
        let task_state: String = sqlx::query_scalar("SELECT state FROM agent_tasks WHERE id = 'task-pause'")
            .fetch_one(&fixture.pool().await)
            .await
            .expect("paused state");
        assert_eq!(task_state, "paused");
        assert!(
            fixture
                .db
                .consume_next_command(WS, "task-pause", at())
                .await
                .expect("paused consume")
                .is_none()
        );
        assert_eq!(fixture.state("queue-paused").await, CommandState::Requested);

        sqlx::query("UPDATE agent_tasks SET state = 'running' WHERE id = 'task-pause'")
            .execute(&fixture.pool().await)
            .await
            .expect("resume");
        assert_eq!(
            fixture
                .db
                .consume_next_command(WS, "task-pause", at())
                .await
                .expect("consume after resume")
                .expect("queued command")
                .command_id,
            "queue-paused"
        );
        fixture
            .request(command("cancel-1", CommandKind::Cancel, "task-pause", json!({})))
            .await;
        fixture
            .request(command(
                "steer-after-cancel",
                CommandKind::Steer,
                "task-pause",
                json!({"guidance":"no"}),
            ))
            .await;
        fixture
            .request(HarnessCommand {
                expires_at: Some(at() - Duration::seconds(1)),
                ..command(
                    "steer-expired-after-cancel",
                    CommandKind::Steer,
                    "task-pause",
                    json!({"guidance":"expired"}),
                )
            })
            .await;
        assert_eq!(
            fixture
                .db
                .consume_next_command(WS, "task-pause", at())
                .await
                .expect("consume cancel")
                .expect("cancel command")
                .command_id,
            "cancel-1"
        );
        assert_eq!(fixture.state("steer-after-cancel").await, CommandState::Superseded);
        assert_eq!(fixture.state("steer-expired-after-cancel").await, CommandState::Expired);
        let states: (String, String) = sqlx::query_as(
            "SELECT t.state, r.state FROM agent_tasks t JOIN agent_runs r ON r.task_id = t.id WHERE t.id = 'task-pause'",
        )
        .fetch_one(&fixture.pool().await)
        .await
        .expect("cancelling states");
        assert_eq!(states, ("cancelling".to_string(), "cancelling".to_string()));
        fixture.close().await;
    }

    #[tokio::test]
    async fn harness_command_interrupt_and_steer_observe_model_call_boundaries() {
        let fixture = Fixture::new().await;
        fixture.seed_target("task-interrupt", "running", "model_calling").await;
        fixture.seed_target("task-steer", "running", "model_calling").await;
        fixture
            .request(command(
                "interrupt-1",
                CommandKind::Interrupt,
                "task-interrupt",
                json!({}),
            ))
            .await;
        fixture
            .request(command(
                "steer-1",
                CommandKind::Steer,
                "task-steer",
                json!({"guidance":"next"}),
            ))
            .await;

        let interrupted = fixture
            .db
            .consume_next_command(WS, "task-interrupt", at())
            .await
            .expect("consume interrupt")
            .expect("current model call");
        assert_eq!(interrupted.kind, CommandKind::Interrupt);
        let states: (String, String) = sqlx::query_as(
            "SELECT t.state, r.state FROM agent_tasks t JOIN agent_runs r ON r.task_id = t.id WHERE t.id = 'task-interrupt'",
        )
        .fetch_one(&fixture.pool().await)
        .await
        .expect("interrupted states");
        assert_eq!(states, ("running".to_string(), "interrupted".to_string()));
        assert!(
            fixture
                .db
                .consume_next_command(WS, "task-steer", at())
                .await
                .expect("steer during call")
                .is_none()
        );
        assert_eq!(fixture.state("steer-1").await, CommandState::Requested);
        sqlx::query("UPDATE agent_runs SET state = 'running' WHERE id = 'run-task-steer'")
            .execute(&fixture.pool().await)
            .await
            .expect("finish model call");
        assert_eq!(
            fixture
                .db
                .consume_next_command(WS, "task-steer", at())
                .await
                .expect("consume before next call")
                .expect("steer command")
                .command_id,
            "steer-1"
        );
        fixture.close().await;
    }

    #[tokio::test]
    async fn harness_command_consume_rolls_back_target_event_and_wake_together() {
        let fixture = Fixture::new().await;
        fixture.seed_target("task-atomic", "running", "running").await;
        fixture
            .request(command(
                "steer-atomic",
                CommandKind::Steer,
                "task-atomic",
                json!({"guidance":"next"}),
            ))
            .await;
        sqlx::query(
            "INSERT INTO task_wake_outbox
                (task_id, event_id, signal_json, dispatch_state, created_at, dispatched_at, resolved_at)
             VALUES ('task-atomic', 'command:steer-atomic:wake', 'different', 'pending', ?, NULL, NULL)",
        )
        .bind(at().to_rfc3339())
        .execute(&fixture.pool().await)
        .await
        .expect("conflicting wake");

        fixture
            .db
            .consume_next_command(WS, "task-atomic", at())
            .await
            .expect_err("wake conflict rolls back consumption");
        assert_eq!(fixture.state("steer-atomic").await, CommandState::Requested);
        let consumed_events: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM session_events WHERE type = 'command.consumed'")
                .fetch_one(&fixture.pool().await)
                .await
                .expect("consumed events");
        assert_eq!(consumed_events, 0);
        let version: i64 = sqlx::query_scalar("SELECT state_version FROM agent_tasks WHERE id = 'task-atomic'")
            .fetch_one(&fixture.pool().await)
            .await
            .expect("task version");
        assert_eq!(version, 1);
        fixture.close().await;
    }

    #[tokio::test]
    async fn harness_command_invalid_recovery_matrix_blocks_consume_without_side_effects() {
        let fixture = Fixture::new().await;
        fixture.seed_target("task-invalid", "running", "running").await;
        sqlx::query(
            "INSERT INTO effect_intents
                (effect_id, session_id, task_id, run_id, tool_call_id, tool_name, request_json,
                 request_digest, idempotency_key, external_ref, state, recovery_from, state_version,
                 result_json, created_at, updated_at)
             VALUES ('effect-invalid', 'session-1', 'task-invalid', 'run-task-invalid', 'call-1',
                     'set_speed', '{}', 'digest-1', 'effect-invalid', NULL, 'prepared', NULL, 1,
                     NULL, ?, ?)",
        )
        .bind(at().to_rfc3339())
        .bind(at().to_rfc3339())
        .execute(&fixture.pool().await)
        .await
        .expect("seed invalid effect");
        sqlx::query(
            "INSERT INTO decisions
                (decision_id, kind, state, session_id, run_id, task_id, effect_id, subject_json,
                 options_json, created_at, updated_at, expires_at, answer, answer_actor,
                 answer_event_id, consumed_at, consumed_by)
             VALUES ('decision-invalid', 'effect_reconciliation', 'requested', 'session-1',
                     'run-task-invalid', 'task-invalid', 'effect-invalid', '{}', '[]', ?, ?, NULL,
                     NULL, NULL, NULL, NULL, NULL)",
        )
        .bind(at().to_rfc3339())
        .bind(at().to_rfc3339())
        .execute(&fixture.pool().await)
        .await
        .expect("seed invalid decision");
        fixture
            .request(command(
                "cmd-invalid",
                CommandKind::Steer,
                "task-invalid",
                json!({"guidance":"next"}),
            ))
            .await;

        let error = fixture
            .db
            .consume_next_command(WS, "task-invalid", at())
            .await
            .expect_err("invalid persisted matrix blocks command consume");
        assert!(
            error.to_string().contains("recovery.invalid_state"),
            "unexpected error: {error}"
        );

        let raw = sqlx::SqlitePool::connect(&format!(
            "sqlite:{}",
            fixture
                .root
                .join("data/agents")
                .join(WS)
                .join("harness/harness.db")
                .display()
        ))
        .await
        .expect("raw workspace database");
        let command_state: String = sqlx::query_scalar("SELECT state FROM commands WHERE command_id = 'cmd-invalid'")
            .fetch_one(&raw)
            .await
            .expect("command untouched");
        let consumed_events: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM session_events WHERE type = 'command.consumed'")
                .fetch_one(&raw)
                .await
                .expect("no consume event");
        let wake_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_wake_outbox")
            .fetch_one(&raw)
            .await
            .expect("no wake");
        raw.close().await;
        assert_eq!(command_state, "requested");
        assert_eq!(consumed_events, 0);
        assert_eq!(wake_count, 0);
        fixture.close().await;
    }
}
