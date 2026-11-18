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
| `HARNESS_BENCH_SCALE=smoke` | Smaller real buckets: workspaces 1/10, events 1k/10k, guard 10k terminal + 1k active, backup 10k, fewer Task/Job iterations. The artifact records the exact scale config. |
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
| Workspace pool | maxConnections=10, minConnections=0, acquireTimeout=30 s, idleTimeout=600 s. Derived from `DatabaseConfig::default()`; `pool::workspace_pool_options` forces `min_connections=0`. |
| Platform fixture | `sqlite::memory:` (benchmark isolation); production platform config is file-backed |
| Binary | `tinyiothub-cloud` 0.4.6, debug test profile |
| Branch / commit | `e4-p2-declaration-closeout` / `5a9eee81f317ef244fa62e4d6d277164211dc96d` |
| Main ref | `origin/main` `295d2209a3d762883791d2e8a820056d72d4d0ff` and local `main` `5cf1947382a8339e2cb80ae06fa9a547df1e62fe` — both lack the Harness/Runtime storage modules (see section 7) |
| Run duration | 182,128.445 ms |
| Final artifact | `target/harness-perf/harness-perf-branch-1793871641.json` (generated 2026-11-05T09:40:41.702492+00:00) |

Fixture details:

- Full scale: workspaces 1/10/100/1000; session events 1k/100k/1M per
  workspace, 96-byte payloads, three composite indexes
  (`execution_mode,session_id/run_id/task_id,seq`). Smoke uses the smaller
  buckets listed above.
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

Events are inserted cumulatively into one dense session (`sess-big`; 1k → 100k
→ 1M). A separate sparse session (`sess-gap`) holds 100 old events plus one
newest event per bucket. Four queries are timed at 30 samples each:

- **newest**: `after = max(seq) - 100` on `sess-big`;
- **in-session long gap**: `after = 100` on `sess-big`. The cursor is far
  behind that same session's newest boundary, so the query traverses a
  session-ID index range with nearly the whole history ahead of it; it still
  returns the 256-row page cap;
- **sparse session after block**: `after = 100` on `sess-gap`. This is a
  different session-ID index range and is reported separately, not as an
  in-session gap;
- **after=0**: first page of `sess-big` (server cap is 256 rows).

The bounded-page memory sample reads the first page and records RSS delta. The
"unbounded preload contrast" reads the whole session through
`projection_boundary_events`; it is the closest measured analogue to the old
whole-history prefetch, not a main-binary sample.

### Snapshot, offline rebuild and failure recovery

The bench writes a projection snapshot whose body is a real canonical document
shaped from the first 1,000 events at the boundary (900 event samples in the
reported run, because the gap session interleaves sequence numbers), then reads
it 50 times. The body byte size is recorded in the artifact and results so the
read is not read as a tiny placeholder. The body keeps `lastSeq` at the
boundary; it is a size/shape sample, not a complete 1M-event projection.

The bench then rebuilds the canonical projection at the boundary through
`ProjectionInspector` (no snapshot present), and finally runs the same
comparison with a barrier that appends one event between the fixed-boundary
read and the post-rebuild high-watermark recheck. The expected failure result
is `StaleAppend` with no replacement snapshot. Rebuild progress is not exposed
by the production call.

### Backup and cross-workspace isolation

`VACUUM INTO` (the same SQLite mechanism used for pre-migration backups) writes
a workspace backup while writers append to the same workspace and to a second
workspace. A writer sample is attributed to the backup window only when the
append **started** inside `[backup start, backup end]`; a write that starts
before the window and finishes inside it is not attributed. Each backup is
opened read-only and checked with `PRAGMA integrity_check`. Finally workspace A
is marked unavailable and workspace B continues writing. The reported
`backupFileBytesFromLastRound` is the backup output; `harnessDbBytes`/
`harnessWalBytes` are the live store, separately labelled.

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
| 1 | 10.736 | 10.736 | 10.736 | 0.095 | 0.697 | 1.860 | 2 | 4,096 | 498,552 |
| 10 | 110.393 | 9.171 | 21.594 | 0.098 | 9.541 | 16.918 | 20 | 40,960 | 4,985,520 |
| 100 | 898.261 | 8.682 | 10.394 | 0.083 | 59.067 | 161.538 | 200 | 409,600 | 49,855,200 |
| 1,000 | 9144.192 | 8.781 | 10.783 | 0.137 | 799.049 | 2927.628 | 1,999 | 4,096,000 | 498,552,000 |

Per-workspace interrupted-run recovery at 1,000 workspaces: p50 0.782, p95 1.134, max 4.020. The interrupted-run and staged-journal recoveries are linear in workspace count in this workload.

The 1,000-workspace WAL total is ~498 KiB per store because each store's WAL
has not reached its 1000-page auto-checkpoint threshold. This is file state,
not a limit; it is not a claim that production leaves the same bytes on disk.
At sample time 1,999 physical connections were observed for 1,000 stores (one
store reported one connection); the per-store maximum was 2.

### Recovery-matrix guard

| Fixture | n | p50 | p95 | Notes |
| --- | --- | --- | --- | --- |
| no effects/commands | 50 | 0.099 | 0.201 | 0 effects, 0 commands |
| 100000 terminal effects + 100000 terminal commands | 20 | 0.104 | 0.139 | Narrow path; partial indexes selected |
| Same 100k terminal rows, `NOT INDEXED` control | 10 | 52.844 | 54.553 | Full-scan cost avoided by the partial index |
| 10000 prepared effects + 10000 requested commands | 10 | 543.493 | 564.688 | 110k total effects/commands |

The narrow query plan was `SCAN e USING INDEX
idx_effect_intents_recovery_scan` plus a correlated decision/task lookup, and
`SCAN commands USING INDEX idx_commands_recovery_scan`.

### Session-event replay (1k / 100k / 1M)

| Events | Newest p50/p95 (rows) | In-session long gap (after=100 on big session) | Sparse session after block | after=0 p50/p95 (rows) | Bounded page time / RSS delta | Unbounded preload | Preload time | Preload RSS delta | DB bytes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1,000 | 1.197 / 1.322 (100) | 2.808 / 2.858 (256) | 0.195 / 0.208 (1) | 2.814 / 2.850 (256) | 2.870 / +16,384 B | 1,000 rows | 10.741 | +0 B | 4,096 |
| 100,000 | 1.187 / 1.237 (100) | 2.806 / 2.891 (256) | 0.198 / 0.215 (2) | 2.791 / 2.846 (256) | 2.808 / +0 B | 100,000 rows | 1047.049 | +125,747,200 B | 43,393,024 |
| 1,000,000 | 1.193 / 1.255 (100) | 2.793 / 3.764 (256) | 0.272 / 1.021 (3) | 2.800 / 2.846 (256) | 2.872 / +0 B | 1,000,000 rows | 10752.656 | +282,427,392 B | 437,485,568 |

The in-session long-gap row queries the dense `sess-big` session with
`after=100`; the cursor is far behind the session's own newest boundary, and the
page still returns the 256-row cap. The sparse-session row queries a different
session that has 100 old rows plus a few newest rows; that is a separate
session-id index range, not a gap inside `sess-big`.

The bounded page stays at 256 rows and constant RSS in this workload. The
unbounded contrast grows with total history. RSS is sampled with `ps -o rss=`
and is approximate; allocator reuse can make a delta zero or negative.

### Snapshot and offline rebuild

| Metric | Result |
| --- | --- |
| Daily snapshot read (n=50) | p50 6.071, p95 6.435, max 6.816 |
| Daily snapshot body | canonical shape, 900 event samples, 269,869 bytes |
| Offline rebuild | 50821.935 ms at boundary seq 1,000,102; session has 1,000,000 events; global high-watermark 1,000,103; 1,000,000 evidence refs; `SnapshotMissing` |
| Rebuild RSS delta (post-call) | +253,804,544 B; not a peak. RSS is directional only. |
| DB / WAL bytes before and after rebuild | 437,485,568 / 4,260,112 (unchanged) |
| Stale append during rebuild | `StaleAppend`, `appendedDuringCompare=true`, 52187.405 ms |
| Replacement on stale append | suppressed (`snapshotRowsAfter=0`) |

The daily snapshot body is a real canonical document shaped from the first
1,000 events at the boundary (900 event samples after interleaving with the gap
session), not a tiny placeholder. Its byte size is recorded above. It is a
size/shape sample, not a complete 1M-event projection.

Rebuild progress is a single call in the production API; no progress callback
exists.

### Backup write latency and isolation

| Round | Backup | Backup bytes | Validation | A writes started in window | B writes started in window |
| --- | --- | --- | --- | --- | --- |
| 0 | 240.083 | 42,795,008 | integrity ok, 100,072 events | 579 samples | 750 samples |
| 1 | 223.431 | 43,098,112 | integrity ok, 100,777 events | 683 samples | 706 samples |
| 2 | 210.422 | 43,438,080 | integrity ok, 101,591 events | 666 samples | 671 samples |

| Writer sample | n | p50 | p95 | max |
| --- | --- | --- | --- | --- |
| Workspace A during its own backup | 1,928 | 0.304 | 0.464 | 12.102 |
| Workspace B during A's backup | 2,127 | 0.293 | 0.384 | 3.810 |
| Workspace B while A is unavailable | 50 | 0.228 | 0.312 | 0.487 |

Marking A unavailable returned `internal error: workspace harness store is unavailable: ws-backup-a`; B kept
writing. This measures latency; it does **not** claim zero blocking. Writer
samples count an append when it **started** inside the backup window; a write
that starts before the window and completes inside it is not attributed.
`backupFileBytesFromLastRound` (43,438,080 B) is the
backup output; `harnessDbBytes` / `harnessWalBytes` are the live store at the end.

### Task create / stream page / Job (branch storage paths)

| Path | n | p50 | p95 | max |
| --- | --- | --- | --- | --- |
| Task create | 200 | 0.251 | 0.277 | 1.899 |
| Task create, duplicate idempotency key | 50 | 0.276 | 0.299 | 0.394 |
| Task create, 64 KiB objective | 50 | 0.278 | 0.299 | 0.307 |
| Task create, 20 workspaces | 200 | 0.249 | 0.269 | 0.278 |
| Task create, concurrent workers | 200 | 0.486 | 1.923 | 37.229 |
| Stream page, newest (100k events) | 50 | 4.973 | 5.488 | 9.340 |
| Stream page, oldest cursor | 50 | 15.241 | 15.950 | 19.964 |
| Job create | 200 | 0.415 | 0.566 | 0.892 |
| Job get | 200 | 0.054 | 0.069 | 0.192 |
| Job create, 64 KiB args | 50 | 6.377 | 9.210 | 9.379 |
| Stream page, 200 x 64 KiB payloads | 1 | 67.104 | — | — |

The oldest-cursor page is the pathological bounded-page case in this fixture
(15.241 ms p50 versus 4.973 ms for newest). It is still bounded at 200 rows; no full-ledger read was observed. Concurrent Task-create p95 (1.923 ms) reflects SQLite write-lock contention, not a queue limit.

## 5. Sample distribution

The artifact records every raw sample and the sample count per metric. The
2026-11-05 final run:

| Group | Sample sets | Total samples |
| --- | --- | --- |
| backup_isolation | 4 | 4,108 |
| events | 12 | 360 |
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
| 1M session events (96 B payloads) | 437,485,568 B DB + 4,260,112 B WAL |
| 110k effects + 110k commands | 53,166,080 B DB + 31,056,592 B WAL |

## 7. main/branch comparison status: INCOMPLETE

The bench checks both the fetched baseline and the local ref independently:

| Ref | Commit | Subject | Harness/Runtime paths |
| --- | --- | --- | --- |
| `origin/main` | `295d2209a3d762883791d2e8a820056d72d4d0ff` | Merge pull request #100 from Grong/ai-consolidation | absent |
| `main` | `5cf1947382a8339e2cb80ae06fa9a547df1e62fe` | Merge pull request #97 from Grong/worktree-fix-dispositions-header | absent |

The required paths are absent on both refs:

- `crates/db/src/harness_workspace.rs`
- `crates/db/src/harness_event.rs`
- `crates/db/src/runtime_task.rs`
- `crates/db/src/runtime_event.rs`
- `crates/db/src/device_job.rs`
- `apps/cloud/src/domains/agent/runtime/stream.rs`
- `apps/cloud/benches/harness.rs`

Therefore neither baseline has a durable workspace store, session-event replay,
durable Task, stream page or Device Job storage path to run against the same
fixture. The pre-existing Task route on both refs is an in-memory directive
enqueue with different semantics. The artifact records per-ref presence,
`main.status = "missing"`, `missingEvidence = 1`, and `overall = "INCOMPLETE"`.
No main p50/p95, memory or file-size numbers are fabricated.

If a future baseline contains these modules, run the same command there (or
wire `HARNESS_BENCH_MAIN_BIN`/`HARNESS_BENCH_MAIN_WORKTREE`) and keep the raw
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
  The rebuild post-call RSS delta in the final run was +253,804,544 B;
  a prior identical run measured a different post-call delta. Neither is a
  peak.
- **Guard large batch.** The guard performs a decision lookup per actionable
  effect. 10k actionable effects cost ~543 ms p50 in this debug build. This is
  the measured worst case; it is not a new quota, and the recovery-matrix
  invariant is fail-closed.
- **Backup is `VACUUM INTO`.** It is the production pre-migration backup
  mechanism. The run shows overlap with writes and other-workspace progress,
  not zero blocking. In the final run the worst round took 240.083 ms and the
  same-workspace writer's max was 12.102 ms; an earlier identical run showed an
  835 ms round and a 164.5 ms same-workspace write. Report the tail, not only
  p95.
- **Snapshot body size matters.** The daily snapshot read now deserializes a
  269,869-byte canonical body (900 event samples). The 6.07 ms p50 is the
  representative read for that body; the earlier 0.1 ms placeholder body is
  superseded and must not be compared with it.
- **Rebuild boundary vs count.** The 1M rebuild is at boundary seq 1,000,102;
  the session contains exactly 1,000,000 events and the global high-watermark
  is 1,000,103. The earlier "1,000,102 events" label was a sequence number, not
  an event count.
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

## 10. Revision history

- 2026-11-05, fix round 1 (commit after `476d69e9`):
  - F1: added a true in-session long-gap query (`sess-big`, `after=100`, 256
    rows) and kept the sparse-session case separate with distinct labels.
  - F2: implemented `HARNESS_BENCH_SCALE=smoke` as real smaller buckets and
    recorded the exact scale config in the artifact.
  - F3: the daily snapshot body is now a canonical document shaped from the
    first 1,000 events; its byte size and event-sample count are recorded.
  - F4: rebuild results report boundary seq, session event count and global
    high-watermark separately.
  - F5: backup writer samples are labelled as "started in window"; backup file
    bytes and live harness DB/WAL bytes are reported separately.
  - F6: per-ref presence is recorded for `origin/main` and local `main`.
  - F7: removed the dead `fn main`; the brief's `cargo test --bench harness`
    entry point is unchanged.
  - F8: sample sets record `droppedSamples` and the run fails if any sample is
    dropped.
  - Residual: recorded the actual workspace-pool construction
    (`min_connections=0` from `pool::workspace_pool_options`).
