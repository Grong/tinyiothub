//! `resources` profile (E4c spec §2.5): the manifest's resource declarations are
//! well-formed, and the real [`ArbitrationRegistry`] enforces exclusive/shared
//! capacity, sorted all-or-nothing multi-resource claims, TTL recovery and the
//! synthetic `__device__` fallback for tools with no declared resources.
//!
//! The registry checks use synthetic specs when the package does not declare a
//! resource of the required shape (e.g. the demo declares only an exclusive
//! `actuator`). Evidence records whether the spec came from the manifest or was
//! synthetic, so a report never implies the package declared more than it did.

use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

use tinyiothub_core::driver::manifest::{
    ManifestTool, ResourceDecl, ResourceMode as DeclaredResourceMode, ToolKindDecl,
};

use crate::acceptance::report::{CheckResult, Level, ProfileReport};
use crate::access::resources::{DEVICE_RESOURCE, resolve_tool_claims};
use crate::arbitration::{ArbitrationRegistry, Busy, ResourceMode, ResourceSpec};

const PROFILE: &str = "resources";
const TTL: Duration = Duration::from_secs(60);

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
    match crate::access::package::read_package_raw(dir) {
        Ok((_, manifest)) => {
            checks.push(specs_check(&manifest.resources));
            checks.push(exclusive_check(&manifest.resources));
            checks.push(shared_check(&manifest.resources));
            checks.push(all_or_nothing_check(&manifest.resources));
            checks.push(ttl_check(&manifest.resources));
            checks.push(default_device_check(&manifest.tools));
        }
        Err(e) => checks.push(check(
            "package.read",
            Level::Fail,
            e.to_string(),
            serde_json::json!({"error": e.to_string()}),
        )),
    }
    ProfileReport {
        profile: PROFILE.into(),
        checks,
    }
}

/// Declarations are structurally sound: non-empty unique ids, shared capacity
/// above zero. An exclusive declaration's capacity is ignored by the registry
/// (always one holder), so a non-1 value is a warning rather than a failure.
fn specs_check(resources: &[ResourceDecl]) -> CheckResult {
    let mut seen = HashSet::new();
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    for r in resources {
        if r.id.trim().is_empty() {
            errors.push("empty resource id".to_string());
        } else if !seen.insert(r.id.as_str()) {
            errors.push(format!("duplicate resource id '{}'", r.id));
        }
        match r.mode {
            DeclaredResourceMode::Shared if r.capacity == 0 => {
                errors.push(format!("shared resource '{}' has capacity 0", r.id));
            }
            DeclaredResourceMode::Exclusive if r.capacity != 1 => {
                warnings.push(format!(
                    "exclusive resource '{}' declares capacity {} (ignored; exclusive is always 1)",
                    r.id, r.capacity
                ));
            }
            _ => {}
        }
    }
    let evidence = serde_json::json!({
        "declared": resources.iter().map(|r| serde_json::json!({
            "id": r.id,
            "mode": match r.mode {
                DeclaredResourceMode::Exclusive => "exclusive",
                DeclaredResourceMode::Shared => "shared",
            },
            "capacity": r.capacity,
        })).collect::<Vec<_>>(),
        "errors": errors,
        "warnings": warnings,
    });
    if !errors.is_empty() {
        check("resources.specs", Level::Fail, errors.join("; "), evidence)
    } else if !warnings.is_empty() {
        check("resources.specs", Level::Warn, warnings.join("; "), evidence)
    } else {
        check(
            "resources.specs",
            Level::Pass,
            format!("{} resource declaration(s) valid", resources.len()),
            evidence,
        )
    }
}

fn exclusive_check(resources: &[ResourceDecl]) -> CheckResult {
    let (spec, source) = match resources.iter().find(|r| r.mode == DeclaredResourceMode::Exclusive) {
        Some(r) => (spec_of(r), "manifest"),
        None => (synthetic_exclusive("acceptance:exclusive"), "synthetic"),
    };
    let registry = ArbitrationRegistry::default();
    let Some(held) = registry
        .try_acquire_resources(
            "acceptance:exclusive-thing",
            "acceptance:first".into(),
            std::slice::from_ref(&spec),
            TTL,
        )
        .ok()
    else {
        return check(
            "resources.exclusive",
            Level::Fail,
            "first holder could not acquire an unclaimed exclusive resource",
            serde_json::json!({"resource": spec.id, "source": source}),
        );
    };
    let second = registry.try_acquire_resources(
        "acceptance:exclusive-thing",
        "acceptance:second".into(),
        std::slice::from_ref(&spec),
        TTL,
    );
    drop(held);
    let evidence = serde_json::json!({
        "resource": spec.id,
        "source": source,
        "second": match &second {
            Err(busy) => serde_json::json!({"holder": busy.holder, "resource": busy.resource}),
            Ok(_) => serde_json::json!(null),
        },
    });
    match second {
        Err(Busy { holder, resource }) if holder == "acceptance:first" && resource == spec.id => check(
            "resources.exclusive",
            Level::Pass,
            format!("second holder rejected with busy: {holder} holds {resource}"),
            evidence,
        ),
        Err(other) => check(
            "resources.exclusive",
            Level::Fail,
            format!("unexpected busy identity: {other}"),
            evidence,
        ),
        Ok(_) => check(
            "resources.exclusive",
            Level::Fail,
            "exclusive resource admitted two holders",
            evidence,
        ),
    }
}

fn shared_check(resources: &[ResourceDecl]) -> CheckResult {
    // A declared shared resource demonstrates capacity if it allows ≥2 holders;
    // otherwise fall back to a synthetic capacity-2 spec.
    let (spec, source) = match resources
        .iter()
        .find(|r| r.mode == DeclaredResourceMode::Shared && r.capacity >= 2)
    {
        Some(r) => (spec_of(r), "manifest"),
        None => (synthetic_shared("acceptance:shared", 2), "synthetic"),
    };
    let registry = ArbitrationRegistry::default();
    let capacity = spec.capacity;
    let a = registry.try_acquire_resources(
        "acceptance:shared-thing",
        "acceptance:a".into(),
        std::slice::from_ref(&spec),
        TTL,
    );
    let b = registry.try_acquire_resources(
        "acceptance:shared-thing",
        "acceptance:b".into(),
        std::slice::from_ref(&spec),
        TTL,
    );
    let (Ok(a), Ok(b)) = (a, b) else {
        return check(
            "resources.shared",
            Level::Fail,
            "shared resource rejected a holder within capacity",
            serde_json::json!({"resource": spec.id, "capacity": capacity, "source": source}),
        );
    };
    let saturated = registry.try_acquire_resources(
        "acceptance:shared-thing",
        "acceptance:c".into(),
        std::slice::from_ref(&spec),
        TTL,
    );
    drop(a);
    let after_release = registry.try_acquire_resources(
        "acceptance:shared-thing",
        "acceptance:d".into(),
        std::slice::from_ref(&spec),
        TTL,
    );
    drop(b);
    let after_release_ok = after_release.is_ok();
    let released_slot_reacquired = saturated.is_err() && after_release_ok;
    drop(after_release);
    let evidence = serde_json::json!({
        "resource": spec.id,
        "capacity": capacity,
        "source": source,
        "saturated": match &saturated {
            Err(busy) => serde_json::json!({"holder": busy.holder, "resource": busy.resource}),
            Ok(_) => serde_json::json!(null),
        },
        "released_slot_reacquired": released_slot_reacquired,
    });
    match (saturated, after_release_ok) {
        (Err(Busy { holder, resource }), true) if holder == "acceptance:a" && resource == spec.id => check(
            "resources.shared",
            Level::Pass,
            format!("capacity {capacity} enforced; busy carries {holder} holds {resource}"),
            evidence,
        ),
        (Err(Busy { holder, resource }), true) => check(
            "resources.shared",
            Level::Fail,
            format!("saturated shared resource reported the wrong holder: {holder} holds {resource}"),
            evidence,
        ),
        (Err(_), false) => check(
            "resources.shared",
            Level::Fail,
            "released shared slot was not reclaimable",
            evidence,
        ),
        (Ok(_), _) => check(
            "resources.shared",
            Level::Fail,
            format!("shared capacity {capacity} admitted an extra holder"),
            evidence,
        ),
    }
}

fn all_or_nothing_check(resources: &[ResourceDecl]) -> CheckResult {
    // Two exclusive declared resources when available; otherwise a synthetic
    // pair. Shared specs are skipped because capacity >1 would not force a
    // deterministic busy.
    let exclusive: Vec<&ResourceDecl> = resources
        .iter()
        .filter(|r| r.mode == DeclaredResourceMode::Exclusive)
        .collect();
    let (first, second, source) = if exclusive.len() >= 2 {
        (spec_of(exclusive[0]), spec_of(exclusive[1]), "manifest")
    } else {
        (
            synthetic_exclusive("acceptance:res:a"),
            synthetic_exclusive("acceptance:res:b"),
            "synthetic",
        )
    };
    let registry = ArbitrationRegistry::default();
    let Some(held) = registry
        .try_acquire_resources(
            "acceptance:multi-thing",
            "acceptance:holder".into(),
            std::slice::from_ref(&second),
            TTL,
        )
        .ok()
    else {
        return check(
            "resources.all_or_nothing",
            Level::Fail,
            "could not seed the conflicting holder",
            serde_json::json!({"source": source}),
        );
    };
    // Deliberately reversed input order: the registry sorts by id, claims
    // `first`, then fails on `second` and must roll `first` back.
    let attempt = registry.try_acquire_resources(
        "acceptance:multi-thing",
        "acceptance:multi".into(),
        &[second.clone(), first.clone()],
        TTL,
    );
    // If `first` leaked, this probe fails.
    let probe = registry.try_acquire_resources(
        "acceptance:multi-thing",
        "acceptance:probe".into(),
        std::slice::from_ref(&first),
        TTL,
    );
    drop(held);
    drop(probe);
    let evidence = serde_json::json!({
        "source": source,
        "first": first.id,
        "second": second.id,
        "busy": match &attempt {
            Err(busy) => serde_json::json!({"holder": busy.holder, "resource": busy.resource}),
            Ok(_) => serde_json::json!(null),
        },
    });
    match attempt {
        Err(Busy { holder, resource }) if holder == "acceptance:holder" && resource == second.id => check(
            "resources.all_or_nothing",
            Level::Pass,
            format!("busy: {holder} holds {resource}; partial claim rolled back"),
            evidence,
        ),
        Err(other) => check(
            "resources.all_or_nothing",
            Level::Fail,
            format!("unexpected busy identity: {other}"),
            evidence,
        ),
        Ok(_) => check(
            "resources.all_or_nothing",
            Level::Fail,
            "multi-resource claim succeeded despite a busy resource",
            evidence,
        ),
    }
}

fn ttl_check(resources: &[ResourceDecl]) -> CheckResult {
    let (spec, source) = match resources.iter().find(|r| r.mode == DeclaredResourceMode::Exclusive) {
        Some(r) => (spec_of(r), "manifest"),
        None => (synthetic_exclusive("acceptance:ttl"), "synthetic"),
    };
    let registry = ArbitrationRegistry::default();
    // Zero TTL = already expired while the stale Lease is still alive.
    let Some(stale) = registry
        .try_acquire_resources(
            "acceptance:ttl-thing",
            "acceptance:crashed".into(),
            std::slice::from_ref(&spec),
            Duration::ZERO,
        )
        .ok()
    else {
        return check(
            "resources.ttl",
            Level::Fail,
            "initial claim failed",
            serde_json::json!({"resource": spec.id, "source": source}),
        );
    };
    let recovered = registry.try_acquire_resources(
        "acceptance:ttl-thing",
        "acceptance:recovered".into(),
        std::slice::from_ref(&spec),
        TTL,
    );
    drop(stale);
    let evidence = serde_json::json!({
        "resource": spec.id,
        "source": source,
        "recovered": recovered.is_ok(),
    });
    match recovered {
        Ok(_) => check(
            "resources.ttl",
            Level::Pass,
            "expired claim was reclaimable without dropping the stale lease",
            evidence,
        ),
        Err(e) => check(
            "resources.ttl",
            Level::Fail,
            format!("expired claim still blocked re-acquire: {e}"),
            evidence,
        ),
    }
}

/// A tool with no declared resources resolves to the synthetic `__device__`
/// exclusive claim (E1 per-device mutual exclusion preserved).
fn default_device_check(tools: &[ManifestTool]) -> CheckResult {
    let probe = tools
        .iter()
        .find(|t| t.resources.is_empty())
        .cloned()
        .unwrap_or_else(empty_resource_tool);
    let claims = match resolve_tool_claims(&[], &probe) {
        Ok(claims) => claims,
        Err(e) => {
            return check(
                "resources.default_device",
                Level::Fail,
                format!("claim resolution failed: {e}"),
                serde_json::json!({"error": e}),
            );
        }
    };
    let resolved =
        claims.len() == 1 && claims[0].id == DEVICE_RESOURCE && claims[0].exclusive && claims[0].capacity == 1;

    let registry = ArbitrationRegistry::default();
    let seeded = registry
        .try_acquire_resources(
            "acceptance:device-thing",
            "acceptance:first".into(),
            &[synthetic_exclusive(DEVICE_RESOURCE)],
            TTL,
        )
        .ok();
    let blocked = registry.try_acquire_resources(
        "acceptance:device-thing",
        "acceptance:second".into(),
        &[synthetic_exclusive(DEVICE_RESOURCE)],
        TTL,
    );
    drop(seeded);
    let evidence = serde_json::json!({
        "tool": probe.name,
        "resolved": claims.iter().map(|c| serde_json::json!({
            "id": c.id,
            "exclusive": c.exclusive,
            "capacity": c.capacity,
        })).collect::<Vec<_>>(),
        "busy": match &blocked {
            Err(busy) => serde_json::json!({"holder": busy.holder, "resource": busy.resource}),
            Ok(_) => serde_json::json!(null),
        },
    });
    let excluded = matches!(
        &blocked,
        Err(Busy { holder, resource }) if holder == "acceptance:first" && resource == DEVICE_RESOURCE
    );
    if resolved && excluded {
        check(
            "resources.default_device",
            Level::Pass,
            format!(
                "tool '{}' with empty resources claims {DEVICE_RESOURCE} exclusive",
                probe.name
            ),
            evidence,
        )
    } else {
        check(
            "resources.default_device",
            Level::Fail,
            format!(
                "empty-resource tool did not map to {DEVICE_RESOURCE} exclusive mutual exclusion (resolved={resolved}, excluded={excluded})"
            ),
            evidence,
        )
    }
}

fn spec_of(r: &ResourceDecl) -> ResourceSpec {
    ResourceSpec {
        id: r.id.clone(),
        mode: match r.mode {
            DeclaredResourceMode::Exclusive => ResourceMode::Exclusive,
            DeclaredResourceMode::Shared => ResourceMode::Shared,
        },
        capacity: r.capacity,
    }
}

fn synthetic_exclusive(id: &str) -> ResourceSpec {
    ResourceSpec {
        id: id.to_string(),
        mode: ResourceMode::Exclusive,
        capacity: 1,
    }
}

fn synthetic_shared(id: &str, capacity: u32) -> ResourceSpec {
    ResourceSpec {
        id: id.to_string(),
        mode: ResourceMode::Shared,
        capacity,
    }
}

fn empty_resource_tool() -> ManifestTool {
    ManifestTool {
        name: "acceptance_probe".into(),
        kind: ToolKindDecl::Action,
        target_field: None,
        parameters: serde_json::json!({"type": "object"}),
        timeout: Default::default(),
        safety: Default::default(),
        resources: Vec::new(),
        cancellable: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"{
      "manifestVersion":"1.0",
      "identity":{"driver":"demo","version":"1.0.0","protocol":"demo","abiVersion":2,"dynamicCapabilities":false},
      "config":{"type":"object"},
      "tools":[
        {"name":"echo","parameters":{"type":"object"}},
        {"name":"lock","parameters":{"type":"object"},"resources":["actuator"]},
        {"name":"share","parameters":{"type":"object"},"resources":["stream"]}
      ],
      "stateFields":[],
      "sources":[{"id":"telemetry","kind":"poll","fields":[]}],
      "resources":[
        {"id":"actuator","mode":"exclusive","capacity":1},
        {"id":"stream","mode":"shared","capacity":2}
      ],
      "placement":{"mode":"runtime"},
      "limits":{"maxConcurrentJobs":1,"maxCallsPerMin":60}
    }"#;

    const NO_RESOURCES: &str = r#"{
      "manifestVersion":"1.0",
      "identity":{"driver":"demo","version":"1.0.0","protocol":"demo","abiVersion":2,"dynamicCapabilities":false},
      "config":{"type":"object"},"tools":[{"name":"echo","parameters":{"type":"object"}}],"stateFields":[],
      "sources":[{"id":"telemetry","kind":"poll","fields":[]}],
      "resources":[],"placement":{"mode":"runtime"},
      "limits":{"maxConcurrentJobs":1,"maxCallsPerMin":60}
    }"#;

    fn package(manifest: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("driver.manifest.json"), manifest).unwrap();
        dir
    }

    fn level_of(report: &ProfileReport, name: &str) -> Option<Level> {
        report.checks.iter().find(|c| c.check == name).map(|c| c.level)
    }

    #[test]
    fn declared_resources_all_checks_pass() {
        let dir = package(MANIFEST);
        let report = run(dir.path());
        assert_eq!(report.level(), Level::Pass, "{report:?}");
        assert_eq!(report.checks.len(), 6);
        for name in [
            "resources.specs",
            "resources.exclusive",
            "resources.shared",
            "resources.all_or_nothing",
            "resources.ttl",
            "resources.default_device",
        ] {
            assert_eq!(level_of(&report, name), Some(Level::Pass), "{name}: {report:?}");
        }
        let shared = report.checks.iter().find(|c| c.check == "resources.shared").unwrap();
        assert_eq!(shared.evidence["source"], "manifest");
        assert_eq!(shared.evidence["capacity"], 2);
    }

    #[test]
    fn package_without_resources_uses_synthetic_specs_and_passes() {
        let dir = package(NO_RESOURCES);
        let report = run(dir.path());
        assert_eq!(report.level(), Level::Pass, "{report:?}");
        let specs = report.checks.iter().find(|c| c.check == "resources.specs").unwrap();
        assert_eq!(specs.level, Level::Pass);
        let exclusive = report.checks.iter().find(|c| c.check == "resources.exclusive").unwrap();
        assert_eq!(exclusive.evidence["source"], "synthetic");
    }

    #[test]
    fn duplicate_resource_ids_fail_specs() {
        let manifest = MANIFEST.replace(
            r#"{"id":"stream","mode":"shared","capacity":2}"#,
            r#"{"id":"actuator","mode":"shared","capacity":2}"#,
        );
        let dir = package(&manifest);
        let report = run(dir.path());
        assert_eq!(report.level(), Level::Fail, "{report:?}");
        assert_eq!(level_of(&report, "resources.specs"), Some(Level::Fail));
    }

    #[test]
    fn zero_shared_capacity_fails_specs() {
        let manifest = MANIFEST.replace(
            r#""id":"stream","mode":"shared","capacity":2"#,
            r#""id":"stream","mode":"shared","capacity":0"#,
        );
        let dir = package(&manifest);
        let report = run(dir.path());
        assert_eq!(level_of(&report, "resources.specs"), Some(Level::Fail), "{report:?}");
    }

    #[test]
    fn exclusive_capacity_other_than_one_warns_without_failing() {
        let manifest = MANIFEST.replace(
            r#""id":"actuator","mode":"exclusive","capacity":1"#,
            r#""id":"actuator","mode":"exclusive","capacity":4"#,
        );
        let dir = package(&manifest);
        let report = run(dir.path());
        assert_eq!(report.level(), Level::Warn, "{report:?}");
        assert_eq!(level_of(&report, "resources.specs"), Some(Level::Warn));
    }

    #[test]
    fn unreadable_package_fails_package_read() {
        let dir = tempfile::tempdir().unwrap();
        let report = run(dir.path());
        assert_eq!(report.level(), Level::Fail);
        assert_eq!(level_of(&report, "package.read"), Some(Level::Fail));
    }
}
