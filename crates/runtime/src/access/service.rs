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
    pub async fn install_dir(&self, workspace_id: &str, src: &Path) -> Result<InstalledPackage, AccessError> {
        let src_pkg = read_package(src)?;
        let id = &src_pkg.manifest.identity;
        let dest = self
            .data_dir
            .join("drivers/workspaces")
            .join(workspace_id)
            .join(&id.driver)
            .join(&id.version);
        std::fs::create_dir_all(&dest)?;
        std::fs::write(dest.join(MANIFEST_FILE), &src_pkg.manifest_json)?;
        if let Some(bin) = &src_pkg.binary_path {
            let ext = bin.extension().and_then(|e| e.to_str()).unwrap_or("so");
            std::fs::copy(bin, dest.join(format!("driver.{ext}")))?;
        }
        let pkg = read_package(&dest)?;
        if let Some(bin) = &pkg.binary_path {
            let registry = crate::driver::driver_registry();
            let guard = registry.write();
            if guard.find(workspace_id, &pkg.manifest.identity.driver).is_some() {
                guard
                    .unload(&pkg.manifest.identity.driver, workspace_id)
                    .map_err(|e| AccessError::Abi(e.to_string()))?;
            }
            guard
                .load_named(bin, workspace_id, &pkg.manifest.identity.driver)
                .map_err(|e| AccessError::Abi(e.to_string()))?;
        }
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
        self.driver_store
            .upsert_active(&rec)
            .await
            .map_err(AccessError::Store)?;
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
            let mut driver =
                crate::driver::create_driver(&driver_name, thing).map_err(|e| AccessError::Abi(e.to_string()))?;
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
