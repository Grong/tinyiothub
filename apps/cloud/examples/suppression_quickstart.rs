//! Loopback-only Assistant suppression quickstart.
//!
//! Starts a synthetic fixture on `127.0.0.1:3109` with an in-memory platform
//! SQLite database, a temporary per-workspace Harness store, two example
//! Sources (`demo.static`, `demo.changing`) and the production suppression HTTP
//! handlers. There is no real provider, device, credential or browser cookie:
//! the synthetic `test-token` is mapped to a local administrator claim.
//!
//! The fixture deliberately drives the real suppression control plane:
//! `SuppressionService::evaluate` computes every decision, full Turns allocate
//! and complete real `suppression_state` attempts, skips commit through
//! `AtomicSkipAudit`, and every full Turn resolves each captured
//! `stateRef`/`payloadRef` through the production Source resolvers before
//! asserting that the assembled request contains every registered Source's
//! canonical content. The only synthetic boundary is model/tool execution:
//! no model or tool is called, so a completed Turn records fixed counts of
//! 2 model calls / 1 tool call.
//!
//! Synthetic gate evidence is sealed at startup from synthetic observations and
//! is accepted only because the server configuration sets
//! `suppression_gate_allow_synthetic`. No client request can set that flag.
//!
//! Run:
//!
//! ```text
//! cargo run -p tinyiothub-cloud --example suppression_quickstart
//! ```
//!
//! Then follow `docs/dev/assistant-suppression.md` from another shell.

use std::collections::BTreeSet;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::extract::{Path, Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Extension, Json, Router};
use chrono::{Duration as ChronoDuration, Utc};
use serde::Deserialize;
use serde_json::{Value, json};

use tinyiothub_cloud::domains::agent::session::suppression::audit::{AtomicSkipAudit, PlannedWake, SkipAuditRequest};
use tinyiothub_cloud::domains::agent::session::suppression::canonical::canonical_bytes;
use tinyiothub_cloud::domains::agent::session::suppression::contract::{
    CanonicalBytes, PayloadRef, SourceDescriptor, SourceSnapshot, StateRef, WorkItem, WorkItemDisposition,
};
use tinyiothub_cloud::domains::agent::session::suppression::error::{ErrorCode, SuppressionError};
use tinyiothub_cloud::domains::agent::session::suppression::preflight::{
    PreflightDecision, SkipVeto, SuppressionService, derive_canary_gate_artifact, derive_shadow_gate_artifact,
    seal_gate_artifact,
};
use tinyiothub_cloud::domains::agent::session::suppression::registry::{
    AssistantSource, SOURCE_VERSION, SourceContext, SourceRegistry,
};
use tinyiothub_cloud::domains::agent::session::trigger::SuppressionOps;
use tinyiothub_cloud::domains::agent::session::wake::{
    ManualOperation, ManualPreflight, ManualStartError, ManualTurnStart,
};
use tinyiothub_cloud::domains::agent::session::workspace_api::create_workspace_router;
use tinyiothub_cloud::state::AppState;
use tinyiothub_core::harness::{ExecutionMode, HarnessEventInput};
use tinyiothub_storage::assistant_schedule::SuppressionConfig;
use tinyiothub_storage::cache::ThingCache;
use tinyiothub_storage::suppression::{
    DuplicateReportLabels, FALSE_SKIP_CONCLUSION_NONE, FalseSkipReview, FalseSkipSample, FullAttempt,
    FullAttemptCompletion, SuppressionMode, SuppressionState, SuppressionTurnObservation, SyncPending,
    TURN_STATE_VERSION,
};
use tinyiothub_storage::{Db, migrations};
use tinyiothub_web::security::Claims;

const WORKSPACE_ID: &str = "ws-suppression-demo";
const TENANT_ID: &str = "tenant-suppression-demo";
const USER_ID: &str = "fixture-user";
const FIXTURE_TOKEN: &str = "test-token";
const STATIC_SOURCE: &str = "demo.static";
const CHANGING_SOURCE: &str = "demo.changing";
const STATIC_REVISION: &str = "static-r1";
const CHANGING_ITEM: &str = "item-1";

fn demo_session_key(workspace_id: &str) -> String {
    format!("agent:{workspace_id}:__assistant__{workspace_id}/main")
}

// ── StaticSource: deterministic, unchanged content ─────────────────────────

/// A Source whose complete state never changes. Repeated `capture` calls return
/// the same revision and hash, so a stored baseline remains skip-eligible.
pub struct StaticSource {
    descriptor: SourceDescriptor,
}

impl StaticSource {
    pub fn new() -> Self {
        Self {
            descriptor: SourceDescriptor {
                source_id: STATIC_SOURCE.to_string(),
                source_version: SOURCE_VERSION,
            },
        }
    }

    fn state_value() -> Value {
        json!({"static": {"value": 42}})
    }

    fn payload_value() -> Value {
        json!({"item": "static-item-1", "text": "unchanged"})
    }
}

impl Default for StaticSource {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AssistantSource for StaticSource {
    fn descriptor(&self) -> SourceDescriptor {
        self.descriptor.clone()
    }

    async fn capture(&self, _cx: &SourceContext) -> Result<SourceSnapshot, SuppressionError> {
        let state = CanonicalBytes::state(canonical_bytes(&Self::state_value())?, STATIC_REVISION)?;
        let payload = CanonicalBytes::payload(canonical_bytes(&Self::payload_value())?, "static-item-1-r1")?;
        Ok(SourceSnapshot {
            source_id: self.descriptor.source_id.clone(),
            source_version: self.descriptor.source_version,
            revision: state.revision().to_string(),
            state_hash: state.hash().to_string(),
            available: true,
            empty: false,
            state_ref: Some(StateRef {
                id: format!("state:{STATIC_SOURCE}"),
                revision: state.revision().to_string(),
                state_hash: state.hash().to_string(),
            }),
            work_items: vec![WorkItem {
                work_key: "static-item-1".to_string(),
                revision: payload.revision().to_string(),
                disposition: WorkItemDisposition::Active,
                payload_ref: PayloadRef {
                    id: format!("payload:{STATIC_SOURCE}:static-item-1"),
                    revision: payload.revision().to_string(),
                    payload_hash: payload.hash().to_string(),
                },
                payload_hash: payload.hash().to_string(),
            }],
            error: None,
        })
    }

    async fn resolve_state(&self, reference: &StateRef) -> Result<CanonicalBytes, SuppressionError> {
        let canonical = CanonicalBytes::state(canonical_bytes(&Self::state_value())?, STATIC_REVISION)?;
        if reference.id != format!("state:{STATIC_SOURCE}")
            || reference.revision != canonical.revision()
            || reference.state_hash != canonical.hash()
        {
            return Err(SuppressionError::from_code(ErrorCode::SourceRefMismatch));
        }
        Ok(canonical)
    }

    async fn resolve_payload(&self, reference: &PayloadRef) -> Result<CanonicalBytes, SuppressionError> {
        let canonical = CanonicalBytes::payload(canonical_bytes(&Self::payload_value())?, "static-item-1-r1")?;
        if reference.id != format!("payload:{STATIC_SOURCE}:static-item-1")
            || reference.revision != canonical.revision()
            || reference.payload_hash != canonical.hash()
        {
            return Err(SuppressionError::from_code(ErrorCode::SourceRefMismatch));
        }
        Ok(canonical)
    }
}

// ── ChangingSource: mutate one work item, which changes only its revision ──

#[derive(Clone, Debug)]
struct ChangingItem {
    revision: String,
    text: String,
}

/// A Source with one mutable work item. The fixture route changes the revision
/// and payload, which must veto a skip while keeping the full request complete.
pub struct ChangingSource {
    item: Mutex<ChangingItem>,
}

impl ChangingSource {
    pub fn new() -> Self {
        Self {
            item: Mutex::new(ChangingItem {
                revision: "1".to_string(),
                text: "unresolved".to_string(),
            }),
        }
    }

    fn set(&self, revision: impl Into<String>, text: impl Into<String>) {
        let mut item = self.item.lock().expect("changing source lock poisoned");
        item.revision = revision.into();
        item.text = text.into();
    }

    fn item(&self) -> ChangingItem {
        self.item.lock().expect("changing source lock poisoned").clone()
    }

    fn state_value(item: &ChangingItem) -> Value {
        json!({"revision": item.revision, "text": item.text})
    }

    fn payload_value(item: &ChangingItem) -> Value {
        json!({"item": CHANGING_ITEM, "revision": item.revision, "text": item.text})
    }

    fn revision(item: &ChangingItem) -> String {
        format!("changing-{}", item.revision)
    }
}

impl Default for ChangingSource {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AssistantSource for ChangingSource {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            source_id: CHANGING_SOURCE.to_string(),
            source_version: SOURCE_VERSION,
        }
    }

    async fn capture(&self, _cx: &SourceContext) -> Result<SourceSnapshot, SuppressionError> {
        let item = self.item();
        let revision = Self::revision(&item);
        let state = CanonicalBytes::state(canonical_bytes(&Self::state_value(&item))?, revision.clone())?;
        let payload = CanonicalBytes::payload(canonical_bytes(&Self::payload_value(&item))?, revision.clone())?;
        Ok(SourceSnapshot {
            source_id: CHANGING_SOURCE.to_string(),
            source_version: SOURCE_VERSION,
            revision: state.revision().to_string(),
            state_hash: state.hash().to_string(),
            available: true,
            empty: false,
            state_ref: Some(StateRef {
                id: format!("state:{CHANGING_SOURCE}"),
                revision: state.revision().to_string(),
                state_hash: state.hash().to_string(),
            }),
            work_items: vec![WorkItem {
                work_key: "changing-item-1".to_string(),
                revision: payload.revision().to_string(),
                disposition: WorkItemDisposition::Active,
                payload_ref: PayloadRef {
                    id: format!("payload:{CHANGING_SOURCE}:changing-item-1"),
                    revision: payload.revision().to_string(),
                    payload_hash: payload.hash().to_string(),
                },
                payload_hash: payload.hash().to_string(),
            }],
            error: None,
        })
    }

    async fn resolve_state(&self, reference: &StateRef) -> Result<CanonicalBytes, SuppressionError> {
        let item = self.item();
        let canonical = CanonicalBytes::state(canonical_bytes(&Self::state_value(&item))?, Self::revision(&item))?;
        if reference.id != format!("state:{CHANGING_SOURCE}")
            || reference.revision != canonical.revision()
            || reference.state_hash != canonical.hash()
        {
            return Err(SuppressionError::from_code(ErrorCode::SourceRefMismatch));
        }
        Ok(canonical)
    }

    async fn resolve_payload(&self, reference: &PayloadRef) -> Result<CanonicalBytes, SuppressionError> {
        let item = self.item();
        let canonical = CanonicalBytes::payload(canonical_bytes(&Self::payload_value(&item))?, Self::revision(&item))?;
        if reference.id != format!("payload:{CHANGING_SOURCE}:changing-item-1")
            || reference.revision != canonical.revision()
            || reference.payload_hash != canonical.hash()
        {
            return Err(SuppressionError::from_code(ErrorCode::SourceRefMismatch));
        }
        Ok(canonical)
    }
}

// ── Fixture Turn execution ─────────────────────────────────────────────────

/// Production-decision fixture operations.
///
/// Decisions use [`SuppressionService::evaluate`], full Turns allocate and
/// complete real `suppression_state` attempts, and skips commit through
/// [`AtomicSkipAudit`]. Only model/tool execution is synthetic: no model or
/// tool is called, so a completed Turn records fixed counts of 2 model calls
/// and 1 tool call.
struct FixtureOps {
    db: Arc<Db>,
    registry: Arc<SourceRegistry>,
    service: SuppressionService,
    sources: Vec<Arc<dyn AssistantSource>>,
}

impl FixtureOps {
    fn new(
        db: Arc<Db>,
        registry: Arc<SourceRegistry>,
        service: SuppressionService,
        sources: Vec<Arc<dyn AssistantSource>>,
    ) -> Self {
        Self {
            db,
            registry,
            service,
            sources,
        }
    }

    async fn config(&self, workspace_id: &str) -> Result<SuppressionConfig, ManualStartError> {
        self.db
            .get_suppression_config(workspace_id)
            .await
            .map_err(|error| ManualStartError::Unavailable(error.to_string()))
            .map(|config| config.unwrap_or_else(|| SuppressionConfig::off(workspace_id.to_string())))
    }

    async fn state(&self, workspace_id: &str, session_key: &str) -> Result<SuppressionState, ManualStartError> {
        self.db
            .load_suppression_state(workspace_id, session_key)
            .await
            .map_err(|error| ManualStartError::Unavailable(error.to_string()))
            .map(|state| state.unwrap_or_else(|| SuppressionState::initial(workspace_id, session_key)))
    }

    async fn decision(
        &self,
        workspace_id: &str,
    ) -> Result<(SuppressionConfig, SuppressionState, PreflightDecision), ManualStartError> {
        let config = self.config(workspace_id).await?;
        let session_key = demo_session_key(workspace_id);
        let state = self.state(workspace_id, &session_key).await?;
        let decision = self
            .service
            .evaluate(SourceContext::new(workspace_id, &session_key), Some(&state), &config, 0)
            .await
            .map_err(|error| ManualStartError::Unavailable(format!("{error:?}")))?;
        Ok((config, state, decision))
    }
}

#[async_trait]
impl SuppressionOps for FixtureOps {
    async fn manual_preflight(&self, workspace_id: &str) -> Result<ManualPreflight, ManualStartError> {
        let (config, _state, decision) = self.decision(workspace_id).await?;
        Ok(ManualPreflight {
            session_key: demo_session_key(workspace_id),
            mode: config.mode,
            reconciliation_seconds: config.reconciliation_seconds,
            config_version: config.config_version,
            registry_version: self.registry.registry_version(),
            descriptors: self.registry.descriptors(),
            decision,
        })
    }

    async fn start_manual(
        &self,
        workspace_id: &str,
        operation: ManualOperation,
    ) -> Result<ManualTurnStart, ManualStartError> {
        let (config, state, decision) = self.decision(workspace_id).await?;
        let session_key = demo_session_key(workspace_id);
        let wake = PlannedWake::unplanned(workspace_id, &session_key);
        let registry_version = self.registry.registry_version();
        let vetoes = decision
            .vetoes
            .iter()
            .map(|veto| veto.as_str().to_string())
            .collect::<Vec<_>>();
        let skip = operation == ManualOperation::Wake && config.mode.allows_skip() && decision.would_skip;

        if skip {
            AtomicSkipAudit::ensure_skip_allowed(&decision)
                .map_err(|error| ManualStartError::Unavailable(format!("{error:?}")))?;
            let attempt = self
                .db
                .begin_skip_attempt(&session_key, &state)
                .await
                .map_err(|error| ManualStartError::Unavailable(error.to_string()))?;
            let turn_id = wake.attempt(attempt.attempt_no).turn_id.clone();
            let attempt_id = attempt.attempt_id.clone();
            let db = self.db.clone();
            let audit = AtomicSkipAudit::new(self.db.clone());
            let workspace_id = workspace_id.to_string();
            let task_session_key = session_key.clone();
            let task_attempt_id = attempt_id.clone();
            let turn_for_log = turn_id.clone();
            tokio::spawn(async move {
                let request = SkipAuditRequest {
                    wake: &wake,
                    attempt: &attempt,
                    decision: &decision,
                    checked_at: Utc::now(),
                };
                match audit.commit(request).await {
                    Ok(committed) => {
                        // The committed skip owns the durable sync barriers;
                        // reconcile each consumer before the next skip.
                        for pending in [
                            SyncPending::Schedule,
                            SyncPending::Projection,
                            SyncPending::Notification,
                        ] {
                            if let Err(error) = db.clear_sync_pending(&workspace_id, &task_session_key, pending).await {
                                eprintln!(
                                    "[fixture] {} barrier {} reconciliation failed: {error}",
                                    committed.attempt_id,
                                    pending_label(pending)
                                );
                            }
                        }
                        let observation = SuppressionTurnObservation {
                            workspace_id: workspace_id.clone(),
                            session_id: task_session_key.clone(),
                            turn_id: turn_for_log.clone(),
                            attempt_id: committed.attempt_id.clone(),
                            mode: config.mode,
                            observed_at: Utc::now(),
                            would_skip: true,
                            vetoes,
                            model_calls: 0,
                            tool_calls: 0,
                            preflight_latency_ms: 1,
                            registry_version: registry_version.clone(),
                        };
                        if let Err(error) = db.record_suppression_turn_observation(&observation).await {
                            eprintln!("[fixture] {} observation write failed: {error}", committed.attempt_id);
                        }
                        eprintln!("[fixture] {} skipped modelCalls=0 toolCalls=0", turn_for_log);
                    }
                    Err(error) => {
                        let _ = db
                            .fail_skip_attempt(
                                &workspace_id,
                                &task_session_key,
                                &task_attempt_id,
                                "fixture_skip_commit_failed",
                            )
                            .await;
                        let payload = json!({
                            "turnId": turn_for_log.clone(),
                            "outcome": "failed",
                            "error": "fixture_skip_commit_failed",
                        });
                        let _ = append_event(
                            &db,
                            &workspace_id,
                            &task_session_key,
                            &format!("{turn_for_log}-failed"),
                            "turn.completed",
                            payload,
                        )
                        .await;
                        eprintln!("[fixture] {turn_for_log} skipped-path failed: {error}");
                    }
                }
            });
            return Ok(ManualTurnStart {
                session_key,
                turn_id,
                attempt_id,
            });
        }

        // `off` has no pre-Turn Source capture from the decision, but the full
        // request still assembles every registered Source.
        let snapshots = if decision.snapshot.is_empty() {
            self.registry
                .capture_all(&SourceContext::new(workspace_id, &session_key))
                .await
                .map_err(|error| ManualStartError::Unavailable(format!("{error:?}")))?
        } else {
            decision.snapshot.clone()
        };
        let full = self
            .db
            .begin_full_attempt(&session_key, &state)
            .await
            .map_err(|error| ManualStartError::Unavailable(error.to_string()))?;
        let turn_id = wake.attempt(full.attempt_no).turn_id.clone();
        let attempt_id = full.attempt_id.clone();
        let turn = ExecuteFullTurn {
            db: self.db.clone(),
            sources: self.sources.clone(),
            workspace_id: workspace_id.to_string(),
            session_key: session_key.clone(),
            turn_id: turn_id.clone(),
            full,
            mode: config.mode,
            config_version: config.config_version,
            registry_version,
            would_skip: decision.would_skip,
            vetoes,
            snapshots,
        };
        let log_turn_id = turn_id.clone();
        let fail_db = self.db.clone();
        let fail_workspace = workspace_id.to_string();
        let fail_session = session_key.clone();
        let fail_attempt = attempt_id.clone();
        tokio::spawn(async move {
            if let Err(error) = execute_full_turn(turn).await {
                eprintln!("[fixture] {log_turn_id} failed: {error}");
                // Only settle/surface a failed terminal while the full attempt
                // is still in progress. If completion already committed, the
                // model Turn result stays authoritative.
                if fail_db
                    .fail_full_attempt(&fail_workspace, &fail_attempt, "fixture_full_turn_failed")
                    .await
                    .is_ok()
                {
                    let payload = json!({
                        "turnId": log_turn_id.clone(),
                        "outcome": "failed",
                        "error": "fixture_full_turn_failed",
                    });
                    let _ = append_event(
                        &fail_db,
                        &fail_workspace,
                        &fail_session,
                        &format!("{log_turn_id}-failed"),
                        "turn.completed",
                        payload,
                    )
                    .await;
                }
            }
        });
        Ok(ManualTurnStart {
            session_key,
            turn_id,
            attempt_id,
        })
    }
}

fn pending_label(pending: SyncPending) -> &'static str {
    match pending {
        SyncPending::Schedule => "schedule",
        SyncPending::Projection => "projection",
        SyncPending::Notification => "notification",
    }
}

struct ExecuteFullTurn {
    db: Arc<Db>,
    sources: Vec<Arc<dyn AssistantSource>>,
    workspace_id: String,
    session_key: String,
    turn_id: String,
    full: FullAttempt,
    mode: SuppressionMode,
    config_version: i64,
    registry_version: String,
    would_skip: bool,
    vetoes: Vec<String>,
    snapshots: Vec<SourceSnapshot>,
}

async fn execute_full_turn(turn: ExecuteFullTurn) -> Result<(), String> {
    // A tiny delay keeps the HTTP 202 and the asynchronous terminal distinct,
    // exactly like a real wake.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    // Resolve every captured immutable ref through the production resolvers
    // and assemble the complete request. This fails if any registered Source
    // is missing, unavailable, or resolves to content that disagrees with its
    // snapshot refs.
    let request = assemble_full_input(&turn.sources, &turn.snapshots, &turn.turn_id).await?;
    let source_count = request["sources"]
        .as_array()
        .map(Vec::len)
        .ok_or_else(|| "assembled request has no sources array".to_string())?;

    append_event(
        &turn.db,
        &turn.workspace_id,
        &turn.session_key,
        &format!("{}-started", turn.turn_id),
        "turn.started",
        json!({"turnId": turn.turn_id}),
    )
    .await?;

    let observed_at = Utc::now();
    if turn.mode == SuppressionMode::Off {
        // Production manual operations in off mode keep the old baseline
        // ineligible and install no new one.
        turn.db
            .settle_full_attempt_without_baseline(&turn.workspace_id, &turn.full.attempt_id, "full_completed")
            .await
            .map_err(|error| error.to_string())?;
    } else {
        let completion = FullAttemptCompletion {
            state_id: format!("state:{}", turn.full.attempt_id),
            state_version: TURN_STATE_VERSION,
            registry_version: turn.registry_version.clone(),
            config_version: turn.config_version,
            control_cursor: 0,
            turn_state: baseline_turn_state(
                &turn.full.attempt_id,
                &turn.turn_id,
                observed_at,
                &turn.snapshots,
                &turn.registry_version,
                turn.mode,
            ),
            observed_at,
        };
        turn.db
            .complete_full_attempt(&turn.workspace_id, &turn.full.attempt_id, &completion)
            .await
            .map_err(|error| error.to_string())?;

        let observation = SuppressionTurnObservation {
            workspace_id: turn.workspace_id.clone(),
            session_id: turn.session_key.clone(),
            turn_id: turn.turn_id.clone(),
            attempt_id: turn.full.attempt_id.clone(),
            mode: turn.mode,
            observed_at,
            would_skip: turn.would_skip,
            vetoes: turn.vetoes.clone(),
            model_calls: 2,
            tool_calls: 1,
            preflight_latency_ms: 1,
            registry_version: turn.registry_version.clone(),
        };
        turn.db
            .record_suppression_turn_observation(&observation)
            .await
            .map_err(|error| error.to_string())?;
    }

    append_event(
        &turn.db,
        &turn.workspace_id,
        &turn.session_key,
        &format!("{}-completed", turn.turn_id),
        "turn.completed",
        json!({
            "turnId": turn.turn_id,
            "outcome": "completed",
            "modelCalls": 2,
            "toolCalls": 1,
            "inputSourceCount": source_count,
            "sources": turn.snapshots.iter().map(|snapshot| snapshot.source_id.clone()).collect::<Vec<_>>(),
        }),
    )
    .await?;
    eprintln!(
        "[fixture] {} completed modelCalls=2 toolCalls=1 inputSources={source_count}",
        turn.turn_id
    );
    Ok(())
}

/// Resolve every snapshot's immutable refs and build the full request.
async fn assemble_full_input(
    sources: &[Arc<dyn AssistantSource>],
    snapshots: &[SourceSnapshot],
    turn_id: &str,
) -> Result<Value, String> {
    let mut entries = Vec::with_capacity(snapshots.len());
    for snapshot in snapshots {
        let source = sources
            .iter()
            .find(|source| source.descriptor().source_id == snapshot.source_id)
            .ok_or_else(|| format!("no registered adapter for {}", snapshot.source_id))?;
        let state_ref = snapshot
            .state_ref
            .as_ref()
            .ok_or_else(|| format!("{} has no stateRef", snapshot.source_id))?;
        let state = source
            .resolve_state(state_ref)
            .await
            .map_err(|error| format!("{} state resolve failed: {error:?}", snapshot.source_id))?;
        if state.revision() != snapshot.revision || state.hash() != snapshot.state_hash {
            return Err(format!(
                "{} resolved state does not match its snapshot",
                snapshot.source_id
            ));
        }
        let state_content = std::str::from_utf8(state.as_bytes())
            .map_err(|error| format!("{} state is not UTF-8: {error}", snapshot.source_id))?;
        if state_content.is_empty() {
            return Err(format!("{} resolved empty state content", snapshot.source_id));
        }

        let mut work_items = Vec::with_capacity(snapshot.work_items.len());
        for item in &snapshot.work_items {
            let payload = source.resolve_payload(&item.payload_ref).await.map_err(|error| {
                format!(
                    "{} payload {} resolve failed: {error:?}",
                    snapshot.source_id, item.work_key
                )
            })?;
            if payload.revision() != item.revision || payload.hash() != item.payload_hash {
                return Err(format!(
                    "{} payload {} does not match its work item",
                    snapshot.source_id, item.work_key
                ));
            }
            let payload_content = std::str::from_utf8(payload.as_bytes())
                .map_err(|error| format!("{} payload {} is not UTF-8: {error}", snapshot.source_id, item.work_key))?;
            work_items.push(json!({
                "workKey": item.work_key,
                "revision": payload.revision(),
                "payloadHash": payload.hash(),
                "content": payload_content,
            }));
        }

        entries.push(json!({
            "sourceId": snapshot.source_id,
            "sourceVersion": snapshot.source_version,
            "revision": state.revision(),
            "stateHash": state.hash(),
            "content": state_content,
            "workItems": work_items,
        }));
    }

    let assembled = entries
        .iter()
        .filter_map(|entry| entry["sourceId"].as_str().map(str::to_string))
        .collect::<BTreeSet<_>>();
    let expected = sources
        .iter()
        .map(|source| source.descriptor().source_id)
        .collect::<BTreeSet<_>>();
    if assembled != expected {
        return Err(format!(
            "assembled request Sources {assembled:?} do not match registered Sources {expected:?}"
        ));
    }
    Ok(json!({"turnId": turn_id, "sources": entries}))
}

fn baseline_turn_state(
    attempt_id: &str,
    turn_id: &str,
    observed_at: chrono::DateTime<Utc>,
    snapshots: &[SourceSnapshot],
    registry_version: &str,
    mode: SuppressionMode,
) -> Value {
    json!({
        "stateId": format!("state:{attempt_id}"),
        "stateVersion": TURN_STATE_VERSION,
        "turnId": turn_id,
        "observedAt": observed_at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        "sources": snapshots.iter().map(|source| json!({
            "sourceId": source.source_id,
            "sourceVersion": source.source_version,
            "revision": source.revision,
            "stateHash": source.state_hash,
            "available": source.available,
            "empty": source.empty,
        })).collect::<Vec<_>>(),
        "controlCursor": 0,
        "registryVersion": registry_version,
        "mode": mode.as_str(),
    })
}

async fn append_event(
    db: &Db,
    workspace_id: &str,
    session_id: &str,
    event_id: &str,
    event_type: &str,
    payload_json: Value,
) -> Result<(), String> {
    let input = HarnessEventInput {
        event_id: event_id.to_string(),
        workspace_id: workspace_id.to_string(),
        session_id: session_id.to_string(),
        execution_mode: ExecutionMode::Production,
        run_id: None,
        task_id: None,
        event_type: event_type.to_string(),
        occurred_at: Utc::now(),
        cause_id: None,
        schema_v: 1,
        payload_digest: format!("fixture-digest-{event_id}"),
        payload_json,
    };
    db.append_session_event(&input)
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

// ── Synthetic gate evidence (server-configured loopback only) ──────────────

fn suppression_error(error: SuppressionError) -> Box<dyn std::error::Error> {
    Box::new(std::io::Error::other(format!("{error:?}")))
}

async fn seed_synthetic_gate_evidence(
    db: &Db,
    workspace_id: &str,
    session_key: &str,
    registry_version: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let now = Utc::now();
    let base = now - ChronoDuration::days(10);

    let mut shadow = Vec::with_capacity(100);
    for index in 0..100u64 {
        let observed_at = base + ChronoDuration::days((index % 3) as i64) + ChronoDuration::minutes(index as i64);
        let would_skip = index < 80;
        shadow.push(SuppressionTurnObservation {
            workspace_id: workspace_id.to_string(),
            session_id: session_key.to_string(),
            turn_id: format!("turn:synthetic-shadow-{index:03}"),
            attempt_id: format!("fixture:shadow:{index}"),
            mode: SuppressionMode::Shadow,
            observed_at,
            would_skip,
            vetoes: if would_skip {
                Vec::new()
            } else {
                vec![SkipVeto::SourceChanged.as_str().to_string()]
            },
            model_calls: 2,
            tool_calls: 1,
            preflight_latency_ms: 5,
            registry_version: registry_version.to_string(),
        });
    }
    for observation in &shadow {
        db.record_suppression_turn_observation(observation).await?;
    }
    let shadow_artifact = seal_gate_artifact(
        derive_shadow_gate_artifact(
            workspace_id,
            "gate:synthetic:shadow:v1".to_string(),
            "fixture-operator",
            now,
            true,
            &shadow,
        )
        .map_err(suppression_error)?,
    );
    db.insert_suppression_gate_artifact(&shadow_artifact).await?;

    let mut canary = Vec::with_capacity(20);
    for index in 0..20u64 {
        let observed_at = base + ChronoDuration::days(10) + ChronoDuration::minutes(index as i64);
        canary.push(SuppressionTurnObservation {
            workspace_id: workspace_id.to_string(),
            session_id: session_key.to_string(),
            turn_id: format!("turn:synthetic-canary-{index:03}"),
            attempt_id: format!("fixture:canary:{index}"),
            mode: SuppressionMode::Canary,
            observed_at,
            would_skip: true,
            vetoes: Vec::new(),
            model_calls: 0,
            tool_calls: 0,
            preflight_latency_ms: 5,
            registry_version: registry_version.to_string(),
        });
    }
    for observation in &canary {
        db.record_suppression_turn_observation(observation).await?;
    }
    let labels = DuplicateReportLabels {
        labeler: "fixture-reviewer".to_string(),
        labelled_at: now,
        baseline_reports: 100,
        canary_reports: 2,
    };
    let review = FalseSkipReview {
        reviewer: "fixture-reviewer".to_string(),
        reviewed_at: now,
        conclusion: FALSE_SKIP_CONCLUSION_NONE.to_string(),
        confirmed_false_skips: 0,
        samples: canary
            .iter()
            .map(|observation| FalseSkipSample {
                turn_id: observation.turn_id.clone(),
                reviewed: true,
                confirmed_false_skip: false,
            })
            .collect(),
    };
    let canary_artifact = seal_gate_artifact(
        derive_canary_gate_artifact(
            workspace_id,
            "gate:synthetic:canary:v1".to_string(),
            "fixture-operator",
            now,
            true,
            &canary,
            &shadow,
            labels,
            review,
        )
        .map_err(suppression_error)?,
    );
    db.insert_suppression_gate_artifact(&canary_artifact).await?;
    Ok(())
}

// ── Seeding and HTTP composition ───────────────────────────────────────────

async fn seed_platform_rows(pool: &sqlx::SqlitePool) -> Result<(), sqlx::Error> {
    let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    sqlx::query(
        "INSERT OR IGNORE INTO subscription_plans (id, name, display_name) VALUES ('plan_free', 'free', 'Free')",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT OR IGNORE INTO tenants (id, name, slug, status, plan_id, subscription_status, timezone, locale, created_at, updated_at)
         VALUES (?, ?, ?, 'active', 'plan_free', 'active', 'UTC', 'zh-CN', ?, ?)",
    )
    .bind(TENANT_ID)
    .bind("Suppression Fixture Tenant")
    .bind(TENANT_ID)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT OR IGNORE INTO workspaces (id, name, description, tenant_id, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(WORKSPACE_ID)
    .bind("Suppression Fixture Workspace")
    .bind("Loopback synthetic fixture; not for production")
    .bind(TENANT_ID)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT OR IGNORE INTO users (id, username, password_hash, display_name, is_enabled)
         VALUES (?, ?, 'x', 'Fixture User', 1)",
    )
    .bind(USER_ID)
    .bind("fixture-user")
    .execute(pool)
    .await?;
    sqlx::query("INSERT OR IGNORE INTO roles (id, name, is_administrator) VALUES (?, ?, 1)")
        .bind("role-fixture-admin")
        .bind("fixture-admin")
        .execute(pool)
        .await?;
    sqlx::query("INSERT OR IGNORE INTO user_roles (id, user_id, role_id) VALUES (?, ?, ?)")
        .bind("user-role-fixture-admin")
        .bind(USER_ID)
        .bind("role-fixture-admin")
        .execute(pool)
        .await?;
    Ok(())
}

fn fixture_claims() -> Claims {
    Claims {
        user_id: USER_ID.to_string(),
        token_id: "fixture-token".to_string(),
        username: "fixture".to_string(),
        tenant_id: TENANT_ID.to_string(),
        workspace_id: WORKSPACE_ID.to_string(),
        exp: None,
    }
}

struct FixtureHandles {
    changing: Arc<ChangingSource>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChangeItemRequest {
    revision: String,
    text: String,
}

async fn fixture_auth(State(fixture): State<Arc<FixtureHandles>>, mut request: Request, next: Next) -> Response {
    // The synthetic source-control route is fixture-only; it performs no
    // workspace operation and carries no credentials. Everything under
    // `/api/` still requires the synthetic bearer token.
    let fixture_route = request.uri().path().starts_with("/fixture/");
    let authorized = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == format!("Bearer {FIXTURE_TOKEN}"));
    if !fixture_route && !authorized {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "loopback fixture requires Authorization: Bearer test-token"})),
        )
            .into_response();
    }
    request.extensions_mut().insert(fixture_claims());
    request.extensions_mut().insert(fixture);
    next.run(request).await
}

/// `POST /fixture/sources/changing/items/item-1`
async fn change_changing_item(
    Extension(fixture): Extension<Arc<FixtureHandles>>,
    Path(item_id): Path<String>,
    Json(request): Json<ChangeItemRequest>,
) -> Response {
    if item_id != CHANGING_ITEM || request.revision.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "only item-1 with a non-empty revision is supported"})),
        )
            .into_response();
    }
    fixture.changing.set(request.revision.clone(), request.text.clone());
    Json(json!({
        "sourceId": CHANGING_SOURCE,
        "itemId": item_id,
        "revision": request.revision,
        "text": request.text,
    }))
    .into_response()
}

fn configure_fixture_environment() {
    // SAFETY: the example runs as a single-threaded fixture setup before any
    // worker thread reads configuration.
    unsafe {
        std::env::set_var("TINYIOTHUB__SERVER__HOST", "127.0.0.1");
        std::env::set_var("TINYIOTHUB__SERVER__PORT", "3109");
        std::env::set_var("TINYIOTHUB__DATABASE__URL", "sqlite::memory:");
        std::env::set_var(
            "TINYIOTHUB__SECURITY__JWT__SECRET",
            "fixture-jwt-secret-that-is-at-least-32-chars",
        );
        std::env::set_var("TINYIOTHUB__SECURITY__JWT__EXPIRATION_SECS", "3600");
        std::env::set_var("TINYIOTHUB__SECURITY__JWT__ISSUER", "tinyiothub-fixture");
        std::env::set_var("TINYIOTHUB__SECURITY__JWT__AUDIENCE", "tinyiothub-fixture");
        std::env::set_var("TINYIOTHUB__MQTT__PRIMARY__HOST", "localhost");
        std::env::set_var("TINYIOTHUB__MQTT__PRIMARY__PORT", "1883");
        std::env::set_var("TINYIOTHUB__MQTT__CLIENT__CLIENT_ID", "fixture-client");
        std::env::set_var("TINYIOTHUB__MQTT__TOPICS__PREFIX", "fixture/");
        std::env::set_var("TINYIOTHUB__MQTT__TOPICS__HEARTBEAT", "heartbeat");
        std::env::set_var("TINYIOTHUB__MQTT__TOPICS__DEVICE_REGISTRATION", "register");
        std::env::set_var("TINYIOTHUB__MQTT__TOPICS__COMMAND", "command");
        std::env::set_var("TINYIOTHUB__MQTT__TOPICS__DATA_UPLOAD", "data");
        std::env::set_var("TINYIOTHUB__MQTT__TOPICS__ALARM", "alarm");
        std::env::set_var("TINYIOTHUB__LOGGING__LEVEL", "warn");
        std::env::set_var("TINYIOTHUB__ENVIRONMENT__NAME", "fixture");
        std::env::set_var("TINYIOTHUB__MINIMAX__BASE_URL", "http://127.0.0.1:9");
        std::env::set_var("TINYIOTHUB__MINIMAX__AUTH_TOKEN", "synthetic-fixture-token");
        std::env::set_var("TINYIOTHUB__MINIMAX__MODEL", "fixture-model");
        std::env::set_var("TINYIOTHUB__AGENT__MEMORY_BACKEND", "none");
        std::env::set_var("TINYIOTHUB__MARKETPLACE__ENABLED", "false");
        // Keep the fixture's per-workspace Harness store out of the checkout.
        std::env::set_var("TINYIOTHUB__PROJECT_ROOT", std::env::temp_dir());
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    configure_fixture_environment();
    let settings = tinyiothub_cloud::shared::config::load_configuration()?;

    let options = sqlx::sqlite::SqliteConnectOptions::from_str("sqlite::memory:")?
        .create_if_missing(true)
        .foreign_keys(true)
        .busy_timeout(std::time::Duration::from_millis(5_000));
    // In-memory SQLite is per-connection. One pooled connection keeps every
    // request on the same database for deterministic fixture behavior.
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    migrations::run_migrations(&pool).await?;
    seed_platform_rows(&pool).await?;

    let workspace_root = tempfile::tempdir()?;
    let mut state = AppState::new(Arc::new(ThingCache::new()), pool.clone(), &settings);
    state.db = Arc::new(Db::new(pool).with_workspace_store_root(workspace_root.path().join("agents")));

    // Build the two example Sources and prove the shared contract.
    let static_source: Arc<dyn AssistantSource> = Arc::new(StaticSource::new());
    let changing = Arc::new(ChangingSource::new());
    let changing_source: Arc<dyn AssistantSource> = changing.clone();
    let mut registry = SourceRegistry::new();
    registry.register(static_source.clone()).map_err(suppression_error)?;
    registry.register(changing_source.clone()).map_err(suppression_error)?;
    let session_key = demo_session_key(WORKSPACE_ID);
    registry
        .assert_source_contract(&SourceContext::new(WORKSPACE_ID, &session_key))
        .await
        .map_err(suppression_error)?;
    let registry_version = registry.registry_version();
    let registry = Arc::new(registry);
    let sources = vec![static_source, changing_source];

    seed_synthetic_gate_evidence(&state.db, WORKSPACE_ID, &session_key, &registry_version).await?;

    let service = SuppressionService::new(registry.clone());
    let ops = Arc::new(FixtureOps::new(state.db.clone(), registry.clone(), service, sources));
    state.suppression_ops = Some(ops);
    // Loopback fixture only: production never sets this flag.
    state.suppression_gate_allow_synthetic = true;

    let fixture = Arc::new(FixtureHandles {
        changing: changing.clone(),
    });
    let app = Router::new()
        .nest("/api/v1/workspaces", create_workspace_router::<AppState>())
        .route("/fixture/sources/changing/items/{item_id}", post(change_changing_item))
        .layer(axum::middleware::from_fn_with_state(fixture, fixture_auth))
        .with_state(state);

    let port = std::env::var("SUPPRESSION_QUICKSTART_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(3109);
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    println!("suppression quickstart fixture listening on http://127.0.0.1:{port}");
    println!("workspace={WORKSPACE_ID} token={FIXTURE_TOKEN} tenant={TENANT_ID}");
    println!("sources={:?} registryVersion={registry_version}", registry.source_ids());
    println!("synthetic gate evidence: shadow + canary (loopback only)");
    println!("press Ctrl-C to stop");
    axum::serve(listener, app).await?;
    drop(workspace_root);
    Ok(())
}
