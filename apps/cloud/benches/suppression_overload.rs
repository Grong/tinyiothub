//! Task 9 suppression-overload performance evidence.
//!
//! A focused, deterministic bench for the bounded preflight path: per-Sample
//! Source capture + canonical hashing under a runtime-configured preflight
//! gate, plus SQLite contention. It records machine/OS/Rust/SQLite versions,
//! fixture sizes, nearest-rank p50/p95/p99, RSS, DB busy timeout/errors and an
//! explicit base-vs-branch section.
//!
//! Run:
//!   cargo test -p tinyiothub-cloud --bench suppression_overload -- --nocapture
//!
//! Optional:
//!   SUPPRESSION_BENCH_SCALE=full   larger fixtures (default: smoke)
//!   SUPPRESSION_BENCH_OUT=<dir>    artifact directory (default: .gstack/tmp)
//!
//! The report never invents a baseline: `main` does not contain the suppression
//! preflight module (Tasks 1-9 live on this branch), so the interleaved
//! base-vs-branch section records `missing` with the exact absent path instead
//! of fabricated numbers.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use tinyiothub_cloud::domains::agent::session::suppression::SuppressionMode;
use tinyiothub_cloud::domains::agent::session::suppression::contract::{
    CanonicalBytes, PayloadRef, SourceDescriptor, SourceSnapshot, StateRef, WorkItem, WorkItemDisposition,
};
use tinyiothub_cloud::domains::agent::session::suppression::preflight::{
    PreflightGate, PreflightLimits, SuppressionService,
};
use tinyiothub_cloud::domains::agent::session::suppression::registry::{
    AssistantSource, SOURCE_VERSION, SourceContext, SourceRegistry,
};
use tinyiothub_storage::assistant_schedule::SuppressionConfig;
use tinyiothub_storage::suppression::SuppressionState;

const WORKSPACE: &str = "ws-suppression-bench";
const SESSION: &str = "agent:ws-suppression-bench:__assistant__ws-suppression-bench/main";

#[derive(Debug, Clone)]
struct Scale {
    name: &'static str,
    item_counts: Vec<usize>,
    concurrency: Vec<usize>,
    samples_per_lane: usize,
    payload_bytes: usize,
}

impl Scale {
    fn current() -> Self {
        match std::env::var("SUPPRESSION_BENCH_SCALE").as_deref() {
            Ok("full") => Self {
                name: "full",
                item_counts: vec![10, 100, 1_000, 10_000],
                concurrency: vec![1, 2, 4, 8],
                samples_per_lane: 60,
                payload_bytes: 256,
            },
            _ => Self {
                name: "smoke",
                item_counts: vec![10, 100, 1_000],
                concurrency: vec![1, 2, 4, 8],
                samples_per_lane: 20,
                payload_bytes: 128,
            },
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "name": self.name,
            "itemCounts": self.item_counts,
            "concurrency": self.concurrency,
            "samplesPerLane": self.samples_per_lane,
            "payloadBytesPerItem": self.payload_bytes,
        })
    }
}

struct BenchSource {
    source_id: String,
    item_count: usize,
    payload_bytes: usize,
}

#[async_trait::async_trait]
impl AssistantSource for BenchSource {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            source_id: self.source_id.clone(),
            source_version: SOURCE_VERSION,
        }
    }

    async fn capture(
        &self,
        _cx: &SourceContext,
    ) -> Result<SourceSnapshot, tinyiothub_cloud::domains::agent::session::suppression::error::SuppressionError> {
        // Yield once so a synchronized burst can actually contend for the
        // gate: without an await point the first task would finish before the
        // next one attempted admission.
        tokio::task::yield_now().await;
        let padding = "x".repeat(self.payload_bytes);
        let mut items = Vec::with_capacity(self.item_count);
        for index in 0..self.item_count {
            let payload = tinyiothub_cloud::domains::agent::session::suppression::canonical::canonical_bytes(
                &json!({ "thingId": format!("device-{index}"), "padding": &padding }),
            )?;
            let payload_hash =
                tinyiothub_cloud::domains::agent::session::suppression::canonical::hash_payload(&payload);
            items.push(WorkItem {
                work_key: format!("device-{index}"),
                revision: payload_hash.clone(),
                disposition: WorkItemDisposition::Active,
                payload_ref: PayloadRef {
                    id: format!("payload:{payload_hash}"),
                    revision: payload_hash.clone(),
                    payload_hash: payload_hash.clone(),
                },
                payload_hash,
            });
        }
        let state = tinyiothub_cloud::domains::agent::session::suppression::canonical::canonical_bytes(
            &json!({ "sourceId": &self.source_id, "workItems": &items }),
        )?;
        let state_hash = tinyiothub_cloud::domains::agent::session::suppression::canonical::hash_state(&state);
        Ok(SourceSnapshot {
            source_id: self.source_id.clone(),
            source_version: SOURCE_VERSION,
            revision: state_hash.clone(),
            state_hash: state_hash.clone(),
            available: true,
            empty: items.is_empty(),
            state_ref: Some(StateRef {
                id: format!("state:{state_hash}"),
                revision: state_hash.clone(),
                state_hash,
            }),
            work_items: items,
            error: None,
        })
    }

    async fn resolve_state(
        &self,
        _reference: &StateRef,
    ) -> Result<CanonicalBytes, tinyiothub_cloud::domains::agent::session::suppression::error::SuppressionError> {
        Err(
            tinyiothub_cloud::domains::agent::session::suppression::error::SuppressionError::new(
                "source_ref_mismatch",
                "bench does not retain state",
                "unused",
            ),
        )
    }

    async fn resolve_payload(
        &self,
        _reference: &PayloadRef,
    ) -> Result<CanonicalBytes, tinyiothub_cloud::domains::agent::session::suppression::error::SuppressionError> {
        Err(
            tinyiothub_cloud::domains::agent::session::suppression::error::SuppressionError::new(
                "work_item_payload_missing",
                "bench does not retain payloads",
                "unused",
            ),
        )
    }
}

fn config(mode: SuppressionMode) -> SuppressionConfig {
    SuppressionConfig {
        workspace_id: WORKSPACE.to_string(),
        mode,
        reconciliation_seconds: tinyiothub_storage::suppression::DEFAULT_RECONCILIATION_SECONDS,
        config_version: 1,
    }
}

fn percentile(sorted: &[u128], percent: f64) -> u128 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = ((percent / 100.0) * (sorted.len() - 1) as f64).round() as usize;
    sorted[rank.min(sorted.len() - 1)]
}

fn distribution(samples: &[u128]) -> Value {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let micros = |value: u128| value as f64 / 1_000.0;
    json!({
        "sampleCount": samples.len(),
        "minMs": micros(sorted.first().copied().unwrap_or(0)),
        "p50Ms": micros(percentile(&sorted, 50.0)),
        "p95Ms": micros(percentile(&sorted, 95.0)),
        "p99Ms": micros(percentile(&sorted, 99.0)),
        "maxMs": micros(sorted.last().copied().unwrap_or(0)),
    })
}

async fn measure_lane(item_count: usize, concurrency: usize, samples: usize, payload_bytes: usize) -> Value {
    let mut registry = SourceRegistry::new();
    registry
        .register(Arc::new(BenchSource {
            source_id: "bench.snapshot".to_string(),
            item_count,
            payload_bytes,
        }))
        .expect("register bench source");
    let gate = PreflightGate::new(
        tinyiothub_cloud::domains::agent::session::suppression::preflight::DEFAULT_PREFLIGHT_CONCURRENCY,
    );
    let service = Arc::new(
        SuppressionService::with_gate(Arc::new(registry), gate.clone()).with_limits(PreflightLimits::default()),
    );
    let config = config(SuppressionMode::Enforce);
    let state = SuppressionState::initial(WORKSPACE, SESSION);

    // Every lane releases each sample from the same barrier: this is the
    // synchronized burst that exposes saturation instead of letting fast
    // lanes finish before slower lanes arrive.
    let barrier = Arc::new(tokio::sync::Barrier::new(concurrency));
    let mut handles = Vec::new();
    for lane in 0..concurrency {
        let service = Arc::clone(&service);
        let config = config.clone();
        let state = state.clone();
        let barrier = Arc::clone(&barrier);
        handles.push(tokio::spawn(async move {
            let mut lane_samples = Vec::with_capacity(samples);
            let mut accepted = 0usize;
            let mut rejected = 0usize;
            for _ in 0..samples {
                barrier.wait().await;
                let started = Instant::now();
                match service
                    .evaluate(SourceContext::new(WORKSPACE, SESSION), Some(&state), &config, 0)
                    .await
                {
                    Ok(_) => {
                        accepted += 1;
                        lane_samples.push(started.elapsed().as_micros());
                    }
                    Err(error) if error.code == "wake_backpressure" => rejected += 1,
                    Err(error) => panic!("bench lane {lane} failed: {}", error.code),
                }
            }
            (lane_samples, accepted, rejected)
        }));
    }

    let mut samples = Vec::new();
    let mut accepted = 0usize;
    let mut rejected = 0usize;
    for handle in handles {
        let (lane_samples, lane_accepted, lane_rejected) = handle.await.expect("bench lane");
        samples.extend(lane_samples);
        accepted += lane_accepted;
        rejected += lane_rejected;
    }

    json!({
        "itemCount": item_count,
        "concurrency": concurrency,
        "payloadBytesPerItem": payload_bytes,
        "gateLimit": gate.limit(),
        "accepted": accepted,
        "backpressure": rejected,
        "inFlightAtEnd": gate.in_flight(),
        "latency": distribution(&samples),
    })
}

fn command_stdout(command: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(command).args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn machine_metadata() -> Value {
    let cpu = command_stdout("sysctl", &["-n", "machdep.cpu.brand_string"])
        .or_else(|| command_stdout("sh", &["-c", "grep -m1 'model name' /proc/cpuinfo | cut -d: -f2-"]))
        .unwrap_or_else(|| "unknown".to_string());
    let memory = command_stdout("sysctl", &["-n", "hw.memsize"])
        .or_else(|| command_stdout("sh", &["-c", "awk '/MemTotal/ {print $2*1024}' /proc/meminfo"]))
        .unwrap_or_else(|| "unknown".to_string());
    json!({
        "platform": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "uname": command_stdout("uname", &["-a"]),
        "osVersion": command_stdout("sw_vers", &["-productVersion"])
            .or_else(|| command_stdout("sh", &["-c", "cat /etc/os-release 2>/dev/null | tr '\\n' ' '"])),
        "cpu": cpu,
        "memoryBytes": memory,
        "logicalCpus": std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0),
        "rustc": command_stdout("rustc", &["--version"]),
    })
}

fn rss_bytes() -> Option<u64> {
    let pid = std::process::id().to_string();
    let output = Command::new("ps").args(["-o", "rss=", "-p", &pid]).output().ok()?;
    let kib: u64 = String::from_utf8_lossy(&output.stdout).trim().parse().ok()?;
    Some(kib * 1024)
}

fn git_output(args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn base_ref_evidence() -> Value {
    let path = "apps/cloud/src/domains/agent/session/suppression/preflight.rs";
    let exists = git_output(&["cat-file", "-e", &format!("main:{path}")]).is_some();
    json!({
        "role": "main",
        "refName": "main",
        "commit": git_output(&["log", "-1", "--format=%H", "main"]),
        "subject": git_output(&["log", "-1", "--format=%s", "main"]),
        "requiredPath": path,
        "requiredPathPresent": exists,
        "status": if exists { "available_but_not_interleaved" } else { "missing" },
        "numbers": Value::Null,
        "reason": if exists {
            "main contains the preflight module but this bench cannot execute a second checkout interleaved without a prebuilt main binary; no main samples were recorded."
        } else {
            "main does not contain the suppression preflight module (Tasks 1-9 are branch-only), so an interleaved base-vs-branch comparison has no baseline workload to run."
        },
    })
}

async fn sqlite_evidence() -> Value {
    let fixture = tinyiothub_storage::test_helpers::fixture_workspace(WORKSPACE).await;
    let db = Arc::new(fixture.db.clone());
    let pool = match db.workspace_pool_for_testing(WORKSPACE).await {
        Ok(pool) => pool,
        Err(error) => {
            fixture.close().await;
            return json!({ "status": "unavailable", "reason": error.to_string() });
        }
    };
    let sqlite_version: String = sqlx::query_scalar("SELECT sqlite_version()")
        .fetch_one(&pool)
        .await
        .unwrap_or_default();
    let busy_timeout: i64 = sqlx::query_scalar("PRAGMA busy_timeout")
        .fetch_one(&pool)
        .await
        .unwrap_or(-1);
    let journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(&pool)
        .await
        .unwrap_or_default();

    let started = Instant::now();
    let mut handles = Vec::new();
    for _ in 0..32 {
        let db = Arc::clone(&db);
        handles.push(tokio::spawn(async move {
            db.load_suppression_state(WORKSPACE, SESSION).await
        }));
    }
    let mut busy_errors = 0usize;
    for handle in handles {
        match handle.await.expect("contention task") {
            Ok(_) => {}
            Err(error) => {
                if error.to_string().to_ascii_lowercase().contains("locked")
                    || error.to_string().to_ascii_lowercase().contains("busy")
                {
                    busy_errors += 1;
                }
            }
        }
    }
    let elapsed_ms = started.elapsed().as_secs_f64() * 1_000.0;
    drop(pool);
    drop(db);
    fixture.close().await;

    json!({
        "status": "complete",
        "sqliteVersion": sqlite_version,
        "journalMode": journal_mode,
        "busyTimeoutMs": busy_timeout,
        "concurrentReads": 32,
        "busyErrors": busy_errors,
        "contentionElapsedMs": elapsed_ms,
    })
}

fn artifact_dir() -> PathBuf {
    std::env::var("SUPPRESSION_BENCH_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(".gstack/tmp"))
}

fn emit(report: &Value) -> PathBuf {
    let pretty = serde_json::to_string_pretty(report).expect("serialize report");
    println!("=== SUPPRESSION_OVERLOAD_PERF_BEGIN ===");
    println!("{pretty}");
    println!("=== SUPPRESSION_OVERLOAD_PERF_END ===");
    let dir = artifact_dir();
    std::fs::create_dir_all(&dir).expect("create artifact directory");
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let path = dir.join(format!("suppression-overload-{stamp}.json"));
    std::fs::write(&path, &pretty).expect("write artifact");
    let _ = std::fs::write(dir.join("suppression-overload-latest.json"), &pretty);
    println!("artifact: {}", path.display());
    path
}

#[test]
fn suppression_overload_performance_evidence() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("tokio runtime");
    runtime.block_on(async {
        let scale = Scale::current();
        let started = Instant::now();
        let mut lanes = Vec::new();
        for &item_count in &scale.item_counts {
            for &concurrency in &scale.concurrency {
                lanes.push(
                    measure_lane(
                        item_count,
                        concurrency,
                        scale.samples_per_lane,
                        scale.payload_bytes,
                    )
                    .await,
                );
            }
        }
        let total_samples: usize = lanes
            .iter()
            .map(|lane| lane["latency"]["sampleCount"].as_u64().unwrap_or(0) as usize)
            .sum();
        let backpressure: u64 = lanes
            .iter()
            .map(|lane| lane["backpressure"].as_u64().unwrap_or(0))
            .sum();
        let sqlite = sqlite_evidence().await;

        let report = json!({
            "schema": "suppression-overload-performance-evidence/v1",
            "generatedAt": chrono::Utc::now().to_rfc3339(),
            "role": "branch",
            "durationMs": started.elapsed().as_secs_f64() * 1_000.0,
            "scale": scale.to_json(),
            "machine": machine_metadata(),
            "binary": {
                "package": env!("CARGO_PKG_NAME"),
                "version": env!("CARGO_PKG_VERSION"),
                "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
                "branch": git_output(&["rev-parse", "--abbrev-ref", "HEAD"]),
                "commit": git_output(&["rev-parse", "HEAD"]),
                "commitSubject": git_output(&["log", "-1", "--format=%s"]),
                "dirtyWorktree": git_output(&["status", "--porcelain"]).is_some_and(|value| !value.is_empty()),
            },
            "limits": {
                "defaultConcurrency": tinyiothub_cloud::domains::agent::session::suppression::preflight::DEFAULT_PREFLIGHT_CONCURRENCY,
                "minConcurrency": tinyiothub_cloud::domains::agent::session::suppression::preflight::MIN_PREFLIGHT_CONCURRENCY,
                "maxConcurrency": tinyiothub_cloud::domains::agent::session::suppression::preflight::MAX_PREFLIGHT_CONCURRENCY,
                "captureDeadlineMs": tinyiothub_cloud::domains::agent::session::suppression::preflight::PREFLIGHT_CAPTURE_DEADLINE.as_millis(),
                "resolveDeadlineMs": tinyiothub_cloud::domains::agent::session::suppression::preflight::PREFLIGHT_RESOLVE_DEADLINE.as_millis(),
                "totalDeadlineMs": tinyiothub_cloud::domains::agent::session::suppression::preflight::PREFLIGHT_TOTAL_DEADLINE.as_millis(),
            },
            "dataSizes": {
                "workspaces": 1,
                "itemCountsPerWorkspace": &scale.item_counts,
                "payloadBytesPerItem": scale.payload_bytes,
            },
            "measurements": lanes,
            "summary": {
                "lanes": lanes.len(),
                "samples": total_samples,
                "backpressureResponses": backpressure,
                "method": "nearest-rank p50/p95/p99 over accepted evaluate samples",
            },
            "memory": {
                "rssBytes": rss_bytes(),
            },
            "sqlite": sqlite,
            "main": base_ref_evidence(),
            "notes": [
                "Debug-profile measurements in a shared 16 GiB workstation; use them for regression direction, not as a production SLA.",
                "Backpressure is counted, never queued: the gate uses try_acquire and the measured lanes stay at or below their limit.",
                "No baseline numbers are claimed because main has no suppression preflight path."
            ],
        });

        emit(&report);
    });
}
