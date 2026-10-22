//! E4d `ZoneService`：maps 权威下的 zone 解析（读取侧）。
//!
//! 语义见 `docs/superpowers/specs/2026-10-22-e4d-spatial-model-design.md` §2.2：
//! - `zone_source='maps'` 时才应由调用方（LiveState）使用；
//! - 位置缺失 / trust=Lost → `None`（不回退层级）；
//! - 图 = `position.map_id`（未知图 fail-closed）或 workspace 默认图；
//! - 显式 `zoneRef` 优先，未知引用 → `None`（fail-closed）；
//! - 多 zone 命中取面积最小（`zone::resolve_zone_for`）。
//!
//! maps/places 与**坐标**分别按工作区/物短 TTL 缓存（默认 5s），避免每次摘要
//! 都查库；快照先用 `list_positions` 批量预热，N 个物不再产生 N 次串行查询。
//! 测试可经 [`ZoneService::with_ttl`]（maps = position = 同一 TTL）或
//! [`ZoneService::with_ttls`] 注入零 TTL 关闭缓存。runtime 仍零 db 依赖——
//! 持久化只经 [`MapStore`] 端口。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use parking_lot::Mutex;

use super::model::{LocationTrust, MapRecord, PlaceKind, PlaceRecord, PlacementMode, ThingPosition, TrustThresholds};
use super::zone::{derive_trust, resolve_zone_for, within_bounds};
use crate::ports::MapStore;

/// maps/places 缓存默认 TTL（spec §1：小缓存 + 失效；CRUD 写穿在 Task 5）。
pub const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(5);

/// 坐标缓存默认 TTL：位置读数高频，短窗口内复用；写入时同步更新而非失效。
pub const DEFAULT_POSITION_CACHE_TTL: Duration = Duration::from_secs(5);

/// 单工作区缓存：zone_source / maps 懒加载，places 按 map 懒加载。
struct WorkspaceCache {
    loaded_at: Instant,
    zone_source: String,
    maps: Option<Vec<MapRecord>>,
    places: HashMap<String, Vec<PlaceRecord>>,
}

/// 单物坐标 + 显式 zone 引用缓存。
#[derive(Clone)]
struct CachedLocation {
    fetched_at: Instant,
    position: Option<ThingPosition>,
    zone_ref: Option<String>,
    /// `false` = 批量预热只拿到坐标，zone_ref 尚未加载（首次访问补一次查询）。
    zone_ref_loaded: bool,
    /// 显式引用的解析结果（快照预热时批量求解；`None` = 尚未解析）。
    resolved_zone: Option<Option<String>>,
}

/// [`ZoneService::location`] 的只读快照。
struct LocationSnapshot {
    position: Option<ThingPosition>,
    zone_ref: Option<String>,
    resolved_zone: Option<Option<String>>,
}

/// zone 读取服务：maps/places/position → zone 名称。
pub struct ZoneService {
    store: Arc<dyn MapStore>,
    thresholds: TrustThresholds,
    ttl: Duration,
    position_ttl: Duration,
    cache: Mutex<HashMap<String, WorkspaceCache>>,
    positions: Mutex<HashMap<(String, String), CachedLocation>>,
}

impl ZoneService {
    pub fn new(store: Arc<dyn MapStore>, thresholds: TrustThresholds) -> Self {
        Self::with_ttls(store, thresholds, DEFAULT_CACHE_TTL, DEFAULT_POSITION_CACHE_TTL)
    }

    /// 指定缓存 TTL（`Duration::ZERO` = 每次调用都回源，测试用）。
    pub fn with_ttl(store: Arc<dyn MapStore>, thresholds: TrustThresholds, ttl: Duration) -> Self {
        Self::with_ttls(store, thresholds, ttl, ttl)
    }

    /// maps/places 与坐标使用不同 TTL。
    pub fn with_ttls(
        store: Arc<dyn MapStore>,
        thresholds: TrustThresholds,
        ttl: Duration,
        position_ttl: Duration,
    ) -> Self {
        Self {
            store,
            thresholds,
            ttl,
            position_ttl,
            cache: Mutex::new(HashMap::new()),
            positions: Mutex::new(HashMap::new()),
        }
    }

    /// trust 阈值（Task 4/6 由同一服务做 age 派生时复用）。
    pub fn thresholds(&self) -> TrustThresholds {
        self.thresholds
    }

    /// 工作区 zone 权威：`hierarchy` / `maps`；未知/缺失归一化为 `hierarchy`
    /// （迁移期回退）。
    pub async fn zone_source(&self, workspace_id: &str) -> Result<String, String> {
        self.ensure_base(workspace_id).await?;
        Ok(self
            .cache
            .lock()
            .get(workspace_id)
            .map(|entry| entry.zone_source.clone())
            .unwrap_or_else(|| "hierarchy".to_string()))
    }

    /// maps 权威下的 zone 名称。`None` = 无位置 / trust Lost / 位置越界 /
    /// 无默认图 / 未知显式 zoneRef（fail-closed，不回退层级）。
    ///
    /// 显式 `zoneRef`（参数或物化列）在工作区内按 `kind=Zone` 独立解析，
    /// **不依赖坐标**；未知引用返回 `None`。
    pub async fn zone_for(
        &self,
        workspace_id: &str,
        thing_id: &str,
        explicit_zone: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Option<String>, String> {
        let location = self.location(workspace_id, thing_id).await?;

        // 1. 显式引用优先（placement/物化列），与坐标无关。
        //    参数引用（来自调用方）直接解析；物化列结果可命中预热缓存。
        if let Some(name) = explicit_zone {
            return self.resolve_zone_ref(workspace_id, name).await;
        }
        if let Some(name) = location.zone_ref.as_deref() {
            if let Some(resolved) = location.resolved_zone.clone() {
                return Ok(resolved);
            }
            let resolved = self.resolve_zone_ref(workspace_id, name).await?;
            if self.position_ttl > Duration::ZERO
                && let Some(entry) = self
                    .positions
                    .lock()
                    .get_mut(&(workspace_id.to_string(), thing_id.to_string()))
            {
                entry.resolved_zone = Some(resolved.clone());
            }
            return Ok(resolved);
        }

        // 2. 点选需要坐标。
        let Some(position) = location.position else {
            return Ok(None);
        };
        let (Some(x), Some(y)) = (position.x, position.y) else {
            return Ok(None);
        };

        // 3. 读时 trust 派生：updated_at 缺失 = 静态位置（fixed/declared，
        //    恒 Trusted）；否则按 age 降级，驱动上报只能降级。
        let trust = effective_trust(&position, now, self.thresholds);
        if trust == LocationTrust::Lost {
            return Ok(None);
        }

        let maps = self.maps(workspace_id).await?;
        let Some(map) = position_map(&maps, &position) else {
            // 显式 map 未知 / 无默认图 → fail-closed。
            return Ok(None);
        };
        if !within_bounds(&map.bounds, x, y) {
            tracing::warn!(
                workspace_id,
                thing_id,
                map_id = %map.id,
                x,
                y,
                "位置越界：保留坐标，zone=None"
            );
            return Ok(None);
        }

        let places = self.places(workspace_id, &map.id).await?;
        Ok(resolve_zone_for(&places, None, Some((x, y))).map(|place| place.name.clone()))
    }

    /// 位置是否越出所属地图边界；`None` = 无坐标或无可用地图（无法判定）。
    pub async fn position_out_of_bounds(
        &self,
        workspace_id: &str,
        position: &ThingPosition,
    ) -> Result<Option<bool>, String> {
        let (Some(x), Some(y)) = (position.x, position.y) else {
            return Ok(None);
        };
        let maps = self.maps(workspace_id).await?;
        let Some(map) = position_map(&maps, position) else {
            return Ok(None);
        };
        Ok(Some(!within_bounds(&map.bounds, x, y)))
    }

    /// 快照批量预热：maps/places/坐标/显式 zone 引用各一次查询，
    /// 随后 N 个 `zone_for` 全部命中缓存（120ms 预算；spec §2.2 carry-forward）。
    pub async fn warm_workspace(&self, workspace_id: &str) -> Result<(), String> {
        if self.zone_source(workspace_id).await? != "maps" {
            return Ok(());
        }
        let maps = self.maps(workspace_id).await?;
        for map in &maps {
            self.places(workspace_id, &map.id).await?;
        }
        if self.position_ttl == Duration::ZERO {
            return Ok(());
        }

        // 名称 → zone 名（workspace 全量，取确定性首项）。预热已加载所有
        // 图的 places，显式引用在这里一次解析，快照循环不再逐物查库。
        let mut zone_names: HashMap<String, String> = HashMap::new();
        for map in &maps {
            for place in self.places(workspace_id, &map.id).await? {
                if place.kind == PlaceKind::Zone {
                    zone_names
                        .entry(place.name.clone())
                        .or_insert_with(|| place.name.clone());
                }
            }
        }

        let positions = self.store.list_positions(workspace_id).await?;
        let zone_refs: HashMap<String, String> = self.store.list_zone_refs(workspace_id).await?.into_iter().collect();
        let now = Instant::now();
        let mut cache = self.positions.lock();
        // 清掉本工作区过期项，避免缓存无界增长。
        cache.retain(|key, entry| key.0 != workspace_id || entry.fetched_at.elapsed() < self.position_ttl);
        for (thing_id, position) in positions {
            cache.insert(
                (workspace_id.to_string(), thing_id),
                CachedLocation {
                    fetched_at: now,
                    position: Some(position),
                    zone_ref: None,
                    // 批量同时拿到全部非空 zone_ref；其余物确定为空。
                    zone_ref_loaded: true,
                    resolved_zone: None,
                },
            );
        }
        for (thing_id, zone_ref) in zone_refs {
            if let Some(entry) = cache.get_mut(&(workspace_id.to_string(), thing_id)) {
                entry.resolved_zone = Some(zone_names.get(&zone_ref).cloned());
                entry.zone_ref = Some(zone_ref);
            }
        }
        Ok(())
    }

    /// 显式 `zoneRef` 工作区内解析（`kind=Zone`；未知 → `None`，fail-closed）。
    async fn resolve_zone_ref(&self, workspace_id: &str, name: &str) -> Result<Option<String>, String> {
        Ok(self
            .store
            .find_place_by_name(workspace_id, name, Some(PlaceKind::Zone))
            .await?
            .map(|place| place.name))
    }

    /// placement/API 写位置（含 map_id）；写后同步刷新缓存。
    pub async fn set_position(
        &self,
        workspace_id: &str,
        thing_id: &str,
        position: &ThingPosition,
    ) -> Result<(), String> {
        self.store.set_position(workspace_id, thing_id, position).await?;
        self.cache_written_position(workspace_id, thing_id, position);
        Ok(())
    }

    /// 驱动读数写位置（保留 map_id/zone_ref）；写后同步刷新缓存。
    pub async fn set_reading_position(
        &self,
        workspace_id: &str,
        thing_id: &str,
        position: &ThingPosition,
    ) -> Result<(), String> {
        self.store
            .set_reading_position(workspace_id, thing_id, position)
            .await?;
        self.cache_written_position(workspace_id, thing_id, position);
        Ok(())
    }

    /// 写物化显式 zone 引用；写后同步刷新缓存。
    pub async fn set_zone_ref(&self, workspace_id: &str, thing_id: &str, zone_ref: Option<&str>) -> Result<(), String> {
        self.store.set_zone_ref(workspace_id, thing_id, zone_ref).await?;
        if self.position_ttl == Duration::ZERO {
            return Ok(());
        }
        if let Some(entry) = self
            .positions
            .lock()
            .get_mut(&(workspace_id.to_string(), thing_id.to_string()))
        {
            entry.zone_ref = zone_ref.map(str::to_string);
            entry.zone_ref_loaded = true;
            entry.resolved_zone = None;
        }
        Ok(())
    }

    /// 读取坐标 + 显式 zone 引用：命中短 TTL 缓存则不回源。
    async fn location(&self, workspace_id: &str, thing_id: &str) -> Result<LocationSnapshot, String> {
        let key = (workspace_id.to_string(), thing_id.to_string());
        if self.position_ttl > Duration::ZERO {
            let cached = self.positions.lock().get(&key).cloned();
            if let Some(entry) = cached
                && entry.fetched_at.elapsed() < self.position_ttl
            {
                if entry.zone_ref_loaded {
                    return Ok(LocationSnapshot {
                        position: entry.position,
                        zone_ref: entry.zone_ref,
                        resolved_zone: entry.resolved_zone,
                    });
                }
                let zone_ref = self.store.get_zone_ref(workspace_id, thing_id).await?;
                if let Some(slot) = self.positions.lock().get_mut(&key) {
                    slot.zone_ref = zone_ref.clone();
                    slot.zone_ref_loaded = true;
                    slot.resolved_zone = None;
                }
                return Ok(LocationSnapshot {
                    position: entry.position,
                    zone_ref,
                    resolved_zone: None,
                });
            }
        }

        let position = self.store.get_position(workspace_id, thing_id).await?;
        let zone_ref = self.store.get_zone_ref(workspace_id, thing_id).await?;
        if self.position_ttl > Duration::ZERO {
            self.positions.lock().insert(
                key,
                CachedLocation {
                    fetched_at: Instant::now(),
                    position: position.clone(),
                    zone_ref: zone_ref.clone(),
                    zone_ref_loaded: true,
                    resolved_zone: None,
                },
            );
        }
        Ok(LocationSnapshot {
            position,
            zone_ref,
            resolved_zone: None,
        })
    }

    fn cache_written_position(&self, workspace_id: &str, thing_id: &str, position: &ThingPosition) {
        if self.position_ttl == Duration::ZERO {
            return;
        }
        let key = (workspace_id.to_string(), thing_id.to_string());
        let mut cache = self.positions.lock();
        match cache.get_mut(&key) {
            Some(entry) => {
                entry.position = Some(position.clone());
                entry.fetched_at = Instant::now();
            }
            None => {
                cache.insert(
                    key,
                    CachedLocation {
                        fetched_at: Instant::now(),
                        position: Some(position.clone()),
                        zone_ref: None,
                        zone_ref_loaded: false,
                        resolved_zone: None,
                    },
                );
            }
        }
    }

    /// 确保工作区缓存新鲜；TTL 过期整条丢弃（maps/places 一并回源）。
    async fn ensure_base(&self, workspace_id: &str) -> Result<(), String> {
        let stale = {
            let mut cache = self.cache.lock();
            match cache.get(workspace_id) {
                Some(entry) if entry.loaded_at.elapsed() < self.ttl => false,
                _ => {
                    cache.remove(workspace_id);
                    true
                }
            }
        };
        if !stale {
            return Ok(());
        }
        let zone_source = normalize_zone_source(&self.store.zone_source(workspace_id).await?);
        self.cache.lock().insert(
            workspace_id.to_string(),
            WorkspaceCache {
                loaded_at: Instant::now(),
                zone_source,
                maps: None,
                places: HashMap::new(),
            },
        );
        Ok(())
    }

    async fn maps(&self, workspace_id: &str) -> Result<Vec<MapRecord>, String> {
        self.ensure_base(workspace_id).await?;
        if let Some(maps) = self.cache.lock().get(workspace_id).and_then(|entry| entry.maps.clone()) {
            return Ok(maps);
        }
        let maps = self.store.list_maps(workspace_id).await?;
        if let Some(entry) = self.cache.lock().get_mut(workspace_id) {
            entry.maps = Some(maps.clone());
        }
        Ok(maps)
    }

    async fn places(&self, workspace_id: &str, map_id: &str) -> Result<Vec<PlaceRecord>, String> {
        self.ensure_base(workspace_id).await?;
        if let Some(places) = self
            .cache
            .lock()
            .get(workspace_id)
            .and_then(|entry| entry.places.get(map_id).cloned())
        {
            return Ok(places);
        }
        let places = self.store.list_places(workspace_id, map_id).await?;
        if let Some(entry) = self.cache.lock().get_mut(workspace_id) {
            entry.places.insert(map_id.to_string(), places.clone());
        }
        Ok(places)
    }
}

/// 未知值按迁移期回退处理（与 `MapStore::zone_source` 端口文档一致）。
fn normalize_zone_source(raw: &str) -> String {
    match raw {
        "maps" => "maps".to_string(),
        _ => "hierarchy".to_string(),
    }
}

/// 读时 trust：`updated_at` 缺失 = 静态位置（fixed/declared）恒 Trusted；
/// 否则按 age 阈值派生，驱动上报只能降级（spec §2.2）。
fn effective_trust(position: &ThingPosition, now: DateTime<Utc>, thresholds: TrustThresholds) -> LocationTrust {
    match position.updated_at.as_deref() {
        None => LocationTrust::Trusted,
        Some(_) => derive_trust(
            PlacementMode::Runtime,
            position.updated_at.as_deref(),
            Some(position.trust),
            now,
            thresholds,
        ),
    }
}

/// 坐标所属地图：显式 `map_id`（未知 → None，fail-closed）或工作区默认图。
fn position_map<'a>(maps: &'a [MapRecord], position: &ThingPosition) -> Option<&'a MapRecord> {
    match position.map_id.as_deref() {
        Some(id) => maps.iter().find(|map| map.id == id),
        None => maps.iter().find(|map| map.is_default),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use std::sync::atomic::AtomicBool;

    use async_trait::async_trait;
    use chrono::TimeZone;

    use crate::maps::model::{Geometry, MapBounds, PlaceKind, ThingPosition};

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 22, 12, 0, 0).unwrap()
    }

    fn map_rec(id: &str, ws: &str, is_default: bool) -> MapRecord {
        MapRecord {
            id: id.into(),
            workspace_id: ws.into(),
            name: id.into(),
            bounds: MapBounds {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 100.0,
            },
            anchor_map_id: None,
            is_default,
            created_at: "2026-10-22T00:00:00Z".into(),
            updated_at: "2026-10-22T00:00:00Z".into(),
        }
    }

    fn place(id: &str, map_id: &str, name: &str, kind: PlaceKind, points: &[(f64, f64)]) -> PlaceRecord {
        PlaceRecord {
            id: id.into(),
            map_id: map_id.into(),
            name: name.into(),
            kind,
            geometry: Geometry::Polygon {
                points: points.to_vec(),
            },
            heading: None,
            created_at: "2026-10-22T00:00:00Z".into(),
            updated_at: "2026-10-22T00:00:00Z".into(),
        }
    }

    fn zone(id: &str, map_id: &str, name: &str, points: &[(f64, f64)]) -> PlaceRecord {
        place(id, map_id, name, PlaceKind::Zone, points)
    }

    fn square(size: f64) -> Vec<(f64, f64)> {
        vec![(0.0, 0.0), (size, 0.0), (size, size), (0.0, size)]
    }

    fn position(x: f64, y: f64) -> ThingPosition {
        ThingPosition {
            map_id: None,
            x: Some(x),
            y: Some(y),
            level: None,
            heading: None,
            trust: LocationTrust::Trusted,
            updated_at: Some("2026-10-22T12:00:00Z".into()),
        }
    }

    /// 可计数/可切换的 MapStore 桩。
    struct StubMapStore {
        source: Mutex<String>,
        maps: Vec<MapRecord>,
        places: Vec<PlaceRecord>,
        positions: HashMap<String, ThingPosition>,
        zone_refs: HashMap<String, String>,
        zone_source_calls: AtomicUsize,
        list_places_calls: AtomicUsize,
        get_position_calls: AtomicUsize,
        list_positions_calls: AtomicUsize,
        find_place_calls: AtomicUsize,
        fail_list_positions: AtomicBool,
    }

    impl StubMapStore {
        fn new(source: &str, maps: Vec<MapRecord>, places: Vec<PlaceRecord>) -> Self {
            Self {
                source: Mutex::new(source.into()),
                maps,
                places,
                positions: HashMap::new(),
                zone_refs: HashMap::new(),
                zone_source_calls: AtomicUsize::new(0),
                list_places_calls: AtomicUsize::new(0),
                get_position_calls: AtomicUsize::new(0),
                list_positions_calls: AtomicUsize::new(0),
                find_place_calls: AtomicUsize::new(0),
                fail_list_positions: AtomicBool::new(false),
            }
        }

        fn with_position(mut self, thing_id: &str, position: ThingPosition) -> Self {
            self.positions.insert(thing_id.into(), position);
            self
        }

        fn with_zone_ref(mut self, thing_id: &str, zone_ref: &str) -> Self {
            self.zone_refs.insert(thing_id.into(), zone_ref.into());
            self
        }
    }

    #[async_trait]
    impl MapStore for StubMapStore {
        async fn list_maps(&self, _workspace_id: &str) -> Result<Vec<MapRecord>, String> {
            Ok(self.maps.clone())
        }

        async fn default_map(&self, _workspace_id: &str) -> Result<Option<MapRecord>, String> {
            Ok(self.maps.iter().find(|map| map.is_default).cloned())
        }

        async fn upsert_map(&self, _map: &MapRecord) -> Result<(), String> {
            Ok(())
        }

        async fn delete_map(&self, _workspace_id: &str, _id: &str) -> Result<bool, String> {
            Ok(false)
        }

        async fn list_places(&self, _workspace_id: &str, map_id: &str) -> Result<Vec<PlaceRecord>, String> {
            self.list_places_calls.fetch_add(1, Ordering::Relaxed);
            Ok(self
                .places
                .iter()
                .filter(|place| place.map_id == map_id)
                .cloned()
                .collect())
        }

        async fn find_place_by_name(
            &self,
            _workspace_id: &str,
            name: &str,
            kind: Option<PlaceKind>,
        ) -> Result<Option<PlaceRecord>, String> {
            self.find_place_calls.fetch_add(1, Ordering::Relaxed);
            Ok(self
                .places
                .iter()
                .find(|place| place.name == name && kind.is_none_or(|k| place.kind == k))
                .cloned())
        }

        async fn upsert_place(&self, _workspace_id: &str, _place: &PlaceRecord) -> Result<(), String> {
            Ok(())
        }

        async fn delete_place(&self, _workspace_id: &str, _map_id: &str, _id: &str) -> Result<bool, String> {
            Ok(false)
        }

        async fn get_position(&self, _workspace_id: &str, thing_id: &str) -> Result<Option<ThingPosition>, String> {
            self.get_position_calls.fetch_add(1, Ordering::Relaxed);
            Ok(self.positions.get(thing_id).cloned())
        }

        async fn set_position(
            &self,
            _workspace_id: &str,
            _thing_id: &str,
            _position: &ThingPosition,
        ) -> Result<(), String> {
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
            self.list_positions_calls.fetch_add(1, Ordering::Relaxed);
            if self.fail_list_positions.load(Ordering::Relaxed) {
                return Err("db down".into());
            }
            Ok(self
                .positions
                .iter()
                .map(|(thing_id, position)| (thing_id.clone(), position.clone()))
                .collect())
        }

        async fn get_zone_ref(&self, _workspace_id: &str, thing_id: &str) -> Result<Option<String>, String> {
            Ok(self.zone_refs.get(thing_id).cloned())
        }

        async fn set_zone_ref(
            &self,
            _workspace_id: &str,
            _thing_id: &str,
            _zone_ref: Option<&str>,
        ) -> Result<(), String> {
            Ok(())
        }

        async fn list_zone_refs(&self, _workspace_id: &str) -> Result<Vec<(String, String)>, String> {
            Ok(self
                .zone_refs
                .iter()
                .map(|(thing_id, zone_ref)| (thing_id.clone(), zone_ref.clone()))
                .collect())
        }

        async fn zone_source(&self, _workspace_id: &str) -> Result<String, String> {
            self.zone_source_calls.fetch_add(1, Ordering::Relaxed);
            Ok(self.source.lock().clone())
        }
    }

    fn zone_service(store: StubMapStore) -> ZoneService {
        ZoneService::new(Arc::new(store), TrustThresholds::default())
    }

    async fn zone_for(service: &ZoneService, thing_id: &str, explicit: Option<&str>) -> Option<String> {
        service.zone_for("ws1", thing_id, explicit, now()).await.unwrap()
    }

    #[tokio::test]
    async fn maps_mode_resolves_polygon_zone_name() {
        let store = StubMapStore::new(
            "maps",
            vec![map_rec("m1", "ws1", true)],
            vec![zone("z1", "m1", "车间A", &square(10.0))],
        )
        .with_position("t1", position(1.0, 1.0));
        let service = zone_service(store);

        assert_eq!(service.zone_source("ws1").await.unwrap(), "maps");
        assert_eq!(zone_for(&service, "t1", None).await.as_deref(), Some("车间A"));
    }

    #[tokio::test]
    async fn smallest_zone_wins_when_polygons_overlap() {
        let store = StubMapStore::new(
            "maps",
            vec![map_rec("m1", "ws1", true)],
            vec![
                zone("big", "m1", "厂房", &square(10.0)),
                zone("small", "m1", "机房", &square(2.0)),
            ],
        )
        .with_position("t1", position(1.0, 1.0));
        let service = zone_service(store);

        assert_eq!(zone_for(&service, "t1", None).await.as_deref(), Some("机房"));
    }

    #[tokio::test]
    async fn explicit_zone_ref_wins_over_point() {
        let store = StubMapStore::new(
            "maps",
            vec![map_rec("m1", "ws1", true)],
            vec![
                zone("big", "m1", "厂房", &square(10.0)),
                zone("small", "m1", "机房", &square(2.0)),
            ],
        )
        .with_position("t1", position(1.0, 1.0));
        let service = zone_service(store);

        assert_eq!(
            zone_for(&service, "t1", Some("厂房")).await.as_deref(),
            Some("厂房"),
            "显式 zoneRef 优先于点选"
        );
    }

    #[tokio::test]
    async fn unknown_explicit_zone_ref_fails_closed() {
        let store = StubMapStore::new(
            "maps",
            vec![map_rec("m1", "ws1", true)],
            vec![zone("small", "m1", "机房", &square(2.0))],
        )
        .with_position("t1", position(1.0, 1.0));
        let service = zone_service(store);

        assert_eq!(zone_for(&service, "t1", Some("不存在")).await, None);
    }

    #[tokio::test]
    async fn lost_trust_yields_none() {
        let mut lost = position(1.0, 1.0);
        lost.trust = LocationTrust::Lost;
        let store = StubMapStore::new(
            "maps",
            vec![map_rec("m1", "ws1", true)],
            vec![zone("z1", "m1", "车间A", &square(10.0))],
        )
        .with_position("t1", lost);
        let service = zone_service(store);

        assert_eq!(zone_for(&service, "t1", None).await, None);
    }

    #[tokio::test]
    async fn missing_position_or_default_map_yields_none() {
        // 无位置
        let store = StubMapStore::new(
            "maps",
            vec![map_rec("m1", "ws1", true)],
            vec![zone("z1", "m1", "车间A", &square(10.0))],
        );
        let service = zone_service(store);
        assert_eq!(zone_for(&service, "t1", None).await, None);

        // 有位置但没有默认图（position.map_id 也未指定）
        let store = StubMapStore::new(
            "maps",
            vec![map_rec("m1", "ws1", false)],
            vec![zone("z1", "m1", "车间A", &square(10.0))],
        )
        .with_position("t1", position(1.0, 1.0));
        let service = zone_service(store);
        assert_eq!(zone_for(&service, "t1", None).await, None);
    }

    #[tokio::test]
    async fn explicit_position_map_wins_over_default() {
        let mut on_m2 = position(1.0, 1.0);
        on_m2.map_id = Some("m2".into());
        let store = StubMapStore::new(
            "maps",
            vec![map_rec("m1", "ws1", true), map_rec("m2", "ws1", false)],
            vec![
                zone("z1", "m1", "默认图区域", &square(10.0)),
                zone("z2", "m2", "二层区域", &square(10.0)),
            ],
        )
        .with_position("t1", on_m2);
        let service = zone_service(store);
        assert_eq!(zone_for(&service, "t1", None).await.as_deref(), Some("二层区域"));
    }

    #[tokio::test]
    async fn unknown_position_map_fails_closed() {
        let mut unknown = position(1.0, 1.0);
        unknown.map_id = Some("missing".into());
        let store = StubMapStore::new(
            "maps",
            vec![map_rec("m1", "ws1", true)],
            vec![zone("z1", "m1", "车间A", &square(10.0))],
        )
        .with_position("t1", unknown);
        let service = zone_service(store);
        assert_eq!(zone_for(&service, "t1", None).await, None);
    }

    #[tokio::test]
    async fn ttl_cache_reuses_maps_and_places_until_expiry() {
        let store = StubMapStore::new(
            "maps",
            vec![map_rec("m1", "ws1", true)],
            vec![zone("z1", "m1", "车间A", &square(10.0))],
        )
        .with_position("t1", position(1.0, 1.0));
        let service = ZoneService::with_ttl(Arc::new(store), TrustThresholds::default(), Duration::from_secs(60));

        assert_eq!(zone_for(&service, "t1", None).await.as_deref(), Some("车间A"));
        assert_eq!(zone_for(&service, "t1", None).await.as_deref(), Some("车间A"));
        assert_eq!(service.zone_source("ws1").await.unwrap(), "maps");
        let cache = service.cache.lock();
        assert_eq!(cache.get("ws1").unwrap().places.len(), 1, "places 命中缓存");
        drop(cache);
    }

    #[tokio::test]
    async fn zero_ttl_disables_cache() {
        let store = Arc::new(
            StubMapStore::new(
                "maps",
                vec![map_rec("m1", "ws1", true)],
                vec![zone("z1", "m1", "车间A", &square(10.0))],
            )
            .with_position("t1", position(1.0, 1.0)),
        );
        let service = ZoneService::with_ttl(store.clone(), TrustThresholds::default(), Duration::ZERO);

        zone_for(&service, "t1", None).await.unwrap();
        zone_for(&service, "t1", None).await.unwrap();
        assert_eq!(store.list_places_calls.load(Ordering::Relaxed), 2);
        // 每个 zone_for 依次确保 maps/places accessor 的 base 新鲜：TTL=0 时
        // 两次 accessor 各回源一次（共 2 次/调用）。
        assert_eq!(store.zone_source_calls.load(Ordering::Relaxed), 4);
    }

    #[tokio::test]
    async fn hierarchy_and_unknown_sources_normalize_to_hierarchy() {
        let store = StubMapStore::new("hierarchy", vec![], vec![]);
        let service = zone_service(store);
        assert_eq!(service.zone_source("ws1").await.unwrap(), "hierarchy");

        let store = StubMapStore::new("bogus", vec![], vec![]);
        let service = zone_service(store);
        assert_eq!(service.zone_source("ws1").await.unwrap(), "hierarchy");
    }

    // ── Task 4：读时 aging / zoneRef / 缓存 / 越界 ───────────────────

    fn timestamp_ago(seconds: i64) -> String {
        (now() - chrono::Duration::seconds(seconds)).to_rfc3339()
    }

    fn aged_position(x: f64, y: f64, age_secs: i64, trust: LocationTrust) -> ThingPosition {
        ThingPosition {
            updated_at: Some(timestamp_ago(age_secs)),
            trust,
            ..position(x, y)
        }
    }

    /// 停止上报的设备：坐标列仍在，但 age > 5min → Lost → zone None
    ///（不能靠库里的旧 trust 永远占着 zone）。
    #[tokio::test]
    async fn stale_position_ages_to_lost_and_yields_no_zone() {
        let store = StubMapStore::new(
            "maps",
            vec![map_rec("m1", "ws1", true)],
            vec![zone("z1", "m1", "车间A", &square(10.0))],
        )
        .with_position("t1", aged_position(1.0, 1.0, 3600, LocationTrust::Trusted));
        let service = ZoneService::with_ttl(Arc::new(store), TrustThresholds::default(), Duration::ZERO);
        assert_eq!(zone_for(&service, "t1", None).await, None);

        // 1 分钟：Uncertain 仍可用于 zone 解析
        let store = StubMapStore::new(
            "maps",
            vec![map_rec("m1", "ws1", true)],
            vec![zone("z1", "m1", "车间A", &square(10.0))],
        )
        .with_position("t1", aged_position(1.0, 1.0, 60, LocationTrust::Trusted));
        let service = ZoneService::with_ttl(Arc::new(store), TrustThresholds::default(), Duration::ZERO);
        assert_eq!(zone_for(&service, "t1", None).await.as_deref(), Some("车间A"));

        // 驱动 trust 只能降级：新鲜坐标 + 上报 Lost → None
        let store = StubMapStore::new(
            "maps",
            vec![map_rec("m1", "ws1", true)],
            vec![zone("z1", "m1", "车间A", &square(10.0))],
        )
        .with_position("t1", aged_position(1.0, 1.0, 0, LocationTrust::Lost));
        let service = ZoneService::with_ttl(Arc::new(store), TrustThresholds::default(), Duration::ZERO);
        assert_eq!(zone_for(&service, "t1", None).await, None);
    }

    /// 无 updated_at = 静态位置（fixed/declared），永不因 age 失效。
    #[tokio::test]
    async fn position_without_timestamp_is_trusted() {
        let mut static_pos = position(1.0, 1.0);
        static_pos.updated_at = None;
        static_pos.trust = LocationTrust::Lost; // 驱动上报不能降级静态位置
        let store = StubMapStore::new(
            "maps",
            vec![map_rec("m1", "ws1", true)],
            vec![zone("z1", "m1", "车间A", &square(10.0))],
        )
        .with_position("t1", static_pos);
        let service = ZoneService::with_ttl(Arc::new(store), TrustThresholds::default(), Duration::ZERO);
        assert_eq!(zone_for(&service, "t1", None).await.as_deref(), Some("车间A"));
    }

    /// 显式 zoneRef 物化列：无坐标也能解析；未知引用 fail-closed。
    #[tokio::test]
    async fn stored_zone_ref_resolves_without_position() {
        let store = StubMapStore::new(
            "maps",
            vec![map_rec("m1", "ws1", true)],
            vec![
                zone("z1", "m1", "机房", &square(2.0)),
                zone("z2", "m1", "厂房", &square(10.0)),
            ],
        )
        .with_zone_ref("t1", "厂房")
        .with_zone_ref("t2", "不存在");
        let service = ZoneService::with_ttl(Arc::new(store), TrustThresholds::default(), Duration::ZERO);

        assert_eq!(zone_for(&service, "t1", None).await.as_deref(), Some("厂房"));
        assert_eq!(zone_for(&service, "t2", None).await, None);
        // 参数显式引用仍优先于物化列
        assert_eq!(zone_for(&service, "t1", Some("机房")).await.as_deref(), Some("机房"));
    }

    /// 快照预热：list_positions 一次取全量，N 个 zone_for 不再逐物查询。
    #[tokio::test]
    async fn warm_workspace_batches_positions_for_many_things() {
        let store = Arc::new(
            StubMapStore::new(
                "maps",
                vec![map_rec("m1", "ws1", true)],
                vec![
                    zone("z1", "m1", "车间A", &square(2.0)),
                    zone("z2", "m1", "车间B", &square(4.0)),
                ],
            )
            .with_position("t1", position(1.0, 1.0))
            .with_position("t2", position(3.0, 3.0))
            .with_position("t3", position(0.0, 0.0))
            .with_zone_ref("t3", "车间B"),
        );
        let service = ZoneService::with_ttls(
            store.clone(),
            TrustThresholds::default(),
            Duration::from_secs(60),
            Duration::from_secs(60),
        );

        service.warm_workspace("ws1").await.unwrap();
        assert_eq!(store.list_positions_calls.load(Ordering::Relaxed), 1);
        assert_eq!(zone_for(&service, "t1", None).await.as_deref(), Some("车间A"));
        assert_eq!(zone_for(&service, "t2", None).await.as_deref(), Some("车间B"));
        assert_eq!(
            zone_for(&service, "t3", None).await.as_deref(),
            Some("车间B"),
            "物化显式 zoneRef 优先于点选"
        );
        assert_eq!(
            store.get_position_calls.load(Ordering::Relaxed),
            0,
            "预热后不得逐物回源"
        );
        assert_eq!(
            store.find_place_calls.load(Ordering::Relaxed),
            0,
            "显式 zoneRef 预热时批量解析，快照循环不再逐物查库"
        );
        assert_eq!(
            store.list_places_calls.load(Ordering::Relaxed),
            1,
            "地点按图缓存，不随物数量增长"
        );
    }

    /// 越界：保留坐标但 zone=None（即使 zone 多边形覆盖该点）。
    #[tokio::test]
    async fn out_of_bounds_position_yields_no_zone() {
        let store = StubMapStore::new(
            "maps",
            vec![map_rec("m1", "ws1", true)], // bounds 0..100
            vec![zone("z1", "m1", "超大区", &square(200.0))],
        )
        .with_position("t1", position(150.0, 50.0));
        let service = ZoneService::with_ttl(Arc::new(store), TrustThresholds::default(), Duration::ZERO);

        assert_eq!(zone_for(&service, "t1", None).await, None);
        let pos = position(150.0, 50.0);
        assert_eq!(service.position_out_of_bounds("ws1", &pos).await.unwrap(), Some(true));
        assert_eq!(
            service
                .position_out_of_bounds("ws1", &position(50.0, 50.0))
                .await
                .unwrap(),
            Some(false)
        );
    }
}
