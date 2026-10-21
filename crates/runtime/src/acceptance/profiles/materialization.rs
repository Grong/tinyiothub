//! `materialization` profile: the package's declaration validates and writing
//! it through the projection port yields exactly the declared fields/tools.

use std::path::Path;

use crate::acceptance::report::{CheckResult, Level, ProfileReport};
use crate::acceptance::test_hub::{TestHub, fixture_thing};

const PROFILE: &str = "materialization";

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
            let decl = if pkg.manifest.identity.dynamic_capabilities {
                crate::driver::describe::declaration_from_thing(
                    &fixture_thing(),
                    &pkg.manifest.identity.driver,
                    &pkg.manifest.identity.version,
                )
            } else {
                tinyiothub_core::driver::declaration::fixed_declaration(&pkg.manifest)
            };
            let valid = tinyiothub_core::driver::declaration::validate_device_declaration(&pkg.manifest, &decl);
            checks.push(check(
                "declaration.valid",
                if valid.is_ok() { Level::Pass } else { Level::Fail },
                format!("{valid:?}"),
            ));
            let hub = TestHub::new();
            let rt = tokio::runtime::Runtime::new().expect("build acceptance runtime");
            let res = rt.block_on(crate::access::materialize::materialize_declaration(
                hub.projection(),
                "test-thing",
                &decl,
            ));
            checks.push(check(
                "projection.write",
                if res.is_ok() { Level::Pass } else { Level::Fail },
                format!("{res:?}"),
            ));
            let (p, t) = hub.projection_counts();
            checks.push(check(
                "projection.count",
                if p == decl.state_fields.len() && t == decl.tools.len() {
                    Level::Pass
                } else {
                    Level::Fail
                },
                format!("props={p} tools={t}"),
            ));
        }
        Err(e) => checks.push(check("package.read", Level::Fail, e.to_string())),
    }
    ProfileReport {
        profile: PROFILE.into(),
        checks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXED_MANIFEST: &str = r#"{
      "manifestVersion":"1.0",
      "identity":{"driver":"demo","version":"1.0.0","protocol":"demo","abiVersion":2,"dynamicCapabilities":false},
      "config":{"type":"object"},
      "tools":[{"name":"echo","parameters":{"type":"object"}}],
      "stateFields":[{"name":"level","type":"integer"}],
      "sources":[{"id":"telemetry","kind":"poll","fields":["level"]}],
      "resources":[],"placement":{"mode":"runtime"},
      "limits":{"maxConcurrentJobs":1,"maxCallsPerMin":60}
    }"#;

    const DYNAMIC_MANIFEST: &str = r#"{
      "manifestVersion":"1.0",
      "identity":{"driver":"demo","version":"1.0.0","protocol":"demo","abiVersion":2,"dynamicCapabilities":true},
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
    fn fixed_package_projects_declared_counts() {
        let dir = package(FIXED_MANIFEST);
        let report = run(dir.path());
        assert_eq!(report.level(), Level::Pass, "{report:?}");
        assert_eq!(level_of(&report, "declaration.valid"), Some(Level::Pass));
        assert_eq!(level_of(&report, "projection.write"), Some(Level::Pass));
        assert_eq!(level_of(&report, "projection.count"), Some(Level::Pass));
        let detail = &report
            .checks
            .iter()
            .find(|c| c.check == "projection.count")
            .unwrap()
            .detail;
        assert_eq!(detail, "props=1 tools=1");
    }

    #[test]
    fn dynamic_package_uses_fixture_declaration() {
        let dir = package(DYNAMIC_MANIFEST);
        let report = run(dir.path());
        assert_eq!(report.level(), Level::Pass, "{report:?}");
        let detail = &report
            .checks
            .iter()
            .find(|c| c.check == "projection.count")
            .unwrap()
            .detail;
        assert_eq!(detail, "props=1 tools=1");
    }

    #[test]
    fn unreadable_package_fails() {
        let dir = tempfile::tempdir().unwrap();
        let report = run(dir.path());
        assert_eq!(report.level(), Level::Fail);
        assert_eq!(level_of(&report, "package.read"), Some(Level::Fail));
    }
}
