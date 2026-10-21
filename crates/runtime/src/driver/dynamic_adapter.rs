// crates/tinyiothub-runtime/src/driver/dynamic_adapter.rs

use std::ffi::{CStr, c_char, c_void};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use libloading::Library;
use tinyiothub_core::driver::CancelToken;
use tinyiothub_core::driver::declaration::DeviceDeclaration;
use tinyiothub_core::driver::dynamic::{CancelHandle, DriverDone, DriverVTableV2};
use tinyiothub_core::driver::{ResultValue, ThingDriver};
use tinyiothub_core::error::Error;
use tinyiothub_core::models::thing::Thing;
use tinyiothub_core::models::thing_command::ThingCommand;

/// Information about a loaded dynamic driver entry.
pub struct DynamicEntry {
    pub name: String,
    pub version: String,
    pub path: std::path::PathBuf,
    pub library: Arc<Library>,
    pub vtable: &'static DriverVTableV2,
    pub ctx: *mut c_void,
    pub loaded_at: chrono::DateTime<chrono::Utc>,
}

/// Wraps an external dynamic library driver, implementing the `ThingDriver` trait.
pub struct DynamicThingDriver {
    ctx: *mut c_void,
    vtable: &'static DriverVTableV2,
    _library: Arc<Library>,
    device: Thing,
}

/// Heap state shared with the driver's completion callback.
struct DoneState {
    tx: mpsc::Sender<(i32, Option<String>)>,
    free: extern "C" fn(*mut c_char),
}

extern "C" fn done_cb(user_data: *mut c_void, status: i32, out_json: *mut c_char) {
    // SAFETY: user_data is a Box<DoneState> allocated in `call`.
    let state = unsafe { Box::from_raw(user_data as *mut DoneState) };
    let json = if out_json.is_null() {
        None
    } else {
        let s = unsafe { CStr::from_ptr(out_json).to_string_lossy().into_owned() };
        (state.free)(out_json);
        Some(s)
    };
    let _ = state.tx.send((status, json));
}

extern "C" fn cancel_is_cancelled(ctx: *mut c_void) -> i32 {
    // SAFETY: ctx points at a CancelToken that outlives the call.
    unsafe { (*(ctx as *const CancelToken)).is_cancelled() as i32 }
}

impl DynamicThingDriver {
    /// Create a new dynamic driver from a registry entry and device config.
    /// SAFETY: `entry.vtable` must be valid for the lifetime of the library.
    pub fn new(entry: &DynamicEntry, device: Thing) -> Result<Self, Error> {
        let vtable_ref: &'static DriverVTableV2 = entry.vtable;

        Ok(Self {
            ctx: entry.ctx,
            vtable: vtable_ref,
            _library: Arc::clone(&entry.library),
            device,
        })
    }

    /// Invoke an async ABI v2 call and wait (E4a staging: synchronous wait on
    /// the completion callback; E4b moves this to per-device workers).
    fn call(
        &self,
        invoke: impl FnOnce(*mut CancelHandle, DriverDone, *mut c_void) -> i32,
        cancel: &CancelToken,
    ) -> Result<String, Error> {
        let (tx, rx) = mpsc::channel();
        let mut handle = CancelHandle {
            is_cancelled: cancel_is_cancelled,
            ctx: cancel as *const _ as *mut _,
        };
        let state = Box::new(DoneState {
            tx,
            free: self.vtable.free_string,
        });
        let user_data = Box::into_raw(state) as *mut c_void;
        let rc = invoke(&mut handle, done_cb, user_data);
        if rc != 0 {
            // SAFETY: the callback was not invoked on the error path.
            unsafe { drop(Box::from_raw(user_data as *mut DoneState)) };
            return Err(Error::DriverError(format!("driver call returned {rc}")));
        }
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok((0, Some(json))) => Ok(json),
            Ok((2, _)) => Err(Error::Cancelled),
            Ok((code, _)) => Err(Error::DriverError(format!("driver callback status {code}"))),
            Err(_) => Err(Error::DriverError("driver callback timed out".into())),
        }
    }

    fn config_json(&self) -> Result<std::ffi::CString, Error> {
        let config = self.device.driver_options.as_deref().unwrap_or("{}");
        std::ffi::CString::new(config).map_err(|e| Error::DriverError(format!("invalid config JSON: {e}")))
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
unsafe impl Send for DynamicThingDriver {}
unsafe impl Sync for DynamicThingDriver {}

impl ThingDriver for DynamicThingDriver {
    fn thing(&self) -> &Thing {
        &self.device
    }

    fn thing_mut(&mut self) -> &mut Thing {
        &mut self.device
    }

    fn read_data(&mut self, cancel: &CancelToken) -> Result<Vec<ResultValue>, Error> {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let config_c = self.config_json()?;
        let sources_c = std::ffi::CString::new("[]").expect("static JSON literal");

        let result_json = self.call(
            |handle, done, user_data| {
                (self.vtable.read_async)(self.ctx, config_c.as_ptr(), sources_c.as_ptr(), handle, done, user_data)
            },
            cancel,
        )?;

        serde_json::from_str(&result_json).map_err(|e| Error::DriverError(format!("read_data JSON parse error: {e}")))
    }

    fn execute_command(&mut self, cmd: &ThingCommand, cancel: &CancelToken) -> Result<bool, Error> {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let config_c = self.config_json()?;
        let cmd_json =
            serde_json::to_string(cmd).map_err(|e| Error::DriverError(format!("command serialize error: {e}")))?;
        let cmd_c =
            std::ffi::CString::new(cmd_json).map_err(|e| Error::DriverError(format!("invalid command JSON: {e}")))?;

        let result_json = self.call(
            |handle, done, user_data| {
                (self.vtable.execute_async)(self.ctx, config_c.as_ptr(), cmd_c.as_ptr(), handle, done, user_data)
            },
            cancel,
        )?;

        serde_json::from_str(&result_json)
            .map_err(|e| Error::DriverError(format!("execute_command JSON parse error: {e}")))
    }

    fn set_demand(&mut self, sources: &[String]) {
        let demand_json = match serde_json::to_string(sources) {
            Ok(json) => json,
            Err(e) => {
                tracing::warn!("failed to serialize demand: {}", e);
                return;
            }
        };
        let demand_c = match std::ffi::CString::new(demand_json) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("invalid demand JSON: {}", e);
                return;
            }
        };
        let rc = (self.vtable.set_demand)(self.ctx, demand_c.as_ptr());
        if rc != 0 {
            tracing::warn!("driver set_demand returned {}", rc);
        }
    }

    fn describe(&mut self) -> Result<DeviceDeclaration, Error> {
        let config_c = self.config_json()?;
        let result_json = unsafe {
            self.call_json_out(
                |out| (self.vtable.describe)(self.ctx, config_c.as_ptr(), out),
                "describe",
            )?
        };
        serde_json::from_str(&result_json).map_err(|e| Error::DriverError(format!("describe JSON parse error: {e}")))
    }
}
