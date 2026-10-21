// crates/tinyiothub-core/src/driver/dynamic.rs

use std::ffi::{c_char, c_int, c_void};

/// Version of the driver ABI
pub const DRIVER_ABI_VERSION: u32 = 1;

/// C-compatible vtable for dynamic drivers.
/// All output strings are allocated by the driver (C side) and must be freed
/// by calling `free_string`.
#[repr(C)]
pub struct DriverVTable {
    pub version: u32,

    /// read_data: config_json -> result_json
    /// Returns 0 on success, non-zero on error.
    /// `out_json` is written with a malloc-allocated C string.
    pub read_data: extern "C" fn(ctx: *mut c_void, config_json: *const c_char, out_json: *mut *mut c_char) -> c_int,

    /// execute_command: config_json + cmd_json -> result_json
    pub execute_command: extern "C" fn(
        ctx: *mut c_void,
        config_json: *const c_char,
        cmd_json: *const c_char,
        out_json: *mut *mut c_char,
    ) -> c_int,

    /// get_schema: -> schema_json
    pub get_schema: extern "C" fn(out_json: *mut *mut c_char) -> c_int,

    /// free_string: release a string allocated by the driver
    pub free_string: extern "C" fn(s: *mut c_char),
}

/// Initialize the driver and return an opaque context pointer.
pub type DriverInitFn = unsafe extern "C" fn() -> *mut c_void;

/// Return a pointer to the static vtable.
pub type DriverVTableFn = unsafe extern "C" fn() -> *const DriverVTable;

/// Destroy the driver context.
pub type DriverDestroyFn = unsafe extern "C" fn(ctx: *mut c_void);

/// Symbol names exported by dynamic drivers.
pub const SYMBOL_INIT: &[u8] = b"tinyiothub_driver_init\0";
pub const SYMBOL_VTABLE: &[u8] = b"tinyiothub_driver_vtable\0";
pub const SYMBOL_DESTROY: &[u8] = b"tinyiothub_driver_destroy\0";

// ---------------------------------------------------------------------------
// ABI v2: asynchronous, cancellable, declarative access.
// ---------------------------------------------------------------------------

pub const DRIVER_ABI_VERSION_V2: u32 = 2;
pub const SYMBOL_VTABLE_V2: &[u8] = b"tinyiothub_driver_vtable_v2\0";

#[repr(C)]
pub struct CancelHandle {
    pub is_cancelled: extern "C" fn(ctx: *mut c_void) -> c_int,
    pub ctx: *mut c_void,
}

/// status: 0 ok, 1 error, 2 cancelled. out_json is driver-allocated;
/// the hub frees it via `free_string`.
pub type DriverDone = extern "C" fn(user_data: *mut c_void, status: c_int, out_json: *mut c_char);

#[repr(C)]
pub struct DriverVTableV2 {
    pub version: u32,
    pub read_async: extern "C" fn(
        ctx: *mut c_void,
        config_json: *const c_char,
        sources_json: *const c_char,
        cancel: *mut CancelHandle,
        done: DriverDone,
        user_data: *mut c_void,
    ) -> c_int,
    pub execute_async: extern "C" fn(
        ctx: *mut c_void,
        config_json: *const c_char,
        cmd_json: *const c_char,
        cancel: *mut CancelHandle,
        done: DriverDone,
        user_data: *mut c_void,
    ) -> c_int,
    pub set_demand: extern "C" fn(ctx: *mut c_void, demand_json: *const c_char) -> c_int,
    pub describe: extern "C" fn(ctx: *mut c_void, config_json: *const c_char, out_json: *mut *mut c_char) -> c_int,
    pub free_string: extern "C" fn(s: *mut c_char),
}

pub fn verify_v2_version(version: u32) -> Result<(), String> {
    if version == DRIVER_ABI_VERSION_V2 {
        Ok(())
    } else {
        Err(format!(
            "ABI v{version} is unsupported by E4 hub; migrate to v2 (async read/execute + cancel + describe) \
             and export symbol tinyiothub_driver_vtable_v2"
        ))
    }
}

pub fn v1_unsupported_message(path: &std::path::Path) -> String {
    format!(
        "driver '{}' does not export tinyiothub_driver_vtable_v2; ABI v1 is unsupported by E4. \
         Migrate to ABI v2 (async read/execute + cancel + describe). See \
         docs/superpowers/specs/2026-10-21-e4-access-declaration-design.md §2.3.",
        path.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v2_version_must_match() {
        assert!(verify_v2_version(2).is_ok());
        let err = verify_v2_version(1).unwrap_err();
        assert!(err.contains("ABI v1 is unsupported"));
        assert!(err.contains("async") && err.contains("cancel") && err.contains("describe"));
    }

    #[test]
    fn v1_message_names_the_artifact() {
        let msg = v1_unsupported_message(std::path::Path::new("/tmp/libfoo.so"));
        assert!(msg.contains("libfoo.so"));
        assert!(msg.contains("tinyiothub_driver_vtable_v2"));
    }
}
