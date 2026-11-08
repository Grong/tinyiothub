# Harness 24h soak and fault-drill runbook (method only)

Scope: one staging deployment of the Harness Core with simulated devices and a
controlled model provider, run continuously for one 24h window while named
faults are injected. This runbook defines the method, the raw evidence to keep,
and the failure criteria.

**This 24h soak is not executed in this change.** No 24h process was started, no
`.log` or metric file was produced, and no result from this runbook may be cited
as executed evidence. The document exists so a future authorized run can be
reproduced and judged. Simulation and controlled-provider results never
substitute for production evidence, and this period sets no merge or production
soak gate — see section 1.

Chinese/English status words follow the product mapping used elsewhere:
`scheduled=计划`, `pending=等待调度`, `unknown=结果未知`,
`cancelled_unknown=停止结果未知`, `simulation=模拟运行`.

## 1. Scope and evidence boundary

1. Record the run as **method only** until the final table in section 7.5 is
   filled with raw artifacts. Until then the only honest statement is
   "有验证方法，尚未执行 24h soak".
2. A short integration or performance run is not a soak. Cross-run claims are
   limited to the exact window, commit, provider, and fault list recorded here.
3. A simulated device or controlled provider result never counts as
   production device, provider, or latency evidence.
4. No merge gate, release gate, or production gate is added by this task. The
   soak is a **future** gate; the result of a past run cannot retroactively
   pass a later commit.
5. Store raw artifacts outside git (under `target/harness-soak/`, which is
   ignored). Do not commit databases, logs, provider payloads, or tokens.

## 2. Preconditions and environment record

Before the scheduled start, record every field below in
`environment.json`. Missing fields invalidate the run rather than being filled
in from memory.

| Field | What to record |
| --- | --- |
| Binary | `tinyiothub-cloud` version, build profile, and full git commit SHA of the running binary |
| Config version | Config file paths, SHA-256 of each, Workspace `assistant_schedules` id/interval/permission mode/`next_wake_at`, Task and Session IDs, and provider/model configuration |
| Provider | Controlled provider name, model ID, script/scenario file hash, recorded latency/error/connection-drop injection policy |
| Device simulator | Simulator version, seed, device count per workspace, command/result script hash |
| Scheduled start / end | UTC ISO-8601 with offset, e.g. `2026-11-10T00:00:00+00:00` |
| Actual start / end | UTC ISO-8601 with offset of process start and of the final quiesce |
| Machine | CPU model, logical CPU count, RAM bytes, disk model and filesystem |
| OS / kernel | OS version, kernel version, architecture |
| Workspaces | Count, plus non-secret workspace IDs and the workspace-to-device mapping |
| DB / WAL | `journal_mode=WAL`, `synchronous`, `busy_timeout`, `page_size`, `cache_size`, `wal_autocheckpoint`, and the per-workspace path layout (`data/agents/<workspace_id>/harness/harness.db`) |
| Idle pools | `maxConnections`, `minConnections`, `acquireTimeout`, `idleTimeout`, and the observed idle physical connection count per workspace after settle |
| `restore_journal` | Per-workspace `restore_journal` (epoch, phase, `updated_at`) must be recovered to a terminal journal state before start; fail only if it stays unreadable or unrecoverable. Record the recovered state |

Also record the host time source, timezone, and whether NTP was active, because
scheduled vs actual wake timing is compared across process restarts.

## 3. Workload

1. **Simulated devices.** Run a deterministic simulator that accepts the same
   command envelope as a real driver and appends every physical command to an
   append-only log keyed by `effect_id` and idempotency key. The simulator must
   be able to delay, reject, drop a response, or deliver a duplicate terminal
   result on command.
2. **Controlled provider.** Use a scripted/recorded provider with fixed model
   ID and scenario hash. It must be able to return success, a bounded error,
   a timeout, and to drop the connection during a model call (drill 10).
   Record its per-call latency and error class. Do not use a live third-party
   provider.
3. **Main and Subagent runs.** Include both execution modes with parent/child
   links. Record Main starts, Subagent starts, completions, parent linkage, and
   any orphaned or duplicated child.
4. **Scheduled vs actual wake.** Keep the planned wake (`scheduled`) and the
   first actual dispatch attempt (`actual`) separately. For every wake record
   `scheduled_at`, `actual_at`, `pending` duration, and oldest pending age.
   `scheduled` is a plan time, never an on-time promise. Report the drift
   distribution and the maximum; do not report only the median.
5. **Fixed evidence cadence.** Capture on a fixed schedule, not on demand:
   - every 60 s: scheduler, stream, and drain gauges;
   - every 5 min: host memory, DB/WAL bytes, idle pool counts;
   - on every fault: the pre/post snapshot around the injection;
   - every hour: a full bundle (metrics, DB sizes, logs, event counts);
   - at the end: a quiesced bundle after all Runs reach a terminal state or are
     explicitly dispositioned.
   A missed capture is an evidence gap and must be listed as such, not
   reconstructed later.

## 4. Metrics to capture raw, not summarized

Keep the raw samples and the queries that produced them. A single aggregate
score is not evidence. For each metric record the sample timestamp, the exact
query/endpoint, and the artifact file.

| Area | Raw data to keep |
| --- | --- |
| LLM errors | Every provider call: provider, model, start/end, latency, error class, retry count, and whether the Run was blocked or advanced |
| Tokens and cost | Per call `input_tokens`, `cached_input_tokens`, `output_tokens`, `cost_usd`; plus durable totals (`agent_runs.tokens`, `agent_daily_cost`). Never estimate a missing value |
| Effects | Counts and event IDs for every `EffectState`: `prepared`, `dispatching`, `cancelling`, `completed`, `failed`, `not_applied`, `cancelled`, `unknown`, `cancelled_unknown`. A zero count for `unknown`/`cancelled_unknown` must be shown from the raw rows, not assumed |
| Reconciliation | `kind=effect_reconciliation` decisions with state `requested`/`answered`/`consumed`/`withdrawn`, `effect.reconcile_attempted`, `effect.unknown`, and the time each unknown effect stayed blocked |
| Duplicate physical actions | Simulator command log grouped by `effect_id` and idempotency key; count of repeated physical commands (must be zero) and any repeated command ID |
| Recovery | Startup recoveries, `run.interrupted` (with `incomplete_tail` and `last_persisted_seq`), interrupted/unknown gauges, and each `restore_journal` phase transition |
| Compaction | Every compare/replace result: boundary sequence, state version, hash/digest match, `StaleAppend` or mismatch, and whether the replacement dispatched any effect (must be zero) |
| SSE `resync` | Harness Task SSE counters: `activeConnections`, `connectionsOpened`, `connectionsClosed`, `resyncs`, `duplicatesDropped`, `firstFrames`, `firstFrameMsTotal`; plus client-observed resync reason and cursor. The legacy `foreignTaskEvents` / `taskLessEvents` counters were retired with the platform Task stream |
| DB / WAL | `harness.db`, `-wal`, and `-shm` bytes on the fixed cadence; `dbQueryMs`; bounded-page latency; write latency samples; lock/busy counts |
| Memory | Host RSS bytes and percent, peak RSS, and per-workspace pool memory where available. RSS is approximate; report the sample method |
| Idle pools | Physical connections per workspace after the settle interval, and the pool settings in force |
| Scheduler | `tick_seq`, `last_started_at`, `last_completed_at`, `stalled_reason`, `nextWakeAt`, `pendingWakes`, `oldestPendingAt`, and every `supervisor.stalled` / `supervisor.recovered` event |

Record the raw metric snapshot as JSON, and quote log lines verbatim (with
timestamp and event name). Redact secrets before writing: no tokens, signed
URLs, provider keys, private objective text, image bytes, or full device
payloads.

## 5. Fault-drill timeline

Inject each fault at a recorded time. For every drill capture a pre-snapshot,
the exact injection, the post-snapshot, and the resulting durable facts.
`Scheduled injection` is the planned UTC time and `Actual injection` is when the
injection really occurred. `Expected outcome` is written before the run;
`Actual outcome` and `Open issues` are filled only from the post-drill
snapshot. Every drill artifact must carry a lowercase expected vs actual
comparison, and an expected vs actual mismatch fails the soak (section 6).
Because the soak is not executed in this change, every scheduled/actual/outcome
cell below is `—`.

| # | Drill | Injection | Scheduled injection | Actual injection | Expected outcome | Actual outcome | Evidence | Open issues |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | SQLite lock contention | Hold a write transaction on one workspace store past `busy_timeout` | — | — | Bounded wait then a fail-closed error; no partial transaction; other workspaces stay available; no duplicate effect | — | Busy/locked counts, error lines, `PRAGMA integrity_check`, effect rows | — |
| 2 | Slow disk | Add fsync/write latency to the workspace filesystem | — | — | Latency rises within a bounded margin; no silent loss; WAL grows and replays; no wrong terminal state | — | Write latency samples, DB/WAL bytes, replay result, integrity check | — |
| 3 | SSE disconnect | Drop the client connection mid-stream, then reconnect with the last cursor | — | — | Reconnect resumes with no gap or duplicate; `resync` only for an unknown cursor | — | Client cursor list, `resyncs`, `duplicatesDropped`, event IDs before/after | — |
| 4 | Clean restart | Send SIGTERM during idle and during an active Run | — | — | Graceful stop; in-flight Run becomes `interrupted`; exactly one `run.interrupted`; no duplicate Job or effect | — | `run.interrupted` rows, process exit, Job/effect identity counts | — |
| 5 | Crash restart | SIGKILL during `dispatching`, before and after the physical send | — | — | `dispatching` becomes `unknown` with a reconciliation decision in the same transaction; Run stays blocked; no blind resend; `restore_journal` remains consistent | — | `effect.unknown` + decision IDs, simulator log, `restore_journal` phase, restart logs | — |
| 6 | Duplicate job result | Deliver the same terminal result twice for one Job | — | — | One terminal transition, one event, one wake; the second result returns the current terminal state without a new wake | — | Job/effect rows, event IDs, wake count, simulator log | — |
| 7 | Compaction | Trigger context compaction and a projection snapshot compare/replace during live appends | — | — | Post-compaction behavior is equivalent to the canonical projection; no fact lost; a mismatch/`StaleAppend` keeps the newer snapshot and dispatches nothing | — | Compare result, boundary/hash, post-compaction event replay, dispatch count (zero) | — |
| 8 | Rebuild from durable facts | Stop dispatch and rebuild the projection from `session_events` | — | — | Rebuilt canonical state equals the pre-rebuild projection at the same high-watermark; no effect dispatched; if an append races the rebuild, the result is `StaleAppend` and no replacement snapshot is written | — | Rebuild duration, boundary seq, projection equality, `StaleAppend`, snapshot rows | — |
| 9 | Restore across a workspace DB swap | Restore an older backup while the external action may have completed after the backup | — | — | All non-terminal effects are promoted to `unknown`/recovery-required; Run pauses, wake stays, Command is not consumed; reconcile before any new dispatch; **no duplicate device action** | — | `restore_journal` epoch/phase, post-restore effect states, reconciliation decision, simulator command log | — |
| 10 | Provider connection drop | Drop the controlled-provider connection during a model call, then restore it | — | — | Bounded, classified provider error; Run blocks or retries per policy without repeating any device effect; no silent success; reconnect resumes the scenario | — | Per-call error record, Run/effect rows, simulator log, reconnect log | — |

Drills 5 and 9 are the duplicate-action probes: the simulator log must show one
physical command for the logical action even though the process or the database
was replaced. Drill 10 additionally checks that a provider disconnect is never
converted into a silent success or a repeated effect. This covers the spec's
"provider 断连" drill; the SSE disconnect in drill 3 is a client-stream fault,
not a provider outage.

## 6. Failure criteria

The soak fails on **any** of the following. A single occurrence is a failure,
not "known noise"; do not average it away.

1. **Any duplicate device action.** The same logical action is physically sent
   twice, whether from retry, recovery, reconcile, restore, or a duplicate job
   result.
   - Proving evidence: simulator append-only command log keyed by `effect_id`
     and idempotency key, deduplicated; every key appears at most once.
2. **Silent loss.** A scheduled wake never runs and is never recorded, a
   `step.tool_result` is missing, a per-run sequence has a gap without
   `incomplete_tail`, or a Run ends without a terminal event.
   - Proving evidence: scheduled-vs-actual wake reconciliation, contiguous
     per-run `seq` checks, terminal-event presence for every Run, and the
     durable event ledger.
3. **Wrong terminal state.** A Run or Task is marked successful while an effect
   is `unknown`, `dispatching`, or `cancelled_unknown`; a Task cancelled with a
   `cancelled_unknown` effect must end `failed(cancel_unknown)`, not
   `completed` or plain `cancelled`.
   - Proving evidence: terminal-state invariant query over effect, Run, Task,
     Job, and wake rows; late/duplicate terminal facts return the current state
     without overwriting it.
4. **State that cannot be reconstructed from durable facts.** The rebuilt
   projection differs from the live projection, or a required fact exists only
   in memory and disappears on restart.
   - Proving evidence: offline rebuild from `session_events` compared at the
     same boundary/high-watermark; `restore_journal` selection on restart;
     restart replay of `run.interrupted`.

A drill that produces the wrong outcome is not "expected behavior for a soak".
Stop the run, keep the artifacts, and record the failure with its evidence.

## 7. Evidence capture and raw templates

### 7.1 Directory and file naming

Create the root from the actual UTC start and the binary commit, for example
`target/harness-soak/2026-11-10T000000Z-<short-sha>/`. Names below are the
required kinds; extra files are allowed but must be listed in the index.

```text
environment.json                 # section 2 fields
scheduled-timeline.json          # every scheduled wake and inject time
metrics-<elapsed>s.json          # 60 s cadence snapshot
host-<elapsed>s.json             # 5 min RSS / DB / WAL / pool snapshot
db-<elapsed>s.json               # sizes, integrity, WAL frames
events-<elapsed>s.jsonl          # exported session_events or effect/decision rows
fault-<nn>-<drill>.json          # injection, expected vs actual outcome, open issues
logs/cloud.log                   # service log, UTC timestamps
logs/simulator.log               # physical command log
final-summary.json               # pass/fail table with actual numbers
README.txt                       # index, artifact map, redactions applied
```

Timestamps are UTC ISO-8601 with offset. Also record the monotonic elapsed
seconds from process start so clock jumps do not hide ordering. Every file
lists its own `captured_at`, `scheduled_at` when applicable, and the exact
command/query that produced it.

### 7.2 Metric snapshot template

```json
{
  "captured_at": "<UTC ISO-8601 with offset>",
  "elapsed_seconds": 0,
  "scheduled_at": "<UTC ISO-8601 or null>",
  "source": "<endpoint or SQL>",
  "values": {},
  "notes": "<any missed capture or redaction>"
}
```

Keep every raw value. Do not replace a per-workspace or per-run value with a
fleet total; totals can be derived later from the raw files.

### 7.3 Fault artifact template

Every `fault-<nn>-<drill>.json` must use these fields. A missing
`actual_outcome` or `open_issues` makes the capture incomplete; record an empty
list, not an omitted field.

```json
{
  "drill": "<nn>-<name>",
  "scheduled_injection": "<UTC ISO-8601 or null>",
  "actual_injection": "<UTC ISO-8601 or null>",
  "expected_outcome": "<copied from the timeline before the run>",
  "actual_outcome": "<observed result>",
  "expected_vs_actual_evidence": [
    "<query/event IDs/simulator log lines that show the comparison>"
  ],
  "open_issues": [],
  "pre_snapshot": "<path>",
  "post_snapshot": "<path>",
  "captured_at": "<UTC ISO-8601 with offset>"
}
```

`actual_outcome` describes what the run did, not what was hoped for.
`expected_vs_actual_evidence` must point at raw rows or log lines, not a
summary. `open_issues` lists unresolved questions even when the drill passes.

### 7.4 Log excerpt template

```text
<UTC timestamp> <level> <event name> <field=value ...>
```

Quote the exact line. Redact secrets and payload bodies before storage. A
paraphrase is not a log excerpt.

### 7.5 Final pass/fail table

Fill this only after the run ends. Until then every cell is `—` and the run is
"not executed in this change"; do not invent numbers.

| Criterion | Expected | Actual | Artifact | Pass/Fail |
| --- | --- | --- | --- | --- |
| Duplicate physical actions | 0 | — | `logs/simulator.log`, `final-summary.json` | — |
| Silent loss | 0 unexplained gaps | — | `events-*.jsonl`, wake reconciliation | — |
| Wrong terminal state | 0 | — | terminal-state invariant query | — |
| Unreconstructable state | 0 | — | rebuild comparison | — |
| `unknown` / `cancelled_unknown` | every one has a reconciliation decision and disposition | — | effect/decision rows | — |
| LLM errors | recorded and classified | — | per-call records | — |
| Tokens / cost | recorded per call | — | per-call records, `agent_daily_cost` | — |
| SSE `resync` | no silent gap or duplicate | — | `stream` gauges, client cursors | — |
| DB/WAL bytes and latency | recorded on cadence | — | `db-*.json`, `metrics-*.json` | — |
| Memory / idle pools | recorded on cadence | — | `host-*.json` | — |
| Fault drills 1–10 | expected vs actual outcome matches | — | `fault-*.json` | — |

## 8. What this soak does not prove

- It does not prove real-device firmware, driver, or network behavior. Devices
  are simulated; a simulator bug can hide a real-device failure.
- It does not prove live-provider quality, latency, pricing, or availability.
  The provider is controlled and scripted; a production provider outage outside
  the injected scenario is untested.
- It does not prove production load, tenant cardinality, or adversarial
  traffic. Workspace and device counts are test inputs, not capacity limits.
- It does not prove another machine, OS, filesystem, or Linux CI. Results are
  tied to the machine and kernel recorded in `environment.json`.
- It does not prove 24x7 reliability. A single 24h window is not a multi-day or
  production-duration claim.
- It does not prove hardware, camera, or playback paths; playback remains
  blocked in `runtime-rollout.md` section 4.
- It does not add or change quotas, and it does not create a merge or
  production soak gate in this period.
- It does not replace `harness-recovery.md` incident handling or
  `harness-performance.md` measurement. Those remain the procedures for their
  own scopes.

## 9. Revision history

- 2026-11-03: initial method. Not executed in this change; no 24h process was
  started and no soak result is claimed.
- 2026-11-06, fix round 1: added scheduled/actual injection, actual outcome,
  and open issues to the drill timeline and the `fault-*.json` template;
  relaxed the `restore_journal` precondition to "recover to a terminal journal
  state"; added the provider-connection-drop drill (drill 10); made the
  expected vs actual pairing explicit in the drill prose.
