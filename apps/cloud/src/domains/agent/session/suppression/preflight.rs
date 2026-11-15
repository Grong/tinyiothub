//! Deterministic preflight decisions.
//!
//! Preflight is read-only: it captures the current Sources at `observedAt` and
//! compares them with the durable baseline/state. It never advances the
//! baseline, cursor, config or fences. The observed snapshot is the
//! external-Source boundary; changes arriving after it belong to the next
//! Turn.

#![allow(clippy::result_large_err)]

use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::Duration as StdDuration;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use tinyiothub_storage::assistant_schedule::SuppressionConfig;
use tinyiothub_storage::suppression::{
    DEFAULT_RECONCILIATION_SECONDS, MAX_RECONCILIATION_SECONDS, MIN_RECONCILIATION_SECONDS, OUTCOME_AUDIT_FAILED,
    OUTCOME_BASELINE_MISSING, OUTCOME_FULL_COMPLETED, OUTCOME_FULL_FAILED, OUTCOME_FULL_IN_PROGRESS,
    OUTCOME_SKIP_IN_PROGRESS, OUTCOME_SKIPPED, SuppressionMode, SuppressionState,
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;

use super::contract::{SourceSnapshot, TURN_STATE_VERSION};
use super::error::{ErrorCode, SuppressionError};
use super::registry::{SourceContext, SourceMode, SourceRegistry};

/// Bound on concurrent preflight captures across every workspace.
pub const DEFAULT_PREFLIGHT_CONCURRENCY: usize = 4;
pub const MIN_PREFLIGHT_CONCURRENCY: usize = 1;
pub const MAX_PREFLIGHT_CONCURRENCY: usize = 32;
/// Per-Source capture deadline. A hanging adapter cannot hold a slot forever.
pub const PREFLIGHT_CAPTURE_DEADLINE: StdDuration = StdDuration::from_secs(2);
/// Per-Source reference-resolution deadline (defaults equal the capture bound).
pub const PREFLIGHT_RESOLVE_DEADLINE: StdDuration = StdDuration::from_secs(2);
/// Whole-preflight deadline; a preflight that exceeds it is a timeout, never a
/// partial comparison.
pub const PREFLIGHT_TOTAL_DEADLINE: StdDuration = StdDuration::from_secs(4);
/// Immediate HTTP retry hint when the global gate is saturated.
pub const PREFLIGHT_BACKPRESSURE_RETRY_AFTER_MS: u64 = 1_000;

/// Preflight deadlines. Defaults are the spec values 2s/2s/4s; tests inject
/// shorter values so timeout behavior stays fast and deterministic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreflightLimits {
    pub capture_deadline: StdDuration,
    pub resolve_deadline: StdDuration,
    pub total_deadline: StdDuration,
}

impl Default for PreflightLimits {
    fn default() -> Self {
        Self {
            capture_deadline: PREFLIGHT_CAPTURE_DEADLINE,
            resolve_deadline: PREFLIGHT_RESOLVE_DEADLINE,
            total_deadline: PREFLIGHT_TOTAL_DEADLINE,
        }
    }
}

/// Runtime-configurable global preflight gate.
///
/// Admission uses `try_acquire`: saturation is an immediate typed answer, not
/// an unbounded waiter queue. The master semaphore owns
/// `MAX_PREFLIGHT_CONCURRENCY` permits so `set_limit` never needs to rebuild
/// it while attempts are in flight.
pub struct PreflightGate {
    limit: AtomicUsize,
    in_flight: AtomicUsize,
    permits: Arc<Semaphore>,
}

impl PreflightGate {
    pub fn new(limit: usize) -> Arc<Self> {
        Arc::new(Self {
            limit: AtomicUsize::new(clamp_concurrency(limit)),
            in_flight: AtomicUsize::new(0),
            permits: Arc::new(Semaphore::new(MAX_PREFLIGHT_CONCURRENCY)),
        })
    }

    pub fn limit(&self) -> usize {
        self.limit.load(Ordering::Acquire)
    }

    /// Runtime reconfiguration. Values outside 1..=32 are clamped.
    pub fn set_limit(&self, limit: usize) {
        self.limit.store(clamp_concurrency(limit), Ordering::Release);
    }

    pub fn in_flight(&self) -> usize {
        self.in_flight.load(Ordering::Acquire)
    }

    pub fn has_capacity(&self) -> bool {
        self.in_flight() < self.limit()
    }

    /// Reserve one slot or return `None` immediately; never waits.
    pub fn try_acquire(self: &Arc<Self>) -> Option<PreflightPermit> {
        let permit = self.permits.clone().try_acquire_owned().ok()?;
        let mut current = self.in_flight.load(Ordering::Acquire);
        loop {
            if current >= self.limit() {
                // The temporary permit is dropped here; the counter was never
                // incremented, so a failed admission cannot leak capacity.
                return None;
            }
            match self
                .in_flight
                .compare_exchange_weak(current, current + 1, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => {
                    return Some(PreflightPermit {
                        gate: Arc::clone(self),
                        _permit: permit,
                    });
                }
                Err(actual) => current = actual,
            }
        }
    }
}

fn clamp_concurrency(limit: usize) -> usize {
    limit.clamp(MIN_PREFLIGHT_CONCURRENCY, MAX_PREFLIGHT_CONCURRENCY)
}

/// RAII slot guard. Dropping it (including on timeout/cancellation/panic)
/// releases both the durable counter and the semaphore permit.
pub struct PreflightPermit {
    gate: Arc<PreflightGate>,
    _permit: OwnedSemaphorePermit,
}

impl Drop for PreflightPermit {
    fn drop(&mut self) {
        self.gate.in_flight.fetch_sub(1, Ordering::AcqRel);
    }
}

static GLOBAL_PREFLIGHT_GATE: LazyLock<Arc<PreflightGate>> =
    LazyLock::new(|| PreflightGate::new(DEFAULT_PREFLIGHT_CONCURRENCY));

/// The process-wide preflight gate shared by scheduled, event and manual wakes.
pub fn global_preflight_gate() -> Arc<PreflightGate> {
    Arc::clone(&GLOBAL_PREFLIGHT_GATE)
}

fn preflight_timeout_error() -> SuppressionError {
    SuppressionError::from_code(ErrorCode::PreflightTimeout)
}

fn backpressure_error() -> SuppressionError {
    let mut error = SuppressionError::from_code(ErrorCode::WakeBackpressure);
    error.context = serde_json::json!({ "retryAfterMs": PREFLIGHT_BACKPRESSURE_RETRY_AFTER_MS });
    error
}

/// A full attempt is durably in progress; its old baseline is ineligible.
pub const FENCE_FULL_IN_PROGRESS: &str = "full_in_progress";
/// The last full attempt failed, was cancelled, partial, degraded or timed out.
pub const FENCE_FULL_FAILED: &str = "full_failed";
/// A skip audit is durably in progress or crashed before settling.
pub const FENCE_AUDIT_IN_PROGRESS: &str = "audit_in_progress";
/// A committed skip still has an unreconciled schedule/projection/notification.
pub const FENCE_SYNC_PENDING: &str = "sync_pending";

/// Stable reason a full Turn is required instead of a skip.
///
/// The wire values are stable snake_case strings; normal reasons
/// (`baseline_missing`, `registry_changed`, `reconciliation_due`) are not
/// error alerts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipVeto {
    Disabled,
    BaselineMissing,
    BaselineCorrupt,
    SourceUnavailable,
    SourceChanged,
    SourceRefMismatch,
    SourceRevisionConflict,
    RegistryChanged,
    NewUserInput,
    ReconciliationDue,
    AuditFailed,
}

impl SkipVeto {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::BaselineMissing => "baseline_missing",
            Self::BaselineCorrupt => "baseline_corrupt",
            Self::SourceUnavailable => "source_unavailable",
            Self::SourceChanged => "source_changed",
            Self::SourceRefMismatch => "source_ref_mismatch",
            Self::SourceRevisionConflict => "source_revision_conflict",
            Self::RegistryChanged => "registry_changed",
            Self::NewUserInput => "new_user_input",
            Self::ReconciliationDue => "reconciliation_due",
            Self::AuditFailed => "audit_failed",
        }
    }
}

/// The read-only preflight answer plus the exact snapshot it decided on.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreflightDecision {
    pub would_skip: bool,
    /// The configured mode this decision was evaluated under. Shadow computes
    /// `would_skip` but `allows_skip()` is false, so callers must gate any
    /// durable skip on this field.
    pub mode: SuppressionMode,
    pub vetoes: Vec<SkipVeto>,
    /// Current Source snapshots at `observedAt`, in registry order.
    pub snapshot: Vec<SourceSnapshot>,
    pub baseline_state_id: Option<String>,
    pub config_version: i64,
    /// Durable fence that makes every baseline ineligible, if any.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub fence: String,
}

struct PreflightInput<'a> {
    observed_at: DateTime<Utc>,
    state: Option<&'a SuppressionState>,
    config: &'a SuppressionConfig,
    current_user_seq: i64,
    registry_version: &'a str,
    snapshots: &'a [SourceSnapshot],
}

/// Read-only preflight over the registered Sources and persisted baseline.
///
/// `context.observedAt` is the external-Source boundary. Changes after that
/// instant belong to the next Turn; this operation never advances a baseline,
/// cursor, config version or fence.
pub struct SuppressionService {
    registry: Arc<SourceRegistry>,
    gate: Arc<PreflightGate>,
    limits: PreflightLimits,
}

impl SuppressionService {
    pub fn new(registry: Arc<SourceRegistry>) -> Self {
        Self {
            registry,
            gate: global_preflight_gate(),
            limits: PreflightLimits::default(),
        }
    }

    /// Inject a gate (tests and composition that needs an isolated capacity).
    pub fn with_gate(registry: Arc<SourceRegistry>, gate: Arc<PreflightGate>) -> Self {
        Self {
            registry,
            gate,
            limits: PreflightLimits::default(),
        }
    }

    pub fn with_limits(mut self, limits: PreflightLimits) -> Self {
        self.limits = limits;
        self
    }

    pub fn registry(&self) -> &SourceRegistry {
        self.registry.as_ref()
    }

    pub fn gate(&self) -> &Arc<PreflightGate> {
        &self.gate
    }

    pub async fn evaluate(
        &self,
        mut context: SourceContext,
        state: Option<&SuppressionState>,
        config: &SuppressionConfig,
        current_user_seq: i64,
    ) -> Result<PreflightDecision, SuppressionError> {
        context.mode = config.mode.into();
        // Off observes nothing: the kill switch must stay cheap and immediate
        // and must not depend on capture capacity.
        if config.mode == SuppressionMode::Off {
            let registry_version = self.registry.registry_version();
            return Ok(evaluate_snapshots(PreflightInput {
                observed_at: context.observed_at,
                state,
                config,
                current_user_seq,
                registry_version: &registry_version,
                snapshots: &[],
            }));
        }
        let _permit = self.gate.try_acquire().ok_or_else(backpressure_error)?;
        let cancel = CancellationToken::new();
        let capture = self.registry.capture_all_bounded(
            &context,
            self.limits.capture_deadline,
            self.limits.resolve_deadline,
            &cancel,
        );
        let snapshots = match tokio::time::timeout(self.limits.total_deadline, capture).await {
            Ok(result) => result?,
            Err(_) => {
                // Cooperative cancellation on top of dropping the slow future.
                cancel.cancel();
                return Err(preflight_timeout_error());
            }
        };
        let registry_version = self.registry.registry_version();
        Ok(evaluate_snapshots(PreflightInput {
            observed_at: context.observed_at,
            state,
            config,
            current_user_seq,
            registry_version: &registry_version,
            snapshots: &snapshots,
        }))
    }
}

impl From<SuppressionMode> for SourceMode {
    fn from(mode: SuppressionMode) -> Self {
        match mode {
            SuppressionMode::Off => Self::Off,
            SuppressionMode::Shadow => Self::Shadow,
            SuppressionMode::Canary => Self::Canary,
            SuppressionMode::Enforce => Self::Enforce,
        }
    }
}

fn evaluate_snapshots(input: PreflightInput<'_>) -> PreflightDecision {
    let mut decision = PreflightDecision {
        would_skip: false,
        mode: input.config.mode,
        vetoes: Vec::new(),
        snapshot: input.snapshots.to_vec(),
        baseline_state_id: input.state.and_then(|state| state.baseline_state_id.clone()),
        config_version: input.config.config_version,
        fence: String::new(),
    };

    if input.config.mode == SuppressionMode::Off {
        decision.vetoes.push(SkipVeto::Disabled);
        return decision;
    }

    let Some(state) = input.state else {
        push_veto(&mut decision.vetoes, SkipVeto::BaselineMissing);
        return decision;
    };

    if state.baseline_state_id.is_none() {
        push_veto(&mut decision.vetoes, SkipVeto::BaselineMissing);
    } else {
        match parse_baseline(state) {
            Ok(baseline) => compare_sources(
                state,
                &baseline,
                input.snapshots,
                input.registry_version,
                &mut decision.vetoes,
            ),
            Err(()) => push_veto(&mut decision.vetoes, SkipVeto::BaselineCorrupt),
        }
    }

    if input.current_user_seq > state.control_cursor {
        push_veto(&mut decision.vetoes, SkipVeto::NewUserInput);
    }

    match reconciliation_due(state, input.observed_at, input.config.reconciliation_seconds) {
        Ok(true) => push_veto(&mut decision.vetoes, SkipVeto::ReconciliationDue),
        Ok(false) => {}
        // A baseline without an installation/observation timestamp cannot be
        // bounded by reconciliation.
        Err(()) if state.baseline_state_id.is_some() => push_veto(&mut decision.vetoes, SkipVeto::BaselineCorrupt),
        Err(()) => {}
    }

    decision.fence = state_fence(state, &mut decision.vetoes);
    decision.would_skip = decision.vetoes.is_empty() && decision.fence.is_empty();
    decision
}

fn compare_sources(
    state: &SuppressionState,
    baseline: &BTreeMap<String, BaselineSource>,
    snapshots: &[SourceSnapshot],
    registry_version: &str,
    vetoes: &mut Vec<SkipVeto>,
) {
    let registry_matches = state.registry_version.as_deref() == Some(registry_version);
    if !registry_matches {
        push_veto(vetoes, SkipVeto::RegistryChanged);
    }

    let baseline_ids: HashSet<&str> = baseline.keys().map(String::as_str).collect();
    let current_ids: HashSet<&str> = snapshots.iter().map(|snapshot| snapshot.source_id.as_str()).collect();
    let source_set_matches = baseline_ids == current_ids;
    if !source_set_matches {
        push_veto(vetoes, SkipVeto::BaselineCorrupt);
    }

    for snapshot in snapshots {
        if !snapshot.available {
            push_veto(vetoes, SkipVeto::SourceUnavailable);
            continue;
        }
        if snapshot_ref_broken(snapshot) {
            push_veto(vetoes, SkipVeto::SourceRefMismatch);
        }
        if !registry_matches || !source_set_matches {
            continue;
        }
        let Some(previous) = baseline.get(&snapshot.source_id) else {
            continue;
        };
        if previous.source_version != snapshot.source_version {
            // An adapter/schema change invalidates the baseline even when the
            // registry hash was not refreshed.
            push_veto(vetoes, SkipVeto::RegistryChanged);
        } else if previous.revision != snapshot.revision {
            push_veto(vetoes, SkipVeto::SourceChanged);
        } else if previous.state_hash != snapshot.state_hash {
            push_veto(vetoes, SkipVeto::SourceRevisionConflict);
        }
    }
}

fn snapshot_ref_broken(snapshot: &SourceSnapshot) -> bool {
    if !snapshot.available {
        return false;
    }
    let Some(state_ref) = snapshot.state_ref.as_ref() else {
        return true;
    };
    if state_ref.revision != snapshot.revision || state_ref.state_hash != snapshot.state_hash {
        return true;
    }
    if snapshot.error.is_some() || (snapshot.empty && !snapshot.work_items.is_empty()) {
        return true;
    }
    snapshot
        .work_items
        .iter()
        .any(|item| item.payload_ref.revision != item.revision || item.payload_ref.payload_hash != item.payload_hash)
}

fn state_fence(state: &SuppressionState, vetoes: &mut Vec<SkipVeto>) -> String {
    match state.latest_outcome.as_str() {
        OUTCOME_FULL_COMPLETED | OUTCOME_SKIPPED => {}
        OUTCOME_BASELINE_MISSING => push_veto(vetoes, SkipVeto::BaselineMissing),
        OUTCOME_AUDIT_FAILED => push_veto(vetoes, SkipVeto::AuditFailed),
        OUTCOME_FULL_IN_PROGRESS | OUTCOME_FULL_FAILED | OUTCOME_SKIP_IN_PROGRESS => {}
        _ => push_veto(vetoes, SkipVeto::BaselineCorrupt),
    }

    if state.full_attempt_in_progress || state.latest_outcome == OUTCOME_FULL_IN_PROGRESS {
        return FENCE_FULL_IN_PROGRESS.to_string();
    }
    if state.full_failed || state.latest_outcome == OUTCOME_FULL_FAILED {
        return FENCE_FULL_FAILED.to_string();
    }
    if state.audit_in_progress || state.latest_outcome == OUTCOME_SKIP_IN_PROGRESS {
        return FENCE_AUDIT_IN_PROGRESS.to_string();
    }
    if state.schedule_sync_pending || state.projection_sync_pending || state.notification_pending {
        return FENCE_SYNC_PENDING.to_string();
    }
    String::new()
}

fn reconciliation_due(state: &SuppressionState, now: DateTime<Utc>, configured_seconds: u32) -> Result<bool, ()> {
    let installed = state.baseline_installed_at.or(state.baseline_observed_at).ok_or(())?;
    let seconds = if configured_seconds == 0 {
        DEFAULT_RECONCILIATION_SECONDS
    } else {
        configured_seconds.clamp(MIN_RECONCILIATION_SECONDS, MAX_RECONCILIATION_SECONDS)
    };
    Ok(now >= installed + Duration::seconds(i64::from(seconds)))
}

fn push_veto(vetoes: &mut Vec<SkipVeto>, veto: SkipVeto) {
    if !vetoes.contains(&veto) {
        vetoes.push(veto);
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BaselineTurnState {
    state_id: String,
    state_version: i64,
    sources: Vec<BaselineSource>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BaselineSource {
    source_id: String,
    source_version: u32,
    revision: String,
    state_hash: String,
    available: bool,
}

fn parse_baseline(state: &SuppressionState) -> Result<BTreeMap<String, BaselineSource>, ()> {
    let baseline_id = state.baseline_state_id.as_deref().ok_or(())?;
    let value = state.baseline_turn_state.as_ref().ok_or(())?;
    let envelope: BaselineTurnState = serde_json::from_value(value.clone()).map_err(|_| ())?;
    if envelope.state_id != baseline_id || envelope.state_version != i64::from(TURN_STATE_VERSION) {
        return Err(());
    }
    let mut sources = BTreeMap::new();
    for source in envelope.sources {
        if source.source_id.is_empty()
            || source.revision.is_empty()
            || source.state_hash.is_empty()
            || !source.available
        {
            return Err(());
        }
        if sources.insert(source.source_id.clone(), source).is_some() {
            return Err(());
        }
    }
    Ok(sources)
}

#[cfg(test)]
#[allow(clippy::module_inception)] // the test path must contain `suppression::preflight`
mod preflight {
    use std::sync::Arc;

    use chrono::{DateTime, Duration, TimeZone, Utc};
    use serde_json::{Value, json};
    use tinyiothub_storage::assistant_schedule::SuppressionConfig;
    use tinyiothub_storage::suppression::{
        DEFAULT_RECONCILIATION_SECONDS, OUTCOME_AUDIT_FAILED, OUTCOME_BASELINE_MISSING, OUTCOME_FULL_COMPLETED,
        OUTCOME_FULL_FAILED, OUTCOME_FULL_IN_PROGRESS, OUTCOME_SKIP_IN_PROGRESS, SuppressionMode, SuppressionState,
    };

    use super::{
        FENCE_AUDIT_IN_PROGRESS, FENCE_FULL_FAILED, FENCE_FULL_IN_PROGRESS, FENCE_SYNC_PENDING, PreflightDecision,
        PreflightInput, SkipVeto, SuppressionService, evaluate_snapshots,
    };
    use crate::domains::agent::session::suppression::contract::{
        CanonicalBytes, PayloadRef, SourceDescriptor, SourceSnapshot, StateRef, TURN_STATE_VERSION,
    };
    use crate::domains::agent::session::suppression::error::SuppressionError;
    use crate::domains::agent::session::suppression::registry::{
        AssistantSource, SOURCE_VERSION, SourceContext, SourceMode, SourceRegistry,
    };

    const WS: &str = "ws-preflight";
    const SESSION: &str = "agent:ws-preflight:__assistant__ws-preflight/main";
    const STATE_ID: &str = "state-1";
    const REGISTRY_VERSION: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn ts(seconds: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(seconds, 0).single().expect("timestamp")
    }

    fn config(mode: SuppressionMode) -> SuppressionConfig {
        SuppressionConfig {
            workspace_id: WS.to_string(),
            mode,
            reconciliation_seconds: DEFAULT_RECONCILIATION_SECONDS,
            config_version: 1,
        }
    }

    fn hash_hex(character: char) -> String {
        format!("sha256:{}", character.to_string().repeat(64))
    }

    fn snapshot(source_id: &str, revision: &str, state_hash: String) -> SourceSnapshot {
        SourceSnapshot {
            source_id: source_id.to_string(),
            source_version: SOURCE_VERSION,
            revision: revision.to_string(),
            state_hash: state_hash.clone(),
            available: true,
            empty: true,
            state_ref: Some(StateRef {
                id: format!("state:{revision}"),
                revision: revision.to_string(),
                state_hash,
            }),
            work_items: vec![],
            error: None,
        }
    }

    fn turn_state(state_id: &str, snapshots: &[SourceSnapshot]) -> Value {
        json!({
            "stateId": state_id,
            "stateVersion": i64::from(TURN_STATE_VERSION),
            "turnId": "turn-1",
            "sources": snapshots
                .iter()
                .map(|snapshot| {
                    json!({
                        "sourceId": snapshot.source_id,
                        "sourceVersion": snapshot.source_version,
                        "revision": snapshot.revision,
                        "stateHash": snapshot.state_hash,
                        "available": snapshot.available,
                    })
                })
                .collect::<Vec<_>>(),
            "controlCursor": 5,
            "registryVersion": REGISTRY_VERSION,
            "mode": "enforce",
        })
    }

    fn state_for(snapshots: &[SourceSnapshot], registry_version: &str) -> SuppressionState {
        let installed = ts(1_800_000_000);
        let mut state = SuppressionState::initial(WS, SESSION);
        state.baseline_state_id = Some(STATE_ID.to_string());
        state.baseline_turn_state = Some(turn_state(STATE_ID, snapshots));
        state.baseline_observed_at = Some(installed);
        state.baseline_installed_at = Some(installed);
        state.registry_version = Some(registry_version.to_string());
        state.control_cursor = 5;
        state.latest_outcome = OUTCOME_FULL_COMPLETED.to_string();
        state.config_version = 1;
        state
    }

    struct DecisionFixture {
        config: SuppressionConfig,
        state: Option<SuppressionState>,
        snapshots: Vec<SourceSnapshot>,
        current_user_seq: i64,
        registry_version: String,
        observed_at: DateTime<Utc>,
    }

    impl DecisionFixture {
        fn evaluate(&self) -> PreflightDecision {
            evaluate_snapshots(PreflightInput {
                observed_at: self.observed_at,
                state: self.state.as_ref(),
                config: &self.config,
                current_user_seq: self.current_user_seq,
                registry_version: &self.registry_version,
                snapshots: &self.snapshots,
            })
        }

        fn unchanged(self) -> Self {
            self
        }

        fn not_due(self) -> Self {
            self.at_offset(60)
        }

        fn due(self) -> Self {
            self.at_offset(i64::from(DEFAULT_RECONCILIATION_SECONDS))
        }

        fn at_offset(mut self, seconds: i64) -> Self {
            let installed = self
                .state
                .as_ref()
                .and_then(|state| state.baseline_installed_at)
                .expect("fixture baseline");
            self.observed_at = installed + Duration::seconds(seconds);
            self
        }

        fn changed_source(mut self) -> Self {
            let state_hash = hash_hex('c');
            let snapshot = &mut self.snapshots[0];
            snapshot.revision = "rev-a2".to_string();
            snapshot.state_hash = state_hash.clone();
            let state_ref = snapshot.state_ref.as_mut().expect("fixture state ref");
            state_ref.id = "state:rev-a2".to_string();
            state_ref.revision = "rev-a2".to_string();
            state_ref.state_hash = state_hash;
            self
        }

        fn with_state(mut self, mutate: impl FnOnce(&mut SuppressionState)) -> Self {
            mutate(self.state.as_mut().expect("fixture baseline"));
            self
        }

        fn with_current_snapshot(mut self, index: usize, mutate: impl FnOnce(&mut SourceSnapshot)) -> Self {
            mutate(&mut self.snapshots[index]);
            self
        }

        fn with_current_user_seq(mut self, seq: i64) -> Self {
            self.current_user_seq = seq;
            self
        }

        fn with_registry_version(mut self, registry_version: &str) -> Self {
            self.registry_version = registry_version.to_string();
            self
        }

        fn without_baseline(mut self) -> Self {
            self.state = None;
            self
        }
    }

    fn decision_fixture(mode: SuppressionMode) -> DecisionFixture {
        let snapshots = vec![
            snapshot("source.a", "rev-a", hash_hex('a')),
            snapshot("source.b", "rev-b", hash_hex('b')),
        ];
        let state = state_for(&snapshots, REGISTRY_VERSION);
        DecisionFixture {
            observed_at: state.baseline_installed_at.expect("fixture timestamp") + Duration::seconds(60),
            config: config(mode),
            state: Some(state),
            snapshots,
            current_user_seq: 5,
            registry_version: REGISTRY_VERSION.to_string(),
        }
    }

    fn assert_veto(decision: &PreflightDecision, veto: SkipVeto) {
        assert!(!decision.would_skip, "expected a full turn");
        assert!(
            decision.vetoes.contains(&veto),
            "missing {veto:?} in {:?}",
            decision.vetoes
        );
    }

    #[test]
    fn unchanged_enforce_snapshot_would_skip() {
        let result = decision_fixture(SuppressionMode::Enforce)
            .unchanged()
            .not_due()
            .evaluate();
        assert!(result.would_skip);
        assert!(result.vetoes.is_empty());
    }

    #[test]
    fn unchanged_snapshot_carries_decision_evidence() {
        let result = decision_fixture(SuppressionMode::Enforce)
            .unchanged()
            .not_due()
            .evaluate();
        assert_eq!(result.baseline_state_id.as_deref(), Some(STATE_ID));
        assert_eq!(result.config_version, 1);
        assert_eq!(result.snapshot.len(), 2);
        assert!(result.fence.is_empty());
    }

    #[test]
    fn shadow_computes_would_skip_but_never_skips() {
        let result = decision_fixture(SuppressionMode::Shadow)
            .unchanged()
            .not_due()
            .evaluate();
        assert!(result.would_skip);
        assert_eq!(result.mode, SuppressionMode::Shadow);
        assert!(result.vetoes.is_empty());
        assert!(!SuppressionMode::Shadow.allows_skip());
    }

    #[test]
    fn off_returns_disabled() {
        let result = decision_fixture(SuppressionMode::Off).unchanged().not_due().evaluate();
        assert_eq!(result.vetoes, vec![SkipVeto::Disabled]);
        assert!(!result.would_skip);
        assert!(!SuppressionMode::Off.allows_skip());
    }

    #[test]
    fn baseline_missing_is_a_normal_full_run_reason() {
        let result = decision_fixture(SuppressionMode::Enforce).without_baseline().evaluate();
        assert_veto(&result, SkipVeto::BaselineMissing);
        assert!(result.vetoes.contains(&SkipVeto::BaselineMissing));
    }

    #[test]
    fn corrupt_baseline_never_skips() {
        let result = decision_fixture(SuppressionMode::Enforce)
            .with_state(|state| {
                state.baseline_turn_state = Some(json!({"stateId": STATE_ID, "stateVersion": 1, "sources": "bad"}));
            })
            .evaluate();
        assert_veto(&result, SkipVeto::BaselineCorrupt);
    }

    #[test]
    fn baseline_with_an_unavailable_source_is_corrupt() {
        let result = decision_fixture(SuppressionMode::Enforce)
            .with_state(|state| {
                let mut turn = state.baseline_turn_state.clone().expect("baseline");
                turn["sources"][0]["available"] = json!(false);
                state.baseline_turn_state = Some(turn);
            })
            .evaluate();
        assert_veto(&result, SkipVeto::BaselineCorrupt);
    }

    #[test]
    fn unavailable_source_never_skips() {
        let result = decision_fixture(SuppressionMode::Enforce)
            .with_current_snapshot(0, |snapshot| {
                snapshot.available = false;
                snapshot.empty = false;
                snapshot.state_ref = None;
                snapshot.error = Some(SuppressionError::new(
                    "source_unavailable",
                    "capture failed",
                    "restore the source",
                ));
            })
            .evaluate();
        assert_veto(&result, SkipVeto::SourceUnavailable);
    }

    #[test]
    fn changed_source_never_skips() {
        let result = decision_fixture(SuppressionMode::Enforce)
            .unchanged()
            .not_due()
            .changed_source()
            .evaluate();
        assert_veto(&result, SkipVeto::SourceChanged);
    }

    #[test]
    fn broken_source_ref_never_skips() {
        let result = decision_fixture(SuppressionMode::Enforce)
            .with_current_snapshot(0, |snapshot| {
                snapshot.state_ref.as_mut().expect("state ref").revision = "stale-revision".to_string();
            })
            .evaluate();
        assert_veto(&result, SkipVeto::SourceRefMismatch);
    }

    #[test]
    fn same_revision_with_a_different_hash_is_a_conflict() {
        let conflict = hash_hex('d');
        let result = decision_fixture(SuppressionMode::Enforce)
            .with_current_snapshot(0, |snapshot| {
                snapshot.state_hash = conflict.clone();
                snapshot.state_ref.as_mut().expect("state ref").state_hash = conflict;
            })
            .evaluate();
        assert_veto(&result, SkipVeto::SourceRevisionConflict);
    }

    #[test]
    fn registry_change_invalidates_the_baseline() {
        let result = decision_fixture(SuppressionMode::Enforce)
            .with_registry_version("sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
            .evaluate();
        assert_veto(&result, SkipVeto::RegistryChanged);
    }

    #[test]
    fn new_user_input_never_skips() {
        let result = decision_fixture(SuppressionMode::Enforce)
            .with_current_user_seq(6)
            .evaluate();
        assert_veto(&result, SkipVeto::NewUserInput);
    }

    #[test]
    fn reconciliation_due_forces_a_full_turn_at_the_boundary() {
        let due = decision_fixture(SuppressionMode::Enforce).unchanged().due().evaluate();
        assert_veto(&due, SkipVeto::ReconciliationDue);

        let not_due = decision_fixture(SuppressionMode::Enforce)
            .unchanged()
            .at_offset(i64::from(DEFAULT_RECONCILIATION_SECONDS) - 1)
            .evaluate();
        assert!(not_due.vetoes.is_empty());
    }

    #[test]
    fn audit_failure_never_skips() {
        let result = decision_fixture(SuppressionMode::Enforce)
            .with_state(|state| {
                state.latest_outcome = OUTCOME_AUDIT_FAILED.to_string();
            })
            .evaluate();
        assert_veto(&result, SkipVeto::AuditFailed);
    }

    #[test]
    fn full_attempt_in_progress_is_a_fence() {
        let result = decision_fixture(SuppressionMode::Enforce)
            .with_state(|state| {
                state.full_attempt_in_progress = true;
                state.latest_outcome = OUTCOME_FULL_IN_PROGRESS.to_string();
            })
            .evaluate();
        assert!(!result.would_skip);
        assert_eq!(result.fence, FENCE_FULL_IN_PROGRESS);
    }

    #[test]
    fn failed_full_attempt_is_a_full_failed_fence() {
        let result = decision_fixture(SuppressionMode::Enforce)
            .with_state(|state| {
                state.full_failed = true;
                state.latest_outcome = OUTCOME_FULL_FAILED.to_string();
            })
            .evaluate();
        assert!(!result.would_skip);
        assert_eq!(result.fence, FENCE_FULL_FAILED);
    }

    #[test]
    fn audit_in_progress_is_a_fence() {
        let result = decision_fixture(SuppressionMode::Enforce)
            .with_state(|state| {
                state.audit_in_progress = true;
                state.latest_outcome = OUTCOME_SKIP_IN_PROGRESS.to_string();
            })
            .evaluate();
        assert!(!result.would_skip);
        assert_eq!(result.fence, FENCE_AUDIT_IN_PROGRESS);
    }

    #[test]
    fn every_pending_sync_barrier_is_a_fence() {
        for mutate in [
            (|state: &mut SuppressionState| state.schedule_sync_pending = true) as fn(&mut SuppressionState),
            |state: &mut SuppressionState| state.projection_sync_pending = true,
            |state: &mut SuppressionState| state.notification_pending = true,
        ] {
            let result = decision_fixture(SuppressionMode::Enforce).with_state(mutate).evaluate();
            assert!(!result.would_skip);
            assert_eq!(result.fence, FENCE_SYNC_PENDING);
        }
    }

    #[test]
    fn degraded_baseline_outcome_cannot_skip() {
        let result = decision_fixture(SuppressionMode::Enforce)
            .with_state(|state| {
                state.latest_outcome = OUTCOME_BASELINE_MISSING.to_string();
                state.baseline_state_id = None;
                state.baseline_turn_state = None;
            })
            .evaluate();
        assert_veto(&result, SkipVeto::BaselineMissing);
    }

    #[test]
    fn valid_empty_source_is_not_an_error() {
        let result = decision_fixture(SuppressionMode::Enforce)
            .unchanged()
            .not_due()
            .evaluate();
        assert!(result.would_skip);
        assert!(result.vetoes.is_empty());
        assert!(
            result
                .snapshot
                .iter()
                .all(|snapshot| snapshot.available && snapshot.empty && snapshot.work_items.is_empty())
        );
    }

    #[test]
    fn evaluate_is_read_only() {
        let fixture = decision_fixture(SuppressionMode::Enforce);
        let before = fixture.state.clone().expect("fixture baseline");
        let _ = fixture.evaluate();
        assert_eq!(fixture.state.expect("fixture baseline"), before);
    }

    #[test]
    fn veto_and_decision_wire_values_are_stable() {
        for (veto, expected) in [
            (SkipVeto::Disabled, "disabled"),
            (SkipVeto::BaselineMissing, "baseline_missing"),
            (SkipVeto::BaselineCorrupt, "baseline_corrupt"),
            (SkipVeto::SourceUnavailable, "source_unavailable"),
            (SkipVeto::SourceChanged, "source_changed"),
            (SkipVeto::SourceRefMismatch, "source_ref_mismatch"),
            (SkipVeto::SourceRevisionConflict, "source_revision_conflict"),
            (SkipVeto::RegistryChanged, "registry_changed"),
            (SkipVeto::NewUserInput, "new_user_input"),
            (SkipVeto::ReconciliationDue, "reconciliation_due"),
            (SkipVeto::AuditFailed, "audit_failed"),
        ] {
            assert_eq!(serde_json::to_value(veto).expect("veto json"), json!(expected));
        }

        let decision = decision_fixture(SuppressionMode::Enforce)
            .unchanged()
            .not_due()
            .evaluate();
        let value = serde_json::to_value(&decision).expect("decision json");
        assert!(value.get("wouldSkip").is_some());
        assert_eq!(value["mode"], "enforce");
        assert!(value.get("vetoes").is_some());
        assert!(value.get("snapshot").is_some());
        assert!(value.get("baselineStateId").is_some());
        assert!(value.get("configVersion").is_some());
    }

    struct StaticSource {
        snapshot: SourceSnapshot,
    }

    #[async_trait::async_trait]
    impl AssistantSource for StaticSource {
        fn descriptor(&self) -> SourceDescriptor {
            SourceDescriptor {
                source_id: self.snapshot.source_id.clone(),
                source_version: self.snapshot.source_version,
            }
        }

        async fn capture(&self, _cx: &SourceContext) -> Result<SourceSnapshot, SuppressionError> {
            Ok(self.snapshot.clone())
        }

        async fn resolve_state(&self, _reference: &StateRef) -> Result<CanonicalBytes, SuppressionError> {
            Err(SuppressionError::new("source_ref_mismatch", "unused", "unused"))
        }

        async fn resolve_payload(&self, _reference: &PayloadRef) -> Result<CanonicalBytes, SuppressionError> {
            Err(SuppressionError::new("work_item_payload_missing", "unused", "unused"))
        }
    }

    #[tokio::test]
    async fn evaluate_captures_registered_sources_and_uses_config_mode() {
        let baseline = snapshot("test.static", "rev-1", hash_hex('1'));
        let mut registry = SourceRegistry::new();
        registry
            .register(Arc::new(StaticSource {
                snapshot: baseline.clone(),
            }))
            .expect("register source");
        let registry_version = registry.registry_version();
        let state = state_for(std::slice::from_ref(&baseline), &registry_version);
        let service = SuppressionService::new(Arc::new(registry));

        let mut context = SourceContext::new(WS, SESSION);
        // The service must apply the configured mode, not the caller's placeholder.
        context.mode = SourceMode::Off;
        context.observed_at = state.baseline_installed_at.expect("installed") + Duration::seconds(60);

        let decision = service
            .evaluate(context, Some(&state), &config(SuppressionMode::Enforce), 5)
            .await
            .expect("preflight");
        assert!(decision.would_skip);
        assert!(decision.vetoes.is_empty());
        assert_eq!(decision.snapshot.len(), 1);
        assert_eq!(decision.baseline_state_id.as_deref(), Some(STATE_ID));
    }

    #[tokio::test]
    async fn evaluate_off_short_circuits_without_capturing() {
        struct ExplodingSource;

        #[async_trait::async_trait]
        impl AssistantSource for ExplodingSource {
            fn descriptor(&self) -> SourceDescriptor {
                SourceDescriptor {
                    source_id: "test.exploding".to_string(),
                    source_version: SOURCE_VERSION,
                }
            }

            async fn capture(&self, _cx: &SourceContext) -> Result<SourceSnapshot, SuppressionError> {
                panic!("off must not capture Sources");
            }

            async fn resolve_state(&self, _reference: &StateRef) -> Result<CanonicalBytes, SuppressionError> {
                Err(SuppressionError::new("source_ref_mismatch", "unused", "unused"))
            }

            async fn resolve_payload(&self, _reference: &PayloadRef) -> Result<CanonicalBytes, SuppressionError> {
                Err(SuppressionError::new("work_item_payload_missing", "unused", "unused"))
            }
        }

        let mut registry = SourceRegistry::new();
        registry.register(Arc::new(ExplodingSource)).expect("register source");
        let service = SuppressionService::new(Arc::new(registry));
        let decision = service
            .evaluate(SourceContext::new(WS, SESSION), None, &config(SuppressionMode::Off), 0)
            .await
            .expect("disabled decision");
        assert_eq!(decision.vetoes, vec![SkipVeto::Disabled]);
        assert!(!decision.would_skip);
    }
}

/// Task 9 overload controls: bounded concurrency, bounded Source capture and
/// database contention/replay safety. The module name is the `cargo test`
/// filter (`suppression_overload`).
#[cfg(test)]
mod suppression_overload {
    use std::sync::Arc;
    use std::time::{Duration as StdDuration, Instant};

    use tinyiothub_storage::assistant_schedule::SuppressionConfig;
    use tinyiothub_storage::error::DbError;
    use tinyiothub_storage::suppression::{SuppressionMode, SuppressionState};

    use super::{
        DEFAULT_PREFLIGHT_CONCURRENCY, MAX_PREFLIGHT_CONCURRENCY, MIN_PREFLIGHT_CONCURRENCY,
        PREFLIGHT_BACKPRESSURE_RETRY_AFTER_MS, PreflightGate, PreflightLimits, SuppressionService,
    };
    use crate::domains::agent::session::suppression::contract::{
        CanonicalBytes, PayloadRef, SourceDescriptor, SourceSnapshot, StateRef,
    };
    use crate::domains::agent::session::suppression::error::SuppressionError;
    use crate::domains::agent::session::suppression::registry::{
        AssistantSource, SOURCE_VERSION, SourceContext, SourceRegistry,
    };

    const WS: &str = "ws-overload";
    const SESSION: &str = "agent:ws-overload:__assistant__ws-overload/main";

    fn config(mode: SuppressionMode) -> SuppressionConfig {
        SuppressionConfig {
            workspace_id: WS.to_string(),
            mode,
            reconciliation_seconds: tinyiothub_storage::suppression::DEFAULT_RECONCILIATION_SECONDS,
            config_version: 1,
        }
    }

    fn context() -> SourceContext {
        SourceContext::new(WS, SESSION)
    }

    fn hash_hex(character: char) -> String {
        format!("sha256:{}", character.to_string().repeat(64))
    }

    fn snapshot(source_id: &str) -> SourceSnapshot {
        let state_hash = hash_hex('a');
        SourceSnapshot {
            source_id: source_id.to_string(),
            source_version: SOURCE_VERSION,
            revision: "rev-a".to_string(),
            state_hash: state_hash.clone(),
            available: true,
            empty: true,
            state_ref: Some(StateRef {
                id: "state:rev-a".to_string(),
                revision: "rev-a".to_string(),
                state_hash,
            }),
            work_items: vec![],
            error: None,
        }
    }

    struct StaticSource {
        snapshot: SourceSnapshot,
    }

    #[async_trait::async_trait]
    impl AssistantSource for StaticSource {
        fn descriptor(&self) -> SourceDescriptor {
            SourceDescriptor {
                source_id: self.snapshot.source_id.clone(),
                source_version: self.snapshot.source_version,
            }
        }

        async fn capture(&self, _cx: &SourceContext) -> Result<SourceSnapshot, SuppressionError> {
            Ok(self.snapshot.clone())
        }

        async fn resolve_state(&self, _reference: &StateRef) -> Result<CanonicalBytes, SuppressionError> {
            Err(SuppressionError::new("source_ref_mismatch", "unused", "unused"))
        }

        async fn resolve_payload(&self, _reference: &PayloadRef) -> Result<CanonicalBytes, SuppressionError> {
            Err(SuppressionError::new("work_item_payload_missing", "unused", "unused"))
        }
    }

    struct HangingSource;

    #[async_trait::async_trait]
    impl AssistantSource for HangingSource {
        fn descriptor(&self) -> SourceDescriptor {
            SourceDescriptor {
                source_id: "test.hanging".to_string(),
                source_version: SOURCE_VERSION,
            }
        }

        async fn capture(&self, _cx: &SourceContext) -> Result<SourceSnapshot, SuppressionError> {
            std::future::pending::<()>().await;
            unreachable!("a pending capture never resolves")
        }

        async fn resolve_state(&self, _reference: &StateRef) -> Result<CanonicalBytes, SuppressionError> {
            Err(SuppressionError::new("source_ref_mismatch", "unused", "unused"))
        }

        async fn resolve_payload(&self, _reference: &PayloadRef) -> Result<CanonicalBytes, SuppressionError> {
            Err(SuppressionError::new("work_item_payload_missing", "unused", "unused"))
        }
    }

    struct SlowSource {
        delay: StdDuration,
        snapshot: SourceSnapshot,
    }

    #[async_trait::async_trait]
    impl AssistantSource for SlowSource {
        fn descriptor(&self) -> SourceDescriptor {
            SourceDescriptor {
                source_id: self.snapshot.source_id.clone(),
                source_version: self.snapshot.source_version,
            }
        }

        async fn capture(&self, _cx: &SourceContext) -> Result<SourceSnapshot, SuppressionError> {
            tokio::time::sleep(self.delay).await;
            Ok(self.snapshot.clone())
        }

        async fn resolve_state(&self, _reference: &StateRef) -> Result<CanonicalBytes, SuppressionError> {
            Err(SuppressionError::new("source_ref_mismatch", "unused", "unused"))
        }

        async fn resolve_payload(&self, _reference: &PayloadRef) -> Result<CanonicalBytes, SuppressionError> {
            Err(SuppressionError::new("work_item_payload_missing", "unused", "unused"))
        }
    }

    fn registry_with(source: Arc<dyn AssistantSource>) -> Arc<SourceRegistry> {
        let mut registry = SourceRegistry::new();
        registry.register(source).expect("register overload source");
        Arc::new(registry)
    }

    fn limits(capture: StdDuration, total: StdDuration) -> PreflightLimits {
        PreflightLimits {
            capture_deadline: capture,
            // Both deadlines are 2s by default; the test keeps them equal so
            // either bound is observable.
            resolve_deadline: capture,
            total_deadline: total,
        }
    }

    #[test]
    fn suppression_overload_concurrency_limit_is_runtime_configurable() {
        assert_eq!(
            PreflightGate::new(DEFAULT_PREFLIGHT_CONCURRENCY).limit(),
            DEFAULT_PREFLIGHT_CONCURRENCY
        );
        assert_eq!(PreflightGate::new(0).limit(), MIN_PREFLIGHT_CONCURRENCY);
        assert_eq!(PreflightGate::new(usize::MAX).limit(), MAX_PREFLIGHT_CONCURRENCY);

        let gate = PreflightGate::new(2);
        let first = gate.try_acquire().expect("first permit");
        let second = gate.try_acquire().expect("second permit");
        assert!(
            gate.try_acquire().is_none(),
            "a saturated gate must refuse instead of queueing"
        );
        drop(first);
        let third = gate.try_acquire().expect("a released permit is reusable");
        assert_eq!(gate.in_flight(), 2);
        drop(second);
        assert_eq!(gate.in_flight(), 1);
        drop(third);
        assert_eq!(gate.in_flight(), 0);

        gate.set_limit(0);
        assert_eq!(gate.limit(), MIN_PREFLIGHT_CONCURRENCY);
        gate.set_limit(1_000);
        assert_eq!(gate.limit(), MAX_PREFLIGHT_CONCURRENCY);
    }

    #[tokio::test]
    async fn suppression_overload_hanging_adapter_times_out_without_skipping() {
        let gate = PreflightGate::new(DEFAULT_PREFLIGHT_CONCURRENCY);
        let service = SuppressionService::with_gate(registry_with(Arc::new(HangingSource)), gate.clone())
            .with_limits(limits(StdDuration::from_millis(30), StdDuration::from_millis(150)));
        let baseline = SuppressionState::initial(WS, SESSION);
        let before = baseline.clone();

        let started = Instant::now();
        let error = service
            .evaluate(context(), Some(&baseline), &config(SuppressionMode::Enforce), 0)
            .await
            .expect_err("a hanging adapter must time out");

        assert_eq!(error.code, "preflight_timeout");
        assert_eq!(error.recovery.side_effects, "none");
        assert!(
            !error.recovery.baseline_advanced,
            "a preflight timeout may never advance the baseline"
        );
        assert!(started.elapsed() < StdDuration::from_secs(1), "timeout must be prompt");
        assert_eq!(gate.in_flight(), 0, "the timed-out attempt must release its permit");
        assert_eq!(baseline, before, "preflight is read-only even on timeout");
    }

    #[tokio::test]
    async fn suppression_overload_source_timeout_forbids_skip_and_does_not_hold_capacity() {
        let gate = PreflightGate::new(1);
        let service = SuppressionService::with_gate(registry_with(Arc::new(HangingSource)), gate.clone())
            .with_limits(limits(StdDuration::from_millis(20), StdDuration::from_millis(60)));
        assert!(gate.has_capacity());

        let error = service
            .evaluate(context(), None, &config(SuppressionMode::Enforce), 0)
            .await
            .expect_err("hanging capture");
        assert_eq!(error.code, "preflight_timeout");
        assert!(
            gate.has_capacity(),
            "a timed-out attempt must not hold the global slot forever"
        );
        assert_eq!(gate.in_flight(), 0);
    }

    #[tokio::test]
    async fn suppression_overload_saturation_returns_backpressure_without_queueing() {
        let gate = PreflightGate::new(1);
        let held = gate.try_acquire().expect("hold the only slot");
        let service = SuppressionService::with_gate(
            registry_with(Arc::new(StaticSource {
                snapshot: snapshot("test.static"),
            })),
            gate.clone(),
        )
        .with_limits(limits(StdDuration::from_secs(2), StdDuration::from_secs(4)));

        let started = Instant::now();
        let error = service
            .evaluate(context(), None, &config(SuppressionMode::Enforce), 0)
            .await
            .expect_err("saturated preflight must backpressure immediately");

        assert_eq!(error.code, "wake_backpressure");
        assert_eq!(error.recovery.side_effects, "none");
        assert!(!error.recovery.baseline_advanced);
        assert_eq!(
            error.context["retryAfterMs"],
            serde_json::json!(PREFLIGHT_BACKPRESSURE_RETRY_AFTER_MS)
        );
        assert!(
            started.elapsed() < StdDuration::from_millis(100),
            "backpressure must not wait for a queued slot"
        );
        assert_eq!(gate.in_flight(), 1, "the failed admission must not consume a slot");
        drop(held);
        assert_eq!(gate.in_flight(), 0);
    }

    #[tokio::test]
    async fn suppression_overload_off_mode_never_consumes_capacity() {
        let gate = PreflightGate::new(1);
        let held = gate.try_acquire().expect("hold the only slot");
        let service = SuppressionService::with_gate(registry_with(Arc::new(HangingSource)), gate.clone());

        let decision = service
            .evaluate(context(), None, &config(SuppressionMode::Off), 0)
            .await
            .expect("off is the kill switch and must not capture");
        assert!(!decision.would_skip);
        assert_eq!(decision.vetoes, vec![super::SkipVeto::Disabled]);
        assert_eq!(gate.in_flight(), 1, "off must be independent of the capture gate");
        drop(held);
    }

    #[tokio::test]
    async fn suppression_overload_synchronized_burst_is_bounded() {
        let gate = PreflightGate::new(2);
        let service = Arc::new(
            SuppressionService::with_gate(
                registry_with(Arc::new(SlowSource {
                    delay: StdDuration::from_millis(100),
                    snapshot: snapshot("test.slow"),
                })),
                gate.clone(),
            )
            .with_limits(limits(StdDuration::from_secs(2), StdDuration::from_secs(4))),
        );
        let config = config(SuppressionMode::Enforce);

        let started = Instant::now();
        let mut handles = Vec::new();
        for _ in 0..8 {
            let service = Arc::clone(&service);
            let config = config.clone();
            handles.push(tokio::spawn(async move {
                service.evaluate(context(), None, &config, 0).await
            }));
        }
        let mut accepted = 0;
        let mut rejected = 0;
        for handle in handles {
            match handle.await.expect("burst task") {
                Ok(_) => accepted += 1,
                Err(error) => {
                    assert_eq!(error.code, "wake_backpressure");
                    rejected += 1;
                }
            }
        }
        assert_eq!(accepted, 2, "only the configured concurrency may run");
        assert_eq!(rejected, 6, "the rest are rejected, never queued");
        assert!(
            started.elapsed() < StdDuration::from_millis(500),
            "saturation must not serialize the burst into a queue"
        );
        assert_eq!(gate.in_flight(), 0);
    }

    #[tokio::test]
    async fn suppression_overload_sqlite_contention_keeps_one_full_attempt() {
        let fixture = tinyiothub_storage::test_helpers::fixture_workspace(WS).await;
        let db = Arc::new(fixture.db.clone());
        let baseline = SuppressionState::initial(WS, SESSION);

        let mut handles = Vec::new();
        for _ in 0..8 {
            let db = Arc::clone(&db);
            let baseline = baseline.clone();
            handles.push(tokio::spawn(
                async move { db.begin_full_attempt(SESSION, &baseline).await },
            ));
        }
        let mut accepted = 0;
        let mut conflicts = 0;
        for handle in handles {
            match handle.await.expect("contention task") {
                Ok(_) => accepted += 1,
                Err(DbError::CasConflict { .. }) => conflicts += 1,
                Err(error) => panic!("SQLite contention must not surface as {error:?}"),
            }
        }
        assert_eq!(accepted, 1, "suppression_state CAS admits exactly one full attempt");
        assert_eq!(conflicts, 7);

        // Reads stay available while the durable fence is in place.
        let mut readers = Vec::new();
        for _ in 0..16 {
            let db = Arc::clone(&db);
            readers.push(tokio::spawn(
                async move { db.load_suppression_state(WS, SESSION).await },
            ));
        }
        for reader in readers {
            let state = reader.await.expect("read task").expect("read must not be busy");
            assert!(state.expect("state row").full_attempt_in_progress);
        }

        drop(db);
        fixture.close().await;
    }

    #[tokio::test]
    async fn suppression_overload_replay_storm_has_one_winner() {
        let fixture = tinyiothub_storage::test_helpers::fixture_workspace(WS).await;
        let db = Arc::new(fixture.db.clone());
        let mut handles = Vec::new();
        for _ in 0..24 {
            let db = Arc::clone(&db);
            handles.push(tokio::spawn(async move {
                db.try_insert_suppression_idempotency(WS, "wake", "storm-key", "sha256:fingerprint", None)
                    .await
            }));
        }
        let mut winners = 0;
        for handle in handles {
            if handle.await.expect("storm task").expect("insert") {
                winners += 1;
            }
        }
        assert_eq!(winners, 1, "the unique key must pick exactly one winner");

        let record = db
            .find_suppression_idempotency(WS, "storm-key")
            .await
            .expect("read winner")
            .expect("winner row");
        assert_eq!(record.operation, "wake");
        assert_eq!(record.request_fingerprint, "sha256:fingerprint");
        assert!(record.result.is_none(), "the reservation has no result yet");

        // A new intent with a different key is admitted independently.
        assert!(
            db.try_insert_suppression_idempotency(WS, "wake", "other-key", "sha256:fingerprint", None)
                .await
                .expect("insert other key")
        );

        drop(db);
        fixture.close().await;
    }

    #[test]
    fn suppression_overload_retry_after_is_a_stable_wire_value() {
        assert_eq!(PREFLIGHT_BACKPRESSURE_RETRY_AFTER_MS, 1_000);
    }
}
