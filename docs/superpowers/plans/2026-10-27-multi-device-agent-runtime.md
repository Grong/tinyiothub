# Multi-Device Agent Runtime Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build one generic, persistent multi-device Agent Runtime that advances Tasks through Jobs, watches, media evidence, and a two-column Workbench without scenario-specific branches.

**Architecture:** `agent` owns Task/Run coordination and wake routing, `runtime` owns Device Jobs and media mechanisms, `db` owns every SQL statement, and `apps/cloud` composes authenticated HTTP/SSE projections. The browser owns one ordered Runtime projection per selected Task; playback is browser-direct, and three default-off rollout flags preserve the legacy directive path until staged enablement.

**Tech Stack:** Rust 2024, Axum, Tokio, SQLite/sqlx, async-trait, tokio-util `CancellationToken`; Vite 7.3.1, Lit 3.3.2, TypeScript 5.7.3, Vitest 4.0.18, hls.js 1.7.3, Playwright for real-browser acceptance.

## Global Constraints

- Source of truth: `docs/superpowers/specs/2026-10-26-multi-device-agent-runtime-design.md`; do not reopen settled decisions without new evidence.
- **Approval decision:** R2 follow-up on 2026-10-27 approves durable decisions, frozen arguments, conditional decision CAS, a unique decision/execution outbox, and idempotent crash continuation. Never use the old in-memory `PendingActionStore` or heartbeat flip-then-execute path for Task approvals.
- **Delivery decision (D6, 2026-10-27):** continue implementation on the current `e4-p2-declaration-closeout` branch using PR #101's declaration baseline `6876707f`; expand the existing PR #101 to include the declaration closeout and complete Runtime in one final PR. Do not wait for PR #101 to merge first and do not create a second Runtime PR. Preserve the staged commits below and all three default-off flags. Push/merge authorization remains separate.
- Task states: `active | waiting_job | waiting_watch | needs_approval | paused | cancelling | completed | failed | cancelled`.
- Device Job states: `accepted | running | done | failed | rejected | cancelled | cancelled_unknown | interrupted`; no Job pause/resume and no percentage progress.
- Task pause blocks later Runs and never aborts the current Run, steers it, or stops Jobs/watches. Cancel aborts the shared Runner/rig token, blocks new tool dispatch, then resolves watches/outbox/Jobs; any `cancelled_unknown` yields `failed(cancel_unknown)`.
- `task_wake_outbox` and Task state changes share one DB transaction. `agent_runs(task_id,wake_event_id)` is the second deduplication layer; `RunStarted` and `RunReport` carry the same `wake_event_id`.
- Approval display/execution share frozen `tool/args_json`. Approve CAS and its unique `decision_event_id` outbox commit atomically; reject creates no DeviceJob. Resume approved work through the original `tool_call_id` and existing Job/wake uniqueness; unknown physical outcomes never auto-replay.
- Single-Job Stop uses `job_id` through `WorkerCommand::Execute`, `CancelSlot`, and `WorkerResult::Cancelled`. Never substitute `DataServer::cancel_calls(thing_id)` or `ArbitrationRegistry::set_stop`.
- All SQL stays in `crates/db`; dependency direction remains `apps/* → crates/* → core`. No new `utils/`, `helpers/`, `application/`, `dto.rs`, or re-export shims.
- Keep `ChatMessage.role/content` textual and add default-empty structured attachments. Trace stores evidence references, never base64.
- Media: `manifestVersion=1.1`; `dataKind` defaults to `values`; JPEG/PNG only, 2 MiB/image, one image/tool call, 8 MiB/Run; evidence retention 7 days.
- Playback descriptor ticket: 5 minutes. Signed HTTPS only except local `http://localhost`; reject userinfo and `file/javascript/data`. Never log URL query, credentials, objective text, image bytes, or Authorization.
- No new fixed Task/device/outbox/video/process-image quotas and no total Task-prompt byte cap. Preserve `describe()` 256 KiB, `SNAPSHOT_MAX_BYTES=6144`, `HOURLY_WAKE_LIMIT=20`, existing Run budgets, and use backpressure/metrics/alerts rather than new hard caps.
- Flags in `workspaces.heartbeat_config`: `runtime_tasks_enabled`, `runtime_media_enabled`, `runtime_workbench_enabled`, all default false. Enable Task → Media → Workbench. Task flag off only selects legacy creation; it never mutates existing Tasks.
- Rollback requires explicit drain and unknown resolution before hiding Workbench/Media. Flags are not physical kill switches.
- Frontend API calls stay under `web/src/api/`; components never call `fetch()` directly. Use Node 24 for every web install/build/test command.
- Preserve light/dark themes, 980/600/400px responsive contracts, keyboard/focus behavior, reduced motion, and WCAG AA. Dedicated Runtime fonts and WebVTT captions remain deferred; label `Captions unavailable` for video.
- Production Rust uses no `unwrap()`/`expect()`. Tests may use `expect()` with a diagnostic. Every future commit step is an instruction for execution, not something performed while writing this plan.

## File Structure and Ownership

| Area | Create | Modify | Responsibility |
|---|---|---|---|
| Shared values | `crates/core/src/{agent_task,device_job,task_approval}.rs` | `crates/core/src/lib.rs`, `crates/core/src/agent_runs.rs`, `crates/core/src/driver/manifest.rs` | Task/Job/Approval values, Run correlation, additive Source declarations |
| Task engine | `crates/agent/src/port/{task,approval}.rs`; `crates/agent/src/runtime/thing_agent/task/{mod,state,coordinator,wake,prompt,approval}.rs` | `runner.rs`, `active_runs.rs`, `manager.rs`, `runtime/events.rs`, `port/{provider,tool,events}.rs`, `adapters/rig/{provider,loop_}.rs` | CAS transitions, wake dedupe, approval execution recovery, cancellation, prompt, attachments |
| Job engine | `crates/runtime/src/job/{mod,service,worker_adapter}.rs` | `crates/runtime/src/driver/worker.rs`, `data_server.rs`, `ports.rs`, `lib.rs` | accepted/running/final lifecycle, real deadline, job-scoped stop |
| Media engine | `crates/runtime/src/media/{mod,source,evidence,playback}.rs` | `crates/runtime/src/ports.rs`, `lib.rs` | SourceItem, image validation, dedupe, local MediaStore, descriptor safety |
| Persistence | `crates/db/src/{runtime_task,device_job,task_approval,media_evidence,runtime_event}.rs`; migrations `20261027000001_runtime_tasks.sql`, `20261027000002_device_jobs.sql`, `20261027000003_media_evidence.sql`, `20261027000004_task_approvals.sql` | `crates/db/src/{lib,database,agent_runs,watch,heartbeat}.rs` | Additive DDL, Rows, transactions, Db facade, flag defaults |
| Cloud | `apps/cloud/src/domains/agent/runtime/{mod,tasks,controls,approvals,projection,stream,admin,types}.rs`; `apps/cloud/src/domains/media/{mod,evidence,playback,types}.rs` | `apps/cloud/src/domains/agent/{mod,host/handler/agent_tasks}.rs`, `apps/cloud/src/api/mod.rs`, composition root | Authenticated handlers, approvals, projections, cursor SSE, admin recovery |
| Web API/model | `web/src/types/runtime.ts`; `web/src/api/{runtime,runtime-stream,playback}.ts`; `web/src/ui/views/{runtime-projection,runtime-url-state}.ts` | `web/src/api/client.ts`, `web/src/ui/app.ts`, `web/package.json`, lockfile | Typed transport, idempotency, hash routes, one projection owner |
| Workbench | `web/src/ui/views/runtime-{list,new,task,loop,devices,flow,playback,task-controls,approval,confirm-dialog}.ts` and colocated CSS/tests | Existing AI-center link only | List/create/detail, Loop/Devices/Flow, approvals, history and controls |
| Acceptance/ops | `web/e2e/runtime.spec.ts`, `web/playwright.config.ts`; `crates/agent/tests/fixtures/runtime_{a,b}/`; `docs/runbooks/runtime-rollout.md` | CI Node/browser setup and existing eval entry points | Two-fixture generality, real browser QA, metrics, rollout/drain |

## Task Dependency Graph

`T1 → T2 → T3 → T4 → T5 → T6 → T6A → T7A → T7 → T7B → T7C → T8`.

T4 media types may be developed in parallel with T2/T3 only by the same integrator who rebases the final interfaces before T5. The reviewed commit order remains sequential.

## Test Fixture Contracts

The snippets below use these test-only builders. Define them inside the listed test modules; do not add production helper modules or mock the behavior under test. Rust fixtures use the real additive migrations on isolated SQLite databases, and media fixtures use an isolated temporary directory. IDs, URLs, tokens, and image bytes are synthetic.

- T1: `sample_task(id,workspace,key,objective)` constructs an `active` `AgentTask` at version 0 with fixed timestamps; `sample_wake(task,event,workspace)` serializes a valid `WakeSignal` for that workspace. Reuse the existing `test_pool()`. `runtime_test_app(flags)` builds the real Router with recording legacy/runtime sinks, and `post_task` sends the optional `Idempotency-Key` verbatim. `runtime_fixture_with_task(status)` additionally seeds one Task and exposes its stored state and legacy call count.
- T2: `JobFixture` owns a real `Db`, `JobService`, and recording fake worker; `accepted_job` persists an owned accepted Job, `job` reads it back, `wake_count` counts real outbox rows, and `cancelled_job_ids` returns worker assertions in request order. `WorkerFixture::paused_execution` gates dequeue; `blocking_driver` creates two independent worker/driver pairs whose driver calls wait on a gate and expose cancellation tokens. `job_execute(job_id,tool)` constructs `WorkerCommand::Execute` with that exact optional ID and a completion sender; `enqueue/start/release_execution/next_cancelled` operate those channels, never a real device.
- T3: `CoordinatorFixture` uses the real coordinator/store/registry with a scripted provider and recording runner/tool dispatcher. `running_task` seeds one active Run and Job; `model_at_tool_boundary` pauses the scripted model immediately before its first tool dispatch. `active_token` reads the registry token, `release_model_tool_boundary` opens the gate, and `emit_job_final`, `persisted_wake`, `running_job`, `execute_calls`, and `tool_dispatch_count` inspect real stores or recording doubles.
- T4: `EvidenceFixture::new()` returns an `EvidenceService` backed by the real temporary store plus test row/object counters. `image_input(mime,size)` creates declared-size input with otherwise valid metadata; magic-byte tests supply actual minimal JPEG/PNG data separately. `tiny_valid_png()` returns a checked-in tiny PNG byte array, `fixed_time()` returns a fixed UTC timestamp, `first_tool_content` selects the first converted rig tool content, and `image_request()` builds one request containing that attachment. `ScriptedProvider::text_only()` advertises no vision capability and records that no image claim was emitted.
- T5/T6: `MediaApiFixture` seeds `ws_1/evidence_1` and exposes real GET responses plus an expiry update. `RuntimeApiFixture` seeds `ws_1/task_1/job_1`, reader/admin identities, and counting coordinator/job adapters. `StreamFixture` owns the real ledger/Router SSE connection; `control_event(event_id,cursor)` creates a `runtime.v1` control envelope, and `append/connect/next_json/next_timeout` perform actual stream I/O.
- T6A: `ApprovalFixture` owns the real migrated Db, coordinator/executor, recording JobService, and injectable clock. `pending_approval()` seeds one Task/Run/tool call with frozen arguments and a preallocated decision event; `decide`, `approve`, `reject`, `expire`, `recover`, `pause_task`, `resume_task`, `dispatch_decision`, `outbox_dispatched`, `job_count`, and `outbox_count` exercise real persistence and routes. `CrashPoint` is `Copy` and covers before decision commit, after commit/before dispatch, after dispatch/before Job insert, and after Job insert/before wake mark. `drive_to_crash(point, pending)` arms that fault before running the decision/dispatch/Job/wake-mark pipeline and returns the injected error; it never simulates a successful physical device call.
- Web: `snapshotWithNoDevices`, `event`, `projectionWithRunningJob`, `projectionFromFixtureB`, `liveTask`, `runningTaskWithJobs`, `pendingApproval`, `approvedProjection`, and `liveEvent` are plain typed object builders. `renderRuntimeDevices` mounts the real element. `controlsFixture`, `jobCardFixture`, `taskFixture`, `approvalFixture`, and `workbenchFixture` mount real components with API spies and return the exact DOM handles/spies referenced by their tests. `approvalFixture` holds a deferred decision response, exposes `resolveDecision`, and implements `switchTask` through the real parent generation/abort path; these fixtures do not add fields such as `locateEvents` to production components. `safeJson`, `clamp`, `associatedTools`, and `taskStatusLabel` are small exported pure functions in the component/projection modules that use them.
- T8: `RuntimeFixture::load(name)` reads only that fixture's `manifest.json/task.json`, starts the shared Runtime with scripted model responses selected by declared tool arguments, and returns the real terminal Task/Jobs/wake IDs/tool calls; `expected_tools()` comes from `task.json`. `EvalFixture::large_task_with_unordered_facts()` seeds multiple insertion orders of the same pending Job/watch/approval facts, exposes `task/runs/jobs/watches/approvals/snapshot`, all pending IDs and the latest stage result, and calls the production `build_task_prompt` signature shown in T3.

---

### Task 1 (T1): Persistent Task Creation and Idempotency

**Files:**
- Create: `crates/core/src/agent_task.rs`, `crates/agent/src/port/task.rs`, `crates/db/src/runtime_task.rs`, `crates/db/migrations/20261027000001_runtime_tasks.sql`
- Create: `apps/cloud/src/domains/agent/runtime/{mod,tasks,types}.rs`
- Modify: `crates/core/src/lib.rs`, `crates/agent/src/port/mod.rs`, `crates/db/src/{lib,database,agent_runs,heartbeat}.rs`, `apps/cloud/src/domains/agent/{mod,host/handler/agent_tasks}.rs`
- Test: `crates/db/src/runtime_task.rs` test module, `apps/cloud/src/domains/agent/host/handler/tests/mod.rs`

**Interfaces:**
- Produces: `AgentTask`, `TaskStatus`, `TaskWake`, `TaskStore`, `CreateTaskOutcome`, `TaskStoreError`.
- `TaskStore::create_task_with_wake(&self, task: &AgentTask, wake: &TaskWake) -> Result<CreateTaskOutcome, TaskStoreError>`.
- `TaskStore::get_task`, `list_tasks`, `wake_by_id`, `transition_task`, `bind_current_run`, `begin_run`, `finish_run`, `claim_pending_wakes`, `mark_wake`, `retry_dead_wake`, and `list_nonterminal_tasks` are the only Task persistence surface used below.
- HTTP: `POST /workspaces/{id}/agent/tasks` requires `Idempotency-Key`; GET list/detail return persisted Task projections.

- [ ] **Step 1: Write failing Task creation and concurrent replay tests**

```rust
#[tokio::test]
async fn same_key_concurrent_create_returns_one_task_and_one_wake() {
    let pool = crate::test_helpers::test_pool().await;
    let db = Db::new(pool);
    let task = sample_task("task_1", "ws_1", "key_1", "inspect both pumps");
    let wake = sample_wake("task_1", "event_1", "ws_1");
    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let mut joins = Vec::new();
    for _ in 0..2 {
        let db = db.clone();
        let task = task.clone();
        let wake = wake.clone();
        let barrier = barrier.clone();
        joins.push(tokio::spawn(async move {
            barrier.wait().await;
            db.create_task_with_wake(&task, &wake).await.expect("create")
        }));
    }
    let a = joins.remove(0).await.expect("first join");
    let b = joins.remove(0).await.expect("second join");
    assert_eq!(a.task.id, b.task.id);
    assert_ne!(a.deduplicated, b.deduplicated);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_wake_outbox")
        .fetch_one(&db.pool()).await.expect("wake count");
    assert_eq!(count, 1);
}

#[tokio::test]
async fn same_key_different_objective_conflicts_without_new_wake() {
    let pool = crate::test_helpers::test_pool().await;
    let db = Db::new(pool);
    let wake = sample_wake("task_1", "event_1", "ws_1");
    db.create_task_with_wake(&sample_task("task_1", "ws_1", "key_1", "A"), &wake)
        .await.expect("first create");
    let error = db.create_task_with_wake(&sample_task("task_2", "ws_1", "key_1", "B"), &wake)
        .await.expect_err("objective conflict");
    assert!(matches!(error, TaskStoreError::ObjectiveConflict));
}
```

- [ ] **Step 2: Run the tests and confirm the missing-schema failure**

Run: `cargo test -p tinyiothub-storage runtime_task::tests -- --nocapture`
Expected: FAIL because `agent_tasks` and `task_wake_outbox` do not exist; no test may be weakened to use ad hoc test-only tables.

- [ ] **Step 3: Add the additive Task migration and shared values**

```sql
ALTER TABLE agent_runs ADD COLUMN task_id TEXT;
ALTER TABLE agent_runs ADD COLUMN wake_event_id TEXT;
CREATE INDEX idx_agent_runs_task ON agent_runs(task_id);
CREATE UNIQUE INDEX idx_agent_runs_task_wake
ON agent_runs(task_id, wake_event_id)
WHERE task_id IS NOT NULL AND wake_event_id IS NOT NULL;

CREATE TABLE agent_tasks (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    idempotency_key TEXT NOT NULL,
    objective TEXT NOT NULL,
    session_key TEXT,
    created_by TEXT NOT NULL,
    status TEXT NOT NULL,
    current_run_id TEXT,
    state_version INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    ended_at TEXT
);
CREATE UNIQUE INDEX idx_agent_tasks_idempotency
ON agent_tasks(workspace_id, idempotency_key);
CREATE INDEX idx_agent_tasks_status_activity
ON agent_tasks(workspace_id, status, updated_at);

CREATE TABLE task_wake_outbox (
    task_id TEXT NOT NULL REFERENCES agent_tasks(id),
    event_id TEXT NOT NULL,
    signal_json TEXT NOT NULL,
    dispatch_state TEXT NOT NULL,
    created_at TEXT NOT NULL,
    dispatched_at TEXT,
    resolved_at TEXT,
    PRIMARY KEY(task_id, event_id)
);
CREATE INDEX idx_task_wake_pending
ON task_wake_outbox(dispatch_state, created_at);

ALTER TABLE watch_rules ADD COLUMN owner_task_id TEXT REFERENCES agent_tasks(id);
CREATE INDEX idx_watch_rules_owner_task ON watch_rules(owner_task_id);
```

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Active, WaitingJob, WaitingWatch, NeedsApproval, Paused,
    Cancelling, Completed, Failed, Cancelled,
}
impl TaskStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
    pub fn is_active(self) -> bool { !self.is_terminal() }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentTask {
    pub id: String,
    pub workspace_id: String,
    pub idempotency_key: String,
    pub objective: String,
    pub session_key: Option<String>,
    pub created_by: String,
    pub status: TaskStatus,
    pub current_run_id: Option<String>,
    pub state_version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WakeDispatchState { Pending, Dispatched, Dead }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskWake {
    pub task_id: String,
    pub event_id: String,
    pub signal_json: String,
    pub dispatch_state: WakeDispatchState,
    pub created_at: DateTime<Utc>,
    pub dispatched_at: Option<DateTime<Utc>>,
    pub resolved_at: Option<DateTime<Utc>>,
}
```

Register `pub mod agent_task;` in core. Reject unknown DB status strings with `TaskStoreError::Unavailable`, never default them to `completed`.

- [ ] **Step 4: Implement the TaskStore port and one-transaction SQLite adapter**

```rust
#[async_trait]
pub trait TaskStore: Send + Sync {
    async fn create_task_with_wake(&self, task: &AgentTask, wake: &TaskWake)
        -> Result<CreateTaskOutcome, TaskStoreError>;
    async fn get_task(&self, workspace_id: &str, task_id: &str)
        -> Result<Option<AgentTask>, TaskStoreError>;
    async fn list_tasks(&self, workspace_id: &str, limit: i64, offset: i64)
        -> Result<Vec<AgentTask>, TaskStoreError>;
    async fn wake_by_id(&self, task_id: &str, event_id: &str)
        -> Result<Option<TaskWake>, TaskStoreError>;
    async fn transition_task(&self, workspace_id: &str, task_id: &str,
        expected_version: i64, next: TaskStatus, wake: Option<&TaskWake>)
        -> Result<AgentTask, TaskStoreError>;
    async fn bind_current_run(&self, task_id: &str, expected_version: i64,
        run_id: Option<&str>) -> Result<AgentTask, TaskStoreError>;
    async fn begin_run(&self, task: &AgentTask, wake_event_id: &str, run_id: &str)
        -> Result<bool, TaskStoreError>;
    async fn finish_run(&self, report: &RunReport) -> Result<(), TaskStoreError>;
    async fn claim_pending_wakes(&self, limit: i64) -> Result<Vec<TaskWake>, TaskStoreError>;
    async fn mark_wake(&self, task_id: &str, event_id: &str,
        state: WakeDispatchState) -> Result<(), TaskStoreError>;
    async fn retry_dead_wake(&self, workspace_id: &str, event_id: &str)
        -> Result<bool, TaskStoreError>;
    async fn list_nonterminal_tasks(&self, workspace_id: Option<&str>)
        -> Result<Vec<AgentTask>, TaskStoreError>;
}

pub struct CreateTaskOutcome { pub task: AgentTask, pub deduplicated: bool }
pub enum TaskStoreError { Validation(String), ObjectiveConflict,
    ConcurrentUpdate, Unavailable(String) }
```

Implement `create_task_with_wake` with `SqliteConnection` transaction: validate `task.workspace_id == parsed_signal.workspace_id`, attempt both inserts, and on the idempotency unique conflict read the original Task inside the transaction. Return the original only when its objective is byte-identical; otherwise `ObjectiveConflict`. `transition_task` must use `WHERE workspace_id=? AND id=? AND state_version=?`, increment `state_version`, set `ended_at` only for terminal states, and insert the optional wake before commit. Zero affected rows maps to `ConcurrentUpdate`. Keep every query in `runtime_task.rs` and expose `Db` delegate methods.

- [ ] **Step 5: Run persistence tests and migration integrity checks**

Run: `cargo test -p tinyiothub-storage runtime_task::tests migrations::tests -- --nocapture`
Expected: PASS, including concurrent replay, conflict, stale-version zero-write, same-transaction rollback, and `PRAGMA foreign_key_check` returning zero rows.

- [ ] **Step 6: Add failing HTTP tests for required key, replay, and legacy flag fallback**

```rust
#[tokio::test]
async fn persistent_create_requires_idempotency_key() {
    let app = runtime_test_app(RuntimeFlags { tasks: true, ..Default::default() }).await;
    let response = post_task(&app, "inspect pump", None).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(count_agent_tasks(&app).await, 0);
}

#[tokio::test]
async fn task_flag_off_preserves_legacy_directive_and_existing_task() {
    let fixture = runtime_fixture_with_task(TaskStatus::WaitingJob).await;
    let response = post_task(&fixture.app, "legacy", Some("new-key")).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(fixture.legacy_sink.calls(), 1);
    assert_eq!(fixture.stored_task().await.status, TaskStatus::WaitingJob);
}
```

Add replay assertions: two identical requests return the same `taskId`, the second has `deduplicated:true`, and only one outbox row exists; changed objective returns 409; no provider call is made by these HTTP tests.

- [ ] **Step 7: Implement flags and persistent Task handlers while preserving legacy creation**

```rust
#[derive(Debug, Clone, Copy, Default)]
pub struct RuntimeFlags {
    pub tasks: bool,
    pub media: bool,
    pub workbench: bool,
}
impl RuntimeFlags {
    pub fn from_heartbeat_json(value: &serde_json::Value) -> Self {
        Self {
            tasks: value.get("runtime_tasks_enabled").and_then(Value::as_bool).unwrap_or(false),
            media: value.get("runtime_media_enabled").and_then(Value::as_bool).unwrap_or(false),
            workbench: value.get("runtime_workbench_enabled").and_then(Value::as_bool).unwrap_or(false),
        }
    }
}
```

Extract `TypedHeader<IdempotencyKey>` using a local `HeaderName::from_static("idempotency-key")` extractor or `HeaderMap`, reject missing/blank keys and blank trimmed objectives with 400. When `tasks=false`, execute the existing `UserDirective` branch unchanged and do not require the new key on that branch. When enabled, persist `objective=text.trim()`, `created_by=claims.user_id`, initial `active`, and one pending wake; return `ApiResponseBuilder::success(json!({"taskId": outcome.task.id, "status":"accepted", "deduplicated": outcome.deduplicated}))`. Map objective conflict to 409 and unavailable storage to 503. Mount GET list/detail with workspace read access; reuse the existing admin guard for create. Approval routes are deferred to their dedicated T6A vertical slice.

- [ ] **Step 8: Verify Task 1 and commit the staged slice**

Run: `cargo test -p tinyiothub-storage runtime_task -- --nocapture && cargo test -p tinyiothub-cloud agent_tasks -- --nocapture`
Expected: PASS; missing key 400, replay idempotent, objective conflict 409, flag-off legacy behavior unchanged.

```bash
git add crates/core/src/agent_task.rs crates/core/src/lib.rs crates/agent/src/port crates/db/src crates/db/migrations/20261027000001_runtime_tasks.sql apps/cloud/src/domains/agent
git commit -m "feat(runtime): add persistent idempotent tasks"
```

### Task 2 (T2): Device Jobs, Real Deadlines, and Job-Scoped Stop

**Files:**
- Create: `crates/core/src/device_job.rs`, `crates/runtime/src/job/{mod,service,worker_adapter}.rs`, `crates/db/src/device_job.rs`, `crates/db/migrations/20261027000002_device_jobs.sql`
- Modify: `crates/core/src/lib.rs`, `crates/runtime/src/{lib,ports,driver/worker,data_server}.rs`, `crates/db/src/{lib,database}.rs`
- Test: `crates/runtime/src/job/service.rs` and `crates/runtime/src/driver/worker.rs` test modules; `crates/db/src/device_job.rs` test module

**Interfaces:**
- Consumes: `TaskStore` correlation IDs and `ValidatedCall.timeout_ms` from the existing declaration path.
- Produces: `DeviceJob`, `DeviceJobState`, `DeviceJobStore`, `JobService::{submit,wait,stop,recover}`.
- Worker identity: `Execute { job_id: Option<String>, cmd: ThingCommand, lease: Option<ResourceLease>, done: Option<oneshot::Sender<CommandOutcome>> }`, `Cancelled { thing_id: String, job_id: Option<String>, kind: CancelKind, within_grace: bool, elapsed_ms: u64 }`, `WorkerHandle::cancel_job(&self, job_id: &str) -> bool`.

- [ ] **Step 1: Write failing terminal-dedup and persistent-stop tests**

```rust
#[tokio::test]
async fn duplicate_terminal_result_does_not_create_second_wake() {
    let fixture = JobFixture::new().await;
    let job = fixture.accepted_job("job_1", "task_1").await;
    assert!(fixture.service.finish(&job.id, JobFinal::Done(json!({"ok":true})))
        .await.expect("first final").wake_created);
    assert!(!fixture.service.finish(&job.id, JobFinal::Done(json!({"ok":true})))
        .await.expect("duplicate final").wake_created);
    assert_eq!(fixture.wake_count("task_1").await, 1);
}

#[tokio::test]
async fn stop_is_scoped_to_task_and_survives_repeated_requests() {
    let fixture = JobFixture::new().await;
    fixture.accepted_job("job_a", "task_a").await;
    fixture.accepted_job("job_b", "task_b").await;
    let first = fixture.service.stop("ws_1", "task_a", "job_a").await.expect("stop");
    let second = fixture.service.stop("ws_1", "task_a", "job_a").await.expect("repeat");
    assert_eq!(first.stop_requested_at, second.stop_requested_at);
    assert!(fixture.service.stop("ws_1", "task_b", "job_a").await.is_err());
    assert_eq!(fixture.job("job_b").await.stop_requested_at, None);
    assert_eq!(fixture.worker.cancelled_job_ids(), vec!["job_a"]);
}
```

- [ ] **Step 2: Run the tests and confirm missing Job storage**

Run: `cargo test -p tinyiothub-runtime job:: -- --nocapture && cargo test -p tinyiothub-storage device_job -- --nocapture`
Expected: FAIL on missing `job` module/table; do not substitute the existing device-wide cancellation API.

- [ ] **Step 3: Add Job values, DDL, and persistence port**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceJobState {
    Accepted, Running, Done, Failed, Rejected, Cancelled,
    CancelledUnknown, Interrupted,
}
impl DeviceJobState {
    pub fn is_active(self) -> bool { matches!(self, Self::Accepted | Self::Running) }
    pub fn is_terminal(self) -> bool { !self.is_active() }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceJob {
    pub id: String,
    pub workspace_id: String,
    pub device_id: String,
    pub tool: String,
    pub args_json: Value,
    pub state: DeviceJobState,
    pub stop_requested_at: Option<DateTime<Utc>>,
    pub progress_json: Option<Value>,
    pub result_json: Option<Value>,
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub tool_call_id: Option<String>,
    pub driver_call_id: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

pub enum DeviceJobError {
    NotFound,
    Validation(String),
    Conflict(String),
    ExecutionConflict,
    Storage(String),
}
```

```sql
CREATE TABLE device_jobs (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    device_id TEXT NOT NULL,
    tool TEXT NOT NULL,
    args_json TEXT NOT NULL,
    state TEXT NOT NULL,
    stop_requested_at TEXT,
    progress_json TEXT,
    result_json TEXT,
    task_id TEXT REFERENCES agent_tasks(id),
    run_id TEXT,
    tool_call_id TEXT UNIQUE,
    driver_call_id TEXT,
    started_at TEXT NOT NULL,
    ended_at TEXT
);
CREATE INDEX idx_device_jobs_task_state ON device_jobs(task_id, state);
CREATE INDEX idx_device_jobs_device_state ON device_jobs(workspace_id, device_id, state);
```

```rust
#[async_trait]
pub trait DeviceJobStore: Send + Sync {
    async fn create_job(&self, job: &DeviceJob) -> Result<(), DeviceJobError>;
    async fn get_job(&self, workspace_id: &str, job_id: &str) -> Result<Option<DeviceJob>, DeviceJobError>;
    async fn get_task_job(&self, workspace_id: &str, task_id: &str, job_id: &str) -> Result<Option<DeviceJob>, DeviceJobError>;
    async fn mark_running(&self, job_id: &str, driver_call_id: &str) -> Result<DeviceJob, DeviceJobError>;
    async fn finish_job(&self, job_id: &str, state: DeviceJobState, result: &Value, wake: Option<&TaskWake>) -> Result<bool, DeviceJobError>;
    async fn request_stop(&self, workspace_id: &str, task_id: &str, job_id: &str) -> Result<DeviceJob, DeviceJobError>;
    async fn active_jobs(&self, workspace_id: Option<&str>, unknown_only: bool) -> Result<Vec<DeviceJob>, DeviceJobError>;
}
```

`finish_job` updates only `accepted/running`, stores one terminal fact, and inserts its optional wake in the same transaction. `request_stop` uses `stop_requested_at=COALESCE(stop_requested_at, ?)` only for an owned active Job; repeated/terminal stop returns the current row and reports whether a new worker assertion is needed.

- [ ] **Step 4: Add worker job identity tests before changing worker code**

```rust
#[tokio::test]
async fn queued_stop_never_calls_driver() {
    let fixture = WorkerFixture::paused_execution().await;
    fixture.enqueue(job_execute("job_1", "set_speed")).await;
    assert!(!fixture.handle.cancel_job("job_1"));
    fixture.release_execution();
    let result = fixture.next_cancelled().await;
    assert_eq!(result.job_id.as_deref(), Some("job_1"));
    assert_eq!(fixture.driver.call_count(), 0);
}

#[tokio::test]
async fn in_flight_stop_cancels_only_matching_job() {
    let fixture = WorkerFixture::blocking_driver().await;
    fixture.start(job_execute("job_a", "set_a")).await;
    fixture.start(job_execute("job_b", "set_b")).await;
    assert!(fixture.handle_for_a.cancel_job("job_a"));
    assert!(!fixture.handle_for_a.cancel_job("job_b"));
    assert!(fixture.driver_a.token().is_cancelled());
    assert!(!fixture.driver_b.token().is_cancelled());
}
```

Run: `cargo test -p tinyiothub-runtime driver::worker::tests -- --nocapture`
Expected: FAIL because `Execute` and `Cancelled` have no `job_id` and `cancel_job` is undefined.

- [ ] **Step 5: Implement job-scoped queued and in-flight cancellation**

```rust
struct ActiveCall {
    job_id: Option<String>,
    token: CancelToken,
}
#[derive(Clone, Default)]
struct CancelSlot {
    current: Arc<Mutex<Option<ActiveCall>>>,
    cancel_requested_at: Arc<Mutex<Option<Instant>>>,
    stopped_jobs: Arc<DashSet<String>>,
}
impl CancelSlot {
    fn begin(&self, job_id: Option<String>) -> CancelToken {
        let token = CancelToken::new();
        *self.current.lock() = Some(ActiveCall { job_id, token: token.clone() });
        *self.cancel_requested_at.lock() = None;
        token
    }
    fn cancel_job(&self, job_id: &str) -> bool {
        self.stopped_jobs.insert(job_id.to_owned());
        let slot = self.current.lock();
        let Some(active) = slot.as_ref() else { return false; };
        if active.job_id.as_deref() != Some(job_id) { return false; }
        self.cancel_requested_at.lock().get_or_insert_with(Instant::now);
        active.token.cancel();
        true
    }
    fn was_stopped(&self, job_id: &str) -> bool { self.stopped_jobs.contains(job_id) }
}
```

Retain `cancel_current` for existing device-stop semantics. Add `job_id` to `Execute` and `Cancelled`; at dequeue, return a cancelled result without calling the driver when `was_stopped(job_id)` is true. On in-flight completion, emit the result against the same ID and remove that ID from `stopped_jobs` only after its terminal projection is handed off. `within_grace=false` maps to `cancelled_unknown`; never map it to successful cancellation.

- [ ] **Step 6: Implement real deadline and bounded model wait**

```rust
pub async fn wait(&self, job_id: &str, run_remaining: Duration) -> JobWait {
    let deadline = self.deadline_for(job_id);
    let wait_for = min_duration(Duration::from_secs(50), run_remaining, deadline.remaining());
    match tokio::time::timeout(wait_for, self.terminal_notification(job_id)).await {
        Ok(final_state) => JobWait::Final(final_state),
        Err(_) if deadline.expired() => {
            let final_state = self.cancel_deadline(job_id).await;
            JobWait::Final(final_state)
        }
        Err(_) => JobWait::Running { job_id: job_id.to_owned() },
    }
}
fn min_duration(a: Duration, b: Duration, c: Duration) -> Duration { a.min(b).min(c) }
```

`submit` performs existing declaration/argument/resource validation first, clamps `ValidatedCall.timeout_ms` by existing declaration rules, persists `accepted`, then submits to the worker. Start the deadline at accepted persistence so it covers queue time. `JobFinal::Failed("timeout")` is a real terminal result, not `running`. Final results call `TaskWake` creation for the owning Task; progress only updates projection.

- [ ] **Step 7: Implement restart recovery without replay**

```rust
pub async fn recover(&self) -> Result<RecoveryReport, DeviceJobError> {
    let mut report = RecoveryReport::default();
    for job in self.store.active_jobs(None, false).await? {
        if job.stop_requested_at.is_some() {
            if let Some(worker) = self.worker_for(&job.device_id) {
                worker.cancel_job(&job.id);
                report.stop_reasserted += 1;
                continue;
            }
        }
        if self.recoverable_driver_call(&job).is_none() {
            self.finish_interrupted(&job).await?;
            report.interrupted += 1;
        }
    }
    Ok(report)
}
```

Test stop-request crash/reassert, no-recoverable-call `interrupted`, repeated terminal facts, two Tasks on one device, and a later claim executing normally. No test claims exactly-once physical side effects.

- [ ] **Step 8: Verify and commit Device Jobs**

Run: `cargo test -p tinyiothub-runtime job:: driver::worker::tests -- --nocapture && cargo test -p tinyiothub-storage device_job -- --nocapture`
Expected: PASS for deadline, queued stop, matching in-flight stop, terminal dedupe, restart, repeat stop, and same-device cross-Task isolation.

```bash
git add crates/core/src/device_job.rs crates/core/src/lib.rs crates/runtime/src crates/db/src crates/db/migrations/20261027000002_device_jobs.sql
git commit -m "feat(runtime): add device jobs and job-scoped stop"
```

### Task 3 (T3): Task Coordinator, Watches, Wake Recovery, and Cancellation

**Files:**
- Create: `crates/agent/src/runtime/thing_agent/task/{mod,state,coordinator,wake,prompt}.rs`
- Modify: `crates/agent/src/runtime/thing_agent/{mod,runner,active_runs,manager}.rs`, `crates/agent/src/runtime/events.rs`, `crates/core/src/agent_runs.rs`, `crates/db/src/{runtime_task,agent_runs,watch}.rs`
- Test: colocated `task` tests and manager cancellation integration tests

**Interfaces:**
- Consumes: `TaskStore`, `DeviceJobStore`, existing `WatchRuleStore`, `DirectiveSink`, `AgentEventBus`.
- Produces: `TaskCoordinator::{pause,resume,cancel,advance,recover}`, `WakeRouter::run`, `validate_transition`, `build_task_prompt`.
- Run identity: `RunReport.task_id/wake_event_id`; `RunStarted.task_id/wake_event_id`; caller-owned cancellation token through registry → manager → `Runner::execute` → rig.

- [ ] **Step 1: Write state-machine and pause/cancel boundary tests**

```rust
#[test]
fn pause_never_changes_waiting_relationship_into_terminal() {
    assert_eq!(validate_transition(TaskStatus::Active, TaskStatus::Paused), Ok(()));
    assert_eq!(validate_transition(TaskStatus::WaitingJob, TaskStatus::Paused), Ok(()));
    assert_eq!(validate_transition(TaskStatus::Paused, TaskStatus::Cancelled), Err(TransitionError::Forbidden));
}

#[tokio::test]
async fn pause_records_wake_without_starting_or_steering_run() {
    let fixture = CoordinatorFixture::running_task().await;
    fixture.coordinator.pause("ws_1", "task_1").await.expect("pause");
    fixture.emit_job_final("job_1", "event_2").await;
    assert_eq!(fixture.runner.execute_calls(), 0);
    assert!(!fixture.active_token().is_cancelled());
    assert_eq!(fixture.persisted_wake("event_2").dispatch_state, WakeDispatchState::Pending);
    assert_eq!(fixture.running_job().state, DeviceJobState::Running);
}

#[tokio::test]
async fn cancel_aborts_shared_run_token_and_blocks_later_tool_dispatch() {
    let fixture = CoordinatorFixture::model_at_tool_boundary().await;
    let cancel = fixture.coordinator.cancel("ws_1", "task_1").await.expect("cancel");
    fixture.release_model_tool_boundary().await;
    assert!(cancel.accepted);
    assert!(fixture.active_token().is_cancelled());
    assert_eq!(fixture.tool_dispatch_count(), 0);
}
```

- [ ] **Step 2: Run the tests and confirm the absent Task coordinator**

Run: `cargo test -p tinyiothub-agent thing_agent::task -- --nocapture`
Expected: FAIL on missing `task` module; do not put these methods into `manager.rs`.

- [ ] **Step 3: Implement explicit transition validation and Task-owned prompt assembly**

```rust
pub enum TransitionError { Forbidden }
pub enum Advance { Started { run_id: String }, Steered, Duplicate, Recorded, Deferred }
pub enum TaskError {
    WakeNotFound,
    InvalidWake(String),
    NotFound,
    CrossWorkspace,
    Store(TaskStoreError),
}

pub fn validate_transition(from: TaskStatus, to: TaskStatus) -> Result<(), TransitionError> {
    let allowed: Vec<TaskStatus> = match from {
        TaskStatus::Active => vec![Active, WaitingJob, WaitingWatch, Paused, Cancelling, Completed, Failed],
        TaskStatus::WaitingJob | TaskStatus::WaitingWatch =>
            vec![Active, Paused, Cancelling, Completed, Failed],
        TaskStatus::Paused => vec![Active, WaitingJob, WaitingWatch, Cancelling],
        TaskStatus::Cancelling => vec![Cancelling, Cancelled, Failed],
        TaskStatus::NeedsApproval => vec![Active, WaitingJob, WaitingWatch, Paused, Cancelling, Completed, Failed],
        TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled => vec![],
    };
    if allowed.contains(&to) { Ok(()) } else { Err(TransitionError::Forbidden) }
}
```

Implement `From<TaskStoreError> for TaskError`. Import each enum variant explicitly rather than adding glob imports. `NeedsApproval` producers and decision/recovery transitions are implemented in T6A. Implement `build_task_prompt(task, runs, jobs, watches, approvals, snapshot)` with sections in fixed order: objective, latest Run result, pending Jobs sorted by `job_id`, pending watches sorted by `rule_id`, pending approvals sorted by `approval_id`, existing 6144-byte device snapshot, wake facts. Include all IDs and stage results; do not truncate total Task prompt bytes or iterate unordered maps. Detections/device text remain fenced as untrusted content.

- [ ] **Step 4: Implement wake transaction dispatch and two-layer Run dedupe**

```rust
pub async fn advance(&self, task_id: &str, wake_event_id: &str) -> Result<Advance, TaskError> {
    let wake = self.store.wake_by_id(task_id, wake_event_id).await?
        .ok_or(TaskError::WakeNotFound)?;
    let signal: WakeSignal = serde_json::from_str(&wake.signal_json)
        .map_err(|error| TaskError::InvalidWake(error.to_string()))?;
    let task = self.store.get_task(&signal.workspace_id, task_id).await?
        .ok_or(TaskError::NotFound)?;
    if signal.workspace_id != task.workspace_id { return Err(TaskError::CrossWorkspace); }
    if task.status == TaskStatus::Paused { return Ok(Advance::Deferred); }
    if task.status.is_terminal() { return Ok(Advance::Recorded); }
    if let Some(run_id) = task.current_run_id {
        return self.steer_existing(task, run_id, wake).await;
    }
    let run_id = format!("run_{}", Uuid::new_v4().simple());
    if !self.store.begin_run(&task, wake_event_id, &run_id).await? {
        return Ok(Advance::Duplicate);
    }
    self.start_run(task, wake, run_id).await
}
```

Add `wake_by_id(task_id,event_id)` to `TaskStore` with its DB delegate and use it here. `begin_run` inserts a running `agent_runs` row with both correlation IDs; the partial unique index turns a crash/retry into `false`. `finish_run` updates that same row rather than letting insert-or-ignore discard the final report. `WakeRouter` scans pending, deserializes and workspace-checks each signal, calls `advance`, then marks `dispatched`; `Advance::Deferred` instead leaves the original wake pending for resume, while duplicate/terminal facts are marked without another Run. Consecutive failures become `dead` using the existing DLQ policy and remain retryable by original `event_id`.

- [ ] **Step 5: Add `wake_event_id` to Run events/reports and prove crash dedupe**

```rust
#[serde(default)]
pub task_id: Option<String>,
#[serde(default)]
pub wake_event_id: Option<String>,
```

Add those fields to `RunReport`, and add `task_id: Option<String>, wake_event_id: Option<String>` to `AgentEventKind::RunStarted`. Update all report constructors with `None` unless a persistent Task owns the Run. Assert `started.wake_event_id == report.wake_event_id` in the coordinator test. Simulate dispatch success followed by a crash before `mark_wake`; retry and assert one `agent_runs` row, one Run, and an eventually dispatched outbox row.

- [ ] **Step 6: Wire one caller-owned cancellation token through active Run execution**

```rust
pub struct ActiveRun {
    pub ctx: RunContext,
    pub cancel: CancellationToken,
}
pub struct ActiveRunRegistry { runs: Arc<DashMap<String, ActiveRun>>, max: usize }
impl ActiveRunRegistry {
    pub fn register(&self, ctx: RunContext, cancel: CancellationToken) -> Option<ActiveRunGuard> {
        if self.runs.len() >= self.max { return None; }
        let run_id = ctx.run_id.clone();
        self.runs.insert(run_id.clone(), ActiveRun { ctx, cancel });
        Some(ActiveRunGuard { runs: Arc::clone(&self.runs), run_id })
    }
    pub fn cancel(&self, run_id: &str) -> bool {
        self.runs.get(run_id).map(|run| run.cancel.cancel()).is_some()
    }
}
```

Change the Runner signature to `execute(&self, agent: AgentHandle, prompt: String, ctx: RunContext, cancel: CancellationToken) -> RunOutcome`; delete its internal `CancellationToken::new()`. Manager creates the token, registers `(ctx,cancel.clone())`, then passes it to Runner and `turn_streamed`. Preserve registry-capacity logging. Rig checks the token immediately before every tool dispatch as well as around model streaming. `cancel` sets `cancelling` before aborting the active Run, blocks subsequent dispatch, resolves owned watches/outbox/Jobs, and maps any unknown to `failed(cancel_unknown)`; pause never calls `ActiveRunRegistry::cancel`.

- [ ] **Step 7: Implement Task-owned watch lifecycle and restart recovery**

The T1 additive migration already added `watch_rules.owner_task_id` and its index; do not edit an applied migration in this later commit. Watch upsert/rebuild verifies Task/workspace equality and `session_key` ownership. Hit/timeout/invalid deletes a one-shot Task watch and writes one wake. Rebuild watches only for nonterminal Tasks, including paused; terminal Tasks do not rebuild. On startup mark unfinished model Runs `interrupted`, recover Jobs through T2, restore persisted waiting relations, and create one recovery Run only for active Tasks with no waiting relationship. Never redispatch an accepted Job blindly.

- [ ] **Step 8: Run race/recovery tests and commit the Task engine**

Run: `cargo test -p tinyiothub-agent thing_agent::task thing_agent::manager -- --nocapture && cargo test -p tinyiothub-storage runtime_task watch -- --nocapture`
Expected: PASS for pause/wake, resume-at-most-once, cancel/tool-boundary, outbox crash, watch hit/timeout/invalid/restart, terminal-job dedupe, and stale CAS zero writes.

```bash
git add crates/core/src/agent_runs.rs crates/agent/src crates/db/src
git commit -m "feat(runtime): coordinate task wakes cancellation and recovery"
```

### Task 4 (T4): Source 1.1, Media Evidence, and Model Attachments

**Files:**
- Create: `crates/core/src/media.rs`, `crates/runtime/src/media/{mod,source,evidence,playback}.rs`, `crates/db/src/media_evidence.rs`, `crates/db/migrations/20261027000003_media_evidence.sql`
- Modify: `crates/core/src/{lib,driver/manifest}.rs`, `crates/runtime/src/{lib,ports}.rs`, `crates/agent/src/port/{provider,tool,events}.rs`, `crates/agent/src/adapters/rig/{provider,loop_}.rs`, `crates/db/src/{lib,database}.rs`
- Test: `crates/runtime/src/media/` tests and `crates/agent/src/adapters/rig/tests.rs`

**Interfaces:**
- Produces: `SourceItem`, `DataKind`, `MediaAttachment`, `EvidenceRef`, `MediaStore`, `MediaEvidenceStore`, `EvidenceService`, `PlaybackDescriptor`.
- Attachment fields on `ChatMessage`, `ToolResult`, and `TurnEvent::ToolResult` are `Vec<MediaAttachment>` with default-empty construction.
- `EvidenceService::put_image(input: ImageInput) -> Result<EvidenceRef, MediaError>` owns validation, hashing, dedupe, and retention.

- [ ] **Step 1: Write Source compatibility and unknown-kind tests**

```rust
#[test]
fn source_defaults_to_values_and_unknown_kind_is_preserved() {
    let old: SourceDecl = serde_json::from_value(json!({"id":"temperature","kind":"poll"}))
        .expect("old declaration");
    assert_eq!(old.data_kind, DataKind::Values);
    let item: SourceItem = serde_json::from_value(json!({
        "v":1,"source":"custom","dataKind":"x_lidar","seq":4,
        "time":"2026-10-27T10:00:00Z","data":{"points":3}
    })).expect("custom source");
    assert_eq!(item.data_kind, DataKind::Custom("x_lidar".into()));
    assert_eq!(item.data["points"], 3);
}

#[test]
fn manifest_1_1_accepts_media_and_state_key() {
    let manifest: DriverManifest = serde_json::from_value(json!({
        "manifestVersion":"1.1","identity":{"driver":"fixture","version":"1.0.0"},
        "sources":[{"id":"front","kind":"callback","dataKind":"image"}],
        "stateFields":[{"name":"zone","type":"string","key":"zone"}]
    })).expect("1.1 manifest");
    assert_eq!(manifest.sources[0].data_kind, DataKind::Image);
    assert_eq!(manifest.state_fields[0].key.as_deref(), Some("zone"));
}
```

- [ ] **Step 2: Run Source tests and confirm missing additive fields**

Run: `cargo test -p tinyiothub-runtime media::source -- --nocapture && cargo test -p tinyiothub-core manifest -- --nocapture`
Expected: FAIL on `data_kind`, `key`, and `SourceItem`; existing 1.0 fixtures remain unchanged.

- [ ] **Step 3: Implement versioned Source values and declaration defaults**

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataKind {
    #[serde(rename="values")] Values,
    #[serde(rename="detections")] Detections,
    #[serde(rename="image")] Image,
    #[serde(rename="video")] Video,
    #[serde(untagged)] Custom(String),
}
impl Default for DataKind { fn default() -> Self { Self::Values } }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all="camelCase")]
pub struct SourceItem {
    pub v: u8,
    pub source: String,
    pub data_kind: DataKind,
    pub seq: u64,
    pub time: DateTime<Utc>,
    #[serde(default)] pub text: Option<String>,
    #[serde(default)] pub data: Value,
    #[serde(default)] pub media: Option<MediaPayload>,
    #[serde(default)] pub error: Option<SourceError>,
}
```

Use a custom `DataKind` deserializer if serde cannot represent the untagged enum variant alongside named variants; preserve every unrecognized string as `Custom`, including `x_<custom>`, and never coerce it to `Values`. Add `#[serde(default)] pub data_kind: DataKind` to `SourceDecl` and `#[serde(default)] pub key: Option<String>` to `StateFieldDecl`. Raise the emitted manifest version to 1.1 while continuing to accept 1.0. `read_device` returns detections/images only when requested by source ID; video returns `video_is_for_human_not_model`.

- [ ] **Step 4: Write failing image-boundary and evidence-dedup tests**

```rust
#[tokio::test]
async fn rejects_svg_oversize_and_run_budget() {
    let service = EvidenceFixture::new().await;
    assert!(matches!(service.put_image(image_input("image/svg+xml", 12)).await,
        Err(MediaError::UnsupportedMime)));
    assert!(matches!(service.put_image(image_input("image/png", 2 * 1024 * 1024 + 1)).await,
        Err(MediaError::ImageTooLarge)));
    let mut run = RunMediaBudget::default();
    for _ in 0..4 { run.record(2 * 1024 * 1024).expect("within 8 MiB"); }
    assert!(matches!(run.record(1), Err(MediaError::RunImageBudget)));
}

#[tokio::test]
async fn concurrent_same_evidence_reuses_row_and_bytes() {
    let service = EvidenceFixture::new().await;
    let input = image_input("image/jpeg", 1024);
    let (a, b) = tokio::join!(service.put_image(input.clone()), service.put_image(input));
    assert_eq!(a.expect("first").evidence_id, b.expect("retry").evidence_id);
    assert_eq!(service.evidence_row_count().await, 1);
    assert_eq!(service.stored_object_count().await, 1);
}
```

Add JPEG/PNG magic and pixel-dimension validation, freshness checks, a rejection when one tool call carries two images, and a test proving a fifth 2 MiB image in the same Run exceeds 8 MiB before another attachment enters the provider.

- [ ] **Step 5: Implement local MediaStore, canonical dedupe, and seven-day metadata**

```rust
pub fn evidence_dedup_key(input: &ImageInput, sha256: &str) -> Result<String, MediaError> {
    let tuple = json!([input.workspace_id, input.task_id, input.run_id,
        input.tool_call_id, input.job_id, input.device_id, input.source_id,
        input.source_seq, sha256]);
    let canonical = serde_json::to_vec(&tuple).map_err(|e| MediaError::Validation(e.to_string()))?;
    Ok(hex::encode(Sha256::digest(&canonical)))
}
```

```sql
CREATE TABLE media_evidence (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    task_id TEXT REFERENCES agent_tasks(id),
    run_id TEXT,
    tool_call_id TEXT,
    job_id TEXT REFERENCES device_jobs(id),
    device_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    source_seq INTEGER NOT NULL,
    media_time TEXT NOT NULL,
    mime TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    sha256 TEXT NOT NULL,
    storage_path TEXT NOT NULL,
    dedup_key TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL
);
CREATE INDEX idx_media_evidence_task ON media_evidence(task_id, created_at);
CREATE INDEX idx_media_evidence_expiry ON media_evidence(expires_at);
```

`LocalMediaStore` writes at most 2 MiB to `data/media/<hex(workspace_id)>/<sha256>` using a temporary file and atomic rename, verifies length/hash, and never follows a caller-supplied path. DB insertion reuses an existing `dedup_key`; same-content cross-Task rows may share the hash file but retain separate associations. `expires_at = created_at + 7 days`. Cleanup deletes only expired rows and unreferenced objects after a grace interval; MediaStore failure affects only that media read. Trace stores `EvidenceRef { evidence_id,mime,source_seq,media_time }`, not bytes.

- [ ] **Step 6: Add attachment roundtrip tests through ToolResult, TurnEvent, and rig**

```rust
#[tokio::test]
async fn rig_forwards_real_image_and_trace_keeps_only_reference() {
    let bytes = tiny_valid_png();
    let attachment = MediaAttachment { evidence_id: "evidence_1".into(), mime: "image/png".into(),
        bytes: bytes.clone(), source_seq: 7, media_time: fixed_time() };
    let result = ToolResult { success: true, output: "image captured".into(),
        error: None, attachments: vec![attachment] };
    let messages = tool_result_to_rig(&result);
    let Some(ToolResultContent::Image(image)) = first_tool_content(&messages) else {
        panic!("image attachment missing");
    };
    assert_eq!(image.data, DocumentSourceKind::Raw(bytes));
    let trace = EvidenceRef::from(&result.attachments[0]);
    assert!(!serde_json::to_string(&trace).expect("trace").contains("base64"));
}

#[tokio::test]
async fn non_vision_provider_returns_image_unavailable() {
    let provider = ScriptedProvider::text_only();
    let error = provider.chat(image_request(), "fixture", None).await.expect_err("vision required");
    assert_eq!(error.downcast_ref::<ProviderError>(), Some(&ProviderError::ImageUnavailable));
}
```

Run: `cargo test -p tinyiothub-agent adapters::rig::tests -- --nocapture`
Expected: FAIL because current rig flattens/skips image blocks and the port types have no attachments.

- [ ] **Step 7: Implement additive attachments and exact rig image mapping**

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaAttachment {
    pub evidence_id: String,
    pub mime: String,
    pub bytes: Vec<u8>,
    pub source_seq: u64,
    pub media_time: DateTime<Utc>,
}
```

Add `#[serde(default, skip_serializing_if="Vec::is_empty")] pub attachments: Vec<MediaAttachment>` to `ChatMessage` and `ToolResult`, and `pub attachments: Vec<MediaAttachment>` to `TurnEvent::ToolResult`. Existing constructors initialize an empty vector; do not rewrite `content` into a multimedia union. Map with the installed rig 0.42 contract:

```rust
ToolResultContent::Image(rig_core::message::Image {
    data: DocumentSourceKind::Raw(attachment.bytes.clone()),
    media_type: Some(match attachment.mime.as_str() {
        "image/jpeg" => ImageMediaType::JPEG,
        "image/png" => ImageMediaType::PNG,
        other => return Err(MediaError::UnsupportedMime(other.to_owned())),
    }),
    detail: None,
    additional_params: None,
})
```

Preserve attachments in both rig→port and port→rig conversions instead of skipping `UserContent::Image`. Implement `From<&MediaAttachment> for EvidenceRef` by copying only ID/MIME/sequence/time. Enforce 2 MiB before any driver `into_owned` and reject a tool result whose attachment count exceeds one. Account for image bytes with this exact Run budget before the model call:

```rust
#[derive(Default)]
pub struct RunMediaBudget { image_bytes: usize }
impl RunMediaBudget {
    pub fn record(&mut self, bytes: usize) -> Result<(), MediaError> {
        let next = self.image_bytes.saturating_add(bytes);
        if next > 8 * 1024 * 1024 { return Err(MediaError::RunImageBudget); }
        self.image_bytes = next;
        Ok(())
    }
}
```

A provider without an explicit vision capability returns `image_unavailable`; no text fallback may claim the image was seen. Retain the existing 256 KiB `describe()` limit.

- [ ] **Step 8: Verify media and commit**

Run: `cargo test -p tinyiothub-runtime media:: -- --nocapture && cargo test -p tinyiothub-agent adapters::rig::tests -- --nocapture && cargo test -p tinyiothub-storage media_evidence -- --nocapture`
Expected: PASS for 1.0 defaults, 1.1 media, unknown kinds, MIME/size/run budgets, dedupe, retention metadata, attachment roundtrip, and non-vision failure.

```bash
git add crates/core/src crates/runtime/src crates/agent/src crates/db/src crates/db/migrations/20261027000003_media_evidence.sql
git commit -m "feat(runtime): add source media evidence and model attachments"
```

### Task 5 (T5): Evidence/Playback APIs and Browser Player Lifecycle

**Files:**
- Create: `apps/cloud/src/domains/media/{mod,evidence,playback,types}.rs`, `web/src/api/playback.ts`, `web/src/ui/views/{runtime-playback.ts,runtime-playback.css,runtime-playback.test.ts}`
- Modify: `apps/cloud/src/api/mod.rs`, cloud composition root, `web/package.json`, web lockfile
- Test: cloud media handler tests and Vitest player lifecycle tests

**Interfaces:**
- Consumes: `MediaStore`, `MediaEvidenceStore`, `PlaybackDescriptor`, Runtime media flag.
- Produces: GET evidence and playback descriptor endpoints; `getPlaybackDescriptor(workspaceId,deviceId,sourceId,signal)`; `<runtime-playback>` with `descriptor` property and `destroy()` lifecycle.
- Browser descriptor: `{ protocol: "mjpeg" | "hls", url: string, expiresAt: string }`.

- [ ] **Step 1: Write failing authorization, expiry, and URL-safety tests**

```rust
#[tokio::test]
async fn evidence_is_workspace_scoped_and_expiry_is_explicit() {
    let fixture = MediaApiFixture::new().await;
    assert_eq!(fixture.get_evidence("ws_other", "evidence_1").await.status(), 404);
    fixture.expire("evidence_1").await;
    assert_eq!(fixture.get_evidence("ws_1", "evidence_1").await.status(), 410);
}

#[test]
fn playback_url_rejects_credentials_and_unsafe_schemes() {
    for url in ["file:///tmp/a.m3u8", "javascript:alert(1)", "data:video/mp4;base64,AA",
        "https://user:pass@camera.example/live.m3u8", "http://camera.example/live.m3u8"] {
        assert!(validate_playback_url(url).is_err(), "accepted {url}");
    }
    assert!(validate_playback_url("https://camera.example/live.m3u8?sig=secret").is_ok());
    assert!(validate_playback_url("http://localhost:8080/live.m3u8?sig=local").is_ok());
}
```

The URL assertion must never print a real credential or signed query in test logs; these strings are synthetic fixtures only.

- [ ] **Step 2: Run API tests and confirm missing media routes**

Run: `cargo test -p tinyiothub-cloud domains::media -- --nocapture`
Expected: FAIL because the media domain and routes do not exist.

- [ ] **Step 3: Implement evidence and five-minute playback descriptor handlers**

```rust
pub struct PlaybackDescriptor {
    pub protocol: PlaybackProtocol,
    pub url: String,
    pub expires_at: DateTime<Utc>,
}
pub fn descriptor_expiry(now: DateTime<Utc>, upstream: DateTime<Utc>) -> DateTime<Utc> {
    upstream.min(now + Duration::minutes(5))
}
```

Evidence GET verifies workspace access, looks up the evidence in the same workspace, returns 410 when expired, 404 for absent/cross-workspace rows, and streams local bytes with the stored JPEG/PNG content type. Playback GET verifies workspace/device/source ownership and the media flag, asks the declaration/upstream `PlaybackProvider` for a signed browser URL, validates scheme/userinfo, and caps expiry at five minutes. Return `playback_unavailable` for missing signed URL, unsupported protocol, or unsafe URL; never proxy bytes or rewrite manifests. Log only workspace/device/source IDs, URL origin, and expiry.

- [ ] **Step 4: Add the typed web API and failing player cleanup tests**

```ts
import { ApiClient, ApiError } from './client.js';
import type { PlaybackDescriptor } from '../types/runtime.js';

export async function getPlaybackDescriptor(
  workspaceId: string, deviceId: string, sourceId: string, signal?: AbortSignal,
): Promise<PlaybackDescriptor> {
  const response = await ApiClient.get<PlaybackDescriptor>(
    `/workspaces/${workspaceId}/devices/${deviceId}/sources/${sourceId}/playback`, undefined, signal,
  );
  if (!response.result) throw new ApiError(500, 'Playback descriptor missing');
  return response.result;
}
```

```ts
it('destroys Hls when the descriptor changes or the element disconnects', async () => {
  const element = await fixture<RuntimePlayback>(html`<runtime-playback></runtime-playback>`);
  element.descriptor = descriptor('first');
  await element.updateComplete;
  const first = FakeHls.instances[0];
  element.descriptor = descriptor('second');
  await element.updateComplete;
  expect(first.destroy).toHaveBeenCalledOnce();
  element.remove();
  expect(FakeHls.activeCount).toBe(0);
});
```

Define the test `FakeHls` as the `vi.mock('hls.js')` double with `static instances: FakeHls[]`, a `destroy = vi.fn()` that removes its instance, and a static `activeCount` getter returning `instances.length`; `descriptor(id)` returns a synthetic HLS descriptor whose URL contains only `id`.

- [ ] **Step 5: Install hls.js and implement one player owner**

Run with Node 24: `npm --prefix web install hls.js@1.7.3`
Expected: `web/package.json` pins `hls.js` and the lockfile changes; no Hub proxy dependency is added.

```ts
@customElement('runtime-playback')
export class RuntimePlayback extends LitElement {
  @property({ attribute: false }) descriptor?: PlaybackDescriptor;
  private hls?: Hls;
  private loadAbort?: AbortController;
  private generation = 0;
  @state() private unavailable = false;

  private showUnavailable(): void { this.unavailable = true; }

  disconnectedCallback(): void { this.destroyPlayer(); super.disconnectedCallback(); }
  updated(changed: PropertyValues<this>): void {
    if (changed.has('descriptor')) this.startPlayer();
  }
  private startPlayer(): void {
    this.destroyPlayer();
    const generation = ++this.generation;
    this.loadAbort = new AbortController();
    const descriptor = this.descriptor;
    if (!descriptor || descriptor.protocol === 'mjpeg') return;
    const video = this.renderRoot.querySelector('video');
    if (!video) return;
    if (video.canPlayType('application/vnd.apple.mpegurl')) {
      video.src = descriptor.url;
    } else if (Hls.isSupported()) {
      const hls = new Hls({ enableWorker: true });
      this.hls = hls;
      hls.on(Hls.Events.ERROR, (_event, data) => {
        if (generation === this.generation && data.fatal) this.showUnavailable();
      });
      hls.loadSource(descriptor.url);
      hls.attachMedia(video);
    } else this.showUnavailable();
  }
  destroyPlayer(): void {
    this.generation += 1;
    this.loadAbort?.abort();
    this.hls?.destroy();
    this.hls = undefined;
    const video = this.renderRoot.querySelector('video');
    video?.pause();
    video?.removeAttribute('src');
    video?.load();
  }
}
```

Render MJPEG as an image-frame element, HLS as native controls without autoplay, an explicit offline/unavailable state, understandable image alt text, and `Captions unavailable`. Do not record video by default; history uses saved image evidence and device facts. Guard every late event by `generation`; old media must never update a new Task. Refresh the descriptor through the owning Task session before expiry, not with an independent untracked interval.

- [ ] **Step 6: Run player tests and verify no stale activity**

Run with Node 24: `npm --prefix web test -- runtime-playback.test.ts`
Expected: PASS for descriptor switch, disconnect during fragment load, HLS fatal error, MJPEG cleanup, stale response rejection, and zero live fake players after removal. Real network/CORS proof remains in T8 Playwright.

- [ ] **Step 7: Verify API/player integration and commit**

Run: `cargo test -p tinyiothub-cloud domains::media -- --nocapture && npm --prefix web run type-check && npm --prefix web test -- runtime-playback.test.ts`
Expected: PASS; evidence 404/410 is explicit, descriptors are capped/safe, and player cleanup has no background fake sessions.

```bash
git add apps/cloud/src/domains/media apps/cloud/src/api/mod.rs apps/cloud/src web/src/api/playback.ts web/src/types/runtime.ts web/src/ui/views/runtime-playback* web/package.json web/package-lock.json
git commit -m "feat(runtime): add evidence and direct playback"
```

### Task 6 (T6): Runtime Projection, Controls, Cursor SSE, and Admin Recovery

**Files:**
- Create: `apps/cloud/src/domains/agent/runtime/{controls,projection,stream,admin}.rs`, `crates/db/src/runtime_event.rs`
- Modify: `apps/cloud/src/domains/agent/runtime/{mod,tasks,types}.rs`, `apps/cloud/src/domains/agent/host/handler/agent_tasks.rs`, `apps/cloud/src/api/mod.rs`, `crates/db/src/{lib,database}.rs`
- Test: cloud Runtime handler/stream integration tests and `runtime_event.rs` DB tests

**Interfaces:**
- Consumes: `TaskCoordinator`, `JobService`, `TaskStore`, `DeviceJobStore`, existing `HubLedger`.
- Produces: pause/resume/cancel/stop handlers; `TaskProjection`; `RuntimeEvent`; `RuntimeStreamHub`; admin outbox retry and active/unknown Job reads.
- SSE frames: `snapshot`, `event`, `resync`; `after` is the last ledger cursor, while `event_id` is the public dedupe identity.

- [ ] **Step 1: Write failing control authorization and idempotency tests**

```rust
#[tokio::test]
async fn controls_require_admin_and_scoped_task_job_ownership() {
    let fixture = RuntimeApiFixture::new().await;
    assert_eq!(fixture.pause_as("reader", "ws_1", "task_1").await.status(), 403);
    assert_eq!(fixture.stop_job_as("admin", "ws_1", "task_2", "job_1").await.status(), 404);
    assert_eq!(fixture.cancel_as("admin", "ws_other", "task_1").await.status(), 404);
}

#[tokio::test]
async fn repeated_cancel_and_stop_return_current_projection() {
    let fixture = RuntimeApiFixture::running_with_job().await;
    let first = fixture.cancel("task_1").await;
    let second = fixture.cancel("task_1").await;
    assert_eq!(first.task_id(), second.task_id());
    assert_eq!(fixture.cancel_call_count(), 1);
    let stop_a = fixture.stop("task_1", "job_1").await;
    let stop_b = fixture.stop("task_1", "job_1").await;
    assert_eq!(stop_a.job()["stopRequestedAt"], stop_b.job()["stopRequestedAt"]);
}
```

Add race tests with pause-release barriers: result-before-cancel cannot be overwritten by cancel; cancel-before-result waits for the Job result and maps unknown to failed. Pause must leave current Run/Job/watch active.

- [ ] **Step 2: Run control tests and confirm missing routes**

Run: `cargo test -p tinyiothub-cloud domains::agent::runtime::controls -- --nocapture`
Expected: FAIL on missing control handlers; approval routes are added only after the T6A persistence tests exist.

- [ ] **Step 3: Implement control handlers as thin coordinator adapters**

```rust
pub async fn pause(State(state): State<AgentState>, Extension(claims): Extension<Claims>,
    Path((workspace_id, task_id)): Path<(String, String)>) -> Json<ApiResponse<Value>> {
    verify_agent_admin!(state, claims, workspace_id);
    let Some(runtime) = state.runtime.as_ref() else { return service_unavailable(); };
    match runtime.coordinator.pause(&workspace_id, &task_id).await {
        Ok(task) => ApiResponseBuilder::success(json!({"task": task})),
        Err(TaskError::NotFound) => ApiResponseBuilder::error_with_code(404, "Task not found"),
        Err(TaskError::ConcurrentUpdate) => ApiResponseBuilder::error_with_code(409, "Task changed; refresh"),
        Err(_) => ApiResponseBuilder::error("Task pause failed"),
    }
}
```

Implement `resume`, `cancel`, and `jobs/{job_id}/stop` with the same guard and typed error mapping. Cancel/stop are idempotent and return the current server projection; no local status is treated as physical success. Stop calls only `JobService::stop(workspace,task,job)`, never Task cancel or device stop. Register the non-approval spec routes here; `/approvals` routes belong to T6A.

- [ ] **Step 4: Write projection ordering and cursor recovery tests**

```rust
#[tokio::test]
async fn reconnect_after_cursor_has_no_duplicate_or_lost_control_events() {
    let fixture = StreamFixture::new().await;
    fixture.append(control_event("event_1", 1)).await;
    fixture.append(control_event("event_2", 2)).await;
    let mut stream = fixture.connect("task_1", Some("1")).await;
    assert_eq!(stream.next_json().await["event"]["eventId"], "event_2");
    fixture.append(control_event("event_2", 2)).await;
    assert!(stream.next_timeout(Duration::from_millis(100)).await.is_none());
}

#[tokio::test]
async fn lagged_subscriber_gets_snapshot_resync_not_partial_delta() {
    let fixture = StreamFixture::with_channel_capacity(1).await;
    let mut stream = fixture.connect("task_1", None).await;
    fixture.append(control_event("event_1", 1)).await;
    fixture.append(control_event("event_2", 2)).await;
    assert_eq!(stream.next_json().await["type"], "resync");
    assert!(stream.next_json().await["snapshot"]["task"]["id"].is_string());
}
```

- [ ] **Step 5: Implement ledger-backed events and one Task projection query surface**

```rust
pub struct RuntimeEvent {
    pub cursor: i64,
    pub event_id: String,
    pub event_type: String,
    pub occurred_at: DateTime<Utc>,
    pub workspace_id: String,
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub tool_call_id: Option<String>,
    pub job_id: Option<String>,
    pub approval_id: Option<String>,
    pub device_id: Option<String>,
    pub source_id: Option<String>,
    pub source_seq: Option<i64>,
    pub evidence_id: Option<String>,
    pub data: Value,
}
```

Store `runtime.v1` envelopes through the existing `HubLedger`; do not create a second event-order table. Add `Db::runtime_events_after(workspace_id, after: Option<i64>, task_id: Option<&str>, limit: i64) -> Result<Vec<RuntimeEvent>>` and `Db::runtime_projection(workspace_id,task_id) -> Result<Option<TaskProjection>>` in `runtime_event.rs`, combining Task, ordered Runs, Jobs, watches, approvals, evidence refs, and associated summary without duplicating `RunReport`. Control/job/watch/Task events are ordered; telemetry/video remain on latest-wins channels.

- [ ] **Step 6: Implement SSE snapshot/event/resync with demand cleanup**

```rust
pub enum RuntimeFrame {
    Snapshot { snapshot: Box<TaskProjection>, cursor: i64 },
    Event { event: Box<RuntimeEvent> },
    Resync { snapshot: Box<TaskProjection>, cursor: i64, reason: String },
}
```

`RuntimeStreamHub` owns per-workspace broadcast subscriptions and connection cleanup. Handler order: workspace/task authorization → load snapshot and cursor → emit `snapshot` → replay persisted frames after `after` → subscribe for deltas with stale-task filtering. On broadcast lag, unknown cursor, or projection mismatch, emit `resync` with a fresh snapshot; never silently continue partial deltas. Use bounded channels and `AbortOnDrop`/guard cleanup so Task switch closes the stream. `event_id` remains stable across retries.

- [ ] **Step 7: Implement admin dead-wake retry and Job diagnostics**

```rust
#[derive(Deserialize)]
pub struct AdminWorkspaceScope { pub workspace_id: String }
pub async fn retry_dead_wake(State(state): State<AgentState>,
    Extension(claims): Extension<Claims>, Path(event_id): Path<String>,
    Query(scope): Query<AdminWorkspaceScope>) -> Json<ApiResponse<Value>> {
    if !super::is_admin(&state.db, &claims.user_id).await {
        return ApiResponseBuilder::error_with_code(403, "Admin required");
    }
    verify_agent_admin!(state, claims, scope.workspace_id.clone());
    match state.db.retry_dead_wake(&scope.workspace_id, &event_id).await {
        Ok(true) => ApiResponseBuilder::success(json!({"eventId":event_id,"state":"pending"})),
        Ok(false) => ApiResponseBuilder::error_with_code(404, "Dead wake not found"),
        Err(_) => ApiResponseBuilder::error("Wake retry failed"),
    }
}
```

`AdminWorkspaceScope` is deserialized from the required `workspaceId` query parameter and checked with the existing workspace-access guard before retry; do not add a workspace field to `Claims`. Retry only `dead → pending` with the original ID. Add `GET /admin/runtime/outbox?state=dead`, `POST /admin/runtime/outbox/{event_id}/retry?workspaceId=...`, and `GET /admin/runtime/jobs?state=active|unknown`. Reject other state filters with 400. Never delete pending/dead rows, create a replacement event, or auto-replay unknown side effects.

- [ ] **Step 8: Verify projection/SSE/admin and commit**

Run: `cargo test -p tinyiothub-cloud domains::agent::runtime -- --nocapture && cargo test -p tinyiothub-storage runtime_event -- --nocapture`
Expected: PASS for admin/tenant guards, control races, idempotent stop/cancel, ordered replay, lag resync, stale disconnect, original-ID retry, and unknown Job visibility.

```bash
git add apps/cloud/src/domains/agent apps/cloud/src/api/mod.rs crates/db/src/runtime_event.rs crates/db/src/lib.rs crates/db/src/database.rs
git commit -m "feat(runtime): add runtime controls projections and recovery apis"
```

### Task 6A (T6A): Durable Approvals and Idempotent Execution Recovery

**Files:**
- Create: `crates/core/src/task_approval.rs`, `crates/agent/src/port/approval.rs`, `crates/agent/src/runtime/thing_agent/task/approval.rs`, `crates/db/src/task_approval.rs`, `crates/db/migrations/20261027000004_task_approvals.sql`, `apps/cloud/src/domains/agent/runtime/approvals.rs`
- Modify: `crates/core/src/lib.rs`, `crates/agent/src/port/mod.rs`, `crates/agent/src/runtime/thing_agent/task/{mod,coordinator,wake}.rs`, `crates/runtime/src/job/service.rs`, `crates/db/src/{lib,database}.rs`, `apps/cloud/src/domains/agent/runtime/{mod,types,projection}.rs`, `apps/cloud/src/api/mod.rs`
- Test: colocated approval store/executor tests and `apps/cloud/src/domains/agent/runtime/approvals.rs` handler tests

**Interfaces:**
- Consumes: T1 Task/outbox CAS, T2 `device_jobs.tool_call_id` uniqueness and Job recovery, T3 wake routing, T6 projection and admin guard.
- Produces: `TaskApproval`, `ApprovalState`, `ApprovalDecision`, `TaskApprovalStore`, `ApprovalExecutor::{execute_decision,recover}`, `JobService::submit_approved`, and approve/reject HTTP handlers.
- Recovery identity: `approval_id → decision_event_id → tool_call_id → job_id`; display and execution always use the same frozen `args_json`.

- [ ] **Step 1: Write failing decision-CAS, frozen-argument, and atomic-outbox tests**

```rust
#[tokio::test]
async fn concurrent_same_decision_creates_one_outbox_and_preserves_arguments() {
    let fixture = ApprovalFixture::new().await;
    let pending = fixture.pending_approval().await;
    let frozen = pending.args_json.clone();
    let (a, b) = tokio::join!(
        fixture.decide(&pending.id, ApprovalDecision::Approve, "admin_1"),
        fixture.decide(&pending.id, ApprovalDecision::Approve, "admin_1"),
    );
    assert_eq!(a.expect("first").approval.decision_event_id,
        b.expect("replay").approval.decision_event_id);
    assert_eq!(fixture.outbox_count(&pending.id).await, 1);
    assert_eq!(fixture.get(&pending.id).await.args_json, frozen);
    assert!(matches!(
        fixture.decide(&pending.id, ApprovalDecision::Reject, "admin_1").await,
        Err(ApprovalError::DecisionConflict)
    ));
}

#[tokio::test]
async fn reject_and_transaction_failure_create_no_execution() {
    let fixture = ApprovalFixture::new().await;
    let pending = fixture.pending_approval().await;
    fixture.decide(&pending.id, ApprovalDecision::Reject, "admin_1")
        .await.expect("reject");
    fixture.execute_decision(&pending.decision_event_id).await.expect("decision wake");
    assert_eq!(fixture.job_count(&pending.tool_call_id).await, 0);

    let other = fixture.pending_approval().await;
    fixture.fail_next_outbox_insert();
    assert!(fixture.decide(&other.id, ApprovalDecision::Approve, "admin_1").await.is_err());
    assert_eq!(fixture.get(&other.id).await.state, ApprovalState::Pending);
    assert_eq!(fixture.outbox_count(&other.id).await, 0);
}
```

- [ ] **Step 2: Run store tests and confirm missing approval persistence**

Run: `cargo test -p tinyiothub-storage task_approval -- --nocapture`
Expected: FAIL because the approval table/port do not exist; do not substitute `PendingActionStore`.

- [ ] **Step 3: Add additive approval DDL, values, and transactional store**

```sql
CREATE TABLE task_approvals (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    task_id TEXT NOT NULL REFERENCES agent_tasks(id),
    run_id TEXT NOT NULL,
    tool_call_id TEXT NOT NULL UNIQUE,
    device_id TEXT NOT NULL,
    tool TEXT NOT NULL,
    args_json TEXT NOT NULL,
    reason TEXT NOT NULL,
    state TEXT NOT NULL,
    decision TEXT,
    decider_id TEXT,
    requested_at TEXT NOT NULL,
    decided_at TEXT,
    expires_at TEXT NOT NULL,
    decision_event_id TEXT NOT NULL UNIQUE,
    updated_at TEXT NOT NULL
);
CREATE INDEX idx_task_approvals_task_state
ON task_approvals(workspace_id, task_id, state);
CREATE INDEX idx_task_approvals_expiry
ON task_approvals(expires_at, state);
```

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all="snake_case")]
pub enum ApprovalState { Pending, Approved, Rejected, Expired, Unavailable }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all="snake_case")]
pub enum ApprovalDecision { Approve, Reject }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskApproval {
    pub id: String,
    pub workspace_id: String,
    pub task_id: String,
    pub run_id: String,
    pub tool_call_id: String,
    pub device_id: String,
    pub tool: String,
    pub args_json: Value,
    pub reason: String,
    pub state: ApprovalState,
    pub decision: Option<ApprovalDecision>,
    pub decider_id: Option<String>,
    pub requested_at: DateTime<Utc>,
    pub decided_at: Option<DateTime<Utc>>,
    pub expires_at: DateTime<Utc>,
    pub decision_event_id: String,
    pub updated_at: DateTime<Utc>,
}

pub struct DecisionOutcome {
    pub approval: TaskApproval,
    pub deduplicated: bool,
}
pub enum ExecutionOutcome { Job(DeviceJob), NoExecution, Deferred }
pub enum ApprovalError {
    NotFound,
    DecisionConflict,
    Storage(String),
    Job(DeviceJobError),
    Task(TaskError),
}

#[async_trait]
pub trait TaskApprovalStore: Send + Sync {
    async fn create_pending(&self, approval: &TaskApproval, expected_version: i64)
        -> Result<AgentTask, ApprovalError>;
    async fn get_scoped(&self, workspace_id: &str, task_id: &str, approval_id: &str)
        -> Result<Option<TaskApproval>, ApprovalError>;
    async fn get_by_decision_event(&self, event_id: &str)
        -> Result<Option<TaskApproval>, ApprovalError>;
    async fn decide(&self, workspace_id: &str, task_id: &str, approval_id: &str,
        decision: ApprovalDecision, decider_id: &str, now: DateTime<Utc>,
        wake: &TaskWake) -> Result<DecisionOutcome, ApprovalError>;
    async fn list_recoverable(&self, workspace_id: Option<&str>)
        -> Result<Vec<TaskApproval>, ApprovalError>;
    async fn expire_due(&self, now: DateTime<Utc>) -> Result<u64, ApprovalError>;
}
```

`create_pending` inserts the frozen request and changes the Task to `needs_approval` in one `BEGIN IMMEDIATE` transaction. `decide` accepts only a pending, unexpired row whose Task is `needs_approval`, or `paused` with that pending request. It requires `wake.event_id == approval.decision_event_id` and matching workspace/Task; handlers derive this wake from the server-side approval, never request input. It writes decision/decider/time and the preallocated `decision_event_id` wake atomically. The wake for reject is a decision fact, not device execution. Same-decision replay returns `deduplicated=true`; opposite decision is `DecisionConflict`. A newly due pending row is atomically changed to `expired` with that same decision wake; repeated expired/unavailable/terminal decisions return the current projection without another wake. `expire_due` performs the same expiry transition during recovery before pending dispatch scans. Task cancel uses the same transaction boundary to mark pending approvals unavailable and invalidate undispatched decision wakes.

- [ ] **Step 4: Run persistence and migration integrity tests**

Run: `cargo test -p tinyiothub-storage task_approval migrations::tests -- --nocapture`
Expected: PASS for concurrent replay, conflict 409 mapping input, frozen arguments, reject without execution intent, all-or-nothing rollback, expiry, and `PRAGMA foreign_key_check` with zero rows.

- [ ] **Step 5: Write failing crash-continuation and control-race tests**

```rust
#[tokio::test]
async fn approved_execution_recovers_from_every_commit_boundary_without_duplicate_jobs() {
    for point in [CrashPoint::BeforeDecisionCommit, CrashPoint::AfterDecisionCommit,
        CrashPoint::AfterDispatchBeforeJob, CrashPoint::AfterJobBeforeWakeMark] {
        let fixture = ApprovalFixture::new().await;
        let pending = fixture.pending_approval().await;
        assert!(fixture.drive_to_crash(point, &pending).await.is_err(), "{point:?}");
        fixture.recover().await.expect("recover");
        fixture.recover().await.expect("idempotent recovery");
        if point == CrashPoint::BeforeDecisionCommit {
            assert_eq!(fixture.get(&pending.id).await.state, ApprovalState::Pending);
            assert_eq!(fixture.outbox_count(&pending.id).await, 0);
            assert_eq!(fixture.job_count(&pending.tool_call_id).await, 0);
        } else {
            assert_eq!(fixture.job_count(&pending.tool_call_id).await, 1, "{point:?}");
            assert_eq!(fixture.single_job(&pending.tool_call_id).await.args_json, pending.args_json);
        }
    }
}

#[tokio::test]
async fn pending_recovers_and_cancel_before_decision_wins() {
    let fixture = ApprovalFixture::new().await;
    let pending = fixture.pending_approval().await;
    fixture.recover().await.expect("pending recovery");
    assert_eq!(fixture.get(&pending.id).await.state, ApprovalState::Pending);
    fixture.cancel_task("task_1").await;
    let outcome = fixture.decide(&pending.id, ApprovalDecision::Approve, "admin_1")
        .await.expect("current unavailable projection");
    assert_eq!(outcome.approval.state, ApprovalState::Unavailable);
    assert_eq!(fixture.job_count(&pending.tool_call_id).await, 0);
}

#[tokio::test]
async fn paused_reject_keeps_original_decision_pending_until_resume() {
    let fixture = ApprovalFixture::new().await;
    let pending = fixture.pending_approval().await;
    fixture.pause_task(&pending.task_id).await.expect("pause");
    fixture.reject(&pending.id, "admin_1").await.expect("reject");
    assert!(matches!(
        fixture.dispatch_decision(&pending.decision_event_id).await.expect("deferred"),
        ExecutionOutcome::Deferred
    ));
    assert!(!fixture.outbox_dispatched(&pending.decision_event_id).await);
    fixture.resume_task(&pending.task_id).await.expect("resume");
    assert!(matches!(
        fixture.dispatch_decision(&pending.decision_event_id).await.expect("resume"),
        ExecutionOutcome::NoExecution
    ));
    assert!(fixture.outbox_dispatched(&pending.decision_event_id).await);
}

#[tokio::test]
async fn paused_expiry_keeps_original_decision_pending_until_resume() {
    let fixture = ApprovalFixture::new().await;
    let pending = fixture.pending_approval().await;
    fixture.pause_task(&pending.task_id).await.expect("pause");
    fixture.expire(&pending.id).await.expect("expire");
    assert!(matches!(
        fixture.dispatch_decision(&pending.decision_event_id).await.expect("deferred"),
        ExecutionOutcome::Deferred
    ));
    assert!(!fixture.outbox_dispatched(&pending.decision_event_id).await);
    fixture.resume_task(&pending.task_id).await.expect("resume");
    assert!(matches!(
        fixture.dispatch_decision(&pending.decision_event_id).await.expect("resume"),
        ExecutionOutcome::NoExecution
    ));
    assert!(fixture.outbox_dispatched(&pending.decision_event_id).await);
}
```

Add pause-before-decision-dispatch, reject/expire/unavailable, pre-decision rollback, existing terminal Job, active Job restart, and unknown-physical-result cases. Paused decisions remain undispatched until resume; unknown/interrupted work is surfaced without automatic replay. No assertion claims physical exactly-once.

- [ ] **Step 6: Write failing HTTP tests and run executor/API tests**

```rust
#[tokio::test]
async fn approval_routes_enforce_scope_admin_and_decision_conflict() {
    let fixture = ApprovalFixture::new().await;
    let pending = fixture.pending_approval().await;
    assert_eq!(fixture.post_decision("reader", "ws_1", "task_1", &pending.id, "approve", "{}")
        .await.status(), 403);
    assert_eq!(fixture.post_decision("admin", "ws_other", "task_1", &pending.id, "approve", "{}")
        .await.status(), 404);
    assert_eq!(fixture.post_decision("admin", "ws_1", "task_1", &pending.id, "approve", "{}")
        .await.status(), 200);
    assert_eq!(fixture.post_decision("admin", "ws_1", "task_1", &pending.id, "approve", "{}")
        .await.status(), 200);
    assert_eq!(fixture.post_decision("admin", "ws_1", "task_1", &pending.id, "reject", "{}")
        .await.status(), 409);
}
```

Run: `cargo test -p tinyiothub-agent thing_agent::task::approval -- --nocapture && cargo test -p tinyiothub-cloud domains::agent::runtime::approvals -- --nocapture`
Expected: FAIL on missing executor/routes. Also verify a body containing `args_json`, `decider_id`, or `decision_event_id` is ignored rather than used; the response projection must still show server-frozen values.

- [ ] **Step 7: Implement approved-job executor, restart recovery, and thin routes**

```rust
pub struct ApprovedToolCall {
    pub workspace_id: String,
    pub task_id: String,
    pub run_id: String,
    pub tool_call_id: String,
    pub device_id: String,
    pub tool: String,
    pub args_json: Value,
}

pub async fn execute_decision(&self, event_id: &str) -> Result<ExecutionOutcome, ApprovalError> {
    let approval = self.store.get_by_decision_event(event_id).await?
        .ok_or(ApprovalError::NotFound)?;
    match approval.state {
        ApprovalState::Approved => {
            let call = ApprovedToolCall::from(&approval);
            match self.jobs.submit_approved(call).await? {
                Some(job) => Ok(ExecutionOutcome::Job(job)),
                None => Ok(ExecutionOutcome::Deferred),
            }
        }
        ApprovalState::Rejected | ApprovalState::Expired | ApprovalState::Unavailable => {
            match self.coordinator.advance(&approval.task_id, event_id).await? {
                Advance::Deferred => Ok(ExecutionOutcome::Deferred),
                _ => Ok(ExecutionOutcome::NoExecution),
            }
        }
        ApprovalState::Pending => Ok(ExecutionOutcome::Deferred),
    }
}
```

`From<&TaskApproval> for ApprovedToolCall` copies only the persisted workspace/Task/Run/tool-call/device/tool/frozen-args fields. Add `JobService::submit_approved(call) -> Result<Option<DeviceJob>, DeviceJobError>` plus `From<DeviceJobError>` and `From<TaskError>` for `ApprovalError`. After existing declaration validation, first resolve an existing `tool_call_id`: return that Job only when every frozen execution field matches, otherwise return `DeviceJobError::ExecutionConflict`. If no Job exists, its DB transaction permits insertion only while the Task is `needs_approval`, inserts `accepted` with the original `tool_call_id`, and moves the Task to `waiting_job` atomically. Paused/cancelling/terminal state with no existing Job returns `Ok(None)` without insertion. New Jobs go to the worker once; recovery never re-enqueues an existing accepted/running Job blindly.

WakeRouter routes `approval_decided` here before generic model advancement. `Deferred`, or a paused `advance`, leaves the original outbox pending so resume can retry the same `decision_event_id`; it never creates another decision event. Cancel invalidates undispatched intents. Startup recovery calls `expire_due` before scanning, leaves unexpired pending rows pending, retries approved rows without a Job, restores existing Job waiting relations, and routes rejected/expired/unavailable facts without execution. Existing terminal Jobs only restore their recorded facts; unknown/interrupted work is never replayed.

Add empty-body `approve`/`reject` handlers with the existing Agent admin guard, scoped 404, conflict 409, expired/unavailable current projection, and no client-supplied execution fields. Include approvals in Task projections and ordered runtime events.

- [ ] **Step 8: Verify all approval recovery and API tests**

Run: `cargo test -p tinyiothub-storage task_approval -- --nocapture && cargo test -p tinyiothub-agent thing_agent::task::approval -- --nocapture && cargo test -p tinyiothub-cloud domains::agent::runtime::approvals -- --nocapture && cargo test -p tinyiothub-runtime job:: -- --nocapture`
Expected: PASS for decision idempotency, atomic outbox, exact frozen args, each crash boundary, one Job maximum, no reject/expired/unavailable execution, pause/cancel races, workspace/admin isolation, and existing Job regressions.

- [ ] **Step 9: Commit the durable approval slice**

```bash
git add crates/core/src/task_approval.rs crates/core/src/lib.rs crates/agent/src crates/runtime/src/job/service.rs crates/db/src crates/db/migrations/20261027000004_task_approvals.sql apps/cloud/src/domains/agent apps/cloud/src/api/mod.rs
git commit -m "feat(runtime): persist approvals and recover execution idempotently"
```

### Task 7A (T7A): Web Runtime API, Routes, Projection, and Task Shell

**Files:**
- Create: `web/src/types/runtime.ts`, `web/src/api/{runtime,runtime-stream}.ts`, `web/src/ui/views/{runtime-projection,runtime-url-state,runtime-list,runtime-new,runtime-task}.ts` and colocated tests
- Modify: `web/src/api/client.ts`, `web/src/ui/app.ts`, existing AI-center navigation link
- Test: `web/src/api/runtime.test.ts`, `web/src/ui/views/{runtime-projection,runtime-url-state,runtime-new}.test.ts`

**Interfaces:**
- Consumes: Task 6 HTTP/SSE contracts and default-off Workbench flag.
- Produces: `createRuntimeTask`, `listRuntimeTasks`, `getRuntimeTask`, `pauseRuntimeTask`, `resumeRuntimeTask`, `cancelRuntimeTask`, `stopRuntimeJob`, `decideRuntimeApproval`; `RuntimeProjection.apply`; `parseRuntimeHash`, `replaceRuntimeTask`.
- Routes: `#/runtime`, `#/runtime/new`, `#/runtime/tasks/{id}` with `view`, `at`, and `event` query state.

- [ ] **Step 1: Write failing API idempotency-header and error-retry tests**

```ts
it('sends the caller key and does not retry 400 or 409', async () => {
  const post = vi.spyOn(ApiClient, 'post').mockRejectedValue(new ApiError(409, 'conflict'));
  await expect(createRuntimeTask('ws_1', 'inspect', 'key_1')).rejects.toMatchObject({ code: 409 });
  expect(post).toHaveBeenCalledOnce();
  expect(post).toHaveBeenCalledWith('/workspaces/ws_1/agent/tasks', { text: 'inspect' },
    { headers: { 'Idempotency-Key': 'key_1' }, signal: undefined });
});

it('retains key only for unchanged network retry', async () => {
  const form = new RuntimeCreateModel();
  const first = form.keyFor('inspect pumps');
  const retry = form.keyFor('inspect pumps');
  const edited = form.keyFor('inspect pumps and valves');
  expect(retry).toBe(first);
  expect(edited).not.toBe(first);
});
```

`RuntimeCreateModel` owns objective/key pairing, not the component. Network failure may be manually retried with the same key; HTTP 400/409 remains visible and is never silently retried.

- [ ] **Step 2: Run API tests and confirm missing Runtime transport**

Run with Node 24: `npm --prefix web test -- runtime.test.ts`
Expected: FAIL because `api/runtime.js` and `RuntimeCreateModel` do not exist.

- [ ] **Step 3: Extend the existing client with optional request options and implement Runtime APIs**

```ts
export interface ApiRequestOptions {
  headers?: Record<string, string>;
  signal?: AbortSignal;
}
static async post<T>(endpoint: string, data?: any, options: ApiRequestOptions = {}):
  Promise<ApiResponse<KeysToCamelCase<T>>> {
  const snakeData = data ? keysToSnakeCase(data) : undefined;
  const response = await request<ApiResponse<T>>(endpoint, {
    method: 'POST', body: snakeData, headers: options.headers, signal: options.signal,
  });
  if (response.code !== 0) throw new ApiError(response.code, response.msg || '请求失败', response);
  return { ...response, result: response.result ? keysToCamelCase(response.result) : response.result,
  } as ApiResponse<KeysToCamelCase<T>>;
}
```

```ts
export function createRuntimeTask(workspaceId: string, text: string, idempotencyKey: string,
  signal?: AbortSignal): Promise<ApiResponse<CreateTaskResult>> {
  return ApiClient.post(`/workspaces/${workspaceId}/agent/tasks`, { text }, {
    headers: { 'Idempotency-Key': idempotencyKey }, signal,
  });
}
export function decideRuntimeApproval(workspaceId: string, taskId: string, approvalId: string,
  decision: 'approve' | 'reject', signal?: AbortSignal): Promise<ApiResponse<TaskProjection>> {
  return ApiClient.post(`/workspaces/${workspaceId}/agent/tasks/${taskId}/approvals/${approvalId}/${decision}`,
    {}, { signal });
}

export class RuntimeCreateModel {
  private objective: string | null = null;
  private key = '';

  keyFor(objective: string): string {
    if (objective !== this.objective) {
      this.objective = objective;
      this.key = crypto.randomUUID();
    }
    return this.key;
  }
}
```

Define `TaskStatus`, `DeviceJobState`, `ApprovalState`, `RuntimeApproval`, `RuntimeTask`, `RuntimeJob`, `RuntimeWatch`, `EvidenceRef`, `RuntimeEvent`, `TaskProjection`, and `PlaybackDescriptor` in `web/src/types/runtime.ts` using the exact server snake/camel-converted fields. Add typed list/get/pause/resume/cancel/stop functions in the same API module. No component calls `fetch()`.

- [ ] **Step 4: Write failing ordered-projection and URL-state tests**

```ts
it('deduplicates event ids and derives devices only from actual references', () => {
  const projection = new RuntimeProjection(snapshotWithNoDevices());
  projection.apply(event('event_1', 'list_devices', { candidates: ['dev_1'] }));
  expect(projection.devices).toEqual([]);
  projection.apply(event('event_2', 'read_device', { deviceId: 'dev_1' }));
  projection.apply(event('event_2', 'read_device', { deviceId: 'dev_1' }));
  expect(projection.devices.map(device => device.deviceId)).toEqual(['dev_1']);
  expect(projection.steps).toHaveLength(2);
});

it('enters history only for at or event and clears task state on switch', () => {
  expect(parseRuntimeHash('#/runtime/tasks/t1?view=flow&device=d1').history).toBeNull();
  expect(parseRuntimeHash('#/runtime/tasks/t1?event=e2').history).toEqual({ event: 'e2' });
  expect(switchTaskUrl('t2', 'flow')).toBe('#/runtime/tasks/t2?view=flow');
});
```

- [ ] **Step 5: Implement one Runtime projection and explicit hash URL state**

```ts
apply(event: RuntimeEvent): void {
  if (this.eventIds.has(event.eventId) || this.frozenHistory) return;
  this.eventIds.add(event.eventId);
  this.events.push(event);
  this.events.sort((a, b) => a.cursor - b.cursor);
  if (associatedTools.has(event.eventType) && event.deviceId) this.associate(event.deviceId);
  if (event.jobId) this.upsertJob(event);
  if (event.approvalId) this.upsertApproval(event);
  if (event.evidenceId) this.upsertEvidence(event);
}
```

Use `associatedTools = new Set(['read_device','call_device','set_device','stop_device','watch_device'])`; job/watch/source evidence also associates its Task device. Keep first-reference order, group nonterminal/terminal Tasks by latest activity, freeze history projections, and derive `stale/offline/connecting` separately from history. `replaceWithSnapshot` clears event IDs, steps, Jobs, approvals, devices, and evidence before applying a snapshot. Do not reuse LiveDevices or Flow stores as alternate ordering owners.

Implement hash parsing for the three routes, preserve `view=flow`, treat only `at/event` as history, clear task-specific `device/at/event` on Task switch, and replace-navigate after create. Extend `app.ts` with an explicit hash-route branch instead of forcing these paths through pathname `navigate()`.

- [ ] **Step 6: Implement dedicated Runtime SSE cursor client**

```ts
export interface RuntimeStreamCallbacks {
  snapshot(projection: TaskProjection, cursor: number): void;
  event(event: RuntimeEvent): void;
  resync(projection: TaskProjection, cursor: number): void;
  connection(state: 'connecting' | 'online' | 'offline'): void;
}
export function connectRuntimeStream(input: RuntimeStreamInput): () => void {
  const controller = new AbortController();
  void consumeRuntimeStream(input, controller.signal).catch(error => {
    if (!controller.signal.aborted) input.callbacks.connection('offline');
  });
  return () => controller.abort();
}
```

Reuse only `parseSseChunk` mechanics from `live.ts`; do not reuse its hardcoded endpoint or `sse-client.ts`. `consumeRuntimeStream` lives in the API layer and may call `fetch()` directly because the JSON `request()` helper cannot consume SSE; build the URL with `encodeURIComponent(workspaceId)`, `taskId`, and `after`, and apply the same auth/workspace headers as `client.ts` without logging them. Track the last cursor and event IDs, handle snapshot/event/resync, and abort reader cleanup on close. Reconnect with bounded exponential backoff and jitter from the last cursor. A generation/task ID check drops every stale response after Task switch. Components still never call `fetch()`.

- [ ] **Step 7: Build list/new/detail shell and creation behavior**

```ts
private async submit(): Promise<void> {
  if (this.pending) return;
  this.pending = true;
  const objective = this.objective.trim();
  try {
    const response = await createRuntimeTask(this.workspaceId, objective, this.model.keyFor(objective));
    if (!response.result) throw new ApiError(500, 'Task creation response missing');
    replaceRuntimeTask(response.result.taskId);
  } catch (error) {
    this.error = error instanceof ApiError ? error.message : 'Network error; retry with the same objective';
  } finally { this.pending = false; }
}
```

List page has only Runtime title, connection state, New task action, Current/Recent rows, empty state, and pagination. New page has visible Objective label, continuous-Task explanation, Create task/Cancel, pending `Creating task`, preserved objective/errors, and replace navigation after accepted. Detail shell has back/Switch task, objective H1, status/intervention, task actions slot, Details IDs disclosure, and projection-owned children. Switching Task first closes SSE and destroys playback, then replaces projection. Register/show `#/runtime*` only when `runtime_workbench_enabled` is true.

- [ ] **Step 8: Verify the shell and commit**

Run with Node 24: `npm --prefix web run type-check && npm --prefix web test -- runtime.test.ts runtime-projection.test.ts runtime-url-state.test.ts runtime-new.test.ts`
Expected: PASS for headers, no 400/409 retry, unchanged-input retry key, replace navigation, hash/history parsing, event dedupe, association rules, stale stream rejection, and flag-off route absence.

```bash
git add web/src/api/client.ts web/src/api/runtime* web/src/types/runtime.ts web/src/ui/app.ts web/src/ui/views/runtime-*
git commit -m "feat(web): add runtime task shell and ordered projection"
```

### Task 7 (T7): Two-Column Agent Loop, Associated Devices, Seam, and Flow

**Files:**
- Create: `web/src/ui/views/{runtime-loop,runtime-devices,runtime-flow}.ts`, colocated CSS and tests
- Modify: `web/src/ui/views/runtime-task.ts`
- Test: `runtime-loop.test.ts`, `runtime-devices.test.ts`, `runtime-flow.test.ts`

**Interfaces:**
- Consumes: the single `RuntimeProjection` from T7A; no separate event store.
- Produces: `<runtime-loop>`, `<runtime-devices>`, `<runtime-flow>` and shared `locate-device` / `view-change` events.

- [ ] **Step 1: Write failing generic ToolCard and association-location tests**

```ts
it('renders job state without percentage and locates a referenced device', async () => {
  const loop = await fixture<RuntimeLoop>(html`<runtime-loop
    .projection=${projectionWithRunningJob('dev_1','job_1')}></runtime-loop>`);
  expect(loop.shadowRoot?.textContent ?? loop.textContent).toContain('running');
  expect(loop.textContent).not.toContain('%');
  loop.deviceLink('dev_1').click();
  expect(loop.locateEvents).toEqual([{ deviceId: 'dev_1' }]);
});

it('does not invent scenario cards for fixture-specific device types', async () => {
  const cards = renderRuntimeDevices(projectionFromFixtureB());
  expect(cards.querySelector('[data-scenario]')).toBeNull();
  expect(cards.deviceIds).toEqual(['fixture-b-camera', 'fixture-b-valve']);
});
```

- [ ] **Step 2: Run component tests and confirm missing Workbench components**

Run with Node 24: `npm --prefix web test -- runtime-loop.test.ts runtime-devices.test.ts`
Expected: FAIL on missing custom elements; do not adapt scenario-specific GoalWatch components.

- [ ] **Step 3: Implement compact Agent Loop and generic device cards**

```ts
private renderJob(job: RuntimeJob): TemplateResult {
  return html`<section class="tool-card" data-job-id=${job.id}>
    <header><strong>${job.tool}</strong><span>${job.state}</span></header>
    <p>${job.deviceId}</p>
    ${job.state === 'cancelled_unknown'
      ? html`<p role="alert">Stop result unknown</p><button @click=${this.refresh}>Refresh status</button>`
      : nothing}
    <details><summary>Details</summary><pre>${safeJson(job.argsJson)}</pre></details>
  </section>`;
}
```

Loop renders Reason/Act/Tool/Result, accepted/running facts, image evidence, Done/Details, and device links. Devices renders only actual associations in first-reference order. Each card uses declaration priority: first video source, else first image source, else key fields (or first three); also health, zone/position, Ready/Busy, active Job state, recent image evidence entry when video exists. No percentages, model-specific components, or `list_devices` associations. Clicking a Loop device/evidence emits locate; detail expands a collapsed rail, scrolls the card into view, and moves focus to its heading.

- [ ] **Step 4: Implement resizable seam with keyboard and persisted layout**

```ts
private onSeamKey(event: KeyboardEvent): void {
  if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
  event.preventDefault();
  const delta = event.key === 'ArrowLeft' ? -24 : 24;
  this.deviceWidth = clamp(this.deviceWidth + delta, 360, window.innerWidth * 0.75);
  localStorage.setItem('runtime.devices.width', String(this.deviceWidth));
}
```

Desktop ≥980px uses Loop/Devices plus a 36px collapsed rail. Separator exposes `role="separator"`, `aria-orientation="vertical"`, `aria-valuenow/min/max`, and width instructions. Pointer drag and ArrowLeft/Right share the same clamp. Central button toggles rail and persists expanded state. Pointer capture is released on completion/cancel; text inputs do not receive seam shortcuts.

- [ ] **Step 5: Implement Flow as a second view of the same projection**

```ts
const controlEdges = projection.events.filter(event => event.channel === 'control');
const dataEdges = projection.events.filter(event => event.channel === 'data');
```

Flow has CONTROL and DATA lanes with device ports aligned to cards; these labels are visual only, not protocol claims. Agent Loop/Flow are同级 tabs, default Loop, `?view=flow` restores with browser navigation, and Devices stays mounted across tab switches. Selecting a device does not enter history; selecting a historical Flow event sets `event`. Reduced motion replaces particles/pulse/smooth scrolling with static bus/icons/text.

- [ ] **Step 6: Verify layout semantics and commit**

Run with Node 24: `npm --prefix web test -- runtime-loop.test.ts runtime-devices.test.ts runtime-flow.test.ts && npm --prefix web run type-check`
Expected: PASS for generic cards, first-reference ordering, locate/focus, no fake progress, keyboard seam bounds/persistence, shared Flow ordering, and reduced-motion static rendering. Pixel/browser checks remain T7C/T8.

```bash
git add web/src/ui/views/runtime-{task,loop,devices,flow}*
git commit -m "feat(web): add runtime loop devices and flow workbench"
```

### Task 7B (T7B): Live/History, Pause/Resume, Cancel, Job Stop, and Approval Interaction

**Files:**
- Create: `web/src/ui/views/{runtime-task-controls,runtime-approval,runtime-confirm-dialog}.ts` and colocated tests
- Modify: `web/src/ui/views/{runtime-task,runtime-loop,runtime-url-state}.ts`
- Test: `runtime-task-controls.test.ts`, `runtime-approval.test.ts`, `runtime-confirm-dialog.test.ts`, `runtime-task.test.ts`

**Interfaces:**
- Consumes: T6 controls and T7A projection/URL state.
- Produces: safe-default Task cancel dialog, inline Job Stop confirmation, inline approval decisions, live/history reduction, and pending control states.

- [ ] **Step 1: Write failing pause and cancel semantics tests**

```ts
it('pause submits directly and explains active work continues', async () => {
  const controls = await controlsFixture(liveTask('active'));
  controls.pauseButton.click();
  expect(controls.pauseApi).toHaveBeenCalledOnce();
  expect(controls.cancelDialogOpen).toBe(false);
  expect(controls.text).toContain('Stops new runs; active jobs and watches continue');
});

it('cancel requires confirmation and defaults to keep running', async () => {
  const controls = await controlsFixture(runningTaskWithJobs());
  controls.openCancel();
  expect(controls.cancelApi).not.toHaveBeenCalled();
  expect(controls.primaryButton.textContent).toContain('Keep running');
  controls.primaryButton.click();
  expect(controls.cancelApi).not.toHaveBeenCalled();
  controls.dangerButton.click();
  expect(controls.cancelApi).toHaveBeenCalledOnce();
});
```

- [ ] **Step 2: Run control tests and confirm missing interaction components**

Run with Node 24: `npm --prefix web test -- runtime-task-controls.test.ts runtime-confirm-dialog.test.ts`
Expected: FAIL on missing controls/dialog; do not reuse the existing ConfirmModal whose default action differs.

- [ ] **Step 3: Implement pause/resume and safe-default cancel dialog**

```ts
renderCancelDialog(): TemplateResult {
  return html`<runtime-confirm-dialog
    .title=${'Cancel task'}
    .body=${cancelBody(this.projection)}
    .primaryLabel=${'Keep running'}
    .dangerLabel=${'Cancel task'}
    @confirm-primary=${this.closeCancel}
    @confirm-danger=${this.confirmCancel}>
  </runtime-confirm-dialog>`;
}
```

Pause/resume submits directly, disables repeats, shows `Pausing…`/`Resuming…`, and uses only the returned projection. Cancel dialog shows objective, active Job/watch count, device names, and unknown-result warning. Focus starts on Keep running, Tab/Shift+Tab are trapped, Escape closes before submission, and focus returns to the overflow trigger. After confirmation, closing the dialog does not undo cancellation; the header retains `Cancelling`. `cancelled_unknown` renders `Cancellation needs attention`, affected devices, and Refresh status; never `All stopped` or automatic retry.

- [ ] **Step 4: Write and pass inline Job Stop tests**

```ts
it('stops only the owned active job after inline confirmation', async () => {
  const card = await jobCardFixture({ id: 'job_1', taskId: 'task_1', state: 'running' });
  expect(card.stopButton.textContent).toContain('Stop job');
  card.stopButton.click();
  expect(card.stopApi).not.toHaveBeenCalled();
  card.confirmStop.click();
  expect(card.stopApi).toHaveBeenCalledWith('ws_1', 'task_1', 'job_1');
  expect(card.text).toContain('only this job');
});
```

Run before implementation: `npm --prefix web test -- runtime-loop.test.ts -t "owned active job"`; expected FAIL because no Stop control exists.
Implement Keep job/Confirm stop inside the originating ToolCard, pending `Stopping`, and `Stop result unknown`/Refresh for unknown. Show Stop only for live, nonterminal, owned accepted/running Jobs. Repeat clicks cannot issue another request; terminal/history/foreign Jobs have no executable Stop.

- [ ] **Step 5: Write failing history isolation tests**

```ts
it('freezes history, removes controls, and resyncs back to live', async () => {
  const task = await taskFixture('#/runtime/tasks/t1?view=flow&event=e2');
  expect(task.historyBanner.textContent).toContain('History · read-only');
  expect(task.executableControls).toEqual([]);
  task.receive(liveEvent('e3'));
  expect(task.projection.eventIds.has('e3')).toBe(false);
  task.backToLive.click();
  expect(task.route).toBe('#/runtime/tasks/t1?view=flow');
  expect(task.resyncCalls).toBe(1);
});
```

Run: `npm --prefix web test -- runtime-task.test.ts -t "freezes history"`; expected FAIL until the history reducer is wired.

- [ ] **Step 6: Implement history mode and stale/offline separation**

History requires `at` or `event`; selecting only a device stays live. Show persistent amber `History · read-only`, factual event time, and Back to live below the header. Remove Pause/Resume/Cancel/Stop and all other executable controls while retaining details and unexpired evidence. Do not merge new events into a frozen history projection. Back to live clears only `at/event`, preserves Task/view, and obtains snapshot/resync. Task switch clears old history/device/pending state. Stale/offline uses a separate visible connection banner and never masquerades as history or a terminal Task.

- [ ] **Step 7: Write failing inline approval and stale-response tests**

```ts
it('approves once with server identity and disables repeat submission', async () => {
  const view = await approvalFixture(pendingApproval());
  view.approveButton.click();
  view.approveButton.click();
  expect(view.decisionApi).toHaveBeenCalledOnce();
  expect(view.decisionApi).toHaveBeenCalledWith('ws_1', 'task_1', 'approval_1', 'approve',
    expect.any(AbortSignal));
  expect(view.approveButton.disabled).toBe(true);
  expect(view.text).toContain('Approving');
  await view.resolveDecision(approvedProjection());
  expect(view.text).toContain('Approved');
  expect(view.approveButton).toBeUndefined();
});

it('shows frozen arguments verbatim and refreshes after a decision conflict', async () => {
  const approval = pendingApproval();
  const view = await approvalFixture(approval);
  expect(view.args.textContent).toBe(safeJson(approval.argsJson));
  view.decisionApi.mockRejectedValue(new ApiError(409, 'Approval already decided'));
  view.approveButton.click();
  await view.updateComplete;
  expect(view.refreshApi).toHaveBeenCalledOnce();
  expect(view.text).toContain('Approval already decided');
  expect(view.approveButton.disabled).toBe(false);
});

it.each(['rejected', 'expired', 'unavailable'] as const)('renders %s without actions', async state => {
  const view = await approvalFixture({ ...pendingApproval(), state });
  expect(view.executableControls).toEqual([]);
  expect(view.text.toLowerCase()).toContain(state);
});

it('does not apply a decision response after Task switch', async () => {
  const view = await approvalFixture(pendingApproval());
  view.approveButton.click();
  await view.switchTask('task_2');
  await view.resolveDecision(approvedProjection());
  expect(view.projectionEvents).toEqual([]);
  expect(view.approveButton.disabled).toBe(false);
});
```

Add rejection, same-decision replay, 409 refresh, expired/unavailable, exact frozen-args display, keyboard focus, and history read-only tests. The browser never sends `args_json`, `decider_id`, or `decision_event_id`.

Run with Node 24: `npm --prefix web test -- runtime-approval.test.ts runtime-task.test.ts`
Expected: FAIL because the inline approval component and Task-switch generation guard do not exist.

- [ ] **Step 8: Implement the inline approval component**

```ts
private async decide(decision: 'approve' | 'reject'): Promise<void> {
  if (this.pendingDecision || this.approval.state !== 'pending' || this.readOnly) return;
  const generation = this.generation;
  this.pendingDecision = decision;
  try {
    const response = await decideRuntimeApproval(this.workspaceId, this.approval.taskId,
      this.approval.id, decision, this.controller.signal);
    if (!response.result) throw new ApiError(500, 'Approval projection missing');
    if (generation !== this.generation) return;
    this.emitProjection(response.result);
  } catch (error) {
    if (generation !== this.generation) return;
    if (error instanceof ApiError && error.code === 409) await this.refreshProjection();
    this.error = error instanceof ApiError ? error.message : 'Approval decision failed';
  } finally {
    if (generation === this.generation) this.pendingDecision = null;
  }
}
```

`runtime-approval` stays inline in the originating Run step and renders tool, device, read-only frozen `safeJson(argsJson)`, reason, risk, and expiry. Live pending rows show Approve once/Reject; pending submission disables both and announces `Approving…`/`Rejecting…`. Approved shows decider/time when present; rejected, expired, and unavailable have distinct text and no executable buttons. Use only returned server projections, never optimistic approval. Task switch aborts the old request, increments generation, and clears pending/error/focus context; history never renders controls. No direct device call or empty approval card.

- [ ] **Step 9: Verify the complete visible state matrix**

Run with Node 24: `npm --prefix web test -- runtime-task-controls.test.ts runtime-approval.test.ts runtime-confirm-dialog.test.ts runtime-loop.test.ts runtime-task.test.ts`
Expected: PASS for loading, empty, error, offline, partial, expired evidence, unsupported media, waiting, cancelled, pending pause/resume/cancel/stop, unknown, history, terminal, approval double-click/replay/conflict/expiry/unavailable, frozen arguments, and Task-switch stale-response isolation.

- [ ] **Step 10: Commit interaction completion**

```bash
git add web/src/ui/views/runtime-{task,loop,task-controls,approval,confirm-dialog,url-state}*
git commit -m "feat(web): complete runtime history approvals and task controls"
```

### Task 7C (T7C): Responsive, Keyboard, Theme, and Reduced-Motion Completion

**Files:**
- Create: `web/src/ui/views/runtime-workbench.css`, `web/src/ui/views/runtime-accessibility.test.ts`
- Modify: Runtime component CSS imports and `web/src/ui/views/runtime-task.ts`
- Test: Vitest semantics tests here; real viewport/contrast/focus checks in T8 Playwright

**Interfaces:**
- Consumes: existing global dark/light tokens in `web/src/styles/base.css` and Runtime DOM from T7/T7B.
- Produces: 980/600/400 responsive behavior, Runtime-local 44px touch targets, focus/screen-reader contracts, and motion media queries.

- [ ] **Step 1: Write failing accessible-name and focus-contract tests**

```ts
it('names tabs, separator, images, and status regions', async () => {
  const workbench = await workbenchFixture();
  expect(workbench.tabList.getAttribute('aria-label')).toBe('Runtime view');
  expect(workbench.seam.getAttribute('role')).toBe('separator');
  expect(workbench.seam.getAttribute('aria-valuemin')).toBe('360');
  expect(workbench.evidenceImage.alt).toContain('front');
  expect(workbench.connectionStatus.getAttribute('role')).toBe('status');
});

it('returns focus after dialogs and supports device navigation by keyboard', async () => {
  const workbench = await workbenchFixture();
  workbench.cancelTrigger.focus();
  workbench.openCancel();
  workbench.escape();
  expect(workbench.activeElement).toBe(workbench.cancelTrigger);
  workbench.deviceLink.focus();
  workbench.deviceLink.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
  expect(workbench.selectedDeviceHeading).toHaveFocus();
});
```

- [ ] **Step 2: Run accessibility tests and confirm semantic gaps**

Run with Node 24: `npm --prefix web test -- runtime-accessibility.test.ts`
Expected: FAIL on the unimplemented labels/focus contracts; do not fix semantics with color alone.

- [ ] **Step 3: Implement exact responsive layouts**

```css
.runtime-workbench { display: grid; grid-template-columns: minmax(0, 1fr) auto var(--runtime-device-width, 360px); }
.runtime-workbench button, .runtime-workbench [role="button"] { min-block-size: 44px; min-inline-size: 44px; }
@media (max-width: 979px) {
  .runtime-workbench { display: flex; flex-direction: column; }
  .runtime-seam { display: none; }
  .runtime-devices { order: 3; }
}
@media (max-width: 599px) {
  .runtime-view-tabs [data-view="devices"] { display: inline-flex; }
  .runtime-workbench[data-view="loop"] .runtime-devices,
  .runtime-workbench[data-view="flow"] .runtime-devices { display: none; }
}
@media (max-width: 399px) {
  .runtime-task-header { align-items: stretch; flex-direction: column; }
  .runtime-objective { overflow-wrap: anywhere; }
}
```

≥980px retains two columns/seam/rail. 600–979px order is header → Loop/Flow tabs → Associated devices with a visible anchor. <600px has同级 Loop/Flow/Devices tabs, one view at a time. <400px wraps long objective/actions without horizontal page scroll. Do not change unrelated shell breakpoints or global button sizes.

- [ ] **Step 4: Implement theme, typography, focus, and motion contracts**

```css
.runtime-workbench { color: var(--text-primary); background: var(--bg-primary); font-size: 16px; }
.runtime-data { font-size: 14px; font-variant-numeric: tabular-nums; }
.runtime-workbench :focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 3px; }
@media (prefers-reduced-motion: reduce) {
  .runtime-workbench *, .runtime-workbench *::before, .runtime-workbench *::after {
    animation: none !important;
    scroll-behavior: auto !important;
    transition: none !important;
  }
  .runtime-data-particles { display: none; }
}
```

Use existing tokens in both themes, no bordered dark dashboard or fixed light card islands, system font stack, body 16/data 14. Add visible labels, `aria-selected`/tab keyboard behavior, status live regions without announcing every telemetry frame, image alt text, native video controls, and textual offline/unavailable states. Following latest jumps immediately under reduced motion; user-initiated video remains manually controlled.

- [ ] **Step 5: Add state announcements without focus theft**

```ts
private announceTaskChange(previous: TaskStatus, next: TaskStatus): void {
  if (previous === next) return;
  this.announcement = `Task ${taskStatusLabel(next)}`;
}
```

Announce only Task/Job/control transitions, not every SSE packet or reading. Loading skeletons are `aria-hidden`, async errors use `role=alert`, and expanding Details does not move focus. Switching Task sets focus to the new objective H1 after old SSE/player cleanup.

- [ ] **Step 6: Verify semantics and commit responsive/accessibility slice**

Run with Node 24: `npm --prefix web test -- runtime-accessibility.test.ts runtime-task.test.ts runtime-flow.test.ts && npm --prefix web run lint && npm --prefix web run format:check`
Expected: PASS for semantics, keyboard focus, reduced-motion DOM, lint, and formatting. Browser viewport, zoom, contrast, and actual media destruction are not claimed until T8.

```bash
git add web/src/ui/views/runtime-*
git commit -m "feat(web): finish runtime responsive accessibility states"
```

### Task 8 (T8): Generality, Evals, Observability, Browser QA, and Rollout

**Files:**
- Create: `crates/agent/tests/fixtures/runtime_{a,b}/{manifest.json,task.json}`, `crates/agent/tests/runtime_generality.rs`, `apps/cloud/tests/runtime_eval.rs`, `docs/runbooks/runtime-rollout.md`
- Create: `web/{playwright.config.ts,e2e/runtime.spec.ts,e2e/fixture-server.ts}`
- Modify: `apps/cloud/src/domains/agent/runtime/admin.rs`, Task/Job/media/stream metrics emission, `web/package.json`, `web/.nvmrc`, `.github/workflows/ci.yml`
- Test: two-fixture integration, prompt/provider evals, rollout integration, Playwright Chromium

**Interfaces:**
- Consumes: every prior public interface; no new scenario API.
- Produces: reusable fixture acceptance, `DrainStatus`, runtime metrics/alerts, staged rollout runbook, and browser evidence.

- [ ] **Step 1: Add two declaration-only fixtures and a failing zero-scenario-branch test**

Fixture A manifest: simulated temperature + valve, objective `Read the greenhouse temperature, open the valve when above threshold, then verify`. Fixture B manifest: camera image + pump, objective `Inspect the entrance image, stop the pump if occupied, then verify`. Thresholds, IDs, sources, and expected evidence live only in each `task.json`; both use the same Runtime tools and UI.

```rust
#[tokio::test]
async fn two_fixtures_complete_without_scenario_branches() {
    for fixture in [RuntimeFixture::load("runtime_a"), RuntimeFixture::load("runtime_b")] {
        let outcome = fixture.run_to_terminal().await;
        assert_eq!(outcome.task.status, TaskStatus::Completed);
        assert!(outcome.jobs.iter().all(|job| job.state == DeviceJobState::Done));
        assert!(outcome.wake_ids.windows(2).all(|pair| pair[0] != pair[1]));
        assert_eq!(outcome.used_tools, fixture.expected_tools());
    }
    let source = include_str!("../src/runtime/thing_agent/task/coordinator.rs");
    for forbidden in ["greenhouse", "entrance", "temperature_above", "occupancy"] {
        assert!(!source.contains(forbidden), "scenario branch: {forbidden}");
    }
}
```

Run: `cargo test -p tinyiothub-agent --test runtime_generality -- --nocapture`
Expected: FAIL until both complete through declaration data alone. Do not add a fixture-named tool, prompt branch, card, or fallback to make the test pass.

- [ ] **Step 2: Add prompt, media, and long-Job eval assertions**

```rust
#[tokio::test]
async fn task_prompt_is_complete_and_deterministic() {
    let fixture = EvalFixture::large_task_with_unordered_facts().await;
    let a = build_task_prompt(&fixture.task, &fixture.runs, &fixture.jobs, &fixture.watches, &fixture.approvals, &fixture.snapshot);
    let b = build_task_prompt(&fixture.task, &fixture.runs, &fixture.jobs, &fixture.watches, &fixture.approvals, &fixture.snapshot);
    assert_eq!(a, b);
    for id in fixture.pending_ids() { assert!(a.contains(&id), "missing pending fact {id}"); }
    assert!(a.contains(&fixture.latest_stage_result));
}
```

Add a mock-vision assertion that exact JPEG/PNG bytes reach the provider and trace has only evidence refs; non-vision returns `image_unavailable`. Add the optional live-provider eval as `#[ignore]` and run it only with an already authorized staging provider configuration; never print keys, prompts containing private objective text, signed URLs, or image payloads. Record watch→Run and Job→UI local timings with targets <2s/<1s, but label them local integration evidence, not production p99.

Run: `cargo test -p tinyiothub-cloud --test runtime_eval -- --nocapture`
Expected: PASS for deterministic complete prompts, long-Job continuation, media capability failure, and no hidden total-prompt truncation. Live-provider results are reported separately and never inferred from mock success.

- [ ] **Step 3: Add structured metrics and failure alerts**

```rust
tracing::info!(metric="runtime_job_final", workspace_id=%job.workspace_id,
    task_id=?job.task_id, run_id=?job.run_id, job_id=%job.id,
    device_id=%job.device_id, state=?job.state, "runtime job final");
```

Emit counters/timers for Task created/completed/failed/cancelled/active/waiting; approval requested/decided/conflicted/expired/unavailable/decision-outbox lag/recovered execution, without logging frozen arguments; outbox pending/dispatched/dead/oldest/latency; Job states/duration/busy/timeout/unknown; watch hits/wake lag/rebuild failure; evidence bytes/failures/orphans; playback sessions/errors/destroy failures; SSE first frame/connections/resync/duplicates; DB lock/latency, RSS, and disk. Add dashboard queries and alerts for `outbox_dead>0`, oldest pending >60s, pending growth 5m, approved decisions without a Job or recorded deferral for >60s, `cancelled_unknown>0`, post-stop execution, Job deadline+60s, disk >80%, RSS >75%, media failure growth, SSE reconnect storm/resync duplicates, and stale player connections. These are observability thresholds only, not new fixed quotas.

- [ ] **Step 4: Implement drain readiness and flag regression tests**

```rust
pub struct DrainStatus {
    pub nonterminal_tasks: u64,
    pub active_runs: u64,
    pub active_jobs: u64,
    pub active_watches: u64,
    pub pending_approvals: u64,
    pub pending_outbox: u64,
    pub unresolved_unknown: u64,
}
impl DrainStatus {
    pub fn can_hide_workbench(&self) -> bool {
        self.nonterminal_tasks == 0 && self.active_runs == 0 && self.active_jobs == 0
            && self.active_watches == 0 && self.pending_approvals == 0
            && self.pending_outbox == 0 && self.unresolved_unknown == 0
    }
}
```

Test Task flag off: new create uses legacy directive, existing Task state/current Run/Job/watch/approval/outbox remain unchanged. Drain uses a fixed ID batch and calls existing Cancel per Task, never a bulk implicit status update. Cancel marks pending approvals unavailable and resolves their undispatched decisions before drain completes. Admin records unknown disposition before clearing `unresolved_unknown`; do not auto-retry side effects. Test that every nonzero drain category, including pending approvals, blocks Workbench/Media shutdown.

- [ ] **Step 5: Write the exact rollout/drain runbook**

Create `docs/runbooks/runtime-rollout.md` with these executable gates in order:

```markdown
1. Confirm declaration baseline `6876707f` from the current PR #101 branch, additive migration, `PRAGMA foreign_key_check`, and legacy API smoke.
2. Deploy all three flags false. Enable `runtime_tasks_enabled` for one staging workspace; run create replay, long Job, watch, cancel, and restart smoke.
3. Create a pending approval; verify double-click/replay creates one decision/outbox, frozen arguments survive restart, reject/expiry/cancel create no Job, and paused approve/reject/expiry each retain the original pending outbox until resume. Verify an approved crash resumes with at most one Job. Resolve any unknown physical result manually without replay.
4. Enable `runtime_media_enabled`; run mock/live vision capability, evidence expiry, safe descriptor, CORS, and player cleanup smoke.
5. Enable `runtime_workbench_enabled`; run keyboard/history/resync/responsive and approval conflict/Task-switch smoke and observe metrics for one hour.
6. Rollback: set only Task flag false, retain Workbench/Media, freeze a nonterminal Task ID batch, and call Cancel for each ID.
7. Record every `cancel_unknown` task/job/device ID, inspect physical device state, and enter an administrator disposition before clearing it.
8. Require DrainStatus all zero, including pending approvals and decision outbox. Only then disable Workbench, then Media. Keep additive rows/evidence; never drop tables or use flags as emergency stop.
9. Recovery enables Task → Media → Workbench only after the failed smoke gate passes.
```

Include 5-minute checks (migration/FK, old API, same-key same-ID, real Job terminal, no paused/percentage, stream connects) and 1-hour checks (local targets, dead/unknown disposition, vision/unavailable honesty, no background video, dashboards/alerts). The estimated 15-minute window never overrides drain completion.

- [ ] **Step 6: Install browser tooling and serve real local HLS fixtures**

Run with Node 24:

```bash
npm --prefix web install -D @playwright/test@1.56.1 @axe-core/playwright@4.10.2
npm --prefix web exec playwright install chromium
mkdir -p web/e2e/fixtures/media
ffmpeg -y -f lavfi -i color=c=blue:s=320x180:d=2 -f lavfi -i sine=frequency=440:duration=2 \
  -c:v libx264 -pix_fmt yuv420p -c:a aac -f hls -hls_time 1 -hls_list_size 3 \
  web/e2e/fixtures/media/live.m3u8
```

Add `test:browser: "playwright test"` to `web/package.json`. `fixture-server.ts` serves the playlist/segments on localhost with `Access-Control-Allow-Origin` only under `/allowed/`; `/denied/` omits it. URLs use synthetic `?sig=fixture`, never real credentials. Configure Playwright Node 24, Vite, and this fixture server; tests use Chromium at 1280, 768, 390, and 360px widths.

- [ ] **Step 7: Write and run real browser lifecycle, responsive, and AA tests**

```ts
test('task switch destroys playback and blocks stale media', async ({ page }) => {
  await openRuntimeTask(page, 'task_video');
  await page.getByRole('button', { name: 'Play evidence' }).click();
  await expect(page.locator('video')).toBeVisible();
  await page.getByRole('button', { name: 'Switch task' }).click();
  await page.getByRole('option', { name: 'task_without_video' }).click();
  await expect(page.locator('video')).toHaveCount(0);
  const oldOriginRequests = countFixtureMediaRequests();
  await page.waitForTimeout(1500);
  expect(countFixtureMediaRequests()).toBe(oldOriginRequests);
  expect(await page.evaluate(() => window.__runtimeActivePlayers ?? 0)).toBe(0);
});

test('responsive tabs, keyboard history, reduced motion, and AA remain usable', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await openRuntimeTask(page, 'task_history');
  await page.getByRole('tab', { name: 'Devices' }).click();
  await expect(page.getByRole('heading', { name: 'Associated devices' })).toBeVisible();
  await page.getByRole('tab', { name: 'Flow' }).click();
  await page.getByRole('button', { name: 'Historical event event_2' }).click();
  await expect(page.getByText('History · read-only')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Cancel task' })).toHaveCount(0);
  expect(await page.locator('.runtime-data-particles').isVisible()).toBe(false);
  const results = await new AxeBuilder({ page }).withTags(['wcag2a','wcag2aa']).analyze();
  expect(results.violations).toEqual([]);
});

test('approval stays single-flight and stale decisions cannot cross Tasks', async ({ page }) => {
  await openRuntimeTask(page, 'task_approval');
  const approve = page.getByRole('button', { name: 'Approve once' });
  await approve.evaluate((button: HTMLButtonElement) => { button.click(); button.click(); });
  await expect(page.getByText('Approved', { exact: true })).toBeVisible();
  expect(countApprovalDecisionRequests()).toBe(1);

  await openRuntimeTask(page, 'task_approval_delayed');
  await page.getByRole('button', { name: 'Approve once' }).click();
  await openRuntimeTask(page, 'task_without_video');
  releaseDelayedDecision();
  await expect(page.getByRole('button', { name: 'Approve once' })).toHaveCount(0);
  await expect(page).toHaveURL(/#\/runtime\/tasks\/task_without_video/);

  await openRuntimeTask(page, 'task_approval_history');
  await expect(page.getByText('History · read-only')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Approve once' })).toHaveCount(0);
});
```

`countApprovalDecisionRequests()` reads only the fixture server's synthetic decision counter, and `releaseDelayedDecision()` resolves its held fixture response; reset both between tests.

Add CORS success/failure, expired signature, unsupported protocol, stale SSE/resync, 200% zoom, 360px no horizontal overflow, seam pointer/keyboard, focus trap/return, light/dark AA, expired evidence, empty/error/offline/partial, safe-default cancel, and approval reject/conflict/expired/unavailable tests. `window.__runtimeActivePlayers` is a test-only counter enabled only when `VITE_RUNTIME_TEST=1`; set that variable on the Playwright-managed Vite server, never on normal development or production builds, because `import.meta.env.MODE` is not a reliable browser-test switch. Production has no global player registry. `countFixtureMediaRequests()` reads only the local fixture server's synthetic-request counter; `openRuntimeTask(page, id)` selects an already-seeded fixture Task through the Workbench URL. Reset media, player, and approval counters between tests.

Run with Node 24: `npm --prefix web run test:browser`
Expected: PASS in Chromium. CORS failure shows `playback_unavailable`, no Hub request proxies media, and Task switch leaves zero player/media activity. If a test fails, record it and fix the implementation; do not replace browser assertions with jsdom.

- [ ] **Step 8: Run the complete pre-PR validation and record honest limits**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p demo-declarative
cargo test --workspace
npm --prefix web run lint
npm --prefix web run format:check
npm --prefix web run type-check
npm --prefix web test
npm --prefix web run build
npm --prefix web run test:browser
```

Use Node 24 for every npm command. Run the full migration chain against a Linux file-backed SQLite database and check `PRAGMA foreign_key_check`; an in-memory baseline alone is insufficient. Run authorized staging smoke for Task → Media → Workbench and the drain runbook. Report two-fixture/mock/local-browser evidence separately from real hardware, production p99, live-provider, and all-camera compatibility; none of those broader claims is proven by this suite.

- [ ] **Step 9: Commit the final staged slice after approval acceptance**

```bash
git add crates/agent/tests apps/cloud/tests apps/cloud/src/domains/agent/runtime docs/runbooks/runtime-rollout.md web .github/workflows/ci.yml
git commit -m "test(runtime): verify generality approvals observability and rollout"
```

Keep the complete Runtime PR as one staged branch with all three flags default off. Do not push, open, or merge it without separate user authorization. Approval implementation and the crash/browser/staging checks above are required acceptance, not optional follow-up work.

---

## Final Review Checklist for the Implementer

- [ ] All 12 reviewed delivery tasks T1,T2,T3,T4,T5,T6,T6A,T7A,T7,T7B,T7C,T8 have passing focused tests and staged commits.
- [ ] `rg -n "PendingActionStore|take_pending_action_reasoned" crates/agent/src/runtime/thing_agent/task apps/cloud/src/domains/agent/runtime` returns no Task-approval use; approval display/execution use identical frozen arguments and original `tool_call_id`.
- [ ] Approval decision CAS/outbox is atomic, every named crash boundary resumes idempotently with at most one Job, and reject/expired/unavailable/cancel-before-decision create none.
- [ ] No Task flag transition mutates existing Tasks; no device-wide stop implements Job Stop; no paused Job or percentage progress exists.
- [ ] Every RunStarted/RunReport Task pair has identical `wake_event_id`; duplicate outbox delivery cannot create another Run.
- [ ] Images reach a vision-capable provider, non-vision fails explicitly, and trace/ledger/logs contain no image bytes or signed query.
- [ ] Browser direct playback is destroyed on Task switch; CORS/signature/protocol failures are visible and never fall back to Hub proxying.
- [ ] One complete PR remains staged with three flags default off; R2 follow-up A is implemented and tested, with no push or PR opened without separate authorization.

---

## Focused R2 Engineering Review — 2026-10-27

Evidence kind: static review of the design, implementation plan and current baseline source. No Runtime approval implementation exists; no implementation build, test, browser QA, hardware, live-provider or production-performance claim is made.

## Scope Challenge Result

- Selected targets: `docs/superpowers/specs/2026-10-26-multi-device-agent-runtime-design.md` and this implementation plan, restricted to R2 follow-up A and its T6A/T7B/T8/gate contracts.
- Structure: reuse the user's exact prior **Original arrangement** answer (`docs/superpowers/specs/2026-10-26-multi-device-agent-runtime-design.md:3325-3329`). Scope accepted as-is; no file/service consolidation and no PR split.
- Accepted scope: durable approval decisions, frozen execution arguments, atomic decision/outbox, Job/Task race handling, crash-idempotent continuation, UI/API and acceptance coverage.
- Current baseline gate: PR #101 was rechecked through `gh pr view 101` and remains OPEN with `mergedAt:null`; the prerequisite at `docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:15` is therefore not yet satisfied.
- Outside coverage is unavailable, not a clean-review substitute.

## Reality rows

```text
REALITY: premise-table pass R2 is approved for planning but not implemented; current Runner creates its own token and worker commands lack job identity (docs/superpowers/specs/2026-10-26-multi-device-agent-runtime-design.md:3013-3046, crates/agent/src/runtime/thing_agent/runner.rs:180-220, crates/runtime/src/driver/worker.rs:129-138)
REALITY: already-done pass E1-E4 infrastructure is reusable but durable Task approvals and Jobs are planned; PR #101 remains the explicit unmerged baseline (docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:15, docs/superpowers/specs/2026-10-26-multi-device-agent-runtime-design.md:859)
REALITY: surface-check pass T6A/T7B/T8 routes, stores, controls and acceptance are planned surfaces rather than existing APIs (docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:1254-1264, docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:1818-1825, docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:2101-2110)
REALITY: binding-decisions pass the current R2 A answer and prior Original arrangement answer govern; historical D2=C remains history only (docs/superpowers/specs/2026-10-26-multi-device-agent-runtime-design.md:3013-3046, docs/superpowers/specs/2026-10-26-multi-device-agent-runtime-design.md:3325-3329)
REALITY: numbers pass existing budgets and media limits are preserved, no new approval TTL or fixed Task quota is introduced, and local timing targets are not production measurements (docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:24-30, docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:2295)
REALITY: deferred-asks pass PR #101 verification remains an implementation prerequisite; prior font and WebVTT dispositions stay deferred and nonblocking (docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:15, docs/superpowers/specs/2026-10-26-multi-device-agent-runtime-design.md:3679-3690)
REALITY: pr-count pass one complete Runtime PR with staged commits and three default-off flags remains binding (docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:15, docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:2304-2317)
REALITY: prior-art pass reuse existing declaration validation, wake dedup, HubLedger and worker instead of adding a parallel execution system (docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:527-551, docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:665, docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:1208)
REALITY: transaction-semantics pass decision plus original outbox is atomic; new Job plus Task transition is atomic; existing Job matching precedes the no-Job state guard (docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:1397, docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:1539-1541)
REALITY: contract-assumptions pass display and execution share frozen fields and correlation IDs; rejected decisions create no execution intent and unknown physical outcomes never auto-replay (docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:20, docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:1261-1264, docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:1539-1543)
REALITY: units pass existing byte and duration limits retain their established units; R2 adds no global fixed approval TTL (docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:24-26, docs/superpowers/specs/2026-10-26-multi-device-agent-runtime-design.md:3036-3039)
REALITY: perf-table pass no benchmark is claimed; outbox/approval alerts are observability thresholds and broader performance remains for later measurement (docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:2164, docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:2295)
REALITY: dependency-fallback pass implementation waits for actual PR #101 merge; all three Runtime flags default off and rollback requires drain (docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:15, docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:27-28)
REALITY: obligation-tiers pass durable approval execution, UI controls and T6A/T7B/T8 acceptance are required; physical exactly-once and unverified live behavior are not claimed (docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:14-31, docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:2304-2317)
```

## Architecture Review

### F1 — Paused non-execution decisions could lose their only wake

- Severity: High; confidence: 10/10; status: fixed in the plan.
- Motivating contract: pause must leave the original decision outbox pending (`docs/superpowers/specs/2026-10-26-multi-device-agent-runtime-design.md:3036-3039`).
- Original defect: the rejected/expired/unavailable branch discarded `Advance::Deferred` and returned `NoExecution`, allowing the generic router rule to mark the wake dispatched (`docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:665`).
- Failure scenario: reject or expire an approval while its Task is paused; Task advancement defers, but the only decision wake is marked dispatched. Resume has no pending work to retry and the Task can remain stuck in its approval relationship.
- Remedy applied within the already approved R2 pause contract: map `Advance::Deferred` to `ExecutionOutcome::Deferred`; only non-deferred non-execution advancement returns `NoExecution` (`docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:1524-1534`).
- No new architecture or product decision was introduced.

No additional architecture finding survived the transaction, correlation, existing-Job, cancellation and recovery checks. In particular, `submit_approved` returns an existing Job without blindly re-enqueueing it; only a newly inserted Job goes to the worker once (`docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:1539`). Database uniqueness is not treated as proof of physical exactly-once.

## Code Quality Review

_No new findings._ The frozen `ApprovedToolCall` has a single persisted source, API handlers accept no browser execution fields, errors remain typed, and SQL stays in `crates/db` (`docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:1468-1477,1539-1543`). The old `PendingActionStore`/heartbeat flip-then-execute path remains explicitly disallowed rather than being wrapped as a substitute.

## Test Review

```text
approve/reject/expire CAS + original outbox (one transaction)
                 |
                 v
       WakeRouter / ApprovalExecutor
          |                  |
 approved |                  | rejected/expired/unavailable
          v                  v
 existing Job match      coordinator.advance
 or Job+Task transaction       |            |
          |               Deferred      advanced
          v                  |            |
 new Job -> worker once   keep pending   mark dispatched
          |                  ^            |
          v                  | resume     v
 terminal Job -> original Task wake ----> original Task
```

### F2 — The paused reject/expiry regression path was not executable

- Severity: High; confidence: 10/10; status: fixed in the plan and QA artifact.
- Motivating lines: the generic requirement said paused decisions remain undispatched, but the concrete tests only exercised approval recovery and cancellation (`docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:1404-1479`).
- Value card: protects=original decision wake and Task liveness; fails_when=a deferred reject/expiry is acknowledged as dispatched; why_new=durable non-execution approval facts are new; seam=ApprovalExecutor + WakeRouter; no_claim=physical exactly-once or live hardware behavior.
- Remedy: added concrete paused-reject and paused-expiry tests that assert pending before resume and dispatched after resume (`docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:1440-1477`), plus the same staged runbook gate at `docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:2196`.
- The existing four crash boundaries, atomic rollback, frozen arguments, zero-Job reject/expiry/unavailable/cancel, double-click, conflict, stale Task-switch response and history assertions remain required. They are planned tests, not passed tests.

### Failure modes

- Decision/outbox DB failure: transaction rolls back; pending can be decided again, with zero partial execution intent. Covered by T6A rollback tests; user sees a retryable error, not success.
- Pause during any decision: original outbox stays pending; F1/F2 cover both execution and non-execution branches. Resume retries the same event; no silent acknowledgement.
- Crash after Job insertion: existing Job identity prevents a second row; recovery restores recorded facts and never blindly re-enqueues accepted/running work. An unrecoverable/interrupted result remains visible instead of being replayed.
- Opposite decision or stale UI response: server returns conflict/current projection; browser refreshes and ignores stale generations. No client argument override.
- Unknown physical result: Task/Job exposes unknown/interrupted state and requires disposition; no test or runbook claims automatic physical resolution.

No failure mode with neither test nor error handling remains silent in the reviewed R2 path. Critical gaps: 0.

## Performance Review

_No new findings._ Approval expiry has an `(expires_at,state)` index and decision/Job lookups use unique identities (`docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:1313-1335,408-428`). Recovery/outbox work uses existing bounded scan/backpressure mechanisms; no measured throughput or production latency is claimed. The added paused branches are constant-time and do not add queries to the model path.

## Outside Voice

Codex execution failed before review because its native binary was missing (`ENOENT`). The mandated native Plan fallback was stopped after the five-minute host timer completed first; no complete report was accepted and no late/partial content was used. Coverage is **unavailable**, never clean or PASS. No outside finding or cross-model agreement is claimed.

## NOT in scope

- Splitting the approved complete Runtime PR or reducing the approved Task/Media/Workbench scope.
- Reusing process-local pending actions or heartbeat flip-then-execute as the durable approval mechanism.
- Automatic replay of unknown physical effects, or claims of physical exactly-once.
- New fixed Task/media/outbox/video quotas or a hidden total Task-prompt cap.
- Dedicated Runtime font selection and WebVTT captions, which retain their prior nonblocking design-deferred dispositions.
- Starting implementation, merging PR #101, pushing, opening a PR, or enabling rollout flags as part of this review.

## What already exists

- Reuse: declaration/argument validation, worker queue and cancellation primitives, Task/outbox/Run dedup design, HubLedger ordering, existing Agent admin/workspace guards, current UI client/projection patterns and existing eval entry points.
- Build: durable `task_approvals`, ApprovalExecutor, `submit_approved`, approval transport/projection and their crash/UI/browser acceptance.
- Keep separate: old heartbeat approval behavior is not the new Task approval implementation; existing device-wide cancellation is not single-Job Stop.
- PR #101 is still an external baseline prerequisite, not an already merged local capability.

## TODOS.md updates

No new TODO proposed in this focused review. The two existing Runtime deferred items were not changed by this review.

## Worktree parallelization strategy

Sequential SDD implementation for the approved task order `T1 → T2 → T3 → T4 → T5 → T6 → T6A → T7A → T7 → T7B → T7C → T8`. The settled design contains optional coordinated lanes, but the user selected Subagent-Driven Development with one implementer at a time; this review does not reopen that execution decision. All work remains one complete PR.

## Implementation Tasks

No thirteenth delivery task is needed. F1/F2 are corrections inside the already approved T6A/T8 scope.

- [ ] **T6A (P1, human: ~4d / CC: ~4h [estimated])** — Durable approvals — preserve deferred non-execution decisions and prove crash/control idempotency
  - Surfaced by: Architecture F1 and Test F2; original R2 follow-up A.
  - Files: `crates/agent/src/runtime/thing_agent/task/approval.rs`, `crates/db/src/task_approval.rs`, `crates/runtime/src/job/service.rs`, approval API/projection tests.
  - Verify: `cargo test -p tinyiothub-storage task_approval -- --nocapture && cargo test -p tinyiothub-agent thing_agent::task::approval -- --nocapture && cargo test -p tinyiothub-cloud domains::agent::runtime::approvals -- --nocapture && cargo test -p tinyiothub-runtime job:: -- --nocapture`
- [ ] **T8 (P1, human: ~5d / CC: ~5h [estimated])** — Acceptance and rollout — include paused approve/reject/expiry recovery in staging evidence
  - Surfaced by: Test F2 and the existing rollout gate.
  - Files: `docs/runbooks/runtime-rollout.md`, Runtime approval/browser/staging acceptance fixtures.
  - Verify: the full T8 Rust/web/browser gates and authorized staging runbook; broader hardware/provider/production claims remain separate.

## Unresolved decisions that may bite you later

None in this focused review. PR #101 being OPEN is a verified prerequisite state, not an unanswered design choice. Prior design-deferred font/caption records remain historical nonblocking deferrals.

## Decision ledger

- R2 follow-up A: actual user answer **“持久化并幂等续跑”**; durable approvals, frozen arguments, atomic original outbox and idempotent continuation approved (`docs/superpowers/specs/2026-10-26-multi-device-agent-runtime-design.md:3013-3046`).
- Scope structure: exact prior **Original arrangement** answer reused; no new scope question (`docs/superpowers/specs/2026-10-26-multi-device-agent-runtime-design.md:3325-3329`).
- F1/F2 remedy: no new choice; it implements the already approved pause/defer/original-event contract and its required regression evidence.
- Execution: preserve selected Subagent-Driven Development, one complete PR, staged task order and three default-off flags. No push/PR authorization granted.
- Baseline: before Task 1, verify PR #101 is merged; its current OPEN state does not satisfy that gate.

Approval readiness: PASS — checked R2 follow-up A actual answer, exact Original arrangement approval, existing pause/defer regression contract and selected execution constraints. F1/F2 require no additional remedy decision. Implementation remains subject to the PR #101 prerequisite and all planned acceptance gates.

## Completion Summary

- Step 0: Scope Challenge — scope accepted as-is (FULL_REVIEW; one complete PR).
- Reality rows: 14/14 applicable rows emitted (`PLAN_REALITY: phase=eng rows=14/14 missing=none verdict=complete`), 0 unverified premises.
- Architecture Review: 1 issue found and fixed in the plan.
- Code Quality Review: 0 issues found.
- Test Review: diagram produced, 1 gap identified and fixed in the plan/QA artifact.
- Performance Review: 0 issues found.
- NOT in scope: written.
- What already exists: written.
- TODOS.md updates: 0 items proposed to user.
- Failure modes: 0 critical gaps flagged after the planned F1/F2 corrections; implementation evidence is still absent.
- Unresolved decisions: 0 in this review.
- Outside voice: Codex unavailable (native binary ENOENT); native Plan fallback incomplete within five minutes; outside coverage unavailable.
- Parallelization: 1 selected SDD lane, 0 parallel / 12 sequential delivery tasks; design alternatives not reopened.
- Lake Score: N/A (no new completeness-scored decision in this focused review).

## Suppressed findings

None. No outside report was available to suppress or adjudicate, and no existing deferred item was silently discarded.

## Focused R2 Corrective Re-review — 2026-10-27

Evidence kind: static re-review of only the corrected paused decision/outbox branch, its T6A/T8/QA regression contracts, and current gates. The earlier focused review above remains historical context. No implementation code was changed and no implementation build, test, browser QA, hardware, live-provider or production-performance claim is made.

## Scope Challenge Result

- Selected targets remain the Runtime design and this implementation plan. This pass reviews only F1/F2 corrections: rejected/expired/unavailable defer propagation, original decision-outbox retention, same-event resume, regression evidence and gate consistency.
- Scope accepted as-is (FULL_REVIEW). The user's D4 answer **“复核修正版”** authorizes this corrective re-review, not a reopened R2 product scope or implementation.
- The original R2 follow-up A answer, staged task order and three default-off flags remain binding. During navigation, D6 superseded only the merge-first delivery gate: the actual answer **“全部并入 PR #101”** selects one combined declaration + Runtime PR on the current branch.
- PR #101 was rechecked: its source is the current `e4-p2-declaration-closeout` branch, its remote head is declaration commit `6876707f`, and local HEAD adds three Runtime documentation commits. D6 authorizes post-review implementation on this baseline before PR #101 merges; push/merge remains separately gated.

## Reality rows

```text
REALITY: premise-table pass F1/F2 are plan corrections under review, not implemented behavior; current evidence is the corrected executor, WakeRouter contract and planned fixtures (docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:62, docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:652-665, docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:1440-1479, docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:1517-1541)
REALITY: already-done pass origin/main was fetched and searched for the Runtime plan/design, task/job source and approval symbols; no merged implementation was found. Declaration commit 6876707f is present in both PR #101 and local HEAD, and D6 selects it as the implementation baseline (docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:15)
REALITY: surface-check pass dispatch_decision and outbox_dispatched are planned ApprovalFixture routes over real persistence, not claims of existing APIs; T6A/T8 remain unchecked delivery work (docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:62, docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:1254-1264)
REALITY: binding-decisions pass D4 selected corrective re-review; R2 follow-up A, historical D2=C preservation, one PR and selected SDD remain unchanged (docs/superpowers/specs/2026-10-26-multi-device-agent-runtime-design.md:3013-3047, docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:2304-2317)
REALITY: numbers pass this correction adds no quotas, TTLs, timing targets or measured values; prior human/CC efforts remain estimates and no benchmark is claimed (docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:24-30, docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:2295)
REALITY: deferred-asks pass D6 removes the PR #101 merge-first wait and selects implementation into that existing PR; prior Runtime font and WebVTT dispositions remain nonblocking deferrals (docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:15, TODOS.md:59-77)
REALITY: pr-count pass D6 selects one combined PR #101 for declaration closeout plus the complete Runtime, with staged commits and three default-off flags; no second Runtime PR or cross-PR queue is introduced, and the user accepted the larger review/rollback boundary (docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:15, docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:2304-2317)
REALITY: prior-art pass the correction reuses the planned coordinator, WakeRouter and ApprovalExecutor rather than adding a second pause or outbox mechanism (docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:644-665, docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:1517-1541)
REALITY: transaction-semantics pass decision plus original outbox remains atomic, Job insertion plus Task transition remains atomic, and Deferred never acknowledges the original wake (docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:1397, docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:1528-1541)
REALITY: contract-assumptions pass rejected/expired/unavailable decisions create no Job; existing Job matching and unknown-result rules remain unchanged (docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:1539-1543, docs/superpowers/specs/2026-10-26-multi-device-agent-runtime-design.md:3032-3044)
REALITY: units pass no unit or numeric contract changed; existing byte and duration units are untouched (docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:24-26)
REALITY: perf-table pass no performance table or benchmark is introduced; the match arm and regression assertions have no claimed production latency or throughput (docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:1528-1534, docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:2164)
REALITY: dependency-fallback pass D6 permits implementation on the current PR #101 branch before merge; the final combined PR must contain both declaration closeout and Runtime, while all three Runtime flags remain default off and rollback still requires drain (docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:15, docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:27-28)
REALITY: obligation-tiers pass the corrected branch and its T6A/T8/QA evidence are must-ship; physical exactly-once and live behavior are not claimed (docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:1440-1479, docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:2191-2205)
```

## Architecture Review

_No new findings._ F1 is verified closed. The single non-execution match arm covers `Rejected | Expired | Unavailable`; only `Advance::Deferred` becomes `ExecutionOutcome::Deferred`, while other advancement outcomes remain `NoExecution` (`docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:1528-1534`). WakeRouter leaves the original wake pending on `Deferred`, and resume retries the same `decision_event_id` without creating another event (`docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:665,1541`). The design requires exactly this behavior (`docs/superpowers/specs/2026-10-26-multi-device-agent-runtime-design.md:3036-3040`).

The approved branch is unchanged: `submit_approved` returns `None` for its deferred path, existing Job lookup and frozen-field matching still precede insertion, and only a newly inserted Job is dispatched once (`docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:1521-1526,1539`). Cancel invalidation, terminal facts and unknown/interrupted recovery are not weakened.

Two shapes before one: not applicable. This corrective re-review introduces no new module, service, package or boundary and does not reopen the settled Runtime architecture.

## Code Quality Review

_No new findings._ The correction is a minimal result-mapping change in the existing executor. It adds no parallel state, implicit retry, duplicate wake, broad wildcard acknowledgment or client-controlled execution field. Typed `TaskError` propagation remains intact, and the planned SQL remains in `crates/db`.

## Test Review

```text
paused Task + rejected/expired/unavailable decision
                         |
                         v
              ApprovalExecutor.execute_decision
                         |
                         v
              coordinator.advance -> Deferred
                         |
                         v
              ExecutionOutcome::Deferred
                         |
                         v
        original decision outbox remains pending
                         |
              resume -> same event_id
                         v
       advance succeeds -> NoExecution -> dispatched
```

F2 is verified closed for the corrected defect. The concrete paused-reject and paused-expiry tests drive `dispatch_decision`, assert `Deferred`, assert the original outbox is not dispatched, resume, then assert `NoExecution` and dispatched (`docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:1440-1477`). The fixture contract explicitly places those helpers over the real migrated DB, coordinator/executor and persistence routes (`docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:62`), so the post-resume assertion covers router acknowledgment rather than only an isolated return value.

Paused unavailable remains mandatory in the T6A control-race matrix (`docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:1479`), and the QA artifact independently requires approved/rejected/expired/unavailable defer with zero Jobs for non-execution decisions. T8's staged smoke gate covers paused approve/reject/expiry (`docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md:2196`); it does not replace or waive the unavailable unit/QA obligations. No new paused-unavailable gap is inferred merely from sharing the already reviewed non-execution match arm.

Value card: protects=original decision identity and Task liveness across pause/resume; fails_when=a deferred decision is marked dispatched or resume creates a replacement event; why_new=durable non-execution approval recovery is new; seam=ApprovalExecutor + WakeRouter + ApprovalFixture; no_claim=implementation tests already passed, physical exactly-once or live hardware behavior.

### Failure modes

- Paused reject/expiry/unavailable: `Deferred` reaches WakeRouter, which leaves the original outbox pending. T6A and QA cover the control path; the user does not receive a false completed advancement.
- Resume after defer: the same event is retried and acknowledged only after non-deferred advancement. Paused reject/expiry tests assert both sides; unavailable remains required by the control matrix and QA artifact.
- Approved execution defer or crash: the original intent remains recoverable; existing Job identity and frozen-field matching prevent a second row, and only new Jobs are dispatched. Existing T6A crash boundaries remain required.
- Cancel/terminal or unknown physical result: undispatched intents are invalidated through the approved cancellation flow; unknown/interrupted effects remain explicit and are never blindly replayed.

No reviewed failure mode is left with neither planned test coverage nor error handling. Critical gaps: 0. These are plan conclusions, not implementation evidence.

## Performance Review

_No new findings._ The correction adds one enum result mapping on an existing decision path and no new query, scan, allocation-heavy loop, quota or blocking call. Scale and throughput are unchanged and unmeasured; no production-performance claim is made.

## Outside Voice

Codex preflight found the CLI but rejected the custom endpoint because no frontier plan-review model is configured; no Codex call was made. The mandated bounded native Plan fallback did not complete before the five-minute timer and was stopped successfully. A late result received after the timer was given no credit and was not used for findings, agreement or clean status. Outside coverage is **unavailable**, not clean or PASS.

## NOT in scope

- Reopening R2 product semantics, one-PR structure, media, UI, fonts, captions or unrelated Runtime architecture.
- Adding another pause, approval, outbox or recovery mechanism for an already corrected branch.
- Starting implementation during the review itself, committing, pushing, merging PR #101, opening another Runtime PR or enabling rollout flags. D6 authorizes post-review implementation on the existing branch; commit/push/merge authorization remains separate.
- Claiming implementation tests, browser QA, staging, hardware, live-provider or production-performance verification.

## What already exists

- Reuse: the planned T3 coordinator/WakeRouter defer contract, T6A ApprovalExecutor and ApprovalFixture, existing T6A crash matrix, T8 rollout runbook and the QA artifact.
- Corrected in the plan: non-execution `Advance::Deferred` propagation and concrete paused reject/expiry regressions.
- Still to build: all durable Runtime approval behavior and its tests. The current source tree does not provide evidence that these planned interfaces already exist.
- Delivery baseline: declaration commit `6876707f` is in PR #101 and local ancestry; local HEAD adds three Runtime documentation commits. D6 makes the current branch the combined implementation line.

## TODOS.md updates

No new TODO is proposed. Existing Runtime font and WebVTT deferrals retain their prior dispositions and were not modified by this re-review.

## Worktree parallelization strategy

Sequential implementation, no parallelization opportunity in this corrective re-review. The selected SDD task order remains `T1 → T2 → T3 → T4 → T5 → T6 → T6A → T7A → T7 → T7B → T7C → T8`, with one fresh implementer at a time and one combined PR #101. No worktree or implementation was started during the review.

## Implementation Tasks

_No new tasks from Architecture._
_No new tasks from Code Quality._
_No new tasks from Test._
_No new tasks from Performance._

F1/F2 remain owned by existing T6A and T8; no thirteenth delivery task or additional remedy is needed. This zero-new-task re-review wrote the empty JSONL artifact `~/.gstack/projects/Grong-tinyiothub/tasks-eng-review-20261027-185608.jsonl`; the earlier T6A/T8 artifact remains unchanged.

## Unresolved decisions that may bite you later

None in this re-review. D6 settled the PR #101 delivery strategy; implementation acceptance remains verification work, not an unanswered design choice. Prior design-deferred font/caption records remain nonblocking historical deferrals.

## Decision ledger

- D4: actual user answer **“复核修正版”**; only the corrected pause/outbox branch and new tests were reopened for review.
- R2 follow-up A: actual user answer **“持久化并幂等续跑”** remains the approved durable approval and idempotent continuation contract (`docs/superpowers/specs/2026-10-26-multi-device-agent-runtime-design.md:3013-3047`).
- F1/F2: no new remedy decision; the corrections implement the exact previously approved pause/defer/original-event behavior and regression evidence.
- Execution: preserve selected Subagent-Driven Development, staged task order and three default-off flags. D6 actual answer **“全部并入 PR #101”** selects the current branch and one combined declaration + Runtime PR instead of a separate Runtime PR. No push/merge authorization was granted.
- Baseline: use PR #101's declaration commit `6876707f`, already in local ancestry. The earlier merge-first prerequisite is superseded by D6; implementation may begin after this review on the current branch.
- Commit boundary: the pre-existing no-commit constraint remains in force; D6 did not explicitly authorize local commits, push or merge.

Approval readiness: PASS — checked D4, D6, R2 follow-up A, the existing pause/defer regression contract and selected execution constraints. No accepted remedy lacks an actual answer or exact prior approval. Engineering plan and delivery-gate clearance do not replace implementation tests, staging or acceptance.

## Completion Summary

- Step 0: Scope Challenge — scope accepted as-is (FULL_REVIEW; F1/F2 corrective re-review plus the explicitly answered D6 delivery-gate update).
- Reality rows: 14 rows emitted; scoped checker passed 12/12 applicable rows (`PLAN_REALITY: phase=eng rows=12/12 missing=none verdict=complete`), 0 unverified premises.
- Architecture Review: 0 issues found; F1 verified closed.
- Code Quality Review: 0 issues found.
- Test Review: diagram produced, 0 gaps identified; F2 verified closed with unavailable obligations preserved.
- Performance Review: 0 issues found.
- NOT in scope: written.
- What already exists: written.
- TODOS.md updates: 0 items proposed to user.
- Failure modes: 0 critical gaps flagged; implementation evidence is still absent.
- Unresolved decisions: 0 in this review.
- Outside voice: Codex unavailable (custom endpoint has no frontier plan-review model); native Plan fallback incomplete within five minutes; outside coverage unavailable.
- Parallelization: 1 selected SDD lane, 0 parallel / 12 sequential delivery tasks.
- Lake Score: N/A (no new completeness-scored decision in this re-review).

## Suppressed findings

None. No completed outside report was available, the late fallback result was excluded by the deadline rule, and no existing deferred item was silently discarded.

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 1 | CLEAR | mode: HOLD_SCOPE, 0 critical gaps |
| Outside Review | Codex `/plan-eng-review` outside voice | Independent 2nd opinion | 9 | UNAVAILABLE | No completed external reviewer; current model preflight failed and bounded native fallback timed out |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 5 | CLEAR | 0 issues, 0 critical gaps; F1/F2 verified and D6 delivery gate answered |
| Design Review | `/plan-design-review` | UI/UX gaps | 1 | ISSUES OPEN | score: 4/10 → 8/10, 21 decisions; 2 prior deferred items |
| DX Review | `/plan-devex-review` | Developer experience gaps | 0 | — | Not applicable to this focused R2 re-review |

- **OUTSIDE COVERAGE:** Codex, plan-review phase, unavailable; current custom endpoint has no frontier plan-review model and the bounded native fallback produced no timely complete report. No outside findings or clean-review credit.
- **VERDICT:** CEO + ENG CLEARED — F1/F2 engineering plan gate passed and D6 selected implementation into the existing PR #101 on the current branch. Implementation tests, staging and acceptance remain required; local commit, push and merge remain separately gated.
**UNRESOLVED DECISIONS:**
- + 2 unresolved from prior reviews (design-deferred Runtime font and WebVTT captions; nonblocking for R2)
