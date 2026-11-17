# AI Visual Evidence and Unified Video Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add real ONVIF/RTSP and GB28181 video Things, alarm-linked AI JPEG evidence, and browser-direct live playback without creating a parallel device or evidence system.

**Architecture:** Extend the existing driver declarations, worker/DataServer/Job completion path, template wizard, tag tables, media evidence store, and detail page. ZLMediaKit handles media, WVP handles GB28181 SIP, and the existing cloud crate supplies a separately deployed transcoder process. Snapshot completion is gated on durable capture evidence; live playback uses short leases and does not proxy video through the Hub.

**Tech Stack:** Rust 2024, Axum, Tokio, SQLx/SQLite, Lit 3, Vite/Vitest, Playwright, ZLMediaKit, WVP-GB28181-pro, FFmpeg, Docker Compose.

**Spec:** `docs/designs/video-perception-streaming.md`

## Global Constraints

- Execute in a fresh worktree created with `superpowers:using-git-worktrees`; do not include unrelated changes from the current dirty worktree.
- Keep `ThingCommand.id = capture_id`; the driver `execute_async` boolean ABI and `DataKind::Video` model-input rejection remain unchanged.
- AI captures only through `call_device(action="snapshot")`; no `analyze_video` tool and no direct `read_device` capture path.
- Only exact `kind=location` tags participate; legacy tags default to `unclassified`. Multiple cameras run in `thingId ASC` order.
- Same `(workspace_id, camera_thing_id, request_id)` returns the same capture/evidence; a different fingerprint returns 409; only a new requestId captures again.
- No capture, playback, or investigation path may automatically clear an alarm or use stale/simulated images as fresh evidence.
- Keep existing limits: 2 MiB/image, 8 MiB/Run, one image/tool call. Add no fixed video-session quota; every wait has a deadline.
- WebRTC is the default. HLS stays disabled until playlist, segment, and key authorization all pass real browser tests.
- Rust orchestration stays in `apps/cloud` and `crates/*`; SQL stays in `crates/db`; migrations are DDL-only. Frontend tests require Node `v24.14.0`.
- All commits end with `Co-Authored-By: Claude Code <noreply@anthropic.com>`.

## Review Focus

- Late driver output after timeout/cancellation: preserve the original terminal state and quarantine mismatched bytes; pinned by Task 6.
- Legacy tags that normalize to the same location: reject classification conflicts rather than silently merging cameras; pinned by Task 2.
- Snapshot without a worker or DataServer: return `capture_failed`, never `executed` or `simulated`; pinned by Tasks 3 and 6.
- GB28181 deterministic auth/config errors versus temporary registration/RTP failures: only the latter retry; pinned by Task 5.
- Expiry racing an unresolved-alarm hold and the legacy hard-delete cleaner: retain a 410 tombstone and never bypass the hold; pinned by Task 9.
- HLS segment/key requests without the lease: return 401/403 and keep the UI in WebRTC-only mode; pinned by Tasks 11 and 12.

---

### Task 1: Driver declarations and event ABI

**Files:**
- Modify: `crates/core/src/driver/manifest.rs`
- Modify: `crates/core/src/driver/declaration.rs`
- Modify: `crates/core/src/driver/mod.rs`
- Modify: `crates/core/src/driver/dynamic.rs`
- Modify: `crates/runtime/src/driver/dynamic_adapter.rs`
- Modify: `spec/driver-manifest.schema.json`
- Test: `crates/core/tests/video_driver_declaration.rs`
- Test: `crates/runtime/src/driver/dynamic_adapter.rs` test module

**Interfaces:**
- Produces: `ActionProduces { source: String, data_kind: DataKind }`, `ManifestTool.produces: Option<ActionProduces>`.
- Produces: `DriverEventDecl { name, level, payload_schema }`, `DriverManifest.events` and `DeviceDeclaration.events` as defaulted vectors.
- Produces: `DriverEvent { event_id, name, level, payload, occurred_at }`, `DriverEventSink = mpsc::Sender<DriverEvent>`.
- Produces: `ThingDriver::set_event_sink(&mut self, _sink: DriverEventSink) {}` and additive `DRIVER_ABI_VERSION_V3`; V2 remains supported and cannot declare events.

- [ ] **Step 1: Write failing declaration and ABI tests**

```rust
#[test]
fn snapshot_produces_and_events_round_trip() {
    let manifest: DriverManifest = serde_json::from_value(json!({
        "manifestVersion":"1.1", "identity": test_identity_v3(),
        "tools":[{"name":"snapshot","kind":"action","safety":"safe",
                  "produces":{"source":"snapshot","dataKind":"image"}}],
        "events":[{"name":"motion_detected","level":"info","payloadSchema":{"type":"object"}}]
    })).unwrap();
    assert_eq!(manifest.tools[0].produces.as_ref().unwrap().source, "snapshot");
    assert_eq!(manifest.events[0].name, "motion_detected");
}

#[test]
fn v2_driver_cannot_declare_events() {
    assert!(validate_device_declaration(&v2_package(), &v3_declaration_with_events()).is_err());
}
```

- [ ] **Step 2: Run tests and confirm the new fields are absent**

Run: `cargo test -p tinyiothub-core --test video_driver_declaration`
Expected: FAIL on missing `produces` and `events`.

- [ ] **Step 3: Implement declarations, validation, JSON schema, and V3 event registration**

Keep all new fields optional/defaulted. Validate unique event names, object payload schemas, `produces` only on actions, and a matching declared Image Source. The V3 adapter registers the optional callback; V2 follows the existing adapter unchanged.

- [ ] **Step 4: Run declaration and dynamic adapter tests**

Run: `cargo test -p tinyiothub-core --test video_driver_declaration && cargo test -p tinyiothub-runtime dynamic_adapter`
Expected: PASS, including V2 compatibility and oversized/invalid event callback rejection.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/driver crates/runtime/src/driver/dynamic_adapter.rs spec/driver-manifest.schema.json crates/core/tests/video_driver_declaration.rs
git commit -m "feat: declare video action products and driver events" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 2: Normalized location tags and device discovery

**Files:**
- Create: `crates/db/migrations/20261117000001_video_tag_location.sql`
- Modify: `crates/db/src/tag.rs`
- Modify: `crates/db/src/thing.rs`
- Modify: `apps/cloud/src/domains/thing/tag/service.rs`
- Modify: `apps/cloud/src/domains/agent/tool/thing/device.rs`
- Test: `crates/db/tests/tag_location_tests.rs`
- Test: `apps/cloud/src/tests/tag_handler_tests.rs`

**Interfaces:**
- Consumes: existing `tags`, `tag_bindings`, and `ListDevices` tool.
- Produces: `normalize_location_tag(&str) -> Result<String, TagError>` using trim + Unicode NFC + Latin lowercase.
- Produces: `Db::list_thing_ids_by_location_tags(workspace_id, tags, match_all, after_thing_id, limit) -> Result<Vec<String>, sqlx::Error>` with deterministic `thingId ASC` keyset pagination.
- Produces: `list_devices(tags: string|string[], match: "all"|"any" = "all")`.

- [ ] **Step 1: Write failing normalization, conflict, and discovery tests**

```rust
#[tokio::test]
async fn location_discovery_is_exact_and_excludes_legacy_tags() {
    insert_tag("Camera", " Kitchen\u{00a0} ", "location", "cam-2").await;
    insert_tag("Sensor", "kitchen", "location", "smoke-1").await;
    insert_tag("Other", "kitchen", "unclassified", "cam-1").await;
    let ids = db.list_thing_ids_by_location_tags(WS, &["kitchen".into()], false, None, 50).await.unwrap();
    assert_eq!(ids, ["cam-2", "smoke-1"]);
}

#[tokio::test]
async fn duplicate_normalized_location_requires_explicit_resolution() {
    assert!(classify_second_kitchen_as_location().await.is_err());
}
```

- [ ] **Step 2: Run tests and confirm the new query is missing**

Run: `cargo test -p tinyiothub-storage --test tag_location_tests`
Expected: FAIL on missing `normalized_name`, `kind`, and query.

- [ ] **Step 3: Implement migration, transactional writes, indexes, and ANY/ALL keyset query**

Add `kind TEXT NOT NULL DEFAULT 'unclassified'` and `normalized_name TEXT NOT NULL`; backfill normalization only. Add a partial unique location index and discovery/binding indexes. Preserve legacy `type`; classify conflicts as errors. Wire the tool schema and service validation without LIKE filtering or offset pagination.

- [ ] **Step 4: Run DB and API tests**

Run: `cargo test -p tinyiothub-storage --test tag_location_tests && cargo test -p tinyiothub-cloud tag_handler_tests`
Expected: PASS for NFC/case/trim, ANY/ALL, dedupe, cursor, cross-workspace rejection, and legacy exclusion.

- [ ] **Step 5: Commit**

```bash
git add crates/db/migrations/20261117000001_video_tag_location.sql crates/db/src/tag.rs crates/db/src/thing.rs crates/db/tests/tag_location_tests.rs apps/cloud/src/domains/thing/tag apps/cloud/src/domains/agent/tool/thing/device.rs apps/cloud/src/tests/tag_handler_tests.rs
git commit -m "feat: discover video devices by normalized location tags" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 3: Capture identity, request replay, and status API

**Files:**
- Create: `crates/db/migrations/20261117000002_video_capture.sql`
- Create: `crates/db/src/media_capture.rs`
- Modify: `crates/db/src/lib.rs`
- Modify: `crates/core/src/media.rs`
- Create: `apps/cloud/src/domains/media/capture.rs`
- Modify: `apps/cloud/src/domains/media/mod.rs`
- Test: `crates/db/tests/video_capture_tests.rs`
- Test: `apps/cloud/src/domains/media/capture.rs` test module

**Interfaces:**
- Produces: `CaptureRequest { workspace_id, camera_thing_id, request_id, request_fingerprint, subject_thing_id, alarm_id, matched_tags, task_id, run_id, tool_call_id, job_id, source_id }`.
- Produces: `CaptureAttempt` and `CaptureReservation::{New(CaptureAttempt), Existing(CaptureAttempt), Conflict}` from `Db::reserve_capture(&CaptureRequest)`; `CaptureReservation::capture_id(&self) -> &str`.
- Produces: `Db::get_capture_in_workspace(workspace_id, capture_id)`.
- Produces: `GET /api/v1/workspaces/{id}/captures/{captureId}` returning state, evidence reference, and recovery metadata.

- [ ] **Step 1: Write failing reservation and API tests**

```rust
#[tokio::test]
async fn request_id_replays_one_capture_and_rejects_other_fingerprint() {
    let first = db.reserve_capture(&request("r1", "fp1")).await.unwrap();
    let capture_id = first.capture_id().to_string();
    assert_eq!(db.reserve_capture(&request("r1", "fp1")).await.unwrap().capture_id(), capture_id);
    assert!(matches!(db.reserve_capture(&request("r1", "fp2")).await, Err(MediaError::Conflict)));
}
```

- [ ] **Step 2: Run tests and confirm reservation is absent**

Run: `cargo test -p tinyiothub-storage --test video_capture_tests`
Expected: FAIL on missing `media_capture_attempts` and `reserve_capture`.

- [ ] **Step 3: Implement capture schema and reservation transaction**

Create the capture table with the spec fields, `response_json`, evidence `capture_attempt_id/status`, and visual analysis table. Enforce the request unique key. A transaction inserts or reads the existing row and compares fingerprints before any command/Job creation.

- [ ] **Step 4: Implement workspace-scoped capture status endpoint**

Return 404 across workspaces, 409 for reconciliation-required states, and RFC 9457 errors without URLs, secrets, or image bytes. `unknown` instructs reconciliation rather than recapture.

- [ ] **Step 5: Run capture and existing media tests**

Run: `cargo test -p tinyiothub-storage --test video_capture_tests && cargo test -p tinyiothub-cloud media::`
Expected: PASS for replay/conflict/concurrent reservation/status/redaction and existing media regressions.

- [ ] **Step 6: Commit**

```bash
git add crates/db/migrations/20261117000002_video_capture.sql crates/db/src/media_capture.rs crates/db/src/lib.rs crates/db/tests/video_capture_tests.rs crates/core/src/media.rs apps/cloud/src/domains/media
git commit -m "feat: persist idempotent video capture attempts" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 4: ONVIF/RTSP driver and ZLMediaKit client

**Files:**
- Create: `crates/runtime/src/media/zlm.rs`
- Create: `crates/runtime/src/driver/drivers/onvif_rtsp_driver.rs`
- Modify: `crates/runtime/src/driver/drivers/mod.rs`
- Modify: `crates/runtime/src/media/mod.rs`
- Create: `drivers/onvif-rtsp/driver.manifest.json`
- Test: `crates/runtime/src/driver/drivers/onvif_rtsp_driver.rs` test module
- Test: `crates/runtime/src/media/zlm.rs` test module

**Interfaces:**
- Consumes: Task 1 declarations and Task 3 capture ID in `ThingCommand.id`.
- Produces: `ZlmClient::{add_stream_proxy, wait_stream_ready, get_snap, delete_stream_proxy, close_sessions, list_sessions}` with deadline and redacted errors.
- Produces: `OnvifRtspDriver` implementing `describe/read_data/read_sources/execute_command`; `snapshot` returns one `SourceItem` with `data.captureId == cmd.id`.

- [ ] **Step 1: Write failing fake-ONVIF and fake-ZLM tests**

```rust
#[tokio::test]
async fn snapshot_returns_correlated_jpeg_and_cleans_proxy() {
    let fixture = VideoFixture::new().await;
    let item = fixture.driver.execute_command(&command("capture-1"), &cancel()).unwrap();
    assert!(item);
    let sources = fixture.driver.read_sources(&["snapshot".into()], &cancel()).unwrap();
    assert_eq!(sources[0].data["captureId"], "capture-1");
    assert_eq!(sources[0].media.as_ref().unwrap().mime, "image/jpeg");
    assert_eq!(fixture.zlm.deleted_proxy_count(), 1);
}
```

- [ ] **Step 2: Run tests and confirm the driver is missing**

Run: `cargo test -p tinyiothub-runtime onvif_rtsp_driver`
Expected: FAIL on missing driver module.

- [ ] **Step 3: Implement bounded ONVIF GetSnapshotUri and ZLM proxy/snapshot flows**

Use the existing HTTP dependency and JPEG validation. Validate every URI/redirect against configured camera CIDR/ports. Do not log credentials or signed query strings. Reuse an active matching playback stream; otherwise clean up the proxy on every exit path.

- [ ] **Step 4: Run driver and media-source tests**

Run: `cargo test -p tinyiothub-runtime onvif_rtsp_driver && cargo test -p tinyiothub-runtime media::`
Expected: PASS for success, timeout, auth failure, redirect rejection, stale capture ID, malformed JPEG, and proxy cleanup.

- [ ] **Step 5: Commit**

```bash
git add crates/runtime/src/media crates/runtime/src/driver/drivers drivers/onvif-rtsp
git commit -m "feat: add real ONVIF RTSP video driver" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 5: GB28181/WVP driver and failure classification

**Files:**
- Create: `crates/runtime/src/media/wvp.rs`
- Create: `crates/runtime/src/driver/drivers/gb28181_driver.rs`
- Modify: `crates/runtime/src/driver/drivers/mod.rs`
- Modify: `crates/runtime/src/media/mod.rs`
- Create: `drivers/gb28181/driver.manifest.json`
- Test: `crates/runtime/src/driver/drivers/gb28181_driver.rs` test module
- Test: `crates/runtime/src/media/wvp.rs` test module

**Interfaces:**
- Consumes: Task 1 declarations and Task 4 shared media conventions.
- Produces: `WvpClient::{wait_register, catalog, invite, bye}` with SIP IDs, channel, transport, and deadlines.
- Produces: `Gb28181Driver` with the same snapshot `SourceItem` contract as Task 4.
- Produces: `VideoFailure::{Retryable, Terminal}`; auth/schema/SSRF/unsupported are terminal, while timeout/not-ready/temporarily busy are retryable.

- [ ] **Step 1: Write failing SIP lifecycle and classification tests**

```rust
#[tokio::test]
async fn invite_snapshot_bye_preserves_capture_id() {
    let fixture = GbFixture::new().await;
    fixture.execute_snapshot("capture-gb1").await.unwrap();
    assert_eq!(fixture.calls(), ["REGISTER", "CATALOG", "INVITE", "SNAP", "BYE"]);
}

#[test]
fn deterministic_gb_failures_are_terminal() {
    assert_eq!(classify_gb(&digest_rejected()), VideoFailure::Terminal);
    assert_eq!(classify_gb(&rtp_timeout()), VideoFailure::Retryable);
}
```

- [ ] **Step 2: Run tests and confirm WVP support is absent**

Run: `cargo test -p tinyiothub-runtime gb28181_driver`
Expected: FAIL on missing WVP client/driver.

- [ ] **Step 3: Implement WVP SIP control and ZLM RTP/PS media flow**

Map WVP app/stream to the deterministic Thing stream. Always send BYE after a successful INVITE, including snapshot failure and cancellation. Never treat WVP rows as Thing ownership or historical registration as current health.

- [ ] **Step 4: Run GB and ONVIF regression tests**

Run: `cargo test -p tinyiothub-runtime gb28181_driver && cargo test -p tinyiothub-runtime onvif_rtsp_driver`
Expected: PASS for SIP sequencing, digest rejection, catalog timeout, INVITE rejection, RTP timeout, cancellation, and BYE cleanup.

- [ ] **Step 5: Commit**

```bash
git add crates/runtime/src/media crates/runtime/src/driver/drivers drivers/gb28181
git commit -m "feat: add GB28181 video driver with WVP SIP control" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 6: Two-stage snapshot completion and crash recovery

**Files:**
- Create: `crates/runtime/src/media/action_result.rs`
- Modify: `crates/runtime/src/driver/worker.rs`
- Modify: `crates/runtime/src/data_server.rs`
- Modify: `crates/runtime/src/job/service.rs`
- Modify: `apps/cloud/src/domains/media/capture.rs`
- Modify: `apps/cloud/src/domains/agent/tool/thing/invoke_action.rs`
- Modify: `crates/runtime/src/media/evidence.rs`
- Test: `crates/runtime/src/driver/worker.rs` test module
- Test: `crates/runtime/src/data_server.rs` test module
- Test: `apps/cloud/tests/video_snapshot_completion.rs`

**Interfaces:**
- Consumes: Tasks 1, 3–5.
- Produces: `SnapshotResult { capture_id, thing_id, source: SourceItem, elapsed_ms }` and `ActionResultStore::{publish, get}`.
- Produces: `CaptureService::complete_internal(SnapshotResult) -> Result<MediaAttachment, CaptureError>`; only its acknowledgement releases external Command/Job success.
- Produces: expected-state capture CAS and `.pending` object recovery with file + parent-directory fsync.

- [ ] **Step 1: Write failing no-premature-success and late-result tests**

```rust
#[tokio::test]
async fn external_success_waits_for_evidence_commit() {
    let fixture = SnapshotFixture::new().await;
    fixture.hold_evidence_commit().await;
    fixture.finish_driver_execute().await;
    assert!(!fixture.external_job_is_done().await);
    fixture.release_evidence_commit().await;
    assert_eq!(fixture.attachment().await.unwrap().evidence_id, fixture.expected_evidence());
}

#[tokio::test]
async fn late_success_cannot_flip_failed_capture() {
    fixture.fail_capture("capture-late").await;
    fixture.publish_late_result("capture-late").await;
    assert_eq!(fixture.capture_status("capture-late").await, "failed");
}
```

- [ ] **Step 2: Run integration tests and confirm premature completion**

Run: `cargo test -p tinyiothub-cloud --test video_snapshot_completion`
Expected: FAIL because generic `WorkerResult::Command` completes first.

- [ ] **Step 3: Implement worker post-read, internal result routing, and external completion acknowledgement**

After a declared snapshot succeeds, read its product once in the same worker/lease. Route the internal result to CaptureService instead of the generic completion sink. Non-snapshot behavior stays unchanged. No worker/DataServer/simulation returns `capture_failed`.

- [ ] **Step 4: Implement durable recovery and direct/Task replay**

Persist `.pending`, rename + fsync, then commit DB state. Recover no-byte/pending/final/DB-complete cases by capture ID without recapturing. Use CAS so failed/cancelled/succeeded cannot be overwritten by late output.

- [ ] **Step 5: Run completion and existing action/Job regression suites**

Run: `cargo test -p tinyiothub-cloud --test video_snapshot_completion && cargo test -p tinyiothub-cloud --test device_job_service && cargo test -p tinyiothub-runtime driver::worker`
Expected: PASS for deadlock prevention, no-worker failure, late results, all crash points, and non-snapshot behavior.

- [ ] **Step 6: Commit**

```bash
git add crates/runtime/src/media/action_result.rs crates/runtime/src/driver/worker.rs crates/runtime/src/data_server.rs crates/runtime/src/job crates/runtime/src/media/evidence.rs apps/cloud/src/domains/media apps/cloud/src/domains/agent/tool/thing/invoke_action.rs apps/cloud/tests/video_snapshot_completion.rs
git commit -m "feat: make snapshot success follow durable evidence" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 7: Creation intents, template migration, and secret handling

**Files:**
- Create: `crates/db/migrations/20261117000003_video_creation_intents.sql`
- Create: `crates/db/src/thing_creation_intent.rs`
- Create: `apps/cloud/src/domains/thing/video_creation.rs`
- Modify: `apps/cloud/src/domains/thing/handler/crud.rs`
- Modify: `crates/db/src/seed/system.sql`
- Modify: `crates/db/src/seed.rs`
- Test: `crates/db/tests/video_creation_intent_tests.rs`
- Test: `apps/cloud/src/tests/video_creation_tests.rs`
- Test: `crates/db/tests/seed_tests.rs`

**Interfaces:**
- Consumes: Tasks 4–6 and existing `createFromTemplate` wizard/API.
- Produces: `VideoCreationService::create(workspace_id, caller_key, CreateThingFromTemplateRequest) -> Result<ThingResponse, VideoCreationError>`.
- Produces: persistent states `validating/completed/failed_retryable/compensating/failed_terminal/abandoned`, deterministic Thing ID/stream, and startup `reconcile_intents()`.

- [ ] **Step 1: Write failing idempotency, migration, and failure-class tests**

```rust
#[tokio::test]
async fn same_creation_key_never_duplicates_thing_or_media_proxy() {
    let first = service.create(WS, "create-1", valid_request()).await.unwrap();
    let second = service.create(WS, "create-1", valid_request()).await.unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(fixture.thing_count(), 1);
    assert_eq!(fixture.proxy_create_count(), 1);
    assert!(service.create(WS, "create-1", changed_request()).await.is_err());
}
```

- [ ] **Step 2: Run tests and confirm the intent path is absent**

Run: `cargo test -p tinyiothub-cloud --test video_creation_tests`
Expected: FAIL on missing `thing_creation_intents` and service.

- [ ] **Step 3: Implement intent state machine and transactional Thing activation**

Reserve the intent before media side effects. Use a temporary driver instance to validate RTSP/GB28181 and take a verification JPEG before any active Thing or `call_device` exists; this is not a simulated capture. Recover/compensate by persisted stage, retain terminal tombstones, and only discard secrets after terminal states. Activate Thing/declarations/actions/sources in one DB transaction after verification.

- [ ] **Step 4: Migrate built-in templates and secret-bearing properties**

Use `builtin_ip_camera` as the sole generic ONVIF/RTSP creation template. Disable new `builtin-onvif-camera` creation, migrate `get_snapshot` with conflict checks, remove only unsupported built-in PTZ projections, and move `rtsp_url` secrets before deleting public values. Do not log URLs or alter custom templates.

- [ ] **Step 5: Run creation and seed migration suites**

Run: `cargo test -p tinyiothub-cloud --test video_creation_tests && cargo test -p tinyiothub-storage --test seed_tests --test video_creation_intent_tests`
Expected: PASS for duplicate/conflicting keys, transient/terminal failures, compensation, restart recovery, and legacy secret/action migration.

- [ ] **Step 6: Commit**

```bash
git add crates/db/migrations/20261117000003_video_creation_intents.sql crates/db/src/thing_creation_intent.rs crates/db/src/seed* crates/db/tests/video_creation_intent_tests.rs crates/db/tests/seed_tests.rs apps/cloud/src/domains/thing apps/cloud/src/tests/video_creation_tests.rs
git commit -m "feat: verify video media before activating Things" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 8: Driver event persistence and post-commit SSE

**Files:**
- Create: `crates/db/migrations/20261117000004_driver_event_dedup.sql`
- Modify: `crates/runtime/src/driver/worker.rs`
- Modify: `crates/runtime/src/data_server.rs`
- Modify: `crates/db/src/runtime_event.rs`
- Modify: `apps/cloud/src/domains/thing/handler/actions.rs`
- Test: `crates/runtime/src/acceptance/profiles/streaming.rs`
- Test: `apps/cloud/src/tests/thing_event_pipeline_tests.rs`

**Interfaces:**
- Consumes: Task 1 `DriverEventSink` and existing EventBus/persistence handler.
- Produces: `EventPersistOutcome::{Inserted, Duplicate}` and `persist_driver_event(registered_identity, DriverEvent) -> Result<EventPersistOutcome, DriverEventError>` with schema validation and `(workspace_id,event_id)` dedupe.
- Produces: post-commit SSE only; read order `occurred_at + event_id` per Thing.

- [ ] **Step 1: Write failing identity, dedupe, and commit-order tests**

```rust
#[tokio::test]
async fn event_is_deduped_and_sse_follows_commit() {
    let event = declared_event("motion-1");
    assert_eq!(persist(event.clone()).await.unwrap(), Inserted);
    assert_eq!(persist(event).await.unwrap(), Duplicate);
    assert_eq!(fixture.sse_count("motion-1"), 1);
    assert!(fixture.commit_time() < fixture.sse_time());
}
```

- [ ] **Step 2: Run event tests and confirm the new sink is unwired**

Run: `cargo test -p tinyiothub-cloud thing_event_pipeline_tests`
Expected: FAIL on missing driver-event dedupe/sink route.

- [ ] **Step 3: Implement bounded event ingestion and persistence**

Inject sink only for declared V3 drivers. Derive workspace/Thing from registered instances, validate payload schema, use bounded queues with drop metrics, and acknowledge after DB commit. Reject old ABI/unknown events without creating fake capabilities.

- [ ] **Step 4: Run runtime and cloud event regressions**

Run: `cargo test -p tinyiothub-runtime acceptance::profiles::streaming && cargo test -p tinyiothub-cloud thing_event`
Expected: PASS for dedupe, out-of-order delivery, restart during commit, forged identity, schema rejection, and explicit offline-loss metrics.

- [ ] **Step 5: Commit**

```bash
git add crates/db/migrations/20261117000004_driver_event_dedup.sql crates/db/src/runtime_event.rs crates/runtime/src/driver/worker.rs crates/runtime/src/data_server.rs crates/runtime/src/acceptance/profiles/streaming.rs apps/cloud/src/domains/thing apps/cloud/src/tests/thing_event_pipeline_tests.rs
git commit -m "feat: ingest declared video driver events" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 9: Hold-aware retention and evidence reconciliation

**Files:**
- Modify: `crates/db/src/media_evidence.rs`
- Modify: `crates/db/src/media_capture.rs`
- Modify: `crates/runtime/src/media/evidence.rs`
- Modify: `apps/cloud/src/domains/media/evidence.rs`
- Modify: `apps/cloud/src/domains/media/capture.rs`
- Test: `crates/db/tests/media_retention_tests.rs`
- Test: `apps/cloud/src/domains/media/capture.rs` test module

**Interfaces:**
- Consumes: Task 3 evidence fields and Task 6 recovery records.
- Produces: `expire_capture_media(now) -> Result<Vec<CaptureExpiry>, MediaError>` using hold checks, object deletion, then `status=expired` tombstone.
- Produces: `reconcile_capture(capture_id) -> Result<CaptureAttempt, CaptureError>` without another physical capture.

- [ ] **Step 1: Write failing hold/tombstone and legacy-cleaner tests**

```rust
#[tokio::test]
async fn hold_and_tombstone_survive_legacy_expiry_pass() {
    let evidence = fixture.succeeded_capture_with_open_alarm().await;
    fixture.run_legacy_and_capture_expiry().await.unwrap();
    assert_eq!(fixture.evidence_status(evidence).await, "active");
    fixture.resolve_alarm().await;
    fixture.run_legacy_and_capture_expiry().await.unwrap();
    assert_eq!(fixture.evidence_status(evidence).await, "expired");
    assert_eq!(fixture.api_status(evidence).await, StatusCode::GONE);
}
```

- [ ] **Step 2: Run retention tests and confirm hard deletion conflicts**

Run: `cargo test -p tinyiothub-storage --test media_retention_tests`
Expected: FAIL because `delete_expired_media` removes rows.

- [ ] **Step 3: Implement retention ownership and one capture-linked cleanup path**

Exclude capture-linked rows from legacy hard deletion. Preserve unrelated legacy media semantics. Handle standalone manual attempts, alarm/audit holds, tombstone expiry, and ordered metadata cleanup.

- [ ] **Step 4: Run retention and capture recovery suites**

Run: `cargo test -p tinyiothub-storage --test media_retention_tests && cargo test -p tinyiothub-cloud media::`
Expected: PASS for default expiry, hold release, manual owner, 410 responses, no dangling links, and no double deletion.

- [ ] **Step 5: Commit**

```bash
git add crates/db/src/media_evidence.rs crates/db/src/media_capture.rs crates/db/tests/media_retention_tests.rs crates/runtime/src/media/evidence.rs apps/cloud/src/domains/media
git commit -m "feat: preserve capture evidence through held retention" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 10: Multi-view investigation and visual evals

**Files:**
- Create: `apps/cloud/src/domains/agent/investigation/visual.rs`
- Create: `apps/cloud/src/domains/agent/investigation/mod.rs`
- Modify: `apps/cloud/src/domains/agent/mod.rs`
- Modify: `apps/cloud/src/domains/agent/tool/thing/invoke_action.rs`
- Modify: `crates/db/src/media_capture.rs`
- Modify: `apps/cloud/tests/runtime_eval.rs`
- Modify: `evals/scenarios.json`
- Test: `apps/cloud/tests/visual_investigation_tests.rs`

**Interfaces:**
- Consumes: Tasks 2–6 and existing alarm RunContext/skills.
- Produces: `discover_visual_subjects(subject_thing_id) -> Result<Vec<VisualCandidate>, VisualError>` with server-side tag freeze.
- Produces: `record_visual_analysis(...)` and `aggregate_visual_verdict(...)`; any `danger_supported` wins, uncertain/missing views require humans, and no result clears the alarm.

- [ ] **Step 1: Write failing multi-camera and aggregation tests**

```rust
#[tokio::test]
async fn mixed_views_preserve_danger_and_partial_failures() {
    let report = fixture.investigate(&["camera-a", "camera-b", "camera-c"]).await;
    assert_eq!(report.capture_order, ["camera-a", "camera-b", "camera-c"]);
    assert_eq!(report.visual_coverage, "partial");
    assert_eq!(report.verdict, "danger_supported");
    assert_eq!(report.alarm_status, "open");
}
```

- [ ] **Step 2: Run investigation tests and confirm the workflow is absent**

Run: `cargo test -p tinyiothub-cloud --test visual_investigation_tests`
Expected: FAIL on missing visual investigation module.

- [ ] **Step 3: Implement candidate discovery, per-camera calls, and separate result layers**

Persist candidates/attempt IDs, acquisition state, visual coverage, and per-view model verdicts. Do not accept alarm IDs or association metadata from model parameters. Non-vision providers reuse valid evidence for later analysis and never mark capture failed.

- [ ] **Step 4: Add visual eval scenarios and run unit/eval suites**

Add no-image baseline, image+provenance, AI+image, wrong-tag decoys, partial failure, non-vision, uncertain, and danger-supported cases. Run: `cargo test -p tinyiothub-cloud --test visual_investigation_tests --test runtime_eval`
Expected: PASS with no automatic clearance and every conclusion traceable to evidence.

- [ ] **Step 5: Commit**

```bash
git add apps/cloud/src/domains/agent/investigation apps/cloud/src/domains/agent/mod.rs apps/cloud/src/domains/agent/tool/thing/invoke_action.rs apps/cloud/tests/visual_investigation_tests.rs apps/cloud/tests/runtime_eval.rs crates/db/src/media_capture.rs evals/scenarios.json
git commit -m "feat: investigate alarms with multi-camera visual evidence" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 11: WebRTC playback leases and media hooks

**Files:**
- Create: `crates/db/migrations/20261117000005_video_playback.sql`
- Create: `crates/db/src/playback_lease.rs`
- Create: `apps/cloud/src/domains/media/lease.rs`
- Create: `apps/cloud/src/domains/media/zlm_playback.rs`
- Modify: `apps/cloud/src/domains/media/playback.rs`
- Modify: `apps/cloud/src/domains/media/types.rs`
- Modify: `apps/cloud/src/domains/media/mod.rs`
- Modify: `apps/cloud/src/state.rs`
- Test: `apps/cloud/src/domains/media/lease.rs` test module
- Test: `apps/cloud/src/domains/media/mod.rs` test module

**Interfaces:**
- Consumes: existing workspace/Thing/Source authorization and Task 4 `ZlmClient`.
- Produces: `PlaybackLeaseService::{issue, renew, revoke, reap_expired}` and `PlaybackLease { lease_id, workspace_id, thing_id, source_id, expires_at }`.
- Produces: `PlaybackProtocol::Webrtc` and descriptor `leaseId`; `POST .../playback/leases/{leaseId}/renew` and `DELETE` lease.
- Produces: ZLMediaKit `on_play` validation and active disconnect after expiry. HLS defaults off.

- [ ] **Step 1: Write failing lease and hook tests**

```rust
#[tokio::test]
async fn expired_lease_is_rejected_and_disconnected() {
    let lease = fixture.issue_webrtc().await.unwrap();
    fixture.expire(lease.id).await;
    assert_eq!(fixture.on_play(lease.id).await.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(fixture.zlm.closed_lease(lease.id), 1);
}

#[tokio::test]
async fn hls_is_not_issued_without_subresource_auth_proof() {
    assert_eq!(fixture.issue_hls().await.unwrap_err().code(), "protocol_unsupported");
}
```

- [ ] **Step 2: Run tests and confirm WebRTC/leases are absent**

Run: `cargo test -p tinyiothub-cloud media::lease`
Expected: FAIL on missing lease service/protocol.

- [ ] **Step 3: Implement lease persistence, provider, renewal, and reclamation**

Issue short Thing/Source-scoped credentials, cap expiry, renew with bounded requests, revoke on pause/tab close, and actively close sessions on expiry/restart. Validate URLs without logging tokens. Do not expose control APIs publicly.

- [ ] **Step 4: Run media API and authorization regressions**

Run: `cargo test -p tinyiothub-cloud media::`
Expected: PASS for issue/renew/expiry/revoke/restart, cross-workspace denial, HLS-off behavior, URL redaction, and existing descriptor tests updated only for the new lease contract.

- [ ] **Step 5: Commit**

```bash
git add crates/db/migrations/20261117000005_video_playback.sql crates/db/src/playback_lease.rs apps/cloud/src/domains/media apps/cloud/src/state.rs
git commit -m "feat: add leased WebRTC video playback" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 12: Device wizard, tag classification, and live player UI

**Files:**
- Create: `web/src/api/media.ts`
- Create: `web/src/ui/media/live-player.ts`
- Modify: `web/src/ui/views/things.ts`
- Modify: `web/src/ui/views/things-detail.ts`
- Modify: `web/src/ui/views/things-detail.test.ts`
- Modify: `web/src/ui/views/things-wizard.test.ts`
- Test: `web/src/ui/media/live-player.test.ts`
- Test: `web/e2e/video-device.spec.ts`

**Interfaces:**
- Consumes: Task 7 creation responses, Task 2 tag classification, and Task 11 lease API.
- Produces: existing wizard fields for RTSP/ONVIF or GB28181 parameters, visible `media_source_verified` and recoverable errors.
- Produces: `live-player` Lit element with `issue/renew/revoke` lifecycle and buffering/offline/unsupported/expired states.

- [ ] **Step 1: Write failing component and wizard tests**

```ts
it('shows live tab only for declared live sources', async () => {
  const host = await detailFixture({ sources: [{ id: 'live', dataKind: 'video' }] });
  expect(host.shadowRoot!.querySelector('[data-tab="live"]')).not.toBeNull();
  host.profile.sources = [];
  await host.updateComplete;
  expect(host.shadowRoot!.querySelector('[data-tab="live"]')).toBeNull();
});

it('revokes the lease when disconnected', async () => {
  const el = await playerFixture();
  el.remove();
  expect(renewMock).not.toHaveBeenCalled();
  expect(revokeMock).toHaveBeenCalledWith('lease-1');
});
```

- [ ] **Step 2: Run tests and confirm UI behavior is missing**

Run: `cd web && pnpm test -- src/ui/media/live-player.test.ts src/ui/views/things-detail.test.ts`
Expected: FAIL on missing live tab/player.

- [ ] **Step 3: Implement wizard mapping, tag controls, and live-player lifecycle**

Keep the existing two-step wizard. Show deterministic config/auth errors without secrets. Default WebRTC, mute audio, destroy on pause/tab/device exit, and never substitute a static frame for live playback. Show HLS only when the server explicitly enables it.

- [ ] **Step 4: Run Vitest and browser flow**

Run: `cd web && pnpm test && pnpm type-check && pnpm test:browser -- video-device.spec.ts`
Expected: PASS for valid/invalid creation, double-submit, tab visibility, lease renewal/expiry/revoke, and WebRTC-only presentation.

- [ ] **Step 5: Commit**

```bash
git add web/src/api/media.ts web/src/ui/media web/src/ui/views/things.ts web/src/ui/views/things-detail* web/src/ui/views/things-wizard.test.ts web/e2e/video-device.spec.ts
git commit -m "feat: add video device creation and live detail player" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 13: H.265 transcoder and pressure feedback

**Files:**
- Create: `apps/cloud/src/domains/media/transcoder.rs`
- Create: `apps/cloud/src/domains/media/pressure.rs`
- Create: `apps/cloud/src/bin/media_transcoder.rs`
- Create: `crates/db/migrations/20261117000006_video_transcode_jobs.sql`
- Modify: `apps/cloud/src/domains/media/lease.rs`
- Modify: `crates/db/src/playback_lease.rs`
- Test: `apps/cloud/src/domains/media/transcoder.rs` test module
- Test: `apps/cloud/src/domains/media/pressure.rs` test module

**Interfaces:**
- Consumes: Task 11 leases and the existing cloud crate, built as a separate `media-transcoder` process.
- Produces: `TranscodeSupervisor::{start_h264, stop, reap}` with persistent `transcode_jobs` rows keyed by `leaseId/thingId/sourceId/jobId`.
- Produces: `ResourcePressureMonitor::admit(WorkClass) -> Result<Admission, MediaBusy>` with deadline, `retry_after`, and alarm > inspection > discretionary playback priority.

- [ ] **Step 1: Write failing process and pressure tests**

```rust
#[tokio::test]
async fn expired_transcode_is_killed_and_reconciled() {
    let job = supervisor.start_h264(expired_lease()).await.unwrap();
    supervisor.reap().await.unwrap();
    assert_eq!(fixture.job_state(job.id), "stopped");
    assert!(!fixture.child_alive(job.pid));
}

#[tokio::test]
async fn preview_pressure_cannot_starve_alarm_capture() {
    fixture.fill_preview_queue().await;
    assert_eq!(monitor.admit(AlarmCapture).await.unwrap().priority(), "accepted");
    assert!(matches!(monitor.admit(DiscretionaryPlayback).await, Err(MediaBusy{..})));
}
```

- [ ] **Step 2: Run tests and confirm supervision is absent**

Run: `cargo test -p tinyiothub-cloud media::transcoder media::pressure`
Expected: FAIL on missing supervisor/monitor.

- [ ] **Step 3: Implement supervised FFmpeg and bounded pressure queues**

Use process groups, wall-clock/output limits, protocol allowlists, and kill-failure retry/alerts. H.264 avoids transcoding; H.265 snapshots remain supported. Keep unknown jobs registered until reconciliation confirms process exit.

- [ ] **Step 4: Run supervisor and lease lifecycle tests**

Run: `cargo test -p tinyiothub-cloud media::transcoder media::pressure media::lease`
Expected: PASS for normal exit, timeout, kill failure, Hub/supervisor restart, pressure priority, and no unbounded queue.

- [ ] **Step 5: Commit**

```bash
git add apps/cloud/src/domains/media apps/cloud/src/bin/media_transcoder.rs crates/db/src/playback_lease.rs crates/db/migrations/20261117000006_video_transcode_jobs.sql
git commit -m "feat: supervise H265 transcoding and media pressure" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 14: Compose deployment, egress, gates, and rollback

**Files:**
- Create: `deploy/docker/docker-compose.local.yml`
- Modify: `deploy/docker/docker-compose.yml`
- Create: `deploy/docker/zlm.ini`
- Create: `deploy/docker/wvp-config.yml`
- Create: `docs/runbooks/video-rollout.md`
- Modify: `.github/workflows/`
- Test: `apps/cloud/tests/video_deployment_config.rs`
- Test: `deploy/docker/tests/video-compose-smoke.sh`

**Interfaces:**
- Consumes: all previous runtime components and pinned ZLMediaKit/WVP commits and image digests.
- Produces: `video_creation_enabled`, `ai_visual_evidence_enabled`, `live_playback_enabled`, default false; Expand→Migrate→Enable→Contract rollout.
- Produces: private control-plane networks, camera CIDR/port egress allowlists, health checks, and license notices.

- [ ] **Step 1: Write failing deployment contract tests**

```rust
#[test]
fn compose_pins_media_and_keeps_control_plane_private() {
    let compose = Compose::load("deploy/docker/docker-compose.local.yml");
    assert!(compose.service("zlmediakit").image.digest().is_some());
    assert!(compose.service("wvp").image.digest().is_some());
    assert!(!compose.service("zlmediakit").ports.contains("8080:80"));
    assert_eq!(compose.env("HLS_ENABLED"), "false");
}
```

- [ ] **Step 2: Run configuration tests and confirm missing deployment**

Run: `cargo test -p tinyiothub-cloud --test video_deployment_config`
Expected: FAIL on missing local Compose/media configuration.

- [ ] **Step 3: Implement pinned Compose, network boundaries, and health checks**

Include reproducible RTSP H.264/H.265 and GB28181 fixtures, the separate transcoder process, exact CORS origins, and no public control API. Record ZLMediaKit supplementary identifiers and all required license texts; unresolved obligations block release.

- [ ] **Step 4: Implement backup/restore and rollback runbook**

Document gate order, in-flight intent/capture/lease drain, secret migration rollback, additive-schema retention, orphan reconciliation, and destructive-cleanup prohibition before rehearsal.

- [ ] **Step 5: Run Compose smoke and rollback checks**

Run: `bash deploy/docker/tests/video-compose-smoke.sh`
Expected: all health checks pass; create/snapshot/play/cleanup succeeds locally; disable gates stops new work without deleting evidence; no orphan proxy/session/process remains.

- [ ] **Step 6: Commit**

```bash
git add deploy/docker docs/runbooks/video-rollout.md .github/workflows apps/cloud/tests/video_deployment_config.rs
git commit -m "feat: add gated video deployment and rollback" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 15: Quickstart, schemas, error catalog, and operator docs

**Files:**
- Create: `docs/getting-started/video.md`
- Create: `docs/api/video.md`
- Create: `docs/api/video-errors.md`
- Create: `docs/runbooks/video-operations.md`
- Modify: `docs/guide/devices.md`
- Modify: `drivers/README.md`
- Test: `apps/cloud/tests/video_documentation.rs`
- Test: `deploy/docker/tests/video-quickstart.sh`

**Interfaces:**
- Consumes: implemented APIs, error codes, migration, and local Compose.
- Produces: versioned HTTP/tool examples, requestId recovery, tag semantics, gate configuration, and executable reconcile/runbook commands.

- [ ] **Step 1: Write failing documentation contract checks**

```rust
#[test]
fn error_catalog_covers_every_public_video_error_code() {
    let public = public_video_error_codes();
    let catalog = documented_error_codes("docs/api/video-errors.md");
    assert_eq!(public, catalog);
    for code in public { assert!(catalog[code].has_what_fix_recovery_and_redaction()); }
}
```

- [ ] **Step 2: Run docs checks and confirm missing coverage**

Run: `cargo test -p tinyiothub-cloud --test video_documentation`
Expected: FAIL on missing quickstart/error catalog entries.

- [ ] **Step 3: Write executable docs and examples**

Cover template creation, AI tools, WebRTC playback, configuration, retention/lease/deadline/retry/pressure, upgrade/rollback, and operations. State that 2–5 minute quickstart is a target until measured and that mock checks do not prove camera/model compatibility.

- [ ] **Step 4: Run quickstart and verify documentation timing**

Run: `bash deploy/docker/tests/video-quickstart.sh`
Expected: fresh local checkout reaches template device → WebRTC → snapshot → AI evidence and cleans up; record actual elapsed time and any failed stage instead of claiming the target.

- [ ] **Step 5: Commit**

```bash
git add docs/getting-started/video.md docs/api/video* docs/runbooks/video-operations.md docs/guide/devices.md drivers/README.md apps/cloud/tests/video_documentation.rs deploy/docker/tests/video-quickstart.sh
git commit -m "docs: add executable video quickstart and recovery" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 16: Full regression, real-stream, staging, and performance gates

**Files:**
- Create: `apps/cloud/tests/video_end_to_end.rs`
- Create: `web/e2e/video-device.spec.ts`
- Create: `evals/video-visual-cases.json`
- Create: `docs/runbooks/video-staging.md`
- Create: `docs/runbooks/video-performance.md`
- Modify: `TODOS.md` only for independently proven completions

**Interfaces:**
- Consumes: Tasks 1–15 and the saved Eng QA artifact `projects/Grong-tinyiothub/chenguorong-e4-p2-declaration-closeout-eng-review-test-plan-20261113-131611.md`.
- Produces: release evidence, not new behavior. No task is marked complete from mocks alone.

- [ ] **Step 1: Run Rust and frontend regression suites**

Run: `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace --lib --bins --tests`
Run: `cd web && pnpm type-check && pnpm lint && pnpm test && pnpm build`
Expected: PASS with nonzero relevant test counts; preserve unrelated pre-existing failures as separately documented evidence, never as fresh passes.

- [ ] **Step 2: Run the 26-path local integration/browser matrix**

Run: `cargo test -p tinyiothub-cloud --test video_end_to_end && cd web && pnpm test:browser -- video-device.spec.ts`
Expected: all creation/tag/snapshot/recovery/event/retention/playback/pressure paths pass using local real streams and real browser interactions.

- [ ] **Step 3: Run visual eval baselines and crash/pressure tests**

Run: `cargo test -p tinyiothub-cloud --test runtime_eval -- --nocapture`
Run: `cargo test -p tinyiothub-cloud --test video_snapshot_completion -- --nocapture`
Expected: no-image, image+provenance, AI+image comparisons are recorded; late results never flip terminal states; alarm capture retains priority; no auto-clear occurs.

- [ ] **Step 4: Run target-camera staging and pilot acceptance**

Follow `docs/runbooks/video-staging.md`: target RTSP H.264/H.265 and GB28181 devices, browser matrix, offline/occlusion/simulated-smoke cases, two reviewers plus adjudication, and no real fire. Record any danger-supported → no-danger result as a stop signal; do not generalize from the pilot.

- [ ] **Step 5: Measure performance and operational envelope**

Follow `docs/runbooks/video-performance.md`: record machine/engine/dataset/N, main-vs-branch interleaving, p50/p95, 10k tag bindings, same-key creation contention, recovery scans, lease expiry, transcoder RSS/CPU, bandwidth, and reclaim time. No capacity or latency claim without measurements.

- [ ] **Step 6: Commit verification artifacts only**

```bash
git add apps/cloud/tests/video_end_to_end.rs web/e2e/video-device.spec.ts evals/video-visual-cases.json docs/runbooks/video-staging.md docs/runbooks/video-performance.md TODOS.md
git commit -m "test: add video release evidence gates" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

## Execution Notes

- Dependency order is intentional: declarations → tags/capture → drivers → snapshot completion → creation/events/retention → investigation/playback/UI → deployment/docs → release evidence.
- Tasks 4 and 5 may proceed in parallel after Task 3. Tasks 8 and 9 may proceed after Task 6. Tasks 11 and 12 require their backend contracts before UI integration.
- Do not mark the whole design complete until Task 16 has actual local and staging evidence. If target hardware is unavailable, land tested code behind all three gates disabled and report the unverified matrix explicitly.
