//! `driver-check inventory`: read-only migration inventory (E4e §3).
//!
//! Lists built-in manifests and every package installed under
//! `<data-dir>/drivers/workspaces/{workspace}/{driver}/{version}/`, with the
//! manifest/binary hashes and the ABI classification of the package binary.
//!
//! ABI classification attempts a dry load through
//! [`DriverValidator::validate`], so a binary that loads but does not export
//! `tinyiothub_driver_vtable_v2` is reported as `v1/unsupported` with the
//! migration guidance message. Anything that cannot be loaded at all is
//! `invalid`; a package without a binary is `none`; an unreadable manifest is
//! `unreadable`.
//!
//! Inventory is informational: the CLI exits 0 even when problems are found.
//! `--strict` turns a `v1/unsupported`, `invalid`, or `unreadable` entry into
//! exit code 1 (for CI migration gates).

use std::path::{Path, PathBuf};

use serde::Serialize;
use tinyiothub_runtime::access::package::{hex_sha256, read_package, read_package_raw};
use tinyiothub_runtime::driver::describe::builtin_manifest_jsons;
use tinyiothub_runtime::driver::validator::DriverValidator;

/// ABI verdict for one installed package binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Abi {
    #[serde(rename = "v2")]
    V2,
    #[serde(rename = "v1/unsupported")]
    V1Unsupported,
    #[serde(rename = "invalid")]
    Invalid,
    #[serde(rename = "none")]
    None,
    #[serde(rename = "unreadable")]
    Unreadable,
}

#[derive(Debug, Clone, Serialize)]
pub struct BuiltinEntry {
    pub driver: String,
    pub version: String,
    pub source: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstalledEntry {
    pub workspace: String,
    pub driver: String,
    pub version: String,
    pub path: String,
    #[serde(rename = "manifestHash")]
    pub manifest_hash: Option<String>,
    #[serde(rename = "binaryPath")]
    pub binary_path: Option<String>,
    #[serde(rename = "binaryHash")]
    pub binary_hash: Option<String>,
    #[serde(rename = "abi")]
    pub abi: Abi,
    pub error: Option<String>,
}

impl InstalledEntry {
    /// Entries `--strict` rejects: anything that cannot run as an ABI v2
    /// package. A manifest-only package (`none`) is not a migration problem by
    /// itself; built-ins ship their manifests in-tree.
    pub fn is_problem(&self) -> bool {
        matches!(self.abi, Abi::V1Unsupported | Abi::Invalid | Abi::Unreadable)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct InventoryReport {
    #[serde(rename = "dataDir")]
    pub data_dir: String,
    pub builtin: Vec<BuiltinEntry>,
    pub installed: Vec<InstalledEntry>,
}

impl InventoryReport {
    pub fn problems(&self) -> usize {
        self.installed.iter().filter(|entry| entry.is_problem()).count()
    }

    pub fn has_problems(&self) -> bool {
        self.problems() > 0
    }

    /// One-line human summary printed after the JSON.
    pub fn summary(&self) -> String {
        let count = |abi: Abi| self.installed.iter().filter(|e| e.abi == abi).count();
        let verdict = if self.has_problems() { "ATTENTION" } else { "OK" };
        format!(
            "inventory: {} builtin manifest(s), {} installed package(s): v2={}, v1/unsupported={}, invalid={}, none={}, unreadable={} -> {verdict}",
            self.builtin.len(),
            self.installed.len(),
            count(Abi::V2),
            count(Abi::V1Unsupported),
            count(Abi::Invalid),
            count(Abi::None),
            count(Abi::Unreadable),
        )
    }
}

/// Scan the built-in manifests and the installed package tree. A missing
/// data-dir is not an error: the inventory still lists built-ins.
pub fn collect(data_dir: &Path) -> InventoryReport {
    InventoryReport {
        data_dir: data_dir.display().to_string(),
        builtin: builtin_entries(),
        installed: installed_entries(data_dir),
    }
}

fn builtin_entries() -> Vec<BuiltinEntry> {
    builtin_manifest_jsons()
        .iter()
        .filter_map(|(name, json)| {
            let value: serde_json::Value = serde_json::from_str(json).ok()?;
            let identity = &value["identity"];
            Some(BuiltinEntry {
                driver: identity["driver"].as_str().unwrap_or(name).to_string(),
                version: identity["version"].as_str().unwrap_or("<unknown>").to_string(),
                source: "builtin",
            })
        })
        .collect()
}

fn installed_entries(data_dir: &Path) -> Vec<InstalledEntry> {
    let root = data_dir.join("drivers/workspaces");
    let mut entries = Vec::new();
    for workspace in sorted_dirs(&root) {
        let workspace_name = dir_name(&workspace);
        for driver_dir in sorted_dirs(&workspace) {
            let driver_name = dir_name(&driver_dir);
            for version_dir in sorted_dirs(&driver_dir) {
                let version_name = dir_name(&version_dir);
                entries.push(entry_for(&workspace_name, &driver_name, &version_name, &version_dir));
            }
        }
    }
    entries
}

/// Sorted, non-hidden sub-directories (`.backup-*` install leftovers are not
/// packages).
fn sorted_dirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = read
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir())
        .filter(|path| !dir_name(path).starts_with('.'))
        .collect();
    dirs.sort();
    dirs
}

fn dir_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string()
}

fn entry_for(workspace: &str, driver_dir: &str, version_dir: &str, path: &Path) -> InstalledEntry {
    match read_package(path) {
        Ok(pkg) => {
            let (abi, error) = classify(pkg.binary_path.as_deref());
            InstalledEntry {
                workspace: workspace.to_string(),
                driver: pkg.manifest.identity.driver,
                version: pkg.manifest.identity.version,
                path: path.display().to_string(),
                manifest_hash: Some(pkg.manifest_hash),
                binary_path: pkg.binary_path.as_ref().map(|p| p.display().to_string()),
                binary_hash: pkg.binary_hash,
                abi,
                error,
            }
        }
        Err(err) => {
            // The manifest may still parse (e.g. an unsupported schema
            // version); keep the authoritative identity/hash when it does.
            let raw = read_package_raw(path).ok();
            let (driver, version, manifest_hash) = match raw {
                Some((json, manifest)) => (
                    manifest.identity.driver,
                    manifest.identity.version,
                    Some(hex_sha256(json.as_bytes())),
                ),
                None => (driver_dir.to_string(), version_dir.to_string(), None),
            };
            InstalledEntry {
                workspace: workspace.to_string(),
                driver,
                version,
                path: path.display().to_string(),
                manifest_hash,
                binary_path: None,
                binary_hash: None,
                abi: Abi::Unreadable,
                error: Some(err.to_string()),
            }
        }
    }
}

fn classify(binary: Option<&Path>) -> (Abi, Option<String>) {
    let Some(binary) = binary else {
        return (Abi::None, None);
    };
    match DriverValidator::validate(binary, "{}") {
        Ok(()) => (Abi::V2, None),
        Err(err) => (classify_error(&err), Some(err.to_string())),
    }
}

/// Missing `tinyiothub_driver_vtable_v2` is the v1 migration signal; every
/// other load/validation failure is an unusable binary.
pub fn classify_error(err: &tinyiothub_runtime::driver::validator::DriverValidationError) -> Abi {
    if err.to_string().contains("ABI v1 is unsupported") {
        Abi::V1Unsupported
    } else {
        Abi::Invalid
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tinyiothub_runtime::driver::validator::DriverValidationError;

    const MANIFEST: &str = r#"{
      "manifestVersion":"1.0",
      "identity":{"driver":"demo","version":"1.0.0","protocol":"demo","abiVersion":2,"dynamicCapabilities":false},
      "config":{"type":"object"},"tools":[],"stateFields":[],
      "sources":[{"id":"telemetry","kind":"poll","fields":[]}],
      "resources":[],"placement":{"mode":"runtime"},
      "limits":{"maxConcurrentJobs":1,"maxCallsPerMin":60}
    }"#;

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

    #[test]
    fn lists_builtin_manifests_even_without_a_data_dir() {
        let dir = tempfile::tempdir().unwrap();
        let report = collect(&dir.path().join("missing"));
        let names: Vec<(&str, &str)> = report
            .builtin
            .iter()
            .map(|entry| (entry.driver.as_str(), entry.version.as_str()))
            .collect();
        assert_eq!(
            names,
            vec![("simulator", "2.0.0"), ("modbus_rtu", "1.0.0"), ("snmp", "1.0.0")]
        );
        assert!(report.builtin.iter().all(|entry| entry.source == "builtin"));
        assert!(report.installed.is_empty());
        assert!(!report.has_problems());
    }

    #[test]
    fn lists_installed_manifest_only_packages_with_hashes() {
        let data = tempfile::tempdir().unwrap();
        let path = package(data.path(), "ws1", "demo", "1.0.0");
        let report = collect(data.path());

        assert_eq!(report.builtin.len(), 3);
        assert_eq!(report.installed.len(), 1);
        let entry = &report.installed[0];
        assert_eq!(entry.workspace, "ws1");
        assert_eq!(entry.driver, "demo");
        assert_eq!(entry.version, "1.0.0");
        assert_eq!(entry.path, path.display().to_string());
        assert_eq!(entry.manifest_hash.as_ref().map(String::len), Some(64));
        assert!(entry.binary_path.is_none());
        assert!(entry.binary_hash.is_none());
        assert_eq!(entry.abi, Abi::None);
        assert!(entry.error.is_none());
        assert!(!entry.is_problem(), "manifest-only packages are informational");
    }

    #[test]
    fn flags_unreadable_manifests_and_still_uses_the_path_identity() {
        let data = tempfile::tempdir().unwrap();
        let dir = data.path().join("drivers/workspaces/ws1/broken/9.9.9");
        std::fs::create_dir_all(&dir).unwrap();
        let report = collect(data.path());

        assert_eq!(report.installed.len(), 1);
        let entry = &report.installed[0];
        assert_eq!(entry.driver, "broken");
        assert_eq!(entry.version, "9.9.9");
        assert_eq!(entry.abi, Abi::Unreadable);
        assert!(entry.manifest_hash.is_none());
        assert!(entry.error.is_some());
        assert!(report.has_problems());
    }

    #[test]
    fn ignores_install_backup_directories() {
        let data = tempfile::tempdir().unwrap();
        package(data.path(), "ws1", "demo", "1.0.0");
        let backup = data.path().join("drivers/workspaces/ws1/demo/.backup-1.0.0-1");
        std::fs::create_dir_all(&backup).unwrap();
        assert_eq!(collect(data.path()).installed.len(), 1);
    }

    #[test]
    fn non_driver_binaries_are_not_reported_as_v2() {
        let data = tempfile::tempdir().unwrap();
        let dir = package(data.path(), "ws1", "legacy", "0.9.0");
        std::fs::write(dir.join("driver.so"), b"not a driver library").unwrap();

        let report = collect(data.path());
        let entry = &report.installed[0];
        assert_ne!(entry.abi, Abi::V2, "{entry:?}");
        assert!(entry.is_problem(), "{entry:?}");
        assert!(entry.error.is_some(), "{entry:?}");
        assert!(report.summary().contains("ATTENTION"), "{}", report.summary());
    }

    #[test]
    fn missing_v2_symbol_is_classified_as_v1_unsupported() {
        let err = DriverValidationError::InvalidFile(
            "driver '/tmp/liblegacy.so' does not export tinyiothub_driver_vtable_v2; ABI v1 is unsupported by E4."
                .into(),
        );
        assert_eq!(classify_error(&err), Abi::V1Unsupported);
        assert_eq!(
            classify_error(&DriverValidationError::LoadFailed("bad file".into())),
            Abi::Invalid
        );
    }
}
