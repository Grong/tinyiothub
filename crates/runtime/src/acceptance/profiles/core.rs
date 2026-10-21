//! `core` profile: package loads, manifest schema, ABI identity, config schema
//! and — when a binary ships — ABI v2 dry-load.

use std::path::Path;

use crate::acceptance::report::{CheckResult, Level, ProfileReport};

const PROFILE: &str = "core";

fn check(name: &str, level: Level, detail: String, evidence: serde_json::Value) -> CheckResult {
    CheckResult {
        profile: PROFILE.into(),
        check: name.into(),
        level,
        detail,
        evidence,
    }
}

pub fn run(dir: &Path) -> ProfileReport {
    let mut checks = Vec::new();
    // Parse without validation so the checks below stay independent: a
    // manifest that fails `validate_manifest` must be reported by
    // `manifest.schema` / `identity.abi`, not swallowed by `manifest.load`.
    match crate::access::package::read_package_raw(dir) {
        Ok((manifest_json, manifest)) => {
            let schema = tinyiothub_core::driver::manifest::validate_manifest(&manifest);
            checks.push(check(
                "manifest.schema",
                if schema.is_ok() { Level::Pass } else { Level::Fail },
                match &schema {
                    Ok(()) => "manifest schema valid".into(),
                    Err(e) => e.to_string(),
                },
                serde_json::json!({
                    "valid": schema.is_ok(),
                    "manifestVersion": manifest.manifest_version,
                    "driver": manifest.identity.driver,
                    "bytes": manifest_json.len(),
                }),
            ));
            let abi = manifest.identity.abi_version;
            checks.push(check(
                "identity.abi",
                if abi == tinyiothub_core::driver::manifest::DRIVER_ABI_V2 {
                    Level::Pass
                } else {
                    Level::Fail
                },
                format!("abi={abi}"),
                serde_json::json!({
                    "abiVersion": abi,
                    "required": tinyiothub_core::driver::manifest::DRIVER_ABI_V2,
                }),
            ));
            let config_ok = manifest.config.is_object();
            checks.push(check(
                "config.schema",
                if config_ok { Level::Pass } else { Level::Fail },
                "config must be an object schema".into(),
                serde_json::json!({
                    "configType": match &manifest.config {
                        serde_json::Value::Null => "null",
                        serde_json::Value::Bool(_) => "bool",
                        serde_json::Value::Number(_) => "number",
                        serde_json::Value::String(_) => "string",
                        serde_json::Value::Array(_) => "array",
                        serde_json::Value::Object(_) => "object",
                    },
                }),
            ));
            if let Some(bin) = crate::access::package::find_binary(dir) {
                let result = crate::driver::validator::DriverValidator::validate(&bin, "{}");
                checks.push(check(
                    "binary.abi_v2",
                    if result.is_ok() { Level::Pass } else { Level::Fail },
                    match &result {
                        Ok(()) => bin.display().to_string(),
                        Err(e) => format!("{}: {e}", bin.display()),
                    },
                    serde_json::json!({"binary": bin.display().to_string()}),
                ));
            }
        }
        Err(e) => checks.push(check(
            "manifest.load",
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const MANIFEST: &str = r#"{
      "manifestVersion":"1.0",
      "identity":{"driver":"demo","version":"1.0.0","protocol":"demo","abiVersion":2,"dynamicCapabilities":false},
      "config":{"type":"object"},"tools":[],"stateFields":[],
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
    fn manifest_only_package_passes_without_binary_check() {
        let dir = package(MANIFEST);
        let report = run(dir.path());
        assert_eq!(report.level(), Level::Pass);
        assert_eq!(level_of(&report, "manifest.schema"), Some(Level::Pass));
        assert_eq!(level_of(&report, "identity.abi"), Some(Level::Pass));
        assert_eq!(level_of(&report, "config.schema"), Some(Level::Pass));
        assert_eq!(level_of(&report, "binary.abi_v2"), None);
    }

    #[test]
    fn unreadable_package_fails_manifest_load() {
        let dir = tempfile::tempdir().unwrap();
        let report = run(dir.path());
        assert_eq!(report.level(), Level::Fail);
        assert_eq!(level_of(&report, "manifest.load"), Some(Level::Fail));
    }

    #[test]
    fn malformed_json_fails_manifest_load() {
        let dir = package("{not json");
        let report = run(dir.path());
        assert_eq!(report.level(), Level::Fail);
        assert_eq!(level_of(&report, "manifest.load"), Some(Level::Fail));
        assert_eq!(level_of(&report, "manifest.schema"), None);
    }

    #[test]
    fn schema_and_identity_checks_are_independent_of_loading() {
        let dir = package(&MANIFEST.replace("\"abiVersion\":2", "\"abiVersion\":1"));
        let report = run(dir.path());
        assert_eq!(report.level(), Level::Fail, "{report:?}");
        assert_eq!(level_of(&report, "manifest.load"), None);
        assert_eq!(level_of(&report, "manifest.schema"), Some(Level::Fail));
        assert_eq!(level_of(&report, "identity.abi"), Some(Level::Fail));
        assert_eq!(level_of(&report, "config.schema"), Some(Level::Pass));

        let schema_check = report.checks.iter().find(|c| c.check == "manifest.schema").unwrap();
        assert_eq!(schema_check.evidence["manifestVersion"], "1.0");
    }

    #[test]
    fn non_object_config_fails_config_schema_only() {
        let dir = package(&MANIFEST.replace("\"config\":{\"type\":\"object\"}", "\"config\":null"));
        let report = run(dir.path());
        assert_eq!(level_of(&report, "manifest.schema"), Some(Level::Pass));
        assert_eq!(level_of(&report, "config.schema"), Some(Level::Fail));
        assert_eq!(report.level(), Level::Fail);
    }

    #[test]
    fn binary_abi_v2_is_validated_when_present() {
        let Some(lib) = demo_cdylib() else {
            eprintln!("skipping binary test: demo-declarative cdylib not built");
            return;
        };
        let dir = package(MANIFEST);
        let ext = lib.extension().and_then(|e| e.to_str()).unwrap_or("so");
        std::fs::copy(&lib, dir.path().join(format!("driver.{ext}"))).unwrap();
        let report = run(dir.path());
        assert_eq!(level_of(&report, "binary.abi_v2"), Some(Level::Pass), "{report:?}");
        assert_eq!(report.level(), Level::Pass);
    }

    fn demo_cdylib() -> Option<PathBuf> {
        let target = std::env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"));
        let profile = if cfg!(debug_assertions) { "debug" } else { "release" };
        [
            "libdemo_declarative.dylib",
            "libdemo_declarative.so",
            "demo_declarative.dll",
            "libdemo_declarative.dll",
        ]
        .iter()
        .map(|name| target.join(profile).join(name))
        .find(|path| path.is_file())
    }
}
