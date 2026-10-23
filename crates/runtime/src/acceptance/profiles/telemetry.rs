//! `telemetry` profile: verify the driver's declared telemetry contract against
//! a real package loaded into an in-memory [`TestHub`].
//!
//! Checks:
//! - `telemetry.required_fields`: one read; every `required` state field carries
//!   a value (missing optional fields warn).
//! - `telemetry.types`: the reading's `value_type` matches the declared type.
//! - `telemetry.range`: numeric readings fall inside the declared `min`/`max`.
//! - `telemetry.interval`: two reads spaced by the poll source's interval; the
//!   second reading must not be observed before the interval elapsed.
//! - `telemetry.push`: a push driver must deliver a reading through the value
//!   sink (populated from the hub's sink consumer); poll-only drivers skip.

use std::path::Path;
use std::time::{Duration, Instant};

use tinyiothub_core::driver::ResultValue;
use tinyiothub_core::driver::manifest::{DriverManifest, SourceDecl, StateFieldDecl};

use crate::acceptance::report::{CheckResult, Level, ProfileReport};
use crate::acceptance::test_hub::{TestHub, ThingOptions};

const PROFILE: &str = "telemetry";
/// A single poll read is near-instantaneous; CI scheduling may be noisy.
const READ_TIMEOUT: Duration = Duration::from_secs(5);
/// Bounded wait for a push driver's first value-sink reading.
const PUSH_TIMEOUT: Duration = Duration::from_secs(5);
/// Scheduling tolerance for the interval check.
const INTERVAL_SLACK: Duration = Duration::from_millis(50);

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
        Ok(loaded) => loaded,
        Err(err) => {
            checks.push(check(
                "telemetry.load",
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

    let push_declared = declares_push(&loaded.manifest);
    let poll_source = primary_poll_source(&loaded.manifest).cloned();

    if !push_declared && poll_source.is_none() {
        let reason = "manifest declares neither a poll source nor a push source; telemetry cannot be observed";
        for name in [
            "telemetry.required_fields",
            "telemetry.types",
            "telemetry.range",
            "telemetry.interval",
            "telemetry.push",
        ] {
            checks.push(check(name, Level::Skip, reason, serde_json::json!({})));
        }
        return ProfileReport {
            profile: PROFILE.into(),
            checks,
        };
    }

    let thing = hub.make_thing(&loaded, ThingOptions::default());
    let handle = match hub.spawn(&loaded.driver, thing.clone()) {
        Ok(handle) => handle,
        Err(err) => {
            checks.push(check(
                "telemetry.spawn",
                Level::Fail,
                err.clone(),
                serde_json::json!({"thing": thing.id, "error": err}),
            ));
            return ProfileReport {
                profile: PROFILE.into(),
                checks,
            };
        }
    };

    // Push drivers are serviced by `start_push` (a submitted poll read is
    // ignored by the worker), so acquire the first reading from the sink path.
    let first = if push_declared {
        wait_for_push_reading(&hub, &thing.id, PUSH_TIMEOUT)
    } else {
        hub.read_and_wait(&handle, READ_TIMEOUT)
    };
    let first_at = Instant::now();

    match &first {
        Ok(values) => {
            checks.push(required_fields_check(&loaded.manifest.state_fields, values));
            checks.push(types_check(&loaded.manifest.state_fields, values));
            checks.push(range_check(&loaded.manifest.state_fields, values));
        }
        Err(err) => {
            checks.push(check(
                "telemetry.required_fields",
                Level::Fail,
                format!("initial telemetry read failed: {err}"),
                serde_json::json!({"error": err}),
            ));
            checks.push(check(
                "telemetry.types",
                Level::Skip,
                "no reading to type-check",
                serde_json::json!({"reason": err}),
            ));
            checks.push(check(
                "telemetry.range",
                Level::Skip,
                "no reading to range-check",
                serde_json::json!({"reason": err}),
            ));
        }
    }

    checks.push(interval_check(
        &hub,
        &handle,
        poll_source.as_ref(),
        push_declared,
        first.is_ok(),
        first_at,
    ));
    checks.push(push_check(push_declared, &first));

    let _ = hub.stop(&handle);
    ProfileReport {
        profile: PROFILE.into(),
        checks,
    }
}

/// A push driver is declared by the manifest capability or a `push`/`stream`
/// source kind.
fn declares_push(manifest: &DriverManifest) -> bool {
    manifest.capabilities.push
        || manifest
            .sources
            .iter()
            .any(|source| matches!(source.kind.as_str(), "push" | "stream"))
}

/// The poll source whose interval the profile drives. Non-demand-only sources
/// win (the `slow` source added for streaming is demand-gated); fall back to
/// the first poll source.
fn primary_poll_source(manifest: &DriverManifest) -> Option<&SourceDecl> {
    manifest
        .sources
        .iter()
        .find(|source| source.kind == "poll" && !source.demand_only)
        .or_else(|| manifest.sources.iter().find(|source| source.kind == "poll"))
}

fn declared_required(fields: &[StateFieldDecl]) -> Vec<String> {
    fields
        .iter()
        .filter(|field| field.required)
        .map(|field| field.name.clone())
        .collect()
}

fn has_value(value: &ResultValue) -> bool {
    value.value.as_deref().is_some_and(|raw| !raw.trim().is_empty())
}

fn required_fields_check(fields: &[StateFieldDecl], values: &[ResultValue]) -> CheckResult {
    if fields.is_empty() {
        return check(
            "telemetry.required_fields",
            Level::Skip,
            "manifest declares no state fields",
            serde_json::json!({}),
        );
    }

    let mut missing_required = Vec::new();
    let mut missing_optional = Vec::new();
    for field in fields {
        let present = values
            .iter()
            .find(|value| value.name == field.name)
            .is_some_and(has_value);
        if present {
            continue;
        }
        if field.required {
            missing_required.push(field.name.clone());
        } else {
            missing_optional.push(field.name.clone());
        }
    }

    let required = declared_required(fields);
    let evidence = serde_json::json!({
        "required": required,
        "missing_required": missing_required,
        "missing_optional": missing_optional,
        "values": values.iter().map(|value| serde_json::json!({
            "name": value.name,
            "value_type": value.value_type,
            "value": value.value,
        })).collect::<Vec<_>>(),
    });

    if !missing_required.is_empty() {
        check(
            "telemetry.required_fields",
            Level::Fail,
            format!("reading is missing required field(s) without a value: {missing_required:?}"),
            evidence,
        )
    } else if !missing_optional.is_empty() {
        check(
            "telemetry.required_fields",
            Level::Warn,
            format!("reading omits optional field(s): {missing_optional:?}"),
            evidence,
        )
    } else {
        check(
            "telemetry.required_fields",
            Level::Pass,
            format!("all {} required field(s) carry a value", required.len()),
            evidence,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValueKind {
    Integer,
    Float,
    Boolean,
    String,
}

/// Canonical kind for a manifest `type`. Returns `None` for types this profile
/// cannot interpret (reported as `warn`, never a pass).
fn declared_kind(data_type: &str) -> Option<ValueKind> {
    match data_type.trim().to_ascii_lowercase().as_str() {
        "int" | "integer" | "i64" | "u64" => Some(ValueKind::Integer),
        "float" | "number" | "double" | "f32" | "f64" => Some(ValueKind::Float),
        "bool" | "boolean" => Some(ValueKind::Boolean),
        "string" | "str" | "text" => Some(ValueKind::String),
        _ => None,
    }
}

fn reading_type_matches(kind: ValueKind, value_type: &str) -> bool {
    matches!(
        (kind, value_type.trim().to_ascii_lowercase().as_str()),
        (ValueKind::Integer, "int" | "integer" | "i64" | "u64")
            | (ValueKind::Float, "float" | "number" | "double" | "f32" | "f64")
            | (ValueKind::Boolean, "bool" | "boolean")
            | (ValueKind::String, "string" | "str" | "text")
    )
}

fn types_check(fields: &[StateFieldDecl], values: &[ResultValue]) -> CheckResult {
    if fields.is_empty() {
        return check(
            "telemetry.types",
            Level::Skip,
            "manifest declares no state fields",
            serde_json::json!({}),
        );
    }

    let mut matched = Vec::new();
    let mut mismatched = Vec::new();
    let mut unknown = Vec::new();
    for field in fields {
        let Some(value) = values.iter().find(|value| value.name == field.name) else {
            continue;
        };
        match declared_kind(&field.data_type) {
            None => unknown.push(serde_json::json!({
                "field": field.name,
                "declared": field.data_type,
                "reading": value.value_type,
            })),
            Some(kind) if reading_type_matches(kind, &value.value_type) => matched.push(serde_json::json!({
                "field": field.name,
                "declared": field.data_type,
                "reading": value.value_type,
            })),
            Some(_) => mismatched.push(serde_json::json!({
                "field": field.name,
                "declared": field.data_type,
                "reading": value.value_type,
            })),
        }
    }

    let evidence = serde_json::json!({
        "matched": matched,
        "mismatched": mismatched,
        "unknown": unknown,
    });

    if !mismatched.is_empty() {
        check(
            "telemetry.types",
            Level::Fail,
            format!("reading type does not match the declaration: {mismatched:?}"),
            evidence,
        )
    } else if !unknown.is_empty() {
        check(
            "telemetry.types",
            Level::Warn,
            format!("declared type(s) not interpretable by this profile: {unknown:?}"),
            evidence,
        )
    } else if matched.is_empty() {
        check(
            "telemetry.types",
            Level::Skip,
            "no declared state field was present in the reading",
            evidence,
        )
    } else {
        check(
            "telemetry.types",
            Level::Pass,
            format!("{} declared field(s) match the reading value_type", matched.len()),
            evidence,
        )
    }
}

fn range_check(fields: &[StateFieldDecl], values: &[ResultValue]) -> CheckResult {
    let mut in_range = Vec::new();
    let mut out_of_range = Vec::new();
    let mut skipped = Vec::new();

    for field in fields {
        let Some(range) = &field.range else {
            continue;
        };
        let Some(value) = values.iter().find(|value| value.name == field.name) else {
            continue;
        };
        let raw = value.value.as_deref().unwrap_or("");
        let Ok(number) = raw.trim().parse::<f64>() else {
            // Declared non-numeric fields (or unparseable payloads) are not a
            // range failure; record the skip in evidence.
            skipped.push(field.name.clone());
            continue;
        };
        if !number.is_finite() {
            skipped.push(field.name.clone());
            continue;
        }

        let below = range.min.is_some_and(|min| number < min);
        let above = range.max.is_some_and(|max| number > max);
        let entry = serde_json::json!({
            "field": field.name,
            "value": number,
            "min": range.min,
            "max": range.max,
        });
        if below || above {
            out_of_range.push(entry);
        } else {
            in_range.push(entry);
        }
    }

    let evidence = serde_json::json!({
        "in_range": in_range,
        "out_of_range": out_of_range,
        "skipped_non_numeric": skipped,
    });

    if !out_of_range.is_empty() {
        check(
            "telemetry.range",
            Level::Fail,
            format!("reading(s) outside the declared range: {out_of_range:?}"),
            evidence,
        )
    } else if in_range.is_empty() {
        check(
            "telemetry.range",
            Level::Skip,
            "no numeric reading carries a declared min/max range",
            evidence,
        )
    } else {
        check(
            "telemetry.range",
            Level::Pass,
            format!("{} numeric reading(s) inside the declared range", in_range.len()),
            evidence,
        )
    }
}

/// Two reads separated by the poll source's interval. The second reading must
/// not be observed before the interval elapsed (small scheduling slack allowed).
fn interval_check(
    hub: &TestHub,
    handle: &crate::driver::worker::WorkerHandle,
    source: Option<&SourceDecl>,
    push_driver: bool,
    first_ok: bool,
    first_at: Instant,
) -> CheckResult {
    if push_driver {
        // Push workers ignore submitted reads; the sampler owns the cadence, so
        // a poll-interval measurement would time out for a legitimate driver.
        return check(
            "telemetry.interval",
            Level::Skip,
            "push driver: interval applies to poll sources only",
            serde_json::json!({}),
        );
    }
    let Some(source) = source else {
        return check(
            "telemetry.interval",
            Level::Skip,
            "manifest declares no poll source; interval is not applicable",
            serde_json::json!({}),
        );
    };
    let interval = Duration::from_millis(source.interval_ms.max(source.min_interval_ms));
    if interval.is_zero() {
        return check(
            "telemetry.interval",
            Level::Skip,
            format!("poll source '{}' declares a zero interval", source.id),
            serde_json::json!({"source": source.id}),
        );
    }
    if !first_ok {
        return check(
            "telemetry.interval",
            Level::Skip,
            "initial read failed; interval was not measured",
            serde_json::json!({"source": source.id, "interval_ms": interval.as_millis()}),
        );
    }

    std::thread::sleep(interval);
    match hub.read_and_wait(handle, READ_TIMEOUT) {
        Ok(values) => {
            let gap = first_at.elapsed();
            let on_time = gap + INTERVAL_SLACK >= interval;
            check(
                "telemetry.interval",
                if on_time && !values.is_empty() {
                    Level::Pass
                } else {
                    Level::Fail
                },
                format!(
                    "second read observed after {}ms (interval {}ms, slack {}ms)",
                    gap.as_millis(),
                    interval.as_millis(),
                    INTERVAL_SLACK.as_millis()
                ),
                serde_json::json!({
                    "source": source.id,
                    "interval_ms": interval.as_millis(),
                    "observed_gap_ms": gap.as_millis(),
                    "slack_ms": INTERVAL_SLACK.as_millis(),
                    "second_read_values": values.len(),
                }),
            )
        }
        Err(err) => check(
            "telemetry.interval",
            Level::Fail,
            format!("second read did not return: {err}"),
            serde_json::json!({
                "source": source.id,
                "interval_ms": interval.as_millis(),
                "error": err,
            }),
        ),
    }
}

fn push_check(push_declared: bool, first: &Result<Vec<ResultValue>, String>) -> CheckResult {
    if !push_declared {
        return check(
            "telemetry.push",
            Level::Skip,
            "manifest declares no push source (poll-only driver); value sink not exercised",
            serde_json::json!({}),
        );
    }
    match first {
        Ok(values) => check(
            "telemetry.push",
            Level::Pass,
            format!("push driver delivered {} value(s) through the value sink", values.len()),
            serde_json::json!({
                "values": values.iter().map(|value| serde_json::json!({
                    "name": value.name,
                    "value_type": value.value_type,
                    "value": value.value,
                })).collect::<Vec<_>>(),
            }),
        ),
        Err(err) => check(
            "telemetry.push",
            Level::Fail,
            format!("no value-sink reading within {}ms: {err}", PUSH_TIMEOUT.as_millis()),
            serde_json::json!({"timeout_ms": PUSH_TIMEOUT.as_millis(), "error": err}),
        ),
    }
}

/// Wait for a push driver's first reading to reach the sink consumer, which
/// records it in the hub's per-thing value map. Polling (not `read_and_wait`)
/// because push workers ignore submitted reads and may emit before we
/// subscribe; the map closes that race.
fn wait_for_push_reading(hub: &TestHub, thing_id: &str, timeout: Duration) -> Result<Vec<ResultValue>, String> {
    let deadline = Instant::now() + timeout;
    loop {
        let cached = hub.cached_values(thing_id);
        if !cached.is_empty() {
            let mut values: Vec<ResultValue> = cached.into_values().collect();
            values.sort_by(|left, right| left.name.cmp(&right.name));
            return Ok(values);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "no reading arrived through the value sink within {}ms",
                timeout.as_millis()
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use tinyiothub_core::driver::manifest::NumericRange;

    fn field(name: &str, data_type: &str, required: bool, range: Option<(f64, f64)>) -> StateFieldDecl {
        StateFieldDecl {
            name: name.into(),
            data_type: data_type.into(),
            unit: None,
            range: range.map(|(min, max)| NumericRange {
                min: Some(min),
                max: Some(max),
            }),
            read_only: false,
            required,
            source: None,
        }
    }

    fn reading(name: &str, value_type: &str, value: &str) -> ResultValue {
        ResultValue::new(name.into(), value_type.into(), Some(value.into()))
    }

    fn level_of(report: &ProfileReport, name: &str) -> Option<Level> {
        report.checks.iter().find(|c| c.check == name).map(|c| c.level)
    }

    #[test]
    fn required_fields_fail_when_a_required_value_is_missing() {
        let fields = vec![field("level", "integer", true, None)];
        let values = vec![reading("other", "int", "1")];
        let result = required_fields_check(&fields, &values);
        assert_eq!(result.level, Level::Fail, "{result:?}");
        assert!(result.detail.contains("level"), "{}", result.detail);
    }

    #[test]
    fn required_fields_warn_when_only_optional_values_are_missing() {
        let fields = vec![
            field("level", "integer", true, None),
            field("rssi", "integer", false, None),
        ];
        let values = vec![reading("level", "int", "0")];
        let result = required_fields_check(&fields, &values);
        assert_eq!(result.level, Level::Warn, "{result:?}");
        assert!(result.detail.contains("rssi"), "{}", result.detail);
    }

    #[test]
    fn required_fields_treat_an_empty_value_as_missing() {
        let fields = vec![field("level", "integer", true, None)];
        let values = vec![reading("level", "int", "  ")];
        assert_eq!(required_fields_check(&fields, &values).level, Level::Fail);
    }

    #[test]
    fn types_accept_the_declared_synonyms() {
        let fields = vec![
            field("count", "integer", true, None),
            field("speed", "float", false, None),
            field("armed", "boolean", false, None),
            field("label", "string", false, None),
        ];
        let values = vec![
            reading("count", "int", "1"),
            reading("speed", "number", "1.5"),
            reading("armed", "bool", "true"),
            reading("label", "string", "ok"),
        ];
        assert_eq!(types_check(&fields, &values).level, Level::Pass);
    }

    #[test]
    fn types_fail_on_a_mismatch_and_warn_on_an_unknown_declaration() {
        let mismatch = types_check(
            &[field("count", "integer", true, None)],
            &[reading("count", "string", "one")],
        );
        assert_eq!(mismatch.level, Level::Fail, "{mismatch:?}");

        let unknown = types_check(
            &[field("stamp", "duration", false, None)],
            &[reading("stamp", "string", "5s")],
        );
        assert_eq!(unknown.level, Level::Warn, "{unknown:?}");
    }

    #[test]
    fn range_fails_outside_bounds_and_skips_non_numeric_values() {
        let fields = vec![field("level", "integer", true, Some((0.0, 100.0)))];
        assert_eq!(
            range_check(&fields, &[reading("level", "int", "42")]).level,
            Level::Pass
        );
        assert_eq!(
            range_check(&fields, &[reading("level", "int", "101")]).level,
            Level::Fail
        );
        assert_eq!(
            range_check(&fields, &[reading("level", "int", "n/a")]).level,
            Level::Skip
        );
    }

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

    #[test]
    fn demo_package_passes_telemetry_and_skips_push() {
        if !demo_built() {
            eprintln!("skipping telemetry test: cargo build -p demo-declarative");
            return;
        }
        let report = run(&demo_package_dir());
        assert_eq!(report.level(), Level::Pass, "{report:?}");
        for name in [
            "telemetry.required_fields",
            "telemetry.types",
            "telemetry.range",
            "telemetry.interval",
        ] {
            assert_eq!(level_of(&report, name), Some(Level::Pass), "check {name}: {report:?}");
        }
        assert_eq!(
            level_of(&report, "telemetry.push"),
            Some(Level::Skip),
            "demo is poll-only: {report:?}"
        );
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
        let load = report.checks.iter().find(|c| c.check == "telemetry.load").unwrap();
        assert_eq!(load.level, Level::Fail);
        assert!(load.detail.contains("cargo build -p demo"), "{}", load.detail);
    }
}
