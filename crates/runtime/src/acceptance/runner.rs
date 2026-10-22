//! Sync acceptance runner: selects profiles and builds the aggregate report.

use std::path::Path;

use crate::acceptance::profiles;
use crate::acceptance::report::{AcceptanceReport, ProfileReport};

pub const PROFILE_NAMES: &[&str] = &[
    "all",
    "core",
    "materialization",
    "migration",
    "resources",
    "placement",
    "cancel",
];

pub fn run_profiles(dir: &Path, profiles: &[&str]) -> AcceptanceReport {
    let pkg = crate::access::package::read_package(dir);
    let (driver, version) = pkg
        .as_ref()
        .map(|p| (p.manifest.identity.driver.clone(), p.manifest.identity.version.clone()))
        .unwrap_or_else(|_| ("<unreadable>".into(), "<unreadable>".into()));
    let mut reports: Vec<ProfileReport> = Vec::new();
    for p in profiles {
        match *p {
            "all" => {
                reports.push(profiles::core::run(dir));
                reports.push(profiles::materialization::run(dir));
                reports.push(profiles::migration::run());
                reports.push(profiles::resources::run(dir));
                reports.push(profiles::placement::run(dir));
                reports.push(profiles::cancel::run(dir));
            }
            "core" => reports.push(profiles::core::run(dir)),
            "materialization" => reports.push(profiles::materialization::run(dir)),
            "migration" => reports.push(profiles::migration::run()),
            "resources" => reports.push(profiles::resources::run(dir)),
            "placement" => reports.push(profiles::placement::run(dir)),
            "cancel" => reports.push(profiles::cancel::run(dir)),
            other => panic!("unknown acceptance profile '{other}'"),
        }
    }
    AcceptanceReport {
        package: dir.display().to_string(),
        driver,
        version,
        profiles: reports,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acceptance::report::Level;

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

    #[test]
    fn all_profiles_pass_for_a_valid_package() {
        let dir = package(MANIFEST);
        let report = run_profiles(dir.path(), &["all"]);
        assert_eq!(report.driver, "demo");
        assert_eq!(report.version, "1.0.0");
        assert_eq!(report.package, dir.path().display().to_string());
        assert_eq!(report.profiles.len(), 6);
        assert!(report.passed(), "{report:?}");
        for p in &report.profiles {
            assert_eq!(p.level(), Level::Pass, "{p:?}");
        }
    }

    #[test]
    fn resources_and_cancel_are_selectable_by_name() {
        let dir = package(MANIFEST);
        let resources = run_profiles(dir.path(), &["resources"]);
        assert_eq!(resources.profiles.len(), 1);
        assert_eq!(resources.profiles[0].profile, "resources");
        assert_eq!(resources.profiles[0].level(), Level::Pass, "{resources:?}");

        let cancel = run_profiles(dir.path(), &["cancel"]);
        assert_eq!(cancel.profiles.len(), 1);
        assert_eq!(cancel.profiles[0].profile, "cancel");
        assert_eq!(cancel.profiles[0].level(), Level::Pass, "{cancel:?}");

        let placement = run_profiles(dir.path(), &["placement"]);
        assert_eq!(placement.profiles.len(), 1);
        assert_eq!(placement.profiles[0].profile, "placement");
        assert_eq!(placement.profiles[0].level(), Level::Pass, "{placement:?}");
    }

    #[test]
    fn profile_selection_only_runs_requested_profiles() {
        let dir = package(MANIFEST);
        let report = run_profiles(dir.path(), &["migration"]);
        assert_eq!(report.profiles.len(), 1);
        assert_eq!(report.profiles[0].profile, "migration");
    }

    #[test]
    fn unreadable_package_is_reported_as_failed() {
        let dir = tempfile::tempdir().unwrap();
        let report = run_profiles(dir.path(), &["core"]);
        assert_eq!(report.driver, "<unreadable>");
        assert!(!report.passed());
    }

    #[test]
    #[should_panic(expected = "unknown acceptance profile")]
    fn unknown_profile_panics() {
        let dir = package(MANIFEST);
        run_profiles(dir.path(), &["nope"]);
    }
}
