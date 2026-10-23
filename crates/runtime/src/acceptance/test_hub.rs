//! In-memory hub used by the acceptance profiles.
//!
//! The projection store records what was written so the materialization
//! profile can compare counts with the declaration. The rest of the hub is an
//! isolated, database-free environment for the lifecycle/telemetry/tools/
//! streaming profiles: a real driver package is loaded into the process-wide
//! [`DriverRegistry`] under a per-hub workspace id, a worker is spawned against
//! an in-memory [`ThingCacheSource`], and readings are collected into an
//! in-memory map + [`EventBus`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tinyiothub_core::driver::manifest::{DriverManifest, ManifestTool, StateFieldDecl};
use tinyiothub_core::driver::{DriverReading, PropertyChangeSource, ResultValue};
use tinyiothub_core::models::thing::{Thing, ThingStatus};
use tinyiothub_core::models::thing_command::ThingCommand;
use tinyiothub_core::models::thing_property::ThingProperty;

use crate::driver::status::ThingOverview;
use crate::driver::worker::{CommandOutcome, WorkerHandle, WorkerResult, spawn_worker};
use crate::driver::{create_driver, driver_registry};
use crate::event_bus::EventBus;
use crate::ports::{DeclarationProjectionStore, ThingCacheSource};

/// Build guidance for a package without a shipped binary and without a built
/// cdylib. The crate/package name is derived from the driver identity.
pub fn build_hint(driver: &str) -> String {
    format!("build the reference cdylib first: cargo build -p {driver}")
}

#[derive(Default)]
struct InMemoryProjection {
    properties: Mutex<Vec<String>>,
    commands: Mutex<Vec<String>>,
}

#[async_trait::async_trait]
impl DeclarationProjectionStore for InMemoryProjection {
    async fn upsert_properties(&self, _thing_id: &str, fields: &[StateFieldDecl]) -> Result<(), String> {
        *self.properties.lock().unwrap() = fields.iter().map(|f| f.name.clone()).collect();
        Ok(())
    }

    async fn upsert_commands(&self, _thing_id: &str, tools: &[ManifestTool]) -> Result<(), String> {
        *self.commands.lock().unwrap() = tools.iter().map(|t| t.name.clone()).collect();
        Ok(())
    }

    async fn mark_stale(
        &self,
        _thing_id: &str,
        _keep_properties: &[String],
        _keep_commands: &[String],
    ) -> Result<(), String> {
        Ok(())
    }
}

/// A driver package loaded for the lifetime of a [`TestHub`].
#[derive(Debug, Clone)]
pub struct LoadedTestDriver {
    pub driver: String,
    pub version: String,
    pub path: PathBuf,
    pub manifest: DriverManifest,
}

/// Optional knobs for [`TestHub::make_thing`].
#[derive(Debug, Clone, Default)]
pub struct ThingOptions {
    pub id: Option<String>,
    pub name: Option<String>,
    /// Raw JSON passed to the driver (`driver_options`). Defaults to `{}`.
    pub driver_options: Option<String>,
}

/// In-memory `ThingCacheSource` backing the hub (and any DataServer wired by a
/// later profile).
#[derive(Default)]
pub struct InMemoryThingCache {
    things: Mutex<HashMap<String, Thing>>,
}

impl ThingCacheSource for InMemoryThingCache {
    fn all(&self) -> Vec<Thing> {
        self.things.lock().unwrap().values().cloned().collect()
    }

    fn get(&self, id: &str) -> Option<Thing> {
        self.things.lock().unwrap().get(id).cloned()
    }

    fn get_by_name(&self, name: &str) -> Option<Thing> {
        self.things.lock().unwrap().values().find(|t| t.name == name).cloned()
    }

    fn insert(&self, device: Thing) {
        self.things.lock().unwrap().insert(device.id.clone(), device);
    }

    fn update(&self, device: Thing) {
        self.insert(device);
    }

    fn remove(&self, id: &str) {
        self.things.lock().unwrap().remove(id);
    }
}

/// One spawned worker plus the bookkeeping needed to stop it and release the
/// registry reference acquired by `create_driver`.
#[derive(Clone)]
struct ManagedHandle {
    thing_id: String,
    driver: String,
    sources: Vec<String>,
    handle: Arc<WorkerHandle>,
}

/// In-memory acceptance environment for a real driver package.
///
/// # Sync-only contract
///
/// The API is synchronous: [`TestHub::spawn`] creates a private multi-thread
/// runtime and [`TestHub::read_and_wait`] / [`TestHub::stop`] drive it with
/// `block_on`. Call them from a sync context (the acceptance runner is sync).
/// Calling `read_and_wait`/`stop` from inside another Tokio runtime panics.
/// `Drop` detects that situation and shuts the private runtime down in the
/// background instead of blocking; in-flight workers are then abandoned (their
/// registry entries are intentionally left loaded, since unloading while the
/// driver code may still run would be unsafe).
pub struct TestHub {
    projection: InMemoryProjection,
    cache: Arc<InMemoryThingCache>,
    event_bus: Arc<EventBus>,
    workspace_id: String,
    /// Created on first worker spawn. Keeping it lazy means projection-only
    /// hubs allocate no runtime (and can be dropped from an async test).
    runtime: std::sync::OnceLock<tokio::runtime::Runtime>,
    /// Latest reading per thing (`thing_id -> property name -> value`).
    values: Arc<Mutex<HashMap<String, HashMap<String, ResultValue>>>>,
    /// Fan-out of the raw readings, so `read_and_wait` can await the next one.
    readings: tokio::sync::broadcast::Sender<DriverReading>,
    handles: Mutex<Vec<ManagedHandle>>,
    loaded: Mutex<HashMap<String, LoadedTestDriver>>,
}

impl Default for TestHub {
    fn default() -> Self {
        Self::new()
    }
}

impl TestHub {
    /// Create an isolated hub. Every hub gets its own workspace id so parallel
    /// tests do not contend for the same `DriverRegistry` entry.
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let seq = NEXT.fetch_add(1, Ordering::Relaxed);
        let workspace_id = format!("acceptance-{}-{seq}", std::process::id());
        let (readings, _) = tokio::sync::broadcast::channel(64);
        Self {
            projection: InMemoryProjection::default(),
            cache: Arc::new(InMemoryThingCache::default()),
            event_bus: Arc::new(EventBus::new()),
            workspace_id,
            runtime: std::sync::OnceLock::new(),
            values: Arc::new(Mutex::new(HashMap::new())),
            readings,
            handles: Mutex::new(Vec::new()),
            loaded: Mutex::new(HashMap::new()),
        }
    }

    /// The hub's private runtime, created on first use.
    fn rt(&self) -> &tokio::runtime::Runtime {
        self.runtime
            .get_or_init(|| tokio::runtime::Runtime::new().expect("build acceptance runtime"))
    }

    pub fn projection(&self) -> &dyn DeclarationProjectionStore {
        &self.projection
    }

    /// (properties, commands) written by the last materialization.
    pub fn projection_counts(&self) -> (usize, usize) {
        (
            self.projection.properties.lock().unwrap().len(),
            self.projection.commands.lock().unwrap().len(),
        )
    }

    pub fn workspace_id(&self) -> &str {
        &self.workspace_id
    }

    pub fn cache(&self) -> Arc<dyn ThingCacheSource> {
        self.cache.clone()
    }

    pub fn event_bus(&self) -> Arc<EventBus> {
        self.event_bus.clone()
    }

    /// Load the package's real ABI v2 cdylib into the global registry under
    /// this hub's workspace. A stale ref-free entry is unloaded first so a
    /// repeated `load_package` is idempotent.
    ///
    /// A package without a shipped binary falls back to the built cdylib in
    /// `target/debug` (`lib<driver>.dylib`/`.so`); if that is missing the
    /// error carries [`build_hint`] rather than skipping silently.
    pub fn load_package(&self, dir: &Path) -> Result<LoadedTestDriver, String> {
        let pkg = crate::access::package::read_package(dir).map_err(|e| e.to_string())?;
        let driver = pkg.manifest.identity.driver.clone();
        let version = pkg.manifest.identity.version.clone();
        let path = pkg
            .binary_path
            .clone()
            .or_else(|| locate_built_library(dir, &driver))
            .ok_or_else(|| {
                format!(
                    "{driver}: no cdylib in {} and none built in {}/{{debug,release}}; {}",
                    dir.display(),
                    target_root().display(),
                    build_hint(&driver)
                )
            })?;

        let stale = driver_registry().read().find(&self.workspace_id, &driver).is_some();
        if stale {
            driver_registry()
                .write()
                .unload(&driver, &self.workspace_id)
                .map_err(|e| format!("failed to unload stale '{driver}': {e}"))?;
        }
        driver_registry()
            .write()
            .load_named(&path, &self.workspace_id, &driver)
            .map_err(|e| format!("failed to load '{}': {e}", path.display()))?;

        let loaded = LoadedTestDriver {
            driver: driver.clone(),
            version,
            path,
            manifest: pkg.manifest.clone(),
        };
        self.loaded.lock().unwrap().insert(driver, loaded.clone());
        Ok(loaded)
    }

    /// Package descriptor for a driver loaded by this hub.
    pub fn loaded(&self, driver: &str) -> Option<LoadedTestDriver> {
        self.loaded.lock().unwrap().get(driver).cloned()
    }

    /// Build a `Thing` whose properties mirror the manifest's state fields.
    pub fn make_thing(&self, driver: &LoadedTestDriver, options: ThingOptions) -> Thing {
        static NEXT_THING: AtomicU64 = AtomicU64::new(0);
        let seq = NEXT_THING.fetch_add(1, Ordering::Relaxed);
        let id = options.id.unwrap_or_else(|| format!("{}-thing-{seq}", driver.driver));
        let name = options.name.unwrap_or_else(|| id.clone());
        let properties: Vec<ThingProperty> = driver
            .manifest
            .state_fields
            .iter()
            .map(|field| state_field_property(&id, field))
            .collect();
        Thing {
            id,
            name,
            driver_name: Some(driver.driver.clone()),
            workspace_id: Some(self.workspace_id.clone()),
            driver_options: Some(options.driver_options.unwrap_or_else(|| "{}".into())),
            properties: Some(properties),
            status: ThingStatus::Offline,
            ..Thing::default()
        }
    }

    /// Spawn a worker for `thing` using a registry-loaded driver.
    ///
    /// Readings are consumed on the hub's runtime: the cache is updated (which
    /// also publishes property-change events on the hub's `EventBus`), the
    /// latest values are recorded per thing, and the raw reading is fanned out
    /// to [`Self::read_and_wait`].
    pub fn spawn(&self, driver: &str, thing: Thing) -> Result<Arc<WorkerHandle>, String> {
        let sources = self
            .loaded
            .lock()
            .unwrap()
            .get(driver)
            .map(LoadedTestDriver::source_ids)
            .ok_or_else(|| format!("driver '{driver}' is not loaded in this hub"))?;

        self.cache.insert(thing.clone());
        let mut wrapper = create_driver(driver, &thing).map_err(|e| format!("create_driver '{driver}': {e}"))?;
        wrapper.set_event_bus(self.event_bus.clone());

        let (sink_tx, mut sink_rx) = tokio::sync::mpsc::channel::<DriverReading>(64);
        let (result_tx, mut result_rx) = tokio::sync::mpsc::unbounded_channel::<WorkerResult>();
        // `spawn_worker` uses `tokio::spawn`; enter this hub's runtime so the
        // worker task lands there (the caller stays in a sync context).
        let _guard = self.rt().enter();
        let handle = Arc::new(spawn_worker(
            thing.clone(),
            wrapper,
            sink_tx,
            self.event_bus.clone() as Arc<dyn PropertyChangeSource>,
            result_tx,
        ));

        let cache = self.cache.clone();
        let bus = self.event_bus.clone();
        let values = self.values.clone();
        let readings = self.readings.clone();
        self.rt().spawn(async move {
            while let Some(reading) = sink_rx.recv().await {
                if let Some(mut device) = cache.get(&reading.thing_id) {
                    device.status = ThingStatus::Online;
                    device.last_heartbeat = Some(now_string());
                    let name = device.name.clone();
                    let events = crate::DataServer::apply_property_values(&mut device, &name, &reading.values);
                    cache.update(device);
                    for event in events {
                        let _ = bus.publish(event).await;
                    }
                }
                {
                    let mut map = values.lock().unwrap();
                    let entry = map.entry(reading.thing_id.clone()).or_default();
                    for value in &reading.values {
                        entry.insert(value.name.clone(), value.clone());
                    }
                }
                let _ = readings.send(reading);
            }
        });

        let cache = self.cache.clone();
        self.rt().spawn(async move {
            while let Some(result) = result_rx.recv().await {
                match result {
                    WorkerResult::Connected { thing_id, .. } => {
                        if let Some(mut device) = cache.get(&thing_id) {
                            device.status = ThingStatus::Online;
                            device.last_heartbeat = Some(now_string());
                            cache.update(device);
                        }
                    }
                    WorkerResult::Disconnected { thing_id, .. } | WorkerResult::ReadFailed { thing_id } => {
                        if let Some(mut device) = cache.get(&thing_id) {
                            device.status = ThingStatus::Offline;
                            cache.update(device);
                        }
                    }
                    _ => {}
                }
            }
        });

        self.handles.lock().unwrap().push(ManagedHandle {
            thing_id: thing.id,
            driver: driver.to_string(),
            sources,
            handle: handle.clone(),
        });
        Ok(handle)
    }

    /// Submit one read and wait (asynchronously, with a deadline) for the
    /// corresponding reading to arrive through the value sink.
    ///
    /// # Panics
    ///
    /// Panics when called from inside a Tokio runtime; use a sync context.
    pub fn read_and_wait(&self, handle: &WorkerHandle, timeout: Duration) -> Result<Vec<ResultValue>, String> {
        let (thing_id, sources) = {
            let handles = self.handles.lock().unwrap();
            let managed = handles
                .iter()
                .find(|m| std::ptr::eq(m.handle.as_ref(), handle))
                .ok_or_else(|| "worker was not spawned by this TestHub".to_string())?;
            (managed.thing_id.clone(), managed.sources.clone())
        };

        // Subscribe before submitting so the reading cannot slip past us.
        let mut rx = self.readings.subscribe();
        handle.submit_read(sources);
        let wait_id = thing_id.clone();
        let reading = self.rt().block_on(async move {
            tokio::time::timeout(timeout, async move {
                loop {
                    match rx.recv().await {
                        Ok(reading) if reading.thing_id == wait_id => return Ok(reading),
                        Ok(_) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(e) => return Err(format!("reading channel closed: {e}")),
                    }
                }
            })
            .await
            .map_err(|_| format!("timed out after {}ms waiting for a reading", timeout.as_millis()))?
        })?;

        {
            let mut map = self.values.lock().unwrap();
            let entry = map.entry(reading.thing_id.clone()).or_default();
            for value in &reading.values {
                entry.insert(value.name.clone(), value.clone());
            }
        }
        Ok(reading.values)
    }

    /// Submit one command and wait for the worker's terminal outcome through
    /// the `done` channel (the same path composite Call steps use).
    ///
    /// Returns `Err` when the handle was not spawned by this hub, the execute
    /// queue is full/closed, the worker stops before reporting, or `timeout`
    /// elapses. A rejected/failed command is reported as
    /// [`CommandOutcome::success`]` == false`, not an `Err`.
    ///
    /// # Panics
    ///
    /// Panics when called from inside a Tokio runtime; use a sync context.
    pub fn execute_and_wait(
        &self,
        handle: &WorkerHandle,
        cmd: ThingCommand,
        timeout: Duration,
    ) -> Result<CommandOutcome, String> {
        {
            let handles = self.handles.lock().unwrap();
            if !handles.iter().any(|m| std::ptr::eq(m.handle.as_ref(), handle)) {
                return Err("worker was not spawned by this TestHub".to_string());
            }
        }

        let (done_tx, done_rx) = tokio::sync::oneshot::channel();
        handle
            .submit_execute_with_lease_and_done(cmd, None, Some(done_tx))
            .map_err(|e| format!("submit execute: {e}"))?;
        self.wait_command_outcome(done_rx, timeout)
    }

    /// Wait for a worker `done` outcome with a deadline. Split out so the
    /// timeout/closed-channel paths are testable without a live driver.
    fn wait_command_outcome(
        &self,
        done_rx: tokio::sync::oneshot::Receiver<CommandOutcome>,
        timeout: Duration,
    ) -> Result<CommandOutcome, String> {
        self.rt().block_on(async move {
            tokio::time::timeout(timeout, done_rx)
                .await
                .map_err(|_| {
                    format!(
                        "timed out after {}ms waiting for the command outcome",
                        timeout.as_millis()
                    )
                })?
                .map_err(|_| "worker dropped the command outcome channel (stopped?)".to_string())
        })
    }

    pub fn status(&self, handle: &WorkerHandle) -> ThingOverview {
        handle.status()
    }

    /// Stop the worker, wait for its task to exit and release the registry
    /// reference acquired by `create_driver`. The cached thing is marked
    /// offline.
    ///
    /// # Panics
    ///
    /// Panics when called from inside a Tokio runtime; use a sync context.
    pub fn stop(&self, handle: &WorkerHandle) -> Result<(), String> {
        let managed = {
            let handles = self.handles.lock().unwrap();
            handles
                .iter()
                .find(|m| std::ptr::eq(m.handle.as_ref(), handle))
                .cloned()
                .ok_or_else(|| "worker was not spawned by this TestHub".to_string())?
        };

        handle.stop();
        let finished = self.rt().block_on(async {
            tokio::time::timeout(Duration::from_secs(5), async {
                while !handle.is_finished() {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .is_ok()
        });
        if !finished {
            return Err(format!(
                "worker for '{}' did not exit within 5s; driver reference kept",
                managed.thing_id
            ));
        }

        self.handles
            .lock()
            .unwrap()
            .retain(|m| !std::ptr::eq(m.handle.as_ref(), handle));
        // Built-in drivers are not in the dynamic registry; release fails with
        // "workspace not found" and there is nothing to release.
        let _ = driver_registry().read().release(&self.workspace_id, &managed.driver);
        if let Some(mut device) = self.cache.get(&managed.thing_id) {
            device.status = ThingStatus::Offline;
            self.cache.update(device);
        }
        Ok(())
    }

    /// Latest values per property for a thing.
    pub fn cached_values(&self, thing_id: &str) -> HashMap<String, ResultValue> {
        self.values.lock().unwrap().get(thing_id).cloned().unwrap_or_default()
    }

    pub fn cached_value(&self, thing_id: &str, name: &str) -> Option<ResultValue> {
        self.values
            .lock()
            .unwrap()
            .get(thing_id)
            .and_then(|values| values.get(name))
            .cloned()
    }

    pub fn cached_thing(&self, thing_id: &str) -> Option<Thing> {
        self.cache.get(thing_id)
    }

    /// Stop every worker for `driver` and unload its registry entry.
    pub fn unload(&self, driver: &str) -> Result<(), String> {
        let workers: Vec<Arc<WorkerHandle>> = self
            .handles
            .lock()
            .unwrap()
            .iter()
            .filter(|m| m.driver == driver)
            .map(|m| m.handle.clone())
            .collect();
        for worker in workers {
            self.stop(&worker)?;
        }

        if driver_registry().read().find(&self.workspace_id, driver).is_some() {
            driver_registry()
                .write()
                .unload(driver, &self.workspace_id)
                .map_err(|e| format!("failed to unload '{driver}': {e}"))?;
        }
        self.loaded.lock().unwrap().remove(driver);
        Ok(())
    }
}

impl Drop for TestHub {
    fn drop(&mut self) {
        let workers: Vec<Arc<WorkerHandle>> = self.handles.lock().unwrap().iter().map(|m| m.handle.clone()).collect();

        // `block_on` (stop/unload) must not run inside another Tokio runtime:
        // that panics. Signal stop and shut our private runtime down in the
        // background instead. The registry entries are deliberately left in
        // place — releasing/unloading them while a worker task may still be
        // executing driver code would be unsafe.
        if tokio::runtime::Handle::try_current().is_ok() {
            for worker in workers {
                worker.stop();
            }
            if let Some(runtime) = self.runtime.take() {
                runtime.shutdown_background();
            }
            return;
        }

        for worker in workers {
            let _ = self.stop(&worker);
        }
        let drivers: Vec<String> = self.loaded.lock().unwrap().keys().cloned().collect();
        for driver in drivers {
            let _ = self.unload(&driver);
        }
    }
}

impl LoadedTestDriver {
    /// Source ids from the manifest (used as the `submit_read` source set).
    pub fn source_ids(&self) -> Vec<String> {
        self.manifest.sources.iter().map(|s| s.id.clone()).collect()
    }
}

fn now_string() -> String {
    chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn state_field_property(thing_id: &str, field: &StateFieldDecl) -> ThingProperty {
    ThingProperty {
        id: format!("{thing_id}-{}", field.name),
        thing_id: thing_id.to_string(),
        name: field.name.clone(),
        display_name: None,
        description: None,
        data_type: Some(field.data_type.clone()),
        unit: field.unit.clone(),
        min_value: field.range.as_ref().and_then(|r| r.min),
        max_value: field.range.as_ref().and_then(|r| r.max),
        default_value: None,
        is_read_only: field.read_only as i32,
        source: field.source.clone(),
        expression: None,
        refs: None,
        created_at: None,
        updated_at: None,
        current_value: None,
        alarm_status: None,
        derived_status: None,
    }
}

fn target_root() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"))
}

/// Find a built cdylib: first next to the manifest, then in the cargo target
/// directory for the active profile (`cargo build -p <driver>` puts it there).
fn locate_built_library(dir: &Path, driver: &str) -> Option<PathBuf> {
    let stem = driver.replace('-', "_");
    let names = [
        format!("lib{stem}.dylib"),
        format!("lib{stem}.so"),
        format!("{stem}.dll"),
        format!("lib{stem}.dll"),
    ];
    if let Some(path) = names.iter().map(|name| dir.join(name)).find(|path| path.is_file()) {
        return Some(path);
    }
    let profile = if cfg!(debug_assertions) { "debug" } else { "release" };
    let target = target_root();
    names
        .iter()
        .map(|name| target.join(profile).join(name))
        .find(|path| path.is_file())
}

/// Fixture with one property and one command (Task 8 shape).
pub fn fixture_thing() -> Thing {
    Thing {
        id: "t1".into(),
        name: "t1".into(),
        status: ThingStatus::Online,
        driver_name: Some("simulator".into()),
        properties: Some(vec![ThingProperty {
            id: "p1".into(),
            thing_id: "t1".into(),
            name: "temperature".into(),
            display_name: None,
            description: None,
            data_type: Some("float".into()),
            unit: Some("C".into()),
            min_value: Some(-40.0),
            max_value: Some(125.0),
            default_value: None,
            is_read_only: 1,
            source: Some("template".into()),
            expression: None,
            refs: None,
            created_at: None,
            updated_at: None,
            current_value: None,
            alarm_status: None,
            derived_status: None,
        }]),
        commands: Some(vec![ThingCommand {
            id: "c1".into(),
            thing_id: "t1".into(),
            name: "restart".into(),
            display_name: None,
            description: None,
            parameters: Some("{\"type\":\"object\"}".into()),
            created_at: "2026-10-21".into(),
        }]),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::materialize::materialize_declaration;
    use tinyiothub_core::driver::manifest::ToolKindDecl;

    fn declaration() -> tinyiothub_core::driver::declaration::DeviceDeclaration {
        tinyiothub_core::driver::declaration::DeviceDeclaration {
            manifest_version: "1.0".into(),
            driver: "demo".into(),
            driver_version: "1.0.0".into(),
            state_fields: vec![StateFieldDecl {
                name: "level".into(),
                data_type: "integer".into(),
                unit: None,
                range: None,
                read_only: false,
                required: true,
                source: None,
            }],
            tools: vec![ManifestTool {
                name: "set_level".into(),
                kind: ToolKindDecl::Action,
                target_field: None,
                parameters: serde_json::json!({"type": "object"}),
                timeout: Default::default(),
                safety: Default::default(),
                resources: vec![],
                cancellable: true,
            }],
        }
    }

    #[tokio::test]
    async fn counts_materialized_properties_and_commands() {
        let hub = TestHub::new();
        materialize_declaration(hub.projection(), "test-thing", &declaration())
            .await
            .unwrap();
        assert_eq!(hub.projection_counts(), (1, 1));
    }

    fn demo_package_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../drivers/demo-declarative")
    }

    fn demo_cdylib() -> Option<PathBuf> {
        // Reuse the hub's lookup so the test follows the same candidate list.
        locate_built_library(&demo_package_dir(), "demo-declarative")
    }

    #[test]
    fn loads_reads_stops_and_unloads_the_demo_driver() {
        if demo_cdylib().is_none() {
            eprintln!("skipping test hub test: {}", build_hint("demo-declarative"));
            return;
        }
        let hub = TestHub::new();
        let loaded = hub.load_package(&demo_package_dir()).expect("load demo package");
        assert_eq!(loaded.driver, "demo-declarative");
        assert_eq!(loaded.version, "1.0.0");
        assert_eq!(loaded.source_ids(), vec!["telemetry".to_string()]);

        let thing = hub.make_thing(&loaded, ThingOptions::default());
        assert_eq!(thing.driver_name.as_deref(), Some("demo-declarative"));
        assert_eq!(thing.workspace_id.as_deref(), Some(hub.workspace_id()));
        assert_eq!(thing.properties.as_ref().map(Vec::len), Some(1));

        let handle = hub.spawn(&loaded.driver, thing.clone()).expect("spawn worker");
        let values = hub.read_and_wait(&handle, Duration::from_secs(5)).expect("worker read");
        let level = values.iter().find(|v| v.name == "level").expect("level reading");
        assert_eq!(level.value.as_deref(), Some("0"));
        assert!(
            hub.status(&handle).health.connection.is_online(),
            "worker must be online"
        );
        assert_eq!(
            hub.cached_value(&thing.id, "level").and_then(|v| v.value),
            Some("0".to_string())
        );
        assert_eq!(hub.cached_thing(&thing.id).map(|t| t.status), Some(ThingStatus::Online));

        hub.stop(&handle).expect("stop worker");
        assert!(handle.is_finished(), "worker task must exit");
        assert_eq!(
            hub.cached_thing(&thing.id).map(|t| t.status),
            Some(ThingStatus::Offline)
        );

        hub.unload(&loaded.driver).expect("unload driver");
        assert!(
            driver_registry()
                .read()
                .find(hub.workspace_id(), &loaded.driver)
                .is_none(),
            "registry entry must be removed"
        );
    }

    #[test]
    fn missing_cdylib_names_the_build_command() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("driver.manifest.json"),
            r#"{
              "manifestVersion":"1.0",
              "identity":{"driver":"demo","version":"1.0.0","protocol":"demo","abiVersion":2,"dynamicCapabilities":false},
              "config":{"type":"object"},"tools":[],"stateFields":[],
              "sources":[{"id":"telemetry","kind":"poll","fields":[]}],
              "resources":[],"placement":{"mode":"runtime"},
              "limits":{"maxConcurrentJobs":1,"maxCallsPerMin":60}
            }"#,
        )
        .unwrap();
        let hub = TestHub::new();
        let err = hub.load_package(dir.path()).unwrap_err();
        assert!(err.contains("cargo build -p demo"), "{err}");
    }

    #[test]
    fn execute_wait_times_out_when_the_outcome_never_arrives() {
        let hub = TestHub::new();
        // Keep the sender alive: the wait must expire, not observe closure.
        let (_done_tx, done_rx) = tokio::sync::oneshot::channel();
        let err = hub
            .wait_command_outcome(done_rx, Duration::from_millis(20))
            .unwrap_err();
        assert!(err.contains("timed out"), "{err}");
    }

    #[test]
    fn execute_wait_reports_a_closed_outcome_channel() {
        let hub = TestHub::new();
        let (done_tx, done_rx) = tokio::sync::oneshot::channel::<CommandOutcome>();
        // The worker dropping `done` (stop/exit) closes the channel.
        drop(done_tx);
        let err = hub.wait_command_outcome(done_rx, Duration::from_secs(1)).unwrap_err();
        assert!(err.contains("dropped"), "{err}");
    }

    #[tokio::test]
    async fn dropping_a_hub_with_a_worker_inside_an_async_context_does_not_panic() {
        if demo_cdylib().is_none() {
            eprintln!("skipping async drop test: {}", build_hint("demo-declarative"));
            return;
        }
        let hub = TestHub::new();
        let loaded = hub.load_package(&demo_package_dir()).expect("load demo package");
        let thing = hub.make_thing(&loaded, ThingOptions::default());
        let _handle = hub.spawn(&loaded.driver, thing).expect("spawn worker");
        drop(hub);
    }
}
