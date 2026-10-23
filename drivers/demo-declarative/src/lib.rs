//! Reference declarative driver (E4a). Build: cargo build -p demo-declarative

use std::ffi::{c_char, c_int, c_void, CStr, CString};

use tinyiothub_core::driver::dynamic::{CancelHandle, DriverDone, DriverVTableV2, DRIVER_ABI_VERSION_V2};
use tinyiothub_core::driver::manifest::DriverManifest;

#[derive(Default)]
struct DemoState {
    level: i64,
}

impl DemoState {
    fn set_level(&mut self, level: i64) -> Result<(), String> {
        if !(0..=100).contains(&level) {
            return Err(format!("level out of range: {level}"));
        }
        self.level = level;
        Ok(())
    }
}

fn run_long_task(ms: u64, is_cancelled: impl Fn() -> bool) -> Result<(), String> {
    let step = 50u64;
    let mut elapsed = 0u64;
    while elapsed < ms {
        if is_cancelled() {
            return Err("cancelled".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(step.min(ms - elapsed)));
        elapsed += step;
    }
    Ok(())
}

fn emit(done: DriverDone, user: *mut c_void, status: c_int, json: String) {
    let c = CString::new(json).unwrap_or_default();
    done(user, status, c.into_raw());
}

fn param(v: &serde_json::Value, key: &str) -> Option<serde_json::Value> {
    let p = v.get("parameters")?;
    if let Some(s) = p.as_str() {
        serde_json::from_str::<serde_json::Value>(s).ok()?.get(key).cloned()
    } else {
        p.get(key).cloned()
    }
}

extern "C" fn demo_read(
    ctx: *mut c_void,
    _config: *const c_char,
    _sources: *const c_char,
    _cancel: *mut CancelHandle,
    done: DriverDone,
    user: *mut c_void,
) -> c_int {
    let state = unsafe { &*(ctx as *const DemoState) };
    // Field name must match `tinyiothub_core::driver::ResultValue` (value_type).
    // `level2` belongs to the demand-only `slow` source and stays inside the
    // declared 0..=100 range.
    let level2 = (state.level + 1).min(100);
    emit(
        done,
        user,
        0,
        format!(
            r#"[{{"name":"level","value_type":"int","value":"{}"}},{{"name":"level2","value_type":"int","value":"{}"}}]"#,
            state.level, level2
        ),
    );
    0
}

extern "C" fn demo_execute(
    ctx: *mut c_void,
    _config: *const c_char,
    cmd_json: *const c_char,
    cancel: *mut CancelHandle,
    done: DriverDone,
    user: *mut c_void,
) -> c_int {
    let state = unsafe { &mut *(ctx as *mut DemoState) };
    let raw = unsafe { CStr::from_ptr(cmd_json) }.to_string_lossy().to_string();
    let v: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            emit(done, user, 1, serde_json::json!({"error": e.to_string()}).to_string());
            return 0;
        }
    };
    match v.get("name").and_then(|x| x.as_str()).unwrap_or("") {
        "echo" => {
            // The hub parses status-0 execute results as `bool`.
            emit(done, user, 0, "true".to_string());
        }
        "set_level" => {
            let level = param(&v, "level").and_then(|x| x.as_i64()).unwrap_or(-1);
            match state.set_level(level) {
                Ok(()) => emit(done, user, 0, "true".to_string()),
                Err(e) => emit(done, user, 1, serde_json::json!({"error": e}).to_string()),
            }
        }
        "long_task" => {
            let ms = param(&v, "ms").and_then(|x| x.as_u64()).unwrap_or(1000);
            let done_addr = done as usize;
            let user_addr = user as usize;
            let cancel_fn: Option<(usize, usize)> = if cancel.is_null() {
                None
            } else {
                unsafe { Some(((*cancel).is_cancelled as usize, (*cancel).ctx as usize)) }
            };
            std::thread::spawn(move || {
                let done_f: DriverDone = unsafe { std::mem::transmute(done_addr) };
                let user_p = user_addr as *mut c_void;
                let is_cancelled = move || match cancel_fn {
                    Some((f, ctx)) => unsafe {
                        let f: extern "C" fn(*mut c_void) -> c_int = std::mem::transmute(f);
                        f(ctx as *mut c_void) != 0
                    },
                    None => false,
                };
                match run_long_task(ms, is_cancelled) {
                    Ok(()) => emit(done_f, user_p, 0, "true".to_string()),
                    Err(e) => emit(done_f, user_p, 2, serde_json::json!({"error": e}).to_string()),
                }
            });
        }
        other => emit(
            done,
            user,
            1,
            serde_json::json!({"error": format!("unknown tool {other}")}).to_string(),
        ),
    }
    0
}

extern "C" fn demo_set_demand(_ctx: *mut c_void, _demand: *const c_char) -> c_int {
    0
}

extern "C" fn demo_describe(_ctx: *mut c_void, _config: *const c_char, out: *mut *mut c_char) -> c_int {
    let m: DriverManifest = serde_json::from_str(include_str!("../driver.manifest.json")).unwrap();
    let decl = tinyiothub_core::driver::declaration::DeviceDeclaration {
        manifest_version: m.manifest_version.clone(),
        driver: m.identity.driver.clone(),
        driver_version: m.identity.version.clone(),
        state_fields: m.state_fields.clone(),
        tools: m.tools.clone(),
    };
    if out.is_null() {
        return 1;
    }
    let s = CString::new(serde_json::to_string(&decl).unwrap()).unwrap();
    unsafe { *out = s.into_raw() };
    0
}

extern "C" fn demo_free_string(s: *mut c_char) {
    if !s.is_null() {
        unsafe { drop(CString::from_raw(s)) };
    }
}

static VTABLE: DriverVTableV2 = DriverVTableV2 {
    version: DRIVER_ABI_VERSION_V2,
    read_async: demo_read,
    execute_async: demo_execute,
    set_demand: demo_set_demand,
    describe: demo_describe,
    free_string: demo_free_string,
};

#[no_mangle]
pub extern "C" fn tinyiothub_driver_init() -> *mut c_void {
    Box::into_raw(Box::new(DemoState::default())) as *mut c_void
}

#[no_mangle]
pub extern "C" fn tinyiothub_driver_vtable_v2() -> *const DriverVTableV2 {
    &VTABLE
}

#[no_mangle]
pub extern "C" fn tinyiothub_driver_destroy(ctx: *mut c_void) {
    if !ctx.is_null() {
        unsafe { drop(Box::from_raw(ctx as *mut DemoState)) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    struct Captured {
        status: c_int,
        body: String,
    }

    extern "C" fn capture_done(user: *mut c_void, status: c_int, out_json: *mut c_char) {
        let slot = unsafe { &*(user as *const Mutex<Option<Captured>>) };
        let body = if out_json.is_null() {
            String::new()
        } else {
            let s = unsafe { CStr::from_ptr(out_json) }.to_string_lossy().into_owned();
            demo_free_string(out_json);
            s
        };
        *slot.lock().unwrap() = Some(Captured { status, body });
    }

    fn call_read(state: *mut c_void) -> (c_int, String) {
        let slot: Mutex<Option<Captured>> = Mutex::new(None);
        let user = &slot as *const _ as *mut c_void;
        let rc = demo_read(
            state,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null_mut(),
            capture_done,
            user,
        );
        assert_eq!(rc, 0, "read_async return code");
        let c = slot.lock().unwrap().take().expect("done callback fired");
        (c.status, c.body)
    }

    fn call_execute(state: *mut c_void, cmd: &str) -> (c_int, String) {
        let slot: Mutex<Option<Captured>> = Mutex::new(None);
        let user = &slot as *const _ as *mut c_void;
        let cmd = CString::new(cmd).unwrap();
        let rc = demo_execute(
            state,
            std::ptr::null(),
            cmd.as_ptr(),
            std::ptr::null_mut(),
            capture_done,
            user,
        );
        assert_eq!(rc, 0, "execute_async return code");
        let c = slot.lock().unwrap().take().expect("done callback fired");
        (c.status, c.body)
    }

    #[test]
    fn manifest_is_valid() {
        let m: tinyiothub_core::driver::manifest::DriverManifest =
            serde_json::from_str(include_str!("../driver.manifest.json")).unwrap();
        tinyiothub_core::driver::manifest::validate_manifest(&m).unwrap();
    }

    #[test]
    fn set_level_updates_state_and_rejects_out_of_range() {
        let mut s = DemoState::default();
        assert_eq!(s.level, 0);
        s.set_level(42).unwrap();
        assert_eq!(s.level, 42);
        assert!(s.set_level(101).is_err());
    }

    #[test]
    fn read_emits_hub_parseable_result_value() {
        let ctx = tinyiothub_driver_init();
        let (status, body) = call_read(ctx);
        assert_eq!(status, 0);
        let values: Vec<tinyiothub_core::driver::ResultValue> = serde_json::from_str(&body).unwrap();
        assert_eq!(values.len(), 2);
        assert_eq!(values[0].name, "level");
        assert_eq!(values[0].value_type, "int");
        assert_eq!(values[0].value.as_deref(), Some("0"));
        assert_eq!(values[1].name, "level2");
        assert_eq!(values[1].value_type, "int");
        assert_eq!(values[1].value.as_deref(), Some("1"));
        tinyiothub_driver_destroy(ctx);
    }

    #[test]
    fn execute_success_emits_json_bool() {
        let ctx = tinyiothub_driver_init();
        // The hub serializes ThingCommand.parameters as a JSON string.
        let (status, body) = call_execute(ctx, r#"{"name":"echo","parameters":"{\"message\":\"hi\"}"}"#);
        assert_eq!(status, 0);
        assert!(serde_json::from_str::<bool>(&body).unwrap());

        let (status, body) = call_execute(ctx, r#"{"name":"set_level","parameters":"{\"level\":42}"}"#);
        assert_eq!(status, 0);
        assert!(serde_json::from_str::<bool>(&body).unwrap());

        let (status, body) = call_execute(ctx, r#"{"name":"set_level","parameters":"{\"level\":101}"}"#);
        assert_eq!(status, 1);
        assert!(serde_json::from_str::<serde_json::Value>(&body)
            .unwrap()
            .get("error")
            .is_some());
        tinyiothub_driver_destroy(ctx);
    }

    #[test]
    fn long_task_aborts_when_cancelled() {
        let flag = Arc::new(AtomicBool::new(false));
        let f = flag.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            f.store(true, Ordering::SeqCst);
        });
        let check = flag.clone();
        let start = Instant::now();
        assert!(run_long_task(1000, move || check.load(Ordering::SeqCst)).is_err());
        // Poll step is 50 ms; anything near the old loop bound indicates a regression.
        assert!(
            start.elapsed() < Duration::from_millis(250),
            "cancellation took {:?}",
            start.elapsed()
        );
    }
}
