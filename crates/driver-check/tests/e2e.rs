//! End-to-end acceptance against the reference declarative driver.
//!
//! The binary coverage below is deterministic: run
//! `cargo build -p demo-declarative` before `cargo test -p driver-check`
//! (the workspace pre-push gate builds it too).

use std::path::{Path, PathBuf};

use tinyiothub_runtime::acceptance::report::Level;
use tinyiothub_runtime::acceptance::runner::run_profiles;

fn demo_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../drivers/demo-declarative")
}

#[test]
fn demo_declarative_passes_all_profiles() {
    let report = run_profiles(&demo_dir(), &["all"]);
    assert!(report.passed(), "{}", serde_json::to_string_pretty(&report).unwrap());
    assert_eq!(report.driver, "demo-declarative");
    assert_eq!(report.version, "1.0.0");
    let names: Vec<&str> = report.profiles.iter().map(|p| p.profile.as_str()).collect();
    assert_eq!(
        names,
        vec![
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
        ]
    );
}

#[test]
fn every_named_profile_runs_alone() {
    use tinyiothub_runtime::acceptance::runner::PROFILE_NAMES;

    for name in PROFILE_NAMES.iter().filter(|name| **name != "all") {
        let report = run_profiles(&demo_dir(), &[name]);
        assert_eq!(report.profiles.len(), 1, "{name}");
        assert_eq!(&report.profiles[0].profile, name, "{name}");
        assert_ne!(report.profiles[0].level(), Level::Fail, "{name}: {report:?}");
    }
}

#[test]
fn demo_placement_profile_skips_map_checks_without_failing() {
    let report = run_profiles(&demo_dir(), &["placement"]);
    let profile = &report.profiles[0];
    assert_eq!(profile.profile, "placement");
    assert_ne!(profile.level(), Level::Fail, "{report:?}");
    // The demo declares no mapRef/zoneRef/position: map-dependent checks skip
    // with evidence while structural checks still run.
    for name in [
        "validator.maps_bounds",
        "validator.zone_geometry",
        "validator.default_unique",
        "placement.refs",
    ] {
        let check = profile
            .checks
            .iter()
            .find(|c| c.check == name)
            .unwrap_or_else(|| panic!("{name}: {report:?}"));
        assert_eq!(check.level, Level::Pass, "{name}: {report:?}");
    }
    let skipped: Vec<&str> = profile
        .checks
        .iter()
        .filter(|c| c.level == Level::Skip)
        .map(|c| c.check.as_str())
        .collect();
    assert!(skipped.contains(&"zone.smallest_area"), "{report:?}");
    assert!(skipped.contains(&"trust.thresholds"), "{report:?}");
}

#[test]
fn cli_all_profile_runs_ten_profiles_and_exits_zero() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_driver-check"))
        .arg("--package")
        .arg(demo_dir())
        .arg("--profile")
        .arg("all")
        .output()
        .expect("run driver-check --profile all");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));

    let (report, summary) = split_output(&out);
    assert_eq!(report["driver"], "demo-declarative");
    assert_eq!(report["version"], "1.0.0");
    let profiles = report["profiles"].as_array().expect("profiles array");
    assert_eq!(profiles.len(), 10, "{report}");
    let names: Vec<&str> = profiles
        .iter()
        .map(|profile| profile["profile"].as_str().expect("profile name"))
        .collect();
    assert_eq!(
        names,
        vec![
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
        ]
    );
    let failed: Vec<&str> = profiles
        .iter()
        .filter(|profile| {
            profile["checks"]
                .as_array()
                .is_some_and(|checks| checks.iter().any(|check| check["level"] == "fail"))
        })
        .filter_map(|profile| profile["profile"].as_str())
        .collect();
    assert!(failed.is_empty(), "failing profiles: {failed:?}\n{report}");
    assert!(
        summary.contains("demo-declarative@1.0.0 -> PASS (10 profile(s)"),
        "{summary}"
    );
}

#[test]
fn inventory_lists_installed_demo_package_as_v2() {
    let lib =
        demo_cdylib().unwrap_or_else(|| panic!("build the reference cdylib first: cargo build -p demo-declarative"));
    let data_dir = tempfile::tempdir().unwrap();
    let package_dir = data_dir.path().join("drivers/workspaces/ws-e4e/demo-declarative/1.0.0");
    std::fs::create_dir_all(&package_dir).unwrap();
    std::fs::copy(
        demo_dir().join("driver.manifest.json"),
        package_dir.join("driver.manifest.json"),
    )
    .unwrap();
    let ext = lib.extension().and_then(|e| e.to_str()).unwrap_or("so");
    std::fs::copy(&lib, package_dir.join(format!("driver.{ext}"))).unwrap();

    let data_arg = data_dir.path().to_str().unwrap();
    for strict in [false, true] {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_driver-check"));
        command.args(["inventory", "--data-dir", data_arg]);
        if strict {
            command.arg("--strict");
        }
        let out = command.output().expect("run driver-check inventory");
        assert!(
            out.status.success(),
            "strict={strict}: {}",
            String::from_utf8_lossy(&out.stderr)
        );

        let (report, summary) = split_output(&out);
        let installed = report["installed"].as_array().expect("installed array");
        assert_eq!(installed.len(), 1, "{report}");
        let entry = &installed[0];
        assert_eq!(entry["workspace"], "ws-e4e");
        assert_eq!(entry["driver"], "demo-declarative");
        assert_eq!(entry["version"], "1.0.0");
        assert_eq!(entry["abi"], "v2", "{report}");
        assert_eq!(entry["manifestHash"].as_str().map(str::len), Some(64));
        assert_eq!(entry["binaryHash"].as_str().map(str::len), Some(64));
        assert!(entry["error"].is_null(), "{report}");
        assert_eq!(entry["path"], package_dir.display().to_string());
        assert!(
            summary.contains("1 installed package(s): v2=1"),
            "strict={strict}: {summary}"
        );
        assert!(summary.ends_with("-> OK"), "strict={strict}: {summary}");
    }
}

/// stdout is pretty JSON followed by the one-line human summary.
fn split_output(out: &std::process::Output) -> (serde_json::Value, String) {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let trimmed = stdout.trim_end();
    let (json, summary) = trimmed.rsplit_once('\n').expect("summary line");
    let value: serde_json::Value = serde_json::from_str(json).expect("CLI JSON");
    (value, summary.trim().to_string())
}

#[test]
fn cli_help_lists_every_profile() {
    use tinyiothub_runtime::acceptance::runner::PROFILE_NAMES;

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_driver-check"))
        .arg("--help")
        .output()
        .expect("run driver-check --help");
    assert!(out.status.success());
    let help = String::from_utf8_lossy(&out.stdout);
    for name in PROFILE_NAMES {
        assert!(help.contains(name), "--help must list profile '{name}': {help}");
    }
}

#[test]
fn demo_declarative_binary_passes_core_when_built() {
    let lib =
        demo_cdylib().unwrap_or_else(|| panic!("build the reference cdylib first: cargo build -p demo-declarative"));
    let dir = tempfile::tempdir().unwrap();
    std::fs::copy(
        demo_dir().join("driver.manifest.json"),
        dir.path().join("driver.manifest.json"),
    )
    .unwrap();
    let ext = lib.extension().and_then(|e| e.to_str()).unwrap_or("so");
    std::fs::copy(&lib, dir.path().join(format!("driver.{ext}"))).unwrap();

    let report = run_profiles(dir.path(), &["core"]);
    let binary = report.profiles[0]
        .checks
        .iter()
        .find(|c| c.check == "binary.abi_v2")
        .expect("binary check should run");
    assert_eq!(binary.level, Level::Pass, "{report:?}");
    assert!(report.passed(), "{report:?}");
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
