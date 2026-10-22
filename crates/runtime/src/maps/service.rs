//! E4d `ZoneService`：maps 权威下的 zone 解析（读取侧）。
//!
//! 语义见 `docs/superpowers/specs/2026-10-22-e4d-spatial-model-design.md` §2.2：
//! - `zone_source='maps'` 时才应由调用方（LiveState）使用；
//! - 位置缺失 / trust=Lost → `None`（不回退层级）；
//! - 图 = `position.map_id`（未知图 fail-closed）或 workspace 默认图；
//! - 显式 `zoneRef` 优先，未知引用 → `None`（fail-closed）；
//! - 多 zone 命中取面积最小（`zone::resolve_zone_for`）。
//!
//! maps/places 按工作区短 TTL 缓存（默认 5s），避免每次摘要都查库；测试可经
//! [`ZoneService::with_ttl`] 注入零 TTL 关闭缓存。runtime 仍零 db 依赖——持久化
//! 只经 [`MapStore`] 端口。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use parking_lot::Mutex;

use super::model::{LocationTrust, MapRecord, PlaceRecord, TrustThresholds};
use super::zone::resolve_zone_for;
use crate::ports::MapStore;

/// maps/places 缓存默认 TTL（spec §1：小缓存 + 失效；CRUD 写穿在 Task 5）。
pub const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(5);

/// 单工作区缓存：zone_source / maps 懒加载，places 按 map 懒加载。
struct WorkspaceCache {
    loaded_at: Instant,
    zone_source: String,
    maps: Option<Vec<MapRecord>>,
    places: HashMap<String, Vec<PlaceRecord>>,
}

/// zone 读取服务：maps/places/position → zone 名称。
pub struct ZoneService {
    store: Arc<dyn MapStore>,
    thresholds: TrustThresholds,
    ttl: Duration,
    cache: Mutex<HashMap<String, WorkspaceCache>>,
}

impl ZoneService {
    pub fn new(store: Arc<dyn MapStore>, thresholds: TrustThresholds) -> Self {
        Self::with_ttl(store, thresholds, DEFAULT_CACHE_TTL)
    }

    /// 指定缓存 TTL（`Duration::ZERO` = 每次调用都回源，测试用）。
    pub fn with_ttl(store: Arc<dyn MapStore>, thresholds: TrustThresholds, ttl: Duration) -> Self {
        Self {
            store,
            thresholds,
            ttl,
            cache: Mutex::new(HashMap::new()),
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

    /// maps 权威下的 zone 名称。`None` = 无位置 / trust Lost / 无默认图 /
    /// 未知显式 zoneRef（fail-closed，不回退层级）。
    ///
    /// `now` 预留给 age→trust 派生（Task 4 起 position 列存的是**生效**
    /// trust：fixed 恒 trusted、runtime 已按 age 降级），当前直接消费该值。
    pub async fn zone_for(
        &self,
        workspace_id: &str,
        thing_id: &str,
        explicit_zone: Option<&str>,
        _now: DateTime<Utc>,
    ) -> Result<Option<String>, String> {
        let Some(position) = self.store.get_position(workspace_id, thing_id).await? else {
            return Ok(None);
        };
        if position.trust == LocationTrust::Lost {
            return Ok(None);
        }
        let (Some(x), Some(y)) = (position.x, position.y) else {
            return Ok(None);
        };

        let maps = self.maps(workspace_id).await?;
        let map_id = match position.map_id.as_deref() {
            // 显式 map 必须存在且属于本工作区——否则 fail-closed。
            Some(id) if maps.iter().any(|map| map.id == id) => id.to_string(),
            Some(_) => return Ok(None),
            None => match maps.iter().find(|map| map.is_default) {
                Some(map) => map.id.clone(),
                None => return Ok(None),
            },
        };

        let places = self.places(workspace_id, &map_id).await?;
        Ok(resolve_zone_for(&places, explicit_zone, Some((x, y))).map(|place| place.name.clone()))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

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
        zone_source_calls: AtomicUsize,
        list_places_calls: AtomicUsize,
    }

    impl StubMapStore {
        fn new(source: &str, maps: Vec<MapRecord>, places: Vec<PlaceRecord>) -> Self {
            Self {
                source: Mutex::new(source.into()),
                maps,
                places,
                positions: HashMap::new(),
                zone_source_calls: AtomicUsize::new(0),
                list_places_calls: AtomicUsize::new(0),
            }
        }

        fn with_position(mut self, thing_id: &str, position: ThingPosition) -> Self {
            self.positions.insert(thing_id.into(), position);
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
}
