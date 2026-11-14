# Runbook: Assistant Suppression

Agent-ready errors, alerts and sync recovery for the Assistant suppression
gate. All commands use the Task 7 HTTP API only; there is no external
notification channel and no production gate bypass.

## 1. Prerequisites

```bash
set -euo pipefail
command -v jq >/dev/null
BASE="${BASE:-http://127.0.0.1:3109}"
WS="${WS:?set WS to the workspace id}"
TOKEN="${TOKEN:?set TOKEN to a workspace read/write token}"
AUTH="Authorization: Bearer $TOKEN"
JSON="Content-Type: application/json"
API="$BASE/api/v1/workspaces/$WS/agent"
```

Reads need workspace read; `PUT`/`POST` need workspace write. Errors that are
not one of the expected decisions return `application/problem+json`.

## 2. Inspect the current gate (read-only)

```bash
# Decision, wouldSkip, vetoes, registryVersion and public Source refs.
curl --fail-with-body "$API/suppression/preflight" -H "$AUTH" | jq .
```

```bash
# Public projection: mode, fences, recent outcomes, metrics, alerts and
# notification state. Never contains payloads, memory/skill bodies,
# workKeys, revisions or credentials.
curl --fail-with-body "$API/suppression/report" -H "$AUTH" \
  | jq '{mode, state, vetoes, alerts, notifications, metrics}'
```

Successful-skip checks to record before and after recovery:

```bash
curl --fail-with-body "$API/suppression/report" -H "$AUTH" \
  | jq '{latestOutcome: .state.latestOutcome, fences: .state.fences,
         alertsActive: .alerts.active, alertCode: .alerts.code,
         notificationPending: .notifications.pending}'
```

## 3. Switch modes (kill switch and rollout)

`off` is the only immediate escape hatch and needs no other fields:

```bash
curl --fail-with-body -X PUT "$API/schedule" -H "$AUTH" -H "$JSON" \
  -d '{"suppressionMode":"off"}' | jq '{suppressionMode, suppressionConfigVersion}'
```

Rollout order is `off -> shadow -> canary -> enforce`. Shadow computes
`wouldSkip` without skipping; canary requires approved shadow evidence; enforce
requires canary evidence. An invalid payload returns HTTP 400
`suppression_config_invalid` and leaves the previous configuration unchanged.

```bash
for MODE in shadow canary enforce; do
  curl --fail-with-body -X PUT "$API/schedule" -H "$AUTH" -H "$JSON" \
    -d "{\"suppressionMode\":\"$MODE\"}" \
    | jq '{suppressionMode, suppressionConfigVersion}'
  curl --fail-with-body "$API/suppression/preflight" -H "$AUTH" | jq '{mode, wouldSkip, vetoes}'
done
```

## 4. Wake and reconcile (write operations)

Every `POST` requires a unique `Idempotency-Key`; the same key replays the
recorded attempt, even if Sources changed. A different operation or request
under the same key returns HTTP 409 `idempotency_conflict`; use a new key.

```bash
# Normal scheduled-wake semantics: may skip or run a full Turn.
TURN=$(curl --fail-with-body -X POST "$API/suppression/wake" \
  -H "$AUTH" -H 'Idempotency-Key: runbook-wake-001' | jq -r '.turnId')
echo "turn=$TURN"

# Forced full Turn, ignoring wouldSkip but keeping pre-turn evidence.
TURN=$(curl --fail-with-body -X POST "$API/suppression/reconcile" \
  -H "$AUTH" -H 'Idempotency-Key: runbook-reconcile-001' | jq -r '.turnId')
echo "turn=$TURN"

# Terminal state; 202 only promised sessionKey/turnId/outcome=accepted.
FINAL=""
for _ in $(seq 1 40); do
  BODY=$(curl --fail-with-body "$API/suppression/report?turnId=$TURN" -H "$AUTH")
  OUTCOME=$(jq -r '.turn.outcome // empty' <<<"$BODY")
  case "$OUTCOME" in
    completed|skipped|failed|cancelled) FINAL=1; jq . <<<"$BODY"; break ;;
  esac
  sleep 0.25
done
test "${FINAL:-}" = "1" || { echo "turn did not settle: $TURN" >&2; exit 1; }
```

In-flight work returns HTTP 409 `wake_in_flight`; read the current Turn and
retry with the same key after it finishes. Capacity saturation returns HTTP 429
`wake_backpressure` with `retryAfterMs`; do not add queued requests.

## 5. Error contract and alerts

Every error carries:

```text
code / what / fix / recovery{action,maxAttempts,sideEffects,baselineAdvanced,reconcile}
codeAnchor / context / trace{traceId,localPath} / redaction / channel
```

- `trace.localPath` is local-only and never returned publicly.
- Public errors omit payloads, memory/skill bodies, credentials, ref
  internals, workKeys, revisions, user paths and URL query strings.
- Async Turn failures write `turn.completed.error` plus the safe
  `errorMessage` summary.
- Alerts use `assistant.suppression.alert` through Session Events + tracing +
  HUD/report. The Session Events adapter excludes `assistant.*`, so an alert
  never becomes semantic work or re-triggers a wake.

Alert payload fields: `code`, `workspaceId`, `turnId`, `baselineStateId`,
`currentRegistryVersion`, `baselineRegistryVersion`, `vetoes`,
`consecutiveFailures`, `metrics`, `traceId`. Inspect via the report:

```bash
curl --fail-with-body "$API/suppression/report" -H "$AUTH" \
  | jq '.alerts, .state.fences'
```

`alerts.active` is derived from real errors (`lastError`) and audit fences, not
from `full_failed` alone: a deliberate off-mode reconcile keeps the old
baseline ineligible but is not an alert.

## 6. Recovery

### 6.1 Automatic startup recovery (R16)

At startup the TriggerRouter starts recovery for every workspace. A racing
scheduled dispatch still hits the durable CAS gates:

1. A stale `full_attempt_in_progress` is inspected by its terminal event and
   settled to `full_failed`; the next wake may run a full Turn.
2. A stale `audit_in_progress` (which can never own a committed skip, because
   the skip state and its six events commit atomically) is settled to
   `audit_failed`; the old baseline stays ineligible and the next skip is
   vetoed.
3. A committed skip (`latestOutcome=skipped`) is replayed from its immutable
   events. Projection and notification are reconciled; the Schedule barrier is
   cleared only after the TriggerRouter already recorded `lastOutcome=skipped`.

Recovery is idempotent and uses at most three total attempts (initial + two
retries, 1s/2s). Unknown commit results reconcile by attempt/event identity
before any retry; permanent configuration/contract errors are not retried.

### 6.2 `committed_partial` Schedule barrier

`schedule_sync_pending=true` with `latestOutcome=skipped` means "skip committed,
Schedule unconfirmed". Do not replay blindly:

```bash
# 1. Confirm the immutable skip outcome.
curl --fail-with-body "$API/suppression/report" -H "$AUTH" \
  | jq '{latestOutcome: .state.latestOutcome, fences: .state.fences}'

# 2. Reconcile the committed skip through the one shared full-Turn path.
curl --fail-with-body -X POST "$API/suppression/reconcile" \
  -H "$AUTH" -H 'Idempotency-Key: runbook-committed-partial-001' | jq .

# 3. Verify: the Schedule writer ran and the barrier is gone.
curl --fail-with-body "$API/suppression/report" -H "$AUTH" \
  | jq '{fences: .state.fences, alerts: .alerts}'
```

If the barrier remains, keep the workspace `off` and collect the report plus
the `assistant.suppression.alert` tracing line (code
`schedule_barrier_failed`) before retrying; do not create a second Turn for the
same planned wake.

### 6.3 Projection or notification barrier

`projection_sync_pending`/`notification_pending` mean the committed skip is
still the source of truth while its consumer has not succeeded:

- Projection: replay is read-only; update or confirm the session projection,
  then clear the barrier at most three total attempts.
- Notification/SSE: never rolls back committed events. Clients resume from the
  database high watermark and the server does not retry indefinitely.
  Persistent failure keeps `notification_pending=true` and alerts with code
  `notification_failed`.

```bash
curl --fail-with-body "$API/suppression/report" -H "$AUTH" \
  | jq '{fences: .state.fences, notificationPending: .notifications.pending, alerts: .alerts}'
```

After the consumer succeeds, run one `POST reconcile` to reconcile, or restart
the service and let startup recovery clear it. Recovery never emits a new
skipped Turn for an already committed skip.

### 6.4 Audit rollback / `audit_failed`

A rolled-back skip leaves `latestOutcome=audit_failed`, `lastError` set and
`baselineStateId` ineligible. No partial skipped lifecycle is visible. Ship the
workspace `off` while fixing the workspace DB, then:

```bash
curl --fail-with-body -X PUT "$API/schedule" -H "$AUTH" -H "$JSON" \
  -d '{"suppressionMode":"off"}' | jq .
curl --fail-with-body -X POST "$API/suppression/reconcile" \
  -H "$AUTH" -H 'Idempotency-Key: runbook-audit-failed-001' | jq .
```

The next successful full Turn installs a fresh baseline. Re-enable only after
two clean preflights.

### 6.5 False skip

```bash
# 1. Immediate kill switch.
curl --fail-with-body -X PUT "$API/schedule" -H "$AUTH" -H "$JSON" \
  -d '{"suppressionMode":"off"}' | jq .
# 2. Force one full reconciliation and compare baseline/current snapshots.
curl --fail-with-body -X POST "$API/suppression/reconcile" \
  -H "$AUTH" -H 'Idempotency-Key: runbook-false-skip-001' | jq .
curl --fail-with-body "$API/suppression/preflight" -H "$AUTH" \
  | jq '{wouldSkip, vetoes, sources}'
```

Re-enable only after root-cause repair, two consecutive clean preflights and
the existing shadow gate still passing. No parameter bypasses the gate.

## 7. Rollback

Kill switch (immediate, no data rollback):

```bash
curl --fail-with-body -X PUT "$API/schedule" -H "$AUTH" -H "$JSON" \
  -d '{"suppressionMode":"off"}' | jq '{suppressionMode, suppressionConfigVersion}'
curl --fail-with-body "$API/suppression/preflight" -H "$AUTH" | jq '{mode, vetoes}'
```

Then verify no stale alert remains after a deliberate off-mode reconcile:

```bash
curl --fail-with-body -X POST "$API/suppression/reconcile" \
  -H "$AUTH" -H "Idempotency-Key: runbook-rollback-$(date +%s)" | jq -r '.turnId'
curl --fail-with-body "$API/suppression/report" -H "$AUTH" \
  | jq '{mode, latestOutcome: .state.latestOutcome, alerts: .alerts}'
```

Expected: `mode=off`, full Turns continue normally, and `alerts.active=false`
unless a real error or audit fence exists.
