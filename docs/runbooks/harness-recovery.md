# Harness Supervisor recovery runbook

Scope: one workspace Harness store (`data/agents/<workspace_id>/harness/harness.db`).
This runbook covers scheduler health, stalled/recovered transitions, startup
crash recovery, and per-row dead-wake retry.

It does not cover provider/model outages, device-driver faults, e-stop handling,
or the 24h soak procedure. `scheduled` is a plan time; it is never a promise of
on-time execution. An AI heartbeat is **not** process liveness.

## 1. Invariants

- `supervisor_state` is a single mutable row:
  `tick_seq`, `last_started_at`, `last_completed_at`, `last_error`,
  `stalled_reason`. Normal ticks only advance this row.
- Only a `stalled` or `recovered` transition appends a permanent
  `session_events` fact (`supervisor.stalled` / `supervisor.recovered`).
- In-process health math uses a monotonic anchor, so a wall-clock jump inside
  the process does not fake a deadline. Persisted UTC is compared after restart
  to detect a real rollback.
- A successful tick clears the recovering state and writes one recovery event.
- Dead-wake retry reuses the original `event_id`. It never creates a
  replacement outbox row and never retries a batch.

## 2. Read the panel and API

The Harness view is `#/harness/supervisor` (`view-harness-supervisor`).
The API is:

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

`idle` with `noNewRuns=true` is a normal state, not a failure. Only `stalled`
means a concrete scheduler problem. `nextWakeAt` is shown as a planned time,
not an on-time guarantee.

## 3. Classify the state

| Observation | Meaning | Action |
|---|---|---|
| `status=idle`, no enabled Workflow | Normal idle scheduler, ticks still run | None |
| `status=healthy`, next wake far in the future | Normal; no Run is due yet | None |
| `stalledReason=tick_deadline_exceeded` | A tick started but did not finish inside the configured deadline (hang/crash), or the process died mid-tick | Section 4.1. A later **completed** tick (success or error) ends this stall |
| `stalledReason=due_not_started` | An enabled cron Workflow's planned boundary passed the grace window and the scheduler never advanced it | Section 4.2 |
| `stalledReason=clock_rollback` | Persisted `last_completed_at` is in the future relative to now (wall clock moved backwards across restart) | Section 4.3 |
| `lastError` non-null but not stalled | Tick completed with a bounded error; next ticks continue | Inspect the error, no forced reset |

Do **not** infer liveness from chat/heartbeat activity. A fresh AI heartbeat can
coexist with a stalled scheduler; the health API deliberately ignores it.

## 4. Stalled playbooks

### 4.1 Tick deadline exceeded

1. Confirm `tickSeq` stopped advancing across two reads at least one tick apart.
2. Check process/host liveness and logs for the workspace:
   `workflow scheduler tick failed` or a hung host.
3. If the process crashed, restart it. Startup recovery marks in-flight Harness
   Runs `interrupted` and appends `run.interrupted` with
   `incomplete_tail=true` and `last_persisted_seq` (Section 6).
4. After restart, wait for a completed tick. `last_completed_at` advances and
   exactly one `supervisor.recovered` event is written. A tick that completes
   with an error also ends a tick-deadline stall and records `last_error`;
   `last_error` is cleared only by a later successful tick.
5. If the tick never completes, collect logs and the current `supervisor_state`
   row as evidence. Do not edit the row to fake a recovery.

### 4.2 Due not started

1. Read enabled Workflows and their cron cursors:
   `agent_workflows.enabled=1`, `workflow_cron_cursors.next_slot`.
2. Compare `next_slot` with the current UTC time. A cursorless enabled Workflow
   uses its `agent_workflows.updated_at` (enable time) as the planned boundary.
3. If the scheduler process is down, restart it. The first tick initializes a
   cursorless plan, advances overdue boundaries, and writes `trigger.missed`
   for skipped slots (XD8: never backfill old slots).
4. If the boundary stays overdue while ticks advance, treat the cron expression
   or Workflow revision as invalid; fix the configuration by appending a new
   revision. Do not delete history or rewrite the cursor manually.
5. Recovery is automatic after the first successful tick that clears the
   stalled reason.

### 4.3 Persisted clock rollback

1. Compare host UTC time with `last_completed_at` in `supervisor_state`.
2. If the host clock is wrong, fix time synchronization and restart.
3. Do not update `last_completed_at` by hand. A correct clock plus a successful
   tick is the recovery evidence.
4. If a restored backup legitimately contains future timestamps, record the
   backup identity and the observed skew as evidence, then restart with the
   corrected clock and verify `supervisor.recovered`.

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
- The UI disables only the selected row during the request, keeps the full
  list, and shows a persistent inline result.

Procedure:

1. Open `#/harness/supervisor` and verify the row's `eventId`, target Task, and
   failure reason.
2. Confirm the failure reason is understood; a retry is not a blind replay of
   a device action. Effects still follow their own dispatch/approval gates.
3. Click `按原事件重试`. The row remains visible with a persistent
   `已按原事件 <event_id> 重新排队，状态：等待调度。` result.
4. Verify the row is now `pending` in `task_wake_outbox` and the row count did
   not increase.
5. If the wake dies again, the same row returns to `dead` with the new failure
   reason. Investigate the cause before confirming again.

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

- the GET response for the workspace (health + dead wakes);
- two `supervisor_state` reads showing `tick_seq`/timestamps;
- matching `session_events` rows: `supervisor.stalled`,
  `supervisor.recovered`, `run.interrupted`, `trigger.missed`;
- for retry: the original `eventId`, target Task, failure reason, and the
  post-retry `dispatch_state='pending'` row with `retried_by`/`retried_at`;
- host time source/skew if a rollback was involved.

## 8. Non-goals and boundaries

- Do not hand-edit `supervisor_state`, `task_wake_outbox`, or session events to
  hide a failure.
- Do not add a bulk retry or a per-tick permanent event stream.
- Do not claim on-time scheduling from `nextWakeAt`; it is a plan.
- This runbook does not prove 24h reliability and does not execute the soak.
