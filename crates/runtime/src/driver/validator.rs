// crates/tinyiothub-runtime/src/driver/validator.rs

use std::path::Path;

use tinyiothub_core::driver::dynamic::{DriverVTableV2, SYMBOL_VTABLE_V2, v1_unsupported_message, verify_v2_version};

type V2VTableFn = unsafe extern "C" fn() -> *const DriverVTableV2;

/// Resolve the ABI v2 vtable symbol, mapping absence to `InvalidFile` with
/// v1 migration guidance. Factored out for testability without a `.so`.
fn v2_vtable_or_v1_error<'lib>(
    symbol: Option<libloading::Symbol<'lib, V2VTableFn>>,
    path: &Path,
) -> Result<libloading::Symbol<'lib, V2VTableFn>, DriverValidationError> {
    symbol.ok_or_else(|| DriverValidationError::InvalidFile(v1_unsupported_message(path)))
}

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
        let lib = unsafe {
            libloading::Library::new(driver_path).map_err(|e| DriverValidationError::LoadFailed(e.to_string()))?
        };

        let vtable_fn: libloading::Symbol<V2VTableFn> =
            v2_vtable_or_v1_error(unsafe { lib.get(SYMBOL_VTABLE_V2) }.ok(), driver_path)?;

        let vtable_ptr = unsafe { vtable_fn() };
        if vtable_ptr.is_null() {
            return Err(DriverValidationError::InvalidFile("driver vtable v2 is null".into()));
        }
        let vtable: &'static DriverVTableV2 = unsafe { &*vtable_ptr };
        verify_v2_version(vtable.version).map_err(DriverValidationError::InvalidFile)?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_v2_symbol_is_invalid_file_with_migration_guidance() {
        let err = v2_vtable_or_v1_error(None, Path::new("/tmp/liblegacy.so")).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("invalid file"), "{message}");
        assert!(message.contains("ABI v1 is unsupported"), "{message}");
        assert!(message.contains("tinyiothub_driver_vtable_v2"), "{message}");
    }
}
