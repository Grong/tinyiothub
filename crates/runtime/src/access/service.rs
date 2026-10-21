//! Access service — package installation, binding and declaration lookup.
//!
//! This is the runtime-side facade over the manifest registry, the device
//! declaration cache and the persistence ports. The composition root wires
//! the concrete (db-backed) ports; runtime itself never names sqlx.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use tinyiothub_core::driver::declaration::{
    DeviceDeclaration, DeviceDeclarationRecord, DriverDeclarationRecord, fixed_declaration, validate_device_declaration,
};
use tinyiothub_core::driver::manifest::MANIFEST_FILE;
use tinyiothub_core::models::thing::Thing;

use super::AccessError;
use super::device::DeviceDeclarationRegistry;
use super::materialize::materialize_declaration;
use super::package::{hex_sha256, read_package};
use super::registry::{InstalledPackage, ManifestRegistry};
use crate::driver::validator::DriverValidator;
use crate::ports::{
    AcceptanceReportStore, DeclarationProjectionStore, DeviceDeclarationStore, DriverDeclarationStore,
    ThingDeclarationSource,
};

pub struct AccessService {
    pub data_dir: PathBuf,
    pub manifests: Arc<ManifestRegistry>,
    pub devices: Arc<DeviceDeclarationRegistry>,
    pub driver_store: Arc<dyn DriverDeclarationStore>,
    pub device_store: Arc<dyn DeviceDeclarationStore>,
    pub projection: Arc<dyn DeclarationProjectionStore>,
    pub things: Arc<dyn ThingDeclarationSource>,
    pub acceptance_store: Arc<dyn AcceptanceReportStore>,
}

/// Releases the dynamic-registry reference acquired by `create_driver` on
/// drop — including the `describe()` error path. Built-in drivers never
/// create this guard.
struct DynamicDriverRelease {
    workspace_id: String,
    driver_name: String,
}

impl Drop for DynamicDriverRelease {
    fn drop(&mut self) {
        if let Err(e) = crate::driver::driver_registry()
            .read()
            .release(&self.workspace_id, &self.driver_name)
        {
            tracing::warn!(
                workspace_id = %self.workspace_id,
                driver = %self.driver_name,
                error = %e,
                "failed to release dynamic driver reference"
            );
        }
    }
}

impl AccessService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        data_dir: PathBuf,
        manifests: Arc<ManifestRegistry>,
        driver_store: Arc<dyn DriverDeclarationStore>,
        device_store: Arc<dyn DeviceDeclarationStore>,
        projection: Arc<dyn DeclarationProjectionStore>,
        things: Arc<dyn ThingDeclarationSource>,
        acceptance_store: Arc<dyn AcceptanceReportStore>,
    ) -> Self {
        Self {
            data_dir,
            manifests,
            devices: Arc::new(DeviceDeclarationRegistry::default()),
            driver_store,
            device_store,
            projection,
            things,
            acceptance_store,
        }
    }

    /// Persist a driver package under
    /// `{data_dir}/drivers/workspaces/{ws}/{driver}/{version}/`, load its
    /// binary (ABI v2 only) and record the active declaration row.
    ///
    /// Install is transactional with respect to the working package:
    /// the incoming package is fully validated before the destination is
    /// touched, and a failed registry load or DB write restores both the
    /// on-disk package and the previously loaded binary.
    pub async fn install_dir(&self, workspace_id: &str, src: &Path) -> Result<InstalledPackage, AccessError> {
        // 1. Validate the entire incoming package before touching anything.
        let src_pkg = read_package(src)?;
        if let Some(bin) = &src_pkg.binary_path {
            DriverValidator::validate(bin, "{}").map_err(|e| AccessError::Abi(e.to_string()))?;
        }
        let id = &src_pkg.manifest.identity;
        if id.dynamic_capabilities && src_pkg.binary_path.is_none() && !crate::driver::is_driver_supported(&id.driver) {
            return Err(AccessError::Abi(format!(
                "driver '{}' declares dynamic capabilities but ships no ABI v2 binary",
                id.driver
            )));
        }

        let driver_dir = self
            .data_dir
            .join("drivers/workspaces")
            .join(workspace_id)
            .join(&id.driver);
        let dest = driver_dir.join(&id.version);

        // 2. Snapshot the working package and move the destination aside so a
        //    failure at any later step can restore it. Replacing the whole
        //    version directory also means a manifest-only reinstall cannot
        //    silently keep a stale `driver.so`.
        let old_package = self
            .manifests
            .get(workspace_id, &id.driver)
            .filter(|p| p.workspace_id == workspace_id);
        let backup = dest.with_file_name(format!(
            ".backup-{}-{}",
            id.version,
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&driver_dir)?;
        let had_backup = if dest.exists() {
            std::fs::rename(&dest, &backup)?;
            true
        } else {
            false
        };
        let old_binary = old_package.as_ref().and_then(|p| p.binary_path.clone()).map(|path| {
            if had_backup && let Ok(rel) = path.strip_prefix(&dest) {
                backup.join(rel)
            } else {
                path
            }
        });
        let restore_backup = || {
            let _ = std::fs::remove_dir_all(&dest);
            if had_backup {
                let _ = std::fs::rename(&backup, &dest);
            }
        };

        // 3. Materialize the new package in the destination directory.
        let write_result = (|| -> Result<(), AccessError> {
            std::fs::create_dir_all(&dest)?;
            std::fs::write(dest.join(MANIFEST_FILE), &src_pkg.manifest_json)?;
            if let Some(bin) = &src_pkg.binary_path {
                let ext = bin.extension().and_then(|e| e.to_str()).unwrap_or("so");
                std::fs::copy(bin, dest.join(format!("driver.{ext}")))?;
            }
            Ok(())
        })();
        if let Err(e) = write_result {
            restore_backup();
            return Err(e);
        }
        let pkg = match read_package(&dest) {
            Ok(p) => p,
            Err(e) => {
                restore_backup();
                return Err(e);
            }
        };

        // 4. Swap the loaded binary: unload the old entry, load the new one,
        //    and reload the old binary if the new load fails.
        let registry = crate::driver::driver_registry();
        let old_was_loaded = registry.read().find(workspace_id, &id.driver).is_some();
        let mut new_loaded = false;
        {
            let guard = registry.write();
            if old_was_loaded && let Err(e) = guard.unload(&id.driver, workspace_id) {
                drop(guard);
                restore_backup();
                return Err(AccessError::Abi(e.to_string()));
            }
            if let Some(bin) = &pkg.binary_path {
                match guard.load_named(bin, workspace_id, &id.driver) {
                    Ok(_) => new_loaded = true,
                    Err(e) => {
                        if old_was_loaded && let Some(old) = &old_binary {
                            let _ = guard.load_named(old, workspace_id, &id.driver);
                        }
                        drop(guard);
                        restore_backup();
                        return Err(AccessError::Abi(e.to_string()));
                    }
                }
            }
        }

        // 5. Persist only after the package is live; on DB failure undo the
        //    registry swap and restore the previous on-disk package.
        let now = Utc::now().to_rfc3339();
        let rec = DriverDeclarationRecord {
            workspace_id: workspace_id.into(),
            driver_name: pkg.manifest.identity.driver.clone(),
            driver_version: pkg.manifest.identity.version.clone(),
            manifest_version: pkg.manifest.manifest_version.clone(),
            manifest_json: pkg.manifest_json.clone(),
            manifest_hash: pkg.manifest_hash.clone(),
            binary_hash: pkg.binary_hash.clone(),
            status: "active".into(),
            installed_at: now,
        };
        if let Err(e) = self.driver_store.upsert_active(&rec).await {
            let guard = registry.write();
            if new_loaded {
                let _ = guard.unload(&id.driver, workspace_id);
            }
            if old_was_loaded && let Some(old) = &old_binary {
                let _ = guard.load_named(old, workspace_id, &id.driver);
            }
            drop(guard);
            restore_backup();
            return Err(AccessError::Store(e));
        }

        if had_backup {
            let _ = std::fs::remove_dir_all(&backup);
        }
        self.manifests.install(workspace_id, pkg)
    }

    pub fn get_manifest(&self, workspace_id: &str, driver: &str) -> Option<Arc<InstalledPackage>> {
        self.manifests.get(workspace_id, driver)
    }

    pub fn package_dir(&self, workspace_id: &str, driver: &str) -> Option<PathBuf> {
        let pkg = self.manifests.get(workspace_id, driver)?;
        let id = &pkg.manifest.identity;
        Some(
            self.data_dir
                .join("drivers/workspaces")
                .join(workspace_id)
                .join(&id.driver)
                .join(&id.version),
        )
    }

    /// Resolve and validate the device declaration for `thing`, persist it,
    /// materialize it into the capability projections and cache it.
    pub async fn bind_thing(&self, workspace_id: &str, thing: &Thing) -> Result<DeviceDeclaration, AccessError> {
        let driver_name = thing
            .driver_name
            .clone()
            .ok_or_else(|| AccessError::NotFound("<none>".into(), workspace_id.into()))?;
        let pkg = self
            .manifests
            .get(workspace_id, &driver_name)
            .ok_or_else(|| AccessError::NotFound(driver_name.clone(), workspace_id.into()))?;
        let decl = if pkg.manifest.identity.dynamic_capabilities {
            // Built-ins take precedence in `create_driver`, so only release
            // the reference when the dynamic registry actually supplied it.
            let dynamic_workspace = thing.workspace_id.as_deref().filter(|ws| {
                !crate::driver::is_driver_supported(&driver_name)
                    && crate::driver::driver_registry().read().find(ws, &driver_name).is_some()
            });
            let mut driver =
                crate::driver::create_driver(&driver_name, thing).map_err(|e| AccessError::Abi(e.to_string()))?;
            // RAII: releases on success, error and panic.
            let _release = dynamic_workspace.map(|ws| DynamicDriverRelease {
                workspace_id: ws.to_string(),
                driver_name: driver_name.clone(),
            });
            driver
                .inner_driver_mut()
                .describe()
                .map_err(|e| AccessError::Abi(e.to_string()))?
        } else {
            fixed_declaration(&pkg.manifest)
        };
        validate_device_declaration(&pkg.manifest, &decl)?;
        let json = serde_json::to_string(&decl)?;
        let rec = DeviceDeclarationRecord {
            workspace_id: workspace_id.into(),
            thing_id: thing.id.clone(),
            driver_name: driver_name.clone(),
            driver_version: decl.driver_version.clone(),
            declaration_json: json.clone(),
            declaration_hash: hex_sha256(json.as_bytes()),
            status: "active".into(),
            updated_at: Utc::now().to_rfc3339(),
        };
        self.device_store.upsert(&rec).await.map_err(AccessError::Store)?;
        materialize_declaration(self.projection.as_ref(), &thing.id, &decl)
            .await
            .map_err(AccessError::Store)?;
        self.devices.upsert(workspace_id, &thing.id, decl.clone());
        Ok(decl)
    }

    /// Load a persisted declaration, falling back from the in-process cache
    /// to the store.
    pub async fn load_declaration(
        &self,
        workspace_id: &str,
        thing_id: &str,
    ) -> Result<Option<DeviceDeclaration>, AccessError> {
        if let Some(d) = self.devices.get(workspace_id, thing_id) {
            return Ok(Some((*d).clone()));
        }
        let Some(row) = self
            .device_store
            .find(workspace_id, thing_id)
            .await
            .map_err(AccessError::Store)?
        else {
            return Ok(None);
        };
        let decl: DeviceDeclaration = serde_json::from_str(&row.declaration_json)?;
        self.devices.upsert(workspace_id, thing_id, decl.clone());
        Ok(Some(decl))
    }

    /// Restore installed packages after a restart: re-read active DB rows,
    /// load their persisted binaries and repopulate the in-memory registry.
    ///
    /// Per-package failures are logged and skipped so one broken package
    /// cannot prevent the remaining packages (or the API) from starting.
    pub async fn rehydrate(&self) -> Result<(), AccessError> {
        let rows = self.driver_store.list_all().await.map_err(AccessError::Store)?;
        for row in rows.into_iter().filter(|r| r.status == "active") {
            if let Err(e) = self.rehydrate_row(&row) {
                tracing::warn!(
                    workspace_id = %row.workspace_id,
                    driver = %row.driver_name,
                    version = %row.driver_version,
                    error = %e,
                    "failed to rehydrate installed driver package"
                );
            }
        }
        Ok(())
    }

    fn rehydrate_row(&self, row: &DriverDeclarationRecord) -> Result<(), AccessError> {
        let dir = self
            .data_dir
            .join("drivers/workspaces")
            .join(&row.workspace_id)
            .join(&row.driver_name)
            .join(&row.driver_version);
        if !dir.is_dir() {
            return Ok(());
        }
        let pkg = read_package(&dir)?;
        if let Some(bin) = &pkg.binary_path {
            let registry = crate::driver::driver_registry();
            let guard = registry.write();
            // Idempotent rehydration: an entry already loaded (e.g. built-in
            // or concurrently restored) is left untouched.
            if guard.find(&row.workspace_id, &row.driver_name).is_none() {
                guard
                    .load_named(bin, &row.workspace_id, &row.driver_name)
                    .map_err(|e| AccessError::Abi(e.to_string()))?;
            }
        }
        self.manifests.install(&row.workspace_id, pkg)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tinyiothub_core::driver::declaration::AcceptanceReportRecord;
    use tinyiothub_core::driver::manifest::{ManifestTool, StateFieldDecl};
    use tinyiothub_core::models::thing::{Thing, ThingStatus};

    const FIXED_MANIFEST: &str = r#"{
      "manifestVersion":"1.0",
      "identity":{"driver":"demo","version":"1.0.0","protocol":"demo","abiVersion":2,"dynamicCapabilities":false},
      "config":{"type":"object"},"tools":[],
      "stateFields":[{"name":"level","type":"integer"}],
      "sources":[{"id":"telemetry","kind":"poll","fields":["level"]}],
      "resources":[],"placement":{"mode":"runtime"},
      "limits":{"maxConcurrentJobs":1,"maxCallsPerMin":60}
    }"#;

    const DYNAMIC_MANIFEST: &str = r#"{
      "manifestVersion":"1.0",
      "identity":{"driver":"demo-dynamic","version":"1.0.0","protocol":"demo","abiVersion":2,"dynamicCapabilities":true},
      "config":{"type":"object"},"tools":[],"stateFields":[],
      "sources":[],"resources":[],"placement":{"mode":"runtime"},
      "limits":{"maxConcurrentJobs":1,"maxCallsPerMin":60}
    }"#;

    const DEMO_REAL_MANIFEST: &str = include_str!("../../../../drivers/demo-declarative/driver.manifest.json");

    #[derive(Default)]
    struct FakeDriverStore(Mutex<Vec<DriverDeclarationRecord>>);

    #[async_trait::async_trait]
    impl DriverDeclarationStore for FakeDriverStore {
        async fn upsert_active(&self, row: &DriverDeclarationRecord) -> Result<(), String> {
            self.0.lock().unwrap().push(row.clone());
            Ok(())
        }
        async fn list_active(&self, ws: &str) -> Result<Vec<DriverDeclarationRecord>, String> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.workspace_id == ws)
                .cloned()
                .collect())
        }
        async fn list_all(&self) -> Result<Vec<DriverDeclarationRecord>, String> {
            Ok(self.0.lock().unwrap().clone())
        }
    }

    struct FailingDriverStore;

    #[async_trait::async_trait]
    impl DriverDeclarationStore for FailingDriverStore {
        async fn upsert_active(&self, _row: &DriverDeclarationRecord) -> Result<(), String> {
            Err("db is down".into())
        }
        async fn list_active(&self, _ws: &str) -> Result<Vec<DriverDeclarationRecord>, String> {
            Ok(Vec::new())
        }
        async fn list_all(&self) -> Result<Vec<DriverDeclarationRecord>, String> {
            Ok(Vec::new())
        }
    }

    #[derive(Default)]
    struct FakeDeviceStore(Mutex<Vec<DeviceDeclarationRecord>>);

    #[async_trait::async_trait]
    impl DeviceDeclarationStore for FakeDeviceStore {
        async fn upsert(&self, row: &DeviceDeclarationRecord) -> Result<(), String> {
            self.0.lock().unwrap().push(row.clone());
            Ok(())
        }
        async fn list_by_workspace(&self, ws: &str) -> Result<Vec<DeviceDeclarationRecord>, String> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.workspace_id == ws)
                .cloned()
                .collect())
        }
        async fn find(&self, ws: &str, thing_id: &str) -> Result<Option<DeviceDeclarationRecord>, String> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .iter()
                .rev()
                .find(|r| r.workspace_id == ws && r.thing_id == thing_id)
                .cloned())
        }
    }

    #[derive(Default)]
    struct FakeProjection {
        props: Mutex<Vec<String>>,
        commands: Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl DeclarationProjectionStore for FakeProjection {
        async fn upsert_properties(&self, _thing_id: &str, fields: &[StateFieldDecl]) -> Result<(), String> {
            self.props.lock().unwrap().extend(fields.iter().map(|f| f.name.clone()));
            Ok(())
        }
        async fn upsert_commands(&self, _thing_id: &str, tools: &[ManifestTool]) -> Result<(), String> {
            self.commands
                .lock()
                .unwrap()
                .extend(tools.iter().map(|t| t.name.clone()));
            Ok(())
        }
        async fn mark_stale(&self, _thing_id: &str, _keep_p: &[String], _keep_c: &[String]) -> Result<(), String> {
            Ok(())
        }
    }

    struct FakeThings(Option<Thing>);

    #[async_trait::async_trait]
    impl ThingDeclarationSource for FakeThings {
        async fn load_thing(&self, _thing_id: &str) -> Result<Option<Thing>, String> {
            Ok(self.0.clone())
        }
    }

    struct FakeAcceptance;

    #[async_trait::async_trait]
    impl AcceptanceReportStore for FakeAcceptance {
        async fn insert(&self, _row: &AcceptanceReportRecord) -> Result<(), String> {
            Ok(())
        }
    }

    struct Fixture {
        service: AccessService,
        driver_store: Arc<FakeDriverStore>,
        device_store: Arc<FakeDeviceStore>,
        projection: Arc<FakeProjection>,
        things: Arc<FakeThings>,
        _data: tempfile::TempDir,
    }

    fn fixture(thing: Option<Thing>) -> Fixture {
        let data = tempfile::tempdir().unwrap();
        let driver_store = Arc::new(FakeDriverStore::default());
        let device_store = Arc::new(FakeDeviceStore::default());
        let projection = Arc::new(FakeProjection::default());
        let things = Arc::new(FakeThings(thing));
        let service = AccessService::new(
            data.path().to_path_buf(),
            Arc::new(ManifestRegistry::new()),
            driver_store.clone(),
            device_store.clone(),
            projection.clone(),
            things.clone(),
            Arc::new(FakeAcceptance),
        );
        Fixture {
            service,
            driver_store,
            device_store,
            projection,
            things,
            _data: data,
        }
    }

    fn package_dir(manifest: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(MANIFEST_FILE), manifest).unwrap();
        dir
    }

    fn thing(driver: &str) -> Thing {
        Thing {
            id: "t1".into(),
            name: "t1".into(),
            driver_name: Some(driver.into()),
            workspace_id: Some("ws1".into()),
            status: ThingStatus::Offline,
            ..Default::default()
        }
    }

    /// Locate the built reference cdylib. The test is skipped when the
    /// workspace has not built `demo-declarative` (e.g. a runtime-only test
    /// invocation); the pre-push gate builds the whole workspace.
    fn demo_cdylib() -> Option<PathBuf> {
        let target = std::env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"));
        let profile = if cfg!(debug_assertions) { "debug" } else { "release" };
        [
            "libdemo_declarative.dylib",
            "libdemo_declarative.so",
            "demo_declarative.dll",
            "libdemo_declarative.dll",
        ]
        .iter()
        .map(|name| target.join(profile).join(name))
        .find(|path| path.is_file())
    }

    fn unique_workspace(prefix: &str) -> String {
        format!("{prefix}-{}", Utc::now().timestamp_nanos_opt().unwrap_or_default())
    }

    #[tokio::test]
    async fn install_persists_package_and_records_active_row() {
        let fx = fixture(None);
        let src = package_dir(FIXED_MANIFEST);

        let pkg = fx.service.install_dir("ws1", src.path()).await.unwrap();

        assert_eq!(pkg.manifest.identity.driver, "demo");
        let persisted = fx
            .service
            .package_dir("ws1", "demo")
            .expect("package dir should be resolvable");
        assert!(persisted.join(MANIFEST_FILE).is_file());
        assert!(fx.service.get_manifest("ws1", "demo").is_some());
        let rows = fx.driver_store.list_active("ws1").await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, "active");
        assert_eq!(rows[0].manifest_hash, pkg.manifest_hash);
    }

    #[tokio::test]
    async fn install_rejects_missing_manifest() {
        let fx = fixture(None);
        let empty = tempfile::tempdir().unwrap();
        let err = fx.service.install_dir("ws1", empty.path()).await.unwrap_err();
        assert!(matches!(err, AccessError::MissingManifest(_)), "{err:?}");
    }

    #[tokio::test]
    async fn install_rejects_invalid_manifest() {
        let fx = fixture(None);
        let src = package_dir(r#"{"manifestVersion":"1.0","identity":{}}"#);
        assert!(fx.service.install_dir("ws1", src.path()).await.is_err());
        assert!(fx.service.get_manifest("ws1", "demo").is_none());
    }

    #[tokio::test]
    async fn install_rejects_dynamic_package_without_binary() {
        let fx = fixture(None);
        let src = package_dir(DYNAMIC_MANIFEST);
        let err = fx.service.install_dir("ws1", src.path()).await.unwrap_err();
        assert!(matches!(err, AccessError::Abi(_)), "{err:?}");
        assert!(fx.service.get_manifest("ws1", "demo-dynamic").is_none());
    }

    #[tokio::test]
    async fn builtin_dynamic_manifest_without_binary_is_allowed() {
        let fx = fixture(None);
        let manifest = DYNAMIC_MANIFEST.replace("demo-dynamic", "simulator");
        let src = package_dir(&manifest);
        let pkg = fx.service.install_dir("ws1", src.path()).await.unwrap();
        assert_eq!(pkg.manifest.identity.driver, "simulator");
        assert!(pkg.binary_path.is_none());
    }

    #[tokio::test]
    async fn manifest_only_reinstall_removes_stale_binary() {
        let fx = fixture(None);
        let src = package_dir(FIXED_MANIFEST);
        fx.service.install_dir("ws1", src.path()).await.unwrap();

        let dest = fx.service.data_dir.join("drivers/workspaces/ws1/demo/1.0.0");
        std::fs::write(dest.join("driver.dylib"), b"stale binary").unwrap();

        let pkg = fx.service.install_dir("ws1", src.path()).await.unwrap();

        assert!(
            pkg.binary_path.is_none(),
            "manifest-only package must not carry a binary"
        );
        let active = fx.driver_store.list_active("ws1").await.unwrap();
        assert!(
            active.last().expect("active row").binary_hash.is_none(),
            "manifest-only package must not record a binary hash"
        );
        assert!(
            !dest.join("driver.dylib").exists(),
            "stale binary survived a manifest-only reinstall"
        );
        assert!(dest.join(MANIFEST_FILE).is_file());
    }

    #[tokio::test]
    async fn corrupt_binary_is_rejected_before_the_working_package_is_touched() {
        let fx = fixture(None);
        let src = package_dir(FIXED_MANIFEST);
        fx.service.install_dir("ws1", src.path()).await.unwrap();
        let dest = fx.service.data_dir.join("drivers/workspaces/ws1/demo/1.0.0");

        let bad = tempfile::tempdir().unwrap();
        std::fs::write(bad.path().join(MANIFEST_FILE), FIXED_MANIFEST).unwrap();
        std::fs::write(bad.path().join("driver.so"), b"not a shared library").unwrap();
        assert!(fx.service.install_dir("ws1", bad.path()).await.is_err());

        assert!(
            fx.service.get_manifest("ws1", "demo").is_some(),
            "working manifest must remain installed"
        );
        assert!(dest.join(MANIFEST_FILE).is_file());
        assert!(!dest.join("driver.so").exists(), "rejected binary must not be copied");
    }

    #[tokio::test]
    async fn db_failure_is_reported_and_package_is_not_registered() {
        let data = tempfile::tempdir().unwrap();
        let service = AccessService::new(
            data.path().to_path_buf(),
            Arc::new(ManifestRegistry::new()),
            Arc::new(FailingDriverStore),
            Arc::new(FakeDeviceStore::default()),
            Arc::new(FakeProjection::default()),
            Arc::new(FakeThings(None)),
            Arc::new(FakeAcceptance),
        );
        let src = package_dir(FIXED_MANIFEST);
        let err = service.install_dir("ws1", src.path()).await.unwrap_err();
        assert!(matches!(err, AccessError::Store(_)), "{err:?}");
        assert!(service.get_manifest("ws1", "demo").is_none());
    }

    #[tokio::test]
    async fn bind_fixed_driver_persists_and_materializes_declaration() {
        let t = thing("demo");
        let fx = fixture(Some(t.clone()));
        let src = package_dir(FIXED_MANIFEST);
        fx.service.install_dir("ws1", src.path()).await.unwrap();

        let decl = fx.service.bind_thing("ws1", &t).await.unwrap();

        assert_eq!(decl.driver, "demo");
        assert_eq!(decl.state_fields[0].name, "level");
        assert_eq!(fx.projection.props.lock().unwrap().as_slice(), ["level"]);
        let rows = fx.device_store.list_by_workspace("ws1").await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].declaration_hash.len(), 64);
        // Cached: second read must not hit the store (store returns the same row).
        let loaded = fx.service.load_declaration("ws1", "t1").await.unwrap().unwrap();
        assert_eq!(loaded, decl);
        assert_eq!(fx.things.0.as_ref().unwrap().id, "t1");
    }

    #[tokio::test]
    async fn bind_rejects_thing_without_installed_driver() {
        let t = thing("missing");
        let fx = fixture(Some(t.clone()));
        let err = fx.service.bind_thing("ws1", &t).await.unwrap_err();
        assert!(matches!(err, AccessError::NotFound(_, _)), "{err:?}");
    }

    #[tokio::test]
    async fn dynamic_bind_releases_ref_count_and_allows_reinstall() {
        let Some(lib) = demo_cdylib() else {
            eprintln!("skipping dynamic bind test: demo-declarative cdylib not built");
            return;
        };
        let ws = unique_workspace("ws-dyn");
        let fx = fixture(None);

        let src = tempfile::tempdir().unwrap();
        let manifest = DEMO_REAL_MANIFEST.replace("\"dynamicCapabilities\": false", "\"dynamicCapabilities\": true");
        std::fs::write(src.path().join(MANIFEST_FILE), &manifest).unwrap();
        let ext = lib.extension().and_then(|e| e.to_str()).unwrap_or("so");
        std::fs::copy(&lib, src.path().join(format!("driver.{ext}"))).unwrap();

        fx.service.install_dir(&ws, src.path()).await.unwrap();

        let t = Thing {
            id: "t-dyn".into(),
            name: "t-dyn".into(),
            driver_name: Some("demo-declarative".into()),
            workspace_id: Some(ws.clone()),
            status: ThingStatus::Offline,
            ..Default::default()
        };
        for _ in 0..2 {
            fx.service.bind_thing(&ws, &t).await.unwrap();
            let loaded = crate::driver::driver_registry().read().list_for_workspace(&ws);
            assert_eq!(loaded.len(), 1, "driver should stay loaded across binds");
            assert_eq!(loaded[0].3, 0, "bind leaked a dynamic driver reference");
        }

        // A leaked ref would make unload fail, so this reinstall is the
        // end-to-end assertion.
        fx.service.install_dir(&ws, src.path()).await.unwrap();
        let _ = crate::driver::driver_registry().write().unload("demo-declarative", &ws);
    }

    #[tokio::test]
    async fn rehydrate_restores_active_packages_and_skips_retired() {
        let fx = fixture(None);
        let active_dir = fx.service.data_dir.join("drivers/workspaces/ws1/demo/1.0.0");
        std::fs::create_dir_all(&active_dir).unwrap();
        std::fs::write(active_dir.join(MANIFEST_FILE), FIXED_MANIFEST).unwrap();

        let record = |version: &str, status: &str| DriverDeclarationRecord {
            workspace_id: "ws1".into(),
            driver_name: "demo".into(),
            driver_version: version.into(),
            manifest_version: "1.0".into(),
            manifest_json: FIXED_MANIFEST.into(),
            manifest_hash: "h".into(),
            binary_hash: None,
            status: status.into(),
            installed_at: "2026-10-21".into(),
        };
        fx.driver_store.0.lock().unwrap().extend([
            record("1.0.0", "active"),
            record("0.9.0", "retired"),
            // Active row whose package directory is gone: skipped, not fatal.
            DriverDeclarationRecord {
                driver_name: "missing".into(),
                ..record("1.0.0", "active")
            },
        ]);

        fx.service.rehydrate().await.unwrap();

        let pkg = fx
            .service
            .get_manifest("ws1", "demo")
            .expect("active package rehydrated");
        assert_eq!(pkg.manifest.identity.driver, "demo");
        assert!(fx.service.get_manifest("ws1", "missing").is_none());
        assert_eq!(fx.service.manifests.list("ws1").len(), 1);
    }

    #[tokio::test]
    async fn load_declaration_falls_back_to_store_then_caches() {
        let fx = fixture(None);
        let decl = DeviceDeclaration {
            manifest_version: "1.0".into(),
            driver: "demo".into(),
            driver_version: "1.0.0".into(),
            state_fields: vec![],
            tools: vec![],
        };
        fx.device_store
            .upsert(&DeviceDeclarationRecord {
                workspace_id: "ws1".into(),
                thing_id: "t1".into(),
                driver_name: "demo".into(),
                driver_version: "1.0.0".into(),
                declaration_json: serde_json::to_string(&decl).unwrap(),
                declaration_hash: "h".into(),
                status: "active".into(),
                updated_at: "2026-10-21".into(),
            })
            .await
            .unwrap();

        assert_eq!(
            fx.service.load_declaration("ws1", "t1").await.unwrap(),
            Some(decl.clone())
        );
        // Now served from the in-process cache.
        fx.device_store.0.lock().unwrap().clear();
        assert_eq!(fx.service.load_declaration("ws1", "t1").await.unwrap(), Some(decl));
        assert!(fx.service.load_declaration("ws1", "nope").await.unwrap().is_none());
    }
}
