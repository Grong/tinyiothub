# Runtime rollout and drain runbook

Scope: durable Runtime Tasks, device Jobs, approvals, media/evidence, and the
Workbench for one workspace at a time. All three flags are default off:

- `runtime_tasks_enabled`
- `runtime_media_enabled`
- `runtime_workbench_enabled`

Rollout is ordered. A later flag is never enabled while an earlier gate is
failing, and the estimated 15-minute window never overrides drain completion.
The flags are not an emergency stop for durable rows: turning a flag off does
not delete Tasks, approvals, outbox rows, Jobs, or evidence.

**Binding rollout decision (2026-10-27): Media and Workbench are BLOCKED.**
Only `runtime_tasks_enabled` may be enabled for a staging workspace. Do not
enable `runtime_media_enabled` or `runtime_workbench_enabled` in this round:
no upstream playback provider exists, so production playback always returns
`503 playback_unavailable`. Sections 4 and 5 describe the intended gates, not
steps that can currently pass. A test fake used by the API tests never
authorizes the production media or Workbench gate.

Every administrative call below requires an admin token for the workspace.
Never paste tokens, signed media URLs, private objective text, image bytes, or
provider keys into tickets or dashboards.

## 1. Baseline and migration

1. Confirm the declaration baseline is commit `6876707f` on the current PR
   #101 branch, plus only additive Runtime migrations.
2. Back up the SQLite database file, then run the full migration chain against a
   file-backed database (not in-memory).
3. Run `PRAGMA foreign_key_check;` and confirm zero rows.
4. Smoke the legacy APIs that do not use the new flags: task list/read,
   device list, and the existing playback descriptor route. With all flags off
   the playback descriptor route must answer `503 playback_unavailable`; that
   fail-closed response is the expected smoke result, not a failure.
5. Do not enable any Runtime flag until the migration, foreign-key check, and
   legacy smoke are recorded.

## 2. Enable Tasks for one staging workspace

1. Keep Media and Workbench false. Enable `runtime_tasks_enabled` for a single
   staging workspace only.
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
8. With Tasks enabled but Media disabled (the required state), call
   `read_device` on an image source. Confirm the tool result is
   `image_unavailable` with reason `media_disabled`, no evidence row is
   created, and no image bytes or attachment reach the model. A missing,
   malformed, or unreadable heartbeat config must produce the same fail-closed
   result.

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

## 4. Media — BLOCKED

**Do not enable `runtime_media_enabled`.** This gate is blocked, not merely
pending evidence. The production composition wires `UnavailablePlaybackProvider`
because no upstream playback provider has been selected or authorized, so the
playback descriptor route always returns `503 playback_unavailable` even when
the media flag is on and the device/source declaration is valid. The in-test
`FakePlayback` provider exists only inside the media API tests; it never
authorizes production and must never be cited as playback evidence.

What exists today and can be verified only in an isolated non-production
environment (it does not unblock the gate):

1. `runtime_media_enabled` gates both evidence access and `read_device` image
   capture. With the flag off (default, or missing/malformed/unreadable config)
   image reads fail closed with `image_unavailable`/`media_disabled` and create
   no evidence row, bytes, or model attachment.
2. Non-vision providers must return `image_unavailable` explicitly.
3. Evidence expiry hides stale media; trace/ledger/log output must contain only
   evidence references, never bytes, storage paths, or signed query strings.
4. The production playback route is expected to fail closed; a success response
   from the production composition is a defect, not a passing gate.

Because step 4 can never pass in this round, the remaining Media and player
checks cannot be recorded as executed, and the gate stays off.

## 5. Workbench — BLOCKED

**Do not enable `runtime_workbench_enabled`.** The Workbench depends on the
blocked Media gate and is out of scope for this round. The keyboard, responsive,
approval-conflict, player-lifecycle, and one-hour observation checks below are
the intended future gate; none may be claimed as passed because the Workbench
and Media flags remain off.

1. Do not enable `runtime_workbench_enabled`.
2. Record the gate as blocked with the Media gate; keep all three flags off in
   production.
3. Reopen this section only after a real upstream playback provider is
   authorized and the Media gate passes with real staging evidence.

## 6. Rollback

1. Set only `runtime_tasks_enabled` false. Leave Media and Workbench BLOCKED
   and off; existing Task rows are not mutated by the flag transition.
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
`canHideWorkbench`/`canHideMedia`. Only after the counts are zero, disable
Workbench first and then Media. Keep additive rows and evidence; never drop
tables and never use a flag as an emergency stop. In this round Media and
Workbench were never enabled, so the expected rollback is Tasks-only and those
two counts gate the already-blocked flags.

## 9. Recovery

Re-enable only `runtime_tasks_enabled` for the staging workspace, and only after
the previously failed Task smoke gate passes. Do not skip a gate because the
15-minute estimate has elapsed. Media and Workbench remain BLOCKED and off;
their recovery order (Media before Workbench) is future work that requires an
authorized upstream playback provider.

## Five-minute checks

- Migration chain completed and `PRAGMA foreign_key_check` returned no rows.
- Old APIs (without Runtime flags) still respond.
- Same idempotency key returns the same Task ID.
- A real Job reaches a terminal state.
- No paused Job and no percentage progress is displayed or stored.
- Runtime SSE connects and streams a first frame.

## One-hour checks

These apply only to the Tasks gate. Media and Workbench are BLOCKED and must
remain off, so no player/media observation can be recorded in this round.

- Local watch -> Run and Job -> UI timing targets (<2s and <1s) are recorded as
  local integration evidence only; they are not production p99.
- Dead outbox = 0 and every `cancelled_unknown` or `interrupted` Job has a
  disposition.
- Vision capability failure is honest (`image_unavailable`), and unavailable
  media never falls back to Hub proxying.
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
- `stream.{activeConnections,connectionsOpened,connectionsClosed,resyncs,duplicatesDropped,foreignTaskEvents,taskLessEvents,firstFrames,firstFrameMsTotal}`
  (Runtime SSE counters; `foreignTaskEvents` counts frames filtered for another
  Task, `taskLessEvents` counts workspace-scoped frames with no Task, and both
  are separate from `duplicatesDropped`. Per-connection dedupe memory is
  bounded; the oldest IDs are evicted after 1024 retained events.)

Structured metrics:

- `runtime_task_created`, `runtime_task_transition`, `runtime_run_started`,
  `runtime_run_final`
- `runtime_approval_requested`, `runtime_approval_decided`,
  `runtime_approval_conflict`, `runtime_approval_expired`,
  `runtime_approval_unavailable`
- `runtime_outbox_claimed`, `runtime_outbox_marked`, `runtime_outbox_retried`
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
  and the Media/Workbench gates are blocked.
- `[gauge]` an SSE reconnect storm, duplicate rate above baseline, or resync
  spike; the counters exist (`connectionsOpened`, `duplicatesDropped`,
  `resyncs`) but thresholds and rate windows are not automated.
- `[NOT IMPLEMENTED]` stale player connections (active players after Task
  switch/stop, or repeated `runtime_playback_destroy_failure`); no server-side
  player registry exists and the Media gate is blocked.

None of the `[NOT IMPLEMENTED]` items may be cited as monitoring coverage in a
rollout sign-off.

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
- Media and Workbench were never enabled or exercised end-to-end in production.
  Playback is proven only to fail closed against the production
  `UnavailablePlaybackProvider`; no upstream provider exists. The API-test
  `FakePlayback` provider is a test double and authorizes nothing.
- The timeline endpoint, media fail-closed tool path, drain categories, cancel
  outcomes, and startup `run.interrupted` replay are covered by local
  file-backed SQLite/HTTP tests. Those are not staging, Linux CI, hardware,
  live-provider, or production latency evidence.
- No Linux CI, staging, production, live-provider, hardware, all-camera, or
  production p99 result is claimed.
