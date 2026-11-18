# Assistant Suppression Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build an observable, rollback-safe generic Source/Revision/WorkItem suppression gate that skips unchanged scheduled Turns without changing full-Turn input semantics.

**Architecture:** Source Adapters own semantic snapshots; `SuppressionService` owns preflight decisions; workspace SQLite owns durable baseline, attempt, idempotency and failure-fence state. Shadow estimates benefits, canary measures real savings, and long-term enforce requires a reviewed gate artifact. Phase 2 incremental input and physical tombstone release are excluded.

**Tech Stack:** Rust 2024, Axum 0.8, Tokio, SQLx/SQLite, serde_json, sha2, existing Lit SSE projection (no frontend changes planned).

**Spec:** `docs/designs/assistant-turn-state-diff.md`

## Global Constraints

- Harness never learns device types, alarm levels, skill thresholds or business skip rules.
- Every full Turn preserves existing history, prompt, tools and all-Source input; delta metadata never replaces content.
- Default mode is `off`; valid modes are `off | shadow | canary | enforce`.
- Only fully successful, non-degraded Turns with all Sources available may install a baseline.
- Full failure, cancellation, partial, timeout, unknown outcome and audit/projection/notification failures must leave durable fences visible to all workers.
- `turn.state` and raw refs are backend-only; public reports exclude refs, workKeys, revisions, payloads, user text and internal paths.
- No new fixed Task/media/work-item quotas; use backpressure, coalescing, metrics and alerts.
- JSON is camelCase, Rust is snake_case, event names are dot-separated lowercase, error codes are snake_case.
- Revision is an opaque UTF-8 string of 1..=256 bytes. Hashes are `sha256:` plus 64 lowercase hex characters.
- `TURN_STATE_VERSION = 1`; each initial `sourceVersion = 1`.
- Reconciliation defaults to 1800 seconds and is configurable in 300..=86400 seconds.
- Recovery defaults to at most 3 total attempts (initial + 2 retries, 1s/2s backoff), not 3 retries.
- No production gate bypass, real credentials, real browser cookies or real device writes in fixtures.
- Commit only the task's files; the working tree already contains unrelated changes that must not be staged.
- Do not start Phase 2 implementation from this plan.

## Review Focus

- **Semantic change hidden by an Adapter:** expect a full Turn, not permanent skip. Task 3 adds real-sample and direct-deletion conformance; Task 4 adds reconciliation fencing.
- **Failure after a valid old baseline:** expect the old baseline to be unusable across restart and another worker. Task 2 adds full/audit/sync fence tests; Task 6 covers the existing runner integration.
- **Off/user/registry change between preflight and commit:** expect no stale skip. Task 5 adds pause-before-commit tests and conditional writes.
- **Committed skip followed by changed-state replay:** expect a new linked attempt, never overwritten history. Task 5 covers scheduler replay; Task 7 covers HTTP fingerprint/idempotency conflicts.
- **Payload or internals exposed by report/errors:** expect only the public allowlist. Task 7 adds cross-tenant serialization and negative synthetic-secret tests; Task 8 checks error redaction.
- **Accepted async response mistaken for a final result:** expect terminal evidence only from report. Tasks 7 and 11 cover accepted-only 202 and bounded polling.
- **Slow/hanging Source or synchronized workspace burst:** expect timeout/backpressure and no unbounded queue. Task 9 owns slow-adapter, replay-storm and SQLite-contention tests.

---

## File Map

- `apps/cloud/src/domains/agent/session/suppression/contract.rs` — Source/WorkItem/ref/version/value contracts.
- `apps/cloud/src/domains/agent/session/suppression/canonical.rs` — JCS canonical bytes and domain-separated hashes.
- `apps/cloud/src/domains/agent/session/suppression/error.rs` — typed errors, public allowlist and stable codes.
- `apps/cloud/src/domains/agent/session/suppression/registry.rs` — Adapter registration, capture/resolve and conformance harness.
- `apps/cloud/src/domains/agent/session/suppression/preflight.rs` — baseline comparison, vetoes and reconciliation decisions.
- `apps/cloud/src/domains/agent/session/suppression/audit.rs` — planned-wake attempts, atomic skip and replay behavior.
- `apps/cloud/src/domains/agent/session/suppression/mod.rs` — crate-private public surface and orchestration.
- `crates/db/src/suppression.rs` — durable state, idempotency and gate-artifact persistence.
- `crates/db/migrations/20261114000001_assistant_suppression.sql` — additive tables/columns/indexes.
- `crates/db/src/harness_event.rs` — extract transaction-scoped append while preserving existing behavior.
- `crates/db/src/assistant_schedule.rs` — suppression mode/config persistence.
- `apps/cloud/src/domains/agent/session/wake.rs` — preflight and full-Turn lifecycle integration.
- `apps/cloud/src/domains/agent/session/trigger.rs` — dispatch outcome and completion-time scheduling.
- `apps/cloud/src/domains/agent/session/workspace_api.rs` — schedule/preflight/report/wake/reconcile HTTP mapping.
- `apps/cloud/examples/suppression_quickstart.rs` — loopback synthetic example.
- `docs/dev/assistant-suppression.md` — Source author guide, errors and quickstart.
- `docs/runbooks/assistant-suppression.md` — recovery, rollout and rollback runbook.
- `CHANGELOG.md` — additive schema/API and upgrade notes.
- Tests stay in the owning module or existing `crates/db/tests/`, `apps/cloud/src/tests/`, and `apps/cloud/tests/` conventions.

---

### Task 1: Canonical Source Contract and Hashing

**Files:**
- Create: `apps/cloud/src/domains/agent/session/suppression/contract.rs`
- Create: `apps/cloud/src/domains/agent/session/suppression/canonical.rs`
- Create: `apps/cloud/src/domains/agent/session/suppression/error.rs`
- Create: `apps/cloud/src/domains/agent/session/suppression/mod.rs`

**Interfaces:**
- Produces: `TURN_STATE_VERSION`, `SourceDescriptor`, `SourceSnapshot`, `WorkItem`, `StateRef`, `PayloadRef`, `CanonicalBytes`, `SuppressionError`.
- Produces: `canonical_bytes(value: &serde_json::Value) -> Result<Vec<u8>, SuppressionError>`, `canonicalize_json(input: &[u8]) -> Result<Vec<u8>, SuppressionError>`, `hash_state(bytes: &[u8]) -> String`, `hash_payload(bytes: &[u8]) -> String`, `registry_version(descriptors: &[SourceDescriptor]) -> String`.
- Guarantees: descriptor/snapshot IDs and versions match; duplicate workKeys are rejected; `available=false` carries a safe error; `available=true` has all required refs/hashes.

- [ ] **Step 1: Write failing canonical and schema tests**

```rust
#[test]
fn canonical_hash_is_stable_for_key_order_and_negative_zero() {
    let a = serde_json::json!({"b": -0.0, "a": "雪"});
    let b = serde_json::json!({"a": "雪", "b": 0});
    assert_eq!(canonical_bytes(&a).unwrap(), canonical_bytes(&b).unwrap());
    assert_eq!(hash_state(&canonical_bytes(&a).unwrap()).len(), 71);
}

#[test]
fn unavailable_snapshot_has_safe_error_and_no_refs() {
    let snapshot = unavailable_fixture("source_unavailable");
    assert!(!snapshot.available);
    assert!(snapshot.state_ref.is_none());
    assert_eq!(snapshot.error.unwrap().code, "source_unavailable");
}
#[test]
fn raw_json_rejects_duplicate_object_keys() {
    let error = canonicalize_json(br#"{"a":1,"a":2}"#).unwrap_err();
    assert_eq!(error.code, "canonical_duplicate_key");
}
```

Also assert rejection of NaN/Infinity, duplicate object keys, duplicate workKeys, descriptor mismatch and duplicate sourceIds. Include empty/256-byte/257-byte revisions, invalid hash strings, and golden vectors for JCS numbers, Unicode, escapes and arrays; prove `hash_state(b"{}") != hash_payload(b"{}")` due to domain separation.

- [ ] **Step 2: Run tests and verify RED**

Run: `cargo test -p tinyiothub-cloud suppression::contract -- --nocapture`
Expected: FAIL because the suppression module and canonical functions do not exist.

- [ ] **Step 3: Implement the typed contract and canonicalizer**

Use one shared JCS/RFC 8785-compatible implementation: UTF-8 NFC, LF, sorted object keys, finite numbers only, -0 normalized to 0, JCS number formatting and deterministic Unicode escaping. `canonicalize_json` parses raw bytes with duplicate-key detection before canonicalization; `canonical_bytes` handles an already validated Value. Arrays preserve order except WorkItems, which sort by `(workKey, revision)`. Use domain prefixes `tinyiothub:source-state:v1\0`, `tinyiothub:work-payload:v1\0`, and `tinyiothub:source-registry:v1\0`. Reuse existing `sha2`; do not add a dependency.

- [ ] **Step 4: Run tests and verify GREEN**

Run: `cargo test -p tinyiothub-cloud suppression::contract -- --nocapture`
Expected: PASS for all schema, golden-vector and property tests.

- [ ] **Step 5: Commit**

```bash
git add apps/cloud/src/domains/agent/session/suppression/
git commit -m "feat(agent): add canonical suppression source contract"
```

### Task 2: Durable Suppression State and Event Transactions

**Files:**
- Create: `crates/db/migrations/20261114000001_assistant_suppression.sql`
- Create: `crates/db/src/suppression.rs`
- Modify: `crates/db/src/lib.rs`
- Modify: `crates/db/src/harness_event.rs:221-289`
- Modify: `crates/db/src/assistant_schedule.rs`
- Test: `crates/db/tests/suppression_tests.rs`

**Interfaces:**
- Consumes: canonical `SourceSnapshot`, versions and attempt IDs from Task 1.
- Produces: `begin_full_attempt`, `complete_full_attempt`, `fail_full_attempt`, `begin_skip_attempt`, `commit_skip`, `clear_sync_pending`, `load_suppression_state`.
- Produces: `append_session_event_tx(&mut Transaction, &HarnessEventInput) -> Result<HarnessEvent>` with identical event_id/digest semantics to the existing append path.

- [ ] **Step 1: Write failing persistence and crash-window tests**

```rust
#[tokio::test]
async fn failed_full_attempt_makes_old_baseline_ineligible_across_restart() {
    let fixture = suppression_db_fixture().await;
    let baseline = install_complete_baseline(&fixture).await;
    let attempt = fixture.db.begin_full_attempt("session-1", &baseline).await.unwrap();
    fixture.db.fail_full_attempt(attempt.attempt_id, "provider_failed").await.unwrap();
    let reloaded = fixture.reopen().await;
    assert_eq!(reloaded.latest_outcome(), "full_failed");
    assert!(!reloaded.baseline_is_eligible());
}
```

Add cases for cancelled/partial/degraded/timeout, two-worker CAS, full_attempt_in_progress after crash, audit_in_progress after failed audit, schedule/projection/notification pending, unknown-outcome reconciliation, and existing append digest conflict. `commit_skip` must reject a changed stateVersion, configVersion, registryVersion, cursor or fence.

- [ ] **Step 2: Run DB tests and verify RED**

Run: `cargo test -p db --test suppression_tests -- --nocapture`
Expected: FAIL because the tables and lifecycle methods are absent.

- [ ] **Step 3: Add additive schema and lifecycle methods**

Create `suppression_state`, `suppression_idempotency`, and `suppression_gate_evaluations`; add `suppression_mode`, `reconciliation_seconds`, and `suppression_config_version` to `assistant_schedules`. All new tables carry workspaceId, createdAt/updatedAt and required version fields. `suppression_idempotency` has `UNIQUE(workspace_id, operation, key)`.

A full attempt writes in-progress before model work; a successful all-available Turn atomically installs baseline/cursor/turn.state and clears the fence; every other terminal writes full_failed. Skip writes audit_in_progress before the audit transaction, and pending sync flags commit with skipped events. No method may report cross-database atomicity.

Extract `append_session_event_tx` from the existing implementation and keep its external behavior unchanged.

- [ ] **Step 4: Run new and existing DB tests**

Run: `cargo test -p db --test suppression_tests && cargo test -p db`
Expected: all PASS, including existing harness event tests.

- [ ] **Step 5: Commit**

```bash
git add crates/db/migrations/20261114000001_assistant_suppression.sql crates/db/src/suppression.rs crates/db/src/lib.rs crates/db/src/harness_event.rs crates/db/src/assistant_schedule.rs crates/db/tests/suppression_tests.rs
git commit -m "feat(db): persist assistant suppression lifecycle"
```

### Task 3: Source Registry and Six Adapters

**Files:**
- Create: `apps/cloud/src/domains/agent/session/suppression/registry.rs`
- Modify: `apps/cloud/src/domains/agent/session/suppression/mod.rs`
- Modify: `apps/cloud/src/domains/agent/session/wake.rs`
- Modify: `apps/cloud/src/domains/agent/prompt/assistant.rs`
- Test: in `registry.rs` and existing wake/prompt tests

**Interfaces:**
- Produces: `AssistantSource` with `descriptor`, `capture`, `resolve_state`, and `resolve_payload` exactly as specified.
- Produces: `SourceRegistry::register`, `capture_all`, `assert_source_contract`.
- Registers exactly `workspace.snapshot`, `session.events`, `turn.brief`, `skills`, `short_term`, and `memory`, all at sourceVersion 1.

- [ ] **Step 1: Write failing registry and conformance tests**

```rust
#[tokio::test]
async fn changing_one_source_keeps_all_sources_in_full_input() {
    let fixture = six_source_fixture().await;
    fixture.change("turn.brief", "new revision").await;
    let request = fixture.run_full_turn().await;
    for source in fixture.registry.source_ids() {
        assert!(request.contains_source(source), "missing {source}");
    }
}
```

Add nil, valid empty, unavailable, noise, close, direct-delete, missing last payload, stale ref, duplicate workKey/sourceId, descriptor mismatch and Session Events self-feedback tests. Six adapters must pass the same `assert_source_contract`. The full-input path must reuse existing stores/prompt inputs rather than inject duplicate content.

- [ ] **Step 2: Run tests and verify RED**

Run: `cargo test -p tinyiothub-cloud suppression::registry -- --nocapture`
Expected: FAIL because registry/adapters/conformance harness do not exist.

- [ ] **Step 3: Implement registry and six adapters**

Each capture batches its WorkItems; no per-item SQL. Session Events excludes Harness lifecycle, `turn.state`, alerts, assistant output, and cursor-owned `message.created(role=user)` from semantic work. Other external semantic events use stable IDs/types and normalized payloads. Closed/directly deleted items produce a new revision and retain the last payload until at least one successful full scheduled Turn.

Registry rejects duplicate sourceIds and descriptor/snapshot mismatch. `CanonicalBytes` is shared validated bytes; hashing uses incremental writers without duplicating a full-size hash buffer.

- [ ] **Step 4: Run adapter and prompt regression tests**

Run: `cargo test -p tinyiothub-cloud suppression::registry && cargo test -p tinyiothub-cloud domains::agent::prompt`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/cloud/src/domains/agent/session/suppression/ apps/cloud/src/domains/agent/session/wake.rs apps/cloud/src/domains/agent/prompt/assistant.rs
git commit -m "feat(agent): register semantic suppression sources"
```

### Task 4: Preflight Decisions and Reconciliation

**Files:**
- Create: `apps/cloud/src/domains/agent/session/suppression/preflight.rs`
- Modify: `apps/cloud/src/domains/agent/session/suppression/mod.rs`
- Modify: `apps/cloud/src/domains/agent/session/wake.rs`
- Test: in `preflight.rs`

**Interfaces:**
- Consumes: Source snapshots, persisted baseline/state, schedule mode/config and current user seq.
- Produces: `SuppressionMode`, `SkipVeto`, `PreflightDecision { wouldSkip, vetoes, snapshot, baselineStateId, configVersion }`.
- Produces: `SuppressionService::evaluate(...) -> Result<PreflightDecision, SuppressionError>`.

- [ ] **Step 1: Write failing decision-table tests**

```rust
#[test]
fn unchanged_enforce_snapshot_would_skip() {
    let result = decision_fixture(SuppressionMode::Enforce)
        .unchanged()
        .not_due()
        .evaluate();
    assert!(result.would_skip);
    assert!(result.vetoes.is_empty());
}
```

Cover every veto: disabled, baseline_missing, baseline_corrupt, source_unavailable, source_changed, source_ref_mismatch, source_revision_conflict, registry_changed, new_user_input, reconciliation_due, audit_failed and sync/full fences. Shadow computes wouldSkip but never skips; off returns disabled; valid empty is not an error; degraded/failed baselines cannot skip.

- [ ] **Step 2: Run tests and verify RED**

Run: `cargo test -p tinyiothub-cloud suppression::preflight -- --nocapture`
Expected: FAIL because the decision entry point is absent.

- [ ] **Step 3: Implement deterministic preflight**

Compare registry/config/state versions, all Source availability, refs/hashes, user cursor, reconciliation deadline and durable fences. Use the configured reconciliationSeconds, default 1800. The observedAt snapshot is the external-Source boundary; later changes belong to the next Turn. Keep full-input behavior unchanged for shadow, veto and reconciliation.

- [ ] **Step 4: Run tests and verify GREEN**

Run: `cargo test -p tinyiothub-cloud suppression::preflight`
Expected: PASS for the full decision table with injected clocks.

- [ ] **Step 5: Commit**

```bash
git add apps/cloud/src/domains/agent/session/suppression/ apps/cloud/src/domains/agent/session/wake.rs
git commit -m "feat(agent): add deterministic suppression preflight"
```

### Task 5: Atomic Skip, Wake Attempts and Replay

**Files:**
- Create: `apps/cloud/src/domains/agent/session/suppression/audit.rs`
- Modify: `apps/cloud/src/domains/agent/session/wake.rs`
- Modify: `apps/cloud/src/domains/agent/session/trigger.rs`
- Modify: `crates/db/src/suppression.rs`
- Test: in `audit.rs` and `crates/db/tests/suppression_tests.rs`

**Interfaces:**
- Produces: stable `wakeId(workspaceId, plannedWakeAt)` and monotonic `attemptNo`; turnId/wakeEventId/event IDs include attemptNo.
- Produces: `AtomicSkipAudit::commit` and replay result using existing event lifecycle and `append_session_event_tx`.

- [ ] **Step 1: Write failing atomicity and replay tests**

```rust
#[tokio::test]
async fn changed_replay_creates_linked_full_attempt_without_overwriting_skip() {
    let fixture = committed_skip_fixture().await;
    fixture.change_source().await;
    let full = fixture.replay_planned_wake().await;
    assert_eq!(full.outcome(), "completed");
    assert_ne!(full.turn_id(), fixture.skip_turn_id());
    assert_eq!(full.previous_attempt_id(), Some(fixture.skip_attempt_id()));
    assert_eq!(fixture.skipped_event_count(), 1);
}
```

Add unchanged replay, duplicate event IDs, transaction rollback, audit failure fence, crash before Schedule update, off-before-commit, user-message-before-commit, registry/config change, and concurrent audit attempts.

- [ ] **Step 2: Run tests and verify RED**

Run: `cargo test -p tinyiothub-cloud suppression::audit -- --nocapture`
Expected: FAIL because attempt-aware atomic skip is absent.

- [ ] **Step 3: Implement atomic skip and immutable replay history**

Derive wakeId from workspace/planned wake and event IDs from `(wakeId, attemptNo)`. Keep skipped events immutable; changed replay links `previousAttemptId/supersedes`. Commit only if canary/enforce mode, config/registry/state versions, cursor and all fences still match. PUT off commits mode/config change before returning. Audit failure never exposes a partial skipped lifecycle and leaves a shared fence.

- [ ] **Step 4: Run audit and existing trigger tests**

Run: `cargo test -p tinyiothub-cloud suppression::audit && cargo test -p tinyiothub-cloud domains::agent::session::trigger`
Expected: all PASS, including in-flight and long-Turn regressions.

- [ ] **Step 5: Commit**

```bash
git add apps/cloud/src/domains/agent/session/suppression/ apps/cloud/src/domains/agent/session/wake.rs apps/cloud/src/domains/agent/session/trigger.rs crates/db/src/suppression.rs crates/db/tests/suppression_tests.rs
git commit -m "fix(agent): make skipped turns atomic and replay-safe"
```

### Task 6: Existing Wake Lifecycle Integration

**Files:**
- Modify: `apps/cloud/src/domains/agent/session/wake.rs:367-532`
- Modify: `apps/cloud/src/domains/agent/session/runner.rs:260-320`
- Modify: `apps/cloud/src/domains/agent/session/trigger.rs:182-193`
- Test: existing wake/trigger tests plus new suppression integration cases

**Interfaces:**
- Consumes: durable attempt APIs and preflight/audit decisions.
- Produces: one shared full-Turn function used by ordinary wake and reconcile; only `forceFullReason` differs.
- Produces: event-write health evidence used before baseline installation.

- [ ] **Step 1: Write failing integration tests**

```rust
#[tokio::test]
async fn failed_reconcile_cannot_reuse_previous_baseline() {
    let fixture = wake_fixture_with_baseline().await;
    fixture.reconcile_with_provider_error().await;
    let decision = fixture.preflight_now().await;
    assert!(!decision.would_skip);
    assert_eq!(decision.fence, "full_failed");
}
```

Cover partial Todos, cancellation, degraded Source, terminal-event write failure, restart, and successful baseline installation. Assert wake and reconcile produce identical request bytes for the same captured state.

- [ ] **Step 2: Run integration tests and verify RED**

Run: `cargo test -p tinyiothub-cloud domains::agent::session::wake -- --nocapture`
Expected: new lifecycle test FAILs because failed attempts currently do not create durable fences.

- [ ] **Step 3: Integrate lifecycle into the existing runner**

Call the existing full-Turn path for both wake and reconcile; do not duplicate history/model/tool code. Mark full in-progress before agent work, clear it only after a fully successful and completely persisted Turn, and write full_failed otherwise. Track Session Event write failures explicitly so a missing terminal event cannot install a baseline.

Keep TriggerRouter as the only Schedule outcome writer. Preserve completion-time plus interval behavior in off/shadow.

- [ ] **Step 4: Run wake, runner and trigger regressions**

Run: `cargo test -p tinyiothub-cloud domains::agent::session::wake && cargo test -p tinyiothub-cloud domains::agent::session::runner && cargo test -p tinyiothub-cloud domains::agent::session::trigger`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/cloud/src/domains/agent/session/wake.rs apps/cloud/src/domains/agent/session/runner.rs apps/cloud/src/domains/agent/session/trigger.rs
git commit -m "feat(agent): integrate durable full-turn suppression lifecycle"
```

### Task 7: Workspace API, Public DTOs and HTTP Idempotency

**Files:**
- Modify: `apps/cloud/src/domains/agent/session/workspace_api.rs`
- Modify: `crates/db/src/assistant_schedule.rs`
- Modify: `crates/db/src/suppression.rs`
- Modify: `apps/cloud/src/api/mod.rs`
- Test: `apps/cloud/src/tests/` following existing API test conventions

**Interfaces:**
- Extends `UpdateScheduleRequest` with `suppressionMode` and `reconciliationSeconds`.
- Adds preflight/report GET and wake/reconcile POST routes from the spec.
- 202 returns only sessionKey, turnId, outcome=`accepted`, and report URL; final counts live in terminal report.
- `suppression_idempotency` persists operation/key/fingerprint/plannedWake/attempt/result.

- [ ] **Step 1: Write failing API contract tests**

```rust
#[tokio::test]
async fn accepted_wake_does_not_claim_final_counts() {
    let response = post_wake(&fixture, "wake-1").await;
    assert_eq!(response.status(), 202);
    assert_eq!(response.json()["outcome"], "accepted");
    assert!(response.json().get("modelCalls").is_none());
}

#[tokio::test]
async fn public_report_excludes_internal_refs_and_work_keys() {
    let body = get_report(&fixture).await;
    assert!(body.get("stateRef").is_none());
    assert!(body.get("payloadRef").is_none());
    assert!(body.get("workKey").is_none());
}
```

Add workspace read/write authorization, cross-tenant access, invalid config 400, in-flight/idempotency conflict 409, backpressure 429, same-request replay, concurrent winner, changed-fingerprint rejection, unknown-outcome reconciliation, and report redaction tests. Raw turn.state must be filtered from public Session Events API/SSE.

- [ ] **Step 2: Run API tests and verify RED**

Run: `cargo test -p tinyiothub-cloud suppression_api -- --nocapture`
Expected: FAIL because routes/DTOs/idempotency semantics are absent.

- [ ] **Step 3: Implement routes and public projections**

Reuse existing workspace auth helpers: GET requires read, PUT/POST requires write. New errors use RFC 9457 problem details. Public DTOs expose only Source IDs, compatibility, availability, counts, vetoes, refValidation, safe error codes and registryVersion. POST requests require Idempotency-Key; same intent replays the recorded result, different intent needs a new key, and concurrent identical calls have one database winner.

PUT off commits before responding and always works when no other fields are supplied. Canary/enforce artifact validation is wired in Task 10.

- [ ] **Step 4: Run API and existing workspace tests**

Run: `cargo test -p tinyiothub-cloud suppression_api && cargo test -p tinyiothub-cloud domains::agent::session::workspace_api`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/cloud/src/domains/agent/session/workspace_api.rs apps/cloud/src/api/mod.rs crates/db/src/assistant_schedule.rs crates/db/src/suppression.rs apps/cloud/src/tests/
git commit -m "feat(api): add safe assistant suppression operations"
```

### Task 8: Agent-Ready Errors, Alerts and Sync Recovery

**Files:**
- Modify: `apps/cloud/src/domains/agent/session/suppression/error.rs`
- Modify: `apps/cloud/src/domains/agent/session/suppression/audit.rs`
- Modify: `apps/cloud/src/domains/agent/session/wake.rs`
- Create: `docs/runbooks/assistant-suppression.md`
- Test: owning modules

**Interfaces:**
- Produces: typed `SuppressionError` fields `code/what/fix/recovery/codeAnchor/context/trace/redaction/channel`.
- Covers all codes in spec §11 plus `idempotency_conflict`, `preflight_timeout`, and `wake_backpressure`.
- Recovery means at most 3 total attempts, initial + 2 retries with 1s/2s delays.

- [ ] **Step 1: Write failing error and recovery tests**

```rust
#[test]
fn context_error_is_actionable_and_redacted() {
    let error = context_too_large_fixture();
    assert_eq!(error.code, "context_too_large");
    assert_eq!(error.recovery.side_effects, "none");
    assert!(!error.baseline_advanced());
    assert!(!error.to_public_json().contains("synthetic-secret"));
}
```

Add every introduced path's field assertions, side-effect state, retry bound, safe cause and channel. Test audit rollback, Schedule committed_partial, projection reconcile, post-commit SSE notification failure, false-skip off/reconcile, and persistent-failure alert. Unknown outcomes must reconcile before retry.

- [ ] **Step 2: Run tests and verify RED**

Run: `cargo test -p tinyiothub-cloud suppression::error -- --nocapture`
Expected: FAIL for missing typed fields and recovery behavior.

- [ ] **Step 3: Implement catalog, alerts and recovery**

Use caller-safe causes, trace IDs and local-only trace paths. Public errors never include payload, memory, credentials, internal refs, user-derived workKeys/revisions or user paths. Async failures use `turn.completed.error` plus safe `errorMessage`; alerts use tracing, HUD and suppression report. SSE failure never rolls back committed events; clients resume from high watermark. Write executable runbook commands using the Task 7 APIs.

- [ ] **Step 4: Run error and recovery tests**

Run: `cargo test -p tinyiothub-cloud suppression::error && cargo test -p tinyiothub-cloud suppression::audit`
Expected: all PASS, including no-leak and exact retry-count tests.

- [ ] **Step 5: Commit**

```bash
git add apps/cloud/src/domains/agent/session/suppression/ apps/cloud/src/domains/agent/session/wake.rs docs/runbooks/assistant-suppression.md
git commit -m "feat(agent): add actionable suppression recovery"
```

### Task 9: Overload Controls and Performance Evidence

**Files:**
- Modify: `apps/cloud/src/domains/agent/session/suppression/preflight.rs`
- Modify: `apps/cloud/src/domains/agent/session/trigger.rs`
- Modify: `apps/cloud/src/domains/agent/session/workspace_api.rs`
- Modify: `apps/cloud/benches/harness.rs` or add a focused existing-convention bench
- Test: owning modules and bench output artifact

**Interfaces:**
- Defaults: preflight concurrency 4 (1..=32), capture/resolve/total deadlines 2s/2s/4s.
- `wake_backpressure` returns 429 with retryAfterMs; scheduled wakes coalesce by `(workspaceId, plannedWakeAt)`.
- Off/shadow jitter is 0; canary/enforce may use deterministic 0..=15s workspace jitter after completion+interval.

- [ ] **Step 1: Write failing timeout, coalescing and scheduling tests**

```rust
#[tokio::test]
async fn hanging_adapter_times_out_without_skipping() {
    let result = fixture_with_hanging_source().preflight().await;
    assert_eq!(result.error_code(), Some("preflight_timeout"));
    assert!(!result.baseline_advanced());
    assert!(!result.in_flight());
}
```

Add semaphore saturation, same-wake coalescing, no unbounded queue, large-state hashing, replay storm, SQLite contention, off/shadow timing parity and canary/enforce completion+interval+jitter tests.

- [ ] **Step 2: Run tests and verify RED**

Run: `cargo test -p tinyiothub-cloud suppression_overload -- --nocapture`
Expected: FAIL because deadlines/backpressure/coalescing are absent.

- [ ] **Step 3: Implement overload controls and measurements**

Use Tokio semaphore/timeouts/cancellation and database idempotency; do not add task/work-item quotas. Incremental hashing avoids a second full buffer. Add synchronized-burst and pathological fixtures. Record machine, OS, Rust/SQLite versions, workspace/item sizes, p50/p95/p99, memory, DB busy and base-vs-branch interleaved results. No speedup claim from a single warm run.

- [ ] **Step 4: Run overload tests and benches**

Run: `cargo test -p tinyiothub-cloud suppression_overload`
Expected: all PASS. Run the focused bench and save the actual report under `.gstack/tmp/` without claiming unmeasured improvements.

- [ ] **Step 5: Commit**

```bash
git add apps/cloud/src/domains/agent/session/suppression/ apps/cloud/src/domains/agent/session/trigger.rs apps/cloud/src/domains/agent/session/workspace_api.rs apps/cloud/benches/
git commit -m "perf(agent): bound suppression preflight overload"
```

### Task 10: Shadow Metrics, Canary Evidence and Gate Artifacts

**Files:**
- Modify: `crates/db/src/suppression.rs`
- Modify: `apps/cloud/src/domains/agent/session/suppression/preflight.rs`
- Modify: `apps/cloud/src/domains/agent/session/workspace_api.rs`
- Modify: `apps/cloud/src/domains/agent/session/trigger.rs`
- Modify: `docs/runbooks/assistant-suppression.md`
- Test: owning modules/API tests

**Interfaces:**
- Produces: estimated shadow formulas with denominators and measured canary metrics.
- Produces: immutable gate artifact with sample IDs/dates, denominators, estimated/measured values, false-skip review, latency, approver and SHA-256.
- Enforces: shadow → canary → enforce evidence order.

- [ ] **Step 1: Write failing gate-validation tests**

```rust
#[test]
fn shadow_estimates_alone_cannot_enable_enforce() {
    let artifact = gate_fixture().with_shadow_only().with_would_skip(0.8);
    let error = validate_transition(SuppressionMode::Enforce, &artifact).unwrap_err();
    assert_eq!(error.code, "shadow_gate_not_met");
}
```

Add missing sample/denominator/approver, canary fewer than 20 Turns, unreviewed false-skip sample, threshold boundaries, stale artifact, and synthetic-fixture-only bypass prevention. Assert shadow execution/model/tool behavior is unchanged and all estimates are labeled estimated.

- [ ] **Step 2: Run tests and verify RED**

Run: `cargo test -p tinyiothub-cloud suppression_gate -- --nocapture`
Expected: FAIL because artifact persistence and transition validation are absent.

- [ ] **Step 3: Implement metrics and evidence-gated transitions**

Shadow estimates: actual model/tool calls on wouldSkip Turns divided by all corresponding shadow calls. Canary records actual model/tool reductions and human-labelled duplicate-report reduction. Require 3 representative days/100 shadow Turns and wouldSkip≥50%, then at least 20 canary Turns with ≥40% model/tool reduction, ≥80% duplicate-report reduction, p95≤250ms and no confirmed false skip after human review. Enforce validates the approved canary artifact; canary validates the shadow artifact. False-skip recovery returns to off and requires root-cause fix plus fresh evidence.

- [ ] **Step 4: Run gate tests**

Run: `cargo test -p tinyiothub-cloud suppression_gate && cargo test -p tinyiothub-cloud suppression_api`
Expected: all PASS; no real-world benefit is claimed from synthetic artifacts.

- [ ] **Step 5: Commit**

```bash
git add crates/db/src/suppression.rs apps/cloud/src/domains/agent/session/ apps/cloud/src/tests/ docs/runbooks/assistant-suppression.md
git commit -m "feat(agent): gate suppression rollout on measured evidence"
```

### Task 11: Source Author Quickstart and TTHW Evidence

**Files:**
- Create: `apps/cloud/examples/suppression_quickstart.rs`
- Create: `docs/dev/assistant-suppression.md`
- Modify: `docs/runbooks/assistant-suppression.md`
- Modify: `CHANGELOG.md`
- Test: quickstart smoke output under `.gstack/tmp/`

**Interfaces:**
- Uses: all public APIs and Source examples from earlier tasks.
- Provides: StaticSource, ChangingSource, in-memory SQLite, loopback-only server, `ws-suppression-demo`, synthetic `test-token`, and synthetic gate evidence.

- [ ] **Step 1: Write the failing clean-shell quickstart smoke expectation**

```text
shadow full -> wouldSkip=true
changed revision -> full request contains every registered Source
canary/enforce skip 1 -> modelCalls=0, toolCalls=0
canary/enforce skip 2 -> modelCalls=0, toolCalls=0
off + reconcile -> full Turn
```

Run the documented commands in a clean shell and capture the current failure. Every curl uses `--fail-with-body`; every wake/reconcile captures the returned turnId and polls report at 250ms intervals, at most 40 attempts/10 seconds.

- [ ] **Step 2: Run quickstart and verify RED**

Run: `cargo run -p tinyiothub-cloud --example suppression_quickstart`
Expected: FAIL because the example does not exist. Record this as a plan-stage failure, not a production bug.

- [ ] **Step 3: Implement fixture and documentation**

Implement only loopback synthetic behavior; no real provider/device/credential and no production gate bypass. Document complete StaticSource and ChangingSource examples, error catalog, API reference, migration/version matrix, recovery and rollback. Use the spec's exact commands, including jq validation and bounded terminal polling. Add CHANGELOG notes; no codemod is needed.

- [ ] **Step 4: Execute quickstart and measure both clocks**

Run: quickstart in a clean shell and save output. Verify all outcomes and zero-call counts. Measure dependency-ready time to first correct wouldSkip and clean-checkout time including build. Report both actual values; if not run, leave TTHW unknown and do not claim the 2–5 minute target was met.

- [ ] **Step 5: Commit**

```bash
git add apps/cloud/examples/suppression_quickstart.rs docs/dev/assistant-suppression.md docs/runbooks/assistant-suppression.md CHANGELOG.md
git commit -m "docs(agent): add executable suppression quickstart"
```

### Task 12: Final Regression, Review and Production Handoff

**Files:**
- Modify: only files required by test/review findings
- Read: full spec and final diff
- Artifact: final verification output under `.gstack/tmp/`

**Interfaces:**
- Produces: test evidence, not production approval.
- Requires: measured shadow/canary artifacts before any production enforce.

- [ ] **Step 1: Run focused and full regression suites**

```bash
cargo fmt --check
cargo clippy -p db -p tinyiothub-cloud --all-targets -- -D warnings
cargo test -p db
cargo test -p tinyiothub-cloud
cargo test -p tinyiothub-cloud --test runtime_eval
```

Expected: all PASS with no new warnings. Run the loopback quickstart once more and verify the final report has no raw refs or secrets. If a command cannot run, report its exact failure rather than claiming completion.

- [ ] **Step 2: Reconcile all 33 spec Success Criteria**

Map each criterion to its test output or explicitly mark it unmeasured. Confirm Phase 2 code is absent, old append/wake/schedule tests remain intact, and no unrelated working-tree files are staged. Run a fresh branch review covering durability, replay, redaction, async semantics and overload.

- [ ] **Step 3: Run production-readiness handoff**

Do not enable production enforce in this branch. Hand off the real shadow observation and canary runbook, including sample denominators, false-skip review and approval requirements. Record actual benefits and TTHW only after measurement. No commit, push or PR is created without a separate user request.

---

## Self-Review Notes

- Spec coverage: Tasks 1–12 map canonical contracts, six Sources, durable fences, preflight, full-input parity, atomic audit, replay, API/DTOs, errors, overload, evidence gates, quickstart and final verification. Phase 2 remains deferred.
- Type consistency: `CanonicalBytes`, `SuppressionError`, `SuppressionMode`, `SkipVeto`, `SourceRegistry`, `SuppressionService`, attempt IDs and gate artifacts use the same names across tasks.
- Proportion: test steps specify behavior and assertions; implementation bodies are intentionally left to the executor.
- Evidence boundaries: no test, benchmark, live provider, clean-checkout TTHW, production savings or false-skip measurement is claimed by this plan.
