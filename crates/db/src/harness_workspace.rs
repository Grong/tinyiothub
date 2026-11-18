use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::{DateTime, Utc};
use sqlx::{
    SqlSafeStr, SqlitePool,
    migrate::{Migration, MigrationType, Migrator},
    sqlite::SqliteConnectOptions,
};
#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Mutex;
#[cfg(test)]
use tokio::sync::Notify;

use crate::{
    database::Db,
    error::{DbError, Result},
};

#[derive(Debug)]
pub(crate) enum WorkspaceStoreState {
    Empty,
    Available(SqlitePool),
    Unavailable,
    Closed,
}

pub(crate) type WorkspaceStoreEntry = Arc<Mutex<WorkspaceStoreState>>;
pub(crate) type WorkspaceStores = Arc<Mutex<HashMap<String, WorkspaceStoreEntry>>>;

#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct WorkspaceStoreTestHooks {
    pub gated_workspace_id: String,
    pub open_started: Notify,
    pub open_release: Notify,
    pub fail_open: AtomicBool,
}

/// Restore journal phase. Each phase is persisted to the platform registry
/// before (and after) the corresponding filesystem rename, so a crash can be
/// classified without opening either SQLite generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestorePhase {
    /// Journal exists; live generation untouched, staging not yet validated.
    Prepared,
    /// Self-contained restore generation validated in `harness.restore/`.
    Staged,
    /// Live directory renamed (staged swap) to `harness.rollback/`.
    LiveRenamed,
    /// `harness.restore/` renamed (staged swap) to `harness/`.
    Swapped,
}

impl RestorePhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::Staged => "staged",
            Self::LiveRenamed => "live_renamed",
            Self::Swapped => "swapped",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        Some(match value {
            "prepared" => Self::Prepared,
            "staged" => Self::Staged,
            "live_renamed" => Self::LiveRenamed,
            "swapped" => Self::Swapped,
            _ => return None,
        })
    }
}

/// One platform-registry restore journal row. Paths are diagnostic only:
/// recovery always derives canonical paths from the registered workspace id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreJournalRow {
    pub workspace_id: String,
    pub epoch: i64,
    pub phase: String,
    pub backup_path: String,
    pub rollback_path: String,
    pub restore_path: String,
    pub updated_at: String,
}

/// Outcome of classifying one journal row at startup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreRecovery {
    /// The staged restore generation won and now serves as `harness/`.
    CompletedForward,
    /// The complete live generation was kept (restore had not started).
    KeptLive,
    /// The retained generation was restored after an incomplete swap.
    RolledBack,
    /// No complete generation exists; the workspace stays unavailable.
    Unavailable,
}

pub(crate) fn default_workspace_store_root() -> PathBuf {
    std::env::var_os("TINYIOTHUB__PROJECT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .join("data/agents")
}

fn harness_migrator() -> Migrator {
    let mut migrator = Migrator::with_migrations(vec![
        Migration::new(
            20261103000001,
            "harness_workspace".into(),
            MigrationType::Simple,
            include_str!("../migrations/20261103000001_harness_workspace.sql").into_sql_str(),
            false,
        ),
        crate::harness_event::migration(),
        crate::harness_effect::migration(),
        crate::harness_command::migration(),
        crate::harness_workflow::migration(),
        crate::harness_snapshot::migration(),
        crate::harness_simulation::migration(),
        crate::harness_supervisor::migration(),
        Migration::new(
            20261105000003,
            "harness_recovery_scan_indexes".into(),
            MigrationType::Simple,
            include_str!("../migrations/20261105000003_harness_recovery_scan_indexes.sql").into_sql_str(),
            false,
        ),
        Migration::new(
            20261105000004,
            "harness_task_store".into(),
            MigrationType::Simple,
            include_str!("../migrations/20261105000004_harness_task_store.sql").into_sql_str(),
            false,
        ),
        Migration::new(
            20261106000001,
            "harness_job_dispositions".into(),
            MigrationType::Simple,
            include_str!("../migrations/20261106000001_harness_job_dispositions.sql").into_sql_str(),
            false,
        ),
        Migration::new(
            20261107000003,
            "harness_events_recent_index".into(),
            MigrationType::Simple,
            include_str!("../migrations/20261107000003_harness_events_recent_index.sql").into_sql_str(),
            false,
        ),
        Migration::new(
            20261114000002,
            "harness_assistant_suppression_workspace".into(),
            MigrationType::Simple,
            include_str!("../migrations/20261114000002_harness_assistant_suppression_workspace.sql").into_sql_str(),
            false,
        ),
        Migration::new(
            20261114000003,
            "harness_suppression_fence_attempt_ids".into(),
            MigrationType::Simple,
            include_str!("../migrations/20261114000003_harness_suppression_fence_attempt_ids.sql").into_sql_str(),
            false,
        ),
        Migration::new(
            20261115000001,
            "harness_suppression_observations".into(),
            MigrationType::Simple,
            include_str!("../migrations/20261115000001_harness_suppression_observations.sql").into_sql_str(),
            false,
        ),
        Migration::new(
            20261116000001,
            "harness_suppression_linearization_and_sources".into(),
            MigrationType::Simple,
            include_str!("../migrations/20261116000001_harness_suppression_linearization_and_sources.sql")
                .into_sql_str(),
            false,
        ),
    ]);
    // Rollback/mixed-version workers must open workspaces that already contain
    // additive migrations unknown to their older binary, matching the platform
    // migrator's forward-tolerance policy.
    migrator.set_ignore_missing(true);
    migrator
}

pub(crate) fn validate_workspace_id(workspace_id: &str) -> Result<()> {
    if workspace_id.is_empty()
        || workspace_id == "."
        || workspace_id == ".."
        || workspace_id.contains('/')
        || workspace_id.contains('\\')
    {
        return Err(DbError::Validation {
            message: "workspace ID must be a single path component".to_string(),
        });
    }
    Ok(())
}

async fn sqlite_generation_is_complete(path: &Path) -> bool {
    if !tokio::fs::try_exists(path).await.unwrap_or(false) {
        return false;
    }
    // Open read-write without creation so a crashed WAL generation can be
    // recovered and checked. The connection never sees another directory's
    // -wal/-shm; it only validates the files inside `path`'s own directory.
    let options = SqliteConnectOptions::new().filename(path).create_if_missing(false);
    let Ok(pool) = SqlitePool::connect_with(options).await else {
        return false;
    };
    let check = sqlx::query_scalar::<_, String>("PRAGMA integrity_check")
        .fetch_one(&pool)
        .await;
    // SQLite accepts a zero-byte or freshly created file as an empty database
    // and reports `integrity_check = ok`. A harness generation always carries
    // migrated tables, so require the harness schema marker; otherwise a crash
    // before the copy wrote any bytes, or a foreign SQLite file, could be
    // selected over a good rollback generation.
    let harness_marker = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sqlite_master
         WHERE type = 'table' AND name IN ('agent_tasks', 'effect_intents')",
    )
    .fetch_one(&pool)
    .await;
    pool.close().await;
    matches!(check, Ok(value) if value == "ok") && matches!(harness_marker, Ok(count) if count > 0)
}

async fn directory_generation_is_complete(directory: &Path) -> bool {
    sqlite_generation_is_complete(&directory.join("harness.db")).await
}

async fn remove_directory_if_present(directory: &Path) -> std::io::Result<()> {
    match tokio::fs::remove_dir_all(directory).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Append SQLite's sidecar suffix (`-wal`, `-shm`, `-journal`) to a path
/// without assuming the main file is valid UTF-8.
fn sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

/// Replace the live generation with one already validated as complete. The
/// caller must never call this with an incomplete source.
async fn adopt_generation(source: &Path, live: &Path) -> bool {
    let _ = remove_directory_if_present(live).await;
    tokio::fs::rename(source, live).await.is_ok() && directory_generation_is_complete(live).await
}

async fn open_workspace_store(db: &Db, _workspace_id: &str, path: &Path) -> Result<SqlitePool> {
    #[cfg(test)]
    if let Some(hooks) = &db.workspace_store_test_hooks {
        if hooks.gated_workspace_id == _workspace_id {
            hooks.open_started.notify_one();
            hooks.open_release.notified().await;
        }
        if hooks.fail_open.swap(false, Ordering::SeqCst) {
            return Err(DbError::Internal("injected workspace open failure".to_string()));
        }
    }

    let parent = path
        .parent()
        .ok_or_else(|| DbError::Internal("workspace store path has no parent".to_string()))?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|error| DbError::Internal(format!("failed to create workspace store directory: {error}")))?;
    let pool = crate::pool::workspace_pool_options(&db.workspace_store_config)
        .connect_with(crate::pool::workspace_connect_options(path))
        .await?;
    if let Err(error) = harness_migrator().run(&pool).await {
        pool.close().await;
        return Err(error.into());
    }
    Ok(pool)
}

impl Db {
    async fn ensure_registered_workspace(&self, workspace_id: &str) -> Result<()> {
        validate_workspace_id(workspace_id)?;
        if crate::workspace::workspace_exists_by_id(self.pool(), workspace_id).await? {
            Ok(())
        } else {
            Err(DbError::NotFound {
                id: workspace_id.to_string(),
            })
        }
    }

    async fn workspace_store_entry(&self, workspace_id: &str) -> WorkspaceStoreEntry {
        let mut stores = self.workspace_stores.lock().await;
        stores
            .entry(workspace_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(WorkspaceStoreState::Empty)))
            .clone()
    }

    async fn open_workspace_store_entry(
        &self,
        workspace_id: &str,
        state: &mut WorkspaceStoreState,
    ) -> Result<SqlitePool> {
        // A restore may have crashed between any two renames. Classify the
        // journal before opening a generation so a partial swap can never mix
        // a DB with another generation's WAL/SHM.
        match self.restore_journal_row(workspace_id).await {
            Ok(Some(row)) => match self.recover_restore_journal(&row).await {
                Ok(RestoreRecovery::Unavailable) => {
                    *state = WorkspaceStoreState::Unavailable;
                    return Err(DbError::Internal(format!(
                        "workspace restore has no complete generation: {workspace_id}"
                    )));
                }
                Ok(RestoreRecovery::CompletedForward | RestoreRecovery::KeptLive | RestoreRecovery::RolledBack) => {}
                Err(error) => {
                    *state = WorkspaceStoreState::Unavailable;
                    return Err(error);
                }
            },
            Ok(None) => {}
            Err(error) => {
                *state = WorkspaceStoreState::Unavailable;
                return Err(error);
            }
        }
        let path = self.workspace_store_root.join(workspace_id).join("harness/harness.db");
        match open_workspace_store(self, workspace_id, &path).await {
            Ok(pool) => {
                *state = WorkspaceStoreState::Available(pool.clone());
                Ok(pool)
            }
            Err(error) => {
                if matches!(state, WorkspaceStoreState::Empty) {
                    *state = WorkspaceStoreState::Unavailable;
                }
                tracing::error!(workspace_id, error = %error, "workspace harness store open failed");
                Err(error)
            }
        }
    }

    pub async fn create_workspace_store(&self, workspace_id: &str) -> Result<()> {
        let entry = self.workspace_store_entry(workspace_id).await;
        let mut state = entry.lock().await;
        match &*state {
            WorkspaceStoreState::Empty => {
                drop(state);
                self.workspace_pool(workspace_id).await.map(|_| ())
            }
            WorkspaceStoreState::Available(_) => self.ensure_registered_workspace(workspace_id).await,
            WorkspaceStoreState::Closed | WorkspaceStoreState::Unavailable => {
                self.ensure_registered_workspace(workspace_id).await?;
                self.open_workspace_store_entry(workspace_id, &mut state)
                    .await
                    .map(|_| ())
            }
        }
    }

    pub(crate) async fn workspace_pool(&self, workspace_id: &str) -> Result<SqlitePool> {
        self.ensure_registered_workspace(workspace_id).await?;
        let entry = self.workspace_store_entry(workspace_id).await;
        let mut state = entry.lock().await;
        match &*state {
            WorkspaceStoreState::Available(pool) => return Ok(pool.clone()),
            WorkspaceStoreState::Unavailable => {
                return Err(DbError::Internal(format!(
                    "workspace harness store is unavailable: {workspace_id}"
                )));
            }
            WorkspaceStoreState::Closed => {
                return Err(DbError::Internal(format!(
                    "workspace harness store is closed: {workspace_id}"
                )));
            }
            WorkspaceStoreState::Empty => {}
        }
        self.open_workspace_store_entry(workspace_id, &mut state).await
    }

    /// Open an already-existing workspace generation without creating one.
    ///
    /// Diagnostics and drain reads must not turn a GET into a filesystem
    /// mutation for a registered workspace that never created a Harness DB
    /// (Task 19C3 Low 2). A restore journal counts as an existing generation:
    /// opening it is required to finish or roll back the interrupted restore.
    pub(crate) async fn try_existing_workspace_pool(&self, workspace_id: &str) -> Result<Option<SqlitePool>> {
        self.ensure_registered_workspace(workspace_id).await?;
        let path = self.workspace_store_root.join(workspace_id).join("harness/harness.db");
        if !tokio::fs::try_exists(&path).await.unwrap_or(false)
            && self.restore_journal_row(workspace_id).await?.is_none()
        {
            return Ok(None);
        }
        self.workspace_pool(workspace_id).await.map(Some)
    }

    #[cfg(any(test, feature = "testing"))]
    pub async fn workspace_pool_for_testing(&self, workspace_id: &str) -> Result<SqlitePool> {
        self.workspace_pool(workspace_id).await
    }

    pub async fn close_workspace_store(&self, workspace_id: &str) -> Result<()> {
        self.ensure_registered_workspace(workspace_id).await?;
        let entry = self.workspace_store_entry(workspace_id).await;
        let mut state = entry.lock().await;
        let previous = std::mem::replace(&mut *state, WorkspaceStoreState::Closed);
        if let WorkspaceStoreState::Available(pool) = previous {
            pool.close().await;
        }
        Ok(())
    }

    /// Degrade one workspace without touching any other. Used by the restore
    /// failure contract and by `recovery.invalid_state`: the pool is closed so
    /// no later dispatch or consume can reach an invalid persisted state.
    pub async fn mark_workspace_unavailable(&self, workspace_id: &str, reason: &str) -> Result<()> {
        let entry = self.workspace_store_entry(workspace_id).await;
        let mut state = entry.lock().await;
        let previous = std::mem::replace(&mut *state, WorkspaceStoreState::Unavailable);
        if let WorkspaceStoreState::Available(pool) = previous {
            pool.close().await;
        }
        tracing::error!(workspace_id, reason, "workspace marked unavailable");
        Ok(())
    }

    pub async fn restore_journal_row(&self, workspace_id: &str) -> Result<Option<RestoreJournalRow>> {
        let row = sqlx::query_as::<_, (String, i64, String, String, String, String, String)>(
            "SELECT workspace_id, epoch, phase, backup_path, rollback_path, restore_path, updated_at
             FROM harness_restore_journal WHERE workspace_id = ?",
        )
        .bind(workspace_id)
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(|row| RestoreJournalRow {
            workspace_id: row.0,
            epoch: row.1,
            phase: row.2,
            backup_path: row.3,
            rollback_path: row.4,
            restore_path: row.5,
            updated_at: row.6,
        }))
    }

    async fn next_restore_epoch(&self, workspace_id: &str) -> Result<i64> {
        let epoch: Option<i64> = sqlx::query_scalar("SELECT epoch FROM harness_restore_journal WHERE workspace_id = ?")
            .bind(workspace_id)
            .fetch_optional(self.pool())
            .await?;
        Ok(epoch.unwrap_or(0) + 1)
    }

    async fn write_restore_journal(
        &self,
        workspace_id: &str,
        epoch: i64,
        phase: RestorePhase,
        backup_path: &Path,
        updated_at: DateTime<Utc>,
    ) -> Result<()> {
        let base = self.workspace_store_root.join(workspace_id);
        sqlx::query(
            "INSERT INTO harness_restore_journal
                (workspace_id, epoch, phase, backup_path, rollback_path, restore_path, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(workspace_id) DO UPDATE SET
                epoch = excluded.epoch,
                phase = excluded.phase,
                backup_path = excluded.backup_path,
                rollback_path = excluded.rollback_path,
                restore_path = excluded.restore_path,
                updated_at = excluded.updated_at",
        )
        .bind(workspace_id)
        .bind(epoch)
        .bind(phase.as_str())
        .bind(backup_path.to_string_lossy().to_string())
        .bind(base.join("harness.rollback").to_string_lossy().to_string())
        .bind(base.join("harness.restore").to_string_lossy().to_string())
        .bind(updated_at.to_rfc3339())
        .execute(self.pool())
        .await?;
        Ok(())
    }

    async fn clear_restore_journal(&self, workspace_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM harness_restore_journal WHERE workspace_id = ?")
            .bind(workspace_id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Classify one journal row using only complete-directory evidence. The
    /// selected generation is moved into `harness/` with crash-safe staged
    /// whole-directory renames; a corrupt journal or a missing staging copy
    /// falls back to the retained generation instead of mixing WAL files.
    async fn recover_restore_journal(&self, row: &RestoreJournalRow) -> Result<RestoreRecovery> {
        let base = self.workspace_store_root.join(&row.workspace_id);
        let live = base.join("harness");
        let rollback = base.join("harness.rollback");
        let restore = base.join("harness.restore");
        let live_ok = directory_generation_is_complete(&live).await;
        let rollback_ok = directory_generation_is_complete(&rollback).await;
        let restore_ok = directory_generation_is_complete(&restore).await;
        let phase = RestorePhase::from_db(&row.phase);

        let rename_into = |source: &Path, target: &Path| {
            let source = source.to_path_buf();
            let target = target.to_path_buf();
            async move {
                tokio::fs::rename(&source, &target)
                    .await
                    .map_err(|error| DbError::Internal(format!("restore rename failed: {error}")))
            }
        };

        let outcome = match phase {
            Some(RestorePhase::Prepared | RestorePhase::Staged) if live_ok => {
                remove_directory_if_present(&restore)
                    .await
                    .map_err(|error| DbError::Internal(format!("restore cleanup failed: {error}")))?;
                remove_directory_if_present(&rollback)
                    .await
                    .map_err(|error| DbError::Internal(format!("restore cleanup failed: {error}")))?;
                RestoreRecovery::KeptLive
            }
            Some(RestorePhase::Prepared | RestorePhase::Staged) => {
                if restore_ok {
                    remove_directory_if_present(&live)
                        .await
                        .map_err(|error| DbError::Internal(format!("restore cleanup failed: {error}")))?;
                    rename_into(&restore, &live).await?;
                    remove_directory_if_present(&rollback)
                        .await
                        .map_err(|error| DbError::Internal(format!("restore cleanup failed: {error}")))?;
                    RestoreRecovery::CompletedForward
                } else if rollback_ok {
                    remove_directory_if_present(&live)
                        .await
                        .map_err(|error| DbError::Internal(format!("restore cleanup failed: {error}")))?;
                    rename_into(&rollback, &live).await?;
                    RestoreRecovery::RolledBack
                } else {
                    return Ok(RestoreRecovery::Unavailable);
                }
            }
            Some(RestorePhase::LiveRenamed) => {
                if live_ok {
                    // The forward rename completed even though the phase write
                    // did not; the live directory is already the new generation.
                    remove_directory_if_present(&rollback)
                        .await
                        .map_err(|error| DbError::Internal(format!("restore cleanup failed: {error}")))?;
                    remove_directory_if_present(&restore)
                        .await
                        .map_err(|error| DbError::Internal(format!("restore cleanup failed: {error}")))?;
                    RestoreRecovery::KeptLive
                } else if restore_ok {
                    remove_directory_if_present(&live)
                        .await
                        .map_err(|error| DbError::Internal(format!("restore cleanup failed: {error}")))?;
                    rename_into(&restore, &live).await?;
                    remove_directory_if_present(&rollback)
                        .await
                        .map_err(|error| DbError::Internal(format!("restore cleanup failed: {error}")))?;
                    RestoreRecovery::CompletedForward
                } else if rollback_ok {
                    remove_directory_if_present(&live)
                        .await
                        .map_err(|error| DbError::Internal(format!("restore cleanup failed: {error}")))?;
                    rename_into(&rollback, &live).await?;
                    remove_directory_if_present(&restore)
                        .await
                        .map_err(|error| DbError::Internal(format!("restore cleanup failed: {error}")))?;
                    RestoreRecovery::RolledBack
                } else {
                    return Ok(RestoreRecovery::Unavailable);
                }
            }
            Some(RestorePhase::Swapped) => {
                if live_ok {
                    remove_directory_if_present(&rollback)
                        .await
                        .map_err(|error| DbError::Internal(format!("restore cleanup failed: {error}")))?;
                    remove_directory_if_present(&restore)
                        .await
                        .map_err(|error| DbError::Internal(format!("restore cleanup failed: {error}")))?;
                    RestoreRecovery::KeptLive
                } else if rollback_ok {
                    remove_directory_if_present(&live)
                        .await
                        .map_err(|error| DbError::Internal(format!("restore cleanup failed: {error}")))?;
                    rename_into(&rollback, &live).await?;
                    remove_directory_if_present(&restore)
                        .await
                        .map_err(|error| DbError::Internal(format!("restore cleanup failed: {error}")))?;
                    RestoreRecovery::RolledBack
                } else if restore_ok {
                    remove_directory_if_present(&live)
                        .await
                        .map_err(|error| DbError::Internal(format!("restore cleanup failed: {error}")))?;
                    rename_into(&restore, &live).await?;
                    RestoreRecovery::CompletedForward
                } else {
                    return Ok(RestoreRecovery::Unavailable);
                }
            }
            None => {
                // Corrupt journal: trust only complete directories. Keep the
                // live generation when it is intact; otherwise fall back to
                // the retained rollback generation, never a mixed WAL pair.
                if live_ok {
                    remove_directory_if_present(&rollback)
                        .await
                        .map_err(|error| DbError::Internal(format!("restore cleanup failed: {error}")))?;
                    remove_directory_if_present(&restore)
                        .await
                        .map_err(|error| DbError::Internal(format!("restore cleanup failed: {error}")))?;
                    RestoreRecovery::KeptLive
                } else if rollback_ok {
                    remove_directory_if_present(&live)
                        .await
                        .map_err(|error| DbError::Internal(format!("restore cleanup failed: {error}")))?;
                    rename_into(&rollback, &live).await?;
                    remove_directory_if_present(&restore)
                        .await
                        .map_err(|error| DbError::Internal(format!("restore cleanup failed: {error}")))?;
                    RestoreRecovery::RolledBack
                } else if restore_ok {
                    remove_directory_if_present(&live)
                        .await
                        .map_err(|error| DbError::Internal(format!("restore cleanup failed: {error}")))?;
                    rename_into(&restore, &live).await?;
                    RestoreRecovery::CompletedForward
                } else {
                    return Ok(RestoreRecovery::Unavailable);
                }
            }
        };
        self.clear_restore_journal(&row.workspace_id).await?;
        tracing::warn!(
            workspace_id = %row.workspace_id,
            phase = %row.phase,
            ?outcome,
            "restore journal recovered"
        );
        Ok(outcome)
    }

    /// Startup scan for every pending restore journal (R29). Other workspaces
    /// keep serving throughout; a workspace without any complete generation
    /// is marked unavailable and keeps its journal for operators.
    pub async fn recover_restore_journals(&self) -> Result<Vec<(String, RestoreRecovery)>> {
        let rows = sqlx::query_as::<_, (String, i64, String, String, String, String, String)>(
            "SELECT workspace_id, epoch, phase, backup_path, rollback_path, restore_path, updated_at
             FROM harness_restore_journal ORDER BY workspace_id",
        )
        .fetch_all(self.pool())
        .await?;
        let mut outcomes = Vec::with_capacity(rows.len());
        for row in rows {
            let row = RestoreJournalRow {
                workspace_id: row.0,
                epoch: row.1,
                phase: row.2,
                backup_path: row.3,
                rollback_path: row.4,
                restore_path: row.5,
                updated_at: row.6,
            };
            let workspace_id = row.workspace_id.clone();
            match self.recover_restore_journal(&row).await {
                Ok(RestoreRecovery::Unavailable) => {
                    self.mark_workspace_unavailable(&workspace_id, "restore journal has no complete DB generation")
                        .await?;
                    outcomes.push((workspace_id, RestoreRecovery::Unavailable));
                }
                Ok(outcome) => outcomes.push((workspace_id, outcome)),
                Err(error) => {
                    tracing::error!(%workspace_id, %error, "restore journal recovery failed");
                    self.mark_workspace_unavailable(&workspace_id, "restore journal recovery failed")
                        .await?;
                    outcomes.push((workspace_id, RestoreRecovery::Unavailable));
                }
            }
        }
        Ok(outcomes)
    }

    /// Crash-safe independent restore (R29). The journal is persisted before
    /// every whole-directory rename; the workspace pool is closed first, the
    /// staged backup is validated as a self-contained harness DB (no WAL/SHM
    /// sidecars), and generations are moved with crash-safe staged renames.
    pub async fn restore_workspace_store(&self, workspace_id: &str, backup_path: &Path) -> Result<()> {
        validate_workspace_id(workspace_id)?;
        self.ensure_registered_workspace(workspace_id).await?;
        let base = self.workspace_store_root.join(workspace_id);
        tokio::fs::create_dir_all(&base)
            .await
            .map_err(|error| DbError::Internal(format!("create workspace directory failed: {error}")))?;
        let canonical_base = tokio::fs::canonicalize(&base)
            .await
            .map_err(|error| DbError::Internal(format!("resolve workspace directory failed: {error}")))?;
        let canonical_backup = tokio::fs::canonicalize(backup_path)
            .await
            .map_err(|error| DbError::Validation {
                message: format!("restore backup is missing: {error}"),
            })?;
        if !canonical_backup.starts_with(&canonical_base) {
            return Err(DbError::Validation {
                message: "restore backup must live inside the workspace directory".to_string(),
            });
        }
        // Only the self-contained main file may be restored. A WAL sibling
        // means commits live outside `harness.db` and copying just the main
        // file would silently drop them.
        for suffix in ["-wal", "-shm"] {
            let sidecar = sidecar_path(&canonical_backup, suffix);
            if tokio::fs::try_exists(&sidecar).await.unwrap_or(false) {
                return Err(DbError::Validation {
                    message: format!("restore backup is not self-contained: {} must not be present", suffix),
                });
            }
        }
        if !sqlite_generation_is_complete(&canonical_backup).await {
            return Err(DbError::Validation {
                message: "restore backup is not a complete harness generation".to_string(),
            });
        }

        let live = base.join("harness");
        let rollback = base.join("harness.rollback");
        let restore = base.join("harness.restore");
        let entry = self.workspace_store_entry(workspace_id).await;
        let mut state = entry.lock().await;
        let previous = std::mem::replace(&mut *state, WorkspaceStoreState::Unavailable);
        if let WorkspaceStoreState::Available(pool) = previous {
            pool.close().await;
        }
        let epoch = self.next_restore_epoch(workspace_id).await?;
        self.write_restore_journal(
            workspace_id,
            epoch,
            RestorePhase::Prepared,
            &canonical_backup,
            Utc::now(),
        )
        .await?;

        // Mirrors the journal phases: until the first live-directory rename
        // succeeds, the live generation is authoritative; afterwards the
        // retained rollback generation is.
        let mut swap_started = false;
        let attempt: Result<()> = async {
            remove_directory_if_present(&restore)
                .await
                .map_err(|error| DbError::Internal(format!("restore staging cleanup failed: {error}")))?;
            tokio::fs::create_dir_all(&restore)
                .await
                .map_err(|error| DbError::Internal(format!("restore staging directory failed: {error}")))?;
            tokio::fs::copy(&canonical_backup, restore.join("harness.db"))
                .await
                .map_err(|error| DbError::Internal(format!("restore staging copy failed: {error}")))?;
            if !directory_generation_is_complete(&restore).await {
                return Err(DbError::Internal(
                    "restore staging failed the SQLite integrity check".to_string(),
                ));
            }
            self.write_restore_journal(workspace_id, epoch, RestorePhase::Staged, &canonical_backup, Utc::now())
                .await?;
            remove_directory_if_present(&rollback)
                .await
                .map_err(|error| DbError::Internal(format!("restore rollback cleanup failed: {error}")))?;
            if tokio::fs::try_exists(&live)
                .await
                .map_err(|error| DbError::Internal(format!("check live directory failed: {error}")))?
            {
                tokio::fs::rename(&live, &rollback)
                    .await
                    .map_err(|error| DbError::Internal(format!("live directory rename failed: {error}")))?;
            }
            swap_started = true;
            self.write_restore_journal(
                workspace_id,
                epoch,
                RestorePhase::LiveRenamed,
                &canonical_backup,
                Utc::now(),
            )
            .await?;
            tokio::fs::rename(&restore, &live)
                .await
                .map_err(|error| DbError::Internal(format!("restore directory rename failed: {error}")))?;
            self.write_restore_journal(
                workspace_id,
                epoch,
                RestorePhase::Swapped,
                &canonical_backup,
                Utc::now(),
            )
            .await?;
            // Open directly: the normal open path would classify and clear the
            // journal we are still using for rollback.
            let database = live.join("harness.db");
            let pool = open_workspace_store(self, workspace_id, &database).await?;
            *state = WorkspaceStoreState::Available(pool);
            Ok(())
        }
        .await;

        match attempt {
            Ok(()) => {
                let _ = remove_directory_if_present(&rollback).await;
                let _ = remove_directory_if_present(&restore).await;
                // The store was already reopened inside `attempt`; a journal
                // cleanup failure must not turn a healthy restore into an Err.
                if let Err(clear_error) = self.clear_restore_journal(workspace_id).await {
                    tracing::error!(
                        workspace_id,
                        error = %clear_error,
                        "restore journal cleanup failed after successful restore"
                    );
                }
                Ok(())
            }
            Err(error) => {
                tracing::error!(workspace_id, %error, "workspace restore failed; rolling back");
                let live_complete = directory_generation_is_complete(&live).await;
                let rollback_complete = directory_generation_is_complete(&rollback).await;
                let restore_complete = directory_generation_is_complete(&restore).await;
                // Mirror `recover_restore_journal`: before the first rename the
                // intact live generation is authoritative (a stale rollback
                // must never replace it); after live_renamed the retained
                // rollback generation is.
                let generation_ready = if swap_started {
                    if rollback_complete {
                        adopt_generation(&rollback, &live).await
                    } else if live_complete {
                        true
                    } else if restore_complete {
                        adopt_generation(&restore, &live).await
                    } else {
                        false
                    }
                } else if live_complete {
                    true
                } else if restore_complete {
                    adopt_generation(&restore, &live).await
                } else if rollback_complete {
                    adopt_generation(&rollback, &live).await
                } else {
                    false
                };
                let _ = remove_directory_if_present(&restore).await;
                if generation_ready {
                    // Reopen first; only clear the journal once the workspace is
                    // usable again, and never fail a successful rollback for a
                    // cleanup error.
                    match open_workspace_store(self, workspace_id, &live.join("harness.db")).await {
                        Ok(pool) => {
                            *state = WorkspaceStoreState::Available(pool);
                            if let Err(clear_error) = self.clear_restore_journal(workspace_id).await {
                                tracing::error!(
                                    workspace_id,
                                    error = %clear_error,
                                    "restore journal cleanup failed after rollback reopen"
                                );
                            }
                        }
                        Err(reopen_error) => {
                            tracing::error!(workspace_id, error = %reopen_error, "rolled-back store reopen failed");
                            *state = WorkspaceStoreState::Unavailable;
                        }
                    }
                } else {
                    *state = WorkspaceStoreState::Unavailable;
                }
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        path::{Path, PathBuf},
        sync::{Arc, atomic::Ordering},
        time::Duration,
    };

    use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
    use tokio::time::timeout;

    use super::{WorkspaceStoreTestHooks, sidecar_path};
    use crate::Db;

    struct HarnessWorkspaceFixture {
        root: PathBuf,
        db: Db,
    }

    impl HarnessWorkspaceFixture {
        async fn new(workspace_ids: &[&str]) -> Self {
            let root = std::env::temp_dir().join(format!("tinyiothub-harness-workspace-{}", uuid::Uuid::new_v4()));
            let pool = crate::test_helpers::test_pool().await;
            let mut db = Db::new(pool);
            db.workspace_store_root = root.join("data/agents");
            crate::seed::seed_system(&db).await.expect("seed platform registry");
            for workspace_id in workspace_ids {
                sqlx::query(
                    "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) VALUES (?, 'workspace', 'tenant-default-001', 'now', 'now')",
                )
                .bind(workspace_id)
                .execute(db.pool())
                .await
                .expect("insert workspace");
            }
            Self { root, db }
        }

        fn database_path(&self, workspace_id: &str) -> PathBuf {
            self.root
                .join("data/agents")
                .join(workspace_id)
                .join("harness/harness.db")
        }

        async fn close(self) {
            self.db.pool().close().await;
            let _ = tokio::fs::remove_dir_all(&self.root).await;
        }
    }

    async fn create_conflicting_workspace_database(path: &Path) {
        tokio::fs::create_dir_all(path.parent().expect("database parent"))
            .await
            .expect("create harness directory");
        let pool = SqlitePool::connect(&format!("sqlite:{}?mode=rwc", path.display()))
            .await
            .expect("open conflicting database");
        sqlx::query(
            "CREATE TABLE _sqlx_migrations (
                version BIGINT PRIMARY KEY,
                description TEXT NOT NULL,
                installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                success BOOLEAN NOT NULL,
                checksum BLOB NOT NULL,
                execution_time BIGINT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .expect("create migration table");
        sqlx::query(
            "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) VALUES (20261103000001, 'harness_workspace', 1, X'00', 0)",
        )
        .execute(&pool)
        .await
        .expect("insert conflicting migration checksum");
        pool.close().await;
    }

    async fn create_probe_table(pool: &SqlitePool) {
        sqlx::query("CREATE TABLE IF NOT EXISTS harness_workspace (id INTEGER PRIMARY KEY)")
            .execute(pool)
            .await
            .expect("create lifecycle probe table");
    }

    #[tokio::test]
    async fn harness_workspace_create_reopen_and_pragmas() {
        let fixture = HarnessWorkspaceFixture::new(&["ws-create"]).await;
        let path = fixture.database_path("ws-create");

        fixture
            .db
            .create_workspace_store("ws-create")
            .await
            .expect("create workspace store");
        assert_eq!(path, fixture.root.join("data/agents/ws-create/harness/harness.db"));
        assert!(path.is_file());

        let pool = fixture.db.workspace_pool("ws-create").await.expect("workspace pool");
        let journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(&pool)
            .await
            .expect("journal mode");
        let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
            .fetch_one(&pool)
            .await
            .expect("foreign keys");
        assert_eq!(journal_mode, "wal");
        assert_eq!(foreign_keys, 1);
        let migration_version: i64 =
            sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE version = 20261103000001")
                .fetch_one(&pool)
                .await
                .expect("harness migration is recorded");
        assert_eq!(migration_version, 20261103000001);
        create_probe_table(&pool).await;
        sqlx::query("INSERT INTO harness_workspace (id) VALUES (1)")
            .execute(&pool)
            .await
            .expect("write workspace store");

        fixture
            .db
            .close_workspace_store("ws-create")
            .await
            .expect("close workspace store");
        let error = fixture
            .db
            .workspace_pool("ws-create")
            .await
            .expect_err("closed store must not reopen implicitly");
        assert!(error.to_string().contains("closed"), "unexpected error: {error}");

        fixture
            .db
            .create_workspace_store("ws-create")
            .await
            .expect("explicitly reopen workspace store");
        let reopened = fixture.db.workspace_pool("ws-create").await.expect("reopened pool");
        let id: i64 = sqlx::query_scalar("SELECT id FROM harness_workspace")
            .fetch_one(&reopened)
            .await
            .expect("read reopened workspace store");
        assert_eq!(id, 1);

        fixture
            .db
            .close_workspace_store("ws-create")
            .await
            .expect("close reopened store");
        fixture.close().await;
    }

    #[tokio::test]
    async fn harness_workspace_rejects_unknown_and_traversing_ids() {
        let fixture = HarnessWorkspaceFixture::new(&[]).await;

        fixture
            .db
            .workspace_pool("ws-unknown")
            .await
            .expect_err("unknown workspace must be rejected");
        fixture
            .db
            .create_workspace_store("../outside")
            .await
            .expect_err("path traversal must be rejected");
        assert!(!fixture.root.join("data/outside").exists());

        fixture.close().await;
    }

    #[tokio::test]
    async fn harness_workspace_migration_failure_is_isolated() {
        let fixture = HarnessWorkspaceFixture::new(&["ws-bad", "ws-good"]).await;
        create_conflicting_workspace_database(&fixture.database_path("ws-bad")).await;

        fixture
            .db
            .create_workspace_store("ws-bad")
            .await
            .expect_err("conflicting migration must fail");
        let error = fixture
            .db
            .workspace_pool("ws-bad")
            .await
            .expect_err("failed workspace must stay unavailable");
        assert!(error.to_string().contains("unavailable"), "unexpected error: {error}");

        fixture
            .db
            .create_workspace_store("ws-good")
            .await
            .expect("another workspace must still migrate");
        let pool = fixture.db.workspace_pool("ws-good").await.expect("good workspace pool");
        create_probe_table(&pool).await;
        sqlx::query("INSERT INTO harness_workspace (id) VALUES (1)")
            .execute(&pool)
            .await
            .expect("another workspace must still write");
        let id: i64 = sqlx::query_scalar("SELECT id FROM harness_workspace")
            .fetch_one(&pool)
            .await
            .expect("another workspace must still read");
        assert_eq!(id, 1);

        fixture
            .db
            .close_workspace_store("ws-good")
            .await
            .expect("close good store");
        fixture.close().await;
    }

    #[tokio::test]
    async fn harness_workspace_slow_open_does_not_block_another_workspace() {
        let mut fixture = HarnessWorkspaceFixture::new(&["ws-slow", "ws-fast"]).await;
        let hooks = Arc::new(WorkspaceStoreTestHooks {
            gated_workspace_id: "ws-slow".to_string(),
            ..Default::default()
        });
        fixture.db.workspace_store_test_hooks = Some(hooks.clone());

        let slow_db = fixture.db.clone();
        let slow = tokio::spawn(async move { slow_db.create_workspace_store("ws-slow").await });
        hooks.open_started.notified().await;

        let fast_db = fixture.db.clone();
        let fast_result = timeout(Duration::from_secs(2), async move {
            fast_db.create_workspace_store("ws-fast").await?;
            let pool = fast_db.workspace_pool("ws-fast").await?;
            create_probe_table(&pool).await;
            sqlx::query("INSERT INTO harness_workspace (id) VALUES (1)")
                .execute(&pool)
                .await
                .expect("write fast workspace");
            let id: i64 = sqlx::query_scalar("SELECT id FROM harness_workspace")
                .fetch_one(&pool)
                .await
                .expect("read fast workspace");
            assert_eq!(id, 1);
            Ok::<(), crate::DbError>(())
        })
        .await;

        hooks.open_release.notify_one();
        slow.await.expect("slow task").expect("slow workspace open");
        fixture
            .db
            .close_workspace_store("ws-slow")
            .await
            .expect("close slow store");
        if matches!(fast_result, Ok(Ok(()))) {
            fixture
                .db
                .close_workspace_store("ws-fast")
                .await
                .expect("close fast store");
        }
        fixture.close().await;

        fast_result
            .expect("fast workspace was blocked by slow workspace open")
            .expect("fast workspace lifecycle");
    }

    #[tokio::test]
    async fn harness_workspace_failed_reopen_preserves_closed_state() {
        let mut fixture = HarnessWorkspaceFixture::new(&["ws-reopen"]).await;
        let hooks = Arc::new(WorkspaceStoreTestHooks::default());
        fixture.db.workspace_store_test_hooks = Some(hooks.clone());
        fixture
            .db
            .create_workspace_store("ws-reopen")
            .await
            .expect("create store");
        fixture
            .db
            .close_workspace_store("ws-reopen")
            .await
            .expect("close store");

        hooks.fail_open.store(true, Ordering::SeqCst);
        fixture
            .db
            .create_workspace_store("ws-reopen")
            .await
            .expect_err("injected reopen failure");
        let error = fixture
            .db
            .workspace_pool("ws-reopen")
            .await
            .expect_err("failed reopen must not permit implicit access");
        fixture.close().await;

        assert!(
            error.to_string().contains("closed"),
            "closed tombstone was lost: {error}"
        );
    }

    async fn write_generation(directory: &Path, marker: &str) {
        tokio::fs::create_dir_all(directory)
            .await
            .expect("create generation directory");
        let path = directory.join("harness.db");
        let pool = SqlitePool::connect(&format!("sqlite:{}?mode=rwc", path.display()))
            .await
            .expect("open generation database");
        // Apply the real harness migrations so the generation carries the
        // schema marker required by the completeness check and can be opened
        // through the normal workspace path without migration conflicts.
        super::harness_migrator().run(&pool).await.expect("migrate generation");
        sqlx::query("CREATE TABLE generation_marker (value TEXT NOT NULL)")
            .execute(&pool)
            .await
            .expect("create marker table");
        sqlx::query("INSERT INTO generation_marker (value) VALUES (?)")
            .bind(marker)
            .execute(&pool)
            .await
            .expect("write marker");
        pool.close().await;
    }

    #[tokio::test]
    async fn final_review_workspace_migrator_tolerates_newer_applied_migrations() {
        let pool = SqlitePool::connect("sqlite::memory:").await.expect("memory db");
        super::harness_migrator().run(&pool).await.expect("current migrations");
        sqlx::query(
            "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) \
             VALUES (99999999999999, 'future_rollback_fixture', 1, X'00', 0)",
        )
        .execute(&pool)
        .await
        .expect("insert newer applied migration");
        super::harness_migrator()
            .run(&pool)
            .await
            .expect("older binary must reopen a newer workspace database");
        pool.close().await;
    }

    async fn read_marker(directory: &Path) -> Option<String> {
        let path = directory.join("harness.db");
        if !tokio::fs::try_exists(&path).await.unwrap_or(false) {
            return None;
        }
        let pool = SqlitePool::connect(&format!("sqlite:{}?mode=ro", path.display()))
            .await
            .ok()?;
        let value = sqlx::query_scalar::<_, String>("SELECT value FROM generation_marker")
            .fetch_one(&pool)
            .await
            .ok();
        pool.close().await;
        value
    }

    fn workspace_base(root: &Path, workspace_id: &str) -> PathBuf {
        root.join("data/agents").join(workspace_id)
    }

    async fn insert_journal(fixture: &HarnessWorkspaceFixture, workspace_id: &str, phase: &str) {
        let base = workspace_base(&fixture.root, workspace_id);
        sqlx::query(
            "INSERT INTO harness_restore_journal
                (workspace_id, epoch, phase, backup_path, rollback_path, restore_path, updated_at)
             VALUES (?, 1, ?, ?, ?, ?, '2026-11-05T00:00:00Z')",
        )
        .bind(workspace_id)
        .bind(phase)
        .bind(base.join("harness/backups/snapshot.db").to_string_lossy().to_string())
        .bind(base.join("harness.rollback").to_string_lossy().to_string())
        .bind(base.join("harness.restore").to_string_lossy().to_string())
        .execute(fixture.db.pool())
        .await
        .expect("insert restore journal");
    }

    async fn journal_count(fixture: &HarnessWorkspaceFixture, workspace_id: &str) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM harness_restore_journal WHERE workspace_id = ?")
            .bind(workspace_id)
            .fetch_one(fixture.db.pool())
            .await
            .expect("journal count")
    }

    #[tokio::test]
    async fn restore_journal_crash_before_rename_keeps_live_generation() {
        let fixture = HarnessWorkspaceFixture::new(&["ws-journal-before"]).await;
        let base = workspace_base(&fixture.root, "ws-journal-before");
        write_generation(&base.join("harness"), "live-old").await;
        write_generation(&base.join("harness.restore"), "restore-new").await;
        insert_journal(&fixture, "ws-journal-before", "staged").await;

        let outcomes = fixture.db.recover_restore_journals().await.expect("recover journal");
        assert_eq!(outcomes.len(), 1);
        assert_eq!(read_marker(&base.join("harness")).await.as_deref(), Some("live-old"));
        assert!(
            !tokio::fs::try_exists(base.join("harness.restore"))
                .await
                .expect("staging")
        );
        assert_eq!(journal_count(&fixture, "ws-journal-before").await, 0);
        let pool = fixture
            .db
            .workspace_pool("ws-journal-before")
            .await
            .expect("live store");
        pool.close().await;
        fixture.close().await;
    }

    #[tokio::test]
    async fn restore_journal_crash_between_renames_finishes_forward_when_restore_is_complete() {
        let fixture = HarnessWorkspaceFixture::new(&["ws-journal-between"]).await;
        let base = workspace_base(&fixture.root, "ws-journal-between");
        write_generation(&base.join("harness.rollback"), "live-old").await;
        write_generation(&base.join("harness.restore"), "restore-new").await;
        insert_journal(&fixture, "ws-journal-between", "live_renamed").await;

        fixture.db.recover_restore_journals().await.expect("recover journal");
        assert_eq!(read_marker(&base.join("harness")).await.as_deref(), Some("restore-new"));
        assert!(
            !tokio::fs::try_exists(base.join("harness.rollback"))
                .await
                .expect("rollback")
        );
        assert_eq!(journal_count(&fixture, "ws-journal-between").await, 0);
        fixture.close().await;
    }

    #[tokio::test]
    async fn restore_journal_crash_between_renames_rolls_back_when_restore_is_missing() {
        let fixture = HarnessWorkspaceFixture::new(&["ws-journal-missing"]).await;
        let base = workspace_base(&fixture.root, "ws-journal-missing");
        write_generation(&base.join("harness.rollback"), "live-old").await;
        insert_journal(&fixture, "ws-journal-missing", "live_renamed").await;

        fixture.db.recover_restore_journals().await.expect("recover journal");
        assert_eq!(read_marker(&base.join("harness")).await.as_deref(), Some("live-old"));
        assert_eq!(journal_count(&fixture, "ws-journal-missing").await, 0);
        fixture.close().await;
    }

    #[tokio::test]
    async fn restore_journal_crash_after_rename_keeps_the_new_complete_generation() {
        let fixture = HarnessWorkspaceFixture::new(&["ws-journal-after"]).await;
        let base = workspace_base(&fixture.root, "ws-journal-after");
        write_generation(&base.join("harness"), "restore-new").await;
        write_generation(&base.join("harness.rollback"), "live-old").await;
        insert_journal(&fixture, "ws-journal-after", "swapped").await;

        fixture.db.recover_restore_journals().await.expect("recover journal");
        assert_eq!(read_marker(&base.join("harness")).await.as_deref(), Some("restore-new"));
        assert!(
            !tokio::fs::try_exists(base.join("harness.rollback"))
                .await
                .expect("rollback")
        );
        assert_eq!(journal_count(&fixture, "ws-journal-after").await, 0);
        fixture.close().await;
    }

    // Crash window: live->rollback rename completed but the journal still
    // says "staged". The complete restore generation must still win.
    #[tokio::test]
    async fn restore_journal_crash_after_first_rename_before_phase_write_finishes_forward() {
        let fixture = HarnessWorkspaceFixture::new(&["ws-journal-gap-first"]).await;
        let base = workspace_base(&fixture.root, "ws-journal-gap-first");
        write_generation(&base.join("harness.rollback"), "live-old").await;
        write_generation(&base.join("harness.restore"), "restore-new").await;
        insert_journal(&fixture, "ws-journal-gap-first", "staged").await;

        let outcomes = fixture.db.recover_restore_journals().await.expect("recover journal");
        assert_eq!(outcomes.len(), 1);
        assert_eq!(read_marker(&base.join("harness")).await.as_deref(), Some("restore-new"));
        assert!(
            !tokio::fs::try_exists(base.join("harness.rollback"))
                .await
                .expect("rollback")
        );
        assert_eq!(journal_count(&fixture, "ws-journal-gap-first").await, 0);
        fixture.close().await;
    }

    // Crash window: restore->live rename completed but the journal still says
    // "live_renamed". The new live generation is complete and must be kept.
    #[tokio::test]
    async fn restore_journal_crash_after_second_rename_before_phase_write_keeps_new_live() {
        let fixture = HarnessWorkspaceFixture::new(&["ws-journal-gap-second"]).await;
        let base = workspace_base(&fixture.root, "ws-journal-gap-second");
        write_generation(&base.join("harness"), "restore-new").await;
        write_generation(&base.join("harness.rollback"), "live-old").await;
        insert_journal(&fixture, "ws-journal-gap-second", "live_renamed").await;

        fixture.db.recover_restore_journals().await.expect("recover journal");
        assert_eq!(read_marker(&base.join("harness")).await.as_deref(), Some("restore-new"));
        assert!(
            !tokio::fs::try_exists(base.join("harness.rollback"))
                .await
                .expect("rollback")
        );
        assert_eq!(journal_count(&fixture, "ws-journal-gap-second").await, 0);
        fixture.close().await;
    }

    // Integrity selection also applies before the first rename: a corrupt live
    // directory is never opened when the staged restore generation is intact.
    #[tokio::test]
    async fn restore_journal_prepared_with_corrupt_live_selects_complete_restore() {
        let fixture = HarnessWorkspaceFixture::new(&["ws-journal-prepared"]).await;
        let base = workspace_base(&fixture.root, "ws-journal-prepared");
        tokio::fs::create_dir_all(base.join("harness"))
            .await
            .expect("create corrupt live directory");
        tokio::fs::write(base.join("harness/harness.db"), b"not a sqlite database")
            .await
            .expect("corrupt live database");
        write_generation(&base.join("harness.restore"), "restore-new").await;
        insert_journal(&fixture, "ws-journal-prepared", "prepared").await;

        fixture.db.recover_restore_journals().await.expect("recover journal");
        assert_eq!(read_marker(&base.join("harness")).await.as_deref(), Some("restore-new"));
        assert_eq!(journal_count(&fixture, "ws-journal-prepared").await, 0);
        fixture.close().await;
    }

    #[tokio::test]
    async fn restore_journal_with_every_generation_missing_keeps_the_journal() {
        let fixture = HarnessWorkspaceFixture::new(&["ws-journal-empty"]).await;
        insert_journal(&fixture, "ws-journal-empty", "swapped").await;

        let outcomes = fixture.db.recover_restore_journals().await.expect("recover journal");
        assert_eq!(outcomes.len(), 1);
        assert_eq!(
            outcomes[0].1,
            super::RestoreRecovery::Unavailable,
            "missing generations must stay unavailable"
        );
        assert_eq!(journal_count(&fixture, "ws-journal-empty").await, 1);
        let error = fixture
            .db
            .workspace_pool("ws-journal-empty")
            .await
            .expect_err("missing generations never open");
        assert!(error.to_string().contains("unavailable"), "unexpected error: {error}");
        fixture.close().await;
    }

    // SQLite reports `integrity_check = ok` for a zero-byte file (it is a
    // valid empty database). Such a generation is not complete: selecting it
    // would silently replace a good retained generation with no harness data.
    #[tokio::test]
    async fn restore_journal_zero_length_live_never_beats_a_complete_rollback() {
        let fixture = HarnessWorkspaceFixture::new(&["ws-journal-zero-live"]).await;
        let base = workspace_base(&fixture.root, "ws-journal-zero-live");
        tokio::fs::create_dir_all(base.join("harness"))
            .await
            .expect("create live directory");
        tokio::fs::write(base.join("harness/harness.db"), b"")
            .await
            .expect("zero-length live database");
        write_generation(&base.join("harness.rollback"), "live-old").await;
        insert_journal(&fixture, "ws-journal-zero-live", "swapped").await;

        fixture.db.recover_restore_journals().await.expect("recover journal");
        assert_eq!(
            read_marker(&base.join("harness")).await.as_deref(),
            Some("live-old"),
            "a zero-length live generation must not win over a complete rollback"
        );
        assert_eq!(journal_count(&fixture, "ws-journal-zero-live").await, 0);
        fixture.close().await;
    }

    #[tokio::test]
    async fn restore_journal_corruption_selects_a_complete_generation() {
        let fixture = HarnessWorkspaceFixture::new(&["ws-journal-corrupt"]).await;
        let base = workspace_base(&fixture.root, "ws-journal-corrupt");
        write_generation(&base.join("harness"), "live-old").await;
        write_generation(&base.join("harness.restore"), "restore-new").await;
        insert_journal(&fixture, "ws-journal-corrupt", "not-a-real-phase").await;

        fixture
            .db
            .recover_restore_journals()
            .await
            .expect("recover corrupt journal");
        assert_eq!(read_marker(&base.join("harness")).await.as_deref(), Some("live-old"));
        assert_eq!(journal_count(&fixture, "ws-journal-corrupt").await, 0);
        // A live generation that never had WAL mixed in stays self-contained.
        assert!(
            !tokio::fs::try_exists(base.join("harness/harness.db-wal"))
                .await
                .expect("wal")
        );
        fixture.close().await;
    }

    #[tokio::test]
    async fn restore_journal_corrupt_live_rolls_back_to_the_retained_generation() {
        let fixture = HarnessWorkspaceFixture::new(&["ws-journal-partial"]).await;
        let base = workspace_base(&fixture.root, "ws-journal-partial");
        tokio::fs::create_dir_all(base.join("harness"))
            .await
            .expect("create live directory");
        tokio::fs::write(base.join("harness/harness.db"), b"not a sqlite database")
            .await
            .expect("corrupt live database");
        write_generation(&base.join("harness.rollback"), "live-old").await;
        insert_journal(&fixture, "ws-journal-partial", "garbage").await;

        fixture
            .db
            .recover_restore_journals()
            .await
            .expect("recover corrupt journal");
        assert_eq!(read_marker(&base.join("harness")).await.as_deref(), Some("live-old"));
        fixture.close().await;
    }

    #[tokio::test]
    async fn restore_journal_without_any_complete_generation_keeps_workspace_unavailable() {
        let fixture = HarnessWorkspaceFixture::new(&["ws-journal-broken", "ws-journal-other"]).await;
        let broken = workspace_base(&fixture.root, "ws-journal-broken");
        tokio::fs::create_dir_all(broken.join("harness"))
            .await
            .expect("create broken live");
        tokio::fs::write(broken.join("harness/harness.db"), b"corrupt")
            .await
            .expect("corrupt generation");
        insert_journal(&fixture, "ws-journal-broken", "live_renamed").await;

        fixture.db.recover_restore_journals().await.expect("recover journal");
        let error = fixture
            .db
            .workspace_pool("ws-journal-broken")
            .await
            .expect_err("broken workspace stays unavailable");
        assert!(error.to_string().contains("unavailable"), "unexpected error: {error}");
        assert_eq!(journal_count(&fixture, "ws-journal-broken").await, 1);
        assert_eq!(read_marker(&broken.join("harness")).await, None);

        // Another workspace is unaffected and still opens.
        let other = workspace_base(&fixture.root, "ws-journal-other");
        write_generation(&other.join("harness"), "other").await;
        let pool = fixture
            .db
            .workspace_pool("ws-journal-other")
            .await
            .expect("other store");
        pool.close().await;
        fixture.close().await;
    }

    #[tokio::test]
    async fn restore_workspace_store_swaps_complete_directories_and_clears_the_journal() {
        let fixture = HarnessWorkspaceFixture::new(&["ws-restore"]).await;
        fixture
            .db
            .create_workspace_store("ws-restore")
            .await
            .expect("create store");
        let base = workspace_base(&fixture.root, "ws-restore");
        let backups = base.join("harness/backups");
        tokio::fs::create_dir_all(&backups).await.expect("backups directory");
        write_generation(&base.join("harness/backup-source"), "live-old").await;
        write_generation(&base.join("harness/backup-source-new"), "restore-new").await;
        let backup = backups.join("snapshot.db");
        tokio::fs::copy(base.join("harness/backup-source-new/harness.db"), &backup)
            .await
            .expect("copy backup");

        fixture
            .db
            .restore_workspace_store("ws-restore", &backup)
            .await
            .expect("restore workspace store");
        assert_eq!(read_marker(&base.join("harness")).await.as_deref(), Some("restore-new"));
        assert_eq!(journal_count(&fixture, "ws-restore").await, 0);
        let pool = fixture.db.workspace_pool("ws-restore").await.expect("restored store");
        pool.close().await;
        fixture.close().await;
    }

    #[tokio::test]
    async fn restore_workspace_store_rejects_a_corrupt_backup_without_touching_live() {
        let fixture = HarnessWorkspaceFixture::new(&["ws-restore-bad"]).await;
        fixture
            .db
            .create_workspace_store("ws-restore-bad")
            .await
            .expect("create store");
        let base = workspace_base(&fixture.root, "ws-restore-bad");
        let backups = base.join("harness/backups");
        tokio::fs::create_dir_all(&backups).await.expect("backups directory");
        let backup = backups.join("corrupt.db");
        tokio::fs::write(&backup, b"not a database")
            .await
            .expect("write corrupt backup");
        let marker: String =
            sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'effect_intents'")
                .fetch_one(&fixture.db.workspace_pool("ws-restore-bad").await.expect("live store"))
                .await
                .expect("live schema survived");
        assert_eq!(marker, "effect_intents");

        fixture
            .db
            .restore_workspace_store("ws-restore-bad", &backup)
            .await
            .expect_err("corrupt backup must be rejected");
        assert_eq!(journal_count(&fixture, "ws-restore-bad").await, 0);
        let pool = fixture.db.workspace_pool("ws-restore-bad").await.expect("live store");
        let tables: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'effect_intents'")
                .fetch_one(&pool)
                .await
                .expect("live schema intact");
        assert_eq!(tables, 1);
        fixture.close().await;
    }

    // A failure after the swap must roll back to the retained generation and
    // reopen it; leaving the workspace permanently `Unavailable` after a
    // successful rollback would strand a healthy store.
    #[tokio::test]
    async fn restore_workspace_store_failure_reopens_the_rolled_back_generation() {
        let mut fixture = HarnessWorkspaceFixture::new(&["ws-restore-fail"]).await;
        fixture
            .db
            .create_workspace_store("ws-restore-fail")
            .await
            .expect("create store");
        let base = workspace_base(&fixture.root, "ws-restore-fail");
        let backups = base.join("harness/backups");
        tokio::fs::create_dir_all(&backups).await.expect("backups directory");
        let backup = backups.join("snapshot.db");
        sqlx::query("VACUUM INTO ?")
            .bind(backup.to_string_lossy().to_string())
            .execute(&fixture.db.workspace_pool("ws-restore-fail").await.expect("live store"))
            .await
            .expect("capture self-contained backup");

        let hooks = Arc::new(WorkspaceStoreTestHooks::default());
        fixture.db.workspace_store_test_hooks = Some(hooks.clone());
        hooks.fail_open.store(true, Ordering::SeqCst);

        let error = fixture
            .db
            .restore_workspace_store("ws-restore-fail", &backup)
            .await
            .expect_err("injected post-swap open failure must roll back");
        assert!(
            error.to_string().contains("injected workspace open failure"),
            "unexpected error: {error}"
        );
        assert_eq!(journal_count(&fixture, "ws-restore-fail").await, 0);

        let pool = fixture
            .db
            .workspace_pool("ws-restore-fail")
            .await
            .expect("rolled-back generation reopens");
        let tables: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'effect_intents'")
                .fetch_one(&pool)
                .await
                .expect("rolled-back schema intact");
        assert_eq!(tables, 1);
        pool.close().await;
        fixture.close().await;
    }

    // A failure before the live directory is replaced (here: the staging path
    // is a file) must keep the intact live generation usable in-process.
    #[tokio::test]
    async fn restore_workspace_store_failure_before_rename_keeps_the_intact_live_generation() {
        let fixture = HarnessWorkspaceFixture::new(&["ws-restore-early"]).await;
        fixture
            .db
            .create_workspace_store("ws-restore-early")
            .await
            .expect("create store");
        let base = workspace_base(&fixture.root, "ws-restore-early");
        let backups = base.join("harness/backups");
        tokio::fs::create_dir_all(&backups).await.expect("backups directory");
        let backup = backups.join("snapshot.db");
        sqlx::query("VACUUM INTO ?")
            .bind(backup.to_string_lossy().to_string())
            .execute(&fixture.db.workspace_pool("ws-restore-early").await.expect("live store"))
            .await
            .expect("capture self-contained backup");
        // `remove_dir_all` on a regular file fails before any rename, leaving
        // the live generation untouched.
        tokio::fs::write(base.join("harness.restore"), b"block staging cleanup")
            .await
            .expect("block staging path");

        fixture
            .db
            .restore_workspace_store("ws-restore-early", &backup)
            .await
            .expect_err("staging cleanup failure aborts the restore");
        assert_eq!(journal_count(&fixture, "ws-restore-early").await, 0);

        let pool = fixture
            .db
            .workspace_pool("ws-restore-early")
            .await
            .expect("intact live generation stays usable");
        let tables: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'effect_intents'")
                .fetch_one(&pool)
                .await
                .expect("live schema intact");
        assert_eq!(tables, 1);
        pool.close().await;
        fixture.close().await;
    }

    // M1: before the first rename the live generation is authoritative. A
    // stale rollback from an older attempt must never replace it.
    #[tokio::test]
    async fn restore_workspace_store_stale_rollback_does_not_replace_intact_live_before_rename() {
        let fixture = HarnessWorkspaceFixture::new(&["ws-restore-stale"]).await;
        fixture
            .db
            .create_workspace_store("ws-restore-stale")
            .await
            .expect("create store");
        let base = workspace_base(&fixture.root, "ws-restore-stale");
        let live = fixture.db.workspace_pool("ws-restore-stale").await.expect("live pool");
        sqlx::query("CREATE TABLE generation_marker (value TEXT NOT NULL)")
            .execute(&live)
            .await
            .expect("live marker table");
        sqlx::query("INSERT INTO generation_marker (value) VALUES ('live')")
            .execute(&live)
            .await
            .expect("live marker");
        let backups = base.join("harness/backups");
        tokio::fs::create_dir_all(&backups).await.expect("backups directory");
        let backup = backups.join("snapshot.db");
        sqlx::query("VACUUM INTO ?")
            .bind(backup.to_string_lossy().to_string())
            .execute(&live)
            .await
            .expect("capture self-contained backup");
        drop(live);
        write_generation(&base.join("harness.rollback"), "stale").await;
        // Block staging cleanup before any rename.
        tokio::fs::write(base.join("harness.restore"), b"block staging cleanup")
            .await
            .expect("block staging path");

        fixture
            .db
            .restore_workspace_store("ws-restore-stale", &backup)
            .await
            .expect_err("staging cleanup failure aborts before any rename");
        assert_eq!(journal_count(&fixture, "ws-restore-stale").await, 0);
        assert_eq!(
            read_marker(&base.join("harness")).await.as_deref(),
            Some("live"),
            "a stale rollback must not replace the intact live generation"
        );
        let pool = fixture
            .db
            .workspace_pool("ws-restore-stale")
            .await
            .expect("live generation stays usable");
        pool.close().await;
        fixture.close().await;
    }

    // M2: a WAL sibling means the backup is not self-contained; copying only
    // the main file would drop committed frames.
    #[tokio::test]
    async fn restore_workspace_store_rejects_a_wal_bearing_backup() {
        let fixture = HarnessWorkspaceFixture::new(&["ws-restore-wal"]).await;
        fixture
            .db
            .create_workspace_store("ws-restore-wal")
            .await
            .expect("create store");
        let base = workspace_base(&fixture.root, "ws-restore-wal");
        let backups = base.join("harness/backups");
        tokio::fs::create_dir_all(&backups).await.expect("backups directory");
        let source_dir = base.join("wal-source");
        tokio::fs::create_dir_all(&source_dir).await.expect("wal source");
        let source_db = source_dir.join("harness.db");
        let source = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&format!("sqlite:{}?mode=rwc", source_db.display()))
            .await
            .expect("wal source database");
        sqlx::query_scalar::<_, String>("PRAGMA journal_mode = WAL")
            .fetch_one(&source)
            .await
            .expect("wal mode");
        sqlx::query("PRAGMA wal_autocheckpoint = 0")
            .execute(&source)
            .await
            .expect("disable autocheckpoint");
        sqlx::query("CREATE TABLE effect_intents (effect_id TEXT PRIMARY KEY)")
            .execute(&source)
            .await
            .expect("harness marker");
        sqlx::query("INSERT INTO effect_intents (effect_id) VALUES ('effect-wal')")
            .execute(&source)
            .await
            .expect("wal commit");
        let backup = backups.join("snapshot.db");
        let source_wal = sidecar_path(&source_db, "-wal");
        assert!(
            tokio::fs::try_exists(&source_wal).await.expect("source wal"),
            "test fixture must leave a WAL sidecar"
        );
        tokio::fs::copy(&source_db, &backup).await.expect("copy main file");
        tokio::fs::copy(&source_wal, sidecar_path(&backup, "-wal"))
            .await
            .expect("copy wal sidecar");
        source.close().await;

        let error = fixture
            .db
            .restore_workspace_store("ws-restore-wal", &backup)
            .await
            .expect_err("WAL-bearing backup must be rejected");
        assert!(
            error.to_string().contains("not self-contained"),
            "unexpected error: {error}"
        );
        assert_eq!(journal_count(&fixture, "ws-restore-wal").await, 0);
        let pool = fixture
            .db
            .workspace_pool("ws-restore-wal")
            .await
            .expect("live store untouched");
        pool.close().await;
        fixture.close().await;
    }

    // L5: a valid SQLite file without the harness schema is not a generation.
    #[tokio::test]
    async fn restore_workspace_store_rejects_a_foreign_sqlite_database() {
        let fixture = HarnessWorkspaceFixture::new(&["ws-restore-foreign"]).await;
        fixture
            .db
            .create_workspace_store("ws-restore-foreign")
            .await
            .expect("create store");
        let base = workspace_base(&fixture.root, "ws-restore-foreign");
        let backups = base.join("harness/backups");
        tokio::fs::create_dir_all(&backups).await.expect("backups directory");
        let backup = backups.join("foreign.db");
        let foreign = SqlitePool::connect(&format!("sqlite:{}?mode=rwc", backup.display()))
            .await
            .expect("foreign database");
        sqlx::query("CREATE TABLE foreign_table (value TEXT NOT NULL)")
            .execute(&foreign)
            .await
            .expect("foreign table");
        foreign.close().await;

        let error = fixture
            .db
            .restore_workspace_store("ws-restore-foreign", &backup)
            .await
            .expect_err("foreign SQLite file must be rejected");
        assert!(
            error.to_string().contains("not a complete harness generation"),
            "unexpected error: {error}"
        );
        assert_eq!(journal_count(&fixture, "ws-restore-foreign").await, 0);
        let pool = fixture
            .db
            .workspace_pool("ws-restore-foreign")
            .await
            .expect("live store untouched");
        pool.close().await;
        fixture.close().await;
    }

    // L7: a real WAL-bearing generation must be selected and recover its
    // committed frames instead of being mistaken for an empty database. The
    // source is a real migrated workspace store so the adopted generation can
    // be reopened without synthetic-schema conflicts.
    #[tokio::test]
    async fn restore_journal_recovery_reads_a_real_wal_generation() {
        let fixture = HarnessWorkspaceFixture::new(&["ws-wal-generation", "ws-wal-source"]).await;
        fixture
            .db
            .create_workspace_store("ws-wal-source")
            .await
            .expect("source store");
        let source_base = workspace_base(&fixture.root, "ws-wal-source");
        let source_db = source_base.join("harness/harness.db");
        let source = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&format!("sqlite:{}?mode=rw", source_db.display()))
            .await
            .expect("wal source database");
        sqlx::query_scalar::<_, String>("PRAGMA journal_mode = WAL")
            .fetch_one(&source)
            .await
            .expect("wal mode");
        sqlx::query("PRAGMA wal_autocheckpoint = 0")
            .execute(&source)
            .await
            .expect("disable autocheckpoint");
        sqlx::query("CREATE TABLE generation_marker (value TEXT NOT NULL)")
            .execute(&source)
            .await
            .expect("generation marker");
        sqlx::query("INSERT INTO generation_marker (value) VALUES ('wal-recovered')")
            .execute(&source)
            .await
            .expect("marker commit");
        let source_wal = sidecar_path(&source_db, "-wal");
        assert!(
            tokio::fs::try_exists(&source_wal).await.expect("source wal"),
            "test fixture must leave a WAL sidecar"
        );
        let base = workspace_base(&fixture.root, "ws-wal-generation");
        let rollback = base.join("harness.rollback");
        tokio::fs::create_dir_all(&rollback).await.expect("rollback directory");
        tokio::fs::copy(&source_db, rollback.join("harness.db"))
            .await
            .expect("copy main generation");
        tokio::fs::copy(&source_wal, rollback.join("harness.db-wal"))
            .await
            .expect("copy wal generation");
        source.close().await;
        fixture
            .db
            .close_workspace_store("ws-wal-source")
            .await
            .expect("close source store");
        insert_journal(&fixture, "ws-wal-generation", "live_renamed").await;

        fixture
            .db
            .recover_restore_journals()
            .await
            .expect("recover WAL generation");
        assert_eq!(
            read_marker(&base.join("harness")).await.as_deref(),
            Some("wal-recovered")
        );
        assert_eq!(journal_count(&fixture, "ws-wal-generation").await, 0);
        let pool = fixture
            .db
            .workspace_pool("ws-wal-generation")
            .await
            .expect("recovered WAL generation");
        let marker: String = sqlx::query_scalar("SELECT value FROM generation_marker")
            .fetch_one(&pool)
            .await
            .expect("WAL-committed marker row");
        assert_eq!(marker, "wal-recovered", "WAL frames must be recovered, not dropped");
        pool.close().await;
        fixture.close().await;
    }
}
