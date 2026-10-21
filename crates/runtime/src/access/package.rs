//! Driver package loading: manifest + optional binary, with sha256 fingerprints.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tinyiothub_core::driver::manifest::{DriverManifest, MANIFEST_FILE, validate_manifest};

use super::AccessError;

#[derive(Debug, Clone)]
pub struct LoadedPackage {
    pub manifest: DriverManifest,
    pub manifest_json: String,
    pub manifest_hash: String,
    pub binary_path: Option<PathBuf>,
    pub binary_hash: Option<String>,
}

pub fn read_package(dir: &Path) -> Result<LoadedPackage, AccessError> {
    let manifest_path = dir.join(MANIFEST_FILE);
    if !manifest_path.is_file() {
        return Err(AccessError::MissingManifest(dir.to_path_buf()));
    }
    let manifest_json = std::fs::read_to_string(&manifest_path)?;
    let manifest: DriverManifest = serde_json::from_str(&manifest_json)?;
    validate_manifest(&manifest)?;
    let manifest_hash = hex_sha256(manifest_json.as_bytes());
    let binary_path = find_binary(dir);
    let binary_hash = match &binary_path {
        Some(p) => Some(hex_sha256(&std::fs::read(p)?)),
        None => None,
    };
    Ok(LoadedPackage {
        manifest,
        manifest_json,
        manifest_hash,
        binary_path,
        binary_hash,
    })
}

fn find_binary(dir: &Path) -> Option<PathBuf> {
    let candidates = [
        "driver.so",
        "driver.dylib",
        "driver.dll",
        "libdriver.so",
        "libdriver.dylib",
    ];
    for c in candidates {
        let p = dir.join(c);
        if p.is_file() {
            return Some(p);
        }
    }
    let mut libs: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| matches!(p.extension().and_then(|e| e.to_str()), Some("so" | "dylib" | "dll")))
        .collect();
    libs.sort();
    libs.into_iter().next()
}

pub fn hex_sha256(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const MANIFEST: &str = r#"{
      "manifestVersion":"1.0",
      "identity":{"driver":"demo","version":"1.0.0","protocol":"demo","abiVersion":2,"dynamicCapabilities":false},
      "config":{"type":"object"},"tools":[],
      "stateFields":[{"name":"level","type":"integer"}],
      "sources":[{"id":"telemetry","kind":"poll","fields":["level"]}],
      "resources":[],"placement":{"mode":"runtime"},
      "limits":{"maxConcurrentJobs":1,"maxCallsPerMin":60}
    }"#;

    #[test]
    fn reads_manifest_and_hashes_it() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("driver.manifest.json"), MANIFEST).unwrap();
        let pkg = read_package(dir.path()).unwrap();
        assert_eq!(pkg.manifest.identity.driver, "demo");
        assert_eq!(pkg.manifest_hash.len(), 64);
        assert!(pkg.binary_path.is_none());
    }

    #[test]
    fn rejects_missing_manifest() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(read_package(dir.path()), Err(AccessError::MissingManifest(_))));
    }

    #[test]
    fn rejects_unsupported_major() {
        let dir = tempfile::tempdir().unwrap();
        let bad = MANIFEST.replace("\"manifestVersion\":\"1.0\"", "\"manifestVersion\":\"2.0\"");
        fs::write(dir.path().join("driver.manifest.json"), bad).unwrap();
        assert!(matches!(read_package(dir.path()), Err(AccessError::Manifest(_))));
    }
}
