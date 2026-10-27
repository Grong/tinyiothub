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
use tinyiothub_core::media::SourceItem;
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

/// Source responses can contain base64 media. Twelve MiB accommodates the
/// 8 MiB decoded-image run budget after base64 expansion plus JSON envelope.
const MAX_READ_RESPONSE_BYTES: usize = 12 * 1024 * 1024;

#[cfg(test)]
thread_local! {
    static OWNED_SOURCE_RESPONSE_CONVERSIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SourceResponsePreflight<'a> {
    #[serde(default)]
    data_kind: tinyiothub_core::media::DataKind,
    #[serde(default, borrow)]
    media: Option<SourceMediaPreflight<'a>>,
}

#[derive(serde::Deserialize)]
struct SourceMediaPreflight<'a> {
    bytes: &'a str,
}

fn decoded_base64_len(encoded: &str) -> Option<usize> {
    let padding = encoded
        .as_bytes()
        .iter()
        .rev()
        .take_while(|byte| **byte == b'=')
        .count();
    let significant = encoded.len().checked_sub(padding)?;
    let decoded = (significant / 4).checked_mul(3)?;
    match significant % 4 {
        0 => Some(decoded),
        2 => decoded.checked_add(1),
        3 => decoded.checked_add(2),
        _ => None,
    }
}

fn own_source_response(bytes: &[u8], own: impl FnOnce(&[u8]) -> String) -> Result<String, Error> {
    let items: Vec<SourceResponsePreflight<'_>> = serde_json::from_slice(bytes)?;
    for item in &items {
        if item.data_kind == tinyiothub_core::media::DataKind::Image
            && let Some(media) = &item.media
            && decoded_base64_len(media.bytes).is_some_and(|size| size > crate::media::evidence::MAX_IMAGE_BYTES)
        {
            return Err(Error::ValidationError(
                tinyiothub_core::media::MediaError::ImageTooLarge.to_string(),
            ));
        }
    }
    Ok(own(bytes))
}

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
    tx: mpsc::Sender<Result<String, Error>>,
    free: extern "C" fn(*mut c_char),
    token: Box<CancelToken>,
    max_output_bytes: Option<usize>,
}

extern "C" fn done_cb(user_data: *mut c_void, status: i32, out_json: *mut c_char) {
    // SAFETY: user_data is a Box<DoneState> allocated in `call_with_timeout`.
    let state = unsafe { Box::from_raw(user_data as *mut DoneState) };
    // Destructure so the owned token clone is dropped *before* `tx.send` wakes
    // the receiver. This makes reclamation observable as soon as the call
    // returns (and keeps E4b lifecycle accounting deterministic).
    let DoneState {
        tx,
        free,
        token,
        max_output_bytes,
    } = *state;
    let json = if out_json.is_null() {
        None
    } else {
        // SAFETY: the ABI contract says a non-null out-pointer is a
        // NUL-terminated allocation owned by the driver until free_string.
        let bytes = unsafe { CStr::from_ptr(out_json) }.to_bytes();
        if let Some(limit) = max_output_bytes
            && bytes.len() > limit
        {
            free(out_json);
            drop(token);
            let _ = tx.send(Err(Error::DriverError(format!(
                "driver callback output exceeds {limit} bytes"
            ))));
            return;
        }
        let body = if status == 0 && max_output_bytes.is_some() {
            own_source_response(bytes, |bytes| {
                #[cfg(test)]
                OWNED_SOURCE_RESPONSE_CONVERSIONS.with(|count| count.set(count.get() + 1));
                String::from_utf8_lossy(bytes).into_owned()
            })
        } else {
            Ok(String::from_utf8_lossy(bytes).into_owned())
        };
        free(out_json);
        match body {
            Ok(body) => Some(body),
            Err(error) => {
                drop(token);
                let _ = tx.send(Err(error));
                return;
            }
        }
    };
    let result = match (status, json) {
        (0, Some(body)) => Ok(body),
        (2, _) => Err(Error::Cancelled),
        (1, Some(body)) => Err(Error::DriverError(format!(
            "driver callback status 1: {}",
            truncate_body(&body)
        ))),
        (1, None) => Err(Error::DriverError("driver callback status 1".into())),
        (code, _) => Err(Error::DriverError(format!("driver callback status {code}"))),
    };
    drop(token);
    let _ = tx.send(result);
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
        self.call_with_timeout_and_limit(invoke, cancel, timeout, None)
    }

    fn call_with_timeout_and_limit(
        &self,
        invoke: impl FnOnce(*mut CancelHandle, DriverDone, *mut c_void) -> i32,
        cancel: &CancelToken,
        timeout: Duration,
        max_output_bytes: Option<usize>,
    ) -> Result<String, Error> {
        let (tx, rx) = mpsc::channel();
        let state = Box::new(DoneState {
            tx,
            free: self.vtable.free_string,
            token: Box::new(cancel.clone()),
            max_output_bytes,
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
            Ok(result) => result,
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

    fn read_sources(&mut self, sources: &[String], cancel: &CancelToken) -> Result<Vec<SourceItem>, Error> {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let config_c = self.config_json()?;
        let sources_json = serde_json::to_string(sources)
            .map_err(|e| Error::DriverError(format!("source IDs serialize error: {e}")))?;
        let sources_c = std::ffi::CString::new(sources_json)
            .map_err(|e| Error::DriverError(format!("invalid source IDs JSON: {e}")))?;

        let result_json = self.call_with_timeout_and_limit(
            |handle, done, user_data| {
                (self.vtable.read_async)(self.ctx, config_c.as_ptr(), sources_c.as_ptr(), handle, done, user_data)
            },
            cancel,
            CALL_TIMEOUT,
            Some(MAX_READ_RESPONSE_BYTES),
        )?;

        serde_json::from_str(&result_json)
            .map_err(|e| Error::DriverError(format!("read_sources JSON parse error: {e}")))
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

    #[derive(Default)]
    struct SourceRequestLog(std::sync::Mutex<Vec<String>>);

    extern "C" fn read_source_items(
        ctx: *mut c_void,
        _config: *const c_char,
        sources: *const c_char,
        _cancel: *mut CancelHandle,
        done: DriverDone,
        user_data: *mut c_void,
    ) -> i32 {
        // SAFETY: the test keeps SourceRequestLog alive for the synchronous call,
        // and sources is a valid NUL-terminated JSON string supplied by the adapter.
        let requested = unsafe { CStr::from_ptr(sources) }
            .to_str()
            .ok()
            .and_then(|json| serde_json::from_str::<Vec<String>>(json).ok())
            .unwrap_or_default();
        // SAFETY: ctx points at the test's SourceRequestLog.
        unsafe { &*(ctx as *const SourceRequestLog) }
            .0
            .lock()
            .expect("source request log")
            .extend(requested);
        done(
            user_data,
            0,
            leak_json(
                r#"[
                    {"v":1,"source":"camera.front","dataKind":"image","seq":7,"time":"2026-10-27T10:00:00Z","data":{}},
                    {"v":1,"source":"camera.rear","dataKind":"detections","seq":8,"time":"2026-10-27T10:00:01Z","data":{"count":2}}
                ]"#,
            ),
        );
        0
    }

    extern "C" fn read_source_items_huge(
        _ctx: *mut c_void,
        _config: *const c_char,
        _sources: *const c_char,
        _cancel: *mut CancelHandle,
        done: DriverDone,
        user_data: *mut c_void,
    ) -> i32 {
        let response = "x".repeat(MAX_READ_RESPONSE_BYTES + 1);
        done(user_data, 0, leak_json(&response));
        0
    }

    extern "C" fn read_source_image_response(
        ctx: *mut c_void,
        _config: *const c_char,
        _sources: *const c_char,
        _cancel: *mut CancelHandle,
        done: DriverDone,
        user_data: *mut c_void,
    ) -> i32 {
        // SAFETY: each test keeps the response String alive for the synchronous call.
        let response = unsafe { &*(ctx as *const String) };
        done(user_data, 0, leak_json(response));
        0
    }

    static SOURCE_VTABLE: DriverVTableV2 = DriverVTableV2 {
        version: 2,
        read_async: read_source_items,
        execute_async: read_ok,
        set_demand: stub_set_demand,
        describe: stub_describe,
        free_string: stub_free_string,
    };

    static HUGE_SOURCE_VTABLE: DriverVTableV2 = DriverVTableV2 {
        version: 2,
        read_async: read_source_items_huge,
        execute_async: read_ok,
        set_demand: stub_set_demand,
        describe: stub_describe,
        free_string: stub_free_string,
    };

    static SOURCE_IMAGE_VTABLE: DriverVTableV2 = DriverVTableV2 {
        version: 2,
        read_async: read_source_image_response,
        execute_async: read_ok,
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
    fn read_sources_forwards_actual_ids_and_parses_source_items() {
        let log = SourceRequestLog::default();
        let thing = Thing {
            id: "stub-1".to_string(),
            name: "stub-1".to_string(),
            ..Thing::default()
        };
        let mut driver = DynamicThingDriver::for_test(
            thing,
            (&log as *const SourceRequestLog).cast_mut().cast(),
            &SOURCE_VTABLE,
        );
        let sources = vec!["camera.front".to_string(), "camera.rear".to_string()];

        let items = driver
            .read_sources(&sources, &CancelToken::new())
            .expect("source items should parse");

        assert_eq!(
            *log.0.lock().expect("source request log"),
            vec!["camera.front".to_string(), "camera.rear".to_string()]
        );
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].source, "camera.front");
        assert_eq!(items[0].data_kind, tinyiothub_core::media::DataKind::Image);
        assert_eq!(items[0].seq, 7);
        assert_eq!(items[1].source, "camera.rear");
        assert_eq!(items[1].data["count"], 2);
    }

    #[test]
    fn source_image_limit_is_enforced_before_owned_response_allocation() {
        use std::cell::Cell;

        let encoded = "A".repeat((crate::media::evidence::MAX_IMAGE_BYTES / 3 + 1) * 4);
        let response = format!(
            r#"[{{"v":1,"source":"camera.front","dataKind":"image","seq":7,"time":"2026-10-27T10:00:00Z","data":{{}},"media":{{"mime":"image/png","bytes":"{encoded}"}}}}]"#
        );
        assert!(response.len() <= MAX_READ_RESPONSE_BYTES);
        let owned = Cell::new(false);

        let error = own_source_response(response.as_bytes(), |bytes| {
            owned.set(true);
            String::from_utf8_lossy(bytes).into_owned()
        })
        .expect_err("an oversized image must be rejected before response ownership");

        assert!(error.to_string().contains("2 MiB"), "{error}");
        assert!(!owned.get(), "the owned response conversion must not run");
    }

    fn source_image_response(bytes: &[u8], byte_array: bool) -> String {
        use base64::Engine as _;

        let encoded = if byte_array {
            serde_json::to_string(bytes).expect("byte array")
        } else {
            format!("\"{}\"", base64::engine::general_purpose::STANDARD.encode(bytes))
        };
        format!(
            r#"[{{"v":1,"source":"camera.front","dataKind":"image","seq":7,"time":"2026-10-27T10:00:00Z","data":{{}},"media":{{"mime":"image/png","bytes":{encoded}}}}}]"#
        )
    }

    fn source_image_driver(response: &String) -> DynamicThingDriver {
        let thing = Thing {
            id: "stub-1".to_string(),
            name: "stub-1".to_string(),
            ..Thing::default()
        };
        DynamicThingDriver::for_test(
            thing,
            (response as *const String).cast_mut().cast(),
            &SOURCE_IMAGE_VTABLE,
        )
    }

    #[test]
    fn read_sources_preserves_exactly_two_mib_base64_image() {
        let mut expected = vec![0xa5; crate::media::evidence::MAX_IMAGE_BYTES];
        expected[..8].copy_from_slice(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
        let response = source_image_response(&expected, false);
        assert!(response.len() <= MAX_READ_RESPONSE_BYTES);
        OWNED_SOURCE_RESPONSE_CONVERSIONS.with(|count| count.set(0));
        let mut driver = source_image_driver(&response);

        let items = driver
            .read_sources(&["camera.front".to_string()], &CancelToken::new())
            .expect("an exactly 2 MiB base64 image must reach final SourceItem deserialization");

        let actual = items[0].media.as_ref().expect("image media");
        assert_eq!(actual.bytes, expected);
        OWNED_SOURCE_RESPONSE_CONVERSIONS.with(|count| assert_eq!(count.get(), 1));
    }

    #[test]
    fn read_sources_rejects_two_mib_plus_one_before_owned_allocation() {
        let response = source_image_response(&vec![0xa5; crate::media::evidence::MAX_IMAGE_BYTES + 1], false);
        assert!(response.len() <= MAX_READ_RESPONSE_BYTES);
        OWNED_SOURCE_RESPONSE_CONVERSIONS.with(|count| count.set(0));
        let mut driver = source_image_driver(&response);

        let error = driver
            .read_sources(&["camera.front".to_string()], &CancelToken::new())
            .expect_err("a 2 MiB+1 image must be rejected");

        assert!(error.to_string().contains("2 MiB"), "{error}");
        OWNED_SOURCE_RESPONSE_CONVERSIONS.with(|count| {
            assert_eq!(
                count.get(),
                0,
                "response ownership and final image allocation must not run"
            );
        });
    }

    #[test]
    fn read_sources_rejects_noncanonical_byte_array_before_owned_allocation() {
        let response = source_image_response(&[0x89, b'P', b'N', b'G'], true);
        OWNED_SOURCE_RESPONSE_CONVERSIONS.with(|count| count.set(0));
        let mut driver = source_image_driver(&response);

        driver
            .read_sources(&["camera.front".to_string()], &CancelToken::new())
            .expect_err("Source image bytes have one canonical base64 representation");

        OWNED_SOURCE_RESPONSE_CONVERSIONS.with(|count| assert_eq!(count.get(), 0));
    }

    #[test]
    fn read_sources_rejects_oversized_callback_before_string_conversion() {
        let mut driver = test_driver(&HUGE_SOURCE_VTABLE);

        let error = driver
            .read_sources(&["camera.front".to_string()], &CancelToken::new())
            .expect_err("oversized source response must be rejected");

        assert!(error.to_string().contains("exceeds"), "{error}");
        assert!(
            error.to_string().contains(&MAX_READ_RESPONSE_BYTES.to_string()),
            "{error}"
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
