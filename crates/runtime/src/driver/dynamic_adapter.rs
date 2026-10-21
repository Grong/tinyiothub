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
    _library: Option<Arc<Library>>,
    device: Thing,
}

/// E4a staging timeout for the completion callback. E4b moves calls to
/// per-device workers with per-operation budgets.
const CALL_TIMEOUT: Duration = Duration::from_secs(5);

/// Spec §2.3: `describe()` must return within 2 s and at most 256 KiB.
pub(crate) const DESCRIBE_TIMEOUT: Duration = Duration::from_secs(2);
pub(crate) const MAX_JSON_OUTPUT_BYTES: usize = 256 * 1024;

/// Convert a driver-allocated out-string into an owned `String`, enforcing
/// the output cap and always releasing the driver's allocation.
fn take_json_out(out_ptr: *mut c_char, free: extern "C" fn(*mut c_char), desc: &str) -> Result<String, Error> {
    if out_ptr.is_null() {
        return Err(Error::DriverError(format!("{desc} returned null")));
    }
    // SAFETY: the ABI contract says a non-null success out-pointer is a
    // NUL-terminated allocation owned by the driver until `free_string`.
    let bytes = unsafe { CStr::from_ptr(out_ptr) }.to_bytes();
    if bytes.len() > MAX_JSON_OUTPUT_BYTES {
        free(out_ptr);
        return Err(Error::DriverError(format!(
            "{desc} output exceeds {MAX_JSON_OUTPUT_BYTES} bytes"
        )));
    }
    let s = String::from_utf8_lossy(bytes).into_owned();
    free(out_ptr);
    Ok(s)
}

/// Work item for the dedicated `describe` thread. `ctx` is an opaque driver
/// pointer and `library` keeps the code mapped for the duration of the call.
struct DescribeJob {
    ctx: *mut c_void,
    vtable: &'static DriverVTableV2,
    config: std::ffi::CString,
    _library: Option<Arc<Library>>,
}

// SAFETY: `ctx` is only ever passed back to the driver's own vtable function,
// and the `Arc<Library>` keeps that code loaded while the thread runs.
unsafe impl Send for DescribeJob {}

/// Thread body for [`describe_with_timeout`]. Kept as a free function so the
/// closure moves the whole `Send` job instead of disjoint-capturing `ctx`.
fn run_describe_job(job: DescribeJob, tx: mpsc::Sender<Result<String, Error>>) {
    let DescribeJob {
        ctx,
        vtable,
        config,
        _library,
    } = job;
    let mut out_ptr: *mut c_char = std::ptr::null_mut();
    let ret = (vtable.describe)(ctx, config.as_ptr(), &mut out_ptr);
    let result = if ret != 0 {
        Err(Error::DriverError(format!("describe failed: code {ret}")))
    } else {
        take_json_out(out_ptr, vtable.free_string, "describe")
    };
    let _ = tx.send(result);
}

/// Run `describe()` on a dedicated thread with a hard deadline.
///
/// On timeout the thread is abandoned: the driver may still be executing, so
/// we neither free its context nor unmap the library from this side. The
/// worker owns a library clone, keeping the code valid until it finishes (or
/// forever, if the driver never returns). This matches the E4a staging
/// callback-timeout policy and is replaced by per-device workers in E4b.
pub(crate) fn describe_with_timeout(
    ctx: *mut c_void,
    vtable: &'static DriverVTableV2,
    config: std::ffi::CString,
    library: Option<Arc<Library>>,
    timeout: Duration,
) -> Result<String, Error> {
    let (tx, rx) = mpsc::channel();
    let job = DescribeJob {
        ctx,
        vtable,
        config,
        _library: library,
    };
    std::thread::Builder::new()
        .name("tinyiothub-driver-describe".into())
        .spawn(move || run_describe_job(job, tx))
        .map_err(|e| Error::DriverError(format!("failed to spawn describe worker: {e}")))?;

    match rx.recv_timeout(timeout) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => Err(Error::DriverError(format!(
            "describe timed out after {}ms",
            timeout.as_millis()
        ))),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err(Error::DriverError("describe worker exited without a result".into()))
        }
    }
}

/// Heap state shared with the driver's completion callback.
///
/// The callback owns this allocation: it reclaims the box (and with it the
/// token clone) when it fires. `token` is deliberately owned here so that
/// `CancelHandle::ctx` never points at a borrowed stack temporary — drivers
/// may poll it from worker threads, including after `call` has timed out.
struct DoneState {
    tx: mpsc::Sender<(i32, Option<String>)>,
    free: extern "C" fn(*mut c_char),
    token: Box<CancelToken>,
}

extern "C" fn done_cb(user_data: *mut c_void, status: i32, out_json: *mut c_char) {
    // SAFETY: user_data is a Box<DoneState> allocated in `call_with_timeout`.
    let state = unsafe { Box::from_raw(user_data as *mut DoneState) };
    // Destructure so the owned token clone is dropped *before* `tx.send` wakes
    // the receiver. This makes reclamation observable as soon as the call
    // returns (and keeps E4b lifecycle accounting deterministic).
    let DoneState { tx, free, token } = *state;
    let json = if out_json.is_null() {
        None
    } else {
        let s = unsafe { CStr::from_ptr(out_json).to_string_lossy().into_owned() };
        free(out_json);
        Some(s)
    };
    drop(token);
    let _ = tx.send((status, json));
}

extern "C" fn cancel_is_cancelled(ctx: *mut c_void) -> i32 {
    // SAFETY: ctx points into the DoneState-owned CancelToken clone, which
    // outlives every driver use of the handle (callback reclaims it).
    unsafe { (*(ctx as *const CancelToken)).is_cancelled() as i32 }
}

/// Maximum error body echoed into `DriverError` messages.
const ERROR_BODY_LIMIT: usize = 256;

fn truncate_body(body: &str) -> String {
    let mut truncated: String = body.chars().take(ERROR_BODY_LIMIT).collect();
    if body.chars().count() > ERROR_BODY_LIMIT {
        truncated.push('…');
    }
    truncated
}

impl DynamicThingDriver {
    /// Create a new dynamic driver from a registry entry and device config.
    /// SAFETY: `entry.vtable` must be valid for the lifetime of the library.
    pub fn new(entry: &DynamicEntry, device: Thing) -> Result<Self, Error> {
        let vtable_ref: &'static DriverVTableV2 = entry.vtable;

        Ok(Self {
            ctx: entry.ctx,
            vtable: vtable_ref,
            _library: Some(Arc::clone(&entry.library)),
            device,
        })
    }

    #[cfg(test)]
    fn for_test(device: Thing, ctx: *mut c_void, vtable: &'static DriverVTableV2) -> Self {
        Self {
            ctx,
            vtable,
            _library: None,
            device,
        }
    }

    /// Invoke an async ABI v2 call and wait (E4a staging: synchronous wait on
    /// the completion callback; E4b moves this to per-device workers).
    fn call(
        &self,
        invoke: impl FnOnce(*mut CancelHandle, DriverDone, *mut c_void) -> i32,
        cancel: &CancelToken,
    ) -> Result<String, Error> {
        self.call_with_timeout(invoke, cancel, CALL_TIMEOUT)
    }

    /// Same as [`Self::call`] with an explicit callback timeout.
    ///
    /// The driver receives a `CancelHandle` whose `ctx` points into the
    /// heap-allocated `DoneState` (an owned clone of `cancel`), so it stays
    /// valid for as long as the driver may use it. The callback reclaims the
    /// `DoneState`; if the timeout elapses without a callback, the allocation
    /// is intentionally leaked for E4a staging — reclaiming it could race a
    /// late callback still holding `user_data`. E4b's per-device workers make
    /// callback lifetimes explicit.
    fn call_with_timeout(
        &self,
        invoke: impl FnOnce(*mut CancelHandle, DriverDone, *mut c_void) -> i32,
        cancel: &CancelToken,
        timeout: Duration,
    ) -> Result<String, Error> {
        let (tx, rx) = mpsc::channel();
        let state = Box::new(DoneState {
            tx,
            free: self.vtable.free_string,
            token: Box::new(cancel.clone()),
        });
        // `Box::into_raw` keeps the heap allocation (and the token inside it)
        // at a stable address. The handle must never point at the borrowed
        // `cancel` reference, which may die before a worker thread sees it.
        let token_ptr = &*state.token as *const CancelToken as *mut c_void;
        let user_data = Box::into_raw(state) as *mut c_void;
        let mut handle = CancelHandle {
            is_cancelled: cancel_is_cancelled,
            ctx: token_ptr,
        };
        let rc = invoke(&mut handle, done_cb, user_data);
        if rc != 0 {
            // SAFETY: the callback was not invoked on the error path.
            unsafe { drop(Box::from_raw(user_data as *mut DoneState)) };
            return Err(Error::DriverError(format!("driver call returned {rc}")));
        }
        match rx.recv_timeout(timeout) {
            Ok((0, Some(json))) => Ok(json),
            Ok((2, _)) => Err(Error::Cancelled),
            Ok((1, Some(body))) => Err(Error::DriverError(format!(
                "driver callback status 1: {}",
                truncate_body(&body)
            ))),
            Ok((1, None)) => Err(Error::DriverError("driver callback status 1".into())),
            Ok((code, _)) => Err(Error::DriverError(format!("driver callback status {code}"))),
            Err(_) => Err(Error::DriverError("driver callback timed out".into())),
        }
    }

    fn config_json(&self) -> Result<std::ffi::CString, Error> {
        let config = self.device.driver_options.as_deref().unwrap_or("{}");
        std::ffi::CString::new(config).map_err(|e| Error::DriverError(format!("invalid config JSON: {e}")))
    }

    /// Describe with an explicit deadline (tests use a short timeout).
    fn describe_with_timeout(&mut self, timeout: Duration) -> Result<DeviceDeclaration, Error> {
        let config_c = self.config_json()?;
        let result_json = describe_with_timeout(self.ctx, self.vtable, config_c, self._library.clone(), timeout)?;
        serde_json::from_str(&result_json).map_err(|e| Error::DriverError(format!("describe JSON parse error: {e}")))
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
        self.describe_with_timeout(DESCRIBE_TIMEOUT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    fn leak_json(s: &str) -> *mut c_char {
        CString::new(s).expect("no interior NUL").into_raw()
    }

    extern "C" fn stub_free_string(s: *mut c_char) {
        if !s.is_null() {
            // SAFETY: every stub string comes from CString::into_raw.
            unsafe { drop(CString::from_raw(s)) };
        }
    }

    extern "C" fn stub_set_demand(_ctx: *mut c_void, _json: *const c_char) -> i32 {
        0
    }

    extern "C" fn stub_describe(_ctx: *mut c_void, _config: *const c_char, out: *mut *mut c_char) -> i32 {
        // SAFETY: out is a valid out-pointer supplied by the adapter.
        unsafe {
            *out = leak_json(r#"{"manifestVersion":"1","driver":"stub","driverVersion":"0.0.0"}"#);
        }
        0
    }

    extern "C" fn stub_describe_huge(_ctx: *mut c_void, _config: *const c_char, out: *mut *mut c_char) -> i32 {
        let s = "x".repeat(MAX_JSON_OUTPUT_BYTES + 1);
        // SAFETY: out is a valid out-pointer supplied by the adapter.
        unsafe {
            *out = leak_json(&s);
        }
        0
    }

    extern "C" fn stub_describe_slow(_ctx: *mut c_void, _config: *const c_char, out: *mut *mut c_char) -> i32 {
        std::thread::sleep(Duration::from_millis(200));
        stub_describe(std::ptr::null_mut(), std::ptr::null(), out)
    }

    extern "C" fn read_ok(
        _ctx: *mut c_void,
        _config: *const c_char,
        _sources: *const c_char,
        _cancel: *mut CancelHandle,
        done: DriverDone,
        user_data: *mut c_void,
    ) -> i32 {
        done(
            user_data,
            0,
            leak_json(r#"[{"name":"x","value_type":"int","value":"1"}]"#),
        );
        0
    }

    extern "C" fn read_err(
        _ctx: *mut c_void,
        _config: *const c_char,
        _sources: *const c_char,
        _cancel: *mut CancelHandle,
        done: DriverDone,
        user_data: *mut c_void,
    ) -> i32 {
        done(user_data, 1, leak_json("boom details"));
        0
    }

    /// Fires `done` from a detached thread. Uses the cancel handle to report
    /// whether the adapter-owned token observed cancellation: status 2 when it
    /// did, status 1 with a body when it did not.
    extern "C" fn read_cancel_async(
        _ctx: *mut c_void,
        _config: *const c_char,
        _sources: *const c_char,
        cancel: *mut CancelHandle,
        done: DriverDone,
        user_data: *mut c_void,
    ) -> i32 {
        let cancel = cancel as usize;
        let user_data = user_data as usize;
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            // SAFETY: DoneState (and its token) is alive until `done` below.
            let handle = cancel as *mut CancelHandle;
            let cancelled = unsafe { ((*handle).is_cancelled)((*handle).ctx) };
            if cancelled == 1 {
                done(user_data as *mut c_void, 2, std::ptr::null_mut());
            } else {
                done(
                    user_data as *mut c_void,
                    1,
                    leak_json("token not observed as cancelled"),
                );
            }
        });
        0
    }

    extern "C" fn read_never_done(
        _ctx: *mut c_void,
        _config: *const c_char,
        _sources: *const c_char,
        _cancel: *mut CancelHandle,
        _done: DriverDone,
        _user_data: *mut c_void,
    ) -> i32 {
        0
    }

    static OK_VTABLE: DriverVTableV2 = DriverVTableV2 {
        version: 2,
        read_async: read_ok,
        execute_async: read_ok,
        set_demand: stub_set_demand,
        describe: stub_describe,
        free_string: stub_free_string,
    };

    static ERR_VTABLE: DriverVTableV2 = DriverVTableV2 {
        version: 2,
        read_async: read_err,
        execute_async: read_err,
        set_demand: stub_set_demand,
        describe: stub_describe,
        free_string: stub_free_string,
    };

    static CANCEL_ASYNC_VTABLE: DriverVTableV2 = DriverVTableV2 {
        version: 2,
        read_async: read_cancel_async,
        execute_async: read_cancel_async,
        set_demand: stub_set_demand,
        describe: stub_describe,
        free_string: stub_free_string,
    };

    static HUGE_DESCRIBE_VTABLE: DriverVTableV2 = DriverVTableV2 {
        version: 2,
        read_async: read_ok,
        execute_async: read_ok,
        set_demand: stub_set_demand,
        describe: stub_describe_huge,
        free_string: stub_free_string,
    };

    static SLOW_DESCRIBE_VTABLE: DriverVTableV2 = DriverVTableV2 {
        version: 2,
        read_async: read_ok,
        execute_async: read_ok,
        set_demand: stub_set_demand,
        describe: stub_describe_slow,
        free_string: stub_free_string,
    };

    static TIMEOUT_VTABLE: DriverVTableV2 = DriverVTableV2 {
        version: 2,
        read_async: read_never_done,
        execute_async: read_never_done,
        set_demand: stub_set_demand,
        describe: stub_describe,
        free_string: stub_free_string,
    };

    fn test_driver(vtable: &'static DriverVTableV2) -> DynamicThingDriver {
        let thing = Thing {
            id: "stub-1".to_string(),
            name: "stub-1".to_string(),
            ..Thing::default()
        };
        DynamicThingDriver::for_test(thing, std::ptr::null_mut(), vtable)
    }

    #[test]
    fn sync_status_zero_returns_json_and_reclaims_done_state() {
        let mut driver = test_driver(&OK_VTABLE);
        let token = CancelToken::new();

        let values = driver.read_data(&token).expect("status 0 maps to Ok");
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].name, "x");
        assert_eq!(
            token.strong_count(),
            1,
            "callback must reclaim DoneState (and its token clone)"
        );
    }

    #[test]
    fn sync_status_one_reports_error_body_and_reclaims_done_state() {
        let mut driver = test_driver(&ERR_VTABLE);
        let token = CancelToken::new();

        let message = driver.read_data(&token).unwrap_err().to_string();
        assert!(message.contains("status 1"), "{message}");
        assert!(
            message.contains("boom details"),
            "error body must be surfaced: {message}"
        );
        assert_eq!(
            token.strong_count(),
            1,
            "callback must reclaim DoneState (and its token clone)"
        );
    }

    #[test]
    fn async_status_two_maps_to_cancelled_and_reclaims_done_state() {
        let mut driver = test_driver(&CANCEL_ASYNC_VTABLE);
        let token = CancelToken::new();
        let token_for_cancel = token.clone();
        let canceller = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            token_for_cancel.cancel();
        });

        let result = driver.read_data(&token);
        canceller.join().expect("canceller thread");
        assert!(
            matches!(result, Err(Error::Cancelled)),
            "the adapter-owned token must observe cross-thread cancellation; got {result:?}"
        );
        assert_eq!(
            token.strong_count(),
            1,
            "callback must reclaim DoneState (and its token clone)"
        );
    }

    #[test]
    fn timeout_maps_to_driver_error_and_keeps_token_alive() {
        let driver = test_driver(&TIMEOUT_VTABLE);
        let token = CancelToken::new();

        let message = driver
            .call_with_timeout(|_handle, _done, _user_data| 0, &token, Duration::from_millis(20))
            .unwrap_err()
            .to_string();
        assert!(message.contains("timed out"), "{message}");
        // E4a staging intentionally leaks DoneState on timeout so a late
        // callback never dereferences freed memory; the owned clone stays alive.
        assert_eq!(
            token.strong_count(),
            2,
            "DoneState must outlive the call when no callback arrives"
        );
    }

    #[test]
    fn describe_returns_the_instance_declaration() {
        let mut driver = test_driver(&OK_VTABLE);
        let decl = driver.describe().expect("describe should succeed");
        assert_eq!(decl.driver, "stub");
        assert_eq!(decl.driver_version, "0.0.0");
    }

    #[test]
    fn oversized_describe_output_is_rejected() {
        let mut driver = test_driver(&HUGE_DESCRIBE_VTABLE);
        let message = driver.describe().unwrap_err().to_string();
        assert!(message.contains("exceeds"), "{message}");
        assert!(message.contains("262144"), "{message}");
    }

    #[test]
    fn hung_describe_times_out_without_blocking_the_caller() {
        let mut driver = test_driver(&SLOW_DESCRIBE_VTABLE);
        let started = std::time::Instant::now();
        let message = driver
            .describe_with_timeout(Duration::from_millis(20))
            .unwrap_err()
            .to_string();
        assert!(message.contains("timed out"), "{message}");
        assert!(
            started.elapsed() < Duration::from_millis(150),
            "describe must not wait for the hung driver"
        );
    }

    #[test]
    fn long_error_bodies_are_truncated() {
        let body = "x".repeat(ERROR_BODY_LIMIT + 10);
        let truncated = truncate_body(&body);
        assert_eq!(truncated.chars().count(), ERROR_BODY_LIMIT + 1);
        assert!(truncated.ends_with('…'));
    }
}
