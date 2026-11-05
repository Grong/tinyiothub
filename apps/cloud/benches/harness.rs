//! Harness Core performance and scale evidence (Spec T13).
//!
//! This is a deterministic measurement entry point, not a production SLA and
//! not a capacity quota. It builds fixed fixtures, records every raw sample,
//! and prints/writes a JSON document with machine, SQLite, binary/config,
//! data-size, concurrency and sample-distribution metadata.
//!
//! Run:
//!   cargo test -p tinyiothub-cloud --bench harness -- --nocapture
//!
//! Optional environment:
//!   HARNESS_BENCH_SCALE=smoke   smaller buckets (workspaces 1/10, events
//!                               1k/10k, guard 10k terminal + 1k active,
//!                               backup 10k, fewer Task/Job iterations).
//!                               Metadata and missing-evidence reporting are
//!                               unchanged.
//!   HARNESS_BENCH_OUT=<dir>     artifact directory (default: target/harness-perf).
//!
//! The main/branch comparison is limited to paths that exist on both refs.
//! This bench checks the `main` ref for the durable Harness/Runtime storage
//! modules and records an explicit `missing` result (with the exact absent
//! files) instead of inventing numbers when they are absent.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use chrono::Utc;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{AssertSqlSafe, Row, SqlitePool};
use tinyiothub_cloud::domains::agent::workflows::{
    CompareBarrier, ProjectionInspector, projection_contract::canonical_state,
};
use tinyiothub_core::{
    agent_task::{AgentTask, TaskStatus, TaskWake, WakeDispatchState},
    device_job::{DeviceJob, DeviceJobState},
    harness::{ExecutionMode, HarnessEventInput},
};
use tinyiothub_storage::{Db, config::DatabaseConfig, harness_event::EventFilter, seed, test_helpers::test_pool};
use uuid::Uuid;

const SESSION_EVENT_BATCH: i64 = 100;

/// Bucket sizes are measurement inputs. `smoke` keeps the same measurement
/// shape but shrinks the buckets so the entry point can be exercised quickly;
/// the default is the full run described in the runbook.
#[derive(Debug, Clone)]
struct Scale {
    name: &'static str,
    workspace_counts: Vec<usize>,
    event_buckets: Vec<i64>,
    guard_terminal: i64,
    guard_active: i64,
    backup_events: i64,
    task_iterations: u64,
    task_dedup_iterations: u64,
    task_large_iterations: u64,
    concurrent_workers: usize,
    concurrent_per_worker: u64,
    multi_workspace_iterations: u64,
    page_iterations: usize,
    runtime_event_count: i64,
}

impl Scale {
    fn current() -> Self {
        match std::env::var("HARNESS_BENCH_SCALE").as_deref() {
            Ok("smoke") => Self {
                name: "smoke",
                workspace_counts: vec![1, 10],
                event_buckets: vec![1_000, 10_000],
                guard_terminal: 10_000,
                guard_active: 1_000,
                backup_events: 10_000,
                task_iterations: 50,
                task_dedup_iterations: 20,
                task_large_iterations: 10,
                concurrent_workers: 4,
                concurrent_per_worker: 10,
                multi_workspace_iterations: 5,
                page_iterations: 20,
                runtime_event_count: 10_000,
            },
            _ => Self {
                name: "full",
                workspace_counts: vec![1, 10, 100, 1_000],
                event_buckets: vec![1_000, 100_000, 1_000_000],
                guard_terminal: 100_000,
                guard_active: 10_000,
                backup_events: 100_000,
                task_iterations: 200,
                task_dedup_iterations: 50,
                task_large_iterations: 50,
                concurrent_workers: 8,
                concurrent_per_worker: 25,
                multi_workspace_iterations: 10,
                page_iterations: 50,
                runtime_event_count: 100_000,
            },
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "name": self.name,
            "workspaceCounts": self.workspace_counts,
            "eventBuckets": self.event_buckets,
            "guardTerminalRows": self.guard_terminal,
            "guardActiveRows": self.guard_active,
            "backupEventsPerWorkspace": self.backup_events,
            "taskIterations": self.task_iterations,
            "taskDedupIterations": self.task_dedup_iterations,
            "taskLargeIterations": self.task_large_iterations,
            "concurrentWorkers": self.concurrent_workers,
            "concurrentPerWorker": self.concurrent_per_worker,
            "multiWorkspaceIterations": self.multi_workspace_iterations,
            "pageIterations": self.page_iterations,
            "runtimeEventCount": self.runtime_event_count,
        })
    }
}

#[test]
fn harness_performance_evidence() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .expect("tokio runtime");
    let report = runtime.block_on(run_all());
    emit_report(&report);
}

// ---------------------------------------------------------------------------
// Report plumbing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct SampleSet {
    name: &'static str,
    unit: &'static str,
    samples: Vec<f64>,
    dropped: u64,
}

impl SampleSet {
    fn new(name: &'static str, unit: &'static str) -> Self {
        Self {
            name,
            unit,
            samples: Vec::new(),
            dropped: 0,
        }
    }

    fn push(&mut self, value: f64) {
        if value.is_finite() && value >= 0.0 {
            self.samples.push(value);
        } else {
            self.dropped += 1;
        }
    }

    fn to_json(&self) -> Value {
        let mut sorted = self.samples.clone();
        sorted.sort_by(f64::total_cmp);
        let n = sorted.len();
        let pct = |p: f64| -> Option<f64> {
            if n == 0 {
                return None;
            }
            let rank = ((p / 100.0) * n as f64).ceil().max(1.0) as usize;
            sorted.get(rank - 1).copied()
        };
        let mean = if n == 0 {
            None
        } else {
            Some(sorted.iter().sum::<f64>() / n as f64)
        };
        json!({
            "name": self.name,
            "unit": self.unit,
            "sampleCount": n,
            "min": sorted.first().copied(),
            "p50": pct(50.0),
            "p95": pct(95.0),
            "p99": pct(99.0),
            "max": sorted.last().copied(),
            "mean": mean,
            "droppedSamples": self.dropped,
            "rawSamples": self.samples,
        })
    }
}

struct Evidence {
    measurements: BTreeMap<String, Vec<Value>>,
    data_sizes: Vec<Value>,
    concurrency: Vec<Value>,
    missing: Vec<Value>,
    notes: Vec<String>,
}

impl Evidence {
    fn new() -> Self {
        Self {
            measurements: BTreeMap::new(),
            data_sizes: Vec::new(),
            concurrency: Vec::new(),
            missing: Vec::new(),
            notes: Vec::new(),
        }
    }

    fn add(&mut self, group: &str, value: Value) {
        self.measurements.entry(group.to_string()).or_default().push(value);
    }
}

// ---------------------------------------------------------------------------
// Environment and metadata
// ---------------------------------------------------------------------------

fn command_stdout(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn git_output(args: &[&str]) -> Option<String> {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let mut full = vec!["-C".to_string(), manifest.to_string()];
    full.extend(args.iter().map(|value| value.to_string()));
    let refs: Vec<&str> = full.iter().map(String::as_str).collect();
    command_stdout("git", &refs)
}

fn git_path_exists(rev: &str, path: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(env!("CARGO_MANIFEST_DIR"))
        .args(["cat-file", "-e", &format!("{rev}:{path}")])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn rss_bytes() -> Option<u64> {
    let pid = std::process::id().to_string();
    let out = Command::new("ps").args(["-o", "rss=", "-p", &pid]).output().ok()?;
    let kb: u64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
    Some(kb * 1024)
}

async fn sqlite_metadata(pool: &SqlitePool) -> Value {
    let sqlite_version: String = sqlx::query_scalar("SELECT sqlite_version()")
        .fetch_one(pool)
        .await
        .unwrap_or_default();
    let journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(pool)
        .await
        .unwrap_or_default();
    let synchronous: i64 = sqlx::query_scalar("PRAGMA synchronous")
        .fetch_one(pool)
        .await
        .unwrap_or(-1);
    let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(pool)
        .await
        .unwrap_or(-1);
    let busy_timeout: i64 = sqlx::query_scalar("PRAGMA busy_timeout")
        .fetch_one(pool)
        .await
        .unwrap_or(-1);
    let page_size: i64 = sqlx::query_scalar("PRAGMA page_size")
        .fetch_one(pool)
        .await
        .unwrap_or(-1);
    let cache_size: i64 = sqlx::query_scalar("PRAGMA cache_size")
        .fetch_one(pool)
        .await
        .unwrap_or(i64::MAX);
    let wal_autocheckpoint: i64 = sqlx::query_scalar("PRAGMA wal_autocheckpoint")
        .fetch_one(pool)
        .await
        .unwrap_or(-1);
    json!({
        "sqliteVersion": sqlite_version,
        "journalMode": journal_mode,
        "synchronous": synchronous,
        "foreignKeys": foreign_keys,
        "busyTimeoutMs": busy_timeout,
        "pageSize": page_size,
        "cacheSizePragma": cache_size,
        "walAutocheckpointPages": wal_autocheckpoint,
    })
}

fn machine_metadata() -> Value {
    let cpu = command_stdout("sysctl", &["-n", "machdep.cpu.brand_string"])
        .or_else(|| command_stdout("sh", &["-c", "grep -m1 'model name' /proc/cpuinfo | cut -d: -f2-"]))
        .unwrap_or_else(|| "unknown".to_string());
    let memory = command_stdout("sysctl", &["-n", "hw.memsize"])
        .or_else(|| command_stdout("sh", &["-c", "awk '/MemTotal/ {print $2*1024}' /proc/meminfo"]))
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    json!({
        "platform": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "uname": command_stdout("uname", &["-a"]),
        "osVersion": command_stdout("sw_vers", &["-productVersion"])
            .or_else(|| command_stdout("sh", &["-c", "cat /etc/os-release 2>/dev/null | tr '\\n' ' '"])),
        "cpu": cpu.trim(),
        "memoryBytes": memory,
        "logicalCpus": std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0),
        "rustc": command_stdout("rustc", &["--version"]),
        "processId": std::process::id(),
    })
}

fn binary_metadata() -> Value {
    let dirty = git_output(&["status", "--porcelain"])
        .map(|value| !value.is_empty())
        .unwrap_or(false);
    json!({
        "package": env!("CARGO_PKG_NAME"),
        "version": env!("CARGO_PKG_VERSION"),
        "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
        "branch": git_output(&["rev-parse", "--abbrev-ref", "HEAD"]),
        "commit": git_output(&["rev-parse", "HEAD"]),
        "commitSubject": git_output(&["log", "-1", "--format=%s"]),
        "dirtyWorktree": dirty,
        "target": format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS),
    })
}

fn ref_evidence(ref_name: &str) -> Value {
    let required = [
        "crates/db/src/harness_workspace.rs",
        "crates/db/src/harness_event.rs",
        "crates/db/src/runtime_task.rs",
        "crates/db/src/runtime_event.rs",
        "crates/db/src/device_job.rs",
        "apps/cloud/src/domains/agent/runtime/stream.rs",
        "apps/cloud/benches/harness.rs",
    ];
    let mut present = Vec::new();
    let mut absent = Vec::new();
    for path in required {
        if git_path_exists(ref_name, path) {
            present.push(path);
        } else {
            absent.push(path);
        }
    }
    json!({
        "name": ref_name,
        "commit": git_output(&["log", "-1", "--format=%H", ref_name]),
        "subject": git_output(&["log", "-1", "--format=%s", ref_name]),
        "requiredPathsPresent": present,
        "requiredPathsAbsent": absent,
        "complete": absent.is_empty(),
    })
}

fn main_ref_evidence() -> Value {
    // Check both the fetched baseline and the local ref independently. A local
    // `main` can be stale; the report must show per-ref presence rather than
    // claiming one ref's result for another.
    let has_origin = git_output(&["rev-parse", "--verify", "origin/main"]).is_some();
    let chosen_name = if has_origin { "origin/main" } else { "main" };
    let mut refs = Vec::new();
    if has_origin {
        refs.push(ref_evidence("origin/main"));
    }
    refs.push(ref_evidence("main"));
    let chosen = refs
        .iter()
        .find(|entry| entry["name"].as_str() == Some(chosen_name))
        .expect("chosen baseline ref");
    let required_paths_present = chosen["requiredPathsPresent"].clone();
    let required_paths_absent = chosen["requiredPathsAbsent"].clone();
    let complete = chosen["complete"].as_bool().unwrap_or(false);
    let local_main_commit = refs
        .iter()
        .find(|entry| entry["name"].as_str() == Some("main"))
        .and_then(|entry| entry["commit"].as_str().map(str::to_string));
    let main_worktree = std::env::var("HARNESS_BENCH_MAIN_WORKTREE").ok();
    let main_binary = std::env::var("HARNESS_BENCH_MAIN_BIN").ok();
    let main_binary_meta = main_binary.as_ref().and_then(|path| {
        let metadata = std::fs::metadata(path).ok()?;
        Some(json!({
            "path": path,
            "bytes": metadata.len(),
            "modified": metadata.modified().ok().and_then(|time| time.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()),
        }))
    });
    let status = if complete {
        "available_but_not_executed"
    } else {
        "missing"
    };
    let reason = if complete {
        "This process is the branch binary. Run the same command in the main worktree (or pass a prebuilt main binary once the bench supports subprocess fixtures); no main samples were recorded here."
    } else {
        "The baseline ref does not contain the durable Harness workspace store, session-event replay, Runtime Task or Device Job storage modules. The pre-existing Task route on main is an in-memory directive enqueue (no durable Task/stream/Job fixture), so an interleaved main/branch p50/p95 for those paths cannot be produced without inventing a different workload."
    };
    json!({
        "role": "main",
        "status": status,
        "refName": chosen_name,
        "ref": chosen["commit"].clone(),
        "commit": chosen["commit"].clone(),
        "subject": chosen["subject"].clone(),
        "localMainCommit": local_main_commit,
        "refs": refs,
        "requiredPathsPresent": required_paths_present,
        "requiredPathsAbsent": required_paths_absent,
        "mainWorktree": main_worktree,
        "mainBinary": main_binary_meta,
        "reason": reason,
        "numbers": null,
    })
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn temp_root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("tinyiothub-harness-perf-{name}-{}", Uuid::new_v4()))
}

fn workspace_db_path(root: &Path, workspace_id: &str) -> PathBuf {
    root.join("data/agents").join(workspace_id).join("harness/harness.db")
}

fn file_sizes(path: &Path) -> Value {
    let size = |suffix: &str| -> u64 {
        let mut value = path.as_os_str().to_os_string();
        value.push(suffix);
        std::fs::metadata(PathBuf::from(value)).map(|m| m.len()).unwrap_or(0)
    };
    json!({
        "dbBytes": std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
        "walBytes": size("-wal"),
        "shmBytes": size("-shm"),
    })
}

async fn platform_db(root: &Path, workspace_ids: &[String]) -> Db {
    let pool = test_pool().await;
    let db = Db::new(pool).with_workspace_store_root(root.join("data/agents"));
    seed::seed_system(&db).await.expect("seed platform registry");
    for workspace_id in workspace_ids {
        sqlx::query(
            "INSERT OR IGNORE INTO workspaces (id, name, tenant_id, created_at, updated_at) VALUES (?, ?, 'tenant-default-001', 'now', 'now')",
        )
        .bind(workspace_id)
        .bind(format!("perf {workspace_id}"))
        .execute(db.pool())
        .await
        .expect("insert workspace");
    }
    db
}

async fn close_and_remove(db: Db, workspace_ids: &[String], root: &Path) {
    for workspace_id in workspace_ids {
        let _ = db.close_workspace_store(workspace_id).await;
    }
    db.pool().close().await;
    let _ = tokio::fs::remove_dir_all(root).await;
}

async fn session_max_seq(pool: &SqlitePool, session_id: &str) -> i64 {
    sqlx::query_scalar("SELECT COALESCE(MAX(seq), 0) FROM session_events WHERE session_id = ?")
        .bind(session_id)
        .fetch_one(pool)
        .await
        .expect("session max seq")
}

async fn total_seq(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COALESCE(MAX(seq), 0) FROM session_events")
        .fetch_one(pool)
        .await
        .expect("session total seq")
}

async fn insert_session_events_batch(
    pool: &SqlitePool,
    session_id: &str,
    task_id: &str,
    count: i64,
    payload_bytes: usize,
) {
    let mut remaining = count;
    let mut offset: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM session_events WHERE session_id = ?")
        .bind(session_id)
        .fetch_one(pool)
        .await
        .expect("existing session event count");
    let padding = "x".repeat(payload_bytes.saturating_sub(32));
    while remaining > 0 {
        let batch = remaining.min(SESSION_EVENT_BATCH);
        let placeholders = (0..batch)
            .map(|_| "(?, ?, 'production', 'run-bench', ?, 'bench.sample', ?, NULL, 1, ?, ?)")
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "INSERT INTO session_events (event_id, session_id, execution_mode, run_id, task_id, type, occurred_at, cause_id, schema_v, payload_digest, payload_json) VALUES {placeholders}"
        );
        let mut query = sqlx::query(AssertSqlSafe(sql));
        let now = Utc::now().to_rfc3339();
        for i in 0..batch {
            let index = offset + i;
            let payload = json!({"text": format!("event-{index}-{padding}")}).to_string();
            let digest = format!("{:064x}", index as u128);
            query = query
                .bind(format!("evt-{session_id}-{index}"))
                .bind(session_id)
                .bind(task_id)
                .bind(&now)
                .bind(digest)
                .bind(payload);
        }
        query.execute(pool).await.expect("bulk insert session events");
        remaining -= batch;
        offset += batch;
    }
}

async fn insert_runtime_events(pool: &SqlitePool, workspace_id: &str, task_id: &str, count: i64, payload_bytes: usize) {
    let mut remaining = count;
    let mut offset = 0i64;
    let padding = "y".repeat(payload_bytes.saturating_sub(64));
    while remaining > 0 {
        let batch = remaining.min(200);
        let mut tx = pool.begin().await.expect("runtime event tx");
        for i in 0..batch {
            let index = offset + i;
            let payload = json!({
                "v": 1,
                "eventId": format!("rtevt-{task_id}-{index}"),
                "eventType": "bench.sample",
                "occurredAt": Utc::now().to_rfc3339(),
                "workspaceId": workspace_id,
                "taskId": task_id,
                "runId": null,
                "toolCallId": null,
                "jobId": null,
                "approvalId": null,
                "deviceId": null,
                "sourceId": null,
                "sourceSeq": null,
                "evidenceId": null,
                "data": {"payload": padding},
            })
            .to_string();
            sqlx::query(
                "INSERT INTO hub_ledger (workspace_id, ts, kind, thing_id, payload) VALUES (?, ?, 'runtime.v1', NULL, ?)",
            )
            .bind(workspace_id)
            .bind(Utc::now().to_rfc3339())
            .bind(payload)
            .execute(&mut *tx)
            .await
            .expect("insert runtime event");
        }
        tx.commit().await.expect("commit runtime events");
        remaining -= batch;
        offset += batch;
    }
}

async fn append_bench_event(db: &Db, workspace_id: &str, sequence: u64, payload_bytes: usize) -> Result<(), String> {
    let payload = json!({"text": format!("live-{sequence}-{}", "z".repeat(payload_bytes.saturating_sub(24)))});
    let payload_json = payload.to_string();
    let input = HarnessEventInput {
        event_id: format!("live:{workspace_id}:{sequence}"),
        workspace_id: workspace_id.to_string(),
        session_id: "live-session".to_string(),
        execution_mode: ExecutionMode::Production,
        run_id: Some("run-live".to_string()),
        task_id: Some("task-live".to_string()),
        event_type: "bench.live".to_string(),
        occurred_at: Utc::now(),
        cause_id: None,
        schema_v: 1,
        payload_digest: format!("{:x}", Sha256::digest(payload_json.as_bytes())),
        payload_json: payload,
    };
    db.append_session_event(&input)
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

// ---------------------------------------------------------------------------
// Startup / migration / recovery scales (workspace 1/10/100/1000)
// ---------------------------------------------------------------------------

async fn measure_startup(evidence: &mut Evidence, scale: &Scale) {
    for &workspace_count in &scale.workspace_counts {
        let root = temp_root(&format!("startup-{workspace_count}"));
        let ids: Vec<String> = (0..workspace_count).map(|index| format!("ws-{index:04}")).collect();
        let db = platform_db(&root, &ids).await;

        let mut open_samples = SampleSet::new("workspace_open_migrate", "ms");
        let mut open_total = 0.0;
        for workspace_id in &ids {
            let started = Instant::now();
            db.create_workspace_store(workspace_id)
                .await
                .expect("create workspace store");
            let elapsed = started.elapsed().as_secs_f64() * 1000.0;
            open_total += elapsed;
            open_samples.push(elapsed);
        }
        let mut record = json!({
            "workspaceCount": workspace_count,
            "openMigrateTotalMs": open_total,
            "openMigrate": open_samples.to_json(),
        });

        // Startup journal scan with no pending restore journal.
        let started = Instant::now();
        let recovered = db.recover_restore_journals().await.expect("empty journal scan");
        record["emptyJournalScanMs"] = json!(started.elapsed().as_secs_f64() * 1000.0);
        record["emptyJournalScanRows"] = json!(recovered.len());

        // Interrupted-run recovery: one running run per workspace.
        for workspace_id in &ids {
            let pool = db
                .workspace_pool_for_testing(workspace_id)
                .await
                .expect("workspace pool");
            sqlx::query(
                "INSERT OR REPLACE INTO agent_tasks (id, state, state_version, execution_mode) VALUES (?, 'running', 0, 'production')",
            )
            .bind(format!("task-{workspace_id}"))
            .execute(&pool)
            .await
            .expect("insert running task");
            sqlx::query(
                "INSERT OR REPLACE INTO agent_runs (id, task_id, state, execution_mode) VALUES (?, ?, 'running', 'production')",
            )
            .bind(format!("run-{workspace_id}"))
            .bind(format!("task-{workspace_id}"))
            .execute(&pool)
            .await
            .expect("insert running run");
        }
        let mut run_samples = SampleSet::new("interrupted_run_recovery_per_workspace", "ms");
        let started = Instant::now();
        let mut recovered_runs = 0usize;
        for workspace_id in &ids {
            let started_ws = Instant::now();
            let events = db
                .recover_interrupted_harness_runs(workspace_id, Utc::now())
                .await
                .expect("recover interrupted runs");
            run_samples.push(started_ws.elapsed().as_secs_f64() * 1000.0);
            recovered_runs += events.len();
        }
        record["interruptedRunRecoveryTotalMs"] = json!(started.elapsed().as_secs_f64() * 1000.0);
        record["interruptedRunRecovery"] = run_samples.to_json();
        record["interruptedRunsRecovered"] = json!(recovered_runs);

        // Restore-journal recovery for every workspace. A `staged` journal with
        // an intact live generation classifies as KeptLive and validates each
        // generation with SQLite integrity_check.
        for workspace_id in &ids {
            let base = root.join("data/agents").join(workspace_id);
            sqlx::query(
                "INSERT OR REPLACE INTO harness_restore_journal
                    (workspace_id, epoch, phase, backup_path, rollback_path, restore_path, updated_at)
                 VALUES (?, 1, 'staged', ?, ?, ?, '2026-11-05T00:00:00Z')",
            )
            .bind(workspace_id)
            .bind(base.join("harness/backups/unused.db").to_string_lossy().to_string())
            .bind(base.join("harness.rollback").to_string_lossy().to_string())
            .bind(base.join("harness.restore").to_string_lossy().to_string())
            .execute(db.pool())
            .await
            .expect("insert restore journal");
        }
        let started = Instant::now();
        let outcomes = db.recover_restore_journals().await.expect("journal recovery");
        record["journalRecoveryTotalMs"] = json!(started.elapsed().as_secs_f64() * 1000.0);
        record["journalRecoveryRows"] = json!(outcomes.len());
        record["journalRecoveryKeptLive"] = json!(
            outcomes
                .iter()
                .filter(|(_, outcome)| format!("{outcome:?}") == "KeptLive")
                .count()
        );

        // Idle pools: physical connections held after opening every store.
        tokio::time::sleep(Duration::from_millis(100)).await;
        let mut physical_connections = 0u32;
        let mut idle_connections = 0usize;
        let mut pool_sizes = Vec::new();
        for workspace_id in &ids {
            let pool = db
                .workspace_pool_for_testing(workspace_id)
                .await
                .expect("workspace pool");
            physical_connections += pool.size();
            idle_connections += pool.num_idle();
            pool_sizes.push(pool.size());
        }
        record["idlePoolPhysicalConnections"] = json!(physical_connections);
        record["idlePoolIdleConnections"] = json!(idle_connections);
        record["idlePoolExpectedStores"] = json!(workspace_count);
        record["idlePoolSizePerWorkspaceMax"] = json!(pool_sizes.iter().copied().max().unwrap_or(0));

        // Data sizes and event counts for this scale.
        let mut db_bytes = 0u64;
        let mut wal_bytes = 0u64;
        for workspace_id in &ids {
            let sizes = file_sizes(&workspace_db_path(&root, workspace_id));
            db_bytes += sizes["dbBytes"].as_u64().unwrap_or(0);
            wal_bytes += sizes["walBytes"].as_u64().unwrap_or(0);
        }
        record["storeBytes"] = json!({"dbBytes": db_bytes, "walBytes": wal_bytes});
        evidence.data_sizes.push(json!({
            "fixture": "startup",
            "workspaceCount": workspace_count,
            "dbBytes": db_bytes,
            "walBytes": wal_bytes,
        }));
        evidence.add("startup", record);

        close_and_remove(db, &ids, &root).await;
    }
}

// ---------------------------------------------------------------------------
// Recovery-matrix guard: empty, narrow partial-index, full scan
// ---------------------------------------------------------------------------

async fn insert_effects(pool: &SqlitePool, workspace_id: &str, start: i64, count: i64, state: &str) {
    let mut remaining = count;
    let mut offset = start;
    let sql = "INSERT INTO effect_intents
        (effect_id, session_id, task_id, run_id, tool_call_id, tool_name, request_json, request_digest,
         idempotency_key, external_ref, state, recovery_from, state_version, result_json, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, 'bench.tool', '{}', 'digest', ?, NULL, ?, NULL, 1, NULL, '2026-11-05T00:00:00Z', '2026-11-05T00:00:00Z')";
    let mut tx = pool.begin().await.expect("effect tx");
    while remaining > 0 {
        let index = offset;
        sqlx::query(sql)
            .bind(format!("{workspace_id}-effect-{index}"))
            .bind(format!("session-{workspace_id}"))
            .bind(format!("task-{workspace_id}"))
            .bind(format!("run-{workspace_id}"))
            .bind(format!("call-{workspace_id}-{index}"))
            .bind(format!("idem-{workspace_id}-{index}"))
            .bind(state)
            .execute(&mut *tx)
            .await
            .expect("insert effect");
        offset += 1;
        remaining -= 1;
    }
    tx.commit().await.expect("commit effects");
}

async fn insert_commands(pool: &SqlitePool, workspace_id: &str, start: i64, count: i64, state: &str) {
    let sql = "INSERT INTO commands
        (command_id, kind, target_task_id, target_run_id, actor, payload_json, created_at, expires_at, state)
        VALUES (?, 'interrupt', ?, ?, 'bench', '{}', '2026-11-05T00:00:00Z', NULL, ?)";
    let mut tx = pool.begin().await.expect("command tx");
    for index in start..start + count {
        sqlx::query(sql)
            .bind(format!("{workspace_id}-command-{index}"))
            .bind(format!("task-{workspace_id}"))
            .bind(format!("run-{workspace_id}"))
            .bind(state)
            .execute(&mut *tx)
            .await
            .expect("insert command");
    }
    tx.commit().await.expect("commit commands");
}

async fn query_plan(pool: &SqlitePool, sql: &str) -> Vec<String> {
    sqlx::query(AssertSqlSafe(sql))
        .fetch_all(pool)
        .await
        .expect("query plan")
        .into_iter()
        .map(|row| row.get::<String, _>("detail"))
        .collect()
}

async fn measure_guard(evidence: &mut Evidence, scale: &Scale) {
    let root = temp_root("guard");
    let workspace_id = "ws-guard".to_string();
    let db = platform_db(&root, std::slice::from_ref(&workspace_id)).await;
    db.create_workspace_store(&workspace_id)
        .await
        .expect("create guard store");
    let pool = db.workspace_pool_for_testing(&workspace_id).await.expect("guard pool");

    let mut empty = SampleSet::new("guard_empty", "ms");
    for _ in 0..50 {
        let started = Instant::now();
        let issue = db
            .find_invalid_recovery_state(&workspace_id)
            .await
            .expect("guard empty");
        assert!(
            issue.is_none(),
            "empty workspace has no invalid recovery state: {issue:?}"
        );
        empty.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    evidence.add(
        "guard",
        json!({"fixture": "no effects/commands", "workspaceCount": 1, "effectCount": 0, "commandCount": 0, "latency": empty.to_json()}),
    );

    // Narrow: terminal rows only. The partial index should keep the guard
    // cost independent of terminal history.
    let terminal = scale.guard_terminal;
    insert_effects(&pool, &workspace_id, 0, terminal, "completed").await;
    insert_commands(&pool, &workspace_id, 0, terminal, "consumed").await;
    let mut narrow = SampleSet::new("guard_narrow_terminal_only", "ms");
    for _ in 0..20 {
        let started = Instant::now();
        let issue = db
            .find_invalid_recovery_state(&workspace_id)
            .await
            .expect("guard narrow");
        assert!(issue.is_none(), "terminal rows are not actionable: {issue:?}");
        narrow.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    let narrow_effect_plan = query_plan(
        &pool,
        "EXPLAIN QUERY PLAN SELECT e.effect_id, e.session_id, e.run_id, e.task_id, e.state, e.recovery_from,
                COALESCE((SELECT task.execution_mode FROM agent_tasks task WHERE task.id = e.task_id), 'production')
         FROM effect_intents e
         WHERE e.state NOT IN ('completed', 'failed', 'not_applied', 'cancelled', 'cancelled_unknown')
         ORDER BY e.created_at, e.effect_id",
    )
    .await;
    let narrow_command_plan = query_plan(
        &pool,
        "EXPLAIN QUERY PLAN SELECT seq, command_id, kind, state, target_task_id, target_run_id
         FROM commands
         WHERE state NOT IN ('consumed', 'rejected', 'superseded', 'expired')
         ORDER BY seq",
    )
    .await;

    // Full-scan control: the same predicates with indexes disabled. This is
    // the cost the partial index avoids, not the guard's normal path.
    let mut full_scan = SampleSet::new("guard_full_scan_100k_terminal", "ms");
    for _ in 0..10 {
        let started = Instant::now();
        let _: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM effect_intents NOT INDEXED
             WHERE state NOT IN ('completed', 'failed', 'not_applied', 'cancelled', 'cancelled_unknown')",
        )
        .fetch_one(&pool)
        .await
        .expect("full effect scan");
        let _: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM commands NOT INDEXED
             WHERE state NOT IN ('consumed', 'rejected', 'superseded', 'expired')",
        )
        .fetch_one(&pool)
        .await
        .expect("full command scan");
        full_scan.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    evidence.add(
        "guard",
        json!({
            "fixture": format!("{terminal} terminal effects + {terminal} terminal commands"),
            "effectCount": terminal,
            "commandCount": terminal,
            "actionableEffectCount": 0,
            "actionableCommandCount": 0,
            "latency": narrow.to_json(),
            "effectQueryPlan": narrow_effect_plan,
            "commandQueryPlan": narrow_command_plan,
            "usesPartialIndex": narrow_effect_plan.iter().any(|line| line.contains("idx_effect_intents_recovery_scan"))
                && narrow_command_plan.iter().any(|line| line.contains("idx_commands_recovery_scan")),
            "fullScanControl": full_scan.to_json(),
        }),
    );

    // Large actionable batch. This exercises the per-effect decision lookup and
    // is the worst-case guard shape measured here.
    let active = scale.guard_active;
    insert_effects(&pool, &workspace_id, terminal, active, "prepared").await;
    insert_commands(&pool, &workspace_id, terminal, active, "requested").await;
    let mut large = SampleSet::new("guard_large_active", "ms");
    for _ in 0..10 {
        let started = Instant::now();
        let issue = db
            .find_invalid_recovery_state(&workspace_id)
            .await
            .expect("guard large");
        assert!(
            issue.is_none(),
            "prepared/requested rows are valid without a decision: {issue:?}"
        );
        large.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    evidence.add(
        "guard",
        json!({
            "fixture": format!("{active} prepared effects + {active} requested commands"),
            "effectCount": terminal + active,
            "commandCount": terminal + active,
            "actionableEffectCount": active,
            "actionableCommandCount": active,
            "latency": large.to_json(),
        }),
    );

    evidence.data_sizes.push(json!({
        "fixture": "guard",
        "workspaceCount": 1,
        "dbBytes": file_sizes(&workspace_db_path(&root, &workspace_id))["dbBytes"],
        "walBytes": file_sizes(&workspace_db_path(&root, &workspace_id))["walBytes"],
        "effectCount": terminal + active,
        "commandCount": terminal + active,
    }));

    close_and_remove(db, &[workspace_id], &root).await;
}

// ---------------------------------------------------------------------------
// Session-event replay: newest / long gap / after=0 at 1k/100k/1M
// ---------------------------------------------------------------------------

struct EventFixture {
    db: Db,
    root: PathBuf,
    workspace_id: String,
    pool: SqlitePool,
    big_session: String,
    gap_session: String,
    task_id: String,
}

async fn event_fixture() -> EventFixture {
    let root = temp_root("events");
    let workspace_id = "ws-events".to_string();
    let db = platform_db(&root, std::slice::from_ref(&workspace_id)).await;
    db.create_workspace_store(&workspace_id)
        .await
        .expect("create event store");
    let pool = db.workspace_pool_for_testing(&workspace_id).await.expect("event pool");
    EventFixture {
        db,
        root,
        workspace_id,
        pool,
        big_session: "sess-big".to_string(),
        gap_session: "sess-gap".to_string(),
        task_id: "task-events".to_string(),
    }
}

async fn event_page_latency(
    fixture: &EventFixture,
    session_id: &str,
    after: i64,
    iterations: usize,
) -> (SampleSet, usize) {
    let mut samples = SampleSet::new("session_event_page", "ms");
    let mut returned = 0usize;
    for _ in 0..iterations {
        let started = Instant::now();
        let page = fixture
            .db
            .session_event_page(
                &fixture.workspace_id,
                &EventFilter {
                    session_id: Some(session_id.to_string()),
                    run_id: None,
                    task_id: Some(fixture.task_id.clone()),
                    execution_mode: ExecutionMode::Production,
                },
                after,
                256,
            )
            .await
            .expect("session event page");
        samples.push(started.elapsed().as_secs_f64() * 1000.0);
        returned = page.events.len();
    }
    (samples, returned)
}

async fn measure_events(evidence: &mut Evidence, scale: &Scale) -> EventFixture {
    let fixture = event_fixture().await;

    // A gap session: 100 old events plus one newest event. Querying after the
    // old block must jump over the whole big-session history through the
    // composite index.
    insert_session_events_batch(&fixture.pool, &fixture.gap_session, &fixture.task_id, 100, 96).await;

    let mut inserted = 0i64;
    for &target in &scale.event_buckets {
        let count = target - inserted;
        if count > 0 {
            insert_session_events_batch(&fixture.pool, &fixture.big_session, &fixture.task_id, count, 96).await;
            inserted = target;
        }
        // One newest gap event per bucket so the jump target is always after
        // the 100-event block.
        insert_session_events_batch(&fixture.pool, &fixture.gap_session, &fixture.task_id, 1, 96).await;

        let total = total_seq(&fixture.pool).await;
        let big_max = session_max_seq(&fixture.pool, &fixture.big_session).await;
        let gap_max = session_max_seq(&fixture.pool, &fixture.gap_session).await;
        let big_session_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM session_events WHERE session_id = ?")
            .bind(&fixture.big_session)
            .fetch_one(&fixture.pool)
            .await
            .expect("big session event count");

        let (newest, newest_rows) = event_page_latency(&fixture, &fixture.big_session, big_max - 100, 30).await;
        // True in-session long-gap case: a cursor 100 events into the densest
        // session, with the rest of that same session's history ahead of it.
        let (in_session_long_gap, in_session_gap_rows) =
            event_page_latency(&fixture, &fixture.big_session, 100, 30).await;
        // Sparse session: 100 old rows then one newest row. This is a separate
        // session-ID index range, not a gap inside the big session.
        let (sparse_after_block, sparse_rows) = event_page_latency(&fixture, &fixture.gap_session, 100, 30).await;
        let (after_zero, zero_rows) = event_page_latency(&fixture, &fixture.big_session, 0, 30).await;

        let rss_before = rss_bytes();
        let started = Instant::now();
        let page = fixture
            .db
            .session_event_page(
                &fixture.workspace_id,
                &EventFilter {
                    session_id: Some(fixture.big_session.clone()),
                    run_id: None,
                    task_id: Some(fixture.task_id.clone()),
                    execution_mode: ExecutionMode::Production,
                },
                0,
                256,
            )
            .await
            .expect("bounded page memory sample");
        let bounded_duration = started.elapsed().as_secs_f64() * 1000.0;
        let rss_after_bounded = rss_bytes();
        let bounded_rows = page.events.len();
        drop(page);

        // Old preload behavior contrast: one unbounded read of the whole
        // session. This is the shape the bounded page replaces; it is not a
        // main-binary sample.
        let rss_before_preload = rss_bytes();
        let started = Instant::now();
        let preload = fixture
            .db
            .projection_boundary_events(&fixture.workspace_id, &fixture.big_session, big_max)
            .await
            .expect("preload boundary");
        let preload_duration = started.elapsed().as_secs_f64() * 1000.0;
        let rss_after_preload = rss_bytes();
        let preload_rows = preload.events.len();
        drop(preload);

        let sizes = file_sizes(&workspace_db_path(&fixture.root, &fixture.workspace_id));
        evidence.add(
            "events",
            json!({
                "eventCount": target,
                "globalHighWatermark": total,
                "bigSessionBoundarySeq": big_max,
                "bigSessionEventCount": big_session_count,
                "gapSessionBoundarySeq": gap_max,
                "payloadBytesApprox": 96,
                "newestCursor": newest.to_json(),
                "newestRows": newest_rows,
                "inSessionLongGap": in_session_long_gap.to_json(),
                "inSessionLongGapRows": in_session_gap_rows,
                "inSessionLongGapAfter": 100,
                "sparseSessionAfterBlock": sparse_after_block.to_json(),
                "sparseSessionAfterBlockRows": sparse_rows,
                "afterZero": after_zero.to_json(),
                "afterZeroRows": zero_rows,
                "boundedPageMemory": {
                    "durationMs": bounded_duration,
                    "rows": bounded_rows,
                    "rssBeforeBytes": rss_before,
                    "rssAfterBytes": rss_after_bounded,
                    "rssDeltaBytes": rss_before.zip(rss_after_bounded).map(|(a, b)| b as i64 - a as i64),
                },
                "unboundedPreloadContrast": {
                    "durationMs": preload_duration,
                    "rows": preload_rows,
                    "rssBeforeBytes": rss_before_preload,
                    "rssAfterBytes": rss_after_preload,
                    "rssDeltaBytes": rss_before_preload.zip(rss_after_preload).map(|(a, b)| b as i64 - a as i64),
                },
                "dbBytes": sizes["dbBytes"],
                "walBytes": sizes["walBytes"],
            }),
        );
        evidence.data_sizes.push(json!({
            "fixture": "events",
            "eventCount": target,
            "dbBytes": sizes["dbBytes"],
            "walBytes": sizes["walBytes"],
        }));
    }
    fixture
}

// ---------------------------------------------------------------------------
// Snapshot read, offline rebuild, stale-append failure recovery
// ---------------------------------------------------------------------------

async fn measure_snapshot_rebuild(evidence: &mut Evidence, fixture: &EventFixture) {
    let max_seq = session_max_seq(&fixture.pool, &fixture.big_session).await;
    let session_event_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM session_events WHERE session_id = ?")
        .bind(&fixture.big_session)
        .fetch_one(&fixture.pool)
        .await
        .expect("session event count");
    let global_high_watermark = total_seq(&fixture.pool).await;
    let projection_name = "task-projection";
    let projection_version = 1i64;
    let sizes_before = file_sizes(&workspace_db_path(&fixture.root, &fixture.workspace_id));

    // Offline rebuild at the fixed boundary with no snapshot present. The
    // inspector reads the immutable event slice, rebuilds the canonical state
    // and compares it; it never writes session facts or dispatches an Effect.
    let inspector = ProjectionInspector::new(fixture.db.clone(), fixture.workspace_id.clone());
    let rss_before = rss_bytes();
    let started = Instant::now();
    let comparison = inspector
        .compare(
            &fixture.workspace_id,
            projection_name,
            &fixture.big_session,
            max_seq,
            projection_version,
        )
        .await
        .expect("offline rebuild comparison");
    let rebuild_duration = started.elapsed().as_secs_f64() * 1000.0;
    let rss_after = rss_bytes();
    let comparison_json = json!({
        "status": format!("{:?}", comparison.status),
        "boundaryLastSeq": comparison.boundary_last_seq,
        "observedHighWatermark": comparison.observed_high_watermark,
        "appendedDuringCompare": comparison.appended_during_compare,
        "matched": comparison.matched,
        "canonicalHashPrefix": comparison.canonical_hash.chars().take(16).collect::<String>(),
        "evidenceRefs": comparison.evidence_refs.len(),
        "snapshotLastSeq": comparison.snapshot_last_seq,
        "snapshotStateVersion": comparison.snapshot_state_version,
        "differences": comparison.differences.len(),
    });
    drop(comparison);
    let sizes_after = file_sizes(&workspace_db_path(&fixture.root, &fixture.workspace_id));

    // A daily snapshot at the fixed boundary. The body is a real canonical
    // document shaped from the first 1,000 events at the boundary; its byte
    // size is recorded so the read latency is not read as a tiny placeholder.
    // The document keeps lastSeq = boundary; it is a size/shape sample, not a
    // complete 1M-event projection.
    let sample_events = fixture
        .db
        .projection_boundary_events(&fixture.workspace_id, &fixture.big_session, max_seq.min(1_000))
        .await
        .expect("snapshot sample events");
    let state_json = canonical_state(projection_name, &fixture.big_session, max_seq, &sample_events.events);
    let state_json_text = serde_json::to_string(&state_json).expect("serialize snapshot body");
    let state_json_bytes = state_json_text.len();
    let state_json_event_samples = sample_events.events.len();
    sqlx::query(
        "INSERT OR REPLACE INTO projection_snapshots
            (projection_name, subject_id, last_seq, projection_version, state_version, state_json, updated_at)
         VALUES (?, ?, ?, ?, 0, ?, ?)",
    )
    .bind(projection_name)
    .bind(&fixture.big_session)
    .bind(max_seq)
    .bind(projection_version)
    .bind(&state_json_text)
    .bind(Utc::now().to_rfc3339())
    .execute(&fixture.pool)
    .await
    .expect("insert snapshot");
    let mut snapshot_read = SampleSet::new("projection_snapshot_read", "ms");
    for _ in 0..50 {
        let started = Instant::now();
        fixture
            .db
            .projection_snapshot(
                &fixture.workspace_id,
                projection_name,
                &fixture.big_session,
                projection_version,
            )
            .await
            .expect("snapshot read")
            .expect("snapshot present");
        snapshot_read.push(started.elapsed().as_secs_f64() * 1000.0);
    }

    // Failure recovery: append one event between the fixed-boundary read and
    // the post-rebuild high-watermark recheck. The comparison must report
    // StaleAppend and must not write a replacement snapshot. Remove the
    // placeholder first so the comparison allocates no difference list over
    // the 1M-event canonical state.
    sqlx::query("DELETE FROM projection_snapshots WHERE projection_name = ? AND subject_id = ?")
        .bind(projection_name)
        .bind(&fixture.big_session)
        .execute(&fixture.pool)
        .await
        .expect("remove placeholder snapshot");
    let counter = Arc::new(AtomicU64::new(0));
    let pool_for_barrier = fixture.pool.clone();
    let session_for_barrier = fixture.big_session.clone();
    let task_for_barrier = fixture.task_id.clone();
    let barrier_counter = counter.clone();
    let barrier: CompareBarrier = Arc::new(move || {
        let pool = pool_for_barrier.clone();
        let session_id = session_for_barrier.clone();
        let task_id = task_for_barrier.clone();
        let counter = barrier_counter.clone();
        Box::pin(async move {
            counter.fetch_add(1, Ordering::SeqCst);
            insert_session_events_batch(&pool, &session_id, &task_id, 1, 96).await;
        })
    });
    let stale_inspector =
        ProjectionInspector::new(fixture.db.clone(), fixture.workspace_id.clone()).with_compare_barrier(barrier);
    let started = Instant::now();
    let stale = stale_inspector
        .compare(
            &fixture.workspace_id,
            projection_name,
            &fixture.big_session,
            max_seq,
            projection_version,
        )
        .await
        .expect("stale-append comparison");
    let stale_duration = started.elapsed().as_secs_f64() * 1000.0;
    let snapshot_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM projection_snapshots WHERE projection_name = ? AND subject_id = ?")
            .bind(projection_name)
            .bind(&fixture.big_session)
            .fetch_one(&fixture.pool)
            .await
            .expect("snapshot count");

    evidence.add(
        "rebuild_snapshot",
        json!({
            "dailySnapshotRead": snapshot_read.to_json(),
            "dailySnapshotBody": {
                "canonicalShape": true,
                "eventSamples": state_json_event_samples,
                "stateJsonBytes": state_json_bytes,
                "note": "Canonical document shaped from the first 1,000 events at the boundary; not a complete 1M-event projection.",
            },
            "offlineRebuild": {
                "durationMs": rebuild_duration,
                "boundarySeq": max_seq,
                "sessionEventCount": session_event_count,
                "globalHighWatermark": global_high_watermark,
                "rssBeforeBytes": rss_before,
                "rssAfterBytes": rss_after,
                "rssDeltaBytes": rss_before.zip(rss_after).map(|(a, b)| b as i64 - a as i64),
                "comparison": comparison_json,
                "dbBytesBefore": sizes_before["dbBytes"],
                "walBytesBefore": sizes_before["walBytes"],
                "dbBytesAfter": sizes_after["dbBytes"],
                "walBytesAfter": sizes_after["walBytes"],
                "progress": "not exposed: the rebuild is a single inspected call; no progress callback exists",
            },
            "failureRecovery": {
                "case": "append between fixed boundary and post-rebuild high-watermark recheck",
                "status": format!("{:?}", stale.status),
                "appendedDuringCompare": stale.appended_during_compare,
                "snapshotRowsAfter": snapshot_count,
                "replacementSuppressed": snapshot_count == 0,
                "durationMs": stale_duration,
            },
        }),
    );
}

// ---------------------------------------------------------------------------
// Backup write latency and cross-workspace isolation
// ---------------------------------------------------------------------------

async fn measure_backup_isolation(evidence: &mut Evidence, scale: &Scale) {
    let root = temp_root("backup");
    let ids = vec!["ws-backup-a".to_string(), "ws-backup-b".to_string()];
    let db = platform_db(&root, &ids).await;
    for id in &ids {
        db.create_workspace_store(id).await.expect("create backup store");
    }
    let pool_a = db.workspace_pool_for_testing(&ids[0]).await.expect("pool a");
    let pool_b = db.workspace_pool_for_testing(&ids[1]).await.expect("pool b");
    // Enough history that VACUUM INTO takes long enough to overlap writers.
    insert_session_events_batch(&pool_a, "sess-backup", "task-backup", scale.backup_events, 96).await;
    insert_session_events_batch(&pool_b, "sess-backup", "task-backup", scale.backup_events, 96).await;

    let mut backup_samples = SampleSet::new("backup_vacuum_into", "ms");
    let mut a_write = SampleSet::new("workspace_write_during_own_backup", "ms");
    let mut b_write = SampleSet::new("other_workspace_write_during_backup", "ms");
    let mut rounds = Vec::new();
    let mut last_backup_bytes = 0u64;

    for round in 0..3u64 {
        let backup_path = root
            .join("data/agents")
            .join(&ids[0])
            .join("harness/backups")
            .join(format!("bench-{round}.db"));
        tokio::fs::create_dir_all(backup_path.parent().expect("backup parent"))
            .await
            .expect("backup dir");
        let _ = tokio::fs::remove_file(&backup_path).await;

        let stop = Arc::new(AtomicBool::new(false));
        let counter_a = Arc::new(AtomicU64::new(round * 1_000_000));
        let counter_b = Arc::new(AtomicU64::new(round * 1_000_000));
        let db_a = db.clone();
        let db_b = db.clone();
        let ws_a = ids[0].clone();
        let ws_b = ids[1].clone();
        let stop_a = stop.clone();
        let stop_b = stop.clone();
        let ca = counter_a.clone();
        let cb = counter_b.clone();
        let writer_a = tokio::spawn(async move {
            let mut samples: Vec<(Instant, f64)> = Vec::new();
            while !stop_a.load(Ordering::Relaxed) {
                let sequence = ca.fetch_add(1, Ordering::Relaxed);
                let started = Instant::now();
                let _ = append_bench_event(&db_a, &ws_a, sequence, 96).await;
                samples.push((started, started.elapsed().as_secs_f64() * 1000.0));
                tokio::task::yield_now().await;
            }
            samples
        });
        let writer_b = tokio::spawn(async move {
            let mut samples: Vec<(Instant, f64)> = Vec::new();
            while !stop_b.load(Ordering::Relaxed) {
                let sequence = cb.fetch_add(1, Ordering::Relaxed);
                let started = Instant::now();
                let _ = append_bench_event(&db_b, &ws_b, sequence, 96).await;
                samples.push((started, started.elapsed().as_secs_f64() * 1000.0));
                tokio::task::yield_now().await;
            }
            samples
        });

        tokio::time::sleep(Duration::from_millis(20)).await;
        let backup_started = Instant::now();
        let vacuum_sql = format!("VACUUM INTO '{}'", backup_path.to_string_lossy());
        sqlx::query(AssertSqlSafe(vacuum_sql))
            .execute(&pool_a)
            .await
            .expect("vacuum into backup");
        let backup_elapsed = backup_started.elapsed().as_secs_f64() * 1000.0;
        let backup_finished = Instant::now();

        tokio::time::sleep(Duration::from_millis(20)).await;
        stop.store(true, Ordering::Relaxed);
        let samples_a = writer_a.await.expect("writer a");
        let samples_b = writer_b.await.expect("writer b");
        let in_backup = |samples: &[(Instant, f64)]| -> Vec<f64> {
            samples
                .iter()
                .filter(|(at, _)| *at >= backup_started && *at <= backup_finished)
                .map(|(_, value)| *value)
                .collect()
        };
        for value in in_backup(&samples_a) {
            a_write.push(value);
        }
        for value in in_backup(&samples_b) {
            b_write.push(value);
        }
        backup_samples.push(backup_elapsed);
        last_backup_bytes = std::fs::metadata(&backup_path).map(|m| m.len()).unwrap_or(0);

        let backup_check: String = if backup_path.exists() {
            let backup_pool = SqlitePool::connect(&format!("sqlite:{}?mode=ro", backup_path.display()))
                .await
                .expect("open backup");
            let check: String = sqlx::query_scalar("PRAGMA integrity_check")
                .fetch_one(&backup_pool)
                .await
                .unwrap_or_else(|_| "error".to_string());
            let backup_events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM session_events")
                .fetch_one(&backup_pool)
                .await
                .unwrap_or(-1);
            backup_pool.close().await;
            display_json(&json!({"integrityCheck": check, "sessionEvents": backup_events}))
        } else {
            "missing".to_string()
        };
        rounds.push(json!({
            "round": round,
            "backupMs": backup_elapsed,
            "backupBytes": std::fs::metadata(&backup_path).map(|m| m.len()).unwrap_or(0),
            "backupValidation": backup_check,
            "workspaceAWriteStartsInBackupWindow": in_backup(&samples_a).len(),
            "workspaceBWriteStartsInBackupWindow": in_backup(&samples_b).len(),
        }));
    }

    // Cross-workspace isolation when one workspace is marked unavailable.
    db.mark_workspace_unavailable(&ids[0], "benchmark isolation probe")
        .await
        .expect("mark workspace unavailable");
    let a_error = db
        .workspace_pool_for_testing(&ids[0])
        .await
        .err()
        .map(|error| error.to_string())
        .unwrap_or_else(|| "unexpectedly available".to_string());
    let mut isolated_write = SampleSet::new("other_workspace_write_while_a_unavailable", "ms");
    for index in 0..50 {
        let started = Instant::now();
        append_bench_event(&db, &ids[1], 9_000_000 + index, 96)
            .await
            .expect("other workspace write");
        isolated_write.push(started.elapsed().as_secs_f64() * 1000.0);
    }

    let sizes_a = file_sizes(&workspace_db_path(&root, &ids[0]));
    evidence.add(
        "backup_isolation",
        json!({
            "workspaceAEventCount": scale.backup_events,
            "workspaceBEventCount": scale.backup_events,
            "rounds": rounds,
            "backupLatency": backup_samples.to_json(),
            "workspaceAWriteDuringOwnBackup": a_write.to_json(),
            "workspaceBWriteDuringOtherBackup": b_write.to_json(),
            "backupFileBytesFromLastRound": last_backup_bytes,
            "harnessDbBytes": sizes_a["dbBytes"],
            "harnessWalBytes": sizes_a["walBytes"],
            "windowAttribution": "write samples counted when the append started inside [backup start, backup end]",
            "workspaceAUnavailableError": a_error,
            "workspaceBWriteWhileAUnavailable": isolated_write.to_json(),
            "blockingClaim": "measured latency only; this does not claim zero blocking",
        }),
    );

    db.close_workspace_store(&ids[1]).await.expect("close b");
    db.pool().close().await;
    let _ = tokio::fs::remove_dir_all(&root).await;
}

// ---------------------------------------------------------------------------
// Existing Task create / stream page / Job paths (branch)
// ---------------------------------------------------------------------------

async fn create_task_once(db: &Db, workspace_id: &str, key: &str, objective_bytes: usize) -> Result<(), String> {
    let now = Utc::now();
    let id = Uuid::new_v4().to_string();
    let task = AgentTask {
        id: id.clone(),
        workspace_id: workspace_id.to_string(),
        idempotency_key: key.to_string(),
        objective: "o".repeat(objective_bytes),
        session_key: None,
        created_by: "bench-user".to_string(),
        status: TaskStatus::Active,
        current_run_id: None,
        state_version: 0,
        created_at: now,
        updated_at: now,
        ended_at: None,
    };
    let wake = TaskWake {
        task_id: id,
        event_id: Uuid::new_v4().to_string(),
        signal_json: json!({"workspace_id": workspace_id}).to_string(),
        dispatch_state: WakeDispatchState::Pending,
        created_at: now,
        dispatched_at: None,
        resolved_at: None,
    };
    db.create_task_with_wake(&task, &wake)
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn create_job_once<'a>(
    db: &'a Db,
    workspace_id: &'a str,
    index: u64,
    args_bytes: usize,
) -> impl std::future::Future<Output = Result<(), String>> + 'a {
    let job = DeviceJob {
        id: format!("job-{workspace_id}-{index}"),
        workspace_id: workspace_id.to_string(),
        device_id: "device-bench".to_string(),
        tool: "bench.tool".to_string(),
        args_json: json!({"payload": "a".repeat(args_bytes)}),
        state: DeviceJobState::Accepted,
        stop_requested_at: None,
        progress_json: None,
        result_json: None,
        task_id: None,
        run_id: None,
        tool_call_id: Some(format!("call-{workspace_id}-{index}")),
        driver_call_id: None,
        timeout_ms: 30_000,
        started_at: Utc::now(),
        ended_at: None,
    };
    async move { db.create_job(&job).await.map(|_| ()).map_err(|error| error.to_string()) }
}

async fn measure_task_paths(evidence: &mut Evidence, scale: &Scale) {
    let root = temp_root("task-paths");
    let mut ids: Vec<String> = (0..20).map(|index| format!("ws-task-{index:02}")).collect();
    ids.insert(0, "ws-task-00".to_string());
    ids.sort();
    ids.dedup();
    let db = platform_db(&root, &ids).await;
    let primary = ids[0].clone();

    let mut create = SampleSet::new("task_create", "ms");
    for index in 0..scale.task_iterations {
        let started = Instant::now();
        create_task_once(&db, &primary, &format!("idem-{index}"), 64)
            .await
            .expect("task create");
        create.push(started.elapsed().as_secs_f64() * 1000.0);
    }

    let mut dedup = SampleSet::new("task_create_dedup", "ms");
    for index in 0..scale.task_dedup_iterations {
        let started = Instant::now();
        create_task_once(&db, &primary, &format!("idem-{}", index % 10), 64)
            .await
            .expect("task create dedup");
        dedup.push(started.elapsed().as_secs_f64() * 1000.0);
    }

    let mut large_create = SampleSet::new("task_create_64k_objective", "ms");
    for index in 0..scale.task_large_iterations {
        let started = Instant::now();
        create_task_once(&db, &primary, &format!("idem-large-{index}"), 64 * 1024)
            .await
            .expect("large task create");
        large_create.push(started.elapsed().as_secs_f64() * 1000.0);
    }

    let mut multi_workspace_create = SampleSet::new("task_create_multi_workspace", "ms");
    for (workspace_index, workspace_id) in ids.iter().enumerate() {
        for index in 0..scale.multi_workspace_iterations {
            let started = Instant::now();
            create_task_once(&db, workspace_id, &format!("multi-{workspace_index}-{index}"), 64)
                .await
                .expect("multi-workspace task create");
            multi_workspace_create.push(started.elapsed().as_secs_f64() * 1000.0);
        }
    }

    // Concurrent Task create.
    let mut concurrent_create = SampleSet::new("task_create_concurrent", "ms");
    let concurrency = scale.concurrent_workers;
    let per_worker = scale.concurrent_per_worker;
    let mut handles = Vec::new();
    for worker in 0..concurrency {
        let db = db.clone();
        let workspace_id = primary.clone();
        handles.push(tokio::spawn(async move {
            let mut samples = Vec::new();
            for index in 0..per_worker {
                let started = Instant::now();
                create_task_once(&db, &workspace_id, &format!("conc-{worker}-{index}"), 64)
                    .await
                    .expect("concurrent task create");
                samples.push(started.elapsed().as_secs_f64() * 1000.0);
            }
            samples
        }));
    }
    for handle in handles {
        for value in handle.await.expect("concurrent worker") {
            concurrent_create.push(value);
        }
    }
    evidence.concurrency.push(json!({
        "path": "task_create",
        "workers": concurrency,
        "samples": concurrent_create.samples.len(),
    }));

    // Stream page: bounded runtime ledger page backing the Task SSE endpoint.
    let stream_task = "task-stream".to_string();
    insert_runtime_events(db.pool(), &primary, &stream_task, scale.runtime_event_count, 96).await;
    let mut newest = SampleSet::new("runtime_event_page_newest", "ms");
    let mut old_cursor = SampleSet::new("runtime_event_page_old_cursor", "ms");
    for _ in 0..scale.page_iterations {
        let started = Instant::now();
        let page = db
            .runtime_events_page(&primary, &stream_task, None, 200)
            .await
            .expect("newest runtime page");
        newest.push(started.elapsed().as_secs_f64() * 1000.0);
        assert!(page.events.len() <= 200);
        let started = Instant::now();
        let page = db
            .runtime_events_page(&primary, &stream_task, Some(2), 200)
            .await
            .expect("old cursor runtime page");
        old_cursor.push(started.elapsed().as_secs_f64() * 1000.0);
        assert!(page.events.len() <= 1);
    }

    // Large-payload stream page.
    insert_runtime_events(db.pool(), &primary, "task-stream-large", 200, 64 * 1024).await;
    let rss_before = rss_bytes();
    let started = Instant::now();
    let large_page = db
        .runtime_events_page(&primary, "task-stream-large", None, 200)
        .await
        .expect("large runtime page");
    let large_duration = started.elapsed().as_secs_f64() * 1000.0;
    let rss_after = rss_bytes();
    let large_payload_rows = large_page.events.len();
    drop(large_page);

    // Job path.
    let mut job_create = SampleSet::new("job_create", "ms");
    for index in 0..scale.task_iterations {
        let started = Instant::now();
        create_job_once(&db, &primary, index, 64).await.expect("job create");
        job_create.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    let mut job_get = SampleSet::new("job_get", "ms");
    for index in 0..scale.task_iterations {
        let started = Instant::now();
        db.get_job(&primary, &format!("job-{primary}-{index}"))
            .await
            .expect("job get")
            .expect("job present");
        job_get.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    let mut large_job = SampleSet::new("job_create_64k_args", "ms");
    for index in 0..scale.task_large_iterations {
        let started = Instant::now();
        create_job_once(&db, &primary, 10_000 + index, 64 * 1024)
            .await
            .expect("large job create");
        large_job.push(started.elapsed().as_secs_f64() * 1000.0);
    }

    evidence.add(
        "task_paths_branch",
        json!({
            "taskCreate": create.to_json(),
            "taskCreateDedup": dedup.to_json(),
            "taskCreate64kObjective": large_create.to_json(),
            "taskCreateMultiWorkspace": multi_workspace_create.to_json(),
            "taskCreateConcurrent": concurrent_create.to_json(),
            "streamPageNewest": newest.to_json(),
            "streamPageOldCursor": old_cursor.to_json(),
            "streamTaskEventCount": scale.runtime_event_count,
            "streamLargePayload": {
                "durationMs": large_duration,
                "rows": large_payload_rows,
                "payloadBytes": 64 * 1024,
                "rssBeforeBytes": rss_before,
                "rssAfterBytes": rss_after,
                "rssDeltaBytes": rss_before.zip(rss_after).map(|(a, b)| b as i64 - a as i64),
            },
            "jobCreate": job_create.to_json(),
            "jobGet": job_get.to_json(),
            "jobCreate64kArgs": large_job.to_json(),
            "pathNote": "Measured at the storage path used by the HTTP handlers; the SSE transport and auth layer are not included.",
        }),
    );

    evidence.data_sizes.push(json!({
        "fixture": "task_paths",
        "workspaceCount": ids.len(),
        "taskCount": scale.task_iterations
            + scale.task_dedup_iterations
            + scale.task_large_iterations
            + ids.len() as u64 * scale.multi_workspace_iterations
            + scale.concurrent_per_worker * concurrency as u64,
        "runtimeEventCount": scale.runtime_event_count + 200,
        "jobCount": scale.task_iterations + scale.task_large_iterations,
    }));

    db.pool().close().await;
    let _ = tokio::fs::remove_dir_all(&root).await;
}

// ---------------------------------------------------------------------------
// Orchestration and output
// ---------------------------------------------------------------------------

fn collect_sample_counts(value: &Value, counts: &mut Vec<u64>) {
    match value {
        Value::Object(map) => {
            if let Some(sample_count) = map.get("sampleCount").and_then(Value::as_u64) {
                counts.push(sample_count);
            }
            for child in map.values() {
                collect_sample_counts(child, counts);
            }
        }
        Value::Array(items) => {
            for child in items {
                collect_sample_counts(child, counts);
            }
        }
        _ => {}
    }
}

async fn run_all() -> Value {
    let scale = Scale::current();
    let mut evidence = Evidence::new();
    evidence.notes.push(format!("scale={}", scale.name));
    evidence.notes.push(format!(
        "Scale config: workspaces {:?}, event buckets {:?}, guard {} terminal + {} active, backup {} events/workspace, runtime events {}, region iterations {} (unique), {} (dedup), {} (large), {} concurrent x {}.",
        scale.workspace_counts,
        scale.event_buckets,
        scale.guard_terminal,
        scale.guard_active,
        scale.backup_events,
        scale.runtime_event_count,
        scale.task_iterations,
        scale.task_dedup_iterations,
        scale.task_large_iterations,
        scale.concurrent_workers,
        scale.concurrent_per_worker,
    ));
    evidence.notes.push(
        "Latency percentiles use nearest-rank on the raw samples; local integration timings are not production p99."
            .to_string(),
    );
    evidence.notes.push(
        "Size buckets are measurement inputs, not quotas or SLA; the artifact records the exact buckets used."
            .to_string(),
    );
    evidence.notes.push(
        "Session-event payloads are 96 bytes; runtime-event pages use 96-byte payloads except the 64KiB large-payload case; Task objective and Job args large cases are 64KiB.".to_string(),
    );
    evidence.notes.push(
        "RSS is sampled with `ps -o rss=` and is approximate; allocator reuse can make deltas zero or negative. The 1M preload contrast is the closest measured analogue to the previous whole-history prefetch, not a main-binary sample.".to_string(),
    );
    evidence.notes.push(
        "The command runs the unoptimized debug test profile. These are local reproducibility numbers, not production p99; run the same entry point with --release for release-build evidence.".to_string(),
    );

    let started = Instant::now();
    measure_startup(&mut evidence, &scale).await;
    measure_guard(&mut evidence, &scale).await;
    let event_fixture = measure_events(&mut evidence, &scale).await;
    measure_snapshot_rebuild(&mut evidence, &event_fixture).await;
    measure_backup_isolation(&mut evidence, &scale).await;
    measure_task_paths(&mut evidence, &scale).await;
    let total_duration = started.elapsed().as_secs_f64() * 1000.0;

    // Clean the event fixture after snapshot/rebuild measurements.
    close_and_remove(
        event_fixture.db,
        std::slice::from_ref(&event_fixture.workspace_id),
        &event_fixture.root,
    )
    .await;

    let main = main_ref_evidence();
    let metadata_root = temp_root("metadata");
    let metadata_workspace = "ws-metadata".to_string();
    let metadata_db = platform_db(&metadata_root, std::slice::from_ref(&metadata_workspace)).await;
    metadata_db
        .create_workspace_store(&metadata_workspace)
        .await
        .expect("create metadata workspace");
    let metadata_workspace_pool = metadata_db
        .workspace_pool_for_testing(&metadata_workspace)
        .await
        .expect("metadata workspace pool");
    let workspace_file_sqlite = sqlite_metadata(&metadata_workspace_pool).await;
    let platform_fixture_sqlite = sqlite_metadata(metadata_db.pool()).await;
    let sqlite = json!({
        "workspaceFile": workspace_file_sqlite,
        "platformFixtureInMemory": platform_fixture_sqlite,
        "note": "Workspace stores are file-backed WAL; the platform fixture is sqlite::memory: for isolation.",
    });
    close_and_remove(metadata_db, std::slice::from_ref(&metadata_workspace), &metadata_root).await;
    let config = DatabaseConfig::default();
    let config_json = json!({
        "platform": {
            "url": config.url,
            "maxConnections": config.max_connections,
            "minConnections": config.min_connections,
            "acquireTimeoutSecs": config.acquire_timeout_secs,
            "idleTimeoutSecs": config.idle_timeout_secs,
        },
        "workspaceStore": {
            "maxConnections": config.max_connections,
            "minConnections": 0,
            "acquireTimeoutSecs": config.acquire_timeout_secs,
            "idleTimeoutSecs": config.idle_timeout_secs,
            "source": "Db::new -> DatabaseConfig::default(); pool::workspace_pool_options sets min_connections=0",
        },
    });

    let all_sample_counts: Vec<Value> = evidence
        .measurements
        .iter()
        .map(|(group, rows)| {
            let mut counts = Vec::new();
            for row in rows {
                collect_sample_counts(row, &mut counts);
            }
            let total: u64 = counts.iter().sum();
            json!({
                "group": group,
                "rowCount": rows.len(),
                "sampleSetCount": counts.len(),
                "sampleCounts": counts,
                "totalSamples": total,
            })
        })
        .collect();

    let mut missing = evidence.missing;
    if main["status"] != "measured" {
        missing.push(json!({
            "evidence": "main/branch interleaved p50/p95 for Task create/stream/Job and Harness storage",
            "mainStatus": main["status"],
            "reason": main["reason"],
            "requiredPathsAbsent": main["requiredPathsAbsent"],
        }));
    }

    let overall = if missing.is_empty() { "COMPLETE" } else { "INCOMPLETE" };
    let overall_reason = if missing.is_empty() {
        "branch and main evidence present".to_string()
    } else {
        format!(
            "branch measurements complete; {} required evidence item(s) missing (see missingEvidence)",
            missing.len()
        )
    };
    json!({
        "schema": "harness-performance-evidence/v1",
        "generatedAt": Utc::now().to_rfc3339(),
        "role": "branch",
        "branchStatus": "complete",
        "overall": overall,
        "overallReason": overall_reason,
        "durationMs": total_duration,
        "scale": scale.name,
        "scaleConfig": scale.to_json(),
        "machine": machine_metadata(),
        "sqlite": sqlite,
        "binary": binary_metadata(),
        "config": config_json,
        "dataSizes": evidence.data_sizes,
        "concurrency": evidence.concurrency,
        "sampleDistribution": {
            "method": "nearest-rank",
            "perMetric": all_sample_counts,
        },
        "measurements": evidence.measurements,
        "main": main,
        "missingEvidence": missing,
        "notes": evidence.notes,
    })
}

fn display_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string())
}

fn artifact_dir() -> PathBuf {
    std::env::var("HARNESS_BENCH_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join("target/harness-perf")
        })
}

fn collect_empty_sample_sets(value: &Value, path: &str, failures: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            if map.get("sampleCount").and_then(Value::as_u64) == Some(0) {
                failures.push(format!("{path} has no samples"));
            }
            if map.get("droppedSamples").and_then(Value::as_u64).unwrap_or(0) > 0 {
                failures.push(format!(
                    "{path} dropped {} non-finite or negative samples",
                    map["droppedSamples"]
                ));
            }
            for (key, child) in map {
                collect_empty_sample_sets(child, &format!("{path}.{key}"), failures);
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                collect_empty_sample_sets(child, &format!("{path}[{index}]"), failures);
            }
        }
        _ => {}
    }
}

fn emit_report(report: &Value) {
    let pretty = serde_json::to_string_pretty(report).expect("serialize report");
    println!("=== HARNESS_PERF_BEGIN ===");
    println!("{pretty}");
    println!("=== HARNESS_PERF_END ===");
    let dir = artifact_dir();
    if let Err(error) = std::fs::create_dir_all(&dir) {
        eprintln!("could not create artifact directory {}: {error}", dir.display());
        return;
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let path = dir.join(format!("harness-perf-branch-{stamp}.json"));
    let latest = dir.join("harness-perf-branch-latest.json");
    if let Err(error) = std::fs::write(&path, &pretty) {
        eprintln!("could not write {}: {error}", path.display());
    } else {
        println!("raw evidence: {}", path.display());
    }
    let _ = std::fs::write(&latest, &pretty);
    println!(
        "OVERALL: {} (main={}, missingEvidence={})",
        report["overall"].as_str().unwrap_or("UNKNOWN"),
        report["main"]["status"].as_str().unwrap_or("unknown"),
        report["missingEvidence"].as_array().map(Vec::len).unwrap_or(0)
    );

    // Fail closed on internal incompleteness (empty samples), but keep an
    // explicitly missing main baseline as recorded evidence rather than
    // invented numbers.
    let mut failures = Vec::new();
    collect_empty_sample_sets(report, "$", &mut failures);
    for field in [
        "machine",
        "sqlite",
        "binary",
        "config",
        "dataSizes",
        "concurrency",
        "sampleDistribution",
    ] {
        if report.get(field).is_none() {
            failures.push(format!("missing metadata field {field}"));
        }
    }
    assert!(failures.is_empty(), "incomplete performance evidence: {failures:?}");
}
