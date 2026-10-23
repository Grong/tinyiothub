// crates/tinyiothub-runtime/src/driver/validator.rs

use std::path::Path;
use std::sync::Arc;

use tinyiothub_core::driver::declaration::DeviceDeclaration;
use tinyiothub_core::driver::dynamic::{
    DriverDestroyFn, DriverInitFn, DriverVTableV2, SYMBOL_DESTROY, SYMBOL_INIT, SYMBOL_VTABLE_V2,
    v1_unsupported_message, verify_v2_version,
};

use super::dynamic_adapter::{DESCRIBE_TIMEOUT, describe_with_timeout};

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

    /// Dry-load the binary, call `describe("{}")` (2 s / 256 KiB limits) and
    /// require the reported identity to match the manifest (spec §2.2/§3.1).
    ///
    /// A hung driver is abandoned: the describe thread keeps the library
    /// mapped and this side never frees `ctx` after a timeout, so a late
    /// return cannot use-after-free. The context allocation is intentionally
    /// leaked in that one path (E4a staging; E4b workers make it explicit).
    pub fn validate_identity(
        driver_path: &Path,
        expected_driver: &str,
        expected_version: &str,
    ) -> Result<(), DriverValidationError> {
        let metadata = std::fs::metadata(driver_path).map_err(|e| DriverValidationError::Io(e.to_string()))?;
        if metadata.len() == 0 {
            return Err(DriverValidationError::InvalidFile("empty file".into()));
        }

        let lib = unsafe {
            libloading::Library::new(driver_path).map_err(|e| DriverValidationError::LoadFailed(e.to_string()))?
        };
        let lib = Arc::new(lib);
        let vtable_fn: libloading::Symbol<V2VTableFn> =
            v2_vtable_or_v1_error(unsafe { lib.get(SYMBOL_VTABLE_V2) }.ok(), driver_path)?;
        let vtable_ptr = unsafe { vtable_fn() };
        if vtable_ptr.is_null() {
            return Err(DriverValidationError::InvalidFile("driver vtable v2 is null".into()));
        }
        let vtable: &'static DriverVTableV2 = unsafe { &*vtable_ptr };
        verify_v2_version(vtable.version).map_err(DriverValidationError::InvalidFile)?;

        let init: libloading::Symbol<DriverInitFn> = unsafe {
            lib.get(SYMBOL_INIT).map_err(|e| {
                DriverValidationError::InvalidFile(format!("missing symbol tinyiothub_driver_init: {e}"))
            })?
        };
        let ctx = unsafe { init() };
        if ctx.is_null() {
            return Err(DriverValidationError::InvalidFile("driver init returned null".into()));
        }

        let config = std::ffi::CString::new("{}").expect("static JSON literal");
        let described = describe_with_timeout(ctx, vtable, config, Some(Arc::clone(&lib)), DESCRIBE_TIMEOUT);
        let timed_out = matches!(
            &described,
            Err(tinyiothub_core::error::Error::DriverError(message)) if message.contains("describe timed out")
        );
        if !timed_out {
            let destroy: libloading::Symbol<DriverDestroyFn> = unsafe { lib.get(SYMBOL_DESTROY) }.map_err(|e| {
                DriverValidationError::InvalidFile(format!("missing symbol tinyiothub_driver_destroy: {e}"))
            })?;
            unsafe { destroy(ctx) };
        }

        if timed_out {
            return Err(DriverValidationError::Timeout);
        }
        match described {
            Ok(json) => {
                let decl: DeviceDeclaration = serde_json::from_str(&json).map_err(|e| {
                    DriverValidationError::InvalidFile(format!("describe returned invalid declaration: {e}"))
                })?;
                check_identity(expected_driver, expected_version, &decl)
                    .map_err(DriverValidationError::IdentityMismatch)
            }
            Err(e) => Err(DriverValidationError::InvalidFile(e.to_string())),
        }
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

/// Compare a binary's `describe()` identity with the manifest's.
pub(crate) fn check_identity(
    expected_driver: &str,
    expected_version: &str,
    decl: &DeviceDeclaration,
) -> Result<(), String> {
    if decl.driver != expected_driver || decl.driver_version != expected_version {
        return Err(format!(
            "binary describes {}@{}, manifest declares {}@{} (manifest/binary mismatch)",
            decl.driver, decl.driver_version, expected_driver, expected_version
        ));
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub enum DriverValidationError {
    Io(String),
    InvalidFile(String),
    LoadFailed(String),
    IdentityMismatch(String),
    Timeout,
    Crash,
}

impl std::fmt::Display for DriverValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DriverValidationError::Io(s) => write!(f, "IO error: {}", s),
            DriverValidationError::InvalidFile(s) => write!(f, "invalid file: {}", s),
            DriverValidationError::LoadFailed(s) => write!(f, "load failed: {}", s),
            DriverValidationError::IdentityMismatch(s) => write!(f, "identity mismatch: {}", s),
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
    fn identity_check_accepts_matching_and_rejects_mismatched() {
        let decl = DeviceDeclaration {
            manifest_version: "1.0".into(),
            driver: "demo".into(),
            driver_version: "1.0.0".into(),
            state_fields: vec![],
            tools: vec![],
        };
        assert!(check_identity("demo", "1.0.0", &decl).is_ok());

        let message = check_identity("demo", "2.0.0", &decl).unwrap_err();
        assert!(message.contains("manifest declares demo@2.0.0"), "{message}");
        let message = check_identity("other", "1.0.0", &decl).unwrap_err();
        assert!(message.contains("binary describes demo@1.0.0"), "{message}");
    }

    #[test]
    fn missing_v2_symbol_is_invalid_file_with_migration_guidance() {
        let err = v2_vtable_or_v1_error(None, Path::new("/tmp/liblegacy.so")).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("invalid file"), "{message}");
        assert!(message.contains("ABI v1 is unsupported"), "{message}");
        assert!(message.contains("tinyiothub_driver_vtable_v2"), "{message}");
    }
}
