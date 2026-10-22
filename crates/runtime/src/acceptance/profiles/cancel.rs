//! `cancel` profile (E4c spec §2.5): a stop request cooperatively cancels an
//! in-flight long call within the 1s grace, a driver that ignores the token is
//! recorded as `cancelled_unknown`, the stop pipeline returns immediately and
//! leaves a `CommandCancelled` event + `call_cancelled` ledger row, and the
//! stop gate rejects subsequent claims (`busy: stop:<reason> holds __device__`).
//!
//! These checks exercise the real worker/registry/DataServer with fake
//! drivers, so they are package-independent. The non-cooperative scenario
//! blocks for 1.2s — no longer than the grace window it is designed to exceed.
//! Cloud `StopTool` composition (auth/approval/response shape) is Task 7's job.

use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc as std_mpsc;
use std::time::{Duration, Instant};

use tinyiothub_core::driver::{CancelToken, DriverMode, PropertyChangeSource, ResultValue, RetryConfig, ThingDriver};
use tinyiothub_core::error::Error;
use tinyiothub_core::models::event::Event;
use tinyiothub_core::models::thing::Thing;
use tinyiothub_core::models::thing_command::ThingCommand;

use crate::DataServer;
use crate::acceptance::report::{CheckResult, Level, ProfileReport};
use crate::arbitration::{ArbitrationRegistry, Busy, DEVICE_RESOURCE};
use crate::driver::worker::{WorkerHandle, WorkerResult};
use crate::driver::{DriverWrapper, spawn_worker};
use crate::ports::{HubLedger, ThingCacheSource};

const PROFILE: &str = "cancel";
const THING_ID: &str = "acceptance-cancel-thing";
const GRACE_MS: u64 = 1_000;
/// Non-cooperative driver ignores the token for this long (> 1s grace);
/// 1.5s leaves a 500ms margin so load-induced jitter cannot flip the outcome.
const NON_COOPERATIVE_BLOCK: Duration = Duration::from_millis(1_500);

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
    let mut checks: Vec<CheckResult> = rt.block_on(async {
        vec![
            cooperative_check().await,
            pipeline_check().await,
            non_cooperative_check().await,
        ]
    });
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

/// End-to-end stop pipeline (minus the cloud `StopTool` wrapper): gate + cancel
/// returns while the long call is still running, then the worker's late result
/// is turned into one `CommandCancelled` event and one `call_cancelled` ledger
/// row by the DataServer consumer.
async fn pipeline_check() -> CheckResult {
    use tinyiothub_core::models::event::{EventType, ThingEventType};

    let thing = Thing {
        id: THING_ID.to_string(),
        name: THING_ID.to_string(),
        workspace_id: Some("ws-acceptance".to_string()),
        ..Thing::default()
    };
    let cache: Arc<dyn ThingCacheSource> = Arc::new(SingleThingCache::new(thing.clone()));
    let bus = Arc::new(crate::event_bus::EventBus::new());
    let server = DataServer::new(cache, bus.clone());
    let ledger = Arc::new(RecordingLedger::default());
    server.set_hub_ledger(ledger.clone());
    let mut events = bus.subscribe();
    // Keep the sender alive so the consumer loop is not shut down mid-check.
    let (_shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel(1);

    let (release, release_rx) = std_mpsc::channel();
    let (started_tx, started) = std_mpsc::channel();
    server.register_worker(
        THING_ID,
        DriverWrapper::new(Box::new(BlockingDriver {
            thing: thing.clone(),
            cooperative: true,
            block: Duration::ZERO,
            release: std::sync::Mutex::new(release_rx),
            started: started_tx,
        })),
    );
    if let Err(e) = server.run(shutdown_rx).await {
        return check(
            "cancel.pipeline",
            Level::Fail,
            format!("DataServer consumer failed to start: {e}"),
            serde_json::Value::Null,
        );
    }
    let Some(worker) = server.workers().get(THING_ID) else {
        return check(
            "cancel.pipeline",
            Level::Fail,
            "worker was not registered",
            serde_json::Value::Null,
        );
    };

    let submitted = worker.submit_execute(command()).is_ok();
    let entered = submitted && started.recv_timeout(Duration::from_secs(2)).is_ok();

    let registry = ArbitrationRegistry::default();
    registry.set_stop(THING_ID, "acceptance stop".into(), Duration::from_secs(60));
    let stop_started = Instant::now();
    let in_flight_hit = server.cancel_calls(THING_ID);
    let stop_elapsed_ms = stop_started.elapsed().as_millis() as u64;
    // The driver is still blocked on `release` (not sent yet): returning at all
    // proves the stop path does not await the long call.
    let immediate = in_flight_hit == 1 && worker.in_flight() && stop_elapsed_ms < GRACE_MS;
    let gated = registry.stop_reason(THING_ID).as_deref() == Some("acceptance stop");

    let _ = release.send(());

    // Bounded await: the first CommandCancelled outcome must be `cancelled`.
    let event_ok = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            match events.recv().await {
                Ok(event) if event.event_type() == &EventType::Device(ThingEventType::CommandCancelled) => {
                    return event.content().metadata().get("outcome") == Some(&serde_json::json!("cancelled"));
                }
                Ok(_) => continue,
                Err(_) => return false,
            }
        }
    })
    .await
    .unwrap_or(false);

    // Bounded await: exactly one `call_cancelled` row.
    let ledger_seen = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if !ledger.rows().is_empty() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or(false);
    let rows = ledger.rows();
    let ledger_ok = ledger_seen
        && rows.len() == 1
        && rows[0].0 == "ws-acceptance"
        && rows[0].1 == "call_cancelled"
        && rows[0].2.as_deref() == Some(THING_ID)
        && rows[0].3["outcome"] == "cancelled";
    let counts = server.cancelled_counts();
    server.remove_device(THING_ID);

    let evidence = serde_json::json!({
        "submitted": submitted,
        "driver_entered": entered,
        "stop_gate": gated,
        "cancel_hits": in_flight_hit,
        "immediate": immediate,
        "stop_elapsed_ms": stop_elapsed_ms,
        "event_command_cancelled": event_ok,
        "ledger_rows": rows.len(),
        "ledger_ok": ledger_ok,
        "cancelled_counts": {"cancelled": counts.0, "cancelled_unknown": counts.1},
    });
    if submitted && entered && immediate && gated && event_ok && ledger_ok && counts == (1, 0) {
        check(
            "cancel.pipeline",
            Level::Pass,
            format!(
                "stop returned in {stop_elapsed_ms}ms while the call was in flight; CommandCancelled + call_cancelled recorded"
            ),
            evidence,
        )
    } else {
        check(
            "cancel.pipeline",
            Level::Fail,
            format!(
                "stop pipeline incomplete (immediate={immediate}, event={event_ok}, ledger={ledger_ok}, counts={counts:?})"
            ),
            evidence,
        )
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

/// Minimal one-thing cache for the DataServer pipeline check.
struct SingleThingCache(std::sync::Mutex<Thing>);

impl SingleThingCache {
    fn new(thing: Thing) -> Self {
        Self(std::sync::Mutex::new(thing))
    }
}

impl ThingCacheSource for SingleThingCache {
    fn all(&self) -> Vec<Thing> {
        vec![self.0.lock().unwrap().clone()]
    }

    fn get(&self, id: &str) -> Option<Thing> {
        let thing = self.0.lock().unwrap();
        (thing.id == id).then(|| thing.clone())
    }

    fn get_by_name(&self, name: &str) -> Option<Thing> {
        let thing = self.0.lock().unwrap();
        (thing.name == name).then(|| thing.clone())
    }

    fn insert(&self, device: Thing) {
        *self.0.lock().unwrap() = device;
    }

    fn update(&self, device: Thing) {
        *self.0.lock().unwrap() = device;
    }

    fn remove(&self, _id: &str) {}
}

/// Records `(workspace, kind, thing_id, payload)` ledger rows.
#[derive(Default)]
struct RecordingLedger(std::sync::Mutex<Vec<(String, String, Option<String>, serde_json::Value)>>);

impl RecordingLedger {
    fn rows(&self) -> Vec<(String, String, Option<String>, serde_json::Value)> {
        self.0.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl HubLedger for RecordingLedger {
    async fn append(
        &self,
        workspace_id: &str,
        kind: &str,
        thing_id: Option<&str>,
        payload: serde_json::Value,
    ) -> Result<(), String> {
        self.0.lock().unwrap().push((
            workspace_id.to_string(),
            kind.to_string(),
            thing_id.map(str::to_string),
            payload,
        ));
        Ok(())
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
        assert_eq!(report.checks.len(), 4);
        for name in [
            "cancel.cooperative",
            "cancel.pipeline",
            "cancel.non_cooperative",
            "cancel.stop_gate",
        ] {
            let check = report
                .checks
                .iter()
                .find(|c| c.check == name)
                .unwrap_or_else(|| panic!("{name}"));
            assert_eq!(check.level, Level::Pass, "{name}: {report:?}");
        }
        let pipeline = report.checks.iter().find(|c| c.check == "cancel.pipeline").unwrap();
        assert_eq!(pipeline.evidence["immediate"], true);
        assert_eq!(pipeline.evidence["event_command_cancelled"], true);
        assert_eq!(pipeline.evidence["ledger_ok"], true);
        assert_eq!(pipeline.evidence["ledger_rows"], 1);
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
