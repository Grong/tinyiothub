//! Deterministic preflight decisions.
//!
//! Preflight is read-only: it captures the current Sources at `observedAt` and
//! compares them with the durable baseline/state. It never advances the
//! baseline, cursor, config or fences. The observed snapshot is the
//! external-Source boundary; changes arriving after it belong to the next
//! Turn.

#![allow(clippy::result_large_err)]

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use tinyiothub_storage::assistant_schedule::SuppressionConfig;
use tinyiothub_storage::suppression::{
    DEFAULT_RECONCILIATION_SECONDS, MAX_RECONCILIATION_SECONDS, MIN_RECONCILIATION_SECONDS, OUTCOME_AUDIT_FAILED,
    OUTCOME_BASELINE_MISSING, OUTCOME_FULL_COMPLETED, OUTCOME_FULL_FAILED, OUTCOME_FULL_IN_PROGRESS,
    OUTCOME_SKIP_IN_PROGRESS, OUTCOME_SKIPPED, SuppressionMode, SuppressionState,
};

use super::contract::{SourceSnapshot, TURN_STATE_VERSION};
use super::error::SuppressionError;
use super::registry::{SourceContext, SourceMode, SourceRegistry};

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
}

impl SuppressionService {
    pub fn new(registry: Arc<SourceRegistry>) -> Self {
        Self { registry }
    }

    pub fn registry(&self) -> &SourceRegistry {
        self.registry.as_ref()
    }

    pub async fn evaluate(
        &self,
        mut context: SourceContext,
        state: Option<&SuppressionState>,
        config: &SuppressionConfig,
        current_user_seq: i64,
    ) -> Result<PreflightDecision, SuppressionError> {
        context.mode = config.mode.into();
        // Off observes nothing: the kill switch must stay cheap and immediate.
        let snapshots = if config.mode == SuppressionMode::Off {
            Vec::new()
        } else {
            self.registry.capture_all(&context).await?
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
