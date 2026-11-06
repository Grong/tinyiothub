# Runtime rollout and drain runbook

Scope: durable Runtime Tasks, device Jobs, approvals, media/evidence, and the
Workbench for one workspace at a time.

Runtime has no rollout flags. The 2026-10-30 decision removed the
`runtime_tasks_enabled` / `runtime_media_enabled` / `runtime_workbench_enabled`
keys from `workspaces.heartbeat_config`: Task creation is always persistent,
`read_device` image capture is always available to Tasks, and the Workbench is
always reachable. Old `runtime_*` keys left in a workspace row are inert and are
dropped the next time the heartbeat config is saved.

There is therefore no per-workspace kill switch for durable rows. Draining is an
operational procedure (section 6), not a flag transition, and it never deletes
Tasks, approvals, outbox rows, Jobs, or evidence.

**Binding rollout decision (2026-10-27): production playback is BLOCKED.** No
upstream playback provider exists, so production playback always returns
`503 playback_unavailable` even though media image capture is always on.
Sections 4 and 5 describe checks that cannot pass in this round. A test fake used
by the API tests never authorizes production playback.

Every administrative call below requires an admin token for the workspace.
Never paste tokens, signed media URLs, private objective text, image bytes, or
provider keys into tickets or dashboards.

## 1. Baseline and migration

1. Confirm the declaration baseline is commit `6876707f` on the current PR
   #101 branch, plus only additive Runtime migrations.
2. Back up the SQLite database file, then run the full migration chain against a
   file-backed database (not in-memory).
3. Run `PRAGMA foreign_key_check;` and confirm zero rows.
4. Smoke the existing APIs: task list/read, device list, and the playback
   descriptor route. The playback descriptor route must answer
   `503 playback_unavailable` because the production composition wires
   `UnavailablePlaybackProvider`; that fail-closed response is the expected
   smoke result, not a failure.
5. Do not start the staging smoke until the migration, foreign-key check, and
   legacy smoke are recorded.

## 2. Verify Tasks for one staging workspace

1. Pick a single staging workspace; Runtime is already active for every
   workspace.
2. Create one Task twice with the same idempotency key and confirm the same
   Task ID and no second creation.
3. Drive one long-running Job to a terminal state and confirm the Job is
   terminal (`done`/`failed`/`rejected`/`cancelled`), not paused and not a
   percentage. If the Run ends as failed while its Job is still running, the
   Task must move to `waiting_job` (not `failed`); the terminal-Job wake then
   starts exactly one continuation Run. A Task with no Job or watch left still
   fails.
4. Create and hit a watch, then cancel the Task while a Job runs. Confirm the
   cancel reaches the recorded terminal state.
5. Restart the process. Confirm the Task, current Run, Job, watch, and outbox
   facts survive; recovery must not create a duplicate Run or Job.
6. Confirm the Runtime SSE endpoint connects and a reconnect resumes from the
   last cursor without duplicating events.
7. Confirm the bounded timeline endpoint seeds the initial page before SSE:
   `GET /api/v1/workspaces/{workspace_id}/agent/tasks/{task_id}/events?limit={1..200}&before={cursor?}`.
   The response is `{ events, oldestCursor, hasMore }`. Events are ascending by
   cursor and use the same serialized shape as SSE `event` frames. `limit`
   defaults to 100 and is capped at 200; `before` is an exclusive, id-keyset
   cursor for loading older pages, and the page reads `limit + 1` rows only to
   set `hasMore`. The query is served by the non-partial
   `(workspace_id, kind, json_extract(payload,'$.taskId'), id)` index, so a page
   never scans the workspace or Task's full ledger. The endpoint checks Task
   existence directly (it does not build the Task projection), is workspace-
   and Task-scoped, returns `404` for an unknown Task and `403` across tenants.
   An empty Task returns `events: []`, `oldestCursor: null`, `hasMore: false`.
8. Call `read_device` on an image source. Confirm the image is persisted as
   evidence and returned as a single model attachment, and that the tool result
   contains no image bytes inline. Selecting a video source must fail with
   `video_is_for_human_not_model`; selecting more than one image must fail with
   `at most one image`. Non-vision providers must return `image_unavailable`
   explicitly rather than silently dropping the attachment.

## 3. Approval gates

1. Create a pending approval and double-click approve. Confirm one durable
   decision and one original outbox row, not two.
2. Replay the same approve request. Confirm the replay is idempotent and the
   frozen arguments shown in the UI match the stored execution arguments.
3. Restart after approval but before execution. Confirm one Job at most and the
   original `tool_call_id`.
   Before submitting, a recovery Run reuses an accepted, running, interrupted,
   or `cancelled_unknown` Job for the same Task/device/tool/arguments and
   returns its ID with `deduplicated: true`. `completed`, `failed`, and
   `rejected` Jobs do not block a deliberate repeat action. The reuse covers
   Run recovery only: it is not a distributed exactly-once guarantee. Two
   concurrent Runs that both pass the pre-submit scan can still submit two
   Jobs; the device worker must keep command execution idempotent.
4. Reject, expire, and cancel before a decision. Confirm each creates no Job.
5. Pause the Task, then approve, reject, and expire one request of each kind.
   Confirm the original decision outbox stays pending until resume, and resume
   acknowledges that same event without creating a replacement event. A paused
   Task is expected to hold a pending decision wake, so the outbox age alert is
   evaluated only while no Task is paused (see Alerts).
6. If a physical result is unknown, record an administrator disposition for the
   Task/Job. Never retry the physical side effect automatically, and never clear
   `unresolved_unknown` without a disposition.

## 4. Playback — BLOCKED

**Playback remains blocked.** The production composition wires
`UnavailablePlaybackProvider` because no upstream playback provider has been
selected or authorized, so the playback descriptor route always returns
`503 playback_unavailable` for a declared device/source. The in-test
`FakePlayback` provider exists only inside the media API tests; it never
authorizes production and must never be cited as playback evidence. Image
evidence and `read_device` image capture no longer depend on any flag.

What exists today and can be verified only in an isolated non-production
environment (it does not unblock playback):

1. `read_device` image capture is always on: the image is persisted as evidence
   and attached once, with `image_unavailable` when capture or storage fails.
2. Non-vision providers must return `image_unavailable` explicitly.
3. Evidence expiry hides stale media; trace/ledger/log output must contain only
   evidence references, never bytes, storage paths, or signed query strings.
4. The production playback route is expected to fail closed; a success response
   from the production composition is a defect, not a passing gate.

Because step 4 can never pass in this round, the remaining player checks cannot
be recorded as executed and playback stays blocked.

## 5. Workbench — always on, player checks BLOCKED

The Workbench hash routes (`#/runtime`, `#/runtime/new`,
`#/runtime/tasks/{id}`) and the sidebar entry are always registered; there is no
flag. Keyboard, responsive, approval-conflict, and Task-switch checks can be
recorded against staging. Player-lifecycle and one-hour observation checks stay
BLOCKED with section 4 because no real playback provider exists.

1. Do not claim player-lifecycle or media observation evidence in production.
2. Record the player gate as blocked with section 4.
3. Reopen this section only after a real upstream playback provider is
   authorized and the playback checks pass with real staging evidence.

## 6. Rollback

1. There is no flag to turn off. Record the incident time and workspace; Task
   creation stays available, so stop new work operationally (announce the
   drain, revoke agent credentials if the incident requires it) before
   continuing.
2. Freeze the nonterminal Task ID batch from
   `GET /api/v1/admin/runtime/drain?workspaceId=<workspace>`.
3. Send that explicit batch to
   `POST /api/v1/admin/runtime/drain/cancel`. The endpoint cancels each ID
   through the existing per-Task Cancel path. It never performs a bulk implicit
   status update and never discovers IDs on its own.
4. Read each per-Task `outcome` in `results`; do not treat the batch as
   all-or-nothing:
   - `cancelled` — this call performed the cancel.
   - `already_terminal` — the Task was already completed/failed/cancelled; the
     cancel was accepted but changed nothing.
   - `in_progress` — the cancel was not accepted and the Task is still
     nonterminal (for example a run is still winding down); re-check drain and
     retry explicitly later rather than assuming success.
   - `not_found` — the ID does not exist in this workspace.
   - `failed` — the per-Task Cancel returned an error, or the post-cancel Task
     lookup itself failed (the outcome is unknown, not proof of progress);
     inspect logs and the Task projection before retrying.
   `cancelled` counts only outcomes that actually performed the cancel. The
   response also recomputes `drain` after the batch.
5. Cancel marks pending approvals unavailable and resolves undispatched
   decisions before drain can complete.

## 7. Unknown results and interrupted Runs

1. Record every `cancel_unknown` Task/Job/device ID surfaced by drain or by
   `cancelled_unknown` and `interrupted` Job gauges.
2. Treat an `interrupted` Job as execution-terminal but physically unknown: it
   counts in `unresolved_unknown` and blocks drain until dispositioned.
3. Inspect the physical device state manually.
4. Enter an administrator disposition at
   `POST /api/v1/admin/runtime/unknown/{job_id}/disposition` for either a
   `cancelled_unknown` or an `interrupted` Job.
5. Only then confirm `unresolved_unknown` is zero. Never auto-retry.
6. On process startup, orphaned `agent_runs` rows still marked `running` are
   reconciled to `interrupted`, and a persisted `run.interrupted` event is
   committed in the same transaction (event ID `run:{run_id}:interrupted`).
   Startup recovery runs before the HTTP listener binds, so no live SSE
   subscriber can exist at that instant; a client that reconnects replays the
   persisted event through its cursor, with the original Task scoping. Do not
   rewrite the persisted event during recovery, and do not expect a live push
   during startup.
7. `run.interrupted` is a Run fact only. It does not disposition an
   `interrupted` Job; use the administrator disposition in step 4 for the
   physical outcome.

## 8. Drain completion

Require every `DrainStatus` category to be zero, including pending approvals and
the decision outbox:

- `nonterminal_tasks`
- `active_runs`
- `active_jobs`
- `active_watches`
- `pending_approvals`
- `pending_outbox`
- `unresolved_unknown`

`GET /api/v1/admin/runtime/drain` returns these counts with
`canHideWorkbench`/`canHideMedia`. Only after the counts are zero may the
operator close out the drain. Keep additive rows and evidence; never drop
tables. There is no flag transition in this procedure, and `canHideMedia` can
never become true while player checks are blocked by section 4.

## 9. Recovery

Resume the staging workspace only after the previously failed Task smoke gate
passes. Do not skip a gate because the 15-minute estimate has elapsed. Playback
and player-lifecycle recovery remains future work that requires an authorized
upstream playback provider.

## Five-minute checks

- Migration chain completed and `PRAGMA foreign_key_check` returned no rows.
- Existing APIs still respond.
- Same idempotency key returns the same Task ID.
- A real Job reaches a terminal state.
- No paused Job and no percentage progress is displayed or stored.
- Runtime SSE connects and streams a first frame.

## One-hour checks

These apply only to Tasks and image evidence. Playback is BLOCKED, so no
player observation can be recorded in this round.

- Local watch -> Run and Job -> UI timing targets (<2s and <1s) are recorded as
  local integration evidence only; they are not production p99.
- Dead outbox = 0 and every `cancelled_unknown` or `interrupted` Job has a
  disposition.
- Vision capability failure is honest (`image_unavailable`), and unavailable
  playback never falls back to Hub proxying.
- The bounded timeline page and SSE reconnect agree on event identity/cursor.
- Dashboard and alert queries below are green; `[NOT IMPLEMENTED]` alerts are
  not claimed as monitored.

## Metrics and dashboard queries

Metrics are structured tracing events plus the durable gauges returned by
`GET /api/v1/admin/runtime/metrics?workspaceId=<workspace>`.

Durable gauges (from the metrics response):

- `drain.*` — the same seven DrainStatus categories.
- `tasks.{active,waitingJob,waitingWatch,needsApproval,paused,cancelling,completed,failed,cancelled}`
- `approvals.{pending,approved,rejected,expired,unavailable,oldestPendingAgeSecs}`
- `outbox.{pending,dispatched,dead,oldestPendingAgeSecs}`
- `jobs.{accepted,running,done,failed,rejected,cancelled,cancelledUnknown,interrupted}`
- `watches.{active,total}`
- `evidence.{count,bytes,expired}`
- `host.{rssBytes,rssPercent,diskUsedPercent}` and `dbQueryMs`
- `stream.{activeConnections,connectionsOpened,connectionsClosed,resyncs,duplicatesDropped,firstFrames,firstFrameMsTotal}`
  (Harness Task SSE counters; the legacy `foreignTaskEvents` /
  `taskLessEvents` counters were retired with the platform Task stream. The
  counters describe the workspace `/agent/tasks/{id}/stream` route.
  Per-connection dedupe memory is bounded; the oldest IDs are evicted after
  1024 retained events.)

Structured metrics:

- `runtime_task_created`, `runtime_task_transition`, `runtime_run_started`,
  `runtime_run_final`
- `runtime_approval_requested`, `runtime_approval_decided`,
  `runtime_approval_conflict`, `runtime_approval_expired`,
  `runtime_approval_unavailable`
- `runtime_outbox_claimed`, `runtime_outbox_marked`, `runtime_outbox_retried`
- `runtime_task_finalize_intents` (retained-intent gauge; fields `count`,
  `memory_count` for process-only staging, `pruned`, `staging_dropped`) and
  `runtime_task_finalize_staging_dropped` (in-memory intents dropped by the
  periodic sweep when their wake was resolved out-of-band)
- `runtime_job_accepted`, `runtime_job_final`, `runtime_job_duration`
- `runtime_watch_hit`, `runtime_watch_timeout`, `runtime_watch_expired`,
  `watch_ledger_write_failed`
- `runtime_evidence_stored`, `runtime_evidence_failure`,
  `runtime_evidence_expired`
- `runtime_sse_connected`, `runtime_sse_disconnected`,
  `runtime_sse_first_frame`, `runtime_sse_resync`
- `runtime_drain_cancel_batch`, `runtime_unknown_disposition`,
  `runtime_host_metrics`
- Browser console metrics (only where telemetry is collected):
  `runtime_playback_session`, `runtime_playback_error`,
  `runtime_playback_destroy_failure`

Alerts are observability thresholds only; they are not new quotas. Labels state
what exists today: `[gauge]` can be queried from the metrics response,
`[log-derived]` needs log/metric aggregation that is not wired to an automated
alert here, and `[NOT IMPLEMENTED]` has no executable signal yet and must not be
claimed as covered.

- `[gauge]` `outbox.dead > 0`
- `[gauge]` `runtime_task_finalize_intents.count > 0` — a finalize intent means
  a completed Run is waiting for teardown to finish. The 30-second sweep retains
  intents for 7 days and only prunes a row when its wake is no longer live
  **and** its Task is terminal or missing (`retry_dead_wake` can resurrect a
  dead wake, and for `report_persisted=false` the intent is the only copy of the
  model output). Operator SLA: investigate within 7 days, before the retention
  cutoff; `memory_count > 0` means the durable intent write is failing and a
  process restart would lose the retained report.
  A partial store outage can produce repeated staging warnings; the
  `memory_count`/`staging_dropped` fields and rate-limited alerting are the only
  signal (in-memory retry currently has no backoff).
  A Paused or otherwise blocked Task that owns a live finalize intent keeps its
  Run `running` until the intent settles; a long-paused Task can therefore leave
  a `running` row in `agent_runs` indefinitely, and that is expected.
- `[gauge]` `outbox.oldestPendingAgeSecs > 60` **only while
  `tasks.paused == 0`**. Paused Tasks deliberately keep decision wakes pending
  until resume (section 3 step 5), so a paused Task must not page continuously.
  Evaluate growth of pending outbox only in windows where no Task is paused.
- `[NOT IMPLEMENTED]` pending outbox growth over a 5-minute window; derive it
  from repeated gauge scrapes before treating it as an alert.
- `[NOT IMPLEMENTED]` an approved decision with no Job and no recorded deferral
  for >60s; there is no deferral fact or dedicated gauge.
- `[gauge]` `jobs.cancelledUnknown > 0`, `jobs.interrupted > 0`, or any
  unresolved unknown disposition (`drain.unresolved_unknown > 0`)
- `[NOT IMPLEMENTED]` any Job execution event after its recorded stop; this
  requires correlating `runtime_job_*` events with `stopRequestedAt`.
- `[NOT IMPLEMENTED]` a Job still running past its deadline + 60s; no deadline
  age gauge is exposed.
- `[gauge]` `host.diskUsedPercent > 80`
- `[gauge]` `host.rssPercent > 75`
- `[log-derived]` growth in `runtime_evidence_failure`; the structured event is
  emitted, but no automated counter/threshold is wired.
- `[log-derived]` growth in `runtime_playback_error`; browser telemetry only,
  and playback is blocked.
- `[gauge]` an SSE reconnect storm, duplicate rate above baseline, or resync
  spike; the counters exist (`connectionsOpened`, `duplicatesDropped`,
  `resyncs`) but thresholds and rate windows are not automated.
- `[NOT IMPLEMENTED]` stale player connections (active players after Task
  switch/stop, or repeated `runtime_playback_destroy_failure`); no server-side
  player registry exists and playback is blocked.

None of the `[NOT IMPLEMENTED]` items may be cited as monitoring coverage in a
rollout sign-off.

## Performance evidence

Harness storage, replay, recovery, backup and Task/stream/Job-path measurements
are reproducible with:

```bash
cargo test -p tinyiothub-cloud --bench harness -- --nocapture
```

See [harness-performance.md](harness-performance.md) for the methodology, raw
samples and limits. The run records machine, SQLite/WAL, binary and config
versions, data sizes, concurrency, sample distribution and local p50/p95; the
size buckets are inputs, not quotas or an SLA. It is a debug-profile local
measurement, not production p99.

The current `main` ref does not contain the durable Harness workspace store,
session-event replay, Runtime Task or Device Job storage modules. The runner
therefore records the main/branch comparison as missing with the exact absent
paths instead of producing a fabricated baseline.

## 24h soak — future gate, not executed

The 24h Harness soak and fault-drill method is documented in
[harness-soak.md](harness-soak.md). It uses simulated devices and a controlled
provider, records raw evidence, and defines the failure criteria for duplicate
physical actions, silent loss, wrong terminal state, and state that cannot be
rebuilt from durable facts.

**The soak is not executed in this change.** No 24h process was started and no
soak result is claimed. It is a future gate, not a merge or production gate
this period. Simulation and controlled-provider results never substitute for
production evidence.

## Evidence limits

- The browser suite runs in Playwright Chromium. That build ships without
  H.264/AAC decoding, so the committed local HLS fixture is a real VP9/Opus
  fragmented-MP4 HLS stream served by the fixture server. H.264/AAC HLS on
  physical cameras is not proven by the browser suite.
- Two-fixture, mock-provider, and local-browser results are separate from real
  hardware, live-provider, production p99, and all-camera compatibility. None of
  those broader claims follows from this runbook.
- Real staging smoke and the 15-minute production window must be recorded
  separately. This runbook does not claim them as executed.
- The 24h soak is a documented method only; it was not executed. No
  simulated-device or controlled-provider result is staging, hardware, or
  production evidence.
- Playback was never exercised end-to-end in production. It is proven only to
  fail closed against the production `UnavailablePlaybackProvider`; no upstream
  provider exists. The API-test `FakePlayback` provider is a test double and
  authorizes nothing.
- Runtime Tasks and `read_device` image evidence are always on; their rollout
  evidence is limited to the checks recorded above and does not imply staging,
  hardware, or production verification.
- The timeline endpoint, media fail-closed tool path, drain categories, cancel
  outcomes, and startup `run.interrupted` replay are covered by local
  file-backed SQLite/HTTP tests. Those are not staging, Linux CI, hardware,
  live-provider, or production latency evidence.
- No Linux CI, staging, production, live-provider, hardware, all-camera, or
  production p99 result is claimed.
