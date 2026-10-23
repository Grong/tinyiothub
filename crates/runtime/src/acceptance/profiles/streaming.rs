//! `streaming` profile: prove the sampler's demand gating against the package's
//! own manifest sources. Everything is deterministic — the sampler is driven by
//! [`Sampler::tick_once`] with injected `Instant`s and fake workers, so there
//! are no sleeps and no timing flakiness.
//!
//! Checks:
//! - `streaming.quiet`: with no demand, always-on sources (`health: true` or
//!   plain non-`demandOnly` poll sources) are submitted while the
//!   `demandOnly` source is not.
//! - `streaming.subscription`: an explicit source subscription makes the next
//!   due tick submit the `demandOnly` source.
//! - `streaming.grace`: after dropping the subscription, the source keeps being
//!   sampled inside the 30s [`DEMAND_GRACE`] window.
//! - `streaming.stop`: past the grace window the `demandOnly` source stops and
//!   the always-on sources continue.
//! - `streaming.push_demand`: a package declaring a `push`/`stream` source gets
//!   `set_demand` updates for the active source set (never `submit_read`).
//!   Packages without one skip with a reason.
//! - `streaming.driver_source`: the real package `.so` is loaded into a
//!   [`TestHub`], a real worker is spawned, and a read of the `demandOnly`
//!   source must return its declared fields. This closes the gap between the
//!   deterministic sampler contract and the real driver; a push worker skips
//!   because reads are sink-driven.
//!
//! A package without a `demandOnly` source skips every check with a reason;
//! skip is never reported as a pass.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tinyiothub_core::driver::manifest::{DriverManifest, SourceDecl};

use crate::acceptance::report::{CheckResult, Level, ProfileReport};
use crate::acceptance::test_hub::{TestHub, ThingOptions};
use crate::access::catalog::{SampledThing, SamplingCatalog};
use crate::access::demand::{DemandTracker, SUBSCRIPTION_TTL};
use crate::access::sampler::{DEMAND_GRACE, Sampler, SamplingWorkers};

const PROFILE: &str = "streaming";
const WORKSPACE: &str = "acceptance-streaming";
const THING_ID: &str = "streaming-thing";
const CHECKS: &[&str] = &[
    "streaming.quiet",
    "streaming.subscription",
    "streaming.grace",
    "streaming.stop",
    "streaming.push_demand",
    "streaming.driver_source",
];
/// Bound for the real-worker read of the demand-gated source.
const DRIVER_READ_TIMEOUT: Duration = Duration::from_secs(5);
/// A first tick has no `due_at`, so the quiet tick submits the always-on
/// sources immediately; the next tick is one interval later.
const SUBSCRIBE_AFTER: Duration = Duration::from_secs(1);
/// 19s after the subscription is dropped: still inside the 30s grace.
const GRACE_AFTER_DROP: Duration = Duration::from_secs(19);
/// 31s after the subscription is dropped: the grace window has expired.
const STOP_AFTER_DROP: Duration = Duration::from_secs(31);

fn check(name: &str, level: Level, detail: impl Into<String>, evidence: serde_json::Value) -> CheckResult {
    CheckResult {
        profile: PROFILE.into(),
        check: name.into(),
        level,
        detail: detail.into(),
        evidence,
    }
}

fn skip(name: &str, reason: &str, evidence: serde_json::Value) -> CheckResult {
    check(name, Level::Skip, reason.to_string(), evidence)
}

fn skip_all(reason: &str) -> Vec<CheckResult> {
    CHECKS
        .iter()
        .map(|name| skip(name, reason, serde_json::json!({"reason": reason})))
        .collect()
}

/// The sampler samples `poll` and `position` sources; anything else never
/// reaches the worker's read path.
fn is_sampled_kind(kind: &str) -> bool {
    kind == "poll" || kind == "position"
}

pub fn run(dir: &Path) -> ProfileReport {
    let manifest = match crate::access::package::read_package(dir) {
        Ok(pkg) => pkg.manifest,
        Err(err) => {
            return ProfileReport {
                profile: PROFILE.into(),
                checks: vec![check(
                    "streaming.load",
                    Level::Fail,
                    err.to_string(),
                    serde_json::json!({"error": err.to_string()}),
                )],
            };
        }
    };

    let mut checks = streaming_checks(&manifest);
    if let Some(demand_only) = manifest.sources.iter().find(|source| source.demand_only) {
        checks.push(driver_source_check(dir, demand_only));
    }
    ProfileReport {
        profile: PROFILE.into(),
        checks,
    }
}

fn streaming_checks(manifest: &DriverManifest) -> Vec<CheckResult> {
    let Some(demand_only) = manifest.sources.iter().find(|source| source.demand_only) else {
        return skip_all("manifest declares no demandOnly source; demand gating cannot be exercised");
    };

    let mut checks = if is_sampled_kind(&demand_only.kind) {
        read_gating_checks(manifest, demand_only)
    } else {
        let reason = format!(
            "demandOnly source '{}' has kind '{}'; the sampler only reads poll/position sources",
            demand_only.id, demand_only.kind
        );
        [
            "streaming.quiet",
            "streaming.subscription",
            "streaming.grace",
            "streaming.stop",
        ]
        .iter()
        .map(|name| {
            skip(
                name,
                &reason,
                serde_json::json!({"source": demand_only.id, "kind": demand_only.kind}),
            )
        })
        .collect()
    };
    checks.push(push_demand_check(manifest, demand_only));
    checks
}

/// Record submitted reads and `set_demand` calls; `push` selects whether the
/// sampler treats the thing as a push worker.
#[derive(Default)]
struct RecordingWorkers {
    reads: Mutex<Vec<(String, Vec<String>)>>,
    demands: Mutex<Vec<(String, Vec<String>)>>,
    push: bool,
}

impl RecordingWorkers {
    fn push() -> Self {
        Self {
            push: true,
            ..Self::default()
        }
    }

    fn read_count(&self) -> usize {
        self.reads.lock().unwrap().len()
    }

    /// Source sets submitted since `from` (one entry per `submit_read`).
    fn reads_since(&self, from: usize) -> Vec<Vec<String>> {
        self.reads
            .lock()
            .unwrap()
            .get(from..)
            .unwrap_or_default()
            .iter()
            .map(|(_, sources)| sources.clone())
            .collect()
    }

    fn demands_since(&self, from: usize) -> Vec<Vec<String>> {
        self.demands
            .lock()
            .unwrap()
            .get(from..)
            .unwrap_or_default()
            .iter()
            .map(|(_, sources)| sources.clone())
            .collect()
    }

    fn demand_count(&self) -> usize {
        self.demands.lock().unwrap().len()
    }
}

impl SamplingWorkers for RecordingWorkers {
    fn submit_read(&self, thing_id: &str, sources: Vec<String>) {
        self.reads.lock().unwrap().push((thing_id.to_string(), sources));
    }

    fn set_demand(&self, thing_id: &str, sources: Vec<String>) {
        self.demands.lock().unwrap().push((thing_id.to_string(), sources));
    }

    fn is_push(&self, _thing_id: &str) -> bool {
        self.push
    }
}

struct ManifestCatalog {
    thing: SampledThing,
}

impl SamplingCatalog for ManifestCatalog {
    fn list(&self) -> Vec<SampledThing> {
        vec![self.thing.clone()]
    }
}

fn build_sampler(manifest: &DriverManifest, workers: Arc<RecordingWorkers>) -> (Sampler, Arc<DemandTracker>) {
    let thing = SampledThing {
        workspace_id: WORKSPACE.into(),
        thing_id: THING_ID.into(),
        driver_name: manifest.identity.driver.clone(),
        sources: manifest.sources.clone(),
    };
    let demand = Arc::new(DemandTracker::new());
    let sampler = Sampler::new(Arc::new(ManifestCatalog { thing }), demand.clone(), workers, None);
    (sampler, demand)
}

fn subscribe(demand: &DemandTracker, source: &str, at: Instant) -> u64 {
    demand.subscribe_at(
        WORKSPACE,
        &[THING_ID.to_string()],
        &[source.to_string()],
        SUBSCRIPTION_TTL,
        at,
    )
}

/// The always-on source whose continuity proves health/plain telemetry keeps
/// sampling while the demand-only source stops. `health: true` wins; otherwise
/// any non-demand-only sampled source.
fn continuous_source(manifest: &DriverManifest) -> Option<&SourceDecl> {
    manifest
        .sources
        .iter()
        .find(|source| source.health && is_sampled_kind(&source.kind))
        .or_else(|| {
            manifest
                .sources
                .iter()
                .find(|source| !source.demand_only && is_sampled_kind(&source.kind))
        })
}

fn contains(batches: &[Vec<String>], source: &str) -> bool {
    batches.iter().any(|batch| batch.iter().any(|id| id == source))
}

fn read_gating_checks(manifest: &DriverManifest, demand_only: &SourceDecl) -> Vec<CheckResult> {
    let workers = Arc::new(RecordingWorkers::default());
    let (sampler, demand) = build_sampler(manifest, workers.clone());
    let base = Instant::now();
    let continuous = continuous_source(manifest);
    let mut checks = Vec::new();

    // a) No demand: always-on sources sample, the demand-only source does not.
    let before = workers.read_count();
    sampler.tick_once(base);
    let quiet = workers.reads_since(before);
    let quiet_evidence = serde_json::json!({
        "demandOnly": demand_only.id,
        "continuous": continuous.map(|source| source.id.clone()),
        "submitted": quiet,
    });
    checks.push(match continuous {
        None => skip(
            "streaming.quiet",
            "manifest declares no always-on sampled source; continuity cannot be asserted",
            quiet_evidence,
        ),
        Some(source) if !contains(&quiet, &demand_only.id) && contains(&quiet, &source.id) => check(
            "streaming.quiet",
            Level::Pass,
            format!(
                "no demand: '{}' was not submitted while always-on sources continued",
                demand_only.id
            ),
            quiet_evidence,
        ),
        Some(_) => check(
            "streaming.quiet",
            Level::Fail,
            format!("no demand: expected only always-on sources, got {:?}", quiet),
            quiet_evidence,
        ),
    });

    // b) Subscription demand: the next due tick submits the demand-only source.
    let subscription = subscribe(&demand, &demand_only.id, base);
    let before = workers.read_count();
    sampler.tick_once(base + SUBSCRIBE_AFTER);
    let subscribed = workers.reads_since(before);
    let subscribed_ok = contains(&subscribed, &demand_only.id);
    checks.push(check(
        "streaming.subscription",
        if subscribed_ok { Level::Pass } else { Level::Fail },
        if subscribed_ok {
            format!("subscription demand submitted '{}'", demand_only.id)
        } else {
            format!(
                "subscription to '{}' did not submit it: {:?}",
                demand_only.id, subscribed
            )
        },
        serde_json::json!({
            "source": demand_only.id,
            "submitted": subscribed,
        }),
    ));

    // c) Grace: dropping the subscription keeps sampling for 30s.
    demand.unsubscribe(&subscription);
    let dropped_at = base + SUBSCRIBE_AFTER;
    let before = workers.read_count();
    sampler.tick_once(dropped_at + GRACE_AFTER_DROP);
    let grace = workers.reads_since(before);
    let grace_ok = contains(&grace, &demand_only.id);
    checks.push(check(
        "streaming.grace",
        if grace_ok { Level::Pass } else { Level::Fail },
        if grace_ok {
            format!(
                "'{}' still sampled {}s after demand disappeared (grace {}s)",
                demand_only.id,
                GRACE_AFTER_DROP.as_secs(),
                DEMAND_GRACE.as_secs()
            )
        } else {
            format!("'{}' stopped inside the grace window: {:?}", demand_only.id, grace)
        },
        serde_json::json!({
            "source": demand_only.id,
            "elapsedAfterDropMs": GRACE_AFTER_DROP.as_millis() as u64,
            "graceMs": DEMAND_GRACE.as_millis() as u64,
            "submitted": grace,
        }),
    ));

    // d) Stop: past the grace window the source stops, always-on sources go on.
    let before = workers.read_count();
    sampler.tick_once(dropped_at + STOP_AFTER_DROP);
    let stopped = workers.reads_since(before);
    let stop_evidence = serde_json::json!({
        "source": demand_only.id,
        "elapsedAfterDropMs": STOP_AFTER_DROP.as_millis() as u64,
        "graceMs": DEMAND_GRACE.as_millis() as u64,
        "continuous": continuous.map(|source| source.id.clone()),
        "submitted": stopped,
    });
    checks.push(match continuous {
        None => skip(
            "streaming.stop",
            "manifest declares no always-on sampled source; continuity cannot be asserted",
            stop_evidence,
        ),
        Some(source) if !contains(&stopped, &demand_only.id) && contains(&stopped, &source.id) => check(
            "streaming.stop",
            Level::Pass,
            format!(
                "'{}' stopped after the grace window while '{}' continued",
                demand_only.id, source.id
            ),
            stop_evidence,
        ),
        Some(_) => check(
            "streaming.stop",
            Level::Fail,
            format!(
                "past the grace window: expected '{}' to stop, got {:?}",
                demand_only.id, stopped
            ),
            stop_evidence,
        ),
    });

    checks
}

/// Push/stream drivers own the read loop, so the sampler only pushes the active
/// source set through `set_demand`. A package without a push/stream source has
/// no such path to exercise; it skips with a reason.
fn push_demand_check(manifest: &DriverManifest, demand_only: &SourceDecl) -> CheckResult {
    let Some(push_source) = manifest
        .sources
        .iter()
        .find(|source| matches!(source.kind.as_str(), "push" | "stream"))
    else {
        return skip(
            "streaming.push_demand",
            "manifest declares no push/stream source; set_demand is not exercised",
            serde_json::json!({"sources": manifest.sources.iter().map(|s| s.id.clone()).collect::<Vec<_>>()}),
        );
    };

    let workers = Arc::new(RecordingWorkers::push());
    let (sampler, demand) = build_sampler(manifest, workers.clone());
    let base = Instant::now();

    let before = workers.demand_count();
    sampler.tick_once(base);
    let initial = workers.demands_since(before);

    subscribe(&demand, &demand_only.id, base);
    let before = workers.demand_count();
    sampler.tick_once(base + SUBSCRIBE_AFTER);
    let demanded = workers.demands_since(before);

    let quiet_ok = !initial.iter().any(|set| set.iter().any(|id| id == &demand_only.id));
    let demanded_ok = demanded.iter().any(|set| set.iter().any(|id| id == &demand_only.id));
    let level = if quiet_ok && demanded_ok && workers.read_count() == 0 {
        Level::Pass
    } else {
        Level::Fail
    };
    check(
        "streaming.push_demand",
        level,
        if level == Level::Pass {
            format!(
                "push source '{}' received set_demand for the active set; no reads were submitted",
                push_source.id
            )
        } else {
            format!(
                "push demand gating failed: initial={initial:?} afterSubscribe={demanded:?} reads={}",
                workers.read_count()
            )
        },
        serde_json::json!({
            "pushSource": push_source.id,
            "demandOnly": demand_only.id,
            "initial": initial,
            "afterSubscribe": demanded,
            "submittedReads": workers.read_count(),
        }),
    )
}

/// Real-package evidence: spawn the packaged driver in a [`TestHub`] and read
/// only the demand-gated source. The deterministic checks above prove sampler
/// gating; this proves the `.so` actually implements the source it declares.
fn driver_source_check(dir: &Path, demand_only: &SourceDecl) -> CheckResult {
    let hub = TestHub::new();
    let loaded = match hub.load_package(dir) {
        Ok(loaded) => loaded,
        Err(err) => {
            return check(
                "streaming.driver_source",
                Level::Fail,
                format!("real driver load failed: {err}"),
                serde_json::json!({"error": err}),
            );
        }
    };
    let thing = hub.make_thing(&loaded, ThingOptions::default());
    let handle = match hub.spawn(&loaded.driver, thing.clone()) {
        Ok(handle) => handle,
        Err(err) => {
            let _ = hub.unload(&loaded.driver);
            return check(
                "streaming.driver_source",
                Level::Fail,
                format!("real worker spawn failed: {err}"),
                serde_json::json!({"error": err}),
            );
        }
    };

    if handle.is_push() {
        let _ = hub.unload(&loaded.driver);
        return skip(
            "streaming.driver_source",
            "push worker: reads are sink-driven; demand is exercised through set_demand",
            serde_json::json!({"source": demand_only.id, "push": true}),
        );
    }

    let result = hub.read_sources_and_wait(&handle, vec![demand_only.id.clone()], DRIVER_READ_TIMEOUT);
    let _ = hub.unload(&loaded.driver);

    match result {
        Ok(values) => {
            let missing: Vec<&String> = demand_only
                .fields
                .iter()
                .filter(|field| !values.iter().any(|value| &value.name == *field))
                .collect();
            let level = if missing.is_empty() && !values.is_empty() {
                Level::Pass
            } else {
                Level::Fail
            };
            check(
                "streaming.driver_source",
                level,
                if level == Level::Pass {
                    format!(
                        "real worker read demand-gated source '{}' and returned its declared field(s)",
                        demand_only.id
                    )
                } else {
                    format!(
                        "real worker read of '{}' is missing declared field(s): {missing:?}",
                        demand_only.id
                    )
                },
                serde_json::json!({
                    "source": demand_only.id,
                    "declared": demand_only.fields,
                    "values": values.iter().map(|value| value.name.clone()).collect::<Vec<_>>(),
                    "binary": loaded.path.display().to_string(),
                }),
            )
        }
        Err(err) => check(
            "streaming.driver_source",
            Level::Fail,
            format!("real worker read of '{}' failed: {err}", demand_only.id),
            serde_json::json!({
                "source": demand_only.id,
                "error": err,
                "binary": loaded.path.display().to_string(),
            }),
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tinyiothub_core::driver::manifest::validate_manifest;

    use super::*;

    fn demo_dir() -> PathBuf {
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

    fn demo_manifest() -> DriverManifest {
        crate::access::package::read_package_raw(&demo_dir())
            .expect("read demo manifest")
            .1
    }

    fn level_of(checks: &[CheckResult], name: &str) -> Option<Level> {
        checks.iter().find(|c| c.check == name).map(|c| c.level)
    }

    fn synthetic_manifest(sources: serde_json::Value) -> DriverManifest {
        let mut value: serde_json::Value = serde_json::from_str(
            r#"{
              "manifestVersion":"1.0",
              "identity":{"driver":"demo","version":"1.0.0","protocol":"demo","abiVersion":2,"dynamicCapabilities":false},
              "config":{"type":"object"},"tools":[],
              "stateFields":[{"name":"level","type":"integer","required":true}],
              "sources":[],
              "resources":[],"placement":{"mode":"runtime"},
              "limits":{"maxConcurrentJobs":1,"maxCallsPerMin":60}
            }"#,
        )
        .unwrap();
        value["sources"] = sources;
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn demo_manifest_declares_level2_and_the_slow_source() {
        let manifest = demo_manifest();
        validate_manifest(&manifest).expect("demo manifest must stay valid");

        let level2 = manifest
            .state_fields
            .iter()
            .find(|field| field.name == "level2")
            .expect("level2 state field");
        assert_eq!(level2.data_type, "integer");
        assert!(level2.required);
        assert_eq!(level2.source.as_deref(), Some("slow"));
        let range = level2.range.as_ref().expect("level2 range");
        assert_eq!(range.min, Some(0.0));
        assert_eq!(range.max, Some(100.0));

        let slow = manifest
            .sources
            .iter()
            .find(|source| source.id == "slow")
            .expect("slow source");
        assert_eq!(slow.kind, "poll");
        assert_eq!(slow.fields, vec!["level2".to_string()]);
        assert!(slow.demand_only);
        assert!(!slow.health);
        assert_eq!(slow.interval_ms, 2000);
        assert_eq!(slow.min_interval_ms, 500);

        let telemetry = manifest
            .sources
            .iter()
            .find(|source| source.id == "telemetry")
            .expect("telemetry source");
        assert!(telemetry.health && !telemetry.demand_only);
    }

    #[test]
    fn demo_streaming_profile_passes_and_skips_push() {
        if !demo_built() {
            eprintln!("skipping streaming profile test: cargo build -p demo-declarative");
            return;
        }
        let report = run(&demo_dir());
        assert_eq!(report.profile, "streaming");
        assert_eq!(report.level(), Level::Pass, "{report:?}");
        for name in [
            "streaming.quiet",
            "streaming.subscription",
            "streaming.grace",
            "streaming.stop",
            "streaming.driver_source",
        ] {
            assert_eq!(level_of(&report.checks, name), Some(Level::Pass), "{name}: {report:?}");
        }
        let push = report
            .checks
            .iter()
            .find(|c| c.check == "streaming.push_demand")
            .expect("push demand check");
        assert_eq!(push.level, Level::Skip, "{push:?}");
        assert!(push.detail.contains("no push/stream source"), "{}", push.detail);
    }

    #[test]
    fn grace_boundary_keeps_sampling_then_stops() {
        let checks = streaming_checks(&demo_manifest());
        let grace = checks
            .iter()
            .find(|c| c.check == "streaming.grace")
            .expect("grace check");
        let batches = grace.evidence["submitted"].as_array().expect("batches");
        assert!(
            batches
                .iter()
                .any(|batch| batch.as_array().is_some_and(|ids| ids.iter().any(|id| id == "slow"))),
            "slow must be sampled 19s after dropping demand: {grace:?}"
        );
        assert_eq!(grace.evidence["elapsedAfterDropMs"], serde_json::json!(19_000));

        let stop = checks.iter().find(|c| c.check == "streaming.stop").expect("stop check");
        let batches = stop.evidence["submitted"].as_array().expect("batches");
        assert!(
            batches
                .iter()
                .all(|batch| batch.as_array().is_some_and(|ids| !ids.iter().any(|id| id == "slow"))),
            "slow must stop 31s after dropping demand: {stop:?}"
        );
        assert!(
            batches.iter().any(|batch| batch
                .as_array()
                .is_some_and(|ids| ids.iter().any(|id| id == "telemetry"))),
            "telemetry must keep sampling: {stop:?}"
        );
        assert_eq!(stop.evidence["elapsedAfterDropMs"], serde_json::json!(31_000));
    }

    #[test]
    fn manifest_without_demand_only_source_skips_every_check_with_a_reason() {
        let manifest = synthetic_manifest(serde_json::json!([
            {"id":"telemetry","kind":"poll","fields":["level"],"intervalMs":1000,"health":true}
        ]));
        let checks = streaming_checks(&manifest);
        assert_eq!(checks.len(), CHECKS.len());
        for check in &checks {
            assert_eq!(check.level, Level::Skip, "{check:?}");
            assert!(!check.detail.is_empty(), "{check:?}");
            assert!(check.detail.contains("no demandOnly source"), "{check:?}");
        }
    }

    #[test]
    fn demand_only_without_an_always_on_sampled_source_skips_continuity() {
        let manifest = synthetic_manifest(serde_json::json!([
            {"id":"slow","kind":"poll","fields":["level"],"demandOnly":true,"intervalMs":1000},
            {"id":"events","kind":"push","fields":["level"],"health":true}
        ]));
        // A health push source never reaches `submit_read`, so it cannot prove
        // continuity.
        assert!(continuous_source(&manifest).is_none());
        let slow = manifest.sources.iter().find(|s| s.id == "slow").unwrap();
        let checks = read_gating_checks(&manifest, slow);
        for name in ["streaming.quiet", "streaming.stop"] {
            let result = checks.iter().find(|c| c.check == name).expect(name);
            assert_eq!(result.level, Level::Skip, "{name}: {result:?}");
            assert!(result.detail.contains("no always-on sampled source"), "{result:?}");
        }
        assert_eq!(level_of(&checks, "streaming.subscription"), Some(Level::Pass));
        assert_eq!(level_of(&checks, "streaming.grace"), Some(Level::Pass));
    }

    #[test]
    fn push_source_gets_set_demand_and_never_submits_reads() {
        let manifest = synthetic_manifest(serde_json::json!([
            {"id":"events","kind":"push","fields":["level"]},
            {"id":"slow","kind":"poll","fields":["level"],"demandOnly":true,"intervalMs":1000}
        ]));
        let slow = manifest.sources.iter().find(|s| s.id == "slow").unwrap();
        let result = push_demand_check(&manifest, slow);
        assert_eq!(result.level, Level::Pass, "{result:?}");
        assert_eq!(result.evidence["submittedReads"], serde_json::json!(0));
    }
}
