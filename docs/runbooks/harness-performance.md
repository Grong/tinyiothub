# Harness performance and scale evidence

Reproducible local measurements for the Harness Core storage, replay, recovery,
backup and existing Task/stream/Job paths.

This runbook records **measurement inputs, not quotas, capacity limits, or an
SLA**. The numbers below come from the unoptimized debug test profile on one
developer machine. They are not production p99 and must not be quoted as such.

## 1. Reproduce

```bash
cargo test -p tinyiothub-cloud --bench harness -- --nocapture
```

Expected: one test passes and prints a JSON document between
`=== HARNESS_PERF_BEGIN ===` and `=== HARNESS_PERF_END ===`. The same document is
written to:

- `target/harness-perf/harness-perf-branch-<unix-seconds>.json`
- `target/harness-perf/harness-perf-branch-latest.json`

The entry point is `apps/cloud/benches/harness.rs`. Every latency set keeps its
raw samples. Percentiles use nearest-rank; p50/p95/p99 in the artifact are
local observations, not production percentiles.

Optional environment:

| Variable | Meaning |
| --- | --- |
| `HARNESS_BENCH_SCALE=smoke` | Smaller buckets for a quick run. The full run uses 1M events and 1,000 workspaces. |
| `HARNESS_BENCH_OUT=<dir>` | Artifact directory (default `target/harness-perf`). |
| `HARNESS_BENCH_MAIN_WORKTREE=<path>` | Records where a main worktree lives. No main samples are produced by this process. |
| `HARNESS_BENCH_MAIN_BIN=<path>` | Records a main binary path/size. No main samples are produced by this process. |

The artifact is `INCOMPLETE` when a required comparison is missing. It never
substitutes an invented baseline or a single aggregate score.

## 2. Environment captured by the 2026-11-05 run

| Field | Value |
| --- | --- |
| Machine | Apple M1, 8 logical CPUs, 17,179,869,184 bytes RAM |
| OS / kernel | macOS 26.3.1, Darwin 25.3.0 (arm64) |
| Rust | rustc 1.95.0 |
| SQLite | 3.50.2 |
| Workspace store journal | WAL, synchronous=2, foreign_keys=1, busy_timeout=5000 ms, page_size=4096, cache_size=-2000 (2 MiB), wal_autocheckpoint=1000 pages |
| Platform fixture | `sqlite::memory:` (benchmark isolation); production platform config is file-backed |
| Binary | `tinyiothub-cloud` 0.4.6, debug test profile |
| Branch / commit | `e4-p2-declaration-closeout` / `5a9eee81f317ef244fa62e4d6d277164211dc96d` |
| Main ref | `origin/main` `295d2209a3d762883791d2e8a820056d72d4d0ff` — no Harness/Runtime storage modules; local `main` is `5cf1947382a8339e2cb80ae06fa9a547df1e62fe` (see section 7) |
| Run duration | 187,173.864 ms |
| Final artifact | `target/harness-perf/harness-perf-branch-1793871065.json` (generated 2026-11-05T09:31:05.146341+00:00) |

Fixture details:

- Session events: 1k / 100k / 1M per workspace, 96-byte payloads, three
  composite indexes (`execution_mode,session_id/run_id/task_id,seq`).
- Recovery guard: 0 actionable, 100k terminal effects + 100k terminal commands,
  and 10k prepared + 10k requested actionable rows.
- Runtime events: 100k rows for the stream page, plus 200 rows with 64 KiB
  payloads.
- Task objective and Job args large cases: 64 KiB.
- Backup: 100,000 session events per workspace, three rounds.
- Concurrency: Task create 8 workers x 25; backup writers in two workspaces.

## 3. Methodology

### Startup, migration, recovery and idle pools

For each bucket (1, 10, 100, 1000 workspaces) the bench creates a fresh
file-backed workspace root, seeds the platform registry, then sequentially
opens and migrates every workspace store. It records the total and per-open
latency, an empty restore-journal scan, one running Run recovered per workspace,
a staged restore-journal recovery for every workspace, and the physical/idle
connection count held by the pools after a 100 ms settle. File bytes are read
from `harness.db` plus `-wal`/`-shm`.

The idle-pool number is the pool size after 100 ms, not after the configured
600 s idle timeout.

### Recovery-matrix guard (M3)

`Db::find_invalid_recovery_state` is timed with:

- no effects or commands;
- 100k terminal effects + 100k terminal commands (narrow path, partial index);
- 10k prepared effects + 10k requested commands (large actionable batch).

`EXPLAIN QUERY PLAN` is recorded for the narrow path. A control query with
`NOT INDEXED` records the full-scan cost the partial index avoids. The narrow
plan used `idx_effect_intents_recovery_scan` and `idx_commands_recovery_scan`.

### Session-event replay

Events are inserted cumulatively into one session (1k → 100k → 1M). A second
"gap" session holds 100 old events plus one newest event per bucket. Three
queries are timed at 30 samples each:

- **newest**: `after = max(seq) - 100`;
- **long gap**: gap session `after = 100`, which must jump over the big
  session's history;
- **after=0**: first page of the big session (server cap is 256 rows).

The bounded-page memory sample reads the first page and records RSS delta. The
"unbounded preload contrast" reads the whole session through
`projection_boundary_events`; it is the closest measured analogue to the old
whole-history prefetch, not a main-binary sample.

### Snapshot, offline rebuild and failure recovery

The bench reads a pre-written projection snapshot 50 times, then rebuilds the
canonical projection at the 1M-event boundary through `ProjectionInspector`
(no snapshot present), and finally runs the same comparison with a barrier that
appends one event between the fixed-boundary read and the post-rebuild
high-watermark recheck. The expected failure result is `StaleAppend` with no
replacement snapshot. Rebuild progress is not exposed by the production call.

### Backup and cross-workspace isolation

`VACUUM INTO` (the same SQLite mechanism used for pre-migration backups) writes
a workspace backup while writers append to the same workspace and to a second
workspace. Write latencies are filtered to the backup window. Each backup is
opened read-only and checked with `PRAGMA integrity_check`. Finally workspace A
is marked unavailable and workspace B continues writing.

### Task create / stream page / Job

Measured at the storage paths used by the HTTP handlers:

- `create_task_with_wake` (unique, duplicate, 64 KiB objective, 20 workspaces,
  8 concurrent workers);
- `runtime_events_page`, the bounded query behind the Task SSE endpoint
  (newest, oldest cursor, 64 KiB payload);
- `create_job` / `get_job` (unique, 64 KiB args).

The SSE transport, authentication middleware and HTTP serialization are outside
this measurement.

## 4. Raw results (2026-11-05, debug profile)

All times are milliseconds. `n` is the number of raw samples in the artifact.

### Startup / migration / recovery / idle pools

| Workspaces | Open+migrate total | Per-open p50 | Per-open p95 | Empty journal scan | Interrupted-run recovery total | Staged journal recovery total | Idle physical connections | DB bytes | WAL bytes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 10.851 | 10.851 | 10.851 | 0.178 | 0.678 | 1.828 | 2 | 4,096 | 498,552 |
| 10 | 89.412 | 8.763 | 10.049 | 0.083 | 5.876 | 16.145 | 20 | 40,960 | 4,985,520 |
| 100 | 916.163 | 8.675 | 10.421 | 0.110 | 56.746 | 161.597 | 200 | 409,600 | 49,855,200 |
| 1,000 | 9325.051 | 8.893 | 10.809 | 0.100 | 758.924 | 2903.915 | 2,000 | 4,096,000 | 498,552,000 |

Per-workspace interrupted-run recovery at 1,000 workspaces: p50 0.766, p95 1.016, max 2.792. The interrupted-run and staged-journal recoveries are linear in workspace count in this workload.

The 1000-workspace WAL total is ~498 KiB per store because each store's WAL has
not reached its 1000-page auto-checkpoint threshold. This is file state, not a
limit; it is not a claim that production leaves the same bytes on disk.

### Recovery-matrix guard

| Fixture | n | p50 | p95 | Notes |
| --- | --- | --- | --- | --- |
| no effects/commands | 50 | 0.094 | 0.105 | 0 effects, 0 commands |
| 100k terminal effects + 100k terminal commands | 20 | 0.098 | 0.124 | Narrow path; partial indexes selected |
| Same 100k terminal rows, `NOT INDEXED` control | 10 | 53.274 | 56.816 | Full-scan cost avoided by the partial index |
| 10k prepared effects + 10k requested commands | 10 | 534.777 | 570.233 | 110k total effects/commands |

The narrow query plan was `SCAN e USING INDEX
idx_effect_intents_recovery_scan` plus a correlated decision/task lookup, and
`SCAN commands USING INDEX idx_commands_recovery_scan`.

### Session-event replay (1k / 100k / 1M)

| Events | Newest p50/p95 (rows) | Long-gap p50/p95 (rows) | after=0 p50/p95 (rows) | Bounded page time / RSS delta | Unbounded preload | Preload time | Preload RSS delta | DB bytes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1,000 | 1.203 / 1.288 (100) | 0.187 / 0.206 (1) | 2.836 / 3.087 (256) | 2.866 / +0 B | 1,000 rows | 10.701 | +491,520 B | 4,096 |
| 100,000 | 1.202 / 1.368 (100) | 0.205 / 0.274 (2) | 2.832 / 3.044 (256) | 2.853 / +0 B | 100,000 rows | 1069.924 | -4,931,584 B | 43,393,024 |
| 1,000,000 | 1.246 / 1.494 (100) | 0.173 / 0.242 (3) | 2.863 / 2.933 (256) | 2.886 / -884,736 B | 1,000,000 rows | 10841.157 | +706,592,768 B | 437,366,784 |

The bounded page stays at 256 rows and constant RSS in this workload. The
unbounded contrast grows with total history. RSS is sampled with `ps -o rss=`
and is approximate; allocator reuse can make a delta zero or negative.

### Snapshot and offline rebuild

| Metric | Result |
| --- | --- |
| Daily snapshot read (n=50) | p50 0.109, p95 0.432, max 0.982 |
| Offline rebuild at 1,000,102 events | 51753.839 ms; 1,000,000 evidence refs; `SnapshotMissing` (no snapshot at the boundary) |
| Rebuild RSS delta (post-call) | +86,245,376 B; not a peak. RSS is directional only. |
| DB / WAL bytes before and after rebuild | 437,366,784 / 4,260,112 (unchanged) |
| Stale append during rebuild | `StaleAppend`, `appendedDuringCompare=true`, 56325.884 ms |
| Replacement on stale append | suppressed (`snapshotRowsAfter=0`) |

Rebuild progress is a single call in the production API; no progress callback
exists, so the artifact records that fact rather than a fabricated percentage.

### Backup write latency and isolation

| Round | Backup | Backup bytes | Validation | A writes during own backup | B writes during A backup |
| --- | --- | --- | --- | --- | --- |
| 0 | 224.396 | 42,795,008 | integrity ok, 100,066 events | 543 samples | 615 samples |
| 1 | 248.316 | 43,065,344 | integrity ok, 100,704 events | 727 samples | 761 samples |
| 2 | 228.039 | 43,429,888 | integrity ok, 101,554 events | 709 samples | 711 samples |

| Writer sample | n | p50 | p95 | max |
| --- | --- | --- | --- | --- |
| Workspace A during its own backup | 1,979 | 0.310 | 0.494 | 7.285 |
| Workspace B during A's backup | 2,087 | 0.303 | 0.424 | 7.919 |
| Workspace B while A is unavailable | 50 | 0.271 | 0.551 | 0.627 |

Marking A unavailable returned `internal error: workspace harness store is unavailable: ws-backup-a`; B kept
writing. This measures latency; it does **not** claim zero blocking. Report the
tail (max), not only p95.

### Task create / stream page / Job (branch storage paths)

| Path | n | p50 | p95 | max |
| --- | --- | --- | --- | --- |
| Task create | 200 | 0.246 | 0.276 | 1.820 |
| Task create, duplicate idempotency key | 50 | 0.277 | 0.359 | 0.407 |
| Task create, 64 KiB objective | 50 | 0.279 | 0.294 | 0.336 |
| Task create, 20 workspaces | 200 | 0.249 | 0.266 | 0.312 |
| Task create, 8 concurrent workers | 200 | 0.483 | 3.352 | 40.572 |
| Stream page, newest (100k events) | 50 | 4.795 | 5.094 | 5.569 |
| Stream page, oldest cursor | 50 | 14.752 | 15.321 | 16.136 |
| Job create | 200 | 0.415 | 0.481 | 0.855 |
| Job get | 200 | 0.054 | 0.092 | 0.129 |
| Job create, 64 KiB args | 50 | 6.077 | 6.987 | 8.489 |
| Stream page, 200 x 64 KiB payloads | 1 | 65.759 | — | — |

The oldest-cursor page is the pathological bounded-page case in this fixture
(14.752 ms p50 versus 4.795 ms for newest). It is still bounded at 200 rows; no full-ledger read was observed. The 8-worker Task-create p95 (3.352 ms) reflects SQLite write-lock contention, not a queue limit.

## 5. Sample distribution

The artifact records every raw sample and the sample count per metric. The
2026-11-05 final run:

| Group | Sample sets | Total samples |
| --- | --- | --- |
| backup_isolation | 4 | 4,119 |
| events | 9 | 270 |
| guard | 4 | 90 |
| rebuild_snapshot | 1 | 50 |
| startup | 8 | 2,222 |
| task_paths_branch | 10 | 1,250 |

## 6. Data sizes

| Fixture | Size |
| --- | --- |
| 1 workspace (empty) | 4,096 B DB + 498,552 B WAL |
| 1,000 workspaces (empty) | 4,096,000 B DB + 498,552,000 B WAL |
| 1k session events | 4,096 B DB + 1,685,112 B WAL |
| 100k session events | 43,393,024 B DB + 4,260,112 B WAL |
| 1M session events (96 B payloads) | 437,366,784 B DB + 4,260,112 B WAL |
| 110k effects + 110k commands | 53,166,080 B DB + 31,056,592 B WAL |

## 7. main/branch comparison status: INCOMPLETE

The baseline checked by the bench is `origin/main`
`295d2209a3d762883791d2e8a820056d72d4d0ff` (local `main` is
`5cf1947382a8339e2cb80ae06fa9a547df1e62fe`). Neither ref contains:

- `crates/db/src/harness_workspace.rs`
- `crates/db/src/harness_event.rs`
- `crates/db/src/runtime_task.rs`
- `crates/db/src/runtime_event.rs`
- `crates/db/src/device_job.rs`
- `apps/cloud/src/domains/agent/runtime/stream.rs`
- `apps/cloud/benches/harness.rs`

Therefore `main` has no durable workspace store, session-event replay, durable
Task, stream page or Device Job storage path to run against the same fixture.
The pre-existing Task route on `main` is an in-memory directive enqueue with
different semantics. The artifact records `main.status = "missing"`,
`missingEvidence = 1`, and `overall = "INCOMPLETE"`. No main p50/p95, memory or
file-size numbers are fabricated.

If a future `main` contains these modules, run the same command there (or wire
`HARNESS_BENCH_MAIN_BIN`/`HARNESS_BENCH_MAIN_WORKTREE`) and keep the raw
artifacts from both refs. Interleaving is only meaningful when both sides run
the same fixture and the same payload sizes.

## 8. Interpretation and known limits

- **Debug profile.** All timings are from `cargo test` (unoptimized). They are
  not release-build latency and not production p99. Run the same entry point
  with `--release` for release evidence.
- **Local machine.** One Apple M1 laptop. Filesystem cache and thermal state
  are not controlled.
- **Platform fixture.** Task/Job storage paths run on an in-memory platform
  database for isolation. Production platform and workspace stores are
  file-backed WAL; only the workspace-store measurements above exercise the
  file-backed WAL path.
- **RSS is approximate.** It does not include shared pages, allocator retention
  or SQLite page cache uniformly. The 1M preload contrast is directional
  evidence of history-proportional memory, not an exact resident-set model.
  The rebuild post-call RSS delta in the final run was +86,245,376 B;
  a prior identical run measured a different post-call delta. Neither is a
  peak.
- **Guard large batch.** The guard performs a decision lookup per actionable
  effect. 10k actionable effects cost ~535 ms p50 in this debug build. This is
  the measured worst case; it is not a new quota, and the recovery-matrix
  invariant is fail-closed.
- **Backup is `VACUUM INTO`.** It is the production pre-migration backup
  mechanism. The run shows overlap with writes and other-workspace progress,
  not zero blocking. In the final run the worst round took 248.316 ms and the
  same-workspace writer's max was 7.285 ms; an earlier identical run showed an
  835 ms round and a 164.5 ms same-workspace write. Report the tail, not only
  p95.
- **Stream page.** The measured query backs the SSE endpoint; transport,
  authentication, JSON serialization and client backpressure are excluded.
- **Rebuild progress** is not exposed by the production call. The artifact says
  so instead of reporting a synthetic progress value.
- **Old cursor** is the slowest bounded page in this fixture. It is still a
  200-row page; the run does not prove an index choice for every cursor value.
- **No fixed Task/media/event quotas are introduced.** Buckets are inputs.

## 9. Maintenance

Rerun the command after changing:

- `crates/db/migrations/20261103*.sql` or `20261105000003_harness_recovery_scan_indexes.sql`;
- workspace pool/connect options, WAL or busy-timeout settings;
- session-event or runtime-event page SQL;
- recovery-matrix guard SQL or indexes;
- projection snapshot/rebuild code;
- backup mechanism.

Compare raw artifacts from the same machine/profile and the same fixture
counts. Do not compare a release run with a debug run, or a 96-byte payload run
with a large-payload run, as if they were the same measurement.
