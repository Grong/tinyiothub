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
requires approved canary evidence. An invalid payload returns HTTP 400
`suppression_config_invalid` and leaves the previous configuration unchanged.

```bash
for MODE in shadow canary enforce; do
  curl --fail-with-body -X PUT "$API/schedule" -H "$AUTH" -H "$JSON" \
    -d "{\"suppressionMode\":\"$MODE\"}" \
    | jq '{suppressionMode, suppressionConfigVersion}'
  curl --fail-with-body "$API/suppression/preflight" -H "$AUTH" | jq '{mode, wouldSkip, vetoes}'
done
```

A mode change that lacks valid evidence fails closed with HTTP 409
`shadow_gate_not_met`; nothing is written. `off` never needs evidence and is
always immediate.

### 3.1 Gate evidence (canary and enforce)

Evidence lives in the workspace table `suppression_gate_evaluations` as
immutable, append-only artifacts. There is no update or delete path, and
re-appending the same `artifactId` is rejected. Every artifact records:

- `artifactId`, `workspaceId`, `mode` (`shadow` unlocks canary, `canary`
  unlocks enforce), sample turn IDs and dates, `registryVersion`, and a
  `synthetic` flag;
- explicit denominators, the estimated or measured values, preflight latency
  distribution (`p50/p95/p99`), the false-skip review (reviewer, conclusion,
  every sample reviewed) and the approver;
- `approvedAt` and `contentSha256` over the canonical artifact bytes (JCS,
  excluding the hash field itself).

Shadow numbers are **counterfactual estimates**, never realized savings:

```text
estimated model savings = model calls on wouldSkip Turns / all shadow model calls
estimated tool savings  = model-driven tool calls on wouldSkip Turns / all shadow tool calls
```

Shadow artifacts also record `wouldSkip` ratio, per-Source veto distribution,
longest consecutive wouldSkip run, unavailable-Source full executions,
reconciliation full executions and preflight latency. Shadow evidence must
cover at least 3 representative days and 100 scheduled Turns with a
`wouldSkip` ratio of at least 50%.

Canary evidence is **measured**: actual model-call reduction, actual tool-call
reduction and the human-labelled duplicate-report reduction, each computed
from baseline and canary denominators, plus the preflight latency distribution
and the human false-skip review. Canary must cover at least 20 real Turns with
model and tool reductions of at least 40%, duplicate-report reduction of at
least 80%, preflight p95 at or below 250ms and no confirmed false skip after
review. Every reviewed `wouldSkip` sample must be marked reviewed.

`PUT suppressionMode=canary` validates the newest approved shadow artifact;
`PUT suppressionMode=enforce` validates the newest approved canary artifact.
Missing sample IDs, denominators or approver, unreviewed false-skip samples,
below-threshold counts, a tampered `contentSha256` and stale artifacts are all
rejected with 409 `shadow_gate_not_met`.

Staleness follows ruling R18 only — there is no age window:

1. the artifact's `registryVersion` differs from the current registered
   Sources, or
2. a confirmed `false_skip_detected` event exists after `approvedAt`.

Synthetic evidence (`synthetic=true`) is valid only in the loopback
quickstart fixture. Production validation always rejects it; there is no
parameter that bypasses the artifact gate. The flag is set by server
configuration in the loopback fixture only; a request field named `synthetic`
is ignored.

### 3.2 Publish gate evidence (admin, idempotent)

Both endpoints require workspace write and an `Idempotency-Key`. Neither
changes `suppressionMode`; the separate `PUT` remains the only mode change.

Metrics are derived server-side from Turns recorded by the wake path. The
request supplies references and human provenance only; model/tool counts are
never typed by the caller.

```bash
# 1. Shadow evidence: all sampleTurnIds must be recorded mode=shadow Turns.
curl --fail-with-body -X POST "$API/suppression/gate-artifacts" \
  -H "$AUTH" -H "$JSON" -H 'Idempotency-Key: gate-shadow-001' -d '{
    "mode": "shadow",
    "sampleTurnIds": ["turn:shadow-a", "turn:shadow-b", "turn:shadow-c"],
    "from": "2026-11-10T00:00:00Z",
    "to": "2026-11-13T00:00:00Z",
    "approver": "alice"
  }' | jq '{artifactId, mode, approvedAt, contentSha256, synthetic}'
```

```bash
# 2. Canary evidence: sampleTurnIds are mode=canary Turns, baselineTurnIds are
#    the pre-canary mode=shadow Turns. Only the human duplicate-report labels
#    and false-skip review are caller-supplied.
curl --fail-with-body -X POST "$API/suppression/gate-artifacts" \
  -H "$AUTH" -H "$JSON" -H 'Idempotency-Key: gate-canary-001' -d '{
    "mode": "canary",
    "sampleTurnIds": ["turn:canary-a", "turn:canary-b"],
    "baselineTurnIds": ["turn:shadow-a", "turn:shadow-b"],
    "from": "2026-11-13T00:00:00Z",
    "to": "2026-11-16T00:00:00Z",
    "approver": "alice",
    "duplicateReportReview": {
      "labeler": "bob",
      "labelledAt": "2026-11-16T00:00:00Z",
      "baselineReports": 50,
      "canaryReports": 8
    },
    "falseSkipReview": {
      "reviewer": "carol",
      "reviewedAt": "2026-11-16T00:00:00Z",
      "conclusion": "no_confirmed_false_skip",
      "confirmedFalseSkips": 0,
      "samples": [{"turnId": "turn:canary-a", "reviewed": true, "confirmedFalseSkip": false}]
    }
  }' | jq '{artifactId, mode, approvedAt, contentSha256, synthetic}'
```

A missing/foreign/duplicate `turnId`, a sample outside `from`/`to`, the wrong
observation mode, missing human provenance, or an artifact that does not match
its recorded observations returns HTTP 409 `shadow_gate_not_met` (duplicate IDs
in the request are a 400 `suppression_config_invalid`). Replaying the same
`Idempotency-Key` returns the recorded artifact without appending a second row.

### 3.3 Record a confirmed false skip

This is the only producer of the `false_skip_detected` alert used by R18
staleness. It requires workspace write and an `Idempotency-Key`, and the
`turnId` must be a recorded canary/enforce `wouldSkip` observation.

```bash
curl --fail-with-body -X POST "$API/suppression/false-skip" \
  -H "$AUTH" -H "$JSON" -H 'Idempotency-Key: false-skip-001' -d '{
    "turnId": "turn:canary-a",
    "reviewer": "carol",
    "rootCause": "adapter tombstone dropped a closed work item"
  }' | jq .
# => {"code":"false_skip_detected","turnId":"turn:canary-a","reviewer":"carol"}

# The alert is now visible to the report and invalidates any artifact approved
# before this confirmation. Fix the root cause and publish fresh evidence.
curl --fail-with-body "$API/suppression/report" -H "$AUTH" | jq '.alerts'
```

### 3.4 Gate authoring and false-skip in a real deployment versus the fixture

The gate-authoring commands in §3.2 and the false-skip confirmation in §3.3 are
**production operations**. They derive every non-human metric from the
workspace observation table and require real recorded Turns; never point them
at the loopback quickstart fixture as evidence of production benefit.

The loopback fixture (`cargo run -p tinyiothub-cloud --example
suppression_quickstart`, see `docs/dev/assistant-suppression.md` §6) pre-seals
explicitly `synthetic=true` shadow and canary artifacts at startup from
synthetic observations. It accepts them only because its server configuration
sets `suppression_gate_allow_synthetic`; the production `PUT` path still rejects
synthetic evidence and no request field can set the flag. The fixture adds no
second gate-authoring path: §3.2 remains the only HTTP writer for artifacts and
§3.3 remains the only confirmed-false-skip producer.

To exercise the runbook against the fixture without real Turns:

```bash
cargo run -p tinyiothub-cloud --example suppression_quickstart
# Other shell: synthetic values only, never production credentials.
BASE=http://127.0.0.1:3109
WS=ws-suppression-demo
AUTH="Authorization: Bearer test-token"
curl --fail-with-body "$BASE/api/v1/workspaces/$WS/agent/suppression/report" -H "$AUTH" \
  | jq '{mode, state, sources, vetoes, alerts}'
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

At startup the TriggerRouter awaits recovery for every workspace before its
first tick, holding the workspace in-flight guard for the duration. A racing
scheduled dispatch still hits the durable CAS gates:

1. A stale `full_attempt_in_progress` is inspected by its terminal event and
   settled to `full_failed`; the next wake may run a full Turn. Fences are
   identified by their own `fullAttemptId`/`auditAttemptId`, never by a
   `latestAttemptId` that a later attempt overwrote, and are settled only when
   older than the 5-minute stale threshold.
2. A stale `audit_in_progress` (which can never own a committed skip, because
   the skip state and its six events commit atomically) is settled to
   `audit_failed`; the old baseline stays ineligible and the next skip is
   vetoed.
3. Every set sync barrier is reconciled from the latest immutable
   `turn.completed(outcome=skipped)` terminal, independent of `latestOutcome`
   (a later full Turn may have overwritten it). Projection and notification are
   cleared when their consumers succeed; the Schedule barrier is cleared only
   after a successful Schedule write for that workspace.

Recovery is idempotent and uses at most three total attempts (initial + two
retries, 1s/2s). Unknown commit results reconcile by attempt/event identity
before any retry; permanent configuration/contract errors are not retried.

**Multi-replica shared-store deployment:** settlement without a lease is
unsupported. Run a single writer process per workspace DB, or disable automatic
settlement until a lease is implemented. Two replicas must not both settle
fences for the same workspace.

### 6.2 `committed_partial` Schedule barrier

`schedule_sync_pending=true` means "a skip committed, the Schedule write for it
is unconfirmed". Do not replay blindly:

```bash
# 1. Confirm the immutable skip terminal and the fences.
curl --fail-with-body "$API/suppression/report" -H "$AUTH" \
  | jq '{latestOutcome: .state.latestOutcome, fences: .state.fences, alerts: .alerts}'

# 2. Keep the workspace off while repairing the Schedule writer / workspace DB.
curl --fail-with-body -X PUT "$API/schedule" -H "$AUTH" -H "$JSON" \
  -d '{"suppressionMode":"off"}' | jq '{suppressionMode}'
```

`POST /suppression/reconcile` **does not clear sync barriers**: it forces a full
Turn, and a full Turn overwrites `latestOutcome` without clearing the skip's
flags. Barriers are cleared by the committed-skip consumer reconciliation that
runs after any suppression-aware Turn, by startup recovery on restart (which
identifies the committed skip from its immutable terminal), and — for Schedule
only — by the TriggerRouter after its next successful Schedule write. Startup
recovery defers while a live attempt is younger than 5 minutes; retry after the
threshold.

```bash
# 3. Restart the service, then verify startup recovery cleared the barrier.
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

After the consumer succeeds, the committed-skip consumer reconciliation after
the next suppression-aware Turn (or a restart's startup recovery) clears the
barrier. `POST /suppression/reconcile` does **not** clear
projection/notification barriers: it forces a full Turn, which leaves the flags
intact. Startup recovery defers while a live attempt is younger than 5 minutes;
retry after the threshold. Recovery never emits a new skipped Turn for an
already committed skip.

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

The next successful full Turn installs a fresh baseline and clears
`audit_failed`. `POST reconcile` does not clear sync barriers; restart after
repairing their consumers so startup recovery can reconcile them. Re-enable
only after two clean preflights.

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
**fresh approved evidence**: the artifact that unlocked the previous mode is
stale once a confirmed false-skip event exists after its `approvedAt`, and the
newest artifact is the one validated. Publish a new approved artifact against
the current `registryVersion`, then `canary` and `enforce` in order. No
parameter bypasses the gate; `off` always remains available.

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
unless a real error, audit fence or sync barrier exists. If a sync barrier
remains, restart the service so startup recovery reconciles it; `POST reconcile`
does not clear barriers.

### Verification

The barrier-recovery sequence in §6.2/§6.3 is covered end to end by:

```bash
cargo test -p tinyiothub-cloud --lib recovery_clears_orphaned_skip_barriers_after_a_later_full_turn
cargo test -p tinyiothub-cloud --lib successful_schedule_write_clears_the_skip_schedule_barrier
```
