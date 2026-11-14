//! Durable Assistant suppression state.
//!
//! Only generic version, cursor and fence semantics live here. Device, alarm,
//! skill and business skip rules stay in the Source adapters (Task 3+). The
//! state row lives in the per-workspace Harness database next to
//! `session_events`, so a skip commits its state, events and pending barriers
//! in one workspace transaction. Only the schedule configuration
//! (`assistant_schedules`) is platform state; no method claims atomicity
//! across the platform and workspace databases.

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::Value;
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, Sqlite, Transaction};
use tinyiothub_core::harness::{HarnessEvent, HarnessEventInput};

use crate::{
    Db,
    error::{DbError, Result},
};

/// Version of the generic Harness turn-state envelope (Task 1 contract).
pub const TURN_STATE_VERSION: i64 = 1;
/// Default reconciliation interval; a full Turn is forced at least this often.
pub const DEFAULT_RECONCILIATION_SECONDS: u32 = 1800;
/// Lower bound for a configured reconciliation interval.
pub const MIN_RECONCILIATION_SECONDS: u32 = 300;
/// Upper bound for a configured reconciliation interval.
pub const MAX_RECONCILIATION_SECONDS: u32 = 86_400;
/// First suppression config generation.
pub const DEFAULT_SUPPRESSION_CONFIG_VERSION: i64 = 1;

pub const OUTCOME_BASELINE_MISSING: &str = "baseline_missing";
pub const OUTCOME_FULL_IN_PROGRESS: &str = "full_in_progress";
pub const OUTCOME_FULL_COMPLETED: &str = "full_completed";
pub const OUTCOME_FULL_FAILED: &str = "full_failed";
pub const OUTCOME_SKIP_IN_PROGRESS: &str = "skip_in_progress";
pub const OUTCOME_SKIPPED: &str = "skipped";
pub const OUTCOME_AUDIT_FAILED: &str = "audit_failed";

/// Workspace-level suppression gate mode. The HTTP DTO lives elsewhere; this
/// enum is the persistence-level value used by schedule updates and commit
/// guards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuppressionMode {
    Off,
    Shadow,
    Canary,
    Enforce,
}

impl SuppressionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Shadow => "shadow",
            Self::Canary => "canary",
            Self::Enforce => "enforce",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "off" => Self::Off,
            "shadow" => Self::Shadow,
            "canary" => Self::Canary,
            "enforce" => Self::Enforce,
            _ => return None,
        })
    }

    /// Only canary/enforce may emit a real skip; shadow is observational.
    pub fn allows_skip(self) -> bool {
        matches!(self, Self::Canary | Self::Enforce)
    }
}

/// One persisted suppression row. `baseline_is_eligible` is the single
/// preflight answer to "may this baseline be compared against at all".
#[derive(Debug, Clone, PartialEq)]
pub struct SuppressionState {
    pub workspace_id: String,
    pub session_id: String,
    pub state_version: i64,
    pub config_version: i64,
    pub registry_version: Option<String>,
    pub control_cursor: i64,
    pub baseline_state_id: Option<String>,
    pub baseline_turn_state: Option<Value>,
    pub baseline_observed_at: Option<DateTime<Utc>>,
    pub baseline_installed_at: Option<DateTime<Utc>>,
    pub attempt_no: i64,
    pub latest_attempt_id: Option<String>,
    pub latest_outcome: String,
    pub full_attempt_in_progress: bool,
    pub full_failed: bool,
    pub audit_in_progress: bool,
    pub schedule_sync_pending: bool,
    pub projection_sync_pending: bool,
    pub notification_pending: bool,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl SuppressionState {
    /// In-memory shape for the first attempt of a session that has no row yet.
    pub fn initial(workspace_id: &str, session_id: &str) -> Self {
        let now = Utc::now();
        Self {
            workspace_id: workspace_id.to_string(),
            session_id: session_id.to_string(),
            state_version: 0,
            config_version: DEFAULT_SUPPRESSION_CONFIG_VERSION,
            registry_version: None,
            control_cursor: 0,
            baseline_state_id: None,
            baseline_turn_state: None,
            baseline_observed_at: None,
            baseline_installed_at: None,
            attempt_no: 0,
            latest_attempt_id: None,
            latest_outcome: OUTCOME_BASELINE_MISSING.to_string(),
            full_attempt_in_progress: false,
            full_failed: false,
            audit_in_progress: false,
            schedule_sync_pending: false,
            projection_sync_pending: false,
            notification_pending: false,
            last_error: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn latest_outcome(&self) -> &str {
        &self.latest_outcome
    }

    /// A baseline may only be compared when it was installed by a fully
    /// successful Turn, no later attempt failed, and every durable fence is
    /// clear. A committed skip keeps the same baseline eligible.
    pub fn baseline_is_eligible(&self) -> bool {
        self.baseline_state_id.is_some()
            && matches!(self.latest_outcome.as_str(), OUTCOME_FULL_COMPLETED | OUTCOME_SKIPPED)
            && !self.full_attempt_in_progress
            && !self.full_failed
            && !self.audit_in_progress
            && !self.schedule_sync_pending
            && !self.projection_sync_pending
            && !self.notification_pending
    }
}

/// A full attempt that has already durably marked `full_attempt_in_progress`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullAttempt {
    pub attempt_id: String,
    pub workspace_id: String,
    pub session_id: String,
    pub attempt_no: i64,
    pub expected_state_version: i64,
}

/// The all-available successful Turn result that may install a baseline.
#[derive(Debug, Clone, PartialEq)]
pub struct FullAttemptCompletion {
    pub state_id: String,
    pub state_version: i64,
    pub registry_version: String,
    pub config_version: i64,
    pub control_cursor: i64,
    pub turn_state: Value,
    pub observed_at: DateTime<Utc>,
}

/// A skip attempt that has durably marked `audit_in_progress`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkipAttempt {
    pub attempt_id: String,
    pub workspace_id: String,
    pub session_id: String,
    pub attempt_no: i64,
    pub expected_state_version: i64,
    pub expected_config_version: i64,
    pub expected_registry_version: String,
    pub expected_control_cursor: i64,
}

/// A committed skip plus the events appended to the caller's transaction.
#[derive(Debug, Clone, PartialEq)]
pub struct CommittedSkip {
    pub attempt_id: String,
    pub attempt_no: i64,
    pub events: Vec<HarnessEvent>,
    pub state: SuppressionState,
}

/// Which post-commit sync barrier has completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncPending {
    Schedule,
    Projection,
    Notification,
}

const STATE_BY_KEY_SQL: &str = "SELECT workspace_id, session_id, state_version, config_version, \
    registry_version, control_cursor, baseline_state_id, baseline_turn_state_json, \
    baseline_observed_at, baseline_installed_at, attempt_no, latest_attempt_id, latest_outcome, \
    full_attempt_in_progress, full_failed, audit_in_progress, schedule_sync_pending, \
    projection_sync_pending, notification_pending, last_error, created_at, updated_at \
    FROM suppression_state WHERE workspace_id = ? AND session_id = ?";

const STATE_BY_ATTEMPT_SQL: &str = "SELECT workspace_id, session_id, state_version, config_version, \
    registry_version, control_cursor, baseline_state_id, baseline_turn_state_json, \
    baseline_observed_at, baseline_installed_at, attempt_no, latest_attempt_id, latest_outcome, \
    full_attempt_in_progress, full_failed, audit_in_progress, schedule_sync_pending, \
    projection_sync_pending, notification_pending, last_error, created_at, updated_at \
    FROM suppression_state WHERE latest_attempt_id = ?";

fn format_ts(ts: DateTime<Utc>) -> String {
    ts.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn parse_ts(value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|parsed| parsed.with_timezone(&Utc))
        .map_err(|error| DbError::Internal(format!("invalid suppression timestamp: {error}")))
}

fn map_state_row(row: &SqliteRow) -> Result<SuppressionState> {
    let created_at: String = row.try_get("created_at")?;
    let updated_at: String = row.try_get("updated_at")?;
    let baseline_observed_at: Option<String> = row.try_get("baseline_observed_at")?;
    let baseline_installed_at: Option<String> = row.try_get("baseline_installed_at")?;
    let baseline_turn_state_json: Option<String> = row.try_get("baseline_turn_state_json")?;
    Ok(SuppressionState {
        workspace_id: row.try_get("workspace_id")?,
        session_id: row.try_get("session_id")?,
        state_version: row.try_get("state_version")?,
        config_version: row.try_get("config_version")?,
        registry_version: row.try_get("registry_version")?,
        control_cursor: row.try_get("control_cursor")?,
        baseline_state_id: row.try_get("baseline_state_id")?,
        baseline_turn_state: baseline_turn_state_json
            .as_deref()
            .map(serde_json::from_str)
            .transpose()?,
        baseline_observed_at: baseline_observed_at.as_deref().map(parse_ts).transpose()?,
        baseline_installed_at: baseline_installed_at.as_deref().map(parse_ts).transpose()?,
        attempt_no: row.try_get("attempt_no")?,
        latest_attempt_id: row.try_get("latest_attempt_id")?,
        latest_outcome: row.try_get("latest_outcome")?,
        full_attempt_in_progress: row.try_get::<i64, _>("full_attempt_in_progress")? != 0,
        full_failed: row.try_get::<i64, _>("full_failed")? != 0,
        audit_in_progress: row.try_get::<i64, _>("audit_in_progress")? != 0,
        schedule_sync_pending: row.try_get::<i64, _>("schedule_sync_pending")? != 0,
        projection_sync_pending: row.try_get::<i64, _>("projection_sync_pending")? != 0,
        notification_pending: row.try_get::<i64, _>("notification_pending")? != 0,
        last_error: row.try_get("last_error")?,
        created_at: parse_ts(&created_at)?,
        updated_at: parse_ts(&updated_at)?,
    })
}

fn validate_completion(completion: &FullAttemptCompletion) -> Result<()> {
    if completion.state_version != TURN_STATE_VERSION {
        return Err(DbError::Validation {
            message: format!(
                "turn state version {} is not {TURN_STATE_VERSION}",
                completion.state_version
            ),
        });
    }
    if completion.state_id.trim().is_empty() {
        return Err(DbError::Validation {
            message: "baseline state id must not be empty".to_string(),
        });
    }
    if completion.registry_version.trim().is_empty() {
        return Err(DbError::Validation {
            message: "baseline registryVersion must not be empty".to_string(),
        });
    }
    if completion.config_version < 1 {
        return Err(DbError::Validation {
            message: "baseline configVersion must be at least 1".to_string(),
        });
    }
    if completion.control_cursor < 0 {
        return Err(DbError::Validation {
            message: "baseline controlCursor must not be negative".to_string(),
        });
    }
    if !completion.turn_state.is_object() {
        return Err(DbError::Validation {
            message: "turn.state must be a JSON object".to_string(),
        });
    }
    Ok(())
}

fn attempt_id(workspace_id: &str, session_id: &str, attempt_no: i64) -> String {
    format!("{workspace_id}:{session_id}:attempt:{attempt_no}")
}

async fn state_by_attempt(db: &Db, workspace_id: &str, attempt_id: &str) -> Result<Option<SuppressionState>> {
    let pool = db.workspace_pool(workspace_id).await?;
    let row = sqlx::query(STATE_BY_ATTEMPT_SQL)
        .bind(attempt_id)
        .fetch_optional(&pool)
        .await?;
    row.as_ref().map(map_state_row).transpose()
}

impl Db {
    pub async fn load_suppression_state(
        &self,
        workspace_id: &str,
        session_id: &str,
    ) -> Result<Option<SuppressionState>> {
        let pool = self.workspace_pool(workspace_id).await?;
        let row = sqlx::query(STATE_BY_KEY_SQL)
            .bind(workspace_id)
            .bind(session_id)
            .fetch_optional(&pool)
            .await?;
        row.as_ref().map(map_state_row).transpose()
    }

    /// Durably fence a full Turn before any model work. The old baseline is
    /// immediately ineligible; a crashed attempt stays in progress until a
    /// recovery path settles it.
    ///
    /// An unresolved `audit_in_progress` fence never blocks full execution:
    /// only another full attempt (or a live audit settlement) does.
    pub async fn begin_full_attempt(&self, session_id: &str, baseline: &SuppressionState) -> Result<FullAttempt> {
        if baseline.session_id != session_id {
            return Err(DbError::Validation {
                message: "baseline session does not match the full attempt session".to_string(),
            });
        }
        let pool = self.workspace_pool(&baseline.workspace_id).await?;
        let now = Utc::now();
        let stamp = format_ts(now);
        sqlx::query(
            "INSERT OR IGNORE INTO suppression_state \
             (workspace_id, session_id, created_at, updated_at) VALUES (?, ?, ?, ?)",
        )
        .bind(&baseline.workspace_id)
        .bind(session_id)
        .bind(&stamp)
        .bind(&stamp)
        .execute(&pool)
        .await?;

        let attempt_no = baseline.attempt_no + 1;
        let attempt_id = attempt_id(&baseline.workspace_id, session_id, attempt_no);
        let updated = sqlx::query(
            "UPDATE suppression_state SET \
             attempt_no = ?, latest_attempt_id = ?, latest_outcome = 'full_in_progress', \
             full_attempt_in_progress = 1, full_failed = 0, last_error = NULL, \
             state_version = state_version + 1, updated_at = ? \
             WHERE workspace_id = ? AND session_id = ? AND state_version = ? \
               AND full_attempt_in_progress = 0",
        )
        .bind(attempt_no)
        .bind(&attempt_id)
        .bind(&stamp)
        .bind(&baseline.workspace_id)
        .bind(session_id)
        .bind(baseline.state_version)
        .execute(&pool)
        .await?
        .rows_affected();
        if updated != 1 {
            return Err(DbError::CasConflict {
                message: format!("session {session_id} already has a full attempt in progress"),
            });
        }
        Ok(FullAttempt {
            attempt_id,
            workspace_id: baseline.workspace_id.clone(),
            session_id: session_id.to_string(),
            attempt_no,
            expected_state_version: baseline.state_version + 1,
        })
    }

    /// Install the only baseline that may be skipped against. Every other
    /// terminal outcome uses [`Db::fail_full_attempt`] instead.
    ///
    /// A successful full Turn supersedes an unresolved skip audit, so this
    /// clears `full_attempt_in_progress`, `full_failed` and
    /// `audit_in_progress` in the same CAS update.
    pub async fn complete_full_attempt(
        &self,
        workspace_id: &str,
        attempt_id: impl AsRef<str>,
        completion: &FullAttemptCompletion,
    ) -> Result<SuppressionState> {
        validate_completion(completion)?;
        let pool = self.workspace_pool(workspace_id).await?;
        let attempt_id = attempt_id.as_ref();
        let current = state_by_attempt(self, workspace_id, attempt_id)
            .await?
            .ok_or_else(|| DbError::NotFound {
                id: attempt_id.to_string(),
            })?;
        if !current.full_attempt_in_progress {
            if current.latest_outcome == OUTCOME_FULL_COMPLETED
                && current.baseline_state_id.as_deref() == Some(completion.state_id.as_str())
            {
                // Retry after an unknown commit result: the stable transition
                // is already stored.
                return Ok(current);
            }
            return Err(DbError::CasConflict {
                message: format!("full attempt {attempt_id} is no longer in progress"),
            });
        }
        let now = Utc::now();
        let turn_state_json = serde_json::to_string(&completion.turn_state)?;
        let updated = sqlx::query(
            "UPDATE suppression_state SET \
             baseline_state_id = ?, baseline_turn_state_json = ?, baseline_observed_at = ?, \
             baseline_installed_at = ?, control_cursor = ?, registry_version = ?, config_version = ?, \
             full_attempt_in_progress = 0, full_failed = 0, audit_in_progress = 0, \
             latest_outcome = 'full_completed', \
             last_error = NULL, state_version = state_version + 1, updated_at = ? \
             WHERE workspace_id = ? AND session_id = ? AND latest_attempt_id = ? \
               AND full_attempt_in_progress = 1",
        )
        .bind(&completion.state_id)
        .bind(&turn_state_json)
        .bind(format_ts(completion.observed_at))
        .bind(format_ts(now))
        .bind(completion.control_cursor)
        .bind(&completion.registry_version)
        .bind(completion.config_version)
        .bind(format_ts(now))
        .bind(&current.workspace_id)
        .bind(&current.session_id)
        .bind(attempt_id)
        .execute(&pool)
        .await?
        .rows_affected();
        if updated != 1 {
            return Err(DbError::CasConflict {
                message: format!("full attempt {attempt_id} lost its completion CAS"),
            });
        }
        self.load_suppression_state(&current.workspace_id, &current.session_id)
            .await?
            .ok_or_else(|| DbError::Internal("committed suppression row disappeared".to_string()))
    }

    /// Every unsuccessful terminal (failed, cancelled, partial, degraded,
    /// timeout, unknown outcome) writes `full_failed`, keeping the old
    /// baseline ineligible until another fully successful Turn.
    pub async fn fail_full_attempt(
        &self,
        workspace_id: &str,
        attempt_id: impl AsRef<str>,
        reason: &str,
    ) -> Result<SuppressionState> {
        let pool = self.workspace_pool(workspace_id).await?;
        let attempt_id = attempt_id.as_ref();
        let reason = reason.trim();
        if reason.is_empty() {
            return Err(DbError::Validation {
                message: "full attempt failure reason must not be empty".to_string(),
            });
        }
        let current = state_by_attempt(self, workspace_id, attempt_id)
            .await?
            .ok_or_else(|| DbError::NotFound {
                id: attempt_id.to_string(),
            })?;
        if !current.full_attempt_in_progress {
            if current.full_failed && current.latest_outcome == OUTCOME_FULL_FAILED {
                // Idempotent reconciliation after an unknown commit result.
                return Ok(current);
            }
            return Err(DbError::CasConflict {
                message: format!("full attempt {attempt_id} is no longer in progress"),
            });
        }
        let now = Utc::now();
        let updated = sqlx::query(
            "UPDATE suppression_state SET \
             full_attempt_in_progress = 0, full_failed = 1, latest_outcome = 'full_failed', \
             last_error = ?, state_version = state_version + 1, updated_at = ? \
             WHERE workspace_id = ? AND session_id = ? AND latest_attempt_id = ? \
               AND full_attempt_in_progress = 1",
        )
        .bind(reason)
        .bind(format_ts(now))
        .bind(&current.workspace_id)
        .bind(&current.session_id)
        .bind(attempt_id)
        .execute(&pool)
        .await?
        .rows_affected();
        if updated != 1 {
            return Err(DbError::CasConflict {
                message: format!("full attempt {attempt_id} lost its failure CAS"),
            });
        }
        self.load_suppression_state(&current.workspace_id, &current.session_id)
            .await?
            .ok_or_else(|| DbError::Internal("committed suppression row disappeared".to_string()))
    }

    /// Settle a rejected or rolled-back skip audit.
    ///
    /// This is the durable `audit_failed` barrier from lifecycle point 3: the
    /// old baseline stays ineligible until another fully successful full Turn.
    /// Repeating the same settlement after an unknown commit result is a
    /// no-op.
    pub async fn fail_skip_attempt(
        &self,
        workspace_id: &str,
        session_id: &str,
        attempt_id: impl AsRef<str>,
        reason: &str,
    ) -> Result<SuppressionState> {
        let attempt_id = attempt_id.as_ref();
        let reason = reason.trim();
        if reason.is_empty() {
            return Err(DbError::Validation {
                message: "skip audit failure reason must not be empty".to_string(),
            });
        }
        let current = self
            .load_suppression_state(workspace_id, session_id)
            .await?
            .ok_or_else(|| DbError::NotFound {
                id: format!("{workspace_id}/{session_id}"),
            })?;
        if !current.audit_in_progress {
            if current.latest_outcome == OUTCOME_AUDIT_FAILED
                && current.latest_attempt_id.as_deref() == Some(attempt_id)
            {
                return Ok(current);
            }
            return Err(DbError::CasConflict {
                message: format!("skip audit attempt {attempt_id} is not in progress"),
            });
        }
        let pool = self.workspace_pool(workspace_id).await?;
        let updated = sqlx::query(
            "UPDATE suppression_state SET \
             audit_in_progress = 0, latest_outcome = 'audit_failed', last_error = ?, \
             state_version = state_version + 1, updated_at = ? \
             WHERE workspace_id = ? AND session_id = ? AND latest_attempt_id = ? \
               AND audit_in_progress = 1",
        )
        .bind(reason)
        .bind(format_ts(Utc::now()))
        .bind(workspace_id)
        .bind(session_id)
        .bind(attempt_id)
        .execute(&pool)
        .await?
        .rows_affected();
        if updated != 1 {
            let current = self
                .load_suppression_state(workspace_id, session_id)
                .await?
                .ok_or_else(|| DbError::NotFound {
                    id: format!("{workspace_id}/{session_id}"),
                })?;
            if !current.audit_in_progress
                && current.latest_outcome == OUTCOME_AUDIT_FAILED
                && current.latest_attempt_id.as_deref() == Some(attempt_id)
            {
                return Ok(current);
            }
            return Err(DbError::CasConflict {
                message: format!("skip audit attempt {attempt_id} lost its settlement CAS"),
            });
        }
        self.load_suppression_state(workspace_id, session_id)
            .await?
            .ok_or_else(|| DbError::Internal("committed suppression row disappeared".to_string()))
    }

    /// Fence a skip before the audit transaction. From here until
    /// [`Db::commit_skip`] succeeds (or a recovery reconciles the fence), no
    /// worker may skip.
    pub async fn begin_skip_attempt(&self, session_id: &str, baseline: &SuppressionState) -> Result<SkipAttempt> {
        if baseline.session_id != session_id {
            return Err(DbError::Validation {
                message: "baseline session does not match the skip attempt session".to_string(),
            });
        }
        if !baseline.baseline_is_eligible() {
            return Err(DbError::CasConflict {
                message: format!("session {session_id} baseline is not eligible for a skip"),
            });
        }
        let pool = self.workspace_pool(&baseline.workspace_id).await?;
        let attempt_no = baseline.attempt_no + 1;
        let attempt_id = attempt_id(&baseline.workspace_id, session_id, attempt_no);
        let updated = sqlx::query(
            "UPDATE suppression_state SET \
             attempt_no = ?, latest_attempt_id = ?, latest_outcome = 'skip_in_progress', \
             audit_in_progress = 1, state_version = state_version + 1, updated_at = ? \
             WHERE workspace_id = ? AND session_id = ? AND state_version = ? \
               AND baseline_state_id IS NOT NULL AND full_attempt_in_progress = 0 \
               AND full_failed = 0 AND audit_in_progress = 0 \
               AND schedule_sync_pending = 0 AND projection_sync_pending = 0 \
               AND notification_pending = 0",
        )
        .bind(attempt_no)
        .bind(&attempt_id)
        .bind(format_ts(Utc::now()))
        .bind(&baseline.workspace_id)
        .bind(session_id)
        .bind(baseline.state_version)
        .execute(&pool)
        .await?
        .rows_affected();
        if updated != 1 {
            return Err(DbError::CasConflict {
                message: format!("session {session_id} skip attempt lost its CAS"),
            });
        }
        Ok(SkipAttempt {
            attempt_id,
            workspace_id: baseline.workspace_id.clone(),
            session_id: session_id.to_string(),
            attempt_no,
            expected_state_version: baseline.state_version + 1,
            expected_config_version: baseline.config_version,
            expected_registry_version: baseline.registry_version.clone().unwrap_or_default(),
            expected_control_cursor: baseline.control_cursor,
        })
    }

    /// Re-validate every decision input and commit the skipped terminal plus
    /// its sync barriers in one workspace transaction.
    ///
    /// `assistant_schedules` config is platform state, so mode/configVersion
    /// are read immediately before the workspace transaction. Everything else
    /// — the state row, the user cursor, the skipped events and the pending
    /// barriers — commits together in the workspace database. Any rejection
    /// or event failure rolls the whole transaction back; only the durable
    /// `audit_in_progress` fence written by [`Db::begin_skip_attempt`]
    /// remains, so no partial skipped lifecycle is visible.
    pub async fn commit_skip(&self, attempt: &SkipAttempt, events: &[HarnessEventInput]) -> Result<CommittedSkip> {
        let schedule = sqlx::query_as::<_, (String, i64)>(
            "SELECT suppression_mode, suppression_config_version \
             FROM assistant_schedules WHERE workspace_id = ?",
        )
        .bind(&attempt.workspace_id)
        .fetch_optional(self.pool())
        .await?;
        let Some((mode, schedule_config_version)) = schedule else {
            return Err(DbError::CasConflict {
                message: format!("workspace {} has no suppression schedule", attempt.workspace_id),
            });
        };
        let mode = SuppressionMode::parse(&mode)
            .ok_or_else(|| DbError::Internal(format!("unknown suppression mode: {mode}")))?;
        if !mode.allows_skip() {
            return Err(DbError::CasConflict {
                message: format!("suppression mode {} does not allow skip", mode.as_str()),
            });
        }
        if schedule_config_version != attempt.expected_config_version {
            return Err(DbError::CasConflict {
                message: format!(
                    "suppression configVersion changed from {} to {schedule_config_version}",
                    attempt.expected_config_version
                ),
            });
        }

        let pool = self.workspace_pool(&attempt.workspace_id).await?;
        let mut tx = pool.begin().await?;
        let committed = match self.commit_skip_in_tx(&mut tx, attempt, events).await {
            Ok(committed) => committed,
            Err(error) => {
                // The audit fence was committed by `begin_skip_attempt`; this
                // transaction never becomes visible.
                let _ = tx.rollback().await;
                return Err(error);
            }
        };
        tx.commit().await?;
        Ok(committed)
    }

    async fn commit_skip_in_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        attempt: &SkipAttempt,
        events: &[HarnessEventInput],
    ) -> Result<CommittedSkip> {
        let row = sqlx::query(STATE_BY_KEY_SQL)
            .bind(&attempt.workspace_id)
            .bind(&attempt.session_id)
            .fetch_optional(&mut **tx)
            .await?;
        let state = row
            .as_ref()
            .map(map_state_row)
            .transpose()?
            .ok_or_else(|| DbError::CasConflict {
                message: format!("session {} lost its suppression state", attempt.session_id),
            })?;

        let mut changed = Vec::new();
        if state.state_version != attempt.expected_state_version {
            changed.push("stateVersion");
        }
        if state.config_version != attempt.expected_config_version {
            changed.push("configVersion");
        }
        if state.registry_version.as_deref() != Some(attempt.expected_registry_version.as_str()) {
            changed.push("registryVersion");
        }
        if state.control_cursor != attempt.expected_control_cursor {
            changed.push("cursor");
        }
        if state.latest_attempt_id.as_deref() != Some(attempt.attempt_id.as_str()) {
            changed.push("attemptId");
        }
        if state.latest_outcome != OUTCOME_SKIP_IN_PROGRESS || !state.audit_in_progress {
            changed.push("auditInProgress");
        }
        if state.full_attempt_in_progress || state.full_failed {
            changed.push("fullFence");
        }
        if state.schedule_sync_pending || state.projection_sync_pending || state.notification_pending {
            changed.push("syncFence");
        }
        if !changed.is_empty() {
            return Err(DbError::CasConflict {
                message: format!(
                    "skip commit rejected for {}: {} changed",
                    attempt.session_id,
                    changed.join(", ")
                ),
            });
        }

        // User input that arrived after the preflight snapshot belongs to the
        // next Turn. Only `message.created(role=user)` controls the cursor.
        let current_cursor: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(seq), 0) FROM session_events \
             WHERE execution_mode = 'production' AND session_id = ? \
               AND type = 'message.created' \
               AND json_extract(payload_json, '$.role') = 'user'",
        )
        .bind(&attempt.session_id)
        .fetch_one(&mut **tx)
        .await?;
        if current_cursor > attempt.expected_control_cursor {
            return Err(DbError::CasConflict {
                message: format!(
                    "session {} received user input at seq {current_cursor} after cursor {}",
                    attempt.session_id, attempt.expected_control_cursor
                ),
            });
        }

        let mut appended = Vec::with_capacity(events.len());
        for event in events {
            if event.workspace_id != attempt.workspace_id {
                return Err(DbError::Validation {
                    message: "skip event workspace does not match the suppression attempt".to_string(),
                });
            }
            appended.push(crate::harness_event::append_session_event_tx(tx, event).await?);
        }

        let updated = sqlx::query(
            "UPDATE suppression_state SET \
             latest_outcome = 'skipped', audit_in_progress = 0, \
             schedule_sync_pending = 1, projection_sync_pending = 1, notification_pending = 1, \
             last_error = NULL, state_version = state_version + 1, updated_at = ? \
             WHERE workspace_id = ? AND session_id = ? AND state_version = ? \
               AND latest_attempt_id = ? AND audit_in_progress = 1 \
               AND full_attempt_in_progress = 0 AND full_failed = 0",
        )
        .bind(format_ts(Utc::now()))
        .bind(&attempt.workspace_id)
        .bind(&attempt.session_id)
        .bind(attempt.expected_state_version)
        .bind(&attempt.attempt_id)
        .execute(&mut **tx)
        .await?
        .rows_affected();
        if updated != 1 {
            return Err(DbError::CasConflict {
                message: format!("session {} skip commit lost its CAS", attempt.session_id),
            });
        }

        let row = sqlx::query(STATE_BY_KEY_SQL)
            .bind(&attempt.workspace_id)
            .bind(&attempt.session_id)
            .fetch_one(&mut **tx)
            .await?;
        let state = map_state_row(&row)?;
        Ok(CommittedSkip {
            attempt_id: attempt.attempt_id.clone(),
            attempt_no: attempt.attempt_no,
            events: appended,
            state,
        })
    }

    /// Clear one post-commit sync barrier after the corresponding consumer
    /// succeeded. A failed consumer leaves its flag set for every worker.
    pub async fn clear_sync_pending(
        &self,
        workspace_id: &str,
        session_id: &str,
        pending: SyncPending,
    ) -> Result<SuppressionState> {
        let sql = match pending {
            SyncPending::Schedule => {
                "UPDATE suppression_state SET schedule_sync_pending = 0, \
                 state_version = state_version + 1, updated_at = ? \
                 WHERE workspace_id = ? AND session_id = ?"
            }
            SyncPending::Projection => {
                "UPDATE suppression_state SET projection_sync_pending = 0, \
                 state_version = state_version + 1, updated_at = ? \
                 WHERE workspace_id = ? AND session_id = ?"
            }
            SyncPending::Notification => {
                "UPDATE suppression_state SET notification_pending = 0, \
                 state_version = state_version + 1, updated_at = ? \
                 WHERE workspace_id = ? AND session_id = ?"
            }
        };
        let pool = self.workspace_pool(workspace_id).await?;
        let updated = sqlx::query(sql)
            .bind(format_ts(Utc::now()))
            .bind(workspace_id)
            .bind(session_id)
            .execute(&pool)
            .await?
            .rows_affected();
        if updated != 1 {
            return Err(DbError::NotFound {
                id: format!("{workspace_id}/{session_id}"),
            });
        }
        self.load_suppression_state(workspace_id, session_id)
            .await?
            .ok_or_else(|| DbError::Internal("committed suppression row disappeared".to_string()))
    }
}
