// crates/tinyiothub-runtime/src/driver/validator.rs

use std::path::Path;

/// Validates a driver by attempting a dry load (RTLD_LAZY).
pub struct DriverValidator;

impl DriverValidator {
    pub fn validate(driver_path: &Path, _test_config: &str) -> Result<(), DriverValidationError> {
        let metadata = std::fs::metadata(driver_path).map_err(|e| DriverValidationError::Io(e.to_string()))?;
        if metadata.len() == 0 {
            return Err(DriverValidationError::InvalidFile("empty file".into()));
        }

        Self::dry_load(driver_path)?;
        Ok(())
    }

    fn dry_load(driver_path: &Path) -> Result<(), DriverValidationError> {
        let _lib = unsafe {
            libloading::Library::new(driver_path).map_err(|e| DriverValidationError::LoadFailed(e.to_string()))?
        };
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum DriverValidationError {
    Io(String),
    InvalidFile(String),
    LoadFailed(String),
    Timeout,
    Crash,
}

impl std::fmt::Display for DriverValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DriverValidationError::Io(s) => write!(f, "IO error: {}", s),
            DriverValidationError::InvalidFile(s) => write!(f, "invalid file: {}", s),
            DriverValidationError::LoadFailed(s) => write!(f, "load failed: {}", s),
            DriverValidationError::Timeout => write!(f, "validation timed out"),
            DriverValidationError::Crash => write!(f, "validator crashed"),
        }
    }
}

impl std::error::Error for DriverValidationError {}
