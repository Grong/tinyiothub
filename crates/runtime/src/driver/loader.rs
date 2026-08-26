// crates/tinyiothub-runtime/src/driver/loader.rs

use std::path::{Path, PathBuf};

use tinyiothub_core::error::Error;

use super::registry::DriverRegistry;
use super::validation::validate_driver_name;
use super::validator::DriverValidator;

/// Loads dynamic driver files from disk into the registry.
pub struct DriverLoader;

impl DriverLoader {
    /// Install a driver package into a workspace's driver directory and load it.
    pub fn install_and_load(
        registry: &DriverRegistry,
        source_path: &Path,
        workspace_id: &str,
        driver_name: &str,
        version: &str,
        data_dir: &Path,
    ) -> Result<String, Error> {
        validate_driver_name(driver_name)?;

        let dest_dir = data_dir
            .join("drivers")
            .join("workspaces")
            .join(workspace_id)
            .join(driver_name)
            .join(version);

        std::fs::create_dir_all(&dest_dir)
            .map_err(|e| Error::IOError(format!("failed to create driver dir: {}", e)))?;

        let dest_path = dest_dir.join("driver.so");

        std::fs::copy(source_path, &dest_path)
            .map_err(|e| Error::IOError(format!("failed to copy driver file: {}", e)))?;

        let test_config = "{}";
        if let Err(e) = DriverValidator::validate(&dest_path, test_config) {
            let _ = std::fs::remove_dir_all(&dest_dir);
            return Err(Error::DriverError(format!("driver validation failed: {}", e)));
        }

        registry.load(&dest_path, workspace_id)
    }

    /// Remove a driver from a workspace.
    pub fn uninstall(
        registry: &DriverRegistry,
        workspace_id: &str,
        driver_name: &str,
        data_dir: &Path,
    ) -> Result<(), Error> {
        registry.unload(driver_name, workspace_id)?;

        let driver_dir = data_dir
            .join("drivers")
            .join("workspaces")
            .join(workspace_id)
            .join(driver_name);

        if driver_dir.exists() {
            std::fs::remove_dir_all(&driver_dir)
                .map_err(|e| Error::IOError(format!("failed to remove driver directory: {}", e)))?;
        }

        Ok(())
    }

    /// Build the canonical on-disk path for a driver installation.
    pub fn driver_path(data_dir: &Path, workspace_id: &str, driver_name: &str, version: &str) -> PathBuf {
        data_dir
            .join("drivers")
            .join("workspaces")
            .join(workspace_id)
            .join(driver_name)
            .join(version)
            .join("driver.so")
    }
}
