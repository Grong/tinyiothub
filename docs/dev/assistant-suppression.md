# Assistant Suppression — Source Author and API Guide

This guide is the authoritative developer reference for the Assistant
suppression gate. It covers the Source adapter contract, the stable error
catalog, the HTTP API, the version/compatibility matrix, recovery and rollback,
and an executable loopback quickstart.

The operational runbook is `docs/runbooks/assistant-suppression.md`. This guide
does not grant any production bypass: `off` is always available, and synthetic
gate evidence is accepted only by the loopback fixture's server configuration.

## 1. Contract at a glance

A Source adapter implements `AssistantSource` (`apps/cloud/src/domains/agent/session/suppression/registry.rs`):

```rust
#[async_trait]
pub trait AssistantSource: Send + Sync {
    fn descriptor(&self) -> SourceDescriptor;
    async fn capture(&self, cx: &SourceContext) -> Result<SourceSnapshot, SuppressionError>;
    async fn resolve_state(&self, reference: &StateRef) -> Result<CanonicalBytes, SuppressionError>;
    async fn resolve_payload(&self, reference: &PayloadRef) -> Result<CanonicalBytes, SuppressionError>;
}
```

Rules shared by every adapter:

- `capture` is deterministic while the semantic content is unchanged: the same
  revision and hashes for repeated calls.
- A revision is 1..=256 UTF-8 bytes.
- A hash is `sha256:` followed by 64 lowercase hex characters.
- `state_ref` and each `payload_ref` must match the snapshot/work-item
  revision and hash; stale refs must be rejected by the resolvers.
- `work_key` values are unique and NFC-normalized uniqueness is enforced by the
  registry.
- The registry owns the Source list. Register once; `sourceId` and
  `sourceVersion` come only from `descriptor()`.
- Sources never see credentials, provider handles, payload bytes in logs, or
  business rules outside their own adapter.

Run the shared conformance check at startup (or in tests):

```rust
registry.assert_source_contract(&SourceContext::new(workspace_id, &session_key)).await?;
```

It checks deterministic capture, descriptor/snapshot shape, resolvable
immutable refs, rejection of stale refs, duplicate workKeys and descriptor
mismatches.

## 2. Complete Source examples

The two examples below are the exact Sources compiled by
`apps/cloud/examples/suppression_quickstart.rs`
(`cargo build -p tinyiothub-cloud --example suppression_quickstart`).

### 2.1 StaticSource — unchanged content

```rust
use async_trait::async_trait;
use serde_json::{Value, json};

use tinyiothub_cloud::domains::agent::session::suppression::canonical::canonical_bytes;
use tinyiothub_cloud::domains::agent::session::suppression::contract::{
    CanonicalBytes, PayloadRef, SourceDescriptor, SourceSnapshot, StateRef, WorkItem,
    WorkItemDisposition,
};
use tinyiothub_cloud::domains::agent::session::suppression::error::{ErrorCode, SuppressionError};
use tinyiothub_cloud::domains::agent::session::suppression::registry::{
    AssistantSource, SOURCE_VERSION, SourceContext,
};

const STATIC_SOURCE: &str = "demo.static";
const STATIC_REVISION: &str = "static-r1";

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

#[async_trait]
impl AssistantSource for StaticSource {
    fn descriptor(&self) -> SourceDescriptor {
        self.descriptor.clone()
    }

    async fn capture(&self, _cx: &SourceContext) -> Result<SourceSnapshot, SuppressionError> {
        let state = CanonicalBytes::state(canonical_bytes(&Self::state_value())?, STATIC_REVISION)?;
        let payload =
            CanonicalBytes::payload(canonical_bytes(&Self::payload_value())?, "static-item-1-r1")?;
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
        let canonical =
            CanonicalBytes::state(canonical_bytes(&Self::state_value())?, STATIC_REVISION)?;
        if reference.id != format!("state:{STATIC_SOURCE}")
            || reference.revision != canonical.revision()
            || reference.state_hash != canonical.hash()
        {
            return Err(SuppressionError::from_code(ErrorCode::SourceRefMismatch));
        }
        Ok(canonical)
    }

    async fn resolve_payload(&self, reference: &PayloadRef) -> Result<CanonicalBytes, SuppressionError> {
        let canonical = CanonicalBytes::payload(
            canonical_bytes(&Self::payload_value())?,
            "static-item-1-r1",
        )?;
        if reference.id != format!("payload:{STATIC_SOURCE}:static-item-1")
            || reference.revision != canonical.revision()
            || reference.payload_hash != canonical.hash()
        {
            return Err(SuppressionError::from_code(ErrorCode::SourceRefMismatch));
        }
        Ok(canonical)
    }
}
```

### 2.2 ChangingSource — mutate one work item and its revision

`ChangingSource` changes only the affected item's revision and payload. The
next preflight vetoes the skip with `source_changed`, so the full Turn still
receives every registered Source.

```rust
use std::sync::Mutex;

use async_trait::async_trait;
use serde_json::{Value, json};

use tinyiothub_cloud::domains::agent::session::suppression::canonical::canonical_bytes;
use tinyiothub_cloud::domains::agent::session::suppression::contract::{
    CanonicalBytes, PayloadRef, SourceDescriptor, SourceSnapshot, StateRef, WorkItem,
    WorkItemDisposition,
};
use tinyiothub_cloud::domains::agent::session::suppression::error::{ErrorCode, SuppressionError};
use tinyiothub_cloud::domains::agent::session::suppression::registry::{
    AssistantSource, SOURCE_VERSION, SourceContext,
};

const CHANGING_SOURCE: &str = "demo.changing";
const CHANGING_ITEM: &str = "item-1";

#[derive(Clone, Debug)]
struct ChangingItem {
    revision: String,
    text: String,
}

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

    pub fn set(&self, revision: impl Into<String>, text: impl Into<String>) {
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
        let state =
            CanonicalBytes::state(canonical_bytes(&Self::state_value(&item))?, revision.clone())?;
        let payload =
            CanonicalBytes::payload(canonical_bytes(&Self::payload_value(&item))?, revision.clone())?;
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
        let canonical = CanonicalBytes::state(
            canonical_bytes(&Self::state_value(&item))?,
            Self::revision(&item),
        )?;
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
        let canonical = CanonicalBytes::payload(
            canonical_bytes(&Self::payload_value(&item))?,
            Self::revision(&item),
        )?;
        if reference.id != format!("payload:{CHANGING_SOURCE}:changing-item-1")
            || reference.revision != canonical.revision()
            || reference.payload_hash != canonical.hash()
        {
            return Err(SuppressionError::from_code(ErrorCode::SourceRefMismatch));
        }
        Ok(canonical)
    }
}
```

## 3. Error catalog

Every new Harness failure carries the stable snake_case code below, a safe
`what`/`fix`, `recovery {action, maxAttempts, sideEffects, baselineAdvanced,
reconcile}`, `codeAnchor`, allowlisted `context`, `trace {traceId, localPath}`,
`redaction` and `channel`. Payloads, memory contents, ref internals and
credentials never leave the backend (`REDACTION_NOTE`).

Public async failures use `turn.completed.error` plus the safe `errorMessage`;
alerts use the `assistant.suppression.alert` Session Events type. Session
Events excludes `assistant.*` from semantic work, so an alert cannot re-trigger
a wake.

The default recovery shape used by the contract/validation codes is:
`action=fix_then_reconcile`, `maxAttempts=0`, `sideEffects=none`,
`baselineAdvanced=false`, `reconcile=not_required`, `channel=event + report`.

<a id="source_unavailable"></a>
### source_unavailable
- what: a required suppression Source is unavailable.
- fix: locate the Source via preflight/report, fix its dependency, then `POST reconcile`.
- recovery: fix_then_reconcile, 0 attempts, no side effects, baseline not advanced, reconcile=required_before_next_skip, channel=event + report.

<a id="source_missing"></a>
### source_missing
- what: a required suppression Source is no longer registered.
- fix: re-register the Source adapter and `POST reconcile`.
- recovery: fix_then_reconcile, 0 attempts, no side effects, baseline not advanced, reconcile=required_before_next_skip, channel=event + report.

<a id="work_item_payload_missing"></a>
### work_item_payload_missing
- what: a closed work item has no retained payload.
- fix: restore the closed tombstone payload before a full Turn; switch the workspace off if it cannot be restored.
- recovery: fix_then_reconcile, 0 attempts, no side effects, baseline not advanced, reconcile=required_before_next_skip, channel=event + report.

<a id="source_revision_conflict"></a>
### source_revision_conflict
- what: a Source reported a revision whose state hash conflicts.
- fix: run the Source conformance test, fix revision generation, then `POST reconcile`.
- recovery: fix_then_reconcile, 0 attempts, no side effects, baseline not advanced, reconcile=required_before_next_skip, channel=event + alert.

<a id="source_ref_mismatch"></a>
### source_ref_mismatch
- what: an immutable reference does not match the captured content.
- fix: rebuild the snapshot once; if it still fails, restore the immutable ref and reconcile without advancing the baseline.
- recovery: fix_then_reconcile, max 1 attempt, no side effects, baseline not advanced, reconcile=required_before_next_skip, channel=event + report.

<a id="baseline_corrupt"></a>
### baseline_corrupt
- what: the stored baseline cannot be parsed or bounded.
- fix: discard the baseline and run a full Turn; switch off before repairing storage if corruption persists.
- recovery: discard_and_full_turn, 0 attempts, side effect `baseline discarded`, baseline not advanced, reconcile=not_required, channel=event + report.

<a id="input_build_failed"></a>
### input_build_failed
- what: the complete Turn input could not be assembled.
- fix: inspect `traceId`, fix the input source, then run a full Turn; switch off if it cannot be built.
- recovery: fix_then_reconcile, 0 attempts, no side effects, baseline not advanced, reconcile=required_before_retry, channel=event + local trace.

<a id="context_too_large"></a>
### context_too_large
- what: measured input plus output reserve and overhead exceeds the provider window.
- fix: reduce registered backlog or select an approved larger-context provider, then reconcile.
- recovery: fix_then_reconcile, max 3 attempts, no side effects, baseline not advanced, reconcile=required_after_unknown_outcome, channel=typed event + alert.

<a id="context_budget_unavailable"></a>
### context_budget_unavailable
- what: the provider context window or token counting method is unknown.
- fix: configure the provider's real window and tokenizer/counting version, then reconcile; never guess.
- recovery: fix_then_reconcile, 0 attempts, no side effects, baseline not advanced, reconcile=required_after_unknown_outcome, channel=typed event + report.

<a id="audit_write_failed"></a>
### audit_write_failed
- what: the atomic skip audit transaction rolled back.
- fix: repair the workspace DB, query events by `plannedWakeAt`, then retry at most three total attempts with 1s/2s backoff.
- recovery: reconcile_then_retry, max 3 attempts, side effect `rolled_back`, baseline not advanced, reconcile=required_before_retry, channel=schedule audit_failed.

<a id="schedule_barrier_failed"></a>
### schedule_barrier_failed
- what: the skip committed but the Schedule update is unconfirmed.
- fix: query the audit by `plannedWakeAt`, reconcile the committed skip, then restore the Schedule; never blindly replay an unknown outcome.
- recovery: reconcile_committed_skip, max 3 attempts, side effect `committed_partial`, baseline not advanced, reconcile=required_before_retry, channel=report + local trace.

<a id="projection_reconcile_failed"></a>
### projection_reconcile_failed
- what: the session projection could not be reconciled after a committed skip.
- fix: read the committed events and reconcile the projection in at most three total attempts; persistent failure alerts and blocks further skips.
- recovery: fix_then_reconcile, max 3 attempts, side effect `committed`, baseline not advanced, reconcile=required_before_retry, channel=report + alert.

<a id="notification_failed"></a>
### notification_failed
- what: the post-commit SSE notification failed.
- fix: clients resume from the high watermark; the server does not retry indefinitely, and persistent failures alert.
- recovery: fix_then_reconcile, max 3 attempts, side effect `committed`, baseline not advanced, reconcile=required_before_retry, channel=report + tracing.

<a id="false_skip_detected"></a>
### false_skip_detected
- what: a dispatched skip was later proven to hide material changes.
- fix: immediately `PUT suppressionMode=off`, then `POST reconcile` and compare baseline/current snapshots; re-enable only after root cause, two clean preflights and fresh gate evidence.
- recovery: off_then_reconcile, 0 attempts, side effect `unknown until reconciled`, baseline not advanced, reconcile=required_before_re_enable, channel=alert + local trace.

<a id="shadow_gate_not_met"></a>
### shadow_gate_not_met
- what: a mode change was requested without a passing, approved, current gate artifact.
- fix: stay in the current mode and inspect the gate metrics; never force a transition.
- recovery: fix_then_reconcile, 0 attempts, no side effects, baseline not advanced, reconcile=not_required, channel=HTTP 409 problem.

<a id="suppression_config_invalid"></a>
### suppression_config_invalid
- what: the suppression configuration is outside the allowed range.
- fix: correct the field and retry the PUT; the previous configuration is unchanged, and `off` with no other fields is always accepted.
- recovery: fix_then_reconcile, 0 attempts, no side effects, baseline not advanced, reconcile=not_required, channel=HTTP 400 problem.

<a id="wake_in_flight"></a>
### wake_in_flight
- what: a scheduled or manual Turn is already in flight for this workspace.
- fix: read the current Turn, wait for completion, then retry with the same `Idempotency-Key`; never wake concurrently.
- recovery: fix_then_reconcile, 0 attempts, side effect `existing wake only`, baseline not advanced, reconcile=not_required, channel=HTTP 409 problem.

<a id="idempotency_conflict"></a>
### idempotency_conflict
- what: the same `Idempotency-Key` names a different operation or request fingerprint.
- fix: use a new `Idempotency-Key` for a different intent; never reuse a key.
- recovery: fix_then_reconcile, 0 attempts, no side effects, baseline not advanced, reconcile=not_required, channel=HTTP 409 problem.

<a id="preflight_timeout"></a>
### preflight_timeout
- what: preflight Source capture exceeded its bound.
- fix: inspect the slow Source, restore it, then run a full Turn; a timeout never advances the baseline and persistent timeouts alert.
- recovery: fix_then_full_turn, 0 attempts, no side effects, baseline not advanced, reconcile=not_required, channel=event + report.

<a id="wake_backpressure"></a>
### wake_backpressure
- what: wake capacity is saturated.
- fix: retry with the same `Idempotency-Key` after `retryAfterMs`; do not enqueue additional requests.
- recovery: fix_then_reconcile, 0 attempts, no side effects, baseline not advanced, reconcile=not_required, channel=HTTP 429 problem.

<a id="canonical_duplicate_key"></a>
### canonical_duplicate_key
- what: canonical JSON contains duplicate object keys.
- fix: emit each key exactly once and reconcile.
- recovery: the default contract recovery shape (see above).

<a id="canonical_non_finite"></a>
### canonical_non_finite
- what: canonical JSON contains a non-finite number.
- fix: emit finite JSON numbers only and reconcile.
- recovery: the default contract recovery shape.

<a id="canonical_invalid_json"></a>
### canonical_invalid_json
- what: raw Source bytes are not valid JSON.
- fix: emit valid canonical JSON and reconcile.
- recovery: the default contract recovery shape.

<a id="invalid_revision"></a>
### invalid_revision
- what: the opaque revision length is outside 1..=256 UTF-8 bytes.
- fix: emit a revision within 1..=256 bytes and reconcile.
- recovery: the default contract recovery shape.

<a id="invalid_hash"></a>
### invalid_hash
- what: the hash is not `sha256:` followed by 64 lowercase hex characters.
- fix: emit a valid sha256 hash and reconcile.
- recovery: the default contract recovery shape.

<a id="duplicate_source_id"></a>
### duplicate_source_id
- what: the Source registry contains duplicate `sourceId` values.
- fix: register each `sourceId` exactly once and reconcile.
- recovery: the default contract recovery shape.

<a id="duplicate_work_key"></a>
### duplicate_work_key
- what: the Source snapshot contains duplicate `workKey` values.
- fix: emit each `workKey` exactly once and reconcile.
- recovery: the default contract recovery shape.

<a id="source_descriptor_mismatch"></a>
### source_descriptor_mismatch
- what: the captured snapshot does not match its registered descriptor.
- fix: fix the adapter descriptor/snapshot identity and reconcile.
- recovery: the default contract recovery shape.

<a id="source_contract_invalid"></a>
### source_contract_invalid
- what: the Source snapshot violates the shared contract.
- fix: fix the Source adapter contract and reconcile.
- recovery: the default contract recovery shape.

## 4. HTTP API reference

All routes are under `/api/v1/workspaces/{workspaceId}/agent`.

| Method | Path | Authorization | Semantics |
|---|---|---|---|
| GET | `/schedule` | workspace read | Returns `suppressionMode`, `reconciliationSeconds`, `suppressionConfigVersion`. |
| PUT | `/schedule` | workspace write + admin | Validates and writes suppression fields; `off` needs no evidence. |
| GET | `/suppression/preflight` | workspace read | Read-only decision: `wouldSkip`, `vetoes`, `registryVersion`, public Source projections. |
| GET | `/suppression/report?turnId=...` | workspace read | Public projection, metrics, recent outcomes, alert and notification state. |
| POST | `/suppression/wake` | workspace write + admin + `Idempotency-Key` | Normal wake; may skip or run a full Turn. Returns 202. |
| POST | `/suppression/reconcile` | workspace write + admin + `Idempotency-Key` | Forces a full Turn, ignoring `wouldSkip`. Returns 202. |
| POST | `/suppression/gate-artifacts` | workspace write + admin + `Idempotency-Key` | Seals an evidence artifact from recorded observations. |
| POST | `/suppression/false-skip` | workspace write + admin + `Idempotency-Key` | Records a human-confirmed false skip. |

`PUT /schedule` fields: `enabled`, `intervalSeconds` (60..=86400), `prompt`,
`suppressionMode` (`off|shadow|canary|enforce`), `reconciliationSeconds`
(300..=86400). Invalid values return HTTP 400 `suppression_config_invalid`
without a partial write. `canary` validates the newest approved shadow artifact;
`enforce` validates the newest approved canary artifact; missing, stale,
tampered or synthetic-in-production evidence returns HTTP 409
`shadow_gate_not_met`.

`POST wake`/`reconcile` return:

```json
{
  "sessionKey": "agent:ws:.../main",
  "turnId": "turn:...",
  "outcome": "accepted",
  "reportUrl": "/api/v1/workspaces/ws/agent/suppression/report?turnId=turn:..."
}
```

The 202 never carries final `modelCalls`/`toolCalls`. Poll the report with the
returned `turnId`:

```bash
TURN=turn:...
for _ in $(seq 1 40); do
  BODY=$(curl --fail-with-body --silent --show-error \
    "$BASE/api/v1/workspaces/$WS/agent/suppression/report?turnId=$TURN" -H "$AUTH")
  OUTCOME=$(jq -r '.turn.outcome // empty' <<<"$BODY")
  case "$OUTCOME" in
    completed|skipped|failed|cancelled) printf '%s\n' "$BODY" | jq .; break ;;
  esac
  sleep 0.25
done
test "${OUTCOME:-}" != "" || { echo "turn did not settle: $TURN" >&2; exit 1; }
```

A skipped terminal always projects `modelCalls=0` and `toolCalls=0`. A
completed terminal projects the recorded `toolCalls`; `modelCalls` stays absent
unless the outcome is `skipped`, because the public projection never invents a
count.

`Idempotency-Key` is required for every write. The same key returns the recorded
result even if Sources changed; a different operation or fingerprint under the
same key returns HTTP 409 `idempotency_conflict`. In-flight work returns
`wake_in_flight`; saturation returns HTTP 429 `wake_backpressure` with
`retryAfterMs`.

### 4.1 Public projection allowlist

`preflight` and `report` return only `sourceId`, `sourceVersion`, compatibility,
availability, empty flag, active/waiting/closed counts, ref-validation status
and a safe error code. They never return refs, `workKey`, revisions, payloads,
memory/skill bodies, user text, credentials or internal paths.

### 4.2 Gate artifacts

`POST /suppression/gate-artifacts` derives every non-human metric from the
workspace observation table; the request supplies only sample references and
human provenance (`approver`, duplicate-report labels, false-skip review).
Duplicate turn IDs in the request are rejected with HTTP 400
`suppression_config_invalid`. A foreign or out-of-window turn ID, a wrong
observation mode, missing provenance or an artifact that disagrees with recorded
observations returns HTTP 409 `shadow_gate_not_met`. Replaying the same
`Idempotency-Key` returns the recorded artifact without appending a second row.

`POST /suppression/false-skip` requires a recorded canary/enforce `wouldSkip`
observation; it emits the single `false_skip_detected` alert producer used by
artifact staleness.

## 5. Version and migration matrix

| Name | Current | Owner | Compatibility rule |
|---|---|---|---|
| `TURN_STATE_VERSION` / `stateVersion` | 1 | `suppression/contract.rs` | v1 is the only readable envelope. Unknown versions are rejected; additive fields inside v1 are allowed. |
| `sourceVersion` | 1 | `suppression/registry.rs` (`SOURCE_VERSION`) | A snapshot is compatible only when its version equals the registered descriptor. A bump makes public projections `compatible=false` and invalidates older gate artifacts through `registryVersion`. |
| `registryVersion` | `sha256:` of canonical descriptors | `canonical::registry_version` | Derived from the registered `(sourceId, sourceVersion)` list in order. Any add/remove/version change produces a new value; existing gate artifacts become stale. |
| `suppressionConfigVersion` / `configVersion` | per workspace, starts at 1 | schedule table | Increments on every accepted suppression update. Preflight decisions record the version they observed. |
| gate artifact `contentSha256` | `sha256:` over canonical JCS artifact bytes excluding the hash field | `preflight::gate_artifact_sha256` | Tampering is rejected at validation time. Artifacts are append-only. |

Database changes are additive:

- `20261114000001_assistant_suppression.sql` adds the schedule suppression
  columns (`suppression_mode`, `reconciliation_seconds`,
  `suppression_config_version`) to the platform scheduler row.
- `20261114000002_harness_assistant_suppression_workspace.sql` adds the
  per-workspace `suppression_state`, `suppression_idempotency` and
  `suppression_gate_evaluations` tables.
- `20261114000003_harness_suppression_fence_attempt_ids.sql` adds the durable
  per-fence attempt identity and start-time columns used by startup recovery.
- `20261115000001_harness_suppression_observations.sql` adds
  `suppression_observations` and its indexes (the sole source for gate
  denominators/numerators).

No destructive migration and no codemod are required. Existing workspaces start
in `off` with config version 1. Rolling back the code but keeping the additive
tables is safe; the old binary ignores them.

## 6. Quickstart

### 6.1 Start the fixture

Terminal 1:

```bash
cargo run -p tinyiothub-cloud --example suppression_quickstart
```

The fixture listens only on `127.0.0.1:3109` (override with
`SUPPRESSION_QUICKSTART_PORT`), uses an in-memory platform SQLite plus a
temporary per-workspace Harness store, and creates:

- workspace `ws-suppression-demo`, tenant `tenant-suppression-demo`;
- synthetic bearer token `test-token` mapped to a local administrator;
- Sources `demo.static` and `demo.changing`;
- synthetic shadow and canary gate evidence, valid only because the server
  configuration sets `suppression_gate_allow_synthetic`.

No real provider, device, credential or browser cookie is involved. There is no
client parameter that enables synthetic evidence.

Production paths the fixture exercises:

- `SuppressionService::evaluate` for every preflight decision, including
  baseline parsing, registry comparison, control cursor and reconciliation
  vetoes;
- `begin_full_attempt` / `complete_full_attempt` (and
  `settle_full_attempt_without_baseline` for `off`) for the durable baseline
  lifecycle;
- `AtomicSkipAudit` + `Db::commit_skip` for the skip terminal, followed by the
  same Schedule/projection/notification barrier reconciliation;
- the production Source adapters' `resolve_state` / `resolve_payload`: every
  full Turn resolves each captured ref and asserts the assembled request
  contains every registered Source's canonical content and work-item payloads;
- the production HTTP handlers, authorization gates, idempotency records and
  public DTOs.

Synthetic boundary: model and tool execution only. The fixture does not call a
model or a tool, so a completed full Turn records fixed counts of 2 model calls
and 1 tool call. Gate observations and artifacts are synthetic
(`sample.synthetic=true`) and exist only because the fixture server sets
`suppression_gate_allow_synthetic`; the synthetic token maps to a local
administrator claim and the fixture binds to loopback only.

### 6.2 Execute the documented flow

Terminal 2:

```bash
set -euo pipefail
command -v jq >/dev/null
BASE=http://127.0.0.1:3109
WS=ws-suppression-demo
AUTH="Authorization: Bearer test-token"
JSON="Content-Type: application/json"
CURL=(curl --fail-with-body --silent --show-error)

wait_turn() {
  local turn_id="$1" body outcome
  for _ in $(seq 1 40); do
    body=$("${CURL[@]}" "$BASE/api/v1/workspaces/$WS/agent/suppression/report?turnId=$turn_id" -H "$AUTH")
    outcome=$(jq -r '.turn.outcome // empty' <<<"$body")
    case "$outcome" in
      completed|skipped|failed|cancelled) printf '%s\n' "$body"; return 0 ;;
    esac
    sleep 0.25
  done
  printf 'turn did not reach a terminal state within 10s: %s\n' "$turn_id" >&2
  return 1
}

# 1. shadow: the full Turn still runs; the next preflight reports wouldSkip=true.
"${CURL[@]}" -X PUT "$BASE/api/v1/workspaces/$WS/agent/schedule" \
  -H "$AUTH" -H "$JSON" -d '{"suppressionMode":"shadow"}' \
  | jq '{suppressionMode, suppressionConfigVersion}'
TURN=$("${CURL[@]}" -X POST "$BASE/api/v1/workspaces/$WS/agent/suppression/wake" \
  -H "$AUTH" -H 'Idempotency-Key: demo-0001' | jq -r '.turnId')
wait_turn "$TURN" | jq '{turn, mode}'
"${CURL[@]}" "$BASE/api/v1/workspaces/$WS/agent/suppression/preflight" -H "$AUTH" \
  | jq '{mode, wouldSkip, vetoes, sourceIds: [.sources[].sourceId]}'

# 2. Change ChangingSource, switch to enforce, then run the next full Turn.
# A mode change increments configVersion; the first full Turn after the change
# installs the baseline at the new version, after which skips may commit. The
# changed revision already forces this Turn to be full.
"${CURL[@]}" -X POST "$BASE/fixture/sources/changing/items/item-1" \
  -H "$JSON" -d '{"revision":"2","text":"resolved"}' | jq .
"${CURL[@]}" -X PUT "$BASE/api/v1/workspaces/$WS/agent/schedule" \
  -H "$AUTH" -H "$JSON" -d '{"suppressionMode":"enforce"}' \
  | jq '{suppressionMode, suppressionConfigVersion}'
"${CURL[@]}" "$BASE/api/v1/workspaces/$WS/agent/suppression/preflight" -H "$AUTH" \
  | jq '{mode, wouldSkip, vetoes}'
TURN=$("${CURL[@]}" -X POST "$BASE/api/v1/workspaces/$WS/agent/suppression/wake" \
  -H "$AUTH" -H 'Idempotency-Key: demo-0002' | jq -r '.turnId')
wait_turn "$TURN" | jq '{turn, mode, sourceIds: [.sources[].sourceId]}'

# 3. enforce (synthetic loopback gate evidence): two zero-call skips.
for KEY in demo-0003 demo-0004; do
  TURN=$("${CURL[@]}" -X POST "$BASE/api/v1/workspaces/$WS/agent/suppression/wake" \
    -H "$AUTH" -H "Idempotency-Key: $KEY" | jq -r '.turnId')
  wait_turn "$TURN" | jq '{turn, mode}'
done

# 4. off + reconcile: forced full Turn.
"${CURL[@]}" -X PUT "$BASE/api/v1/workspaces/$WS/agent/schedule" \
  -H "$AUTH" -H "$JSON" -d '{"suppressionMode":"off"}' | jq '{suppressionMode}'
TURN=$("${CURL[@]}" -X POST "$BASE/api/v1/workspaces/$WS/agent/suppression/reconcile" \
  -H "$AUTH" -H 'Idempotency-Key: demo-0005' | jq -r '.turnId')
wait_turn "$TURN" | jq '{turn, mode}'
```

Expected observable sequence:

1. shadow full: `turn.outcome=completed`, `turn.toolCalls=1`; the following
   preflight has `wouldSkip=true`.
2. changed revision + enforce: preflight has `wouldSkip=false` and `vetoes`
   contains `source_changed`; the full Turn completes under enforce and the
   report still lists `demo.static` and `demo.changing`. This Turn installs the
   baseline at the new `suppressionConfigVersion`.
3. enforce skips 1 and 2: `turn.outcome=skipped`, `turn.modelCalls=0`,
   `turn.toolCalls=0`.
4. off + reconcile: `turn.outcome=completed` with `turn.toolCalls=1`; preflight
   is `mode=off`, `wouldSkip=false`, veto `disabled`.

Do not switch mode and immediately expect a skip: `commit_skip` rejects a
baseline whose persisted `configVersion` predates the current schedule. The
production-equivalent recovery is a full Turn under the new configuration (as
step 2 does here); an attempted skip returns `audit_failed` and keeps the old
baseline ineligible.

## 7. Recovery and rollback

Use `docs/runbooks/assistant-suppression.md` for the complete procedures.

- Kill switch: `PUT {"suppressionMode":"off"}` is immediate and needs no other
  fields or evidence.
- Reconcile: `POST /suppression/reconcile` forces one full Turn and keeps the
  pre-Turn evidence; it does **not** clear sync barriers.
- `committed_partial`: do not replay blindly; confirm the immutable skip
  terminal, keep the workspace off, repair the Schedule writer, restart.
- `audit_failed`: keep off while repairing the workspace DB, then a successful
  full Turn installs a fresh baseline.
- False skip: switch off, reconcile, compare baseline/current snapshots, then
  re-enable only after root-cause repair, two clean preflights and fresh
  approved evidence.
- Production gate evidence is never synthetic. The loopback fixture's
  `synthetic=true` artifacts are accepted only because its server configuration
  sets `suppression_gate_allow_synthetic`; production validation rejects them.
