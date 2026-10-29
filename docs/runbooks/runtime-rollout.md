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
   device list, and the existing playback descriptor route.
5. Do not enable any Runtime flag until the migration, foreign-key check, and
   legacy smoke are recorded.

## 2. Enable Tasks for one staging workspace

1. Keep Media and Workbench false. Enable `runtime_tasks_enabled` for a single
   staging workspace only.
2. Create one Task twice with the same idempotency key and confirm the same
   Task ID and no second creation.
3. Drive one long-running Job to a terminal state and confirm the Job is
   terminal (`done`/`failed`/`rejected`/`cancelled`), not paused and not a
   percentage.
4. Create and hit a watch, then cancel the Task while a Job runs. Confirm the
   cancel reaches the recorded terminal state.
5. Restart the process. Confirm the Task, current Run, Job, watch, and outbox
   facts survive; recovery must not create a duplicate Run or Job.
6. Confirm the Runtime SSE endpoint connects and a reconnect resumes from the
   last cursor without duplicating events.

## 3. Approval gates

1. Create a pending approval and double-click approve. Confirm one durable
   decision and one original outbox row, not two.
2. Replay the same approve request. Confirm the replay is idempotent and the
   frozen arguments shown in the UI match the stored execution arguments.
3. Restart after approval but before execution. Confirm one Job at most and the
   original `tool_call_id`.
4. Reject, expire, and cancel before a decision. Confirm each creates no Job.
5. Pause the Task, then approve, reject, and expire one request of each kind.
   Confirm the original decision outbox stays pending until resume, and resume
   acknowledges that same event without creating a replacement event.
6. If a physical result is unknown, record an administrator disposition for the
   Task/Job. Never retry the physical side effect automatically, and never clear
   `unresolved_unknown` without a disposition.

## 4. Enable Media

1. Enable `runtime_media_enabled` for the same staging workspace.
2. Run the mock vision capability check and, only with an already authorized
   staging provider, the optional live-provider check. Non-vision providers
   must return `image_unavailable` explicitly.
3. Confirm evidence expiry hides stale media and that trace/ledger/log output
   contains only evidence references, never bytes or signed query strings.
4. Confirm the playback descriptor is short-lived, protocol-limited, and
   CORS-valid from the Workbench origin.
5. Play and stop evidence. Confirm the player is destroyed, no background
   video requests continue, and switching Task leaves zero active players.

## 5. Enable Workbench and observe

1. Enable `runtime_workbench_enabled`.
2. Run keyboard-only navigation, history, seam resize, SSE resync, and
   responsive checks at desktop, 768px, 390px, and 360px.
3. Confirm an approval conflict renders the current projection and never the
   stale client decision.
4. Keep all three flags on for one hour and watch the dashboards and alerts in
   the section below.

## 6. Rollback

1. Set only `runtime_tasks_enabled` false. Keep Media and Workbench at their
   current values; existing Task rows are not mutated by the flag transition.
2. Freeze the nonterminal Task ID batch from
   `GET /api/v1/admin/runtime/drain?workspaceId=<workspace>`.
3. Send that explicit batch to
   `POST /api/v1/admin/runtime/drain/cancel`. The endpoint cancels each ID
   through the existing per-Task Cancel path. It never performs a bulk implicit
   status update and never discovers IDs on its own.
4. Cancel marks pending approvals unavailable and resolves undispatched
   decisions before drain can complete.

## 7. Unknown results

1. Record every `cancel_unknown` Task/Job/device ID surfaced by drain or by
   `cancelled_unknown` and `interrupted` Job gauges.
2. Treat an `interrupted` Job as execution-terminal but physically unknown: it
   counts in `unresolved_unknown` and blocks drain until dispositioned.
3. Inspect the physical device state manually.
4. Enter an administrator disposition at
   `POST /api/v1/admin/runtime/unknown/{job_id}/disposition` for either a
   `cancelled_unknown` or an `interrupted` Job.
5. Only then confirm `unresolved_unknown` is zero. Never auto-retry.

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
tables and never use a flag as an emergency stop.

## 9. Recovery

Re-enable in order Task -> Media -> Workbench, and only after the previously
failed smoke gate passes. Do not skip a gate because the 15-minute estimate has
elapsed.

## Five-minute checks

- Migration chain completed and `PRAGMA foreign_key_check` returned no rows.
- Old APIs (without Runtime flags) still respond.
- Same idempotency key returns the same Task ID.
- A real Job reaches a terminal state.
- No paused Job and no percentage progress is displayed or stored.
- Runtime SSE connects and streams a first frame.

## One-hour checks

- Local watch -> Run and Job -> UI timing targets (<2s and <1s) are recorded as
  local integration evidence only; they are not production p99.
- Dead outbox = 0 and every `cancelled_unknown` or `interrupted` Job has a
  disposition.
- Vision capability failure is honest (`image_unavailable`), and unavailable
  media never falls back to Hub proxying.
- No video request continues after Task switch, player stop, or history.
- Dashboard and alert queries below are green.

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
- `stream.{activeConnections,connectionsOpened,connectionsClosed,resyncs,duplicatesDropped,foreignTaskEvents,firstFrames,firstFrameMsTotal}`
  (Runtime SSE counters; `foreignTaskEvents` counts frames filtered for another
  Task and is separate from `duplicatesDropped`)

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

Alerts are observability thresholds only; they are not new quotas:

- `outbox.dead > 0`
- `outbox.oldestPendingAgeSecs > 60`
- pending outbox growth over a 5-minute window
- an approved decision with no Job and no recorded deferral for >60s
- `jobs.cancelledUnknown > 0`, `jobs.interrupted > 0`, or any unresolved
  unknown disposition
- any Job execution event after its recorded stop
- a Job still running past its deadline + 60s
- `host.diskUsedPercent > 80`
- `host.rssPercent > 75`
- growth in `runtime_evidence_failure` / `runtime_playback_error`
- an SSE reconnect storm, duplicate rate above baseline, or resync spike
- stale player connections (active players after Task switch/stop, or repeated
  `runtime_playback_destroy_failure`)

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
