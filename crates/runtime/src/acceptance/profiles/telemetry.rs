//! `telemetry` profile: verify the driver's declared telemetry contract against
//! a real package loaded into an in-memory [`TestHub`].
//!
//! Checks:
//! - `telemetry.required_fields`: one read; every `required` state field carries
//!   a value (missing optional fields warn).
//! - `telemetry.types`: the reading's `value_type` **and payload** match the
//!   declared type (a numeric declaration with a non-numeric payload fails).
//! - `telemetry.range`: numeric readings fall inside the declared `min`/`max`;
//!   unparseable/NaN payloads for numeric declarations fail, unbounded
//!   `range: {}` declarations are skipped with a reason.
//! - `telemetry.second_read`: a second, fresh poll read is serviced.
//! - `telemetry.interval`: always a skip — cadence is sampler-owned and
//!   covered by the sampler/streaming tests, not measured here.
//! - `telemetry.push`: a push driver must deliver a complete required batch
//!   through the value sink. The loaded driver's `is_push()` is authoritative;
//!   a manifest-only push declaration on a poll driver warns (ABI v2 dynamic
//!   drivers cannot push), and poll-only manifests skip.

use std::path::Path;
use std::time::{Duration, Instant};

use tinyiothub_core::driver::ResultValue;
use tinyiothub_core::driver::manifest::{DriverManifest, SourceDecl, StateFieldDecl};

use crate::acceptance::report::{CheckResult, Level, ProfileReport};
use crate::acceptance::test_hub::{TestHub, ThingOptions};

const PROFILE: &str = "telemetry";
/// A single poll read is near-instantaneous; CI scheduling may be noisy.
const READ_TIMEOUT: Duration = Duration::from_secs(5);
/// Bounded wait for a push driver's first complete value-sink reading.
const PUSH_TIMEOUT: Duration = Duration::from_secs(5);
/// Poll period for the push snapshot helper (and tests).
const PUSH_POLL_INTERVAL: Duration = Duration::from_millis(10);

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

    let manifest_declares_push = declares_push(&loaded.manifest);
    let poll_source = primary_poll_source(&loaded.manifest).cloned();
    let required = declared_required(&loaded.manifest.state_fields);

    if !manifest_declares_push && poll_source.is_none() {
        let reason = "manifest declares neither a poll source nor a push source; telemetry cannot be observed";
        for name in [
            "telemetry.required_fields",
            "telemetry.types",
            "telemetry.range",
            "telemetry.second_read",
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
    // The worker is the authority on push mode: a manifest may over-declare
    // (ABI v2 dynamic drivers never push), and a built-in push driver may not
    // carry the declaration at all.
    let driver_is_push = handle.is_push();

    // Push workers ignore submitted reads (`start_push` owns the sink), so a
    // push driver's initial reading must come from the value-sink path.
    let first = if driver_is_push {
        wait_for_push_values(&required, PUSH_TIMEOUT, || {
            hub.cached_values(&thing.id).into_values().collect()
        })
    } else {
        hub.read_and_wait(&handle, READ_TIMEOUT)
    };

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

    checks.push(second_read_check(
        &hub,
        &handle,
        driver_is_push,
        poll_source.is_some(),
        first.is_ok(),
    ));
    checks.push(interval_check(driver_is_push, poll_source.is_some()));
    checks.push(push_check(manifest_declares_push, driver_is_push, &first));

    if let Err(err) = hub.stop(&handle) {
        checks.push(check(
            "telemetry.stop",
            Level::Fail,
            format!("worker cleanup failed: {err}"),
            serde_json::json!({"error": err}),
        ));
    }

    ProfileReport {
        profile: PROFILE.into(),
        checks,
    }
}

/// A push driver is declared by the manifest capability or a `push` source
/// kind. `stream` is a separate capability (`capabilities.streaming`), not a
/// value-sink source kind, so it is deliberately not treated as push.
fn declares_push(manifest: &DriverManifest) -> bool {
    manifest.capabilities.push || manifest.sources.iter().any(|source| source.kind == "push")
}

/// The poll source whose readings the profile exercises. Non-demand-only
/// sources win (the `slow` source added for streaming is demand-gated); fall
/// back to the first poll source.
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

/// Whether the payload is consistent with the declared kind. Integers must be
/// integral (`"1.5"` is not an int payload); floats may be integer-valued but
/// must parse and be finite (`NaN`/`inf` are rejected).
fn payload_matches(kind: ValueKind, raw: &str) -> bool {
    let raw = raw.trim();
    match kind {
        ValueKind::Integer => raw.parse::<i64>().is_ok(),
        ValueKind::Float => raw.parse::<f64>().is_ok_and(f64::is_finite),
        ValueKind::Boolean => matches!(raw.to_ascii_lowercase().as_str(), "true" | "false" | "1" | "0"),
        ValueKind::String => true,
    }
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
        let Some(kind) = declared_kind(&field.data_type) else {
            unknown.push(serde_json::json!({
                "field": field.name,
                "declared": field.data_type,
                "reading": value.value_type,
            }));
            continue;
        };

        let type_ok = reading_type_matches(kind, &value.value_type);
        // A missing payload is scored by `required_fields`; only validate a
        // payload that is actually present.
        let payload_ok = value.value.as_deref().is_none_or(|raw| payload_matches(kind, raw));
        let entry = serde_json::json!({
            "field": field.name,
            "declared": field.data_type,
            "reading": value.value_type,
            "value": value.value,
            "type_match": type_ok,
            "payload_match": payload_ok,
        });
        if type_ok && payload_ok {
            matched.push(entry);
        } else {
            mismatched.push(entry);
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
            format!("reading type/payload does not match the declaration: {mismatched:?}"),
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
            format!(
                "{} declared field(s) match the reading value_type and payload",
                matched.len()
            ),
            evidence,
        )
    }
}

fn range_check(fields: &[StateFieldDecl], values: &[ResultValue]) -> CheckResult {
    let mut in_range = Vec::new();
    let mut out_of_range = Vec::new();
    let mut invalid = Vec::new();
    let mut skipped_non_numeric = Vec::new();
    let mut unbounded = Vec::new();

    for field in fields {
        let Some(range) = &field.range else {
            continue;
        };
        if range.min.is_none() && range.max.is_none() {
            // `range: {}` used to pass any numeric value: there is no bound to
            // check, so report the declaration as skipped instead of a pass.
            unbounded.push(field.name.clone());
            continue;
        }
        let Some(value) = values.iter().find(|value| value.name == field.name) else {
            continue;
        };
        let numeric_decl = matches!(
            declared_kind(&field.data_type),
            Some(ValueKind::Integer | ValueKind::Float)
        );
        let raw = value.value.as_deref().unwrap_or("");
        let parsed = raw.trim().parse::<f64>();
        let number = match parsed {
            Ok(number) if number.is_finite() => number,
            _ if numeric_decl => {
                // A numeric declaration with a non-numeric/NaN payload is a
                // contract violation, never a silent skip.
                invalid.push(serde_json::json!({
                    "field": field.name,
                    "value_type": value.value_type,
                    "value": value.value,
                    "reason": "numeric declaration with a non-numeric or non-finite payload",
                }));
                continue;
            }
            _ => {
                // Declared non-numeric fields (or unparseable string payloads)
                // are outside range checking; record the skip in evidence.
                skipped_non_numeric.push(field.name.clone());
                continue;
            }
        };

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
        "invalid": invalid,
        "skipped_non_numeric": skipped_non_numeric,
        "unbounded": unbounded,
    });

    if !invalid.is_empty() || !out_of_range.is_empty() {
        check(
            "telemetry.range",
            Level::Fail,
            format!("invalid and/or out-of-range reading(s): invalid={invalid:?}, out_of_range={out_of_range:?}"),
            evidence,
        )
    } else if in_range.is_empty() {
        let reason = if !unbounded.is_empty() && skipped_non_numeric.is_empty() {
            format!("declared range has no min/max bound(s): {unbounded:?}")
        } else {
            "no numeric reading carries a bounded min/max range".to_string()
        };
        check("telemetry.range", Level::Skip, reason, evidence)
    } else {
        check(
            "telemetry.range",
            Level::Pass,
            format!("{} numeric reading(s) inside the declared range", in_range.len()),
            evidence,
        )
    }
}

/// A second poll read must be serviced with a fresh, non-empty reading.
fn second_read_check(
    hub: &TestHub,
    handle: &crate::driver::worker::WorkerHandle,
    driver_is_push: bool,
    has_poll_source: bool,
    first_ok: bool,
) -> CheckResult {
    if driver_is_push {
        return check(
            "telemetry.second_read",
            Level::Skip,
            "push driver: reads are sink-driven, not submitted",
            serde_json::json!({}),
        );
    }
    if !has_poll_source {
        return check(
            "telemetry.second_read",
            Level::Skip,
            "manifest declares no poll source",
            serde_json::json!({}),
        );
    }
    if !first_ok {
        return check(
            "telemetry.second_read",
            Level::Skip,
            "initial read failed; second read not attempted",
            serde_json::json!({}),
        );
    }

    match hub.read_and_wait(handle, READ_TIMEOUT) {
        Ok(values) if !values.is_empty() => check(
            "telemetry.second_read",
            Level::Pass,
            format!("a second fresh read returned {} value(s)", values.len()),
            serde_json::json!({"values": values.len()}),
        ),
        Ok(_) => check(
            "telemetry.second_read",
            Level::Fail,
            "second read returned no values",
            serde_json::json!({"values": 0}),
        ),
        Err(err) => check(
            "telemetry.second_read",
            Level::Fail,
            format!("second read did not return: {err}"),
            serde_json::json!({"error": err}),
        ),
    }
}

/// Cadence is owned by the sampler, not the driver; this profile cannot make
/// an honest interval assertion by sleeping itself.
fn interval_check(driver_is_push: bool, has_poll_source: bool) -> CheckResult {
    let reason = if driver_is_push {
        "push driver: delivery cadence is sink-owned; covered by sampler/streaming tests"
    } else if !has_poll_source {
        "manifest declares no poll source; covered by sampler/streaming tests"
    } else {
        "cadence is sampler-owned; covered by sampler/streaming tests"
    };
    check(
        "telemetry.interval",
        Level::Skip,
        reason,
        serde_json::json!({"sampler_owned": true}),
    )
}

fn push_check(
    manifest_declares_push: bool,
    driver_is_push: bool,
    first: &Result<Vec<ResultValue>, String>,
) -> CheckResult {
    if driver_is_push {
        return match first {
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
                format!(
                    "no complete value-sink reading within {}ms: {err}",
                    PUSH_TIMEOUT.as_millis()
                ),
                serde_json::json!({"timeout_ms": PUSH_TIMEOUT.as_millis(), "error": err}),
            ),
        };
    }

    if manifest_declares_push {
        // ABI v2 dynamic drivers cannot push. Do not wait for a sink reading
        // that can never arrive; surface the declaration mismatch instead.
        return check(
            "telemetry.push",
            Level::Warn,
            "manifest declares a push source but the loaded driver runs in poll mode (ABI v2 dynamic drivers cannot push); value sink not exercised",
            serde_json::json!({"manifest_declares_push": true, "driver_is_push": false}),
        );
    }

    check(
        "telemetry.push",
        Level::Skip,
        "manifest declares no push source (poll-only driver); value sink not exercised",
        serde_json::json!({"manifest_declares_push": false, "driver_is_push": false}),
    )
}

/// Wait for a push driver's first complete reading through the sink consumer.
///
/// Completeness means every `required` state field carries a value; when the
/// manifest declares none, any non-empty batch completes. The snapshot closure
/// keeps the polling logic unit-testable without a live driver.
fn wait_for_push_values(
    required: &[String],
    timeout: Duration,
    mut snapshot: impl FnMut() -> Vec<ResultValue>,
) -> Result<Vec<ResultValue>, String> {
    let deadline = Instant::now() + timeout;
    loop {
        let mut values = snapshot();
        values.sort_by(|left, right| left.name.cmp(&right.name));
        let complete = if required.is_empty() {
            !values.is_empty()
        } else {
            required
                .iter()
                .all(|name| values.iter().find(|value| &value.name == name).is_some_and(has_value))
        };
        if complete {
            return Ok(values);
        }
        if Instant::now() >= deadline {
            let missing: Vec<&String> = required
                .iter()
                .filter(|name| !values.iter().find(|value| &value.name == *name).is_some_and(has_value))
                .collect();
            return Err(if missing.is_empty() {
                format!(
                    "no reading arrived through the value sink within {}ms",
                    timeout.as_millis()
                )
            } else {
                format!(
                    "incomplete push reading after {}ms; missing required field(s): {missing:?}",
                    timeout.as_millis()
                )
            });
        }
        std::thread::sleep(PUSH_POLL_INTERVAL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

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
            reading("speed", "number", "1"),
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
    fn types_fail_on_a_non_numeric_payload_for_a_numeric_declaration() {
        let result = types_check(
            &[field("level", "integer", true, None)],
            &[reading("level", "int", "abc")],
        );
        assert_eq!(result.level, Level::Fail, "{result:?}");

        let nan = types_check(
            &[field("level", "float", true, None)],
            &[reading("level", "float", "NaN")],
        );
        assert_eq!(nan.level, Level::Fail, "{nan:?}");
    }

    #[test]
    fn types_fail_on_a_fractional_integer_payload() {
        let result = types_check(
            &[field("level", "integer", true, None)],
            &[reading("level", "int", "1.5")],
        );
        assert_eq!(result.level, Level::Fail, "{result:?}");
    }

    #[test]
    fn range_fails_outside_bounds_and_skips_non_numeric_values() {
        let fields = vec![
            field("level", "integer", true, Some((0.0, 100.0))),
            field("label", "string", false, Some((0.0, 100.0))),
        ];
        assert_eq!(
            range_check(
                &fields,
                &[reading("level", "int", "42"), reading("label", "string", "n/a")]
            )
            .level,
            Level::Pass
        );
        assert_eq!(
            range_check(&fields, &[reading("level", "int", "101")]).level,
            Level::Fail
        );
    }

    #[test]
    fn range_fails_on_unparseable_or_non_finite_numeric_payloads() {
        let fields = vec![field("level", "integer", true, Some((0.0, 100.0)))];
        for payload in ["abc", "NaN", "inf"] {
            let result = range_check(&fields, &[reading("level", "int", payload)]);
            assert_eq!(result.level, Level::Fail, "payload {payload}: {result:?}");
        }
    }

    #[test]
    fn range_skips_when_no_bound_is_declared() {
        let unbounded = StateFieldDecl {
            range: Some(NumericRange { min: None, max: None }),
            ..field("level", "integer", true, None)
        };
        let result = range_check(&[unbounded], &[reading("level", "int", "42")]);
        assert_eq!(result.level, Level::Skip, "{result:?}");
        assert!(result.detail.contains("no min/max"), "{}", result.detail);
    }

    #[test]
    fn push_wait_requires_a_complete_required_batch() {
        let required = vec!["level".to_string(), "level2".to_string()];
        let calls = AtomicUsize::new(0);
        let values = wait_for_push_values(&required, Duration::from_millis(500), || {
            let call = calls.fetch_add(1, Ordering::SeqCst);
            match call {
                0 => vec![],
                1 => vec![reading("level", "int", "1")],
                _ => vec![reading("level", "int", "1"), reading("level2", "int", "2")],
            }
        })
        .expect("complete batch");
        assert_eq!(values.len(), 2);
        assert_eq!(values[0].name, "level");
        assert_eq!(values[1].name, "level2");
    }

    #[test]
    fn push_wait_times_out_with_the_missing_fields() {
        let required = vec!["level".to_string(), "level2".to_string()];
        let err = wait_for_push_values(&required, Duration::from_millis(60), || {
            vec![reading("level", "int", "1")]
        })
        .unwrap_err();
        assert!(err.contains("level2"), "{err}");
    }

    #[test]
    fn push_check_warns_when_the_manifest_over_declares() {
        let result = push_check(true, false, &Ok(vec![reading("level", "int", "0")]));
        assert_eq!(result.level, Level::Warn, "{result:?}");
        assert!(result.detail.contains("poll mode"), "{}", result.detail);
    }

    #[test]
    fn push_check_passes_for_a_push_driver_reading() {
        let result = push_check(false, true, &Ok(vec![reading("level", "int", "0")]));
        assert_eq!(result.level, Level::Pass, "{result:?}");
    }

    #[test]
    fn interval_is_a_skip_not_a_claim() {
        let result = interval_check(false, true);
        assert_eq!(result.level, Level::Skip);
        assert!(result.detail.contains("sampler-owned"), "{}", result.detail);
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
            "telemetry.second_read",
        ] {
            assert_eq!(level_of(&report, name), Some(Level::Pass), "check {name}: {report:?}");
        }
        assert_eq!(
            level_of(&report, "telemetry.interval"),
            Some(Level::Skip),
            "interval is sampler-owned: {report:?}"
        );
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
