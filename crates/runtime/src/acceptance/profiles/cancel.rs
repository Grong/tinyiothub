//! `cancel` profile (E4c spec §2.5): a stop request cooperatively cancels an
//! in-flight long call within the 1s grace, a driver that ignores the token is
//! recorded as `cancelled_unknown`, and the stop gate rejects subsequent
//! claims (`busy: stop:<reason> holds __device__`).
//!
//! These checks exercise the real worker/registry with fake drivers, so they
//! are package-independent. The non-cooperative scenario blocks for 1.2s — no
//! longer than the grace window it is designed to exceed.

use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc as std_mpsc;
use std::time::Duration;

use tinyiothub_core::driver::{CancelToken, DriverMode, PropertyChangeSource, ResultValue, RetryConfig, ThingDriver};
use tinyiothub_core::error::Error;
use tinyiothub_core::models::event::Event;
use tinyiothub_core::models::thing::Thing;
use tinyiothub_core::models::thing_command::ThingCommand;

use crate::acceptance::report::{CheckResult, Level, ProfileReport};
use crate::arbitration::{ArbitrationRegistry, Busy, DEVICE_RESOURCE};
use crate::driver::worker::{WorkerHandle, WorkerResult};
use crate::driver::{DriverWrapper, spawn_worker};

const PROFILE: &str = "cancel";
const THING_ID: &str = "acceptance-cancel-thing";
const GRACE_MS: u64 = 1_000;
/// Non-cooperative driver ignores the token for this long (> 1s grace).
const NON_COOPERATIVE_BLOCK: Duration = Duration::from_millis(1_200);

fn check(name: &str, level: Level, detail: impl Into<String>, evidence: serde_json::Value) -> CheckResult {
    CheckResult {
        profile: PROFILE.into(),
        check: name.into(),
        level,
        detail: detail.into(),
        evidence,
    }
}

pub fn run(_dir: &Path) -> ProfileReport {
    let rt = tokio::runtime::Runtime::new().expect("build acceptance runtime");
    let mut checks: Vec<CheckResult> =
        rt.block_on(async { vec![cooperative_check().await, non_cooperative_check().await] });
    checks.push(gate_check());
    ProfileReport {
        profile: PROFILE.into(),
        checks,
    }
}

async fn cooperative_check() -> CheckResult {
    let mut harness = Harness::spawn(true, Duration::ZERO);
    let detail = run_cancel(&mut harness).await;
    harness.handle.stop();
    match detail {
        Ok(CancelledOutcome {
            requested,
            within_grace,
            elapsed_ms,
        }) => {
            let evidence = serde_json::json!({
                "cancel_requested": requested,
                "within_grace": within_grace,
                "elapsed_ms": elapsed_ms,
                "grace_ms": GRACE_MS,
            });
            if requested && within_grace && elapsed_ms <= GRACE_MS {
                check(
                    "cancel.cooperative",
                    Level::Pass,
                    format!("in-flight call cancelled in {elapsed_ms}ms (<= {GRACE_MS}ms grace)"),
                    evidence,
                )
            } else {
                check(
                    "cancel.cooperative",
                    Level::Fail,
                    format!(
                        "cooperative cancel missed the grace window (requested={requested}, within_grace={within_grace}, elapsed_ms={elapsed_ms})"
                    ),
                    evidence,
                )
            }
        }
        Err(e) => check("cancel.cooperative", Level::Fail, e, serde_json::Value::Null),
    }
}

async fn non_cooperative_check() -> CheckResult {
    let mut harness = Harness::spawn(false, NON_COOPERATIVE_BLOCK);
    let detail = run_cancel(&mut harness).await;
    harness.handle.stop();
    match detail {
        Ok(CancelledOutcome {
            requested,
            within_grace,
            elapsed_ms,
        }) => {
            let evidence = serde_json::json!({
                "cancel_requested": requested,
                "within_grace": within_grace,
                "elapsed_ms": elapsed_ms,
                "grace_ms": GRACE_MS,
                "blocked_ms": NON_COOPERATIVE_BLOCK.as_millis() as u64,
            });
            if requested && !within_grace && elapsed_ms > GRACE_MS {
                check(
                    "cancel.non_cooperative",
                    Level::Pass,
                    format!("driver ignored cancel for {elapsed_ms}ms -> cancelled_unknown"),
                    evidence,
                )
            } else {
                check(
                    "cancel.non_cooperative",
                    Level::Fail,
                    format!(
                        "non-cooperative driver was not recorded as cancelled_unknown (requested={requested}, within_grace={within_grace}, elapsed_ms={elapsed_ms})"
                    ),
                    evidence,
                )
            }
        }
        Err(e) => check("cancel.non_cooperative", Level::Fail, e, serde_json::Value::Null),
    }
}

/// The stop gate rejects a claim after `set_stop` and the rejection carries the
/// stop reason (human-readable `busy:` text, same shape the tools surface).
fn gate_check() -> CheckResult {
    let registry = ArbitrationRegistry::default();
    registry.set_stop(THING_ID, "acceptance stop".into(), Duration::from_secs(60));
    let blocked = registry.try_acquire_resources(
        THING_ID,
        "acceptance:next".into(),
        &[crate::arbitration::ResourceSpec {
            id: DEVICE_RESOURCE.into(),
            mode: crate::arbitration::ResourceMode::Exclusive,
            capacity: 1,
        }],
        Duration::from_secs(60),
    );
    let reason = registry.stop_reason(THING_ID);
    let evidence = serde_json::json!({
        "stop_reason": reason,
        "busy": match &blocked {
            Err(busy) => serde_json::json!({"holder": busy.holder, "resource": busy.resource, "text": busy.to_string()}),
            Ok(_) => serde_json::json!(null),
        },
    });
    match blocked {
        Err(Busy { holder, resource }) if holder == "stop:acceptance stop" && resource == DEVICE_RESOURCE => check(
            "cancel.stop_gate",
            Level::Pass,
            format!("subsequent claim rejected: {holder} holds {resource}"),
            evidence,
        ),
        Err(other) => check(
            "cancel.stop_gate",
            Level::Fail,
            format!("gate rejected with the wrong identity: {other}"),
            evidence,
        ),
        Ok(_) => check(
            "cancel.stop_gate",
            Level::Fail,
            "claim succeeded while the stop gate was active",
            evidence,
        ),
    }
}

struct CancelledOutcome {
    requested: bool,
    within_grace: bool,
    elapsed_ms: u64,
}

/// Submit a long `Execute`, wait for the driver to enter it, cancel, release
/// the driver and read the single worker outcome.
async fn run_cancel(harness: &mut Harness) -> Result<CancelledOutcome, String> {
    harness
        .handle
        .submit_execute(command())
        .map_err(|e| format!("submit failed: {e}"))?;
    harness
        .started
        .recv_timeout(Duration::from_secs(2))
        .map_err(|_| "driver never entered the call".to_string())?;
    if !harness.handle.in_flight() {
        return Err("worker did not report an in-flight call".to_string());
    }
    let requested = harness.handle.cancel_current();
    harness
        .release
        .send(())
        .map_err(|_| "driver already released".to_string())?;

    let result = tokio::time::timeout(Duration::from_secs(5), harness.results.recv())
        .await
        .map_err(|_| "cancelled outcome did not arrive".to_string())?
        .ok_or_else(|| "worker result channel closed".to_string())?;
    match result {
        WorkerResult::Cancelled {
            thing_id,
            within_grace,
            elapsed_ms,
        } => {
            if thing_id != THING_ID {
                return Err(format!("unexpected thing id '{thing_id}'"));
            }
            Ok(CancelledOutcome {
                requested,
                within_grace,
                elapsed_ms,
            })
        }
        other => Err(format!("expected Cancelled, got {other:?}")),
    }
}

struct Harness {
    handle: WorkerHandle,
    release: std_mpsc::Sender<()>,
    started: std_mpsc::Receiver<()>,
    results: tokio::sync::mpsc::UnboundedReceiver<WorkerResult>,
}

impl Harness {
    fn spawn(cooperative: bool, block: Duration) -> Self {
        let (release, release_rx) = std_mpsc::channel();
        let (started_tx, started) = std_mpsc::channel();
        let thing = Thing {
            id: THING_ID.to_string(),
            name: THING_ID.to_string(),
            ..Thing::default()
        };
        let driver = DriverWrapper::new(Box::new(BlockingDriver {
            thing: thing.clone(),
            cooperative,
            block,
            release: std::sync::Mutex::new(release_rx),
            started: started_tx,
        }));
        let (sink_tx, _sink_rx) = tokio::sync::mpsc::channel(16);
        let (result_tx, results) = tokio::sync::mpsc::unbounded_channel();
        let handle = spawn_worker(thing, driver, sink_tx, Arc::new(NoEvents::new()), result_tx);
        Self {
            handle,
            release,
            started,
            results,
        }
    }
}

struct NoEvents(tokio::sync::broadcast::Sender<Event>);

impl NoEvents {
    fn new() -> Self {
        Self(tokio::sync::broadcast::channel(1).0)
    }
}

impl PropertyChangeSource for NoEvents {
    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<Event> {
        self.0.subscribe()
    }
}

/// Blocks inside the call until the test releases it. Cooperative: checks the
/// token. Non-cooperative: sleeps past the grace window, ignoring the token.
struct BlockingDriver {
    thing: Thing,
    cooperative: bool,
    block: Duration,
    release: std::sync::Mutex<std_mpsc::Receiver<()>>,
    started: std_mpsc::Sender<()>,
}

impl ThingDriver for BlockingDriver {
    fn thing(&self) -> &Thing {
        &self.thing
    }

    fn thing_mut(&mut self) -> &mut Thing {
        &mut self.thing
    }

    fn mode(&self) -> DriverMode {
        DriverMode::Poll
    }

    /// Long timeout: the blocking call must not be pre-empted by the retry
    /// layer (the cancel token, not the timeout, ends the call).
    fn retry_config(&self) -> RetryConfig {
        RetryConfig {
            max_attempts: 1,
            base_interval: Duration::from_millis(1),
            max_interval: Duration::from_millis(1),
            timeout: Duration::from_secs(300),
            ..RetryConfig::default()
        }
    }

    fn read_data(&mut self, _cancel: &CancelToken) -> Result<Vec<ResultValue>, Error> {
        Ok(Vec::new())
    }

    fn execute_command(&mut self, _cmd: &ThingCommand, cancel: &CancelToken) -> Result<bool, Error> {
        let _ = self.started.send(());
        let _ = self.release.lock().map(|release| release.recv());
        if self.cooperative {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
        } else {
            // Ignores the token: the worker can only report the late return.
            std::thread::sleep(self.block);
        }
        Ok(true)
    }
}

fn command() -> ThingCommand {
    ThingCommand {
        id: "acceptance-long-task".into(),
        thing_id: THING_ID.into(),
        name: "long_task".into(),
        display_name: None,
        description: None,
        parameters: None,
        created_at: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_profile_passes_against_fake_drivers() {
        let report = run(Path::new("."));
        assert_eq!(report.level(), Level::Pass, "{report:?}");
        assert_eq!(report.checks.len(), 3);
        for name in ["cancel.cooperative", "cancel.non_cooperative", "cancel.stop_gate"] {
            let check = report
                .checks
                .iter()
                .find(|c| c.check == name)
                .unwrap_or_else(|| panic!("{name}"));
            assert_eq!(check.level, Level::Pass, "{name}: {report:?}");
        }
        let non_coop = report
            .checks
            .iter()
            .find(|c| c.check == "cancel.non_cooperative")
            .unwrap();
        assert_eq!(non_coop.evidence["within_grace"], false);
        let gate = report.checks.iter().find(|c| c.check == "cancel.stop_gate").unwrap();
        assert!(gate.detail.contains("stop:acceptance stop"), "{gate:?}");
        assert!(gate.detail.contains(DEVICE_RESOURCE), "{gate:?}");
    }
}
