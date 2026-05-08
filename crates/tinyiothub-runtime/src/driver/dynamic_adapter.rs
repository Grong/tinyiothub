// crates/tinyiothub-runtime/src/driver/dynamic_adapter.rs

use std::ffi::{CStr, c_char, c_void};
use std::sync::Arc;

use libloading::Library;
use tinyiothub_core::driver::dynamic::DriverVTable;
use tinyiothub_core::driver::{DeviceDriver, ResultValue};
use tinyiothub_core::error::Error;
use tinyiothub_core::models::device::Device;
use tinyiothub_core::models::device_command::DeviceCommand;

/// Information about a loaded dynamic driver entry.
pub struct DynamicEntry {
    pub name: String,
    pub version: String,
    pub path: std::path::PathBuf,
    pub library: Arc<Library>,
    pub vtable: &'static DriverVTable,
    pub ctx: *mut c_void,
    pub loaded_at: chrono::DateTime<chrono::Utc>,
}

/// Wraps an external dynamic library driver, implementing the `DeviceDriver` trait.
pub struct DynamicDeviceDriver {
    ctx: *mut c_void,
    vtable: &'static DriverVTable,
    _library: Arc<Library>,
    device: Device,
}

impl DynamicDeviceDriver {
    /// Create a new dynamic driver from a registry entry and device config.
    /// SAFETY: `entry.vtable` must be valid for the lifetime of the library.
    pub fn new(entry: &DynamicEntry, device: Device) -> Result<Self, Error> {
        let vtable_ref: &'static DriverVTable = entry.vtable;

        Ok(Self {
            ctx: entry.ctx,
            vtable: vtable_ref,
            _library: Arc::clone(&entry.library),
            device,
        })
    }

    /// Helper: call an FFI function that returns a JSON string via out-pointer.
    /// The string is freed via the driver's `free_string` function.
    unsafe fn call_json_out<F>(&self, f: F, desc: &str) -> Result<String, Error>
    where
        F: FnOnce(*mut *mut c_char) -> i32,
    {
        let mut out_ptr: *mut c_char = std::ptr::null_mut();
        let ret = f(&mut out_ptr);
        if ret != 0 {
            return Err(Error::DriverError(format!("{} failed: code {}", desc, ret)));
        }
        if out_ptr.is_null() {
            return Err(Error::DriverError(format!("{} returned null", desc)));
        }
        let s = unsafe { CStr::from_ptr(out_ptr).to_string_lossy().to_string() };
        (self.vtable.free_string)(out_ptr);
        Ok(s)
    }
}

// SAFETY: The raw pointer `ctx` is only accessed through the vtable functions,
// and the vtable itself is static. The library is kept alive via Arc.
unsafe impl Send for DynamicDeviceDriver {}
unsafe impl Sync for DynamicDeviceDriver {}

impl DeviceDriver for DynamicDeviceDriver {
    fn device(&self) -> &Device {
        &self.device
    }

    fn device_mut(&mut self) -> &mut Device {
        &mut self.device
    }

    fn read_data(&mut self) -> Result<Vec<ResultValue>, Error> {
        let config = self.device.driver_options.as_ref().map(|s| s.as_str()).unwrap_or("{}");
        let config_c =
            std::ffi::CString::new(config).map_err(|e| Error::DriverError(format!("invalid config JSON: {}", e)))?;

        let result_json = unsafe {
            self.call_json_out(
                |out| (self.vtable.read_data)(self.ctx, config_c.as_ptr(), out),
                "read_data",
            )?
        };

        serde_json::from_str(&result_json).map_err(|e| Error::DriverError(format!("read_data JSON parse error: {}", e)))
    }

    fn execute_command(&mut self, cmd: &DeviceCommand) -> Result<bool, Error> {
        let config = self.device.driver_options.as_ref().map(|s| s.as_str()).unwrap_or("{}");
        let config_c =
            std::ffi::CString::new(config).map_err(|e| Error::DriverError(format!("invalid config JSON: {}", e)))?;
        let cmd_json =
            serde_json::to_string(cmd).map_err(|e| Error::DriverError(format!("command serialize error: {}", e)))?;
        let cmd_c =
            std::ffi::CString::new(cmd_json).map_err(|e| Error::DriverError(format!("invalid command JSON: {}", e)))?;

        let result_json = unsafe {
            self.call_json_out(
                |out| (self.vtable.execute_command)(self.ctx, config_c.as_ptr(), cmd_c.as_ptr(), out),
                "execute_command",
            )?
        };

        serde_json::from_str(&result_json)
            .map_err(|e| Error::DriverError(format!("execute_command JSON parse error: {}", e)))
    }
}
