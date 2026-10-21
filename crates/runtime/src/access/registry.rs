//! Per-workspace registry of active driver package manifests.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use parking_lot::RwLock;
use tinyiothub_core::driver::manifest::DriverManifest;

use super::AccessError;
use super::package::{LoadedPackage, hex_sha256, read_package};

#[derive(Debug, Clone)]
pub struct InstalledPackage {
    pub workspace_id: String,
    pub manifest: DriverManifest,
    pub manifest_hash: String,
    pub binary_path: Option<std::path::PathBuf>,
}

pub struct ManifestRegistry {
    active: RwLock<HashMap<(String, String), Arc<InstalledPackage>>>,
}

impl ManifestRegistry {
    pub fn new() -> Self {
        Self {
            active: RwLock::new(HashMap::new()),
        }
    }

    /// Register a built-in manifest under the empty workspace id, making it
    /// visible as a fallback to every workspace.
    pub fn seed_builtin(&self, manifest_json: &str) -> Result<(), AccessError> {
        let manifest: DriverManifest = serde_json::from_str(manifest_json)?;
        tinyiothub_core::driver::manifest::validate_manifest(&manifest)?;
        let pkg = Arc::new(InstalledPackage {
            workspace_id: String::new(),
            manifest: manifest.clone(),
            manifest_hash: hex_sha256(manifest_json.as_bytes()),
            binary_path: None,
        });
        self.active
            .write()
            .insert((String::new(), manifest.identity.driver), pkg);
        Ok(())
    }

    pub fn install(&self, workspace_id: &str, pkg: LoadedPackage) -> Result<InstalledPackage, AccessError> {
        let installed = InstalledPackage {
            workspace_id: workspace_id.to_string(),
            manifest: pkg.manifest,
            manifest_hash: pkg.manifest_hash,
            binary_path: pkg.binary_path,
        };
        self.active.write().insert(
            (workspace_id.to_string(), installed.manifest.identity.driver.clone()),
            Arc::new(installed.clone()),
        );
        Ok(installed)
    }

    pub fn install_dir(&self, workspace_id: &str, dir: &Path) -> Result<InstalledPackage, AccessError> {
        self.install(workspace_id, read_package(dir)?)
    }

    /// Look up a workspace's package, falling back to built-ins.
    pub fn get(&self, workspace_id: &str, driver: &str) -> Option<Arc<InstalledPackage>> {
        let map = self.active.read();
        map.get(&(workspace_id.to_string(), driver.to_string()))
            .cloned()
            .or_else(|| map.get(&(String::new(), driver.to_string())).cloned())
    }

    /// List packages visible to a workspace (its own plus built-ins).
    pub fn list(&self, workspace_id: &str) -> Vec<Arc<InstalledPackage>> {
        self.active
            .read()
            .iter()
            .filter(|((ws, _), _)| ws == workspace_id || ws.is_empty())
            .map(|(_, v)| v.clone())
            .collect()
    }
}

impl Default for ManifestRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeds_and_validates_every_builtin_manifest() {
        let registry = ManifestRegistry::new();
        for (name, json) in crate::driver::describe::builtin_manifest_jsons() {
            registry
                .seed_builtin(json)
                .unwrap_or_else(|e| panic!("built-in manifest '{name}' must validate: {e}"));
            let pkg = registry
                .get("any-workspace", name)
                .unwrap_or_else(|| panic!("built-in '{name}' must be a fallback for every workspace"));
            assert_eq!(pkg.manifest.identity.driver, name);
        }
        assert_eq!(registry.list("any-workspace").len(), 3);
    }
}
