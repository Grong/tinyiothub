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

use chrono::{DateTime, Duration, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tinyiothub_storage::assistant_schedule::SuppressionConfig;
use tinyiothub_storage::suppression::{
    CanaryDenominators, CanaryMeasured, DEFAULT_RECONCILIATION_SECONDS, FALSE_SKIP_CONCLUSION_NONE,
    LatencyDistribution, MAX_RECONCILIATION_SECONDS, MIN_RECONCILIATION_SECONDS, OUTCOME_AUDIT_FAILED,
    OUTCOME_BASELINE_MISSING, OUTCOME_FULL_COMPLETED, OUTCOME_FULL_FAILED, OUTCOME_FULL_IN_PROGRESS,
    OUTCOME_SKIP_IN_PROGRESS, OUTCOME_SKIPPED, ShadowDenominators, ShadowEstimates, SuppressionGateArtifact,
    SuppressionGateMode, SuppressionMode, SuppressionState,
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

// ── Rollout gate evidence (Task 10) ──
//
// Shadow produces counterfactual *estimated* numbers with explicit
// denominators. Canary produces *measured* reductions and a human false-skip
// review. An immutable, approved artifact is the only evidence that unlocks
// the next mode; `off` is always allowed.

/// Shadow evidence must cover at least three representative days.
pub const GATE_MIN_SHADOW_DAYS: usize = 3;
/// Shadow evidence must cover at least 100 scheduled Turns.
pub const GATE_MIN_SHADOW_TURNS: u64 = 100;
/// At least half of shadow Turns must have been wouldSkip.
pub const GATE_MIN_WOULD_SKIP_RATIO: f64 = 0.5;
/// The canary must observe at least 20 real Turns.
pub const GATE_MIN_CANARY_TURNS: u64 = 20;
/// Measured model and tool calls must each drop by at least 40%.
pub const GATE_MIN_CANARY_REDUCTION: f64 = 0.4;
/// Human-labelled duplicate reports must drop by at least 80%.
pub const GATE_MIN_DUPLICATE_REPORT_REDUCTION: f64 = 0.8;
/// Preflight p95 may not exceed 250ms.
pub const GATE_MAX_PREFLIGHT_P95_MS: u64 = 250;

const GATE_RATIO_EPSILON: f64 = 1e-6;

/// One observed shadow Turn. Shadow never removes model or tool calls; these
/// are the counts that actually ran while the decision was being recorded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShadowTurnObservation {
    pub turn_id: String,
    pub date: String,
    pub would_skip: bool,
    pub vetoes: Vec<String>,
    pub model_calls: u64,
    pub model_driven_tool_calls: u64,
    pub unavailable_source_full_execution: bool,
    pub reconciliation_full_execution: bool,
    pub preflight_latency_ms: u64,
}

/// One measured canary Turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanaryTurnObservation {
    pub turn_id: String,
    pub date: String,
    pub model_calls: u64,
    pub model_driven_tool_calls: u64,
    pub duplicate_reports: u64,
    pub preflight_latency_ms: u64,
}

/// The pre-canary baseline the measured canary Turns are compared against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CanaryBaseline {
    pub model_calls: u64,
    pub model_driven_tool_calls: u64,
    pub duplicate_reports: u64,
}

fn ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn reduction(baseline: u64, measured: u64) -> f64 {
    if baseline == 0 {
        0.0
    } else {
        1.0 - measured as f64 / baseline as f64
    }
}

/// Nearest-rank percentile over an unsorted sample.
pub fn latency_distribution(samples: &[u64]) -> LatencyDistribution {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    LatencyDistribution {
        sample_count: sorted.len() as u64,
        p50_ms: percentile(&sorted, 50),
        p95_ms: percentile(&sorted, 95),
        p99_ms: percentile(&sorted, 99),
    }
}

fn percentile(sorted: &[u64], percentile: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = (sorted.len() * percentile).div_ceil(100).max(1);
    sorted[rank - 1]
}

/// Estimated model savings = model calls on wouldSkip Turns / all shadow
/// model calls. Estimated tool savings uses model-driven tool calls. Every
/// value is counterfactual: shadow never claims an already-realized saving.
pub fn compute_shadow_metrics(
    observations: &[ShadowTurnObservation],
) -> (ShadowDenominators, ShadowEstimates, LatencyDistribution) {
    let mut would_skip_turns = 0;
    let mut model_calls = 0;
    let mut tool_calls = 0;
    let mut model_calls_on_would_skip = 0;
    let mut tool_calls_on_would_skip = 0;
    let mut unavailable_source_full_executions = 0;
    let mut reconciliation_full_executions = 0;
    let mut veto_distribution: std::collections::BTreeMap<String, u64> = std::collections::BTreeMap::new();
    let mut streak = 0;
    let mut longest_consecutive_skips = 0;
    let mut latencies = Vec::with_capacity(observations.len());

    for observation in observations {
        model_calls += observation.model_calls;
        tool_calls += observation.model_driven_tool_calls;
        if observation.would_skip {
            would_skip_turns += 1;
            model_calls_on_would_skip += observation.model_calls;
            tool_calls_on_would_skip += observation.model_driven_tool_calls;
            streak += 1;
            longest_consecutive_skips = longest_consecutive_skips.max(streak);
        } else {
            streak = 0;
        }
        for veto in &observation.vetoes {
            *veto_distribution.entry(veto.clone()).or_insert(0) += 1;
        }
        if observation.unavailable_source_full_execution {
            unavailable_source_full_executions += 1;
        }
        if observation.reconciliation_full_execution {
            reconciliation_full_executions += 1;
        }
        latencies.push(observation.preflight_latency_ms);
    }

    let denominators = ShadowDenominators {
        turns: observations.len() as u64,
        would_skip_turns,
        model_calls,
        model_driven_tool_calls: tool_calls,
        model_calls_on_would_skip_turns: model_calls_on_would_skip,
        tool_calls_on_would_skip_turns: tool_calls_on_would_skip,
        unavailable_source_full_executions,
        reconciliation_full_executions,
    };
    let estimates = ShadowEstimates {
        estimated: true,
        would_skip_ratio: ratio(would_skip_turns, denominators.turns),
        estimated_model_savings: ratio(model_calls_on_would_skip, model_calls),
        estimated_tool_savings: ratio(tool_calls_on_would_skip, tool_calls),
        veto_distribution,
        longest_consecutive_skips,
    };
    (denominators, estimates, latency_distribution(&latencies))
}

/// Measured canary reductions. `measured=true` marks the numbers as observed
/// rather than estimated.
pub fn compute_canary_metrics(
    observations: &[CanaryTurnObservation],
    baseline: &CanaryBaseline,
) -> (CanaryDenominators, CanaryMeasured, LatencyDistribution) {
    let mut model_calls = 0;
    let mut tool_calls = 0;
    let mut duplicate_reports = 0;
    let mut latencies = Vec::with_capacity(observations.len());
    for observation in observations {
        model_calls += observation.model_calls;
        tool_calls += observation.model_driven_tool_calls;
        duplicate_reports += observation.duplicate_reports;
        latencies.push(observation.preflight_latency_ms);
    }
    let denominators = CanaryDenominators {
        turns: observations.len() as u64,
        baseline_model_calls: baseline.model_calls,
        model_calls,
        baseline_model_driven_tool_calls: baseline.model_driven_tool_calls,
        model_driven_tool_calls: tool_calls,
        baseline_duplicate_reports: baseline.duplicate_reports,
        duplicate_reports,
    };
    let measured = CanaryMeasured {
        measured: true,
        model_call_reduction: reduction(baseline.model_calls, model_calls),
        tool_call_reduction: reduction(baseline.model_driven_tool_calls, tool_calls),
        duplicate_report_reduction: reduction(baseline.duplicate_reports, duplicate_reports),
    };
    (denominators, measured, latency_distribution(&latencies))
}

fn truncate_to_seconds(timestamp: DateTime<Utc>) -> DateTime<Utc> {
    Utc.timestamp_opt(timestamp.timestamp(), 0)
        .single()
        .unwrap_or(timestamp)
}

/// Fill the artifact's canonical SHA-256 after normalizing timestamps to the
/// precision the append-only store round-trips.
pub fn seal_gate_artifact(mut artifact: SuppressionGateArtifact) -> SuppressionGateArtifact {
    artifact.approved_at = truncate_to_seconds(artifact.approved_at);
    artifact.false_skip_review.reviewed_at = truncate_to_seconds(artifact.false_skip_review.reviewed_at);
    artifact.content_sha256 = gate_artifact_sha256(&artifact);
    artifact
}

/// SHA-256 over canonical (JCS) artifact bytes, excluding the hash field.
pub fn gate_artifact_sha256(artifact: &SuppressionGateArtifact) -> String {
    let content = tinyiothub_storage::suppression::gate_artifact_content_value(artifact);
    let bytes = super::canonical::canonical_bytes(&content)
        .unwrap_or_else(|error| panic!("gate artifact must canonicalize: {error:?}"));
    format!("sha256:{:x}", Sha256::digest(&bytes))
}

/// Everything staleness and provenance validation depends on that is not
/// stored inside the immutable artifact itself.
#[derive(Clone, Copy, Debug)]
pub struct GateValidationContext<'a> {
    pub workspace_id: &'a str,
    pub current_registry_version: &'a str,
    /// Latest confirmed false-skip event for the workspace, if any (R18).
    pub latest_confirmed_false_skip_at: Option<DateTime<Utc>>,
    /// Only the loopback synthetic fixture may set this.
    pub allow_synthetic: bool,
}

fn gate_not_met(reason: impl Into<String>) -> SuppressionError {
    let mut error = SuppressionError::from_code(ErrorCode::ShadowGateNotMet);
    error.what = reason.into();
    error
}

fn close(left: f64, right: f64) -> bool {
    (left - right).abs() <= GATE_RATIO_EPSILON
}

/// Validate a mode change against approved evidence.
///
/// `off` and `shadow` are always allowed (the kill switch and observation
/// modes). `canary` requires a valid approved shadow artifact; `enforce`
/// requires a valid approved canary-measured artifact. Every rejection is the
/// stable `shadow_gate_not_met` error; there is no bypass parameter.
pub fn validate_transition(
    target: SuppressionMode,
    artifact: &SuppressionGateArtifact,
    context: &GateValidationContext<'_>,
) -> Result<(), SuppressionError> {
    if matches!(target, SuppressionMode::Off | SuppressionMode::Shadow) {
        return Ok(());
    }
    if artifact.workspace_id != context.workspace_id {
        return Err(gate_not_met("gate artifact belongs to a different workspace"));
    }
    if artifact.approver.trim().is_empty() {
        return Err(gate_not_met("gate artifact has no approver"));
    }
    if artifact.sample.turn_ids.is_empty() || artifact.sample.dates.is_empty() {
        return Err(gate_not_met("gate artifact has no sample turn ids or dates"));
    }
    if artifact.sample.registry_version != context.current_registry_version {
        return Err(gate_not_met(
            "gate artifact registryVersion does not match the current registry",
        ));
    }
    if artifact.sample.synthetic && !context.allow_synthetic {
        return Err(gate_not_met(
            "synthetic fixture evidence is only valid in the loopback fixture",
        ));
    }
    if artifact.content_sha256 != gate_artifact_sha256(artifact) {
        return Err(gate_not_met("gate artifact SHA-256 does not cover its canonical bytes"));
    }
    if let Some(false_skip_at) = context.latest_confirmed_false_skip_at
        && false_skip_at > artifact.approved_at
    {
        return Err(gate_not_met(
            "a confirmed false skip after approval invalidates this artifact; fix the root cause and approve fresh evidence",
        ));
    }

    match target {
        SuppressionMode::Canary => validate_shadow_artifact(artifact),
        SuppressionMode::Enforce => validate_canary_artifact(artifact),
        SuppressionMode::Off | SuppressionMode::Shadow => Ok(()),
    }
}

fn validate_shadow_artifact(artifact: &SuppressionGateArtifact) -> Result<(), SuppressionError> {
    if artifact.mode != SuppressionGateMode::Shadow {
        return Err(gate_not_met(
            "canary requires an approved shadow artifact, not a canary artifact",
        ));
    }
    let Some(denominators) = artifact.denominators.shadow.as_ref() else {
        return Err(gate_not_met("shadow artifact is missing its denominators"));
    };
    if denominators.turns < GATE_MIN_SHADOW_TURNS {
        return Err(gate_not_met(format!(
            "shadow artifact covers {} Turns; at least {GATE_MIN_SHADOW_TURNS} are required",
            denominators.turns
        )));
    }
    let days = artifact.sample.dates.iter().collect::<HashSet<_>>().len();
    if days < GATE_MIN_SHADOW_DAYS {
        return Err(gate_not_met(format!(
            "shadow artifact covers {days} representative days; at least {GATE_MIN_SHADOW_DAYS} are required"
        )));
    }
    if !artifact.shadow_estimates.estimated {
        return Err(gate_not_met("shadow metrics must be labelled estimated"));
    }
    if denominators.would_skip_turns > denominators.turns {
        return Err(gate_not_met("shadow wouldSkip count exceeds the shadow Turn count"));
    }
    if denominators.model_calls == 0 || denominators.model_driven_tool_calls == 0 {
        return Err(gate_not_met("shadow artifact is missing model or tool denominators"));
    }
    if denominators.model_calls_on_would_skip_turns > denominators.model_calls
        || denominators.tool_calls_on_would_skip_turns > denominators.model_driven_tool_calls
    {
        return Err(gate_not_met("shadow wouldSkip numerators exceed their denominators"));
    }
    let derived_ratio = ratio(denominators.would_skip_turns, denominators.turns);
    if !close(artifact.shadow_estimates.would_skip_ratio, derived_ratio) {
        return Err(gate_not_met(
            "shadow wouldSkip ratio does not match its denominator counts",
        ));
    }
    if artifact.shadow_estimates.would_skip_ratio < GATE_MIN_WOULD_SKIP_RATIO {
        return Err(gate_not_met(format!(
            "shadow wouldSkip ratio {:.3} is below {GATE_MIN_WOULD_SKIP_RATIO}",
            artifact.shadow_estimates.would_skip_ratio
        )));
    }
    if !close(
        artifact.shadow_estimates.estimated_model_savings,
        ratio(denominators.model_calls_on_would_skip_turns, denominators.model_calls),
    ) {
        return Err(gate_not_met(
            "estimated model savings do not match their denominator counts",
        ));
    }
    if !close(
        artifact.shadow_estimates.estimated_tool_savings,
        ratio(
            denominators.tool_calls_on_would_skip_turns,
            denominators.model_driven_tool_calls,
        ),
    ) {
        return Err(gate_not_met(
            "estimated tool savings do not match their denominator counts",
        ));
    }
    Ok(())
}

fn validate_canary_artifact(artifact: &SuppressionGateArtifact) -> Result<(), SuppressionError> {
    if artifact.mode != SuppressionGateMode::Canary {
        return Err(gate_not_met("enforce requires an approved canary-measured artifact"));
    }
    let Some(denominators) = artifact.denominators.canary.as_ref() else {
        return Err(gate_not_met("canary artifact is missing its denominators"));
    };
    if denominators.turns < GATE_MIN_CANARY_TURNS {
        return Err(gate_not_met(format!(
            "canary artifact covers {} Turns; at least {GATE_MIN_CANARY_TURNS} are required",
            denominators.turns
        )));
    }
    if denominators.baseline_model_calls == 0
        || denominators.baseline_model_driven_tool_calls == 0
        || denominators.baseline_duplicate_reports == 0
    {
        return Err(gate_not_met("canary artifact is missing baseline denominators"));
    }
    if !artifact.canary_measured.measured {
        return Err(gate_not_met("canary metrics must be labelled measured"));
    }
    if !zero_or_positive(artifact.canary_measured.model_call_reduction) {
        return Err(gate_not_met("canary model reduction is not a finite number"));
    }
    if !close(
        artifact.canary_measured.model_call_reduction,
        reduction(denominators.baseline_model_calls, denominators.model_calls),
    ) {
        return Err(gate_not_met(
            "canary model reduction does not match its denominator counts",
        ));
    }
    if !close(
        artifact.canary_measured.tool_call_reduction,
        reduction(
            denominators.baseline_model_driven_tool_calls,
            denominators.model_driven_tool_calls,
        ),
    ) {
        return Err(gate_not_met(
            "canary tool reduction does not match its denominator counts",
        ));
    }
    if !close(
        artifact.canary_measured.duplicate_report_reduction,
        reduction(denominators.baseline_duplicate_reports, denominators.duplicate_reports),
    ) {
        return Err(gate_not_met(
            "duplicate-report reduction does not match its denominator counts",
        ));
    }
    if artifact.canary_measured.model_call_reduction < GATE_MIN_CANARY_REDUCTION {
        return Err(gate_not_met(format!(
            "canary model reduction {:.3} is below {GATE_MIN_CANARY_REDUCTION}",
            artifact.canary_measured.model_call_reduction
        )));
    }
    if artifact.canary_measured.tool_call_reduction < GATE_MIN_CANARY_REDUCTION {
        return Err(gate_not_met(format!(
            "canary tool reduction {:.3} is below {GATE_MIN_CANARY_REDUCTION}",
            artifact.canary_measured.tool_call_reduction
        )));
    }
    if artifact.canary_measured.duplicate_report_reduction < GATE_MIN_DUPLICATE_REPORT_REDUCTION {
        return Err(gate_not_met(format!(
            "duplicate-report reduction {:.3} is below {GATE_MIN_DUPLICATE_REPORT_REDUCTION}",
            artifact.canary_measured.duplicate_report_reduction
        )));
    }
    if artifact.latency.sample_count == 0 || artifact.latency.p95_ms > GATE_MAX_PREFLIGHT_P95_MS {
        return Err(gate_not_met(format!(
            "preflight p95 {}ms exceeds {GATE_MAX_PREFLIGHT_P95_MS}ms or has no samples",
            artifact.latency.p95_ms
        )));
    }

    let review = &artifact.false_skip_review;
    if review.reviewer.trim().is_empty() {
        return Err(gate_not_met("false-skip review has no reviewer"));
    }
    if review.samples.is_empty() {
        return Err(gate_not_met("false-skip review has no sample turn ids"));
    }
    if review.samples.iter().any(|sample| !sample.reviewed) {
        return Err(gate_not_met("an unreviewed wouldSkip sample blocks enforce"));
    }
    let confirmed = review
        .samples
        .iter()
        .filter(|sample| sample.confirmed_false_skip)
        .count() as u64;
    if confirmed > 0 || review.confirmed_false_skips > 0 || review.confirmed_false_skips != confirmed {
        return Err(gate_not_met(
            "a confirmed false skip blocks enforce until a fresh artifact is approved",
        ));
    }
    if review.conclusion != FALSE_SKIP_CONCLUSION_NONE {
        return Err(gate_not_met(
            "false-skip review conclusion is not no_confirmed_false_skip",
        ));
    }
    Ok(())
}

fn zero_or_positive(value: f64) -> bool {
    value.is_finite() && value >= 0.0
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

/// Task 10 gate evidence: estimated shadow metrics, measured canary metrics
/// and evidence-gated `shadow -> canary -> enforce` transitions.
///
/// The module name is the `cargo test` filter (`suppression_gate`). Fixture
/// builders are `pub(crate)` so the HTTP contract tests reuse the same sealed
/// artifacts instead of hand-crafting bytes.
#[cfg(test)]
pub(crate) mod suppression_gate {
    use std::collections::BTreeMap;
    use std::ops::Deref;

    use chrono::{DateTime, Duration, TimeZone, Utc};
    use tinyiothub_storage::error::DbError;
    use tinyiothub_storage::suppression::{
        CanaryDenominators, CanaryMeasured, FALSE_SKIP_CONCLUSION_NONE, FalseSkipReview, FalseSkipSample,
        GateDenominators, GateSample, LatencyDistribution, ShadowDenominators, ShadowEstimates,
        SuppressionGateArtifact, SuppressionGateMode, SuppressionMode,
    };

    use super::{
        CanaryBaseline, CanaryTurnObservation, GateValidationContext, ShadowTurnObservation, compute_canary_metrics,
        compute_shadow_metrics, gate_artifact_sha256, seal_gate_artifact, validate_transition,
    };
    use crate::domains::agent::session::suppression::error::SuppressionError;

    pub(crate) const GATE_WS: &str = "ws-gate-fixture";
    pub(crate) const GATE_REGISTRY: &str = "sha256:gate-registry-v1";

    fn ts(seconds: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(seconds, 0).single().expect("timestamp")
    }

    fn shadow_estimates_fixture() -> ShadowEstimates {
        ShadowEstimates {
            estimated: true,
            would_skip_ratio: 0.6,
            estimated_model_savings: 0.6,
            estimated_tool_savings: 0.5,
            veto_distribution: BTreeMap::from([
                ("source_changed".to_string(), 30),
                ("reconciliation_due".to_string(), 12),
            ]),
            longest_consecutive_skips: 4,
        }
    }

    fn measured_fixture() -> CanaryMeasured {
        CanaryMeasured {
            measured: true,
            model_call_reduction: 0.4,
            tool_call_reduction: 0.4,
            duplicate_report_reduction: 0.8,
        }
    }

    /// A sealed artifact whose mutations always re-seal, so every builder
    /// step produces bytes whose `contentSha256` still verifies.
    pub(crate) struct GateFixture {
        artifact: SuppressionGateArtifact,
    }

    impl Deref for GateFixture {
        type Target = SuppressionGateArtifact;

        fn deref(&self) -> &Self::Target {
            &self.artifact
        }
    }

    impl GateFixture {
        /// Valid shadow evidence: 100 turns over 3 days with 60% wouldSkip.
        pub(crate) fn shadow(workspace_id: &str) -> Self {
            let turn_ids = (0..100).map(|index| format!("shadow-turn-{index:03}")).collect();
            Self::seal(SuppressionGateArtifact {
                artifact_id: format!("gate:shadow:{workspace_id}"),
                workspace_id: workspace_id.to_string(),
                mode: SuppressionGateMode::Shadow,
                sample: GateSample {
                    turn_ids,
                    dates: vec![
                        "2026-11-10".to_string(),
                        "2026-11-11".to_string(),
                        "2026-11-12".to_string(),
                    ],
                    registry_version: GATE_REGISTRY.to_string(),
                    synthetic: false,
                },
                denominators: GateDenominators {
                    shadow: Some(ShadowDenominators {
                        turns: 100,
                        would_skip_turns: 60,
                        model_calls: 1_000,
                        model_driven_tool_calls: 500,
                        model_calls_on_would_skip_turns: 600,
                        tool_calls_on_would_skip_turns: 250,
                        unavailable_source_full_executions: 4,
                        reconciliation_full_executions: 2,
                    }),
                    canary: None,
                },
                shadow_estimates: shadow_estimates_fixture(),
                canary_measured: CanaryMeasured {
                    measured: false,
                    model_call_reduction: 0.0,
                    tool_call_reduction: 0.0,
                    duplicate_report_reduction: 0.0,
                },
                false_skip_review: FalseSkipReview {
                    reviewer: "reviewer-1".to_string(),
                    reviewed_at: ts(1_900_000_000),
                    conclusion: FALSE_SKIP_CONCLUSION_NONE.to_string(),
                    confirmed_false_skips: 0,
                    samples: vec![],
                },
                latency: LatencyDistribution {
                    sample_count: 100,
                    p50_ms: 40,
                    p95_ms: 120,
                    p99_ms: 200,
                },
                approver: "approver-1".to_string(),
                approved_at: ts(1_900_000_000),
                content_sha256: String::new(),
            })
        }

        /// Valid canary evidence: 20 turns, 40% model/tool reduction, 80%
        /// duplicate-report reduction and p95 = 250ms.
        pub(crate) fn canary(workspace_id: &str) -> Self {
            let turn_ids = (0..20).map(|index| format!("canary-turn-{index:03}")).collect();
            Self::seal(SuppressionGateArtifact {
                artifact_id: format!("gate:canary:{workspace_id}"),
                workspace_id: workspace_id.to_string(),
                mode: SuppressionGateMode::Canary,
                sample: GateSample {
                    turn_ids,
                    dates: vec![
                        "2026-11-13".to_string(),
                        "2026-11-14".to_string(),
                        "2026-11-15".to_string(),
                    ],
                    registry_version: GATE_REGISTRY.to_string(),
                    synthetic: false,
                },
                denominators: GateDenominators {
                    shadow: Some(ShadowDenominators {
                        turns: 100,
                        would_skip_turns: 60,
                        model_calls: 1_000,
                        model_driven_tool_calls: 500,
                        model_calls_on_would_skip_turns: 600,
                        tool_calls_on_would_skip_turns: 250,
                        unavailable_source_full_executions: 4,
                        reconciliation_full_executions: 2,
                    }),
                    canary: Some(CanaryDenominators {
                        turns: 20,
                        baseline_model_calls: 1_000,
                        model_calls: 600,
                        baseline_model_driven_tool_calls: 1_000,
                        model_driven_tool_calls: 600,
                        baseline_duplicate_reports: 1_000,
                        duplicate_reports: 200,
                    }),
                },
                shadow_estimates: shadow_estimates_fixture(),
                canary_measured: measured_fixture(),
                false_skip_review: FalseSkipReview {
                    reviewer: "reviewer-1".to_string(),
                    reviewed_at: ts(1_900_000_001),
                    conclusion: FALSE_SKIP_CONCLUSION_NONE.to_string(),
                    confirmed_false_skips: 0,
                    samples: vec![FalseSkipSample {
                        turn_id: "canary-turn-000".to_string(),
                        reviewed: true,
                        confirmed_false_skip: false,
                    }],
                },
                latency: LatencyDistribution {
                    sample_count: 20,
                    p50_ms: 40,
                    p95_ms: 250,
                    p99_ms: 300,
                },
                approver: "approver-1".to_string(),
                approved_at: ts(1_900_000_001),
                content_sha256: String::new(),
            })
        }

        fn seal(artifact: SuppressionGateArtifact) -> Self {
            Self {
                artifact: seal_gate_artifact(artifact),
            }
        }

        pub(crate) fn mutate(mut self, mutate: impl FnOnce(&mut SuppressionGateArtifact)) -> Self {
            mutate(&mut self.artifact);
            self.artifact = seal_gate_artifact(self.artifact);
            self
        }

        pub(crate) fn with_shadow_only(self) -> Self {
            self.mutate(|artifact| artifact.mode = SuppressionGateMode::Shadow)
        }

        pub(crate) fn with_would_skip(self, ratio: f64) -> Self {
            self.mutate(|artifact| {
                let Some(shadow) = artifact.denominators.shadow.as_mut() else {
                    panic!("shadow fixture");
                };
                shadow.would_skip_turns = (shadow.turns as f64 * ratio).round() as u64;
                artifact.shadow_estimates.would_skip_ratio = shadow.would_skip_turns as f64 / shadow.turns as f64;
            })
        }

        pub(crate) fn with_approver(self, approver: &str) -> Self {
            self.mutate(|artifact| artifact.approver = approver.to_string())
        }

        pub(crate) fn with_empty_sample_turn_ids(self) -> Self {
            self.mutate(|artifact| artifact.sample.turn_ids.clear())
        }

        pub(crate) fn with_sample_dates(self, dates: &[&str]) -> Self {
            self.mutate(|artifact| artifact.sample.dates = dates.iter().map(|date| date.to_string()).collect())
        }

        pub(crate) fn with_shadow_turns(self, turns: u64) -> Self {
            self.mutate(|artifact| {
                let Some(shadow) = artifact.denominators.shadow.as_mut() else {
                    panic!("shadow fixture");
                };
                shadow.turns = turns;
                shadow.would_skip_turns = shadow.would_skip_turns.min(turns);
                artifact.shadow_estimates.would_skip_ratio =
                    shadow.would_skip_turns as f64 / shadow.turns.max(1) as f64;
            })
        }

        pub(crate) fn without_shadow_denominators(self) -> Self {
            self.mutate(|artifact| artifact.denominators.shadow = None)
        }

        pub(crate) fn without_canary_denominators(self) -> Self {
            self.mutate(|artifact| artifact.denominators.canary = None)
        }

        pub(crate) fn with_canary_turns(self, turns: u64) -> Self {
            self.mutate(|artifact| {
                let Some(canary) = artifact.denominators.canary.as_mut() else {
                    panic!("canary fixture");
                };
                canary.turns = turns;
            })
        }

        pub(crate) fn with_canary_model_reduction(self, reduction: f64) -> Self {
            self.mutate(|artifact| {
                let Some(canary) = artifact.denominators.canary.as_mut() else {
                    panic!("canary fixture");
                };
                canary.model_calls = reduced(canary.baseline_model_calls, reduction);
                artifact.canary_measured.model_call_reduction =
                    measured_reduction(canary.baseline_model_calls, canary.model_calls);
            })
        }

        pub(crate) fn with_canary_tool_reduction(self, reduction: f64) -> Self {
            self.mutate(|artifact| {
                let Some(canary) = artifact.denominators.canary.as_mut() else {
                    panic!("canary fixture");
                };
                canary.model_driven_tool_calls = reduced(canary.baseline_model_driven_tool_calls, reduction);
                artifact.canary_measured.tool_call_reduction =
                    measured_reduction(canary.baseline_model_driven_tool_calls, canary.model_driven_tool_calls);
            })
        }

        pub(crate) fn with_duplicate_report_reduction(self, reduction: f64) -> Self {
            self.mutate(|artifact| {
                let Some(canary) = artifact.denominators.canary.as_mut() else {
                    panic!("canary fixture");
                };
                canary.duplicate_reports = reduced(canary.baseline_duplicate_reports, reduction);
                artifact.canary_measured.duplicate_report_reduction =
                    measured_reduction(canary.baseline_duplicate_reports, canary.duplicate_reports);
            })
        }

        pub(crate) fn with_preflight_p95(self, p95_ms: u64) -> Self {
            self.mutate(|artifact| artifact.latency.p95_ms = p95_ms)
        }

        pub(crate) fn with_registry_version(self, registry_version: &str) -> Self {
            self.mutate(|artifact| artifact.sample.registry_version = registry_version.to_string())
        }

        pub(crate) fn with_synthetic(self, synthetic: bool) -> Self {
            self.mutate(|artifact| artifact.sample.synthetic = synthetic)
        }

        pub(crate) fn with_approved_at(self, approved_at: DateTime<Utc>) -> Self {
            self.mutate(|artifact| artifact.approved_at = approved_at)
        }

        pub(crate) fn with_review(self, mutate: impl FnOnce(&mut FalseSkipReview)) -> Self {
            self.mutate(|artifact| mutate(&mut artifact.false_skip_review))
        }

        /// Deliberately tamper with the stored hash without re-sealing.
        pub(crate) fn with_content_sha256(mut self, content_sha256: &str) -> Self {
            self.artifact.content_sha256 = content_sha256.to_string();
            self
        }

        pub(crate) fn artifact(&self) -> SuppressionGateArtifact {
            self.artifact.clone()
        }
    }

    fn reduced(baseline: u64, reduction: f64) -> u64 {
        (baseline as f64 * (1.0 - reduction)).round() as u64
    }

    fn measured_reduction(baseline: u64, measured: u64) -> f64 {
        if baseline == 0 {
            0.0
        } else {
            1.0 - measured as f64 / baseline as f64
        }
    }

    pub(crate) fn shadow_fixture(workspace_id: &str) -> GateFixture {
        GateFixture::shadow(workspace_id)
    }

    pub(crate) fn canary_fixture(workspace_id: &str) -> GateFixture {
        GateFixture::canary(workspace_id)
    }

    fn context() -> GateValidationContext<'static> {
        GateValidationContext {
            workspace_id: GATE_WS,
            current_registry_version: GATE_REGISTRY,
            latest_confirmed_false_skip_at: None,
            allow_synthetic: false,
        }
    }

    fn assert_gate_not_met(result: Result<(), SuppressionError>) {
        match result {
            Ok(()) => panic!("expected shadow_gate_not_met, but validation passed"),
            Err(error) => assert_eq!(error.code, "shadow_gate_not_met"),
        }
    }

    // ── Gate order ──

    #[test]
    fn suppression_gate_shadow_estimates_alone_cannot_enable_enforce() {
        let artifact = shadow_fixture(GATE_WS).with_shadow_only().with_would_skip(0.8);
        assert_gate_not_met(validate_transition(SuppressionMode::Enforce, &artifact, &context()));
    }

    #[test]
    fn suppression_gate_canary_requires_an_approved_shadow_artifact() {
        let canary = canary_fixture(GATE_WS);
        assert_gate_not_met(validate_transition(SuppressionMode::Canary, &canary, &context()));

        let shadow = shadow_fixture(GATE_WS);
        assert!(validate_transition(SuppressionMode::Canary, &shadow, &context()).is_ok());
    }

    #[test]
    fn suppression_gate_enforce_requires_an_approved_canary_artifact() {
        let shadow = shadow_fixture(GATE_WS);
        assert_gate_not_met(validate_transition(SuppressionMode::Enforce, &shadow, &context()));

        let canary = canary_fixture(GATE_WS);
        assert!(validate_transition(SuppressionMode::Enforce, &canary, &context()).is_ok());
    }

    #[test]
    fn suppression_gate_off_and_shadow_need_no_artifact() {
        let stale = canary_fixture(GATE_WS).with_registry_version("sha256:old-registry");
        assert!(validate_transition(SuppressionMode::Off, &stale, &context()).is_ok());
        assert!(validate_transition(SuppressionMode::Shadow, &stale, &context()).is_ok());
    }

    // ── Missing evidence ──

    #[test]
    fn suppression_gate_missing_sample_denominator_or_approver_is_rejected() {
        for artifact in [
            shadow_fixture(GATE_WS).with_empty_sample_turn_ids(),
            shadow_fixture(GATE_WS).with_sample_dates(&[]),
            shadow_fixture(GATE_WS).with_approver(""),
            shadow_fixture(GATE_WS).without_shadow_denominators(),
        ] {
            assert_gate_not_met(validate_transition(SuppressionMode::Canary, &artifact, &context()));
        }

        for artifact in [
            canary_fixture(GATE_WS).with_empty_sample_turn_ids(),
            canary_fixture(GATE_WS).with_approver(""),
            canary_fixture(GATE_WS).without_canary_denominators(),
        ] {
            assert_gate_not_met(validate_transition(SuppressionMode::Enforce, &artifact, &context()));
        }
    }

    #[test]
    fn suppression_gate_shadow_threshold_boundaries_are_exact() {
        // 99 shadow Turns is not enough; 100 is.
        assert_gate_not_met(validate_transition(
            SuppressionMode::Canary,
            &shadow_fixture(GATE_WS).with_shadow_turns(99),
            &context(),
        ));
        assert!(
            validate_transition(
                SuppressionMode::Canary,
                &shadow_fixture(GATE_WS).with_shadow_turns(100),
                &context()
            )
            .is_ok()
        );

        // Two representative days are not enough; three are.
        assert_gate_not_met(validate_transition(
            SuppressionMode::Canary,
            &shadow_fixture(GATE_WS).with_sample_dates(&["2026-11-10", "2026-11-11"]),
            &context(),
        ));
        assert!(
            validate_transition(
                SuppressionMode::Canary,
                &shadow_fixture(GATE_WS).with_sample_dates(&["2026-11-10", "2026-11-11", "2026-11-12"]),
                &context()
            )
            .is_ok()
        );

        // wouldSkip must reach 50%.
        assert_gate_not_met(validate_transition(
            SuppressionMode::Canary,
            &shadow_fixture(GATE_WS).with_would_skip(0.49),
            &context(),
        ));
        assert!(
            validate_transition(
                SuppressionMode::Canary,
                &shadow_fixture(GATE_WS).with_would_skip(0.50),
                &context()
            )
            .is_ok()
        );
    }

    #[test]
    fn suppression_gate_canary_threshold_boundaries_are_exact() {
        // 19 canary Turns is not enough; 20 is.
        assert_gate_not_met(validate_transition(
            SuppressionMode::Enforce,
            &canary_fixture(GATE_WS).with_canary_turns(19),
            &context(),
        ));
        assert!(
            validate_transition(
                SuppressionMode::Enforce,
                &canary_fixture(GATE_WS).with_canary_turns(20),
                &context()
            )
            .is_ok()
        );

        // Model and tool reductions must each reach 40%.
        assert_gate_not_met(validate_transition(
            SuppressionMode::Enforce,
            &canary_fixture(GATE_WS).with_canary_model_reduction(0.399),
            &context(),
        ));
        assert_gate_not_met(validate_transition(
            SuppressionMode::Enforce,
            &canary_fixture(GATE_WS).with_canary_tool_reduction(0.399),
            &context(),
        ));
        assert!(
            validate_transition(
                SuppressionMode::Enforce,
                &canary_fixture(GATE_WS).with_canary_model_reduction(0.4),
                &context()
            )
            .is_ok()
        );

        // Duplicate-report reduction must reach 80%.
        assert_gate_not_met(validate_transition(
            SuppressionMode::Enforce,
            &canary_fixture(GATE_WS).with_duplicate_report_reduction(0.799),
            &context(),
        ));
        assert!(
            validate_transition(
                SuppressionMode::Enforce,
                &canary_fixture(GATE_WS).with_duplicate_report_reduction(0.8),
                &context()
            )
            .is_ok()
        );

        // Preflight p95 must stay at or below 250ms.
        assert_gate_not_met(validate_transition(
            SuppressionMode::Enforce,
            &canary_fixture(GATE_WS).with_preflight_p95(251),
            &context(),
        ));
        assert!(
            validate_transition(
                SuppressionMode::Enforce,
                &canary_fixture(GATE_WS).with_preflight_p95(250),
                &context()
            )
            .is_ok()
        );
    }

    #[test]
    fn suppression_gate_unreviewed_or_confirmed_false_skip_samples_are_rejected() {
        let unreviewed = canary_fixture(GATE_WS).with_review(|review| {
            review.samples = vec![FalseSkipSample {
                turn_id: "canary-turn-000".to_string(),
                reviewed: false,
                confirmed_false_skip: false,
            }];
        });
        assert_gate_not_met(validate_transition(SuppressionMode::Enforce, &unreviewed, &context()));

        let confirmed_sample = canary_fixture(GATE_WS).with_review(|review| {
            review.samples = vec![FalseSkipSample {
                turn_id: "canary-turn-000".to_string(),
                reviewed: true,
                confirmed_false_skip: true,
            }];
        });
        assert_gate_not_met(validate_transition(
            SuppressionMode::Enforce,
            &confirmed_sample,
            &context(),
        ));

        let counted = canary_fixture(GATE_WS).with_review(|review| review.confirmed_false_skips = 1);
        assert_gate_not_met(validate_transition(SuppressionMode::Enforce, &counted, &context()));

        let missing_reviewer = canary_fixture(GATE_WS).with_review(|review| review.reviewer.clear());
        assert_gate_not_met(validate_transition(
            SuppressionMode::Enforce,
            &missing_reviewer,
            &context(),
        ));

        let wrong_conclusion =
            canary_fixture(GATE_WS).with_review(|review| review.conclusion = "confirmed_false_skip".to_string());
        assert_gate_not_met(validate_transition(
            SuppressionMode::Enforce,
            &wrong_conclusion,
            &context(),
        ));

        let no_samples = canary_fixture(GATE_WS).with_review(|review| review.samples.clear());
        assert_gate_not_met(validate_transition(SuppressionMode::Enforce, &no_samples, &context()));
    }

    // ── Staleness (ruling R18) ──

    #[test]
    fn suppression_gate_stale_registry_artifact_is_rejected() {
        let artifact = shadow_fixture(GATE_WS).with_registry_version("sha256:previous-registry");
        assert_gate_not_met(validate_transition(SuppressionMode::Canary, &artifact, &context()));
    }

    #[test]
    fn suppression_gate_confirmed_false_skip_after_approval_is_stale() {
        let artifact = canary_fixture(GATE_WS);
        let approved_at = artifact.approved_at;
        let after = GateValidationContext {
            latest_confirmed_false_skip_at: Some(approved_at + Duration::seconds(1)),
            ..context()
        };
        assert_gate_not_met(validate_transition(SuppressionMode::Enforce, &artifact, &after));

        let before = GateValidationContext {
            latest_confirmed_false_skip_at: Some(approved_at - Duration::seconds(1)),
            ..context()
        };
        assert!(
            validate_transition(SuppressionMode::Enforce, &artifact, &before).is_ok(),
            "a fixed root cause before approval leaves a fresh artifact valid"
        );
    }

    #[test]
    fn suppression_gate_has_no_invented_age_window() {
        let ancient = canary_fixture(GATE_WS).with_approved_at(ts(1_000_000_000));
        assert!(
            validate_transition(SuppressionMode::Enforce, &ancient, &context()).is_ok(),
            "R18: age alone never invalidates an artifact"
        );
    }

    #[test]
    fn suppression_gate_rejects_a_tampered_artifact_hash() {
        let artifact = canary_fixture(GATE_WS).with_content_sha256("sha256:not-the-real-hash");
        assert_gate_not_met(validate_transition(SuppressionMode::Enforce, &artifact, &context()));

        let mut mutated = canary_fixture(GATE_WS).artifact();
        mutated.approver = "someone-else".to_string();
        assert_gate_not_met(validate_transition(SuppressionMode::Enforce, &mutated, &context()));
    }

    #[test]
    fn suppression_gate_synthetic_fixture_artifact_is_loopback_only() {
        let synthetic = shadow_fixture(GATE_WS).with_synthetic(true);
        assert_gate_not_met(validate_transition(SuppressionMode::Canary, &synthetic, &context()));

        let loopback = GateValidationContext {
            allow_synthetic: true,
            ..context()
        };
        assert!(
            validate_transition(SuppressionMode::Canary, &synthetic, &loopback).is_ok(),
            "the loopback fixture may use synthetic evidence"
        );
    }

    // ── Metrics ──

    fn shadow_observation(
        turn_id: &str,
        would_skip: bool,
        vetoes: &[&str],
        model_calls: u64,
        tool_calls: u64,
        latency_ms: u64,
        unavailable: bool,
        reconciliation: bool,
    ) -> ShadowTurnObservation {
        ShadowTurnObservation {
            turn_id: turn_id.to_string(),
            date: "2026-11-10".to_string(),
            would_skip,
            vetoes: vetoes.iter().map(|veto| veto.to_string()).collect(),
            model_calls,
            model_driven_tool_calls: tool_calls,
            unavailable_source_full_execution: unavailable,
            reconciliation_full_execution: reconciliation,
            preflight_latency_ms: latency_ms,
        }
    }

    #[test]
    fn suppression_gate_shadow_metrics_use_explicit_denominators_and_estimates() {
        let observations = vec![
            shadow_observation("t1", true, &[], 5, 2, 30, false, false),
            shadow_observation("t2", false, &["source_changed"], 7, 3, 50, false, false),
            shadow_observation("t3", true, &[], 3, 1, 20, false, false),
            shadow_observation(
                "t4",
                false,
                &["source_unavailable", "reconciliation_due"],
                5,
                4,
                100,
                true,
                true,
            ),
        ];
        let before = observations.clone();
        let (denominators, estimates, latency) = compute_shadow_metrics(&observations);

        assert_eq!(observations, before, "shadow metrics are pure estimates");
        assert!(estimates.estimated, "shadow numbers must be labelled estimated");
        assert_eq!(denominators.turns, 4);
        assert_eq!(denominators.would_skip_turns, 2);
        assert_eq!(denominators.model_calls, 20);
        assert_eq!(denominators.model_calls_on_would_skip_turns, 8);
        assert_eq!(denominators.model_driven_tool_calls, 10);
        assert_eq!(denominators.tool_calls_on_would_skip_turns, 3);
        assert_eq!(denominators.unavailable_source_full_executions, 1);
        assert_eq!(denominators.reconciliation_full_executions, 1);
        assert_eq!(estimates.would_skip_ratio, 0.5);
        assert_eq!(estimates.estimated_model_savings, 0.4);
        assert_eq!(estimates.estimated_tool_savings, 0.3);
        assert_eq!(estimates.longest_consecutive_skips, 1);
        assert_eq!(estimates.veto_distribution.get("source_changed"), Some(&1));
        assert_eq!(estimates.veto_distribution.get("source_unavailable"), Some(&1));
        assert_eq!(estimates.veto_distribution.get("reconciliation_due"), Some(&1));
        assert_eq!(latency.sample_count, 4);
        assert_eq!(latency.p50_ms, 30);
        assert_eq!(latency.p95_ms, 100);
        assert_eq!(latency.p99_ms, 100);

        // No field claims shadow has already saved calls.
        let value = serde_json::to_value(&estimates).expect("estimates json");
        assert_eq!(value["estimated"], true);
        assert!(value.get("savedModelCalls").is_none());
        assert!(value.get("actualModelCallsSaved").is_none());
        assert_eq!(value["estimatedModelSavings"], 0.4);
        assert_eq!(value["estimatedToolSavings"], 0.3);
    }

    #[test]
    fn suppression_gate_shadow_metrics_do_not_change_execution_behavior() {
        let observations = vec![
            shadow_observation("t1", true, &[], 5, 2, 30, false, false),
            shadow_observation("t2", true, &[], 4, 1, 40, false, false),
        ];
        let model_calls_before: u64 = observations.iter().map(|turn| turn.model_calls).sum();
        let tool_calls_before: u64 = observations.iter().map(|turn| turn.model_driven_tool_calls).sum();
        let (_denominators, _estimates, _latency) = compute_shadow_metrics(&observations);

        assert_eq!(
            observations.iter().map(|turn| turn.model_calls).sum::<u64>(),
            model_calls_before,
            "shadow mode must not remove model calls"
        );
        assert_eq!(
            observations
                .iter()
                .map(|turn| turn.model_driven_tool_calls)
                .sum::<u64>(),
            tool_calls_before,
            "shadow mode must not remove tool calls"
        );
        assert!(
            !SuppressionMode::Shadow.allows_skip(),
            "shadow only observes; it never executes a skip"
        );
    }

    #[test]
    fn suppression_gate_consecutive_would_skip_streak_is_the_longest_run() {
        let observations = vec![
            shadow_observation("t1", true, &[], 1, 1, 10, false, false),
            shadow_observation("t2", true, &[], 1, 1, 10, false, false),
            shadow_observation("t3", true, &[], 1, 1, 10, false, false),
            shadow_observation("t4", false, &[], 1, 1, 10, false, false),
            shadow_observation("t5", true, &[], 1, 1, 10, false, false),
        ];
        let (_denominators, estimates, _latency) = compute_shadow_metrics(&observations);
        assert_eq!(estimates.longest_consecutive_skips, 3);
        assert_eq!(estimates.would_skip_ratio, 0.8);
    }

    #[test]
    fn suppression_gate_canary_metrics_are_measured_from_denominators() {
        let baseline = CanaryBaseline {
            model_calls: 30,
            model_driven_tool_calls: 20,
            duplicate_reports: 10,
        };
        let observations = vec![
            canary_observation("c1", 4, 2, 1, 100),
            canary_observation("c2", 3, 2, 0, 200),
            canary_observation("c3", 5, 3, 1, 300),
            canary_observation("c4", 6, 3, 0, 400),
        ];
        let (denominators, measured, latency) = compute_canary_metrics(&observations, &baseline);

        assert!(measured.measured, "canary numbers are measured, not estimated");
        assert_eq!(denominators.turns, 4);
        assert_eq!(denominators.baseline_model_calls, 30);
        assert_eq!(denominators.model_calls, 18);
        assert_eq!(denominators.baseline_model_driven_tool_calls, 20);
        assert_eq!(denominators.model_driven_tool_calls, 10);
        assert_eq!(denominators.baseline_duplicate_reports, 10);
        assert_eq!(denominators.duplicate_reports, 2);
        assert_eq!(measured.model_call_reduction, 0.4);
        assert_eq!(measured.tool_call_reduction, 0.5);
        assert_eq!(measured.duplicate_report_reduction, 0.8);
        assert_eq!(latency.sample_count, 4);
        assert_eq!(latency.p95_ms, 400);
    }

    fn canary_observation(
        turn_id: &str,
        model_calls: u64,
        tool_calls: u64,
        duplicate_reports: u64,
        latency_ms: u64,
    ) -> CanaryTurnObservation {
        CanaryTurnObservation {
            turn_id: turn_id.to_string(),
            date: "2026-11-13".to_string(),
            model_calls,
            model_driven_tool_calls: tool_calls,
            duplicate_reports,
            preflight_latency_ms: latency_ms,
        }
    }

    // ── Immutable persistence ──

    #[tokio::test]
    async fn suppression_gate_artifacts_are_immutable_append_only_records() {
        let fixture = tinyiothub_storage::test_helpers::fixture_workspace("ws-gate-db").await;
        let artifact = shadow_fixture("ws-gate-db").artifact();
        fixture
            .db
            .insert_suppression_gate_artifact(&artifact)
            .await
            .expect("append the first artifact");

        let loaded = fixture
            .db
            .load_suppression_gate_artifact("ws-gate-db", &artifact.artifact_id)
            .await
            .expect("load artifact")
            .expect("artifact row");
        assert_eq!(loaded, artifact, "artifact round-trips without mutation");
        assert_eq!(
            gate_artifact_sha256(&loaded),
            loaded.content_sha256,
            "stored SHA-256 still covers the canonical artifact bytes"
        );

        let latest = fixture
            .db
            .latest_suppression_gate_artifact("ws-gate-db", SuppressionGateMode::Shadow)
            .await
            .expect("latest artifact")
            .expect("shadow artifact");
        assert_eq!(latest.artifact_id, artifact.artifact_id);

        let duplicate = fixture
            .db
            .insert_suppression_gate_artifact(&artifact)
            .await
            .expect_err("a repeated artifactId is rejected");
        assert!(matches!(duplicate, DbError::Conflict { .. }));

        let canary = canary_fixture("ws-gate-db").artifact();
        fixture
            .db
            .insert_suppression_gate_artifact(&canary)
            .await
            .expect("append the canary artifact");
        let workspace_pool = fixture
            .db
            .workspace_pool_for_testing("ws-gate-db")
            .await
            .expect("workspace pool");
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM suppression_gate_evaluations")
            .fetch_one(&workspace_pool)
            .await
            .expect("count artifacts");
        assert_eq!(count, 2, "artifacts are only ever appended");

        fixture.close().await;
    }
}
