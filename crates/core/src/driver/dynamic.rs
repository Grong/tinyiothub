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
