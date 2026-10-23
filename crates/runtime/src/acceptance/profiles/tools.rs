//! `tools` profile: verify the declared tool contract against a real package
//! loaded into an in-memory [`TestHub`].
//!
//! Checks:
//! - `tools.declared`: manifest tools are structurally valid (unique names,
//!   object parameter schemas, declared resources, valid set targets).
//! - `tools.safe_roundtrip`: every short-running `safe` tool is invoked through
//!   a spawned worker with parameters that satisfy its schema; the worker's
//!   `done` outcome must report success. Tools whose declared **default or max**
//!   timeout exceeds [`AUTO_EXECUTE_MAX_MS`] are skipped with the reason in
//!   evidence — the suite never auto-runs them and never waits past
//!   [`AUTO_EXECUTE_WAIT`].
//! - `tools.invalid_params`: a synthesized invalid value (out of range,
//!   wrong type, or missing required field) is rejected by `validate_params`.
//!   Invalid parameters are never dispatched to the driver.
//! - `tools.undeclared`: an undeclared tool is rejected by the real call
//!   validator; nothing is dispatched.
//! - `tools.control_skip`: `control`/`dangerous` tools are never auto-called;
//!   they are reported as a skip with the withheld names.
//!
//! A package without tools skips every check with a reason.

use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};
use tinyiothub_core::driver::declaration::{DeviceDeclaration, fixed_declaration, validate_device_declaration};
use tinyiothub_core::driver::manifest::{DriverManifest, ManifestTool, ToolSafetyDecl, validate_manifest};
use tinyiothub_core::driver::schema::validate_params;
use tinyiothub_core::models::thing_command::ThingCommand;

use crate::acceptance::report::{CheckResult, Level, ProfileReport};
use crate::acceptance::test_hub::{LoadedTestDriver, TestHub, ThingOptions};
use crate::access::validator::{CallError, ValidatedCall, validate_call};

const PROFILE: &str = "tools";
/// Both the declared default timeout and the declared max timeout must stay
/// within this bound for a safe tool to be auto-executed. A 1s default with a
/// 10-minute max could otherwise block the suite (`long_task` declares 10s).
const AUTO_EXECUTE_MAX_MS: u64 = 2_000;
/// Hard cap for one auto-executed round trip: the bound plus scheduling slack.
const AUTO_EXECUTE_WAIT: Duration = Duration::from_millis(AUTO_EXECUTE_MAX_MS + 1_000);
/// Sentinel name used to prove undeclared tools are rejected.
const UNDECLARED_TOOL: &str = "__acceptance_undeclared__";
const CHECKS: &[&str] = &[
    "tools.declared",
    "tools.safe_roundtrip",
    "tools.invalid_params",
    "tools.undeclared",
    "tools.control_skip",
];

fn check(name: &str, level: Level, detail: impl Into<String>, evidence: Value) -> CheckResult {
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
            checks.push(check("tools.load", Level::Fail, err.clone(), json!({"error": err})));
            return ProfileReport {
                profile: PROFILE.into(),
                checks,
            };
        }
    };

    if let Some(skips) = no_tools_skips(&loaded.manifest) {
        checks.extend(skips);
        return ProfileReport {
            profile: PROFILE.into(),
            checks,
        };
    }

    let decl = fixed_declaration(&loaded.manifest);
    checks.push(declared_check(&loaded.manifest, &decl));
    checks.extend(safe_roundtrip_checks(&hub, &loaded));
    checks.push(invalid_params_check(&decl));
    checks.push(undeclared_check(&decl));
    checks.push(control_skip_check(&decl));

    ProfileReport {
        profile: PROFILE.into(),
        checks,
    }
}

/// A package without tools has nothing to exercise: every check is a skip with
/// an explicit reason (never a pass).
fn no_tools_skips(manifest: &DriverManifest) -> Option<Vec<CheckResult>> {
    if !manifest.tools.is_empty() {
        return None;
    }
    Some(
        CHECKS
            .iter()
            .map(|name| {
                check(
                    name,
                    Level::Skip,
                    "manifest declares no tools; nothing to exercise",
                    json!({"tools": 0}),
                )
            })
            .collect(),
    )
}

fn tool_evidence(tools: &[ManifestTool]) -> Vec<Value> {
    tools
        .iter()
        .map(|tool| {
            json!({
                "name": tool.name,
                "kind": tool.kind,
                "safety": tool.safety,
                "resources": tool.resources,
                "defaultTimeoutMs": tool.timeout.default_ms,
            })
        })
        .collect()
}

fn declared_check(manifest: &DriverManifest, decl: &DeviceDeclaration) -> CheckResult {
    let evidence = json!({
        "tools": tool_evidence(&manifest.tools),
        "resources": manifest.resources.iter().map(|r| r.id.clone()).collect::<Vec<_>>(),
    });
    match (validate_manifest(manifest), validate_device_declaration(manifest, decl)) {
        (Ok(()), Ok(())) => check(
            "tools.declared",
            Level::Pass,
            format!("{} declared tool(s) are structurally valid", manifest.tools.len()),
            evidence,
        ),
        (Err(err), _) | (_, Err(err)) => check(
            "tools.declared",
            Level::Fail,
            format!("tool declaration is invalid: {err}"),
            evidence,
        ),
    }
}

/// A safe tool selected for auto-execution with schema-valid parameters.
struct PlannedCall {
    tool: ManifestTool,
    params: Value,
}

/// Safe-tool selection, split from execution so the bounds can be unit-tested
/// without a live driver.
struct SafeExecutionPlan {
    calls: Vec<PlannedCall>,
    excluded: Vec<Value>,
}

fn plan_safe_tools(tools: &[ManifestTool]) -> SafeExecutionPlan {
    let mut calls = Vec::new();
    let mut excluded = Vec::new();
    for tool in tools {
        if tool.safety != ToolSafetyDecl::Safe {
            // control/dangerous are reported by `tools.control_skip`.
            continue;
        }
        if tool.timeout.default_ms > AUTO_EXECUTE_MAX_MS || tool.timeout.max_ms > AUTO_EXECUTE_MAX_MS {
            excluded.push(json!({
                "name": tool.name,
                "reason": format!(
                    "declared timeout (default {}ms, max {}ms) exceeds the {}ms auto-execute bound",
                    tool.timeout.default_ms, tool.timeout.max_ms, AUTO_EXECUTE_MAX_MS
                ),
            }));
            continue;
        }
        let Some(params) = valid_params(&tool.parameters) else {
            excluded.push(json!({
                "name": tool.name,
                "reason": "profile could not synthesize parameters that satisfy the declared schema",
            }));
            continue;
        };
        // Re-validate: synthesized values must satisfy the schema before the
        // tool is ever considered for dispatch.
        if let Err(err) = validate_params(&tool.parameters, &params) {
            excluded.push(json!({
                "name": tool.name,
                "reason": format!("profile synthesized parameters the schema rejects: {err}"),
            }));
            continue;
        }
        calls.push(PlannedCall {
            tool: tool.clone(),
            params,
        });
    }
    SafeExecutionPlan { calls, excluded }
}

/// Execute short-running safe tools, waiting on the worker's `done` outcome.
fn safe_roundtrip_checks(hub: &TestHub, loaded: &LoadedTestDriver) -> Vec<CheckResult> {
    let SafeExecutionPlan { calls: plan, excluded } = plan_safe_tools(&loaded.manifest.tools);

    if plan.is_empty() {
        return vec![check(
            "tools.safe_roundtrip",
            Level::Skip,
            "no short-running safe tool can be invoked automatically",
            json!({"excluded": excluded}),
        )];
    }

    let thing = hub.make_thing(loaded, ThingOptions::default());
    let handle = match hub.spawn(&loaded.driver, thing.clone()) {
        Ok(handle) => handle,
        Err(err) => {
            return vec![check(
                "tools.safe_roundtrip",
                Level::Fail,
                format!("worker spawn failed: {err}"),
                json!({
                    "error": err,
                    "planned": plan.iter().map(|call| call.tool.name.clone()).collect::<Vec<_>>(),
                }),
            )];
        }
    };

    let mut executed = Vec::new();
    let mut failed = Vec::new();
    for call in &plan {
        let tool = &call.tool;
        let params = &call.params;
        // Never dispatch parameters that do not satisfy the schema (planning
        // already re-validated; this is defense in depth).
        if let Err(err) = validate_params(&tool.parameters, params) {
            failed.push(tool.name.clone());
            executed.push(json!({
                "tool": tool.name,
                "params": params,
                "executed": false,
                "error": format!("profile parameters failed validation: {err}"),
            }));
            continue;
        }
        // The declared max is bounded by AUTO_EXECUTE_MAX_MS, so a fixed cap
        // here cannot outlive the selection bound by more than the slack.
        match hub.execute_and_wait(&handle, command(&thing.id, &tool.name, params), AUTO_EXECUTE_WAIT) {
            Ok(outcome) if outcome.success && !outcome.cancelled => executed.push(json!({
                "tool": tool.name,
                "params": params,
                "executed": true,
                "success": true,
            })),
            Ok(outcome) => {
                failed.push(tool.name.clone());
                executed.push(json!({
                    "tool": tool.name,
                    "params": params,
                    "executed": true,
                    "success": outcome.success,
                    "cancelled": outcome.cancelled,
                    "error": outcome.error,
                }));
            }
            Err(err) => {
                failed.push(tool.name.clone());
                executed.push(json!({
                    "tool": tool.name,
                    "params": params,
                    "executed": true,
                    "error": err,
                }));
            }
        }
    }

    let mut checks = Vec::new();
    checks.push(if failed.is_empty() {
        check(
            "tools.safe_roundtrip",
            Level::Pass,
            format!(
                "{} safe tool(s) executed successfully with valid parameters",
                plan.len()
            ),
            json!({"executed": executed, "excluded": excluded}),
        )
    } else {
        check(
            "tools.safe_roundtrip",
            Level::Fail,
            format!("safe tool round trip failed for {failed:?}"),
            json!({"executed": executed, "excluded": excluded}),
        )
    });
    if let Err(err) = hub.stop(&handle) {
        checks.push(check(
            "tools.stop",
            Level::Fail,
            format!("worker cleanup failed: {err}"),
            json!({"error": err}),
        ));
    }
    checks
}

fn command(thing_id: &str, tool: &str, params: &Value) -> ThingCommand {
    ThingCommand {
        id: format!("acceptance-{tool}"),
        thing_id: thing_id.to_string(),
        name: tool.to_string(),
        display_name: None,
        description: None,
        parameters: Some(params.to_string()),
        created_at: String::new(),
    }
}

/// Prove the driver's declared schema rejects an invalid value. The invalid
/// parameters are only validated locally, never dispatched.
fn invalid_params_check(decl: &DeviceDeclaration) -> CheckResult {
    let candidate = decl
        .tools
        .iter()
        .filter(|tool| tool.safety == ToolSafetyDecl::Safe)
        .find_map(|tool| invalid_params(&tool.parameters).map(|(params, expected)| (tool, params, expected)));
    let Some((tool, params, expected)) = candidate else {
        return check(
            "tools.invalid_params",
            Level::Skip,
            "no declared safe tool exposes a checkable parameter schema",
            json!({}),
        );
    };
    rejected_params_check(tool, &params, &expected)
}

fn rejected_params_check(tool: &ManifestTool, params: &Value, expected: &str) -> CheckResult {
    match validate_params(&tool.parameters, params) {
        Ok(()) => check(
            "tools.invalid_params",
            Level::Fail,
            format!("invalid parameters were accepted by '{}': {params}", tool.name),
            json!({
                "tool": tool.name,
                "params": params,
                "expected": expected,
                "accepted": true,
            }),
        ),
        Err(err) => check(
            "tools.invalid_params",
            Level::Pass,
            format!("'{}' rejected invalid parameters: {err}", tool.name),
            json!({
                "tool": tool.name,
                "params": params,
                "expected": expected,
                "accepted": false,
                "error": err,
            }),
        ),
    }
}

fn undeclared_check(decl: &DeviceDeclaration) -> CheckResult {
    if decl.tools.iter().any(|tool| tool.name == UNDECLARED_TOOL) {
        return check(
            "tools.undeclared",
            Level::Skip,
            format!("declaration already uses the sentinel name '{UNDECLARED_TOOL}'"),
            json!({"tool": UNDECLARED_TOOL}),
        );
    }
    undeclared_result_check(validate_call(decl, UNDECLARED_TOOL, None, None))
}

/// Only [`CallError::UndeclaredTool`] proves the undeclared-tool rejection.
/// Any other outcome (a different `CallError` variant or a successful
/// validation) fails the check with the actual error: it means the expected
/// rejection was not exercised, and must not be reported as a pass.
fn undeclared_result_check(result: Result<ValidatedCall, CallError>) -> CheckResult {
    match result {
        Err(CallError::UndeclaredTool(name)) if name == UNDECLARED_TOOL => check(
            "tools.undeclared",
            Level::Pass,
            format!("undeclared tool '{UNDECLARED_TOOL}' was rejected before dispatch"),
            json!({
                "tool": UNDECLARED_TOOL,
                "rejected": true,
                "error": format!("action '{UNDECLARED_TOOL}' not declared"),
            }),
        ),
        Err(err) => check(
            "tools.undeclared",
            Level::Fail,
            format!("expected UndeclaredTool for '{UNDECLARED_TOOL}', got: {err}"),
            json!({
                "tool": UNDECLARED_TOOL,
                "rejected": false,
                "error": err.to_string(),
            }),
        ),
        Ok(call) => check(
            "tools.undeclared",
            Level::Fail,
            format!(
                "undeclared tool '{UNDECLARED_TOOL}' passed validation as '{}'",
                call.tool.name
            ),
            json!({"tool": UNDECLARED_TOOL, "rejected": false}),
        ),
    }
}

fn control_skip_check(decl: &DeviceDeclaration) -> CheckResult {
    let withheld: Vec<&ManifestTool> = decl
        .tools
        .iter()
        .filter(|tool| tool.safety != ToolSafetyDecl::Safe)
        .collect();
    if withheld.is_empty() {
        return check(
            "tools.control_skip",
            Level::Skip,
            "manifest declares no control/dangerous tools; nothing was withheld",
            json!({"withheld": []}),
        );
    }
    let names: Vec<&str> = withheld.iter().map(|tool| tool.name.as_str()).collect();
    check(
        "tools.control_skip",
        Level::Skip,
        format!(
            "{} control/dangerous tool(s) not auto-executed: {}",
            names.len(),
            names.join(", ")
        ),
        json!({
            "withheld": withheld.iter().map(|tool| json!({
                "name": tool.name,
                "kind": tool.kind,
                "safety": tool.safety,
            })).collect::<Vec<_>>(),
            "reason": "safety is control/dangerous; the suite never auto-runs them",
        }),
    )
}

/// Build a parameter object satisfying the declared schema's required fields.
/// Returns `None` when a required property cannot be synthesized (unknown type).
fn valid_params(schema: &Value) -> Option<Value> {
    let obj = schema.as_object()?;
    let mut params = serde_json::Map::new();
    if let Some(required) = obj.get("required").and_then(Value::as_array) {
        let properties = obj.get("properties").and_then(Value::as_object)?;
        for name in required {
            let name = name.as_str()?;
            let property = properties.get(name)?;
            params.insert(name.to_string(), example_value(property)?);
        }
    }
    Some(Value::Object(params))
}

fn example_value(property: &Value) -> Option<Value> {
    if let Some(values) = property.get("enum").and_then(Value::as_array) {
        return values.first().cloned();
    }
    match property.get("type").and_then(Value::as_str)? {
        "string" => Some(json!("acceptance")),
        // JSON Schema numbers may be written in float form (`1.0`), so parse
        // with `as_f64`. Round integers and clamp to the declared bounds; a
        // `maximum` without a `minimum` must not fall back to 0.
        "integer" => {
            let min = property.get("minimum").and_then(Value::as_f64);
            let max = property.get("maximum").and_then(Value::as_f64);
            Some(json!(integer_example(min, max)))
        }
        "number" => {
            let min = property.get("minimum").and_then(Value::as_f64).unwrap_or(0.0);
            let value = property
                .get("maximum")
                .and_then(Value::as_f64)
                .map_or(min, |max| min.min(max));
            Some(json!(value))
        }
        "boolean" => Some(json!(false)),
        "array" => Some(json!([])),
        "object" => Some(json!({})),
        _ => None,
    }
}

fn integer_example(min: Option<f64>, max: Option<f64>) -> i64 {
    let mut value = min.unwrap_or(0.0);
    if let Some(max) = max {
        value = value.min(max);
    }
    let mut rounded = value.round() as i64;
    // Rounding up must not escape a fractional maximum.
    if let Some(max) = max {
        let max_integer = max.floor() as i64;
        if rounded > max_integer {
            rounded = max_integer;
        }
    }
    rounded
}

/// Build an invalid parameter object plus a short description of the expected
/// rejection (out of range, wrong type, missing/unknown field).
fn invalid_params(schema: &Value) -> Option<(Value, String)> {
    let obj = schema.as_object()?;
    let properties = obj.get("properties").and_then(Value::as_object);
    let required: Vec<&str> = obj
        .get("required")
        .and_then(Value::as_array)
        .map(|names| names.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    let target = required
        .first()
        .copied()
        .or_else(|| properties.and_then(|props| props.keys().next().map(String::as_str)));
    if let Some(name) = target {
        let property = properties?.get(name)?;
        let (invalid, expected) = invalid_value(property)?;
        let mut base = valid_params(schema)?;
        if let Value::Object(map) = &mut base {
            map.insert(name.to_string(), invalid);
        }
        return Some((base, expected));
    }
    if obj.get("additionalProperties").and_then(Value::as_bool) == Some(false) {
        return Some((json!({"__acceptance_unknown__": null}), "unknown field".into()));
    }
    None
}

fn invalid_value(property: &Value) -> Option<(Value, String)> {
    let data_type = property.get("type").and_then(Value::as_str);
    if let Some(minimum) = property.get("minimum").and_then(Value::as_f64) {
        let value = if data_type == Some("integer") {
            json!((minimum as i64).saturating_sub(1))
        } else {
            json!(minimum - 1.0)
        };
        return Some((value, "below minimum".into()));
    }
    if let Some(maximum) = property.get("maximum").and_then(Value::as_f64) {
        let value = if data_type == Some("integer") {
            json!((maximum as i64).saturating_add(1))
        } else {
            json!(maximum + 1.0)
        };
        return Some((value, "above maximum".into()));
    }
    let wrong = match data_type? {
        "string" => json!(42),
        "integer" | "number" => json!("not-a-number"),
        "boolean" => json!("not-a-boolean"),
        "array" => json!({}),
        "object" => json!([]),
        _ => return None,
    };
    Some((wrong, "wrong type".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use tinyiothub_core::driver::manifest::ToolKindDecl;

    fn demo_package_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../drivers/demo-declarative")
    }

    fn demo_manifest() -> DriverManifest {
        crate::access::package::read_package_raw(&demo_package_dir())
            .expect("read demo manifest")
            .1
    }

    fn demo_declaration() -> DeviceDeclaration {
        fixed_declaration(&demo_manifest())
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

    fn manifest_with_tools(tools: Value) -> DriverManifest {
        let mut value: Value = serde_json::from_str(
            r#"{
              "manifestVersion":"1.0",
              "identity":{"driver":"demo","version":"1.0.0","protocol":"demo","abiVersion":2,"dynamicCapabilities":false},
              "config":{"type":"object"},"tools":[],
              "stateFields":[{"name":"level","type":"integer","readOnly":false,"required":true}],
              "sources":[{"id":"telemetry","kind":"poll","fields":["level"]}],
              "resources":[{"id":"actuator","mode":"exclusive","capacity":1}],
              "placement":{"mode":"runtime"},
              "limits":{"maxConcurrentJobs":1,"maxCallsPerMin":60}
            }"#,
        )
        .unwrap();
        value["tools"] = tools;
        serde_json::from_value(value).unwrap()
    }

    fn tool(name: &str, safety: &str, parameters: Value) -> Value {
        json!({
            "name": name,
            "kind": "action",
            "parameters": parameters,
            "safety": safety,
            "resources": [],
            "cancellable": true,
        })
    }

    fn tool_with_timeout(name: &str, safety: &str, parameters: Value, default_ms: u64, max_ms: u64) -> Value {
        let mut tool = tool(name, safety, parameters);
        tool["timeout"] = json!({"defaultMs": default_ms, "maxMs": max_ms});
        tool
    }

    #[test]
    fn no_tools_manifest_skips_every_check_with_a_reason() {
        let manifest = manifest_with_tools(json!([]));
        let skips = no_tools_skips(&manifest).expect("empty tools means skip");
        assert_eq!(skips.len(), CHECKS.len());
        for (skip, name) in skips.iter().zip(CHECKS) {
            assert_eq!(&skip.check, name);
            assert_eq!(skip.level, Level::Skip, "{skip:?}");
            assert!(!skip.detail.is_empty(), "{skip:?}");
        }
        assert!(no_tools_skips(&demo_manifest()).is_none());
    }

    #[test]
    fn declared_check_passes_for_the_demo_manifest() {
        let manifest = demo_manifest();
        let result = declared_check(&manifest, &fixed_declaration(&manifest));
        assert_eq!(result.level, Level::Pass, "{result:?}");
        assert_eq!(result.evidence["tools"].as_array().map(Vec::len), Some(3));
    }

    #[test]
    fn declared_check_fails_on_duplicate_tool_names() {
        let manifest = manifest_with_tools(json!([
            tool("echo", "safe", json!({"type": "object"})),
            tool("echo", "safe", json!({"type": "object"})),
        ]));
        let result = declared_check(&manifest, &fixed_declaration(&manifest));
        assert_eq!(result.level, Level::Fail, "{result:?}");
        assert!(result.detail.contains("duplicate"), "{}", result.detail);
    }

    #[test]
    fn declared_check_fails_on_undeclared_resource_and_bad_schema() {
        let mut bad_resource = tool("echo", "safe", json!({"type": "object"}));
        bad_resource["resources"] = json!(["missing"]);
        let manifest = manifest_with_tools(json!([bad_resource]));
        let result = declared_check(&manifest, &fixed_declaration(&manifest));
        assert_eq!(result.level, Level::Fail, "{result:?}");

        let manifest = manifest_with_tools(json!([tool("echo", "safe", json!("not-an-object"))]));
        let result = declared_check(&manifest, &fixed_declaration(&manifest));
        assert_eq!(result.level, Level::Fail, "{result:?}");
    }

    #[test]
    fn invalid_params_check_rejects_an_echo_wrong_type() {
        let result = invalid_params_check(&demo_declaration());
        assert_eq!(result.level, Level::Pass, "{result:?}");
        let error = result.evidence["error"].as_str().expect("rejection message");
        assert!(error.contains("message"), "path missing from {error}");
        assert_eq!(result.evidence["accepted"], json!(false));
    }

    #[test]
    fn invalid_params_check_fails_when_the_schema_accepts_the_value() {
        // Directly exercise the fail-if-accepted branch: a valid echo payload
        // must never be reported as rejected evidence.
        let decl = demo_declaration();
        let echo = decl.tools.iter().find(|tool| tool.name == "echo").expect("demo echo");
        let result = rejected_params_check(echo, &json!({"message": "hi"}), "wrong type");
        assert_eq!(result.level, Level::Fail, "{result:?}");
        assert_eq!(result.evidence["accepted"], json!(true));
    }

    #[test]
    fn invalid_params_out_of_range_uses_the_declared_bound() {
        let manifest = manifest_with_tools(json!([tool(
            "set_level",
            "safe",
            json!({
                "type": "object",
                "properties": {"level": {"type": "integer", "minimum": 0, "maximum": 100}},
                "required": ["level"],
                "additionalProperties": false
            })
        )]));
        let decl = fixed_declaration(&manifest);
        let result = invalid_params_check(&decl);
        assert_eq!(result.level, Level::Pass, "{result:?}");
        assert_eq!(result.evidence["params"]["level"], json!(-1));
        let error = result.evidence["error"].as_str().expect("rejection message");
        assert!(error.contains("minimum"), "range missing from {error}");
        assert_eq!(decl.tools[0].kind, ToolKindDecl::Action);
    }

    #[test]
    fn undeclared_tool_is_rejected() {
        let result = undeclared_check(&demo_declaration());
        assert_eq!(result.level, Level::Pass, "{result:?}");
        assert_eq!(result.evidence["rejected"], json!(true));
        assert!(result.detail.contains("rejected"), "{}", result.detail);
    }

    #[test]
    fn control_and_dangerous_tools_are_withheld() {
        let manifest = manifest_with_tools(json!([
            tool("safe_tool", "safe", json!({"type": "object"})),
            tool("set_level", "control", json!({"type": "object"})),
            tool("reboot", "dangerous", json!({"type": "object"})),
        ]));
        let result = control_skip_check(&fixed_declaration(&manifest));
        assert_eq!(result.level, Level::Skip, "{result:?}");
        let names: Vec<&str> = result.evidence["withheld"]
            .as_array()
            .expect("withheld list")
            .iter()
            .filter_map(|entry| entry["name"].as_str())
            .collect();
        assert_eq!(names, vec!["set_level", "reboot"]);
        assert!(result.detail.contains("not auto-executed"), "{}", result.detail);
    }

    #[test]
    fn plan_excludes_a_safe_tool_with_a_long_max_timeout() {
        let manifest = manifest_with_tools(json!([
            // 1s default but a 10-minute max: could block the suite.
            tool_with_timeout("blocking", "safe", json!({"type": "object"}), 1_000, 600_000),
            tool_with_timeout(
                "echo",
                "safe",
                json!({
                    "type": "object",
                    "properties": {"message": {"type": "string"}},
                    "required": ["message"],
                }),
                1_000,
                2_000,
            ),
        ]));
        let plan = plan_safe_tools(&manifest.tools);
        assert_eq!(plan.calls.len(), 1, "excluded: {:?}", plan.excluded);
        assert_eq!(plan.calls[0].tool.name, "echo");
        let excluded: Vec<&str> = plan
            .excluded
            .iter()
            .filter_map(|entry| entry["name"].as_str())
            .collect();
        assert_eq!(excluded, vec!["blocking"]);
        let reason = plan.excluded[0]["reason"].as_str().expect("reason");
        assert!(reason.contains("max 600000ms"), "{reason}");
    }

    #[test]
    fn synthesized_params_clamp_to_maximum_and_parse_float_minimum() {
        // `maximum` without `minimum` used to fall back to 0 and violate the
        // schema; the valid example must clamp to -1.
        let only_max = json!({
            "type": "object",
            "properties": {"n": {"type": "integer", "maximum": -1}},
            "required": ["n"],
        });
        let params = valid_params(&only_max).expect("synthesize");
        assert_eq!(params["n"], json!(-1), "{params}");
        validate_params(&only_max, &params).expect("clamped params must validate");

        // A float-form minimum (`1.0`) must still produce an integer payload.
        let float_min = json!({
            "type": "object",
            "properties": {"n": {"type": "integer", "minimum": 1.0}},
            "required": ["n"],
        });
        let params = valid_params(&float_min).expect("synthesize");
        assert_eq!(params["n"], json!(1), "{params}");
        assert!(params["n"].is_i64(), "integer payload expected: {params}");
        validate_params(&float_min, &params).expect("float-minimum params must validate");

        let manifest = manifest_with_tools(json!([tool_with_timeout("bounded", "safe", only_max, 1_000, 2_000,)]));
        let plan = plan_safe_tools(&manifest.tools);
        assert_eq!(plan.calls.len(), 1, "excluded: {:?}", plan.excluded);
        assert_eq!(plan.calls[0].params["n"], json!(-1));
    }

    #[test]
    fn undeclared_check_fails_on_a_non_undeclared_error() {
        for err in [
            CallError::NoDeclaration("thing".into()),
            CallError::InvalidParams("missing required field".into()),
            CallError::Unavailable("declaration store".into()),
        ] {
            let result = undeclared_result_check(Err(err));
            assert_eq!(result.level, Level::Fail, "{result:?}");
            assert_eq!(result.evidence["rejected"], json!(false));
        }
    }

    #[test]
    fn demo_package_executes_echo_and_withholds_set_level() {
        if !demo_built() {
            eprintln!(
                "SKIP tools demo test: real cdylib absent; run `cargo build -p demo-declarative` \
                 (Task 6 E2E must exercise the real .so path)"
            );
            return;
        }
        let report = run(&demo_package_dir());
        assert_eq!(report.level(), Level::Pass, "{report:?}");
        for name in [
            "tools.declared",
            "tools.safe_roundtrip",
            "tools.invalid_params",
            "tools.undeclared",
        ] {
            assert_eq!(level_of(&report, name), Some(Level::Pass), "check {name}: {report:?}");
        }
        assert_eq!(
            level_of(&report, "tools.control_skip"),
            Some(Level::Skip),
            "control tools must be withheld: {report:?}"
        );

        let roundtrip = report
            .checks
            .iter()
            .find(|c| c.check == "tools.safe_roundtrip")
            .expect("safe roundtrip check");
        let executed: Vec<&str> = roundtrip.evidence["executed"]
            .as_array()
            .expect("executed list")
            .iter()
            .filter(|entry| entry["executed"] == json!(true))
            .filter_map(|entry| entry["tool"].as_str())
            .collect();
        assert_eq!(executed, vec!["echo"], "{roundtrip:?}");
        let excluded: Vec<&str> = roundtrip.evidence["excluded"]
            .as_array()
            .expect("excluded list")
            .iter()
            .filter_map(|entry| entry["name"].as_str())
            .collect();
        assert_eq!(
            excluded,
            vec!["long_task"],
            "long_task must not auto-run: {roundtrip:?}"
        );

        let control = report
            .checks
            .iter()
            .find(|c| c.check == "tools.control_skip")
            .expect("control skip check");
        let withheld: Vec<&str> = control.evidence["withheld"]
            .as_array()
            .expect("withheld list")
            .iter()
            .filter_map(|entry| entry["name"].as_str())
            .collect();
        assert!(withheld.contains(&"set_level"), "{control:?}");
    }
}
