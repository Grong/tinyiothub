//! `lifecycle` profile: load a real driver package, spawn a worker, read,
//! stop, respawn/read (reconnect), and prove a failed read does not panic or
//! kill the worker. Every check runs against the in-memory
//! [`TestHub`](crate::acceptance::test_hub::TestHub); no database or hardware.

use std::collections::HashSet;
use std::path::Path;
use std::time::{Duration, Instant};

use crate::acceptance::report::{CheckResult, Level, ProfileReport};
use crate::acceptance::test_hub::{TestHub, ThingOptions};
use crate::driver::worker::WorkerHandle;

const PROFILE: &str = "lifecycle";
/// Generous deadline: the read itself is instantaneous, but CI may be loaded.
const READ_TIMEOUT: Duration = Duration::from_secs(5);
/// Invalid-config reads never produce a reading; this only bounds the wait.
const FAILED_READ_TIMEOUT: Duration = Duration::from_millis(1000);
/// Time allowed for a failure counter (or an in-flight read) to become visible.
const FAILURE_WAIT: Duration = Duration::from_secs(2);
/// Exponential retry backoff is 500ms then 1000ms; leave scheduling margin.
const ATTEMPT_BACKOFF: Duration = Duration::from_millis(1100);

fn check(name: &str, level: Level, detail: impl Into<String>, evidence: serde_json::Value) -> CheckResult {
    CheckResult {
        profile: PROFILE.into(),
        check: name.into(),
        level,
        detail: detail.into(),
        evidence,
    }
}

pub fn run(dir: &Path) -> ProfileReport {
    let mut checks = Vec::new();
    let hub = TestHub::new();

    let loaded = match hub.load_package(dir) {
        Ok(loaded) => {
            checks.push(check(
                "package.load",
                Level::Pass,
                format!(
                    "loaded {} {} from {}",
                    loaded.driver,
                    loaded.version,
                    loaded.path.display()
                ),
                serde_json::json!({
                    "driver": loaded.driver,
                    "version": loaded.version,
                    "binary": loaded.path.display().to_string(),
                    "sources": loaded.source_ids(),
                }),
            ));
            loaded
        }
        Err(err) => {
            checks.push(check(
                "package.load",
                Level::Fail,
                err.clone(),
                serde_json::json!({"error": err}),
            ));
            return ProfileReport {
                profile: PROFILE.into(),
                checks,
            };
        }
    };

    let required: Vec<String> = loaded
        .manifest
        .state_fields
        .iter()
        .filter(|field| field.required)
        .map(|field| field.name.clone())
        .collect();
    let thing = hub.make_thing(&loaded, ThingOptions::default());

    let handle = match hub.spawn(&loaded.driver, thing.clone()) {
        Ok(handle) => {
            checks.push(check(
                "worker.spawn",
                Level::Pass,
                format!("worker started for '{}'", thing.id),
                serde_json::json!({"thing": thing.id, "driver": loaded.driver}),
            ));
            handle
        }
        Err(err) => {
            checks.push(check(
                "worker.spawn",
                Level::Fail,
                err.clone(),
                serde_json::json!({"error": err}),
            ));
            return finish(hub, loaded.driver, checks);
        }
    };

    match hub.read_and_wait(&handle, READ_TIMEOUT) {
        Ok(values) => {
            let names: HashSet<&str> = values.iter().map(|v| v.name.as_str()).collect();
            let missing: Vec<&String> = required.iter().filter(|name| !names.contains(name.as_str())).collect();
            let level = if missing.is_empty() { Level::Pass } else { Level::Fail };
            checks.push(check(
                "worker.read",
                level,
                if missing.is_empty() {
                    format!("read returned {} value(s)", values.len())
                } else {
                    format!("read is missing required field(s): {missing:?}")
                },
                serde_json::json!({
                    "values": values.iter().map(|v| serde_json::json!({
                        "name": v.name,
                        "valueType": v.value_type,
                        "value": v.value,
                    })).collect::<Vec<_>>(),
                    "required": required,
                }),
            ));
        }
        Err(err) => checks.push(check(
            "worker.read",
            Level::Fail,
            err.clone(),
            serde_json::json!({"error": err}),
        )),
    }

    let overview = hub.status(&handle);
    let online = overview.health.connection.is_online();
    let cached_online = hub.cached_thing(&thing.id).is_some_and(|t| t.status.is_online());
    checks.push(check(
        "worker.online",
        if online && cached_online {
            Level::Pass
        } else {
            Level::Fail
        },
        format!(
            "worker connection status: {:?}, cached thing online={cached_online}",
            overview.health.connection
        ),
        serde_json::json!({
            "connection": overview.health.connection,
            "cached_online": cached_online,
            "successes": overview.health.total_successes,
            "failures": overview.health.total_failures,
        }),
    ));

    match hub.stop(&handle) {
        Ok(()) => {
            let exited = handle.is_finished();
            let cached_offline = hub.cached_thing(&thing.id).is_some_and(|t| !t.status.is_online());
            checks.push(check(
                "worker.stop",
                if exited && cached_offline {
                    Level::Pass
                } else {
                    Level::Fail
                },
                format!("worker exited={exited}, cached status offline={cached_offline}"),
                serde_json::json!({"exited": exited, "cached_offline": cached_offline}),
            ));
        }
        Err(err) => checks.push(check(
            "worker.stop",
            Level::Fail,
            err.clone(),
            serde_json::json!({"error": err}),
        )),
    }

    // Reconnect: a fresh worker on the same thing reads again.
    let reconnect = hub.spawn(&loaded.driver, thing.clone()).and_then(|handle| {
        let values = hub.read_and_wait(&handle, READ_TIMEOUT)?;
        let overview = hub.status(&handle);
        let _ = hub.stop(&handle);
        Ok((values, overview.health.connection))
    });
    match reconnect {
        Ok((values, connection)) => checks.push(check(
            "worker.reconnect",
            if !values.is_empty() && connection.is_online() {
                Level::Pass
            } else {
                Level::Fail
            },
            format!("respawned worker read {} value(s), status {connection:?}", values.len()),
            serde_json::json!({
                "values": values.len(),
                "connection": connection,
            }),
        )),
        Err(err) => checks.push(check(
            "worker.reconnect",
            Level::Fail,
            err.clone(),
            serde_json::json!({"error": err}),
        )),
    }

    checks.push(failed_read_check(&hub, &loaded.driver));

    finish(hub, loaded.driver, checks)
}

/// Drive a read through a deliberately invalid `driver_options` payload (a NUL
/// byte): the dynamic adapter's `CString` conversion rejects it before reaching
/// the driver, so no reading is produced. A timeout alone is not evidence of a
/// failure (the hub could simply not have scheduled the read), so the check
/// requires the worker's `total_failures` counter to move: the retry budget is
/// driven to a terminal failure and then one more read is issued, proving the
/// worker kept processing after the failure without panicking.
fn failed_read_check(hub: &TestHub, driver: &str) -> CheckResult {
    static NEXT_BAD: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = NEXT_BAD.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let Some(loaded) = hub.loaded(driver) else {
        return check(
            "worker.read_failure",
            Level::Skip,
            format!("driver '{driver}' is no longer loaded"),
            serde_json::Value::Null,
        );
    };
    // A NUL byte cannot be passed through the adapter's `CString` config
    // conversion, so the read fails before the driver is invoked. This is a
    // deterministic failure the demo driver cannot swallow (it ignores normal
    // config values entirely).
    let bad = hub.make_thing(
        &loaded,
        ThingOptions {
            id: Some(format!("{}-bad-{seq}", loaded.driver)),
            name: None,
            driver_options: Some("{\"x\":\"\u{0}\"}".into()),
        },
    );
    let handle = match hub.spawn(driver, bad.clone()) {
        Ok(handle) => handle,
        Err(err) => {
            return check(
                "worker.read_failure",
                Level::Skip,
                format!("could not spawn the invalid-config worker: {err}"),
                serde_json::json!({"error": err}),
            );
        }
    };
    // Expected to time out: the adapter rejects the read and the worker waits
    // out its retry backoff instead of producing a reading. The timeout itself
    // is not scored — the failure counters below are.
    let timed_out = hub.read_and_wait(&handle, FAILED_READ_TIMEOUT).err();

    // The worker's overview (which carries `total_failures`) is only published
    // on a terminal failure, so drive the retry budget to exhaustion. Each
    // probe source id is a completion signal: the worker records it before
    // invoking the driver. Sleep out the 500ms/1000ms exponential backoff
    // between attempts.
    let mut processed = true;
    for (index, source) in [
        "__acceptance_failure_1",
        "__acceptance_failure_2",
        "__acceptance_failure_3",
    ]
    .iter()
    .enumerate()
    {
        if index > 0 {
            std::thread::sleep(ATTEMPT_BACKOFF);
        }
        if !submit_and_await_attempt(&handle, source, FAILURE_WAIT) {
            processed = false;
            break;
        }
    }
    let alive = !handle.is_finished();
    if !processed {
        let _ = hub.stop(&handle);
        return check(
            "worker.read_failure",
            Level::Skip,
            format!("worker did not process the failing reads (read_err={timed_out:?}, worker_alive={alive})"),
            serde_json::json!({
                "read_error": timed_out,
                "worker_alive": alive,
                "config": "NUL byte in driver_options",
            }),
        );
    }

    let Some(first_failures) = wait_for_failures(&handle, 0, FAILURE_WAIT) else {
        let _ = hub.stop(&handle);
        return check(
            "worker.read_failure",
            Level::Skip,
            format!("adapter did not surface a read failure (read_err={timed_out:?}, worker_alive={alive})"),
            serde_json::json!({
                "read_error": timed_out,
                "worker_alive": alive,
                "config": "NUL byte in driver_options",
            }),
        );
    };

    // A second read after the retry budget is exhausted fails immediately and
    // must still increment the counter: the worker kept processing.
    let second_processed = submit_and_await_attempt(&handle, "__acceptance_failure_4", FAILURE_WAIT);
    let second_failures = second_processed
        .then(|| wait_for_failures(&handle, first_failures, FAILURE_WAIT))
        .flatten();
    let alive = !handle.is_finished();
    let _ = hub.stop(&handle);

    match second_failures {
        Some(second) if alive => check(
            "worker.read_failure",
            Level::Pass,
            format!(
                "failed reads registered (total_failures {first_failures} -> {second}) and the worker stayed alive={alive}"
            ),
            serde_json::json!({
                "first_failures": first_failures,
                "second_failures": second,
                "read_error": timed_out,
                "worker_alive": alive,
                "config": "NUL byte in driver_options",
            }),
        ),
        Some(second) => check(
            "worker.read_failure",
            Level::Fail,
            format!(
                "failures were processed (total_failures {first_failures} -> {second}) but the worker exited (worker_alive=false)"
            ),
            serde_json::json!({
                "first_failures": first_failures,
                "second_failures": second,
                "read_error": timed_out,
                "worker_alive": alive,
            }),
        ),
        None => check(
            "worker.read_failure",
            Level::Fail,
            format!(
                "second read did not add a failure (total_failures stayed at {first_failures}, worker_alive={alive})"
            ),
            serde_json::json!({
                "first_failures": first_failures,
                "worker_alive": alive,
                "read_error": timed_out,
            }),
        ),
    }
}

/// Submit a read with a unique source id and wait until the worker records it
/// (`last_read_sources` is written before the driver call). Returns false when
/// the attempt is not observed within `timeout`.
fn submit_and_await_attempt(handle: &WorkerHandle, source: &str, timeout: Duration) -> bool {
    let probe = vec![source.to_string()];
    handle.submit_read(probe.clone());
    let deadline = Instant::now() + timeout;
    loop {
        if handle.last_read_sources() == probe {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Poll the worker overview until `total_failures` exceeds `above`. Counter
/// updates are asynchronous, so poll with a bounded deadline instead of
/// sleeping a fixed amount.
fn wait_for_failures(handle: &WorkerHandle, above: u64, timeout: Duration) -> Option<u64> {
    let deadline = Instant::now() + timeout;
    loop {
        let total = handle.status().health.total_failures;
        if total > above {
            return Some(total);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn finish(hub: TestHub, driver: String, mut checks: Vec<CheckResult>) -> ProfileReport {
    let result = hub.unload(&driver);
    checks.push(check(
        "driver.unload",
        if result.is_ok() { Level::Pass } else { Level::Fail },
        match &result {
            Ok(()) => "registry entry removed".to_string(),
            Err(err) => err.clone(),
        },
        serde_json::json!({"workspace": hub.workspace_id()}),
    ));
    ProfileReport {
        profile: PROFILE.into(),
        checks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn demo_package_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../drivers/demo-declarative")
    }

    fn demo_built() -> bool {
        let target = std::env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"));
        let profile = if cfg!(debug_assertions) { "debug" } else { "release" };
        target.join(profile).join("libdemo_declarative.dylib").is_file()
            || target.join(profile).join("libdemo_declarative.so").is_file()
    }

    fn level_of(report: &ProfileReport, name: &str) -> Option<Level> {
        report.checks.iter().find(|c| c.check == name).map(|c| c.level)
    }

    #[test]
    fn demo_package_passes_lifecycle() {
        if !demo_built() {
            eprintln!("skipping lifecycle test: cargo build -p demo-declarative");
            return;
        }
        let report = run(&demo_package_dir());
        assert_eq!(report.level(), Level::Pass, "{report:?}");
        for name in [
            "package.load",
            "worker.spawn",
            "worker.read",
            "worker.online",
            "worker.stop",
            "worker.reconnect",
            "worker.read_failure",
            "driver.unload",
        ] {
            assert_eq!(level_of(&report, name), Some(Level::Pass), "check {name}: {report:?}");
        }
    }

    #[test]
    fn missing_cdylib_fails_with_build_guidance() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("driver.manifest.json"),
            r#"{
              "manifestVersion":"1.0",
              "identity":{"driver":"demo","version":"1.0.0","protocol":"demo","abiVersion":2,"dynamicCapabilities":false},
              "config":{"type":"object"},"tools":[],"stateFields":[],
              "sources":[{"id":"telemetry","kind":"poll","fields":[]}],
              "resources":[],"placement":{"mode":"runtime"},
              "limits":{"maxConcurrentJobs":1,"maxCallsPerMin":60}
            }"#,
        )
        .unwrap();
        let report = run(dir.path());
        assert_eq!(report.level(), Level::Fail);
        let load = report.checks.iter().find(|c| c.check == "package.load").unwrap();
        assert_eq!(load.level, Level::Fail);
        assert!(load.detail.contains("cargo build -p demo"), "{}", load.detail);
    }
}
