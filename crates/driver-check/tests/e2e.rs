//! End-to-end acceptance against the reference declarative driver.

use std::path::{Path, PathBuf};

use tinyiothub_runtime::acceptance::report::Level;
use tinyiothub_runtime::acceptance::runner::run_profiles;

fn demo_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../drivers/demo-declarative")
}

#[test]
fn demo_declarative_passes_core_materialization_migration() {
    let report = run_profiles(&demo_dir(), &["all"]);
    assert!(report.passed(), "{}", serde_json::to_string_pretty(&report).unwrap());
    assert_eq!(report.driver, "demo-declarative");
    assert_eq!(report.version, "1.0.0");
}

#[test]
fn demo_declarative_binary_passes_core_when_built() {
    let Some(lib) = demo_cdylib() else {
        eprintln!("skipping binary acceptance: demo-declarative cdylib not built");
        return;
    };
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
