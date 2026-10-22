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
use tinyiothub_core::driver::manifest::{DriverManifest, LocationTrustDecl, MANIFEST_FILE, PlacementMode};
use tinyiothub_core::models::spatial::{LocationTrust, PlaceKind, ThingPosition};
use tinyiothub_core::models::thing::Thing;

use super::AccessError;
use super::DemandRecorder;
use super::demand::{DemandTracker, SUBSCRIPTION_TTL, SubscriptionId};
use super::device::DeviceDeclarationRegistry;
use super::materialize::materialize_declaration;
use super::package::{hex_sha256, read_package};
use super::registry::{InstalledPackage, ManifestRegistry};
use crate::driver::validator::DriverValidator;
use crate::maps::zone::within_bounds;
use crate::ports::{
    AcceptanceReportStore, DeclarationProjectionStore, DeviceDeclarationStore, DriverDeclarationStore, MapStore,
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
    /// Shared demand tracker (sampler + read tools + Hub stream).
    pub demand: Arc<DemandTracker>,
    /// E4d placement 物化写面（fixed/declared → 坐标列/map_id/zone_ref）；
    /// None = 未接线，带引用的静态 placement 在 bind 时 fail-closed。
    pub map_store: Option<Arc<dyn MapStore>>,
    /// E4d 空间读取缓存；物化/重绑后 invalidate，避免 5s TTL 内
    /// DataServer/watch 读到旧坐标/zoneRef。
    pub zones: Option<Arc<crate::maps::ZoneService>>,
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
        demand: Arc<DemandTracker>,
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
            demand,
            map_store: None,
            zones: None,
        }
    }

    /// 注入 E4d 空间写面（组合根在 `bind_thing` 前调用）。
    pub fn with_map_store(mut self, map_store: Arc<dyn MapStore>) -> Self {
        self.map_store = Some(map_store);
        self
    }

    /// 注入共享 `ZoneService`：placement 物化/重绑后清缓存。
    pub fn with_zone_service(mut self, zones: Arc<crate::maps::ZoneService>) -> Self {
        self.zones = Some(zones);
        self
    }

    fn invalidate_zones(&self, workspace_id: &str) {
        if let Some(zones) = &self.zones {
            zones.invalidate(workspace_id);
        }
    }

    /// Record a recent read so `demandOnly` sources keep sampling it.
    pub fn record_read(&self, workspace_id: &str, thing_id: &str, property: &str) {
        self.demand.record_read(workspace_id, thing_id, property);
    }

    /// Register an explicit subscription with the default TTL.
    pub fn subscribe(&self, workspace_id: &str, things: &[String], sources: &[String]) -> SubscriptionId {
        self.demand.subscribe(workspace_id, things, sources, SUBSCRIPTION_TTL)
    }

    /// Renew a subscription for another full TTL. Returns false if unknown.
    pub fn touch_subscription(&self, id: &SubscriptionId) -> bool {
        self.demand.touch(id)
    }

    /// Drop a subscription. Unknown ids are a no-op.
    pub fn unsubscribe(&self, id: &SubscriptionId) {
        self.demand.unsubscribe(id);
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
        //    The workspace id must be non-empty: `""` is the built-in slot.
        if workspace_id.trim().is_empty() {
            return Err(AccessError::Validation(
                "workspace id is required for driver install".into(),
            ));
        }
        let src_pkg = read_package(src)?;
        let id = &src_pkg.manifest.identity;
        // Path-safe identity: these values become directory names below.
        crate::driver::validation::validate_driver_name(&id.driver)
            .map_err(|e| AccessError::Validation(e.to_string()))?;
        tinyiothub_core::driver::manifest::validate_driver_version(&id.version)?;
        // A workspace package must not shadow a built-in implementation:
        // `create_driver` always prefers built-ins, so execution and the
        // declaration would come from different sources.
        if crate::driver::is_driver_supported(&id.driver) {
            return Err(AccessError::Validation(format!(
                "driver '{}' is a built-in implementation; install the package under a distinct driver name",
                id.driver
            )));
        }
        // Anything we accept must be executable: a non-built-in package with
        // no binary would materialize a declaration that can never poll.
        if src_pkg.binary_path.is_none() {
            return Err(AccessError::Validation(format!(
                "driver '{}' ships no ABI v2 binary and is not a built-in implementation, so it cannot execute",
                id.driver
            )));
        }
        if let Some(bin) = &src_pkg.binary_path {
            // Reject manifest/binary identity mismatches before committing
            // (spec §2.2/§3.1), using the 2 s / 256 KiB describe limits.
            DriverValidator::validate_identity(bin, &id.driver, &id.version)
                .map_err(|e| AccessError::Abi(e.to_string()))?;
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
        let mut installed = self.manifests.install(workspace_id, pkg)?;
        // Re-describe/re-materialize devices already bound to this driver.
        // Per-device failures keep the old declaration and never fail the
        // install (spec §3.1: upgrade then drift is reported, not rolled back).
        installed.warnings = self.rebind_bound_devices(workspace_id, &id.driver).await;
        Ok(installed)
    }

    /// Re-bind every active device of `(workspace, driver)` after an install.
    /// Returns human-readable warnings for devices that could not be
    /// re-described; their previous declaration/projection is left untouched.
    async fn rebind_bound_devices(&self, workspace_id: &str, driver: &str) -> Vec<String> {
        let rows = match self.device_store.list_by_workspace(workspace_id).await {
            Ok(rows) => rows,
            Err(e) => {
                tracing::warn!(
                    workspace_id,
                    driver,
                    error = %e,
                    "failed to list bound devices for post-install re-materialization"
                );
                return vec![format!("could not list bound devices for re-materialization: {e}")];
            }
        };
        let mut warnings = Vec::new();
        for row in rows
            .into_iter()
            .filter(|r| r.driver_name == driver && r.status == "active")
        {
            let thing = match self.things.load_thing(&row.thing_id).await {
                Ok(Some(thing)) => thing,
                Ok(None) => {
                    warnings.push(format!("thing '{}' no longer exists", row.thing_id));
                    continue;
                }
                Err(e) => {
                    warnings.push(format!("thing '{}' could not be loaded: {e}", row.thing_id));
                    continue;
                }
            };
            if let Err(e) = self.bind_thing(workspace_id, &thing).await {
                // `bind_thing` persists the new declaration before
                // materializing; put the previous row back so a failed
                // rebind truly keeps the old declaration.
                if let Err(restore) = self.device_store.upsert(&row).await {
                    tracing::warn!(
                        workspace_id,
                        driver,
                        thing_id = %row.thing_id,
                        error = %restore,
                        "failed to restore the previous declaration after a failed rebind"
                    );
                }
                tracing::warn!(
                    workspace_id,
                    driver,
                    thing_id = %row.thing_id,
                    error = %e,
                    "post-install re-materialization failed; keeping the previous declaration"
                );
                warnings.push(format!("thing '{}' kept its previous declaration: {e}", row.thing_id));
            }
        }
        warnings
    }

    pub fn get_manifest(&self, workspace_id: &str, driver: &str) -> Option<Arc<InstalledPackage>> {
        self.manifests.get(workspace_id, driver)
    }

    pub fn package_dir(&self, workspace_id: &str, driver: &str) -> Option<PathBuf> {
        if workspace_id.trim().is_empty() {
            return None;
        }
        let pkg = self.manifests.get(workspace_id, driver)?;
        let id = &pkg.manifest.identity;
        if crate::driver::validation::validate_driver_name(&id.driver).is_err()
            || tinyiothub_core::driver::manifest::validate_driver_version(&id.version).is_err()
        {
            return None;
        }
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
        // E4d §2.3：static placement 在建物/绑定驱动时物化；引用不存在则
        // fail-closed（绑定失败），不落半套坐标。放在声明校验之后，避免
        // 非法声明留下已物化的坐标。
        self.materialize_placement(workspace_id, thing, &pkg.manifest).await?;
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

    /// 物化 manifest placement：
    /// - `fixed|declared`：`position`/`mapRef`/`zoneRef` → 坐标列 + `map_id`
    ///   + `zone_ref`；清掉清单已移除的旧引用/静态坐标。
    /// - `runtime`：清掉旧静态 placement 状态（`zone_ref`、placement-owned
    ///   `map_id`、静态坐标），保留 runtime 坐标（继续按 age 派生）。
    /// - 位置摄取门：仅 runtime + 已声明 `kind=position` 的 `fromSource` 置 1。
    /// - 地图/区域名不存在、区域不属于 `mapRef`、坐标越界 → 绑定失败。
    /// - 静态位置不写 `position_updated_at`（读时恒 Trusted，不随 age 失效）。
    async fn materialize_placement(
        &self,
        workspace_id: &str,
        thing: &Thing,
        manifest: &DriverManifest,
    ) -> Result<(), AccessError> {
        let placement = &manifest.placement;
        let static_mode = placement.mode != PlacementMode::Runtime;
        let has_refs = placement.position.is_some() || placement.map_ref.is_some() || placement.zone_ref.is_some();

        let Some(store) = &self.map_store else {
            return if static_mode && has_refs {
                Err(AccessError::Validation(
                    "placement materialization requires a map store".into(),
                ))
            } else {
                Ok(())
            };
        };

        // 重绑/驱动升级：旧 placement 状态可能已被新清单移除。
        let current = store
            .get_position(workspace_id, &thing.id)
            .await
            .map_err(AccessError::Store)?;
        let current_zone_ref = store
            .get_zone_ref(workspace_id, &thing.id)
            .await
            .map_err(AccessError::Store)?;

        if !static_mode {
            // runtime：显式 zoneRef 优先级最高，必须清掉；placement-owned
            // map_id 清空；旧静态坐标整组清空（否则它们永不 aging）。
            if current_zone_ref.is_some() {
                store
                    .set_zone_ref(workspace_id, &thing.id, None)
                    .await
                    .map_err(AccessError::Store)?;
            }
            let next = match current.clone() {
                Some(mut position) if position.updated_at.is_some() => {
                    // runtime 坐标保留，只清 placement-owned 地图。
                    position.map_id = None;
                    position
                }
                // 旧静态坐标（updated_at=None）或从未定位：清空。
                _ => ThingPosition::default(),
            };
            if current.is_some() && current.as_ref() != Some(&next) {
                store
                    .set_position(workspace_id, &thing.id, &next)
                    .await
                    .map_err(AccessError::Store)?;
            }
            Self::materialize_ingest(store.as_ref(), workspace_id, thing, manifest).await?;
            self.invalidate_zones(workspace_id);
            return Ok(());
        }

        let map = match &placement.map_ref {
            Some(name) => {
                let maps = store.list_maps(workspace_id).await.map_err(AccessError::Store)?;
                Some(maps.iter().find(|map| &map.name == name).cloned().ok_or_else(|| {
                    AccessError::Validation(format!(
                        "placement mapRef '{name}' not found in workspace '{workspace_id}'"
                    ))
                })?)
            }
            None => None,
        };

        let zone = match &placement.zone_ref {
            Some(name) => {
                // zone 名称只保证「每图唯一」（UNIQUE(map_id,name)）：有
                // mapRef 时必须在**该图内**解析，否则多楼层同名 zone 会先
                // 命中默认图/更早的图，导致合法绑定被误拒。无 mapRef 时按
                // 工作区级解析（默认图优先，与 ZoneService 一致）。
                let found = match &map {
                    Some(map) => store
                        .list_places(workspace_id, &map.id)
                        .await
                        .map_err(AccessError::Store)?
                        .into_iter()
                        .find(|place| place.kind == PlaceKind::Zone && place.name == *name),
                    None => store
                        .find_place_by_name(workspace_id, name, Some(PlaceKind::Zone))
                        .await
                        .map_err(AccessError::Store)?,
                };
                Some(found.ok_or_else(|| {
                    let scope = map
                        .as_ref()
                        .map(|map| format!(" map '{}'", map.name))
                        .unwrap_or_default();
                    AccessError::Validation(format!(
                        "placement zoneRef '{name}' not found in workspace '{workspace_id}'{scope}"
                    ))
                })?)
            }
            None => None,
        };

        // 静态 placement 是权威：清单没有 position 时清空旧坐标（重绑不得
        // 遗留旧静态坐标或旧 map_id）。
        let mut next = current.clone().unwrap_or_default();
        next.map_id = map.as_ref().map(|map| map.id.clone());
        next.trust = trust_from_decl(placement.trust);
        next.updated_at = None;
        if let Some(raw) = &placement.position {
            let (x, y, level, heading) = parse_placement_position(raw)?;
            if let Some(map) = &map
                && !within_bounds(&map.bounds, x, y)
            {
                return Err(AccessError::Validation(format!(
                    "placement position ({x}, {y}) is outside map '{}' bounds",
                    map.name
                )));
            }
            next.x = Some(x);
            next.y = Some(y);
            next.level = level;
            next.heading = heading;
        } else {
            next.x = None;
            next.y = None;
            next.level = None;
            next.heading = None;
        }
        let has_position = next.x.is_some() || next.y.is_some() || next.map_id.is_some();
        if current.as_ref() != Some(&next) && (current.is_some() || has_position) {
            store
                .set_position(workspace_id, &thing.id, &next)
                .await
                .map_err(AccessError::Store)?;
        }

        let target_zone = zone.map(|zone| zone.name);
        if target_zone != current_zone_ref {
            store
                .set_zone_ref(workspace_id, &thing.id, target_zone.as_deref())
                .await
                .map_err(AccessError::Store)?;
        }
        Self::materialize_ingest(store.as_ref(), workspace_id, thing, manifest).await?;
        self.invalidate_zones(workspace_id);
        Ok(())
    }

    /// 摄取门 = `runtime` 且 `fromSource` 指向 manifest 声明的 kind=position
    /// source；其余一律关闭（fixed/declared 恒 trusted，位置不随驱动漂移）。
    async fn materialize_ingest(
        store: &dyn MapStore,
        workspace_id: &str,
        thing: &Thing,
        manifest: &DriverManifest,
    ) -> Result<(), AccessError> {
        let placement = &manifest.placement;
        let ingest = placement.mode == PlacementMode::Runtime
            && placement.from_source.as_deref().is_some_and(|source_id| {
                manifest
                    .sources
                    .iter()
                    .any(|source| source.id == source_id && source.kind == "position")
            });
        let current = store
            .get_position_ingest(workspace_id, &thing.id)
            .await
            .map_err(AccessError::Store)?;
        if current != ingest {
            store
                .set_position_ingest(workspace_id, &thing.id, ingest)
                .await
                .map_err(AccessError::Store)?;
        }
        Ok(())
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
        // Rows come from the DB, but the path is still built from them:
        // never trust persisted identity strings for path construction.
        if row.workspace_id.trim().is_empty() {
            return Err(AccessError::Validation("empty workspace id in persisted row".into()));
        }
        crate::driver::validation::validate_driver_name(&row.driver_name)
            .map_err(|e| AccessError::Validation(e.to_string()))?;
        tinyiothub_core::driver::manifest::validate_driver_version(&row.driver_version)?;
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

fn trust_from_decl(decl: LocationTrustDecl) -> LocationTrust {
    match decl {
        LocationTrustDecl::Trusted => LocationTrust::Trusted,
        LocationTrustDecl::Uncertain => LocationTrust::Uncertain,
        LocationTrustDecl::Lost => LocationTrust::Lost,
    }
}

/// placement.position：`{x, y, level?, heading?}`（米 / 整数层 / 度）。
fn parse_placement_position(raw: &serde_json::Value) -> Result<(f64, f64, Option<i64>, Option<f64>), AccessError> {
    let object = raw
        .as_object()
        .ok_or_else(|| AccessError::Validation("placement.position must be an object".into()))?;
    let number = |key: &str| object.get(key).and_then(serde_json::Value::as_f64);
    let x = number("x").ok_or_else(|| AccessError::Validation("placement.position.x must be a number".into()))?;
    let y = number("y").ok_or_else(|| AccessError::Validation("placement.position.y must be a number".into()))?;
    if !x.is_finite() || !y.is_finite() {
        return Err(AccessError::Validation("placement.position x/y must be finite".into()));
    }
    let level = object.get("level").and_then(serde_json::Value::as_i64);
    let heading = number("heading").filter(|value| value.is_finite());
    Ok((x, y, level, heading))
}

impl DemandRecorder for AccessService {
    fn record_read(&self, workspace_id: &str, thing_id: &str, property: &str) {
        self.demand.record_read(workspace_id, thing_id, property);
    }
}

#[async_trait::async_trait]
impl super::validator::AccessCallValidator for AccessService {
    async fn validate(
        &self,
        workspace_id: &str,
        thing_id: &str,
        tool: &str,
        params: Option<&serde_json::Value>,
    ) -> Result<super::validator::ValidatedCall, super::validator::CallError> {
        let decl = self
            .load_declaration(workspace_id, thing_id)
            .await
            .map_err(|e| super::validator::CallError::Unavailable(e.to_string()))?
            .ok_or_else(|| super::validator::CallError::NoDeclaration(thing_id.into()))?;
        super::validator::validate_call(&decl, tool, params, None)
    }
}

#[async_trait::async_trait]
impl super::resources::ResourceResolver for AccessService {
    async fn resolve_action(
        &self,
        workspace_id: &str,
        thing_id: &str,
        tool_name: &str,
    ) -> Result<Vec<super::resources::ResourceClaim>, String> {
        let decl = self
            .load_declaration(workspace_id, thing_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("no declaration for thing '{thing_id}'"))?;
        let tool = decl
            .tools
            .iter()
            .find(|t| t.name == tool_name)
            .ok_or_else(|| format!("action '{tool_name}' not declared"))?;
        // 未声明 resources 的工具回落合成 `__device__`——这一步不需要包
        // 清单；先返回，避免“包暂不可用”把原本受 E1 互斥保护的调用判死
        // （Issue 12）。
        if tool.resources.is_empty() {
            return Ok(vec![super::resources::device_claim()]);
        }
        let package = self
            .manifests
            .get(workspace_id, &decl.driver)
            .ok_or_else(|| format!("manifest for driver '{}' unavailable", decl.driver))?;
        super::resources::resolve_tool_claims(&package.manifest.resources, tool)
    }

    async fn resolve_properties(
        &self,
        workspace_id: &str,
        thing_id: &str,
        properties: &[String],
    ) -> Result<Vec<super::resources::ResourceClaim>, String> {
        // set_device 从未要求设备必须有声明：无声明时沿用 E1 合成设备资源
        // （仍然每设备互斥），而不是把存量设备的写入拒绝掉。
        let Some(decl) = self
            .load_declaration(workspace_id, thing_id)
            .await
            .map_err(|e| e.to_string())?
        else {
            return Ok(vec![super::resources::device_claim()]);
        };
        let package = self
            .manifests
            .get(workspace_id, &decl.driver)
            .ok_or_else(|| format!("manifest for driver '{}' unavailable", decl.driver))?;
        super::resources::resolve_property_claims(&package.manifest.resources, &decl, properties)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
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

    struct FailingProjection;

    #[async_trait::async_trait]
    impl DeclarationProjectionStore for FailingProjection {
        async fn upsert_properties(&self, _thing_id: &str, _fields: &[StateFieldDecl]) -> Result<(), String> {
            Err("projection write failed".into())
        }
        async fn upsert_commands(&self, _thing_id: &str, _tools: &[ManifestTool]) -> Result<(), String> {
            Err("projection write failed".into())
        }
        async fn mark_stale(&self, _thing_id: &str, _keep_p: &[String], _keep_c: &[String]) -> Result<(), String> {
            Err("projection write failed".into())
        }
    }

    struct FakeThings(Option<Thing>);

    #[async_trait::async_trait]
    impl ThingDeclarationSource for FakeThings {
        async fn load_thing(&self, _thing_id: &str) -> Result<Option<Thing>, String> {
            Ok(self.0.clone())
        }
    }

    // ── E4d placement 物化桩 ─────────────────────────────────────────

    use std::collections::HashMap;

    use tinyiothub_core::models::spatial::{Geometry, MapBounds, MapRecord, PlaceKind, PlaceRecord};

    use crate::ports::MapStore;

    #[derive(Default)]
    struct FakeMapStore {
        maps: Mutex<Vec<MapRecord>>,
        places: Mutex<Vec<PlaceRecord>>,
        positions: Mutex<HashMap<String, ThingPosition>>,
        zone_refs: Mutex<HashMap<String, String>>,
        ingest: Mutex<bool>,
    }

    impl FakeMapStore {
        fn with_floor() -> Self {
            let map = MapRecord {
                id: "m1".into(),
                workspace_id: "ws1".into(),
                name: "floor-1".into(),
                bounds: MapBounds {
                    x: 0.0,
                    y: 0.0,
                    w: 10.0,
                    h: 10.0,
                },
                anchor_map_id: None,
                is_default: true,
                created_at: "2026-10-22T00:00:00Z".into(),
                updated_at: "2026-10-22T00:00:00Z".into(),
            };
            let zone = PlaceRecord {
                id: "z1".into(),
                map_id: "m1".into(),
                name: "机房".into(),
                kind: PlaceKind::Zone,
                geometry: Geometry::Polygon {
                    points: vec![(0.0, 0.0), (5.0, 0.0), (5.0, 5.0), (0.0, 5.0)],
                },
                heading: None,
                created_at: "2026-10-22T00:00:00Z".into(),
                updated_at: "2026-10-22T00:00:00Z".into(),
            };
            Self {
                maps: Mutex::new(vec![map]),
                places: Mutex::new(vec![zone]),
                ..Default::default()
            }
        }

        fn position(&self, thing_id: &str) -> Option<ThingPosition> {
            self.positions.lock().unwrap().get(thing_id).cloned()
        }

        /// 两张图 + 同名 zone（多楼层「机房」）；非默认图 floor-2 排在后面。
        /// `find_place_by_name`（工作区级）会先返回默认图 m1 的 zone，正是
        /// mapRef 未限定解析时误拒合法绑定的场景。
        fn with_two_floors_same_zone() -> Self {
            let store = Self::with_floor();
            store.maps.lock().unwrap().push(MapRecord {
                id: "m2".into(),
                workspace_id: "ws1".into(),
                name: "floor-2".into(),
                bounds: MapBounds {
                    x: 0.0,
                    y: 0.0,
                    w: 10.0,
                    h: 10.0,
                },
                anchor_map_id: None,
                is_default: false,
                created_at: "2026-10-22T00:00:00Z".into(),
                updated_at: "2026-10-22T00:00:00Z".into(),
            });
            store.places.lock().unwrap().push(PlaceRecord {
                id: "z2".into(),
                map_id: "m2".into(),
                name: "机房".into(),
                kind: PlaceKind::Zone,
                geometry: Geometry::Polygon {
                    points: vec![(0.0, 0.0), (5.0, 0.0), (5.0, 5.0), (0.0, 5.0)],
                },
                heading: None,
                created_at: "2026-10-22T00:00:00Z".into(),
                updated_at: "2026-10-22T00:00:00Z".into(),
            });
            store
        }

        fn zone_ref(&self, thing_id: &str) -> Option<String> {
            self.zone_refs.lock().unwrap().get(thing_id).cloned()
        }

        fn ingest(&self) -> bool {
            *self.ingest.lock().unwrap()
        }
    }

    #[async_trait::async_trait]
    impl MapStore for FakeMapStore {
        async fn list_maps(&self, _workspace_id: &str) -> Result<Vec<MapRecord>, String> {
            Ok(self.maps.lock().unwrap().clone())
        }
        async fn default_map(&self, _workspace_id: &str) -> Result<Option<MapRecord>, String> {
            Ok(self.maps.lock().unwrap().iter().find(|m| m.is_default).cloned())
        }
        async fn upsert_map(&self, _map: &MapRecord) -> Result<(), String> {
            Ok(())
        }
        async fn delete_map(&self, _workspace_id: &str, _id: &str) -> Result<bool, String> {
            Ok(false)
        }
        async fn list_places(&self, _workspace_id: &str, map_id: &str) -> Result<Vec<PlaceRecord>, String> {
            Ok(self
                .places
                .lock()
                .unwrap()
                .iter()
                .filter(|p| p.map_id == map_id)
                .cloned()
                .collect())
        }
        async fn find_place_by_name(
            &self,
            _workspace_id: &str,
            name: &str,
            kind: Option<PlaceKind>,
        ) -> Result<Option<PlaceRecord>, String> {
            Ok(self
                .places
                .lock()
                .unwrap()
                .iter()
                .find(|p| p.name == name && kind.is_none_or(|k| p.kind == k))
                .cloned())
        }
        async fn upsert_place(&self, _workspace_id: &str, _place: &PlaceRecord) -> Result<(), String> {
            Ok(())
        }
        async fn delete_place(&self, _workspace_id: &str, _map_id: &str, _id: &str) -> Result<bool, String> {
            Ok(false)
        }
        async fn get_position(&self, _workspace_id: &str, thing_id: &str) -> Result<Option<ThingPosition>, String> {
            Ok(self.position(thing_id))
        }
        async fn set_position(
            &self,
            _workspace_id: &str,
            thing_id: &str,
            position: &ThingPosition,
        ) -> Result<(), String> {
            self.positions
                .lock()
                .unwrap()
                .insert(thing_id.to_string(), position.clone());
            Ok(())
        }
        async fn set_reading_position(
            &self,
            _workspace_id: &str,
            _thing_id: &str,
            _position: &ThingPosition,
        ) -> Result<(), String> {
            Ok(())
        }
        async fn list_positions(&self, _workspace_id: &str) -> Result<Vec<(String, ThingPosition)>, String> {
            Ok(self
                .positions
                .lock()
                .unwrap()
                .iter()
                .map(|(id, pos)| (id.clone(), pos.clone()))
                .collect())
        }
        async fn get_zone_ref(&self, _workspace_id: &str, thing_id: &str) -> Result<Option<String>, String> {
            Ok(self.zone_ref(thing_id))
        }
        async fn set_zone_ref(
            &self,
            _workspace_id: &str,
            thing_id: &str,
            zone_ref: Option<&str>,
        ) -> Result<(), String> {
            match zone_ref {
                Some(name) => {
                    self.zone_refs
                        .lock()
                        .unwrap()
                        .insert(thing_id.to_string(), name.to_string());
                }
                None => {
                    self.zone_refs.lock().unwrap().remove(thing_id);
                }
            }
            Ok(())
        }
        async fn list_zone_refs(&self, _workspace_id: &str) -> Result<Vec<(String, String)>, String> {
            Ok(self
                .zone_refs
                .lock()
                .unwrap()
                .iter()
                .map(|(id, name)| (id.clone(), name.clone()))
                .collect())
        }
        async fn get_position_ingest(&self, _workspace_id: &str, _thing_id: &str) -> Result<bool, String> {
            Ok(self.ingest())
        }
        async fn set_position_ingest(&self, _workspace_id: &str, _thing_id: &str, ingest: bool) -> Result<(), String> {
            *self.ingest.lock().unwrap() = ingest;
            Ok(())
        }
        async fn zone_source(&self, _workspace_id: &str) -> Result<String, String> {
            Ok("maps".into())
        }
    }

    /// 把 FIXED_MANIFEST 的 placement 替换为静态 placement。
    fn placement_manifest(mode: &str, map_ref: Option<&str>, zone_ref: Option<&str>, position: Option<&str>) -> String {
        let mut fields = vec![format!("\"mode\":\"{mode}\"")];
        if let Some(map_ref) = map_ref {
            fields.push(format!("\"mapRef\":\"{map_ref}\""));
        }
        if let Some(zone_ref) = zone_ref {
            fields.push(format!("\"zoneRef\":\"{zone_ref}\""));
        }
        if let Some(position) = position {
            fields.push(format!("\"position\":{position}"));
        }
        let placement = format!("{{{}}}", fields.join(","));
        FIXED_MANIFEST.replace(
            "\"placement\":{\"mode\":\"runtime\"}",
            &format!("\"placement\":{placement}"),
        )
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
        demand: Arc<DemandTracker>,
        _data: tempfile::TempDir,
    }

    fn fixture(thing: Option<Thing>) -> Fixture {
        let data = tempfile::tempdir().unwrap();
        let driver_store = Arc::new(FakeDriverStore::default());
        let device_store = Arc::new(FakeDeviceStore::default());
        let projection = Arc::new(FakeProjection::default());
        let things = Arc::new(FakeThings(thing));
        let demand = Arc::new(DemandTracker::new());
        let service = AccessService::new(
            data.path().to_path_buf(),
            Arc::new(ManifestRegistry::new()),
            driver_store.clone(),
            device_store.clone(),
            projection.clone(),
            things.clone(),
            Arc::new(FakeAcceptance),
            demand.clone(),
        );
        Fixture {
            service,
            driver_store,
            device_store,
            projection,
            things,
            demand,
            _data: data,
        }
    }

    fn package_dir(manifest: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(MANIFEST_FILE), manifest).unwrap();
        dir
    }

    /// A manifest-only package that is intentionally unusable, used for
    /// "must reject" cases.
    fn manifest_only_package(manifest: &str) -> tempfile::TempDir {
        package_dir(manifest)
    }

    /// The vendored reference package (manifest + built cdylib). Returns
    /// `None` when the cdylib has not been built, so runtime-only test runs
    /// skip instead of failing.
    fn demo_package() -> Option<tempfile::TempDir> {
        demo_package_with_manifest(DEMO_REAL_MANIFEST)
    }

    fn demo_package_with_manifest(manifest: &str) -> Option<tempfile::TempDir> {
        let lib = demo_cdylib()?;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(MANIFEST_FILE), manifest).unwrap();
        let ext = lib.extension().and_then(|e| e.to_str()).unwrap_or("so");
        std::fs::copy(&lib, dir.path().join(format!("driver.{ext}"))).unwrap();
        Some(dir)
    }

    fn skip_without_demo_cdylib(test: &str) -> bool {
        if demo_cdylib().is_none() {
            eprintln!("skipping {test}: demo-declarative cdylib not built");
            return true;
        }
        false
    }

    fn thing(driver: &str) -> Thing {
        thing_in("ws1", driver)
    }

    fn thing_in(workspace: &str, driver: &str) -> Thing {
        Thing {
            id: "t1".into(),
            name: "t1".into(),
            driver_name: Some(driver.into()),
            workspace_id: Some(workspace.into()),
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
        if skip_without_demo_cdylib("install_persists_package_and_records_active_row") {
            return;
        }
        let ws = unique_workspace("ws-install");
        let fx = fixture(None);
        let src = demo_package().expect("demo package");

        let pkg = fx.service.install_dir(&ws, src.path()).await.unwrap();

        assert_eq!(pkg.manifest.identity.driver, "demo-declarative");
        assert!(pkg.warnings.is_empty(), "warnings: {:?}", pkg.warnings);
        let persisted = fx
            .service
            .package_dir(&ws, "demo-declarative")
            .expect("package dir should be resolvable");
        assert!(persisted.join(MANIFEST_FILE).is_file());
        assert!(fx.service.get_manifest(&ws, "demo-declarative").is_some());
        let rows = fx.driver_store.list_active(&ws).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, "active");
        assert_eq!(rows[0].manifest_hash, pkg.manifest_hash);
    }

    #[tokio::test]
    async fn install_rejects_path_traversal_and_empty_workspace() {
        let fx = fixture(None);
        // Driver name traversal: read_package accepts it (non-empty), the
        // service must reject it before building any path.
        let traversal = package_dir(&FIXED_MANIFEST.replace("\"driver\":\"demo\"", "\"driver\":\"../wsB/foo\""));
        let err = fx.service.install_dir("ws1", traversal.path()).await.unwrap_err();
        assert!(matches!(err, AccessError::Validation(_)), "{err:?}");
        assert!(
            !fx.service.data_dir.join("drivers/workspaces/wsB").exists(),
            "traversal manifest must not create a cross-workspace directory"
        );

        // Version traversal is rejected by manifest validation.
        let bad_version = package_dir(&FIXED_MANIFEST.replace("\"version\":\"1.0.0\"", "\"version\":\"..\""));
        assert!(fx.service.install_dir("ws1", bad_version.path()).await.is_err());
        let oversized = format!("1.{}", "0".repeat(128));
        let bad_version =
            package_dir(&FIXED_MANIFEST.replace("\"version\":\"1.0.0\"", &format!("\"version\":\"{oversized}\"")));
        assert!(fx.service.install_dir("ws1", bad_version.path()).await.is_err());

        // Empty workspace would write into the built-in slot.
        let src = package_dir(FIXED_MANIFEST);
        let err = fx.service.install_dir("", src.path()).await.unwrap_err();
        assert!(matches!(err, AccessError::Validation(_)), "{err:?}");
        assert!(!fx.service.data_dir.join("drivers/workspaces").exists());
    }

    #[tokio::test]
    async fn install_rejects_manifest_only_and_builtin_packages() {
        let fx = fixture(None);
        // Non-built-in without a binary: accepted by the old dynamic-only
        // check, unusable at runtime.
        let err = fx
            .service
            .install_dir("ws1", manifest_only_package(FIXED_MANIFEST).path())
            .await
            .unwrap_err();
        assert!(matches!(err, AccessError::Validation(_)), "{err:?}");
        assert!(err.to_string().contains("cannot execute"), "{err}");

        // Built-in names must not be shadowed by a workspace package.
        let builtin = DYNAMIC_MANIFEST.replace("demo-dynamic", "simulator");
        let err = fx
            .service
            .install_dir("ws1", manifest_only_package(&builtin).path())
            .await
            .unwrap_err();
        assert!(matches!(err, AccessError::Validation(_)), "{err:?}");
        assert!(err.to_string().contains("built-in"), "{err}");
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
        assert!(matches!(err, AccessError::Validation(_)), "{err:?}");
        assert!(fx.service.get_manifest("ws1", "demo-dynamic").is_none());
    }

    #[tokio::test]
    async fn reinstall_replaces_the_whole_version_directory() {
        if skip_without_demo_cdylib("reinstall_replaces_the_whole_version_directory") {
            return;
        }
        let ws = unique_workspace("ws-reinstall");
        let fx = fixture(None);
        let src = demo_package().expect("demo package");
        let pkg = fx.service.install_dir(&ws, src.path()).await.unwrap();
        let dest = fx
            .service
            .data_dir
            .join("drivers/workspaces")
            .join(&ws)
            .join("demo-declarative/1.0.0");
        let binary = pkg.binary_path.as_ref().expect("binary package");
        let actual_ext = binary.extension().and_then(|e| e.to_str()).unwrap_or("so");
        let stale_ext = if actual_ext == "dylib" { "so" } else { "dylib" };
        std::fs::write(dest.join(format!("driver.{stale_ext}")), b"stale binary").unwrap();

        let pkg = fx.service.install_dir(&ws, src.path()).await.unwrap();

        assert!(pkg.binary_path.is_some(), "binary package must stay binary");
        assert!(
            !dest.join(format!("driver.{stale_ext}")).exists(),
            "stale binary survived a reinstall"
        );
        assert!(dest.join(MANIFEST_FILE).is_file());
    }

    #[tokio::test]
    async fn corrupt_binary_is_rejected_before_the_working_package_is_touched() {
        if skip_without_demo_cdylib("corrupt_binary_is_rejected_before_the_working_package_is_touched") {
            return;
        }
        let ws = unique_workspace("ws-corrupt");
        let fx = fixture(None);
        let src = demo_package().expect("demo package");
        fx.service.install_dir(&ws, src.path()).await.unwrap();
        let dest = fx
            .service
            .data_dir
            .join("drivers/workspaces")
            .join(&ws)
            .join("demo-declarative/1.0.0");

        let bad = tempfile::tempdir().unwrap();
        std::fs::write(bad.path().join(MANIFEST_FILE), DEMO_REAL_MANIFEST).unwrap();
        std::fs::write(bad.path().join("driver.so"), b"not a shared library").unwrap();
        assert!(fx.service.install_dir(&ws, bad.path()).await.is_err());

        assert!(
            fx.service.get_manifest(&ws, "demo-declarative").is_some(),
            "working manifest must remain installed"
        );
        assert!(dest.join(MANIFEST_FILE).is_file());
        assert!(!dest.join("driver.so").exists(), "rejected binary must not be copied");
    }

    #[tokio::test]
    async fn binary_identity_mismatch_is_rejected_before_commit() {
        if skip_without_demo_cdylib("binary_identity_mismatch_is_rejected_before_commit") {
            return;
        }
        let fx = fixture(None);
        let mismatched = DEMO_REAL_MANIFEST.replace("\"driver\": \"demo-declarative\"", "\"driver\": \"demo-other\"");
        let src = demo_package_with_manifest(&mismatched).expect("demo package");
        let err = fx.service.install_dir("ws1", src.path()).await.unwrap_err();
        assert!(matches!(err, AccessError::Abi(_)), "{err:?}");
        assert!(err.to_string().contains("identity mismatch"), "{err}");
        assert!(fx.service.get_manifest("ws1", "demo-other").is_none());
    }

    #[tokio::test]
    async fn install_rematerializes_already_bound_devices() {
        if skip_without_demo_cdylib("install_rematerializes_already_bound_devices") {
            return;
        }
        let ws = unique_workspace("ws-rematerialize");
        let t = thing_in(&ws, "demo-declarative");
        let fx = fixture(Some(t.clone()));
        let src = demo_package().expect("demo package");
        fx.service.install_dir(&ws, src.path()).await.unwrap();
        fx.service.bind_thing(&ws, &t).await.unwrap();

        // Same identity/version, but the package now declares an extra state
        // field. Bound devices must pick it up without being recreated.
        let updated = DEMO_REAL_MANIFEST.replace(
            "\"stateFields\": [",
            "\"stateFields\": [{\"name\":\"temp\",\"type\":\"float\",\"required\":false},",
        );
        let src = demo_package_with_manifest(&updated).expect("updated package");
        let pkg = fx.service.install_dir(&ws, src.path()).await.unwrap();
        assert!(pkg.warnings.is_empty(), "warnings: {:?}", pkg.warnings);

        let rows = fx.device_store.list_by_workspace(&ws).await.unwrap();
        let latest = rows.last().expect("re-materialized row");
        assert!(
            latest.declaration_json.contains("\"temp\""),
            "bound device kept the old declaration: {}",
            latest.declaration_json
        );
        assert!(
            fx.projection.props.lock().unwrap().contains(&"temp".to_string()),
            "projection was not refreshed"
        );
    }

    #[tokio::test]
    async fn install_records_a_warning_when_a_bound_device_cannot_rebind() {
        if skip_without_demo_cdylib("install_records_a_warning_when_a_bound_device_cannot_rebind") {
            return;
        }
        // `FakeThings(None)` cannot load the bound thing: install succeeds
        // but reports the per-device failure instead of rolling back.
        let ws = unique_workspace("ws-warning");
        let fx = fixture(None);
        fx.device_store
            .upsert(&DeviceDeclarationRecord {
                workspace_id: ws.clone(),
                thing_id: "gone".into(),
                driver_name: "demo-declarative".into(),
                driver_version: "1.0.0".into(),
                declaration_json: "{}".into(),
                declaration_hash: "h".into(),
                status: "active".into(),
                updated_at: "2026-10-21".into(),
            })
            .await
            .unwrap();
        let src = demo_package().expect("demo package");
        let pkg = fx.service.install_dir(&ws, src.path()).await.unwrap();
        assert_eq!(pkg.warnings.len(), 1, "warnings: {:?}", pkg.warnings);
        assert!(pkg.warnings[0].contains("gone"), "{:?}", pkg.warnings);
        assert!(fx.service.get_manifest(&ws, "demo-declarative").is_some());
    }

    #[tokio::test]
    async fn failed_rebind_restores_the_previous_declaration_row() {
        if skip_without_demo_cdylib("failed_rebind_restores_the_previous_declaration_row") {
            return;
        }
        let ws = unique_workspace("ws-restore");
        let t = thing_in(&ws, "demo-declarative");
        let data = tempfile::tempdir().unwrap();
        let device_store = Arc::new(FakeDeviceStore::default());
        let old = DeviceDeclarationRecord {
            workspace_id: ws.clone(),
            thing_id: t.id.clone(),
            driver_name: "demo-declarative".into(),
            driver_version: "1.0.0".into(),
            declaration_json: "{\"driver\":\"old\"}".into(),
            declaration_hash: "old".into(),
            status: "active".into(),
            updated_at: "2026-10-21".into(),
        };
        device_store.upsert(&old).await.unwrap();
        let service = AccessService::new(
            data.path().to_path_buf(),
            Arc::new(ManifestRegistry::new()),
            Arc::new(FakeDriverStore::default()),
            device_store.clone(),
            Arc::new(FailingProjection),
            Arc::new(FakeThings(Some(t))),
            Arc::new(FakeAcceptance),
            Arc::new(DemandTracker::new()),
        );

        let src = demo_package().expect("demo package");
        let pkg = service.install_dir(&ws, src.path()).await.unwrap();
        assert_eq!(pkg.warnings.len(), 1, "warnings: {:?}", pkg.warnings);

        let restored = device_store.find(&ws, "t1").await.unwrap().expect("row");
        assert_eq!(
            restored.declaration_json, old.declaration_json,
            "failed rebind must restore the previous declaration row"
        );
    }

    #[tokio::test]
    async fn db_failure_is_reported_and_package_is_not_registered() {
        if skip_without_demo_cdylib("db_failure_is_reported_and_package_is_not_registered") {
            return;
        }
        let data = tempfile::tempdir().unwrap();
        let service = AccessService::new(
            data.path().to_path_buf(),
            Arc::new(ManifestRegistry::new()),
            Arc::new(FailingDriverStore),
            Arc::new(FakeDeviceStore::default()),
            Arc::new(FakeProjection::default()),
            Arc::new(FakeThings(None)),
            Arc::new(FakeAcceptance),
            Arc::new(DemandTracker::new()),
        );
        let src = demo_package().expect("demo package");
        let ws = unique_workspace("ws-dbfail");
        let err = service.install_dir(&ws, src.path()).await.unwrap_err();
        assert!(matches!(err, AccessError::Store(_)), "{err:?}");
        assert!(service.get_manifest(&ws, "demo-declarative").is_none());
    }

    #[tokio::test]
    async fn bind_fixed_driver_persists_and_materializes_declaration() {
        if skip_without_demo_cdylib("bind_fixed_driver_persists_and_materializes_declaration") {
            return;
        }
        let ws = unique_workspace("ws-bind");
        let t = thing_in(&ws, "demo-declarative");
        let fx = fixture(Some(t.clone()));
        let src = demo_package().expect("demo package");
        fx.service.install_dir(&ws, src.path()).await.unwrap();

        let decl = fx.service.bind_thing(&ws, &t).await.unwrap();

        assert_eq!(decl.driver, "demo-declarative");
        assert_eq!(decl.state_fields[0].name, "level");
        assert_eq!(fx.projection.props.lock().unwrap().as_slice(), ["level"]);
        let rows = fx.device_store.list_by_workspace(&ws).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].declaration_hash.len(), 64);
        // Cached: second read must not hit the store (store returns the same row).
        let loaded = fx.service.load_declaration(&ws, "t1").await.unwrap().unwrap();
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

    /// fixed / declared placement → 坐标列 + map_id + zone_ref；静态位置不写
    /// updated_at（读时恒 Trusted）。
    #[tokio::test]
    async fn bind_materializes_fixed_and_declared_placement() {
        for mode in ["fixed", "declared"] {
            let t = thing_in("ws1", "demo");
            let fx = fixture(Some(t.clone()));
            let store = Arc::new(FakeMapStore::with_floor());
            fx.service
                .manifests
                .seed_builtin(&placement_manifest(
                    mode,
                    Some("floor-1"),
                    Some("机房"),
                    Some(r#"{"x":1.5,"y":2.5,"level":1}"#),
                ))
                .expect("placement manifest must validate");
            let mut service = fx.service;
            service.map_store = Some(store.clone());

            service.bind_thing("ws1", &t).await.unwrap();

            let pos = store.position("t1").expect("materialized position");
            assert_eq!((pos.x, pos.y), (Some(1.5), Some(2.5)), "mode={mode}");
            assert_eq!(pos.level, Some(1));
            assert_eq!(pos.map_id.as_deref(), Some("m1"));
            assert_eq!(pos.trust, LocationTrust::Trusted);
            assert_eq!(pos.updated_at, None, "静态位置不得写入 updated_at");
            assert_eq!(store.zone_ref("t1").as_deref(), Some("机房"));
            assert!(!store.ingest(), "static placement 不得启用位置摄取");
        }
    }

    /// 同名 zone 出现在多张图时，`mapRef` 必须把 `zoneRef` 限定在该图内解析
    ///（zone 名称只保证每图唯一；多楼层「机房」是合法配置）。
    #[tokio::test]
    async fn bind_resolves_zone_ref_within_map_ref_when_names_repeat() {
        let t = thing_in("ws1", "demo");
        let fx = fixture(Some(t.clone()));
        let store = Arc::new(FakeMapStore::with_two_floors_same_zone());
        fx.service
            .manifests
            .seed_builtin(&placement_manifest(
                "fixed",
                Some("floor-2"),
                Some("机房"),
                Some(r#"{"x":1.5,"y":2.5}"#),
            ))
            .expect("placement manifest must validate");
        let mut service = fx.service;
        service.map_store = Some(store.clone());

        service
            .bind_thing("ws1", &t)
            .await
            .expect("同名 zone 在 mapRef 图上存在，绑定必须成功");

        let pos = store.position("t1").expect("materialized position");
        assert_eq!(pos.map_id.as_deref(), Some("m2"), "必须落在 mapRef 指定的图");
        assert_eq!(store.zone_ref("t1").as_deref(), Some("机房"));
    }

    /// Task 4 follow-up：物化/重绑写穿共享 ZoneService 的缓存（否则 5s TTL
    /// 内 watch/DataServer 仍读旧坐标与旧 zoneRef）。
    #[tokio::test]
    async fn bind_invalidates_zone_cache_after_materialization() {
        use std::time::Duration;

        use crate::maps::{TrustThresholds, ZoneService};

        let t = thing_in("ws1", "demo");
        let fx = fixture(Some(t.clone()));
        let store = Arc::new(FakeMapStore::with_floor());
        fx.service
            .manifests
            .seed_builtin(&placement_manifest(
                "fixed",
                Some("floor-1"),
                Some("机房"),
                Some(r#"{"x":1.5,"y":2.5}"#),
            ))
            .unwrap();

        let zones = Arc::new(ZoneService::with_ttl(
            store.clone(),
            TrustThresholds::default(),
            Duration::from_secs(60),
        ));
        let now = chrono::Utc::now();
        // 绑定前先读一次：坐标/zoneRef 缺失进入 60s 缓存。
        assert_eq!(zones.zone_for("ws1", "t1", None, now).await.unwrap(), None);

        let mut service = fx.service;
        service.map_store = Some(store.clone());
        service.zones = Some(zones.clone());
        service.bind_thing("ws1", &t).await.unwrap();

        assert_eq!(
            zones.zone_for("ws1", "t1", None, now).await.unwrap().as_deref(),
            Some("机房"),
            "bind 必须先失效缓存再供读"
        );
    }

    /// 驱动升级 fixed → runtime（新清单移除 zoneRef/position）：旧静态
    /// placement 状态必须清空，否则显式 zoneRef 会把设备永久钉在旧区域。
    #[tokio::test]
    async fn rebind_to_runtime_clears_stale_static_placement() {
        let t = thing_in("ws1", "demo");
        let fx = fixture(Some(t.clone()));
        let store = Arc::new(FakeMapStore::with_floor());
        fx.service
            .manifests
            .seed_builtin(&placement_manifest(
                "fixed",
                Some("floor-1"),
                Some("机房"),
                Some(r#"{"x":1.5,"y":2.5,"level":1}"#),
            ))
            .unwrap();
        let mut service = fx.service;
        service.map_store = Some(store.clone());

        service.bind_thing("ws1", &t).await.unwrap();
        assert!(store.position("t1").is_some());
        assert_eq!(store.zone_ref("t1").as_deref(), Some("机房"));

        // 新 manifest：runtime，无 zoneRef/position。
        service
            .manifests
            .seed_builtin(&placement_manifest("runtime", None, None, None))
            .unwrap();
        service.bind_thing("ws1", &t).await.unwrap();

        assert_eq!(
            store.position("t1"),
            Some(ThingPosition::default()),
            "旧静态坐标（含 map_id）必须清空"
        );
        assert_eq!(store.zone_ref("t1"), None, "旧 zoneRef 必须清空");
        assert!(!store.ingest());
    }

    /// 摄取门：仅 runtime + fromSource 指向已声明的 kind=position source。
    #[tokio::test]
    async fn position_ingest_requires_runtime_and_declared_position_source() {
        let cases = [
            (
                placement_manifest("fixed", None, None, Some(r#"{"x":1,"y":1}"#)),
                false,
                "fixed",
            ),
            (
                placement_manifest("runtime", None, None, None),
                false,
                "runtime without source",
            ),
            (runtime_position_manifest(), true, "runtime + position source"),
        ];
        for (manifest, expected, label) in cases {
            let t = thing_in("ws1", "demo");
            let fx = fixture(Some(t.clone()));
            let store = Arc::new(FakeMapStore::with_floor());
            fx.service.manifests.seed_builtin(&manifest).expect("manifest");
            let mut service = fx.service;
            service.map_store = Some(store.clone());
            service.bind_thing("ws1", &t).await.unwrap();
            assert_eq!(store.ingest(), expected, "{label}");
        }
    }

    /// runtime + fromSource 指向已声明的 position source。
    fn runtime_position_manifest() -> String {
        FIXED_MANIFEST
            .replace(
                "\"placement\":{\"mode\":\"runtime\"}",
                "\"placement\":{\"mode\":\"runtime\",\"fromSource\":\"position\"}",
            )
            .replace(
                "\"sources\":[{",
                "\"sources\":[{\"id\":\"position\",\"kind\":\"position\",\"fields\":[]}, {",
            )
    }

    /// 无效 mapRef / zoneRef / 越界坐标 / 未接线 map store → bind 失败
    ///（fail-closed，且不落声明行）。
    #[tokio::test]
    async fn bind_rejects_invalid_or_unwired_placement() {
        let cases = [
            (
                placement_manifest("fixed", Some("missing-map"), None, Some(r#"{"x":1,"y":1}"#)),
                "mapRef",
                true,
            ),
            (
                placement_manifest("fixed", None, Some("missing-zone"), Some(r#"{"x":1,"y":1}"#)),
                "zoneRef",
                true,
            ),
            (
                placement_manifest("fixed", Some("floor-1"), None, Some(r#"{"x":99,"y":1}"#)),
                "outside",
                true,
            ),
            (
                placement_manifest("fixed", Some("floor-1"), None, Some(r#"{"x":1,"y":1}"#)),
                "map store",
                false,
            ),
        ];
        for (manifest, needle, wire_store) in cases {
            let t = thing_in("ws1", "demo");
            let fx = fixture(Some(t.clone()));
            fx.service.manifests.seed_builtin(&manifest).expect("manifest");
            let mut service = fx.service;
            if wire_store {
                service.map_store = Some(Arc::new(FakeMapStore::with_floor()));
            }
            let err = service.bind_thing("ws1", &t).await.unwrap_err();
            assert!(
                err.to_string().contains(needle),
                "expected error containing {needle:?}, got: {err}"
            );
            assert!(
                service.device_store.list_by_workspace("ws1").await.unwrap().is_empty(),
                "fail-closed：绑定失败不得落声明行"
            );
        }
    }

    /// zoneRef 与 mapRef 不一致（区域在别的图）→ bind 失败。
    #[tokio::test]
    async fn bind_rejects_zone_ref_on_another_map() {
        let t = thing_in("ws1", "demo");
        let fx = fixture(Some(t.clone()));
        fx.service
            .manifests
            .seed_builtin(&placement_manifest(
                "fixed",
                Some("floor-1"),
                Some("机房"),
                Some(r#"{"x":1,"y":1}"#),
            ))
            .unwrap();
        let store = Arc::new(FakeMapStore::with_floor());
        // 区域挪到另一张图
        {
            let mut maps = store.maps.lock().unwrap();
            let template = maps[0].clone();
            maps.push(MapRecord {
                id: "m2".into(),
                name: "floor-2".into(),
                is_default: false,
                ..template
            });
        }
        store.places.lock().unwrap()[0].map_id = "m2".into();
        let mut service = fx.service;
        service.map_store = Some(store);

        let err = service.bind_thing("ws1", &t).await.unwrap_err();
        assert!(
            err.to_string().contains("not found in workspace 'ws1' map 'floor-1'"),
            "zoneRef 必须在 mapRef 图内解析：{err}"
        );
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

    #[test]
    fn access_service_records_reads_and_subscriptions_in_shared_tracker() {
        let fx = fixture(None);
        let demand = &fx.demand;

        // Fresh tracker: nothing is demanded.
        assert!(!demand.has_demand_for_property("ws1", "t1", "level"));

        // A read keeps the property (and thus the thing) in demand.
        fx.service.record_read("ws1", "t1", "level");
        assert!(demand.has_demand_for_property("ws1", "t1", "level"));
        assert!(demand.has_demand("ws1", "t1", "telemetry"));
        // Workspace isolation: another workspace sees no demand.
        assert!(!demand.has_demand_for_property("ws2", "t1", "level"));
        assert!(!demand.has_demand("ws2", "t1", "telemetry"));

        // Trait-object dispatch used by the read tools hits the same tracker.
        let recorder: &dyn DemandRecorder = &fx.service;
        recorder.record_read("ws1", "t1", "humidity");
        assert!(demand.has_demand_for_property("ws1", "t1", "humidity"));

        // Subscribe → touch → unsubscribe.
        let things = vec!["t1".to_string()];
        let sources = vec!["telemetry".to_string()];
        let id = fx.service.subscribe("ws1", &things, &sources);
        assert!(demand.has_subscription_demand("ws1", "t1", "telemetry"));
        assert!(!demand.has_subscription_demand("ws2", "t1", "telemetry"));
        assert!(fx.service.touch_subscription(&id));

        fx.service.unsubscribe(&id);
        assert!(!demand.has_subscription_demand("ws1", "t1", "telemetry"));
        // Unknown ids are no-ops.
        assert!(!fx.service.touch_subscription(&id));
        fx.service.unsubscribe(&id);
    }

    #[test]
    fn subscribe_uses_the_30s_ttl_boundary() {
        let fx = fixture(None);
        let things = vec!["t1".to_string()];
        let sources = vec!["telemetry".to_string()];

        // subscribe() uses the current instant internally; bracket it so the
        // 29s / 31s assertions pin TTL=30s regardless of scheduling jitter.
        let before = Instant::now();
        let id = fx.service.subscribe("ws1", &things, &sources);
        let after = Instant::now();

        assert!(
            fx.demand
                .has_subscription_demand_at("ws1", "t1", "telemetry", before + Duration::from_secs(29)),
            "subscription must still demand at 29s"
        );
        assert!(
            !fx.demand
                .has_subscription_demand_at("ws1", "t1", "telemetry", after + Duration::from_secs(31)),
            "subscription must be expired by 31s"
        );

        // touch renews the full TTL from the touch instant.
        assert!(fx.service.touch_subscription(&id));
        assert!(fx.demand.has_subscription_demand_at(
            "ws1",
            "t1",
            "telemetry",
            Instant::now() + Duration::from_secs(29)
        ));
        fx.service.unsubscribe(&id);
    }

    // ── E4c：资源解析（声明 → 包级 resources[]）────────────

    const RESOURCE_MANIFEST: &str = r#"{
      "manifestVersion":"1.0",
      "identity":{"driver":"resource-demo","version":"1.0.0","protocol":"demo",
                  "abiVersion":2,"dynamicCapabilities":true},
      "config":{"type":"object"},"tools":[],
      "stateFields":[{"name":"level","type":"integer"}],
      "sources":[],
      "resources":[
        {"id":"actuator","mode":"exclusive","capacity":1},
        {"id":"stream","mode":"shared","capacity":2}
      ],
      "placement":{"mode":"runtime"},
      "limits":{"maxConcurrentJobs":1,"maxCallsPerMin":60}
    }"#;

    fn resource_declaration() -> DeviceDeclaration {
        serde_json::from_value(serde_json::json!({
            "manifestVersion":"1.0",
            "driver":"resource-demo",
            "driverVersion":"1.0.0",
            "stateFields":[{"name":"level","type":"integer"}],
            "tools":[
                {"name":"fire","kind":"action","parameters":{"type":"object"},
                 "resources":["stream","actuator"]},
                {"name":"set_level","kind":"set","targetField":"level",
                 "parameters":{"type":"object","properties":{"level":{"type":"integer"}}},
                 "resources":["stream"]},
                {"name":"ghost","kind":"action","parameters":{"type":"object"},
                 "resources":["not-declared"]}
            ]
        }))
        .expect("declaration json")
    }

    async fn fixture_with_resource_declaration() -> Fixture {
        let fx = fixture(None);
        fx.service
            .manifests
            .seed_builtin(RESOURCE_MANIFEST)
            .expect("resource manifest must validate");
        let decl = resource_declaration();
        let json = serde_json::to_string(&decl).unwrap();
        fx.service
            .device_store
            .upsert(&DeviceDeclarationRecord {
                workspace_id: "ws1".into(),
                thing_id: "t1".into(),
                driver_name: decl.driver.clone(),
                driver_version: decl.driver_version.clone(),
                declaration_json: json.clone(),
                declaration_hash: "hash".into(),
                status: "active".into(),
                updated_at: "2026-10-22T00:00:00Z".into(),
            })
            .await
            .expect("device store");
        fx
    }

    #[tokio::test]
    async fn resolver_reads_package_resources_for_actions_and_properties() {
        use super::super::resources::{ResourceClaim, ResourceResolver};

        let fx = fixture_with_resource_declaration().await;
        let resolver: &dyn ResourceResolver = &fx.service;

        // 排序 + 声明模式/容量。
        let claims = resolver.resolve_action("ws1", "t1", "fire").await.unwrap();
        assert_eq!(
            claims,
            vec![
                ResourceClaim {
                    id: "actuator".into(),
                    exclusive: true,
                    capacity: 1,
                },
                ResourceClaim {
                    id: "stream".into(),
                    exclusive: false,
                    capacity: 2,
                },
            ]
        );

        // 属性 → kind=set 工具的 resources。
        let claims = resolver
            .resolve_properties("ws1", "t1", &["level".into()])
            .await
            .unwrap();
        assert_eq!(claims.len(), 1);
        assert_eq!(claims[0].id, "stream");
        assert!(!claims[0].exclusive);

        // 未声明的属性 → 合成设备资源（E1）。
        let claims = resolver
            .resolve_properties("ws1", "t1", &["temp".into()])
            .await
            .unwrap();
        assert_eq!(claims[0].id, super::super::resources::DEVICE_RESOURCE);

        // fail-closed：未声明的资源 id / 工具。
        let err = resolver.resolve_action("ws1", "t1", "ghost").await.unwrap_err();
        assert_eq!(err, "unknown resource 'not-declared'");
        let err = resolver.resolve_action("ws1", "t1", "missing").await.unwrap_err();
        assert_eq!(err, "action 'missing' not declared");

        // 无声明设备（set_device 存量路径）→ E1 合成设备资源。
        let undeclared = fixture(None);
        let resolver: &dyn ResourceResolver = &undeclared.service;
        let claims = resolver
            .resolve_properties("ws1", "legacy", &["level".into()])
            .await
            .unwrap();
        assert_eq!(claims, vec![super::super::resources::device_claim()]);
    }

    /// Issue 12：工具未声明 resources 时，合成 `__device__` 的解析不需要包
    /// 清单——包暂不可用（未安装/加载失败）不得让原本受 E1 互斥保护的调用
    /// 失败；真正声明了资源的工具仍 fail-closed。
    #[tokio::test]
    async fn resolve_action_without_package_uses_device_claim_for_empty_resources() {
        use super::super::resources::ResourceResolver;

        let fx = fixture(None);
        // 故意不 seed manifest：包不可用。
        let decl: DeviceDeclaration = serde_json::from_value(serde_json::json!({
            "manifestVersion":"1.0",
            "driver":"resource-demo",
            "driverVersion":"1.0.0",
            "stateFields":[],
            "tools":[
                {"name":"echo","kind":"action","parameters":{"type":"object"},"resources":[]},
                {"name":"fire","kind":"action","parameters":{"type":"object"},
                 "resources":["actuator"]}
            ]
        }))
        .expect("declaration json");
        fx.service
            .device_store
            .upsert(&DeviceDeclarationRecord {
                workspace_id: "ws1".into(),
                thing_id: "t1".into(),
                driver_name: decl.driver.clone(),
                driver_version: decl.driver_version.clone(),
                declaration_json: serde_json::to_string(&decl).unwrap(),
                declaration_hash: "hash".into(),
                status: "active".into(),
                updated_at: "2026-10-22T00:00:00Z".into(),
            })
            .await
            .expect("device store");

        let resolver: &dyn ResourceResolver = &fx.service;
        let claims = resolver.resolve_action("ws1", "t1", "echo").await.unwrap();
        assert_eq!(claims, vec![super::super::resources::device_claim()]);

        // 声明了资源的工具仍要求清单（fail-closed）。
        let err = resolver.resolve_action("ws1", "t1", "fire").await.unwrap_err();
        assert!(err.contains("manifest for driver"), "{err}");
    }
}
