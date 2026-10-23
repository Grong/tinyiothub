//! Sync acceptance runner: selects profiles and builds the aggregate report.

use std::path::Path;

use crate::acceptance::profiles;
use crate::acceptance::report::{AcceptanceReport, ProfileReport};

/// `all` first, then every concrete profile in the stable order used by
/// `run_profiles("all")`.
pub const PROFILE_NAMES: &[&str] = &[
    "all",
    "core",
    "materialization",
    "migration",
    "lifecycle",
    "telemetry",
    "tools",
    "streaming",
    "resources",
    "cancel",
    "placement",
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
                reports.push(profiles::lifecycle::run(dir));
                reports.push(profiles::telemetry::run(dir));
                reports.push(profiles::tools::run(dir));
                reports.push(profiles::streaming::run(dir));
                reports.push(profiles::resources::run(dir));
                reports.push(profiles::cancel::run(dir));
                reports.push(profiles::placement::run(dir));
            }
            "core" => reports.push(profiles::core::run(dir)),
            "materialization" => reports.push(profiles::materialization::run(dir)),
            "migration" => reports.push(profiles::migration::run()),
            "lifecycle" => reports.push(profiles::lifecycle::run(dir)),
            "telemetry" => reports.push(profiles::telemetry::run(dir)),
            "tools" => reports.push(profiles::tools::run(dir)),
            "streaming" => reports.push(profiles::streaming::run(dir)),
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
    fn all_runs_every_profile_in_the_stable_declared_order() {
        let dir = package(MANIFEST);
        let report = run_profiles(dir.path(), &["all"]);
        assert_eq!(report.driver, "demo");
        assert_eq!(report.version, "1.0.0");
        assert_eq!(report.package, dir.path().display().to_string());
        let names: Vec<&str> = report.profiles.iter().map(|p| p.profile.as_str()).collect();
        assert_eq!(names, PROFILE_NAMES[1..].to_vec());
        assert_eq!(names.len(), 10);
    }

    #[test]
    fn profile_names_are_unique_and_all_comes_first() {
        assert_eq!(PROFILE_NAMES[0], "all");
        let mut seen = std::collections::HashSet::new();
        for name in PROFILE_NAMES {
            assert!(seen.insert(*name), "duplicate profile name: {name}");
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
