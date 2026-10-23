//! CLI coverage for `driver-check inventory` (E4e §3).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const MANIFEST: &str = r#"{
  "manifestVersion":"1.0",
  "identity":{"driver":"demo","version":"1.0.0","protocol":"demo","abiVersion":2,"dynamicCapabilities":false},
  "config":{"type":"object"},"tools":[],"stateFields":[],
  "sources":[{"id":"telemetry","kind":"poll","fields":[]}],
  "resources":[],"placement":{"mode":"runtime"},
  "limits":{"maxConcurrentJobs":1,"maxCallsPerMin":60}
}"#;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_driver-check")
}

fn package(data_dir: &Path, workspace: &str, driver: &str, version: &str) -> PathBuf {
    let dir = data_dir
        .join("drivers/workspaces")
        .join(workspace)
        .join(driver)
        .join(version);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("driver.manifest.json"), MANIFEST).unwrap();
    dir
}

fn run(args: &[&str]) -> Output {
    Command::new(binary()).args(args).output().expect("run driver-check")
}

/// stdout is pretty JSON followed by the one-line summary.
fn split_output(out: &Output) -> (serde_json::Value, String) {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let trimmed = stdout.trim_end();
    let (json, summary) = trimmed.rsplit_once('\n').expect("summary line");
    let value: serde_json::Value = serde_json::from_str(json).expect("inventory JSON");
    (value, summary.trim().to_string())
}

#[test]
fn inventory_lists_builtins_and_installed_manifest_only_packages() {
    let data = tempfile::tempdir().unwrap();
    let path = package(data.path(), "ws1", "demo", "1.0.0");

    let out = run(&["inventory", "--data-dir", data.path().to_str().unwrap()]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let (json, summary) = split_output(&out);

    let builtin = json["builtin"].as_array().expect("builtin array");
    assert_eq!(builtin.len(), 3);
    assert!(builtin.iter().all(|entry| entry["source"] == "builtin"));
    assert!(
        builtin
            .iter()
            .any(|entry| entry["driver"] == "simulator" && entry["version"] == "2.0.0")
    );

    let installed = json["installed"].as_array().expect("installed array");
    assert_eq!(installed.len(), 1);
    let entry = &installed[0];
    assert_eq!(entry["workspace"], "ws1");
    assert_eq!(entry["driver"], "demo");
    assert_eq!(entry["version"], "1.0.0");
    assert_eq!(entry["path"], path.display().to_string());
    assert_eq!(entry["manifestHash"].as_str().map(str::len), Some(64));
    assert!(entry["binaryPath"].is_null());
    assert!(entry["binaryHash"].is_null());
    assert_eq!(entry["abi"], "none");
    assert!(entry["error"].is_null());

    assert!(
        summary.contains("3 builtin manifest(s), 1 installed package(s)"),
        "{summary}"
    );
    assert!(summary.ends_with("-> OK"), "{summary}");
}

#[test]
fn inventory_reports_non_driver_binaries_and_strict_turns_them_into_exit_1() {
    let data = tempfile::tempdir().unwrap();
    let dir = package(data.path(), "ws1", "legacy", "0.9.0");
    std::fs::write(dir.join("driver.so"), b"not a driver library").unwrap();

    let lenient = run(&["inventory", "--data-dir", data.path().to_str().unwrap()]);
    assert!(lenient.status.success(), "inventory is informational by default");
    let (json, summary) = split_output(&lenient);
    let entry = &json["installed"].as_array().expect("installed")[0];
    assert_ne!(entry["abi"], "v2", "{entry}");
    assert!(entry["error"].is_string(), "{entry}");
    assert!(summary.contains("ATTENTION"), "{summary}");

    let strict = run(&["inventory", "--data-dir", data.path().to_str().unwrap(), "--strict"]);
    assert_eq!(
        strict.status.code(),
        Some(1),
        "--strict must fail on v1/invalid packages"
    );
    assert!(String::from_utf8_lossy(&strict.stdout).contains("ATTENTION"));
}

#[test]
fn inventory_missing_data_dir_still_lists_builtins() {
    let data = tempfile::tempdir().unwrap();
    let missing = data.path().join("does-not-exist");
    let out = run(&["inventory", "--data-dir", missing.to_str().unwrap()]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let (json, summary) = split_output(&out);
    assert_eq!(json["builtin"].as_array().unwrap().len(), 3);
    assert!(json["installed"].as_array().unwrap().is_empty());
    assert!(summary.contains("0 installed package(s)"), "{summary}");
}

#[test]
fn inventory_requires_a_data_dir() {
    let out = run(&["inventory"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--data-dir is required"));
}

#[test]
fn top_level_help_mentions_inventory() {
    let out = run(&["--help"]);
    assert!(out.status.success());
    let help = String::from_utf8_lossy(&out.stdout);
    assert!(help.contains("inventory --data-dir"), "{help}");
}
