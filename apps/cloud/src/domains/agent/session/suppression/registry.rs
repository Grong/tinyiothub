//! Source registry and the six first-party semantic Source adapters.
//!
//! This module is the only place where the initial Source set is registered:
//! `workspace.snapshot`, `session.events`, `turn.brief`, `skills`,
//! `short_term` and `memory`, all at `sourceVersion = 1`.
//!
//! The Harness only understands descriptors, opaque revisions, refs and
//! canonical hashes. Every business rule (device health, event ownership,
//! skill injection mode, file presence) stays inside its adapter.

// The typed error is the public prevention contract; boxing it would change
// every adapter signature for a clippy size heuristic.
#![allow(clippy::result_large_err)]

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use tinyiothub_core::config::SystemPromptsConfig;
use tinyiothub_core::harness::{ExecutionMode, HarnessEvent};
use tinyiothub_runtime::watch::live_state::WorkspaceSnapshot;
use tinyiothub_skills::LoadedSkill;
use tinyiothub_storage::Db;
use tinyiothub_storage::harness_event::EventFilter;
use tokio_util::sync::CancellationToken;
use unicode_normalization::UnicodeNormalization;

use crate::domains::agent::prompt::{MEMORY_MAX_TOKENS, MemorySelection, PromptMemorySource, paths, select_memory};

use super::canonical::{
    canonical_bytes, canonical_source_state, hash_payload, hash_state, registry_version as canonical_registry_version,
};
use super::contract::{
    PayloadRef, SourceDescriptor, SourceSnapshot, WorkItem, WorkItemDisposition, validate_descriptors,
};
// Re-exported for the conformance tests and for later harness modules.
pub(crate) use super::contract::{CanonicalBytes, StateRef};
use super::error::ErrorCode;
pub(crate) use super::error::SuppressionError;

/// The six first-party Source IDs, in registration order.
pub const WORKSPACE_SNAPSHOT_SOURCE: &str = "workspace.snapshot";
pub const SESSION_EVENTS_SOURCE: &str = "session.events";
pub const TURN_BRIEF_SOURCE: &str = "turn.brief";
pub const SKILLS_SOURCE: &str = "skills";
pub const SHORT_TERM_SOURCE: &str = "short_term";
pub const MEMORY_SOURCE: &str = "memory";

/// Every initial adapter starts at schema version 1.
pub const SOURCE_VERSION: u32 = 1;

/// Workspace mode attached to a capture. It never changes adapter semantics;
/// it exists so the Harness can pass the eventual suppression mode through.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceMode {
    #[default]
    Off,
    Shadow,
    Canary,
    Enforce,
}

impl SourceMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Shadow => "shadow",
            Self::Canary => "canary",
            Self::Enforce => "enforce",
        }
    }
}

/// Everything an adapter may know about a capture. No credentials, provider
/// handles, payloads or business rules enter this type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceContext {
    pub workspace_id: String,
    pub session_id: String,
    pub observed_at: DateTime<Utc>,
    pub mode: SourceMode,
}

impl SourceContext {
    pub fn new(workspace_id: impl Into<String>, session_id: impl Into<String>) -> Self {
        Self {
            workspace_id: workspace_id.into(),
            session_id: session_id.into(),
            observed_at: Utc::now(),
            mode: SourceMode::Off,
        }
    }
}

/// Port for the existing live workspace snapshot. Implemented by the
/// composition layer around `ThingAgentManager::workspace_snapshot`; adapters
/// never invent a second device-state store.
#[async_trait]
pub trait WorkspaceSnapshotProvider: Send + Sync {
    async fn workspace_snapshot(&self, workspace_id: &str) -> Option<WorkspaceSnapshot>;
}

/// A semantic Source adapter. The Harness only calls `descriptor`, `capture`
/// and the two resolvers; everything else is the adapter's business.
#[async_trait]
pub trait AssistantSource: Send + Sync {
    fn descriptor(&self) -> SourceDescriptor;
    async fn capture(&self, cx: &SourceContext) -> Result<SourceSnapshot, SuppressionError>;
    async fn resolve_state(&self, reference: &StateRef) -> Result<CanonicalBytes, SuppressionError>;
    async fn resolve_payload(&self, reference: &PayloadRef) -> Result<CanonicalBytes, SuppressionError>;
}

struct RegisteredSource {
    descriptor: SourceDescriptor,
    source: Arc<dyn AssistantSource>,
}

/// Resolved immutable content for one captured Source. Full Turns build input
/// from these exact bytes rather than rereading mutable stores.
#[derive(Clone, Debug)]
pub struct ResolvedSource {
    pub source_id: String,
    pub work_items: Vec<WorkItem>,
    pub state: CanonicalBytes,
    pub payloads: Vec<CanonicalBytes>,
}

/// Registration and conformance entry point for all Source adapters.
#[derive(Default)]
pub struct SourceRegistry {
    sources: Vec<RegisteredSource>,
}

impl SourceRegistry {
    pub fn new() -> Self {
        Self { sources: Vec::new() }
    }

    /// Register one adapter. Duplicate `sourceId` values are rejected, so the
    /// registry is the single owner of the six-Source list.
    pub fn register(&mut self, source: Arc<dyn AssistantSource>) -> Result<(), SuppressionError> {
        let descriptor = source.descriptor();
        if descriptor.source_id.is_empty() {
            return Err(contract_error(
                "source_contract_invalid",
                "source descriptor requires a non-empty sourceId",
            ));
        }
        if self
            .sources
            .iter()
            .any(|registered| registered.descriptor.source_id == descriptor.source_id)
        {
            return Err(contract_error(
                "duplicate_source_id",
                "registry already contains this sourceId",
            ));
        }
        self.sources.push(RegisteredSource { descriptor, source });
        Ok(())
    }

    pub fn source_ids(&self) -> Vec<&str> {
        self.sources
            .iter()
            .map(|registered| registered.descriptor.source_id.as_str())
            .collect()
    }

    pub fn descriptors(&self) -> Vec<SourceDescriptor> {
        self.sources
            .iter()
            .map(|registered| registered.descriptor.clone())
            .collect()
    }

    pub fn registry_version(&self) -> String {
        canonical_registry_version(&self.descriptors())
    }

    /// Capture every registered Source in registration order. A capture error
    /// or a descriptor/snapshot mismatch aborts the whole batch: the Harness
    /// must never compare a partial Source set.
    pub async fn capture_all(&self, cx: &SourceContext) -> Result<Vec<SourceSnapshot>, SuppressionError> {
        validate_descriptors(&self.descriptors())?;
        let mut snapshots = Vec::with_capacity(self.sources.len());
        for registered in &self.sources {
            let snapshot = registered.source.capture(cx).await?;
            snapshot.validate(&registered.descriptor)?;
            snapshots.push(snapshot);
        }
        Ok(snapshots)
    }

    /// Bounded preflight capture.
    ///
    /// Every Source gets its own deadline, and an adapter's internal reference
    /// resolution is inside the same capture future, so the effective
    /// per-Source budget is `min(capture_deadline, resolve_deadline)` (2s/2s
    /// with the defaults). Cancellation drops the slow future immediately; the
    /// caller's whole-preflight deadline is the outer bound.
    pub async fn capture_all_bounded(
        &self,
        cx: &SourceContext,
        capture_deadline: std::time::Duration,
        resolve_deadline: std::time::Duration,
        cancel: &CancellationToken,
    ) -> Result<Vec<SourceSnapshot>, SuppressionError> {
        validate_descriptors(&self.descriptors())?;
        let per_source = capture_deadline.min(resolve_deadline);
        let mut snapshots = Vec::with_capacity(self.sources.len());
        for registered in &self.sources {
            let snapshot = tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(SuppressionError::from_code(ErrorCode::PreflightTimeout)),
                result = tokio::time::timeout(per_source, registered.source.capture(cx)) => match result {
                    Ok(result) => result?,
                    Err(_) => return Err(SuppressionError::from_code(ErrorCode::PreflightTimeout)),
                },
            };
            snapshot.validate(&registered.descriptor)?;
            snapshots.push(snapshot);
        }
        Ok(snapshots)
    }

    /// Resolve every captured ref with an independent per-Source deadline and
    /// verify revision/hash before the result can authorize skip or build a
    /// full Turn. The caller's whole-preflight deadline wraps this batch too.
    pub async fn resolve_all_bounded(
        &self,
        snapshots: &[SourceSnapshot],
        resolve_deadline: std::time::Duration,
        cancel: &CancellationToken,
    ) -> Result<Vec<ResolvedSource>, SuppressionError> {
        if snapshots.len() != self.sources.len() {
            return Err(contract_error(
                "source_contract_invalid",
                "capture/source count changed",
            ));
        }
        let mut resolved = Vec::with_capacity(snapshots.len());
        for (registered, snapshot) in self.sources.iter().zip(snapshots.iter()) {
            if !snapshot.available {
                return Err(contract_error(
                    "source_unavailable",
                    "cannot resolve an unavailable Source",
                ));
            }
            let future = async {
                let state_ref = snapshot.state_ref.as_ref().ok_or_else(ref_mismatch_error)?;
                let state = registered.source.resolve_state(state_ref).await?;
                if state.revision() != snapshot.revision || state.hash() != snapshot.state_hash {
                    return Err(ref_mismatch_error());
                }
                let mut payloads = Vec::with_capacity(snapshot.work_items.len());
                for item in &snapshot.work_items {
                    let payload = registered.source.resolve_payload(&item.payload_ref).await?;
                    if payload.revision() != item.revision || payload.hash() != item.payload_hash {
                        return Err(ref_mismatch_error());
                    }
                    payloads.push(payload);
                }
                Ok(ResolvedSource {
                    source_id: snapshot.source_id.clone(),
                    work_items: snapshot.work_items.clone(),
                    state,
                    payloads,
                })
            };
            let source = tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(SuppressionError::from_code(ErrorCode::PreflightTimeout)),
                result = tokio::time::timeout(resolve_deadline, future) => match result {
                    Ok(result) => result?,
                    Err(_) => return Err(SuppressionError::from_code(ErrorCode::PreflightTimeout)),
                },
            };
            resolved.push(source);
        }
        Ok(resolved)
    }

    /// Shared conformance harness used by every adapter in tests (and usable as
    /// a startup check): deterministic capture, validated descriptor/snapshot
    /// shape, resolvable immutable refs, and rejection of stale refs, duplicate
    /// workKeys and descriptor mismatches.
    pub async fn assert_source_contract(&self, cx: &SourceContext) -> Result<(), SuppressionError> {
        validate_descriptors(&self.descriptors())?;
        for registered in &self.sources {
            let descriptor = &registered.descriptor;
            let first = registered.source.capture(cx).await?;
            first.validate(descriptor)?;
            let second = registered.source.capture(cx).await?;
            second.validate(descriptor)?;
            if first != second {
                return Err(contract_error(
                    "source_contract_invalid",
                    "capture must be deterministic when content is unchanged",
                ));
            }
            if !first.available {
                continue;
            }

            let state_ref = first
                .state_ref
                .as_ref()
                .expect("available snapshots were validated above");
            let state = registered.source.resolve_state(state_ref).await?;
            if state.revision() != first.revision || state.hash() != first.state_hash {
                return Err(contract_error(
                    "source_ref_mismatch",
                    "resolved state does not match the captured revision/hash",
                ));
            }
            for item in &first.work_items {
                let payload = registered.source.resolve_payload(&item.payload_ref).await?;
                if payload.revision() != item.revision || payload.hash() != item.payload_hash {
                    return Err(contract_error(
                        "source_ref_mismatch",
                        "resolved payload does not match the work item revision/hash",
                    ));
                }
            }

            let mut stale_state = state_ref.clone();
            stale_state.revision = "stale-revision".to_string();
            if registered.source.resolve_state(&stale_state).await.is_ok() {
                return Err(contract_error(
                    "source_ref_mismatch",
                    "a stale state ref must be rejected",
                ));
            }
            if let Some(item) = first.work_items.first() {
                let mut stale_payload = item.payload_ref.clone();
                stale_payload.revision = "stale-revision".to_string();
                if registered.source.resolve_payload(&stale_payload).await.is_ok() {
                    return Err(contract_error(
                        "source_ref_mismatch",
                        "a stale payload ref must be rejected",
                    ));
                }
            }

            let wrong_descriptor = SourceDescriptor {
                source_id: "other.source".to_string(),
                source_version: descriptor.source_version,
            };
            if first.clone().validate(&wrong_descriptor).is_ok() {
                return Err(contract_error(
                    "source_descriptor_mismatch",
                    "a descriptor/snapshot mismatch must be rejected",
                ));
            }
            if let Some(item) = first.work_items.first() {
                let mut duplicate = first.clone();
                duplicate.work_items.push(item.clone());
                duplicate.empty = false;
                if duplicate.validate(descriptor).is_ok() {
                    return Err(contract_error(
                        "duplicate_work_key",
                        "duplicate workKey values must be rejected",
                    ));
                }
            }
        }
        Ok(())
    }
}

/// One external semantic Session Event after normalization.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticSessionEvent {
    pub event_id: String,
    pub event_type: String,
    pub payload: Value,
}

/// Batched Session Events capture plus the cursor-owned user high watermark.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionEventsCapture {
    pub events: Vec<SemanticSessionEvent>,
    pub control_cursor: i64,
}

const EVENT_PAGE_SIZE: i64 = 200;

/// Load one session's production events in batches and split them into the
/// user-message cursor (control input only) and external semantic events.
///
/// The cursor predicate is exactly the one Task 2 uses for the skip re-check:
/// `execution_mode = 'production' AND type = 'user/message' AND
/// payload_json->>'$.role' = 'user'`, scoped to the session. No raw SQL is
/// added here; the existing `session_event_page` delegate already applies the
/// execution-mode/session filter.
pub(crate) async fn load_session_events(
    db: &Db,
    workspace_id: &str,
    session_id: &str,
) -> Result<SessionEventsCapture, SuppressionError> {
    let filter = EventFilter {
        session_id: Some(session_id.to_string()),
        run_id: None,
        task_id: None,
        execution_mode: ExecutionMode::Production,
    };
    let mut events = Vec::new();
    let mut after = 0i64;
    loop {
        let page = db
            .session_event_page(workspace_id, &filter, after, EVENT_PAGE_SIZE)
            .await
            .map_err(|_| source_unavailable("load workspace session events"))?;
        let count = page.events.len();
        if let Some(last) = page.events.last() {
            after = last.seq;
        }
        events.extend(page.events);
        if count < EVENT_PAGE_SIZE as usize || after >= page.high_watermark {
            break;
        }
    }

    let control_cursor = events
        .iter()
        .filter(|event| {
            event.event_type == "user/message" && event.payload_json.get("role").and_then(Value::as_str) == Some("user")
        })
        .map(|event| event.seq)
        .max()
        .unwrap_or(0);
    let external = events
        .into_iter()
        .filter(is_external_semantic_event)
        .map(|event| SemanticSessionEvent {
            event_id: event.event_id,
            event_type: event.event_type,
            payload: event.payload_json,
        })
        .collect();
    Ok(SessionEventsCapture {
        events: external,
        control_cursor,
    })
}

/// Harness lifecycle, agent activity, assistant output and events owned by
/// other Source adapters never become semantic work items. The v2 vocabulary
/// uses slash names; the cursor-owned user messages are excluded here and
/// participate only through `control_cursor`.
fn is_external_semantic_event(event: &HarnessEvent) -> bool {
    if !event.event_type.contains('.') && !event.event_type.contains('/') {
        return false;
    }
    if event.event_type == "turn.state" {
        return false;
    }
    const EXCLUDED_PREFIXES: &[&str] = &[
        "turn.",
        "turn/",
        "step.",
        "step/",
        "tool.",
        "tool/",
        "session.",
        "session/",
        "wake.",
        "wake/",
        "trigger.",
        "trigger/",
        "message.",
        "message/",
        "user.",
        "user/",
        // question/approval/todo/task events are semantic work here: the
        // integrated registry has no other Source that owns their current
        // payloads. Excluding them would hide answers and open work until the
        // next timed reconciliation.
        "run.",
        "run/",
        "assistant.",
        "assistant/",
        "harness.",
        "harness/",
        "capability.",
        "capability/",
        "decision.",
        "decision/",
        "subagent.",
        "subagent/",
        "skill.",
        "skill/",
    ];
    !EXCLUDED_PREFIXES
        .iter()
        .any(|prefix| event.event_type.starts_with(prefix))
}

/// Immutable captured state/payload retention shared by the file-, DB- and
/// snapshot-backed adapters. Closed or directly deleted work items keep their
/// last payload here (at least until a later successful full Turn may release
/// it); a missing retained payload is reported, never faked as a healthy empty
/// set.
#[derive(Default)]
pub(crate) struct SourceCache {
    entries: Mutex<BTreeMap<String, CachedEntry>>,
    state: Mutex<Option<CanonicalBytes>>,
    db: Option<Arc<Db>>,
    identity: Mutex<Option<CacheIdentity>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CacheIdentity {
    workspace_id: String,
    session_id: String,
    source_id: String,
}

#[derive(Clone, Debug)]
struct CachedEntry {
    payload_bytes: Option<Vec<u8>>,
    payload_hash: Option<String>,
    revision: String,
    disposition: WorkItemDisposition,
}

impl SourceCache {
    fn persistent(db: Arc<Db>) -> Self {
        Self {
            db: Some(db),
            ..Self::default()
        }
    }

    fn persistent_identity(&self) -> Result<CacheIdentity, SuppressionError> {
        self.identity
            .lock()
            .expect("source cache poisoned")
            .clone()
            .ok_or_else(ref_mismatch_error)
    }

    async fn merge_persistent(
        &self,
        cx: &SourceContext,
        source_id: &str,
        current: Vec<(String, Vec<u8>, WorkItemDisposition)>,
    ) -> Result<Vec<WorkItem>, SuppressionError> {
        *self.identity.lock().expect("source cache poisoned") = Some(CacheIdentity {
            workspace_id: cx.workspace_id.clone(),
            session_id: cx.session_id.clone(),
            source_id: source_id.to_string(),
        });
        let Some(db) = &self.db else {
            return self.merge(current);
        };
        let pool = db
            .source_cache_pool(&cx.workspace_id)
            .await
            .map_err(|_| source_unavailable("open the workspace Source cache"))?;
        let rows = sqlx::query(
            "SELECT work_key, revision, disposition, payload_hash, payload_bytes \
             FROM suppression_source_entries WHERE workspace_id = ? AND session_id = ? AND source_id = ?",
        )
        .bind(&cx.workspace_id)
        .bind(&cx.session_id)
        .bind(source_id)
        .fetch_all(&pool)
        .await
        .map_err(|_| source_unavailable("load retained Source entries"))?;
        let retained = SourceCache::default();
        {
            let mut entries = retained.entries.lock().expect("source cache poisoned");
            for row in rows {
                let disposition = match row.get::<String, _>("disposition").as_str() {
                    "active" => WorkItemDisposition::Active,
                    "waiting" => WorkItemDisposition::Waiting,
                    "closed" => WorkItemDisposition::Closed,
                    _ => {
                        return Err(contract_error(
                            "source_contract_invalid",
                            "unknown retained disposition",
                        ));
                    }
                };
                entries.insert(
                    row.get::<String, _>("work_key"),
                    CachedEntry {
                        payload_bytes: Some(row.get::<Vec<u8>, _>("payload_bytes")),
                        payload_hash: Some(row.get::<String, _>("payload_hash")),
                        revision: row.get::<String, _>("revision"),
                        disposition,
                    },
                );
            }
        }
        let items = retained.merge(current)?;
        let entries_to_persist = {
            retained
                .entries
                .lock()
                .expect("source cache poisoned")
                .iter()
                .map(|(key, entry)| (key.clone(), entry.clone()))
                .collect::<Vec<_>>()
        };
        let mut tx = pool
            .begin()
            .await
            .map_err(|_| source_unavailable("begin Source cache write"))?;
        for chunk in entries_to_persist.chunks(50) {
            let mut sql = String::from(
                "INSERT INTO suppression_source_entries \
                 (workspace_id, session_id, source_id, work_key, revision, disposition, payload_hash, payload_bytes, updated_at) VALUES ",
            );
            for index in 0..chunk.len() {
                if index > 0 {
                    sql.push(',');
                }
                sql.push_str("(?, ?, ?, ?, ?, ?, ?, ?, ?)");
            }
            sql.push_str(
                " ON CONFLICT(workspace_id, session_id, source_id, work_key) DO UPDATE SET \
                   revision = excluded.revision, disposition = excluded.disposition, \
                   payload_hash = excluded.payload_hash, payload_bytes = excluded.payload_bytes, \
                   updated_at = excluded.updated_at",
            );
            let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
            for (work_key, entry) in chunk {
                let Some(payload_bytes) = &entry.payload_bytes else {
                    return Err(missing_payload_error());
                };
                let Some(payload_hash) = &entry.payload_hash else {
                    return Err(missing_payload_error());
                };
                query = query
                    .bind(&cx.workspace_id)
                    .bind(&cx.session_id)
                    .bind(source_id)
                    .bind(work_key)
                    .bind(&entry.revision)
                    .bind(disposition_name(entry.disposition))
                    .bind(payload_hash)
                    .bind(payload_bytes)
                    .bind(Utc::now().to_rfc3339());
            }
            query
                .execute(&mut *tx)
                .await
                .map_err(|_| source_unavailable("retain Source entries in one batch"))?;
        }
        tx.commit()
            .await
            .map_err(|_| source_unavailable("commit Source entries"))?;
        Ok(items)
    }

    async fn snapshot_persistent(
        &self,
        cx: &SourceContext,
        source_id: &str,
        items: Vec<WorkItem>,
    ) -> Result<SourceSnapshot, SuppressionError> {
        let snapshot = self.snapshot(source_id, items)?;
        let Some(db) = &self.db else {
            return Ok(snapshot);
        };
        let state = self
            .state
            .lock()
            .expect("source cache poisoned")
            .clone()
            .ok_or_else(ref_mismatch_error)?;
        let pool = db
            .source_cache_pool(&cx.workspace_id)
            .await
            .map_err(|_| source_unavailable("open the workspace Source state cache"))?;
        sqlx::query(
            "INSERT OR IGNORE INTO suppression_source_states \
             (workspace_id, session_id, source_id, state_hash, revision, state_bytes, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&cx.workspace_id)
        .bind(&cx.session_id)
        .bind(source_id)
        .bind(state.hash())
        .bind(state.revision())
        .bind(state.as_bytes())
        .bind(Utc::now().to_rfc3339())
        .execute(&pool)
        .await
        .map_err(|_| source_unavailable("retain immutable Source state"))?;
        Ok(snapshot)
    }

    async fn resolve_state_persistent(&self, reference: &StateRef) -> Result<CanonicalBytes, SuppressionError> {
        let Some(db) = &self.db else {
            return self.resolve_state(reference);
        };
        let identity = self.persistent_identity()?;
        let pool = db
            .source_cache_pool(&identity.workspace_id)
            .await
            .map_err(|_| source_unavailable("open the workspace Source state cache"))?;
        let bytes = sqlx::query_scalar::<_, Vec<u8>>(
            "SELECT state_bytes FROM suppression_source_states \
             WHERE workspace_id = ? AND session_id = ? AND source_id = ? AND state_hash = ? AND revision = ?",
        )
        .bind(identity.workspace_id)
        .bind(identity.session_id)
        .bind(identity.source_id)
        .bind(&reference.state_hash)
        .bind(&reference.revision)
        .fetch_optional(&pool)
        .await
        .map_err(|_| ref_mismatch_error())?
        .ok_or_else(ref_mismatch_error)?;
        CanonicalBytes::state(bytes, reference.revision.clone())
    }

    async fn resolve_payload_persistent(&self, reference: &PayloadRef) -> Result<CanonicalBytes, SuppressionError> {
        let Some(db) = &self.db else {
            return self.resolve_payload(reference);
        };
        let identity = self.persistent_identity()?;
        let pool = db
            .source_cache_pool(&identity.workspace_id)
            .await
            .map_err(|_| source_unavailable("open the workspace Source payload cache"))?;
        let bytes = sqlx::query_scalar::<_, Vec<u8>>(
            "SELECT payload_bytes FROM suppression_source_entries \
             WHERE workspace_id = ? AND session_id = ? AND source_id = ? AND payload_hash = ? AND revision = ? LIMIT 1",
        )
        .bind(identity.workspace_id)
        .bind(identity.session_id)
        .bind(identity.source_id)
        .bind(&reference.payload_hash)
        .bind(&reference.revision)
        .fetch_optional(&pool)
        .await
        .map_err(|_| missing_payload_error())?
        .ok_or_else(missing_payload_error)?;
        CanonicalBytes::payload(bytes, reference.revision.clone())
    }

    /// Merge the current semantic items into the retained set. Items that are
    /// absent from `current` become closed tombstones with a changed revision
    /// and their last payload retained.
    fn merge(&self, current: Vec<(String, Vec<u8>, WorkItemDisposition)>) -> Result<Vec<WorkItem>, SuppressionError> {
        let mut entries = self.entries.lock().expect("source cache poisoned");
        // Reject duplicate canonical identities instead of silently keeping
        // the last item: work item identity is `(sourceId, workKey)`.
        let mut current_keys = HashSet::with_capacity(current.len());
        let mut normalized_current = Vec::with_capacity(current.len());
        for (work_key, bytes, disposition) in current {
            let work_key: String = work_key.nfc().collect();
            if !current_keys.insert(work_key.clone()) {
                return Err(contract_error(
                    "duplicate_work_key",
                    "capture produced canonically equivalent workKey values",
                ));
            }
            normalized_current.push((work_key, bytes, disposition));
        }

        for (work_key, bytes, disposition) in normalized_current {
            let payload_hash = hash_payload(&bytes);
            entries.insert(
                work_key,
                CachedEntry {
                    payload_bytes: Some(bytes),
                    payload_hash: Some(payload_hash.clone()),
                    revision: payload_hash,
                    disposition,
                },
            );
        }

        for (work_key, entry) in entries.iter_mut() {
            if current_keys.contains(work_key) || entry.disposition == WorkItemDisposition::Closed {
                continue;
            }
            let Some(payload_hash) = entry.payload_hash.clone() else {
                return Err(missing_payload_error());
            };
            entry.disposition = WorkItemDisposition::Closed;
            entry.revision = format!("closed:{payload_hash}");
        }

        let mut items = Vec::with_capacity(entries.len());
        for (work_key, entry) in entries.iter() {
            let (Some(payload_hash), Some(_payload_bytes)) = (entry.payload_hash.clone(), entry.payload_bytes.as_ref())
            else {
                return Err(missing_payload_error());
            };
            items.push(WorkItem {
                work_key: work_key.clone(),
                revision: entry.revision.clone(),
                disposition: entry.disposition,
                payload_ref: PayloadRef {
                    id: payload_ref_id(&payload_hash),
                    revision: entry.revision.clone(),
                    payload_hash: payload_hash.clone(),
                },
                payload_hash,
            });
        }
        Ok(items)
    }

    /// Build the complete available snapshot for the current retained set and
    /// remember its canonical state for immutable ref resolution.
    fn snapshot(&self, source_id: &str, items: Vec<WorkItem>) -> Result<SourceSnapshot, SuppressionError> {
        let bytes = canonical_source_state(source_id, &items)?;
        let state_hash = hash_state(&bytes);
        let revision = state_hash.clone();
        let state = CanonicalBytes::state(bytes, revision.clone())?;
        *self.state.lock().expect("source cache poisoned") = Some(state);
        let empty = items.is_empty();
        Ok(SourceSnapshot {
            source_id: source_id.to_string(),
            source_version: SOURCE_VERSION,
            revision: revision.clone(),
            state_hash: state_hash.clone(),
            available: true,
            empty,
            state_ref: Some(StateRef {
                id: state_ref_id(&revision),
                revision,
                state_hash,
            }),
            work_items: items,
            error: None,
        })
    }

    fn resolve_state(&self, reference: &StateRef) -> Result<CanonicalBytes, SuppressionError> {
        let state = self
            .state
            .lock()
            .expect("source cache poisoned")
            .clone()
            .ok_or_else(ref_mismatch_error)?;
        if state.revision() != reference.revision || state.hash() != reference.state_hash {
            return Err(ref_mismatch_error());
        }
        Ok(state)
    }

    fn resolve_payload(&self, reference: &PayloadRef) -> Result<CanonicalBytes, SuppressionError> {
        let entries = self.entries.lock().expect("source cache poisoned");
        let mut saw_hash = false;
        for entry in entries.values() {
            if entry.payload_hash.as_deref() != Some(reference.payload_hash.as_str()) {
                continue;
            }
            saw_hash = true;
            if payload_ref_id(&reference.payload_hash) != reference.id || entry.revision != reference.revision {
                continue;
            }
            let Some(bytes) = entry.payload_bytes.as_ref() else {
                return Err(missing_payload_error());
            };
            return CanonicalBytes::payload(bytes.clone(), reference.revision.clone());
        }
        if saw_hash {
            Err(ref_mismatch_error())
        } else {
            Err(missing_payload_error())
        }
    }

    #[cfg(test)]
    fn seed_missing(&self, work_key: &str) {
        self.entries.lock().expect("source cache poisoned").insert(
            work_key.to_string(),
            CachedEntry {
                payload_bytes: None,
                payload_hash: Some(hash_payload(b"lost-payload")),
                revision: "closed:lost-payload".to_string(),
                disposition: WorkItemDisposition::Closed,
            },
        );
    }
}

fn disposition_name(disposition: WorkItemDisposition) -> &'static str {
    match disposition {
        WorkItemDisposition::Active => "active",
        WorkItemDisposition::Waiting => "waiting",
        WorkItemDisposition::Closed => "closed",
    }
}

async fn capture_items(
    cx: &SourceContext,
    source_id: &str,
    cache: &SourceCache,
    current: Vec<(String, Vec<u8>, WorkItemDisposition)>,
) -> Result<SourceSnapshot, SuppressionError> {
    match cache.merge_persistent(cx, source_id, current).await {
        Ok(items) => match cache.snapshot_persistent(cx, source_id, items).await {
            Ok(snapshot) => Ok(snapshot),
            Err(error) => Ok(unavailable_snapshot(source_id, error)),
        },
        // A duplicate work key is a source contract violation: reject the
        // capture instead of collapsing it or masking it as unavailable.
        Err(error) if error.code == "duplicate_work_key" => Err(error),
        Err(error) => Ok(unavailable_snapshot(source_id, error)),
    }
}

fn unavailable_snapshot(source_id: &str, error: SuppressionError) -> SourceSnapshot {
    let revision = format!("unavailable:{}", error.code);
    let state_hash = hash_state(revision.as_bytes());
    SourceSnapshot {
        source_id: source_id.to_string(),
        source_version: SOURCE_VERSION,
        revision,
        state_hash,
        available: false,
        empty: false,
        state_ref: None,
        work_items: vec![],
        error: Some(error),
    }
}

fn descriptor(source_id: &str) -> SourceDescriptor {
    SourceDescriptor {
        source_id: source_id.to_string(),
        source_version: SOURCE_VERSION,
    }
}

fn payload_ref_id(payload_hash: &str) -> String {
    format!("payload:{payload_hash}")
}

fn state_ref_id(revision: &str) -> String {
    format!("state:{revision}")
}

fn document_payload(content: &str) -> Result<Vec<u8>, SuppressionError> {
    canonical_bytes(&json!({ "content": content }))
}

/// Resolve the workspace directory exactly like `build_assistant_prompt`.
pub(crate) fn workspace_dir_for(config: &SystemPromptsConfig, workspace_id: &str) -> PathBuf {
    if config.workspace_dir.is_empty() {
        paths::workspace_dir(workspace_id)
    } else {
        Path::new(&config.workspace_dir).join(workspace_id)
    }
}

/// Read an optional workspace document. Missing files are a valid empty state;
/// every other I/O failure degrades the Source instead of pretending "empty".
pub(crate) fn read_workspace_file(workspace_dir: &Path, file_name: &str) -> Result<Option<String>, SuppressionError> {
    match std::fs::read_to_string(workspace_dir.join(file_name)) {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(source_unavailable(format!("read {file_name}"))),
    }
}

pub(crate) fn load_skills_for(workspace_dir: &Path) -> Vec<LoadedSkill> {
    crate::domains::agent::pool::load_workspace_skills(workspace_dir)
}

fn inject_mode(mode: tinyiothub_skills::InjectMode) -> &'static str {
    match mode {
        tinyiothub_skills::InjectMode::Always => "always",
        tinyiothub_skills::InjectMode::Index => "index",
        tinyiothub_skills::InjectMode::Trigger => "trigger",
    }
}

fn agent_id_from_session(session_id: &str) -> String {
    tinyiothub_agent::session::SessionKey::parse(session_id)
        .map(|key| key.agent_id)
        .unwrap_or_else(|_| "default".to_string())
}

fn source_unavailable(what: impl Into<String>) -> SuppressionError {
    SuppressionError::new(
        "source_unavailable",
        what,
        "restore the source dependency and reconcile",
    )
}

fn missing_payload_error() -> SuppressionError {
    SuppressionError::new(
        "work_item_payload_missing",
        "a closed work item has no retained payload",
        "restore the retained payload before reconciling",
    )
}

fn ref_mismatch_error() -> SuppressionError {
    SuppressionError::new(
        "source_ref_mismatch",
        "reference does not match the captured immutable content",
        "rebuild the snapshot and reconcile",
    )
}

fn contract_error(code: &str, what: impl Into<String>) -> SuppressionError {
    SuppressionError::new(code, what, "fix the source adapter contract and reconcile")
}

// ============================================================================
// Adapters
// ============================================================================

/// Live workspace snapshot: one work item per known device. Health/offline and
/// reported values are the adapter's semantics; timestamps never enter the
/// projection because `WorkspaceSnapshot` carries none.
pub struct WorkspaceSnapshotSource {
    provider: Option<Arc<dyn WorkspaceSnapshotProvider>>,
    cache: SourceCache,
}

impl WorkspaceSnapshotSource {
    pub fn new(provider: Option<Arc<dyn WorkspaceSnapshotProvider>>) -> Self {
        Self {
            provider,
            cache: SourceCache::default(),
        }
    }
}

#[async_trait]
impl AssistantSource for WorkspaceSnapshotSource {
    fn descriptor(&self) -> SourceDescriptor {
        descriptor(WORKSPACE_SNAPSHOT_SOURCE)
    }

    async fn capture(&self, cx: &SourceContext) -> Result<SourceSnapshot, SuppressionError> {
        let Some(provider) = &self.provider else {
            return Ok(unavailable_snapshot(
                WORKSPACE_SNAPSHOT_SOURCE,
                source_unavailable("workspace snapshot provider is not configured"),
            ));
        };
        let Some(snapshot) = provider.workspace_snapshot(&cx.workspace_id).await else {
            return Ok(unavailable_snapshot(
                WORKSPACE_SNAPSHOT_SOURCE,
                source_unavailable("workspace snapshot engine is not running"),
            ));
        };

        let mut current = Vec::with_capacity(snapshot.devices.len());
        for device in &snapshot.devices {
            let mut values = BTreeMap::new();
            for (key, value) in &device.values {
                values.insert(key.clone(), value.clone());
            }
            let mut properties = device.properties.clone();
            properties.sort();
            properties.dedup();
            let payload = json!({
                "thingId": device.thing_id,
                "name": device.name,
                "health": health_str(device.health),
                "online": device.online,
                "zone": device.zone,
                "values": values,
                "properties": properties,
            });
            let bytes = canonical_bytes(&payload)?;
            let disposition = if device.online {
                WorkItemDisposition::Active
            } else {
                WorkItemDisposition::Waiting
            };
            current.push((device.thing_id.clone(), bytes, disposition));
        }
        capture_items(cx, WORKSPACE_SNAPSHOT_SOURCE, &self.cache, current).await
    }

    async fn resolve_state(&self, reference: &StateRef) -> Result<CanonicalBytes, SuppressionError> {
        self.cache.resolve_state_persistent(reference).await
    }

    async fn resolve_payload(&self, reference: &PayloadRef) -> Result<CanonicalBytes, SuppressionError> {
        self.cache.resolve_payload_persistent(reference).await
    }
}

fn health_str(health: tinyiothub_runtime::watch::live_state::Health) -> &'static str {
    use tinyiothub_runtime::watch::live_state::Health;
    match health {
        Health::Ok => "ok",
        Health::Attention => "attention",
        Health::Bad => "bad",
        Health::Unknown => "unknown",
    }
}

/// Optional in-process snapshot slot. The composition layer may populate it
/// from the existing thing-agent manager; keeping it behind the provider port
/// avoids a second live-state path.
#[derive(Default)]
pub struct SnapshotProviderSlot {
    snapshot: Mutex<Option<WorkspaceSnapshot>>,
}

impl SnapshotProviderSlot {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(snapshot: WorkspaceSnapshot) -> Self {
        Self {
            snapshot: Mutex::new(Some(snapshot)),
        }
    }

    pub fn set(&self, snapshot: WorkspaceSnapshot) {
        *self.snapshot.lock().expect("snapshot slot poisoned") = Some(snapshot);
    }

    pub fn get(&self) -> Option<WorkspaceSnapshot> {
        self.snapshot.lock().expect("snapshot slot poisoned").clone()
    }
}

#[async_trait]
impl WorkspaceSnapshotProvider for SnapshotProviderSlot {
    async fn workspace_snapshot(&self, _workspace_id: &str) -> Option<WorkspaceSnapshot> {
        self.get()
    }
}

/// Session Events: Harness lifecycle, assistant output, other-domain events and
/// cursor-owned user messages are excluded; remaining external semantic events
/// use their stable event ID, type and normalized payload.
pub struct SessionEventsSource {
    db: Arc<Db>,
    cache: SourceCache,
}

impl SessionEventsSource {
    pub fn new(db: Arc<Db>) -> Self {
        Self {
            db,
            cache: SourceCache::default(),
        }
    }
}

#[async_trait]
impl AssistantSource for SessionEventsSource {
    fn descriptor(&self) -> SourceDescriptor {
        descriptor(SESSION_EVENTS_SOURCE)
    }

    async fn capture(&self, cx: &SourceContext) -> Result<SourceSnapshot, SuppressionError> {
        match load_session_events(&self.db, &cx.workspace_id, &cx.session_id).await {
            Ok(capture) => {
                let mut current = Vec::with_capacity(capture.events.len());
                for event in capture.events {
                    let payload = json!({
                        "eventId": event.event_id,
                        "type": event.event_type,
                        "payload": event.payload,
                    });
                    let bytes = canonical_bytes(&payload)?;
                    current.push((event.event_id, bytes, WorkItemDisposition::Active));
                }
                capture_items(cx, SESSION_EVENTS_SOURCE, &self.cache, current).await
            }
            Err(error) => Ok(unavailable_snapshot(SESSION_EVENTS_SOURCE, error)),
        }
    }

    async fn resolve_state(&self, reference: &StateRef) -> Result<CanonicalBytes, SuppressionError> {
        self.cache.resolve_state_persistent(reference).await
    }

    async fn resolve_payload(&self, reference: &PayloadRef) -> Result<CanonicalBytes, SuppressionError> {
        self.cache.resolve_payload_persistent(reference).await
    }
}

/// Turn Brief adapter. A missing or blank file is a valid empty Source; a file
/// that existed and was deleted becomes a closed tombstone with its last
/// payload.
pub struct TurnBriefSource {
    config: Arc<SystemPromptsConfig>,
    cache: SourceCache,
}

impl TurnBriefSource {
    pub fn new(config: Arc<SystemPromptsConfig>) -> Self {
        Self {
            config,
            cache: SourceCache::default(),
        }
    }
}

#[async_trait]
impl AssistantSource for TurnBriefSource {
    fn descriptor(&self) -> SourceDescriptor {
        descriptor(TURN_BRIEF_SOURCE)
    }

    async fn capture(&self, cx: &SourceContext) -> Result<SourceSnapshot, SuppressionError> {
        let workspace_dir = workspace_dir_for(&self.config, &cx.workspace_id);
        match read_workspace_file(&workspace_dir, "TURN_BRIEF.md") {
            Ok(Some(content)) if !content.trim().is_empty() => {
                let bytes = document_payload(&content)?;
                capture_items(
                    cx,
                    TURN_BRIEF_SOURCE,
                    &self.cache,
                    vec![("document".to_string(), bytes, WorkItemDisposition::Active)],
                )
                .await
            }
            Ok(_) => capture_items(cx, TURN_BRIEF_SOURCE, &self.cache, vec![]).await,
            Err(error) => Ok(unavailable_snapshot(TURN_BRIEF_SOURCE, error)),
        }
    }

    async fn resolve_state(&self, reference: &StateRef) -> Result<CanonicalBytes, SuppressionError> {
        self.cache.resolve_state_persistent(reference).await
    }

    async fn resolve_payload(&self, reference: &PayloadRef) -> Result<CanonicalBytes, SuppressionError> {
        self.cache.resolve_payload_persistent(reference).await
    }
}

/// Skill set adapter. A skill's identity is its name; title, description,
/// version, injection mode, trigger and body all change the revision because
/// they can change model behavior. File paths and mtimes never enter it.
pub struct SkillsSource {
    config: Arc<SystemPromptsConfig>,
    cache: SourceCache,
}

impl SkillsSource {
    pub fn new(config: Arc<SystemPromptsConfig>) -> Self {
        Self {
            config,
            cache: SourceCache::default(),
        }
    }
}

#[async_trait]
impl AssistantSource for SkillsSource {
    fn descriptor(&self) -> SourceDescriptor {
        descriptor(SKILLS_SOURCE)
    }

    async fn capture(&self, cx: &SourceContext) -> Result<SourceSnapshot, SuppressionError> {
        let workspace_dir = workspace_dir_for(&self.config, &cx.workspace_id);
        let mut current = Vec::new();
        for skill in load_skills_for(&workspace_dir) {
            let payload = json!({
                "name": skill.name,
                "title": skill.title,
                "description": skill.description,
                "version": skill.version,
                "inject": inject_mode(skill.inject),
                "trigger": skill.trigger,
                "body": skill.body,
            });
            let bytes = canonical_bytes(&payload)?;
            current.push((skill.name, bytes, WorkItemDisposition::Active));
        }
        capture_items(cx, SKILLS_SOURCE, &self.cache, current).await
    }

    async fn resolve_state(&self, reference: &StateRef) -> Result<CanonicalBytes, SuppressionError> {
        self.cache.resolve_state_persistent(reference).await
    }

    async fn resolve_payload(&self, reference: &PayloadRef) -> Result<CanonicalBytes, SuppressionError> {
        self.cache.resolve_payload_persistent(reference).await
    }
}

/// `SHORT_TERM.md`: still-relevant short-lived notes. Read directly from the
/// workspace; no second storage is introduced.
pub struct ShortTermSource {
    config: Arc<SystemPromptsConfig>,
    cache: SourceCache,
}

impl ShortTermSource {
    pub fn new(config: Arc<SystemPromptsConfig>) -> Self {
        Self {
            config,
            cache: SourceCache::default(),
        }
    }
}

#[async_trait]
impl AssistantSource for ShortTermSource {
    fn descriptor(&self) -> SourceDescriptor {
        descriptor(SHORT_TERM_SOURCE)
    }

    async fn capture(&self, cx: &SourceContext) -> Result<SourceSnapshot, SuppressionError> {
        let workspace_dir = workspace_dir_for(&self.config, &cx.workspace_id);
        match read_workspace_file(&workspace_dir, "SHORT_TERM.md") {
            Ok(Some(content)) if !content.trim().is_empty() => {
                let bytes = document_payload(&content)?;
                capture_items(
                    cx,
                    SHORT_TERM_SOURCE,
                    &self.cache,
                    vec![("document".to_string(), bytes, WorkItemDisposition::Active)],
                )
                .await
            }
            Ok(_) => capture_items(cx, SHORT_TERM_SOURCE, &self.cache, vec![]).await,
            Err(error) => Ok(unavailable_snapshot(SHORT_TERM_SOURCE, error)),
        }
    }

    async fn resolve_state(&self, reference: &StateRef) -> Result<CanonicalBytes, SuppressionError> {
        self.cache.resolve_state_persistent(reference).await
    }

    async fn resolve_payload(&self, reference: &PayloadRef) -> Result<CanonicalBytes, SuppressionError> {
        self.cache.resolve_payload_persistent(reference).await
    }
}

/// Long-term memory adapter. It reuses the prompt's exact selection logic, so
/// the revision describes the memory content the model actually receives.
/// Timestamps, counters and storage metadata are noise.
pub struct MemorySourceAdapter {
    config: Arc<SystemPromptsConfig>,
    memory_source: Option<Arc<dyn PromptMemorySource>>,
    cache: SourceCache,
}

impl MemorySourceAdapter {
    pub fn new(memory_source: Option<Arc<dyn PromptMemorySource>>, config: Arc<SystemPromptsConfig>) -> Self {
        Self {
            config,
            memory_source,
            cache: SourceCache::default(),
        }
    }
}

#[async_trait]
impl AssistantSource for MemorySourceAdapter {
    fn descriptor(&self) -> SourceDescriptor {
        descriptor(MEMORY_SOURCE)
    }

    async fn capture(&self, cx: &SourceContext) -> Result<SourceSnapshot, SuppressionError> {
        let Some(memory_source) = &self.memory_source else {
            return Ok(unavailable_snapshot(
                MEMORY_SOURCE,
                source_unavailable("long-term memory source is not configured"),
            ));
        };
        let workspace_dir = workspace_dir_for(&self.config, &cx.workspace_id);
        let agent_id = agent_id_from_session(&cx.session_id);
        match select_memory(
            memory_source.as_ref(),
            &workspace_dir,
            &cx.workspace_id,
            &agent_id,
            MEMORY_MAX_TOKENS,
        )
        .await
        {
            Ok(MemorySelection::Profile(content)) => {
                let bytes = document_payload(&content)?;
                capture_items(
                    cx,
                    MEMORY_SOURCE,
                    &self.cache,
                    vec![("profile".to_string(), bytes, WorkItemDisposition::Active)],
                )
                .await
            }
            Ok(MemorySelection::Entries(entries)) => {
                let mut current = Vec::with_capacity(entries.len());
                for entry in entries {
                    let payload = json!({
                        "zone": entry.zone.as_str(),
                        "content": entry.content,
                    });
                    let bytes = canonical_bytes(&payload)?;
                    current.push((entry.id, bytes, WorkItemDisposition::Active));
                }
                capture_items(cx, MEMORY_SOURCE, &self.cache, current).await
            }
            Err(_) => Ok(unavailable_snapshot(
                MEMORY_SOURCE,
                source_unavailable("load active memory entries"),
            )),
        }
    }

    async fn resolve_state(&self, reference: &StateRef) -> Result<CanonicalBytes, SuppressionError> {
        self.cache.resolve_state_persistent(reference).await
    }

    async fn resolve_payload(&self, reference: &PayloadRef) -> Result<CanonicalBytes, SuppressionError> {
        self.cache.resolve_payload_persistent(reference).await
    }
}

/// Build the registry with exactly the six initial Sources at version 1.
pub fn default_sources(
    config: Arc<SystemPromptsConfig>,
    db: Arc<Db>,
    memory_source: Option<Arc<dyn PromptMemorySource>>,
    snapshot_provider: Option<Arc<dyn WorkspaceSnapshotProvider>>,
) -> Result<SourceRegistry, SuppressionError> {
    let mut registry = SourceRegistry::new();
    let mut workspace = WorkspaceSnapshotSource::new(snapshot_provider);
    workspace.cache = SourceCache::persistent(db.clone());
    registry.register(Arc::new(workspace))?;
    let mut events = SessionEventsSource::new(db.clone());
    events.cache = SourceCache::persistent(db.clone());
    registry.register(Arc::new(events))?;
    let mut brief = TurnBriefSource::new(config.clone());
    brief.cache = SourceCache::persistent(db.clone());
    registry.register(Arc::new(brief))?;
    let mut skills = SkillsSource::new(config.clone());
    skills.cache = SourceCache::persistent(db.clone());
    registry.register(Arc::new(skills))?;
    let mut short_term = ShortTermSource::new(config.clone());
    short_term.cache = SourceCache::persistent(db.clone());
    registry.register(Arc::new(short_term))?;
    let mut memory = MemorySourceAdapter::new(memory_source, config);
    memory.cache = SourceCache::persistent(db);
    registry.register(Arc::new(memory))?;
    Ok(registry)
}

#[cfg(test)]
#[allow(clippy::module_inception)] // the test path must contain `suppression::registry`
mod registry {
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    use serde_json::json;
    use tempfile::TempDir;
    use tinyiothub_core::config::SystemPromptsConfig;
    use tinyiothub_core::memory::{AgentMemory, Confidence, MemorySource, MemoryZone};
    use tinyiothub_runtime::watch::live_state::{DeviceSnapshot, Health, WorkspaceSnapshot};
    use tinyiothub_storage::Db;

    use crate::domains::agent::prompt::PromptMemorySource;
    use crate::domains::agent::prompt::assistant::build_assistant_prompt_with_resolved_sources;
    use crate::domains::agent::session::context::TurnContext;
    use crate::domains::agent::session::events::session_event;
    use crate::domains::agent::session::suppression::canonical::{hash_payload, hash_state};
    use crate::domains::agent::session::suppression::contract::{
        PayloadRef, SourceDescriptor, SourceSnapshot, WorkItemDisposition,
    };

    use super::{
        AssistantSource, MemorySourceAdapter, SessionEventsSource, ShortTermSource, SnapshotProviderSlot, SourceCache,
        SourceContext, SourceMode, SourceRegistry, TurnBriefSource, WorkspaceSnapshotProvider, WorkspaceSnapshotSource,
        default_sources, load_session_events,
    };

    const WS: &str = "ws-registry";
    const ASSISTANT: &str = "__assistant__ws-registry";
    const SESSION: &str = "agent:ws-registry:__assistant__ws-registry/main";

    fn workspace_dir(root: &Path) -> std::path::PathBuf {
        root.join(WS)
    }

    fn config(root: &Path) -> Arc<SystemPromptsConfig> {
        Arc::new(SystemPromptsConfig {
            workspace_dir: root.display().to_string(),
            ..Default::default()
        })
    }

    fn context() -> SourceContext {
        SourceContext::new(WS, SESSION)
    }

    fn memory(id: &str, content: &str) -> AgentMemory {
        AgentMemory {
            id: id.into(),
            workspace_id: WS.into(),
            agent_id: ASSISTANT.into(),
            zone: MemoryZone::Work,
            content: content.into(),
            source: MemorySource::Reflection,
            confidence: Confidence::High,
            tags: vec![],
            pinned: false,
            supersedes: None,
            thing_id: None,
            snapshot_data: None,
            snapshot_time: None,
            effectiveness: 1.0,
            load_count: 0,
            reference_count: 0,
            created_at: "2026-11-14T00:00:00Z".into(),
            updated_at: "2026-11-14T00:00:00Z".into(),
        }
    }

    struct FakeMemorySource {
        entries: Mutex<Vec<AgentMemory>>,
    }

    #[async_trait::async_trait]
    impl PromptMemorySource for FakeMemorySource {
        async fn list_active(&self, _workspace_id: &str, _agent_id: &str) -> anyhow::Result<Vec<AgentMemory>> {
            Ok(self.entries.lock().unwrap().clone())
        }

        async fn record_load(&self, _id: &str) -> anyhow::Result<()> {
            Ok(())
        }
    }

    fn device(thing_id: &str, name: &str) -> DeviceSnapshot {
        DeviceSnapshot {
            thing_id: thing_id.into(),
            name: name.into(),
            health: Health::Ok,
            online: true,
            zone: "zone-a".into(),
            values: vec![("temperature".into(), "21.5".into())],
            properties: vec!["temperature".into()],
        }
    }

    fn snapshot(devices: Vec<DeviceSnapshot>, total: u32) -> WorkspaceSnapshot {
        WorkspaceSnapshot { v: 1, devices, total }
    }

    struct SixSourceFixture {
        root: TempDir,
        config: Arc<SystemPromptsConfig>,
        db: Arc<Db>,
        registry: SourceRegistry,
        memory: Arc<FakeMemorySource>,
        snapshot: Arc<SnapshotProviderSlot>,
        session_key: String,
    }

    impl SixSourceFixture {
        async fn setup() -> Self {
            let root = tempfile::tempdir().unwrap();
            let workspace = workspace_dir(root.path());
            std::fs::create_dir_all(workspace.join("skills")).unwrap();
            std::fs::write(workspace.join("TURN_BRIEF.md"), "turn-brief-marker").unwrap();
            std::fs::write(workspace.join("SHORT_TERM.md"), "short-term-marker").unwrap();
            std::fs::write(
                workspace.join("skills/inspection.md"),
                "---\nname: inspection\ninject: index\n---\n\n# Skill Title Marker\n\n检查设备。",
            )
            .unwrap();

            // File-backed platform DB plus a per-test workspace-store root.
            // The default root is `<repo>/data/agents`, so parallel fixtures
            // would race the shared workspace migrations and leak events.
            let options = sqlx::sqlite::SqliteConnectOptions::new()
                .filename(root.path().join("platform.db"))
                .create_if_missing(true);
            let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
            tinyiothub_storage::test_helpers::run_all_migrations(&pool)
                .await
                .unwrap();
            let db = Arc::new(Db::new(pool).with_workspace_store_root(root.path().join("workspace-stores")));
            tinyiothub_storage::seed::seed_system(&db).await.unwrap();
            sqlx::query("INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) VALUES (?, 'workspace', 'tenant-default-001', 'now', 'now')")
                .bind(WS)
                .execute(db.pool())
                .await
                .unwrap();
            db.create_workspace_store(WS).await.unwrap();

            let session_key = SESSION.to_string();
            let context = TurnContext::new(WS.into(), ASSISTANT.into(), session_key.clone(), "run".into(), 0);
            for (event_type, payload) in [
                (
                    "user/message",
                    json!({"messageId": "user-1", "role": "user", "text": "cursor-owned user text"}),
                ),
                (
                    "assistant/message",
                    json!({"messageId": "assistant-1", "role": "assistant", "text": "assistant output"}),
                ),
                ("turn/start", json!({"turnId": "turn-1"})),
                ("step.text", json!({"turnId": "turn-1", "text": "streamed step"})),
                (
                    "tool/call",
                    json!({"turnId": "turn-1", "tool": "read_device", "args": {}}),
                ),
                ("turn/end", json!({"turnId": "turn-1", "outcome": "completed"})),
                (
                    "thing.event",
                    json!({"marker": "session-event-marker", "thingId": "device-1", "temperature": 21.5}),
                ),
            ] {
                db.append_session_event(&session_event(&context, event_type, payload))
                    .await
                    .unwrap();
            }

            let memory = Arc::new(FakeMemorySource {
                entries: Mutex::new(vec![memory("memory-1", "memory-marker")]),
            });
            let snapshot = Arc::new(SnapshotProviderSlot::with(snapshot(
                vec![device("device-1", "device-marker")],
                1,
            )));
            let config = config(root.path());
            let registry = default_sources(
                config.clone(),
                db.clone(),
                Some(memory.clone() as Arc<dyn PromptMemorySource>),
                Some(snapshot.clone() as Arc<dyn WorkspaceSnapshotProvider>),
            )
            .unwrap();
            Self {
                root,
                config,
                db,
                registry,
                memory,
                snapshot,
                session_key,
            }
        }

        fn workspace_dir(&self) -> std::path::PathBuf {
            workspace_dir(self.root.path())
        }

        async fn change(&self, source_id: &str, value: &str) {
            match source_id {
                "workspace.snapshot" => {
                    self.snapshot.set(snapshot(vec![device("device-1", value)], 1));
                }
                "session.events" => {
                    let context =
                        TurnContext::new(WS.into(), ASSISTANT.into(), self.session_key.clone(), "run".into(), 0);
                    self.db
                        .append_session_event(&session_event(
                            &context,
                            "thing.event",
                            json!({ "marker": value, "thingId": "device-1" }),
                        ))
                        .await
                        .unwrap();
                }
                "turn.brief" => {
                    std::fs::write(self.workspace_dir().join("TURN_BRIEF.md"), value).unwrap();
                }
                "skills" => {
                    std::fs::write(
                        self.workspace_dir().join("skills/extra.md"),
                        format!("---\nname: extra\ninject: index\n---\n\n# Extra Marker\n\n{value}"),
                    )
                    .unwrap();
                }
                "short_term" => {
                    std::fs::write(self.workspace_dir().join("SHORT_TERM.md"), value).unwrap();
                }
                "memory" => {
                    *self.memory.entries.lock().unwrap() = vec![memory("memory-1", value)];
                }
                other => panic!("unknown source {other}"),
            }
        }

        async fn run_full_turn(&self) -> FullTurnRequest {
            // The full-input path still captures every Source (contract check);
            // the rendered request only needs the prompt text.
            let snapshots = self
                .registry
                .capture_all(&context())
                .await
                .expect("all six sources must capture and validate");
            let resolved = self
                .registry
                .resolve_all_bounded(
                    &snapshots,
                    std::time::Duration::from_secs(2),
                    &tokio_util::sync::CancellationToken::new(),
                )
                .await
                .expect("all captured refs must resolve");
            let package = build_assistant_prompt_with_resolved_sources(
                &self.config,
                WS,
                ASSISTANT,
                "turn-input".into(),
                &resolved,
            )
            .await
            .expect("full prompt");
            FullTurnRequest {
                text: format!("{}\n{}", package.system_prompt, package.turn_input),
            }
        }
    }

    struct FullTurnRequest {
        text: String,
    }

    impl FullTurnRequest {
        fn contains_source(&self, source_id: &str) -> bool {
            let marker = match source_id {
                "workspace.snapshot" => "<workspace_snapshot>",
                "session.events" => "<session_events>",
                "turn.brief" => "## Turn Brief",
                "skills" => "## 技能（Skills）",
                "short_term" => "## Short Term",
                "memory" => "## Dynamic Memory",
                other => panic!("unknown source {other}"),
            };
            self.text.contains(marker)
        }
    }

    #[tokio::test]
    async fn changing_one_source_keeps_all_sources_in_full_input() {
        let fixture = SixSourceFixture::setup().await;
        fixture.change("turn.brief", "new revision").await;
        let request = fixture.run_full_turn().await;
        for source in fixture.registry.source_ids() {
            assert!(request.contains_source(source), "missing {source}");
        }
    }

    #[tokio::test]
    async fn six_adapters_pass_the_same_contract() {
        let fixture = SixSourceFixture::setup().await;
        assert_eq!(fixture.registry.source_ids().len(), 6);
        fixture
            .registry
            .assert_source_contract(&context())
            .await
            .expect("all six adapters must satisfy the shared conformance contract");
    }

    #[tokio::test]
    async fn nil_file_sources_are_valid_empty_with_stable_refs() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(workspace_dir(root.path())).unwrap();
        let config = config(root.path());
        for source in [
            Box::new(TurnBriefSource::new(config.clone())) as Box<dyn AssistantSource>,
            Box::new(ShortTermSource::new(config)) as Box<dyn AssistantSource>,
        ] {
            let snap = source.capture(&context()).await.unwrap();
            assert!(snap.available);
            assert!(snap.empty);
            assert!(snap.work_items.is_empty());
            assert!(snap.state_ref.is_some());
            let again = source.capture(&context()).await.unwrap();
            assert_eq!(snap.revision, again.revision);
            assert_eq!(snap.state_hash, again.state_hash);
        }
    }

    #[tokio::test]
    async fn unavailable_source_has_safe_error_and_no_refs() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(workspace_dir(root.path()).join("TURN_BRIEF.md")).unwrap();
        let source = TurnBriefSource::new(config(root.path()));
        let snap = source.capture(&context()).await.unwrap();
        assert!(!snap.available);
        assert!(snap.state_ref.is_none());
        assert!(snap.work_items.is_empty());
        assert_eq!(snap.error.unwrap().code, "source_unavailable");
    }

    #[tokio::test]
    async fn noise_does_not_change_revision() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(workspace_dir(root.path())).unwrap();
        let path = workspace_dir(root.path()).join("TURN_BRIEF.md");
        std::fs::write(&path, "stable-content").unwrap();
        let source = TurnBriefSource::new(config(root.path()));
        let first = source.capture(&context()).await.unwrap();
        // Same semantic content: mtime and filesystem metadata are noise.
        std::fs::write(&path, "stable-content").unwrap();
        let second = source.capture(&context()).await.unwrap();
        assert_eq!(first.revision, second.revision);
        assert_eq!(first.state_hash, second.state_hash);
    }

    #[tokio::test]
    async fn direct_delete_keeps_closed_tombstone_with_last_payload() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(workspace_dir(root.path())).unwrap();
        let path = workspace_dir(root.path()).join("SHORT_TERM.md");
        std::fs::write(&path, "last-known-payload").unwrap();
        let source = ShortTermSource::new(config(root.path()));
        let first = source.capture(&context()).await.unwrap();
        assert_eq!(first.work_items.len(), 1);

        std::fs::remove_file(&path).unwrap();
        let second = source.capture(&context()).await.unwrap();
        assert_ne!(first.revision, second.revision);
        let tombstone = second
            .work_items
            .iter()
            .find(|item| item.work_key == "document")
            .expect("closed tombstone");
        assert_eq!(tombstone.disposition, WorkItemDisposition::Closed);

        let payload = source.resolve_payload(&tombstone.payload_ref).await.unwrap();
        assert!(String::from_utf8_lossy(payload.as_bytes()).contains("last-known-payload"));
        assert_eq!(payload.hash(), tombstone.payload_hash);
    }

    #[tokio::test]
    async fn missing_last_payload_is_reported_not_faked_as_empty() {
        let cache = SourceCache::default();
        cache.seed_missing("gone");
        let error = cache.merge(vec![]).unwrap_err();
        assert_eq!(error.code, "work_item_payload_missing");
    }

    #[test]
    fn duplicate_work_keys_are_rejected_not_collapsed() {
        let cache = SourceCache::default();
        let error = cache
            .merge(vec![
                ("same".to_string(), b"first".to_vec(), WorkItemDisposition::Active),
                ("same".to_string(), b"second".to_vec(), WorkItemDisposition::Active),
            ])
            .unwrap_err();
        assert_eq!(error.code, "duplicate_work_key");

        // Canonically equivalent keys are the same identity as well.
        let cache = SourceCache::default();
        let error = cache
            .merge(vec![
                ("é".to_string(), b"first".to_vec(), WorkItemDisposition::Active),
                ("e\u{0301}".to_string(), b"second".to_vec(), WorkItemDisposition::Active),
            ])
            .unwrap_err();
        assert_eq!(error.code, "duplicate_work_key");
    }

    struct DuplicateWorkKeySource {
        cache: SourceCache,
    }

    #[async_trait::async_trait]
    impl AssistantSource for DuplicateWorkKeySource {
        fn descriptor(&self) -> SourceDescriptor {
            SourceDescriptor {
                source_id: "test.duplicate".into(),
                source_version: 1,
            }
        }

        async fn capture(&self, cx: &SourceContext) -> Result<SourceSnapshot, super::SuppressionError> {
            super::capture_items(
                cx,
                "test.duplicate",
                &self.cache,
                vec![
                    ("same".into(), b"first".to_vec(), WorkItemDisposition::Active),
                    ("same".into(), b"second".to_vec(), WorkItemDisposition::Active),
                ],
            )
            .await
        }

        async fn resolve_state(
            &self,
            _reference: &super::StateRef,
        ) -> Result<super::CanonicalBytes, super::SuppressionError> {
            Err(super::SuppressionError::new("source_ref_mismatch", "unused", "unused"))
        }

        async fn resolve_payload(
            &self,
            _reference: &PayloadRef,
        ) -> Result<super::CanonicalBytes, super::SuppressionError> {
            Err(super::SuppressionError::new(
                "work_item_payload_missing",
                "unused",
                "unused",
            ))
        }
    }

    #[tokio::test]
    async fn capture_rejects_duplicate_work_keys() {
        let source = DuplicateWorkKeySource {
            cache: SourceCache::default(),
        };
        let error = source.capture(&context()).await.unwrap_err();
        assert_eq!(error.code, "duplicate_work_key");
    }

    #[tokio::test]
    async fn stale_refs_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(workspace_dir(root.path())).unwrap();
        std::fs::write(workspace_dir(root.path()).join("TURN_BRIEF.md"), "brief").unwrap();
        let source = TurnBriefSource::new(config(root.path()));
        let snap = source.capture(&context()).await.unwrap();

        let mut stale_state = snap.state_ref.clone().unwrap();
        stale_state.revision = "stale-revision".into();
        let error = source.resolve_state(&stale_state).await.unwrap_err();
        assert_eq!(error.code, "source_ref_mismatch");

        let mut stale_payload = snap.work_items[0].payload_ref.clone();
        stale_payload.revision = "stale-revision".into();
        let error = source.resolve_payload(&stale_payload).await.unwrap_err();
        assert_eq!(error.code, "source_ref_mismatch");

        let mut unknown_payload = snap.work_items[0].payload_ref.clone();
        unknown_payload.payload_hash = hash_payload(b"never-seen");
        unknown_payload.revision = unknown_payload.payload_hash.clone();
        let error = source.resolve_payload(&unknown_payload).await.unwrap_err();
        assert_eq!(error.code, "work_item_payload_missing");
    }

    #[tokio::test]
    async fn duplicate_source_ids_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        let config = config(root.path());
        let mut registry = SourceRegistry::new();
        registry
            .register(Arc::new(TurnBriefSource::new(config.clone())))
            .unwrap();
        let error = registry.register(Arc::new(TurnBriefSource::new(config))).unwrap_err();
        assert_eq!(error.code, "duplicate_source_id");
    }

    struct VersionTwoSource;

    #[async_trait::async_trait]
    impl AssistantSource for VersionTwoSource {
        fn descriptor(&self) -> SourceDescriptor {
            SourceDescriptor {
                source_id: "test.source".into(),
                source_version: 2,
            }
        }

        async fn capture(&self, _cx: &SourceContext) -> Result<SourceSnapshot, super::SuppressionError> {
            let cache = SourceCache::default();
            Ok(cache.snapshot("test.source", cache.merge(vec![]).unwrap()).unwrap())
        }

        async fn resolve_state(
            &self,
            _reference: &super::StateRef,
        ) -> Result<super::CanonicalBytes, super::SuppressionError> {
            Err(super::SuppressionError::new("source_ref_mismatch", "unused", "unused"))
        }

        async fn resolve_payload(
            &self,
            _reference: &PayloadRef,
        ) -> Result<super::CanonicalBytes, super::SuppressionError> {
            Err(super::SuppressionError::new(
                "work_item_payload_missing",
                "unused",
                "unused",
            ))
        }
    }

    #[tokio::test]
    async fn descriptor_mismatch_is_rejected() {
        let mut registry = SourceRegistry::new();
        registry.register(Arc::new(VersionTwoSource)).unwrap();
        let error = registry.capture_all(&context()).await.unwrap_err();
        assert_eq!(error.code, "source_descriptor_mismatch");
    }

    #[tokio::test]
    async fn session_events_exclude_self_feedback_and_cursor_user_messages() {
        let fixture = SixSourceFixture::setup().await;
        let capture = load_session_events(&fixture.db, WS, &fixture.session_key)
            .await
            .unwrap();
        assert_eq!(capture.events.len(), 1);
        assert_eq!(capture.events[0].event_type, "thing.event");
        assert!(capture.control_cursor > 0);

        let source = SessionEventsSource::new(fixture.db.clone());
        let first = source.capture(&context()).await.unwrap();
        assert_eq!(first.work_items.len(), 1);

        // More Harness/assistant noise must not feed back into the source.
        let noise = TurnContext::new(
            WS.into(),
            ASSISTANT.into(),
            fixture.session_key.clone(),
            "run".into(),
            0,
        );
        for (event_type, payload) in [
            ("turn/start", json!({"turnId": "turn-2"})),
            (
                "assistant/message",
                json!({"messageId": "assistant-2", "role": "assistant", "text": "another report"}),
            ),
            ("assistant.suppression.alert", json!({"code": "source_unavailable"})),
            (
                "subagent/started",
                json!({"subagentId": "sub-2", "task": "inspect pump"}),
            ),
            (
                "subagent/completed",
                json!({"subagentId": "sub-2", "summary": "model-written subagent summary"}),
            ),
            ("skill/loaded", json!({"name": "inspection", "body": "skill body"})),
        ] {
            fixture
                .db
                .append_session_event(&session_event(&noise, event_type, payload))
                .await
                .unwrap();
        }
        let second = source.capture(&context()).await.unwrap();
        assert_eq!(first.revision, second.revision);

        // A new external semantic event still changes the revision.
        fixture.change("session.events", "new-external-event").await;
        let third = source.capture(&context()).await.unwrap();
        assert_ne!(second.revision, third.revision);
        assert_eq!(third.work_items.len(), 2);
    }

    #[tokio::test]
    async fn final_review_question_approval_todo_and_task_changes_enter_full_input() {
        let fixture = SixSourceFixture::setup().await;
        let source = SessionEventsSource::new(fixture.db.clone());
        let noise = TurnContext::new(
            WS.into(),
            ASSISTANT.into(),
            fixture.session_key.clone(),
            "run-external-work".into(),
            0,
        );
        let mut previous = source.capture(&context()).await.unwrap();
        for (event_type, marker) in [
            ("question/answered", "answer-marker"),
            ("approval/resolved", "approval-marker"),
            ("todo/write", "todo-marker"),
            ("task.created", "task-marker"),
        ] {
            fixture
                .db
                .append_session_event(&session_event(
                    &noise,
                    event_type,
                    json!({"eventType": event_type, "text": marker}),
                ))
                .await
                .unwrap();
            let current = source.capture(&context()).await.unwrap();
            assert_ne!(
                previous.revision, current.revision,
                "{event_type} must force a full Turn"
            );
            previous = current;
        }

        let request = fixture.run_full_turn().await;
        for marker in ["answer-marker", "approval-marker", "todo-marker", "task-marker"] {
            assert!(request.text.contains(marker), "full input omitted {marker}");
        }
    }

    #[tokio::test]
    async fn each_source_change_only_affects_its_own_revision() {
        let fixture = SixSourceFixture::setup().await;
        let before = fixture.registry.capture_all(&context()).await.unwrap();
        fixture.change("turn.brief", "new revision").await;
        let after = fixture.registry.capture_all(&context()).await.unwrap();

        for (before, after) in before.iter().zip(after.iter()) {
            if before.source_id == "turn.brief" {
                assert_ne!(before.revision, after.revision);
            } else {
                assert_eq!(
                    before.revision, after.revision,
                    "{} changed unexpectedly",
                    before.source_id
                );
            }
        }
    }

    #[tokio::test]
    async fn final_review_deleted_payload_survives_registry_rebuild_and_enters_input() {
        let mut fixture = SixSourceFixture::setup().await;
        fixture.registry.capture_all(&context()).await.unwrap();
        fixture.registry = default_sources(
            fixture.config.clone(),
            fixture.db.clone(),
            Some(fixture.memory.clone() as Arc<dyn PromptMemorySource>),
            Some(fixture.snapshot.clone() as Arc<dyn WorkspaceSnapshotProvider>),
        )
        .unwrap();
        std::fs::remove_file(fixture.workspace_dir().join("TURN_BRIEF.md")).unwrap();

        let snapshots = fixture.registry.capture_all(&context()).await.unwrap();
        let brief = snapshots
            .iter()
            .find(|source| source.source_id == "turn.brief")
            .unwrap();
        assert_eq!(brief.work_items.len(), 1);
        assert_eq!(brief.work_items[0].disposition, WorkItemDisposition::Closed);
        let request = fixture.run_full_turn().await;
        assert!(request.text.contains("turn-brief-marker"));
    }

    #[tokio::test]
    async fn full_input_injects_each_source_content_exactly_once() {
        let fixture = SixSourceFixture::setup().await;
        let request = fixture.run_full_turn().await;
        for (marker, label) in [
            ("turn-brief-marker", "turn brief"),
            ("short-term-marker", "short term"),
            ("memory-marker", "memory"),
            ("device-marker", "workspace snapshot"),
            ("Skill Title Marker", "skills"),
            ("session-event-marker", "session events"),
        ] {
            assert_eq!(
                request.text.matches(marker).count(),
                1,
                "{label} must enter the full input exactly once"
            );
        }
    }

    #[tokio::test]
    async fn descriptor_mismatch_and_duplicate_work_keys_are_validated() {
        let fixture = SixSourceFixture::setup().await;
        let snapshots = fixture.registry.capture_all(&context()).await.unwrap();
        for snapshot in snapshots {
            let descriptor = SourceDescriptor {
                source_id: snapshot.source_id.clone(),
                source_version: snapshot.source_version,
            };
            let mut wrong = snapshot.clone();
            wrong.source_version = descriptor.source_version + 1;
            assert!(wrong.validate(&descriptor).is_err());

            if !snapshot.work_items.is_empty() {
                let mut duplicate = snapshot.clone();
                duplicate.work_items.push(duplicate.work_items[0].clone());
                duplicate.empty = false;
                let error = duplicate.validate(&descriptor).unwrap_err();
                assert_eq!(error.code, "duplicate_work_key");
            }
        }
    }

    #[tokio::test]
    async fn revision_hash_conflict_is_rejected() {
        let fixture = SixSourceFixture::setup().await;
        let snapshots = fixture.registry.capture_all(&context()).await.unwrap();
        for snapshot in snapshots.into_iter().filter(|snapshot| snapshot.available) {
            let descriptor = SourceDescriptor {
                source_id: snapshot.source_id.clone(),
                source_version: snapshot.source_version,
            };
            let mut conflict = snapshot.clone();
            conflict.state_hash = hash_state(b"other-state");
            let error = conflict.validate(&descriptor).unwrap_err();
            assert_eq!(error.code, "source_ref_mismatch");
        }
    }

    #[tokio::test]
    async fn valid_empty_sources_are_not_source_unavailable() {
        let fixture = SixSourceFixture::setup().await;
        let cx = SourceContext {
            workspace_id: WS.into(),
            session_id: SESSION.into(),
            observed_at: chrono::Utc::now(),
            mode: SourceMode::Shadow,
        };
        let memory = MemorySourceAdapter::new(None, fixture.config.clone());
        let snap = memory.capture(&cx).await.unwrap();
        assert!(
            !snap.available,
            "an unwired memory source must not fake a healthy empty set"
        );
        assert_eq!(snap.error.unwrap().code, "source_unavailable");

        let provider: Arc<dyn WorkspaceSnapshotProvider> = Arc::new(SnapshotProviderSlot::with(snapshot(vec![], 0)));
        let empty_snapshot = WorkspaceSnapshotSource::new(Some(provider));
        let snap = empty_snapshot.capture(&cx).await.unwrap();
        assert!(snap.available && snap.empty);
        assert!(snap.state_ref.is_some());
        assert!(fixture.registry.source_ids().contains(&"workspace.snapshot"));
    }
}

/// Task 9: per-Source capture bounds and large-state hashing evidence.
#[cfg(test)]
mod suppression_overload {
    use std::sync::Arc;
    use std::time::{Duration as StdDuration, Instant};

    use tokio_util::sync::CancellationToken;

    use crate::domains::agent::session::suppression::canonical::{canonical_bytes, hash_payload};
    use crate::domains::agent::session::suppression::contract::{
        CanonicalBytes, PayloadRef, SourceDescriptor, SourceSnapshot, StateRef, WorkItemDisposition,
    };
    use crate::domains::agent::session::suppression::error::SuppressionError;

    use super::{AssistantSource, SOURCE_VERSION, SourceCache, SourceContext, SourceMode, SourceRegistry};

    const WS: &str = "ws-registry-overload";
    const SESSION: &str = "agent:ws-registry-overload:__assistant__ws-registry-overload/main";

    fn context() -> SourceContext {
        SourceContext {
            workspace_id: WS.to_string(),
            session_id: SESSION.to_string(),
            observed_at: chrono::Utc::now(),
            mode: SourceMode::Enforce,
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

    #[tokio::test]
    async fn suppression_overload_hanging_source_capture_is_bounded() {
        let mut registry = SourceRegistry::new();
        registry
            .register(Arc::new(HangingSource))
            .expect("register hanging source");
        let started = Instant::now();
        let error = registry
            .capture_all_bounded(
                &context(),
                StdDuration::from_millis(25),
                StdDuration::from_millis(25),
                &CancellationToken::new(),
            )
            .await
            .expect_err("a hanging Source must hit the per-Source deadline");
        assert_eq!(error.code, "preflight_timeout");
        assert!(!error.recovery.baseline_advanced);
        assert!(started.elapsed() < StdDuration::from_secs(1));
    }

    #[tokio::test]
    async fn suppression_overload_cancellation_stops_capture_without_a_result() {
        let mut registry = SourceRegistry::new();
        registry
            .register(Arc::new(HangingSource))
            .expect("register hanging source");
        let cancel = CancellationToken::new();
        cancel.cancel();
        let started = Instant::now();
        let error = registry
            .capture_all_bounded(
                &context(),
                StdDuration::from_secs(30),
                StdDuration::from_secs(30),
                &cancel,
            )
            .await
            .expect_err("a cancelled preflight must abort without waiting for the deadline");
        assert_eq!(error.code, "preflight_timeout");
        assert!(started.elapsed() < StdDuration::from_secs(1));
    }

    /// The canonical hash is computed over one retained canonical buffer; this
    /// exercises a 10x item payload and proves deterministic revisions without
    /// a second full copy or quadratic re-hash.
    #[test]
    fn suppression_overload_large_state_hashing_is_deterministic() {
        let cache = SourceCache::default();
        let current: Vec<(String, Vec<u8>, WorkItemDisposition)> = (0..10_000)
            .map(|index| {
                let payload = canonical_bytes(&serde_json::json!({
                    "thingId": format!("device-{index}"),
                    "health": "ok",
                    "values": { "temperature": format!("{index}") },
                }))
                .expect("canonical payload");
                (format!("device-{index}"), payload, WorkItemDisposition::Active)
            })
            .collect();
        let expected_payload_hash = hash_payload(&current[0].1);

        let started = Instant::now();
        let first = cache.merge(current.clone()).expect("first merge");
        assert_eq!(first.len(), 10_000);
        assert_eq!(first[0].work_key, "device-0");
        assert_eq!(first[0].payload_hash, expected_payload_hash);
        let first_state = cache
            .snapshot("workspace.snapshot", first.clone())
            .expect("first snapshot");
        assert!(!first_state.empty);

        let second = cache.merge(current).expect("second merge");
        assert_eq!(second, first, "unchanged content keeps identical revisions");
        let second_state = cache.snapshot("workspace.snapshot", second).expect("second snapshot");
        assert_eq!(second_state.state_hash, first_state.state_hash);
        assert_eq!(second_state.revision, first_state.revision);
        assert!(
            started.elapsed() < StdDuration::from_secs(10),
            "10k-item hashing must stay linear and bounded"
        );
    }
}
