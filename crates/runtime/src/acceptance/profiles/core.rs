//! `core` profile: package loads, manifest schema, ABI identity, config schema
//! and — when a binary ships — ABI v2 dry-load.

use std::path::Path;

use crate::acceptance::report::{CheckResult, Level, ProfileReport};

const PROFILE: &str = "core";

fn check(name: &str, level: Level, detail: String) -> CheckResult {
    CheckResult {
        profile: PROFILE.into(),
        check: name.into(),
        level,
        detail,
        evidence: serde_json::Value::Null,
    }
}

pub fn run(dir: &Path) -> ProfileReport {
    let mut checks = Vec::new();
    match crate::access::package::read_package(dir) {
        Ok(pkg) => {
            let schema_ok = tinyiothub_core::driver::manifest::validate_manifest(&pkg.manifest).is_ok();
            checks.push(check(
                "manifest.schema",
                if schema_ok { Level::Pass } else { Level::Fail },
                schema_ok.to_string(),
            ));
            let abi = pkg.manifest.identity.abi_version;
            checks.push(check(
                "identity.abi",
                if abi == tinyiothub_core::driver::manifest::DRIVER_ABI_V2 {
                    Level::Pass
                } else {
                    Level::Fail
                },
                format!("abi={abi}"),
            ));
            checks.push(check(
                "config.schema",
                if pkg.manifest.config.is_object() {
                    Level::Pass
                } else {
                    Level::Fail
                },
                "config must be an object schema".into(),
            ));
            if let Some(bin) = &pkg.binary_path {
                let ok = crate::driver::validator::DriverValidator::validate(bin, "{}").is_ok();
                checks.push(check(
                    "binary.abi_v2",
                    if ok { Level::Pass } else { Level::Fail },
                    bin.display().to_string(),
                ));
            }
        }
        Err(e) => checks.push(check("manifest.load", Level::Fail, e.to_string())),
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
