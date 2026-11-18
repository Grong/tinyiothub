# Assistant / Harness Task recovery runbook

Scope: one workspace Harness store (`data/agents/<workspace_id>/harness/harness.db`).
This runbook covers Assistant schedule health, Task/Run/Session recovery,
startup crash recovery, unknown-Effect reconciliation, and per-row dead-wake
retry.

It does not cover provider/model outages, device-driver faults, e-stop handling,
or the 24h soak procedure. `next_wake_at` is a plan time; it is never a promise
of on-time execution. A chat or Assistant wake is **not** process liveness.

## 1. Invariants

- `assistant_schedules` is the single source for Assistant scheduling:
  `enabled`, `interval_seconds`, `permission_mode`, `prompt`, `next_wake_at`,
  `last_wake_at`, `last_outcome`.
- `TriggerRouter` ticks every 30 seconds: it ensures a schedule row exists for
  every workspace, scans due enabled rows, and dispatches at most one in-flight
  wake per workspace. A skipped dispatch still advances `next_wake_at`.
- Event wakes (device events, watch hits, alarms) enter the same router and are
  deduplicated per `{workspace}:{trigger}:{dedup_key}` in a 60-second window
  (at most 3 dispatches). A workspace with a wake in flight rejects new event
  wakes until it settles.
- A dispatch records `last_wake_at`, `last_outcome`, and the next
  `next_wake_at`. Do not hand-edit these columns.
- Dead-wake retry reuses the original `event_id`. It never creates a
  replacement outbox row and never retries a batch.
- The retained Supervisor API (`/harness/supervisor`) still exposes
  `supervisor_state` and Workflow cron-derived fields for pre-cutover stores.
  The Workflow scheduler that advanced those fields is retired; current
  deployments advance `assistant_schedules`, not `supervisor_state`.

## 2. Read the panel and API

The operator view is the Assistant view `#/assistant`. Its HUD calls
`GET /api/v1/workspaces/{workspace_id}/agent/hud` and displays the schedule
read-only (a nested summary). Schedule changes are made through
`PUT /api/v1/workspaces/{workspace_id}/agent/schedule` (API/ops); that endpoint
returns the full schedule DTO, while the HUD returns the read-only summary.

The retained Supervisor API is:

```text
GET  /api/v1/workspaces/{workspace_id}/harness/supervisor
POST /api/v1/workspaces/{workspace_id}/harness/supervisor/dead-wakes/{task_id}/{event_id}/retry
```

The GET is workspace-member readable and returns:

```json
{
  "health": {
    "status": "idle | healthy | stalled",
    "stalledReason": "tick_deadline_exceeded | due_not_started | clock_rollback | null",
    "tickSeq": 0,
    "lastStartedAt": null,
    "lastCompletedAt": null,
    "nextWakeAt": null,
    "oldestPendingAt": null,
    "schedulingDelaySeconds": 0,
    "activeMains": 0,
    "activeSubagents": 0,
    "enabledWorkflows": 0,
    "pendingWakes": 0,
    "lastError": null,
    "noNewRuns": true,
    "observedAt": "..."
  },
  "deadWakes": [],
  "canRetry": false,
  "readOnly": true
}
```

`enabledWorkflows` and the Workflow cron-derived `nextWakeAt` source are
retained legacy fields from the retired Workflow experiment. They are zero/null
on current deployments. Read `assistant_schedules.next_wake_at` (or the
`/agent/schedule` response) for the current Assistant schedule.

`idle` with `noNewRuns=true` is a normal state, not a failure. Only `stalled`
means a concrete scheduler problem. `nextWakeAt` is shown as a planned time,
not an on-time guarantee.

## 3. Classify the state

| Observation | Meaning | Action |
|---|---|---|
| `assistant_schedules.enabled=false` (HUD `paused`) | Assistant deliberately paused; no scheduled wake is due | None |
| `schedule.enabled=true`, `next_wake_at` within the interval | Normal; no wake is due yet | None |
| `next_wake_at` overdue while the process is alive | `TriggerRouter` is not advancing the schedule | Section 4.1 |
| `last_outcome` records a skipped/error wake | The turn was skipped or bounded by a provider error; the schedule advanced | Inspect the outcome/provider error, no forced reset |
| `assistant schedule: due scan failed` or `ensure failed` in logs | Workspace store or workspace enumeration failed | Inspect the store error, then Section 4.1 |
| `event wake dropped: workspace wake in flight` repeated for one workspace | An in-flight guard is stuck | Section 4.2 |
| `stalledReason=*` from the retained Supervisor API | Legacy Workflow-derived signal (pre-cutover stores only; not produced by the Assistant scheduler) | Section 4.3 |

Do **not** infer liveness from chat or Assistant activity. A fresh Assistant
turn can coexist with a stalled scheduler; the health API deliberately ignores
it.

## 4. Stalled playbooks

### 4.1 Overdue Assistant schedule

1. Read the schedule from `/agent/schedule` or:
   `SELECT enabled, next_wake_at, last_wake_at, last_outcome FROM assistant_schedules WHERE workspace_id = ?`.
2. Confirm `next_wake_at` is in the past and `last_wake_at` is older than one
   interval. A disabled row is paused, not stalled.
3. Check process/host liveness and logs for: `assistant schedule: due scan
   failed`, `assistant schedule: ensure failed`, `assistant schedule: mark wake
   failed`, or a hung host.
4. If the process crashed, restart it. Startup recovery marks in-flight Runs
   `interrupted` and appends `run.interrupted` with `incomplete_tail=true` and
   `last_persisted_seq` (Section 6).
5. After restart, wait one 30-second `TriggerRouter` tick. A due enabled row is
   dispatched, and `last_wake_at`/`last_outcome` update while `next_wake_at`
   advances.
6. If `next_wake_at` never advances, collect two `assistant_schedules` reads
   and the matching logs as evidence. Do not edit the row to fake a wake.

### 4.2 In-flight wake stuck

1. Repeated `event wake dropped: workspace wake in flight` for one workspace
   means the in-memory per-workspace guard was not released (for example a hung
   executor).
2. Confirm no Run or Job for that workspace is actually active. A genuinely
   running turn is expected to hold the guard; no action is needed.
3. Restart the process to clear in-memory guards once the active turn is
   settled. Startup recovery (Section 6) handles interrupted Runs.
4. A retained `due_not_started` value from the Supervisor API is a pre-cutover
   Workflow signal. Do not create Workflow revisions. Ensure the workspace has
   an `assistant_schedules` row (the router ensures one on its next tick) and
   verify that the router advances it.

### 4.3 Persisted clock rollback (retained Supervisor)

`supervisor_state` is a retained legacy row: no production path advances it
after the Assistant cutover, so a persisted clock rollback here is historical
diagnostic data, not Assistant scheduler health.

1. Compare host UTC time with `last_completed_at` in `supervisor_state`.
2. If the host clock is wrong, fix time synchronization and restart.
3. Do not update `last_completed_at` by hand. The observable recovery check is
   the Assistant schedule: after restart, a due row is dispatched and
   `assistant_schedules.last_wake_at`/`next_wake_at` advance. A store error
   appears as `assistant schedule: due scan failed` in the logs.
4. If a restored backup legitimately contains future timestamps, record the
   backup identity and the observed skew as evidence. Do not wait for a
   `supervisor.recovered` event; it is not emitted by the current Assistant
   scheduler.

## 5. Per-row dead-wake retry

Dead wakes are listed from `task_wake_outbox WHERE dispatch_state='dead'` with
their original `event_id`, target `task_id`, failure reason, and creation time.
The outbox identity is `(task_id, event_id)`: the same event ID may exist under
two Tasks, and each row is retried independently.

Failure reasons written by production paths:

- `cancelled` — a pending wake was retired because its Task was cancelled;
- `dispatch_failed` — the wake dispatcher exhausted its bounded consecutive
  failure budget.

A dead wake whose target Task is `completed`, `failed`, `cancelled`,
`cancelling`, or `interrupted` is refused with `409 Conflict`; it is not
re-pended. This prevents a cancellation-produced dead wake from becoming a
blind replay.

Requirements:

- The caller must have Agent management permission. The permission and
  tenant/workspace checks happen before the workspace store is opened.
- A workspace member without that permission sees the panel read-only.
- Confirming a row retries **only** that original `(task_id, event_id)` row.
  The outbox primary key is unchanged; no replacement event is inserted.
- A repeated confirmation returns the same pending fact and does not duplicate
  the wake. There is no bulk retry entry point.
- Retry is an admin API operation. The Assistant view `#/assistant` has no
  dead-wake list in this cutover, so use the Supervisor API and inspect
  `task_wake_outbox`.

Procedure:

1. Read `GET /api/v1/workspaces/{workspace_id}/harness/supervisor` and verify
   the row's `eventId`, target Task, and failure reason.
2. Confirm the failure reason is understood; a retry is not a blind replay of
   a device action. Effects still follow their own dispatch/approval gates.
3. POST the retry endpoint above. The response keeps the original identity.
4. Verify the row is now `pending` in `task_wake_outbox` and the row count did
   not increase.
5. If the wake dies again, the same row returns to `dead` with the new failure
   reason. Investigate the cause before retrying again.

If the API returns 403, the caller lacks Agent management permission; do not
retry with a broader role or a different workspace token. If it returns 404,
the event identity is not a dead or pending wake in this workspace store.

## 6. Startup crash recovery

At startup, `bootstrap::reconcile_harness_runs` iterates registered workspaces
that already own a Harness store and calls
`Db::recover_interrupted_harness_runs`. For each `agent_runs` row in
`running`/`model_calling`:

- the run transitions to `interrupted`;
- an in-flight `agent_tasks` row transitions to `interrupted`;
- `run.interrupted` is appended in the same transaction with
  `incomplete_tail=true` and `last_persisted_seq` computed only from that run;
- the event keeps the run's own `execution_mode` (simulation stays simulation);
- repeated recovery is idempotent and emits no second event.

A workspace without a Harness store is skipped; recovery never creates empty
stores.

### 6.1 Interrupted device Job → Effect `unknown` (never `failed`)

When startup Job recovery cannot prove whether a physical send happened, it
marks the Job `interrupted`. The Effect is **not** failed:

- `dispatching` settles to `unknown` and atomically creates one
  `kind=effect_reconciliation` Decision (`requested`) with
  `decision_id = reconciliation:<effect_id>`; the Run stays blocked.
- after a stop request the same Job fact settles `cancelled_unknown`, not a
  confirmed stop.
- an `unknown` Effect is never re-dispatched automatically.

Resolve it by confirming the physical outcome, then answering the Decision:

```text
POST /api/v1/workspaces/{workspace_id}/agent/effects/{effect_id}/reconcile
{"answer": "applied" | "not_applied", "evidence": {...}}
```

The answer and its continuation wake are committed immediately. The periodic
(and startup) recovery pass consumes the answer into the Effect
(`completed` / `not_applied`) even while the Task is paused; the Task itself
stays paused. Resume is refused with a 409 whose message names the
reconciliation wait until that consumption has happened (normally within one
dispatch interval). The operator order is therefore:

1. answer the reconciliation;
2. wait for the Effect to leave `unknown` (the Harness attention item clears and
   the Decision becomes `consumed`);
3. resume the Task (`POST .../agent/tasks/{task_id}/resume`); the pending
   continuation then runs.

A resume attempted between steps 1 and 2 is not lost; it returns the
reconciliation conflict and can be retried. Never answer from inference:
`applied` asserts the device action executed, `not_applied` asserts it did not.

### 6.2 Stuck `dispatching` after a transient settle failure

The Job result sink can fail to settle an Effect (for example a transient
SQLite busy/unreadable-workspace error) and only logs it. The periodic Effect
pass now also settles every `dispatching`/`cancelling` Effect whose Job is
already terminal and ensures reconciliation for `unknown` Effects, so such an
Effect clears without a process restart. A still-active Job is never touched,
and a `dispatching` Effect with no Job row is only handled by startup
reconciliation (the periodic pass must not race a live permit/submit window).
If a `dispatching` Effect persists beyond a few seconds, inspect the Job row and
the workspace store errors before restarting.

## 7. Evidence to collect

Record, without secrets or raw provider payloads:

- the `/agent/hud` and `/agent/schedule` responses (status, next/actual wake,
  and last outcome);
- two `assistant_schedules` reads showing
  `next_wake_at`/`last_wake_at`/`last_outcome`;
- matching `session_events` rows: `run.interrupted` and Task/Run lifecycle
  events;
- for pre-cutover stores only: the retained Supervisor GET response and
  `supervisor_state` reads showing `tick_seq`/timestamps; `supervisor.stalled`
  / `supervisor.recovered` events may not exist on current deployments (the
  Supervisor is not advanced after cutover);
- for retry: the original `eventId`, target Task, failure reason, and the
  post-retry `dispatch_state='pending'` row with `retried_by`/`retried_at`;
- host time source/skew if a rollback was involved.

## 8. Non-goals and boundaries

- Do not hand-edit `assistant_schedules`, `supervisor_state`, `task_wake_outbox`,
  or session events to hide a failure.
- Do not add a bulk retry or a per-tick permanent event stream.
- Do not claim on-time scheduling from `next_wake_at`; it is a plan.
- This runbook does not prove 24h reliability and does not execute the soak.
