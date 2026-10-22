//! E1 值守中枢：LiveState——实时态内存镜像 + Lagged 重同步 + health/zone 派生。
//!
//! 不变量（data_server.rs `cache_and_publish`）：device_cache 先写、EventBus
//! 后发。因此事件流出现 Lagged 空洞时，清空内部态后从 `ThingCacheSource`
//! 全量重读即得完整真相。
//!
//! health v1（spec §2.2，E4 声明制后细化）：ThingStatus error→Bad；
//! offline→Attention；否则 Ok；空洞期（状态未知）→Unknown。
//!
//! zone（E4d §2.2）：`workspaces.zone_source='maps'` 且注入了
//! [`ZoneService`] 时由 maps 权威解析（无位置/无 zone → 空串，**不回退**层级）；
//! `hierarchy` 或未注入 ZoneService（测试/降级形态）时保持 E1b 行为：沿
//! parent_id 读 cache 拼祖先链名（根在前，`/` 连接），无祖先=空串。

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use tinyiothub_core::models::thing::ThingStatus;

use crate::maps::ZoneService;
use crate::ports::ThingCacheSource;

/// 物健康度（v1 派生，非存储）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    Ok,
    Attention,
    Bad,
    Unknown,
}

/// 单物实时摘要：`summary()` 的输出。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThingLiveSummary {
    pub health: Health,
    pub zone: String,
    pub online: bool,
    pub values: HashMap<String, String>,
}

/// E2 turn 开头快照（spec §4）：工作区实时态投影。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSnapshot {
    pub v: u8,
    /// health worst-first，同级按 zone、name。
    pub devices: Vec<DeviceSnapshot>,
    /// 本工作区已知设备数（快照渲染时用；omitted 由渲染器按预算计算）。
    pub total: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceSnapshot {
    pub thing_id: String,
    pub name: String,
    pub health: Health,
    pub online: bool,
    pub zone: String,
    /// Properties with a reported value (rendered into the prompt).
    pub values: Vec<(String, String)>,
    /// Declared property names from the cached thing (E4b: snapshot reads
    /// record demand for unsampled declared properties too).
    pub properties: Vec<String>,
}

impl Health {
    /// 排序权重：Bad > Attention > Unknown > Ok（worst-first）。
    pub fn rank(&self) -> u8 {
        match self {
            Self::Bad => 0,
            Self::Attention => 1,
            Self::Unknown => 2,
            Self::Ok => 3,
        }
    }
}

/// 单物内部实时态。`status: None` 表示状态未知（空洞期/cache 查无此物）。
#[derive(Debug, Default)]
struct ThingLive {
    status: Option<ThingStatus>,
    online: bool,
    values: HashMap<String, String>,
}

/// 实时态中枢。纯逻辑、零 sqlx；持久化读面只经 port（cache / ZoneService）。
pub struct LiveState {
    cache: Arc<dyn ThingCacheSource>,
    /// E4d maps 权威；None = 未接线（测试/降级）→ hierarchy。
    zones: Option<Arc<ZoneService>>,
    states: HashMap<String, ThingLive>,
}

impl LiveState {
    /// hierarchy-only 形态（E1b 行为，测试/未接线）。
    pub fn new(cache: Arc<dyn ThingCacheSource>) -> Self {
        Self {
            cache,
            zones: None,
            states: HashMap::new(),
        }
    }

    /// 注入 ZoneService：`zone_source=maps` 的工作区走 maps 权威。
    pub fn with_zone_service(cache: Arc<dyn ThingCacheSource>, zones: Arc<ZoneService>) -> Self {
        Self {
            cache,
            zones: Some(zones),
            states: HashMap::new(),
        }
    }

    /// 属性变更事件：更新 values，并按 cache 真相刷新状态（cache miss 保留原状）。
    pub fn apply_property_change(&mut self, thing_id: &str, property: &str, value: &str) {
        let cached = self.cache.get(thing_id);
        let live = self.states.entry(thing_id.to_string()).or_default();
        live.values.insert(property.to_string(), value.to_string());
        if let Some(thing) = cached {
            live.status = Some(thing.status.clone());
            live.online = thing.is_online();
        }
    }

    /// 可用性事件：cache 有真相则以真相刷新 status/online（Error 不被可用性
    /// 信号掩盖，与 apply_property_change 对称）；cache miss 才按事件信号落
    /// Online/Offline。
    pub fn set_availability(&mut self, thing_id: &str, online: bool) {
        let cached = self.cache.get(thing_id);
        let live = self.states.entry(thing_id.to_string()).or_default();
        if let Some(thing) = cached {
            live.status = Some(thing.status.clone());
            live.online = thing.is_online();
        } else {
            live.online = online;
            live.status = Some(if online {
                ThingStatus::Online
            } else {
                ThingStatus::Offline
            });
        }
    }

    /// Lagged 空洞：清空内部态，从 cache 全量重建（cache 先写后发，重读即真相）。
    pub fn handle_lagged(&mut self) {
        self.states.clear();
        for thing in self.cache.all() {
            let values = thing
                .properties
                .as_deref()
                .unwrap_or(&[])
                .iter()
                .filter_map(|p| p.current_value.as_ref().map(|v| (p.name.clone(), v.clone())))
                .collect();
            self.states.insert(
                thing.id.clone(),
                ThingLive {
                    online: thing.is_online(),
                    status: Some(thing.status.clone()),
                    values,
                },
            );
        }
    }

    /// 单物摘要；无实时态（未见过该物事件且未经重同步）返回 None。
    /// health/zone 在读时即时派生，永远与 cache 真相一致。
    pub async fn summary(&self, thing_id: &str) -> Option<ThingLiveSummary> {
        let live = self.states.get(thing_id)?;
        Some(ThingLiveSummary {
            health: Self::derive_health(live),
            zone: self.derive_zone(thing_id, None).await,
            online: live.online,
            values: live.values.clone(),
        })
    }

    /// E2：工作区实时快照。只含 cache 确知属于该工作区的物
    /// （Lagged 重同步读全局 cache，必须在此按工作区隔离）。
    ///
    /// maps 模式先批量预热坐标/地点（`ZoneService::warm_workspace`），
    /// 避免 N 个物产生 N 次串行查询；预热失败降级 hierarchy（读面可用性
    /// 优先，spec §2.2）。
    pub async fn workspace_snapshot(&self, workspace_id: &str) -> WorkspaceSnapshot {
        let use_hierarchy = match &self.zones {
            None => true,
            Some(zones) => match zones.zone_source(workspace_id).await {
                Ok(source) if source == "maps" => match zones.warm_workspace(workspace_id).await {
                    Ok(()) => false,
                    Err(e) => {
                        tracing::warn!(
                            workspace_id,
                            error = %e,
                            "maps 批量预热失败，快照降级 hierarchy"
                        );
                        true
                    }
                },
                Ok(_) => true,
                Err(e) => {
                    tracing::warn!(workspace_id, error = %e, "zone_source 读取失败，快照降级 hierarchy");
                    true
                }
            },
        };

        let mut devices: Vec<DeviceSnapshot> = Vec::with_capacity(self.states.len());
        for (id, live) in &self.states {
            let Some(cached) = self.cache.get(id) else {
                continue;
            };
            if cached.workspace_id.as_deref() != Some(workspace_id) {
                continue;
            }
            let zone = if use_hierarchy {
                self.hierarchy_zone(id)
            } else {
                self.derive_zone(id, Some(workspace_id)).await
            };
            devices.push(DeviceSnapshot {
                thing_id: id.clone(),
                name: cached.get_display_name().to_string(),
                health: Self::derive_health(live),
                online: live.online,
                zone,
                values: live.values.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                properties: cached
                    .properties
                    .as_ref()
                    .map(|props| props.iter().map(|p| p.name.clone()).collect())
                    .unwrap_or_default(),
            });
        }
        devices.sort_by(|a, b| {
            a.health
                .rank()
                .cmp(&b.health.rank())
                .then_with(|| a.zone.cmp(&b.zone))
                .then_with(|| a.name.cmp(&b.name))
        });
        WorkspaceSnapshot {
            v: 1,
            total: devices.len() as u32,
            devices,
        }
    }

    /// health v1：error→Bad；offline→Attention；否则 Ok；状态未知（空洞期）→Unknown。
    fn derive_health(live: &ThingLive) -> Health {
        match &live.status {
            None => Health::Unknown,
            Some(ThingStatus::Error) => Health::Bad,
            Some(ThingStatus::Offline) => Health::Attention,
            Some(ThingStatus::Online) => Health::Ok,
        }
    }

    /// E4d zone 权威切换：map 权威 → ZoneService；否则沿 parent_id 拼祖先链。
    ///
    /// maps 模式返回 `None` 时保持空串（spec §2.2：不回退层级）。MapStore 读失败
    /// 时降级为 hierarchy（可用性优先；maps 数据缺失不等于层级不存在）。显式
    /// `placement.zoneRef` 已物化进物记录，由 `ZoneService` 内部优先解析
    /// （与坐标无关），这里不再单独传参。
    async fn derive_zone(&self, thing_id: &str, workspace_hint: Option<&str>) -> String {
        let Some(zones) = &self.zones else {
            return self.hierarchy_zone(thing_id);
        };
        let workspace_id = match workspace_hint {
            Some(ws) => ws.to_string(),
            None => match self.cache.get(thing_id).and_then(|thing| thing.workspace_id) {
                Some(ws) => ws,
                None => return self.hierarchy_zone(thing_id),
            },
        };
        match zones.zone_source(&workspace_id).await {
            Ok(source) if source == "maps" => match zones.zone_for(&workspace_id, thing_id, None, Utc::now()).await {
                Ok(Some(name)) => name,
                Ok(None) => String::new(),
                Err(e) => {
                    tracing::warn!(workspace_id, thing_id, error = %e, "zone_source=maps 读取失败，降级 hierarchy");
                    self.hierarchy_zone(thing_id)
                }
            },
            Ok(_) => self.hierarchy_zone(thing_id),
            Err(e) => {
                tracing::warn!(workspace_id, error = %e, "zone_source 读取失败，降级 hierarchy");
                self.hierarchy_zone(thing_id)
            }
        }
    }

    /// zone v1（hierarchy 回退）：沿 parent_id 读 cache 拼祖先链名（根在前，`/` 连接），
    /// 无祖先=空串。祖先不在 cache（悬空 parent）时以 id 原文充段；深度上限防环。
    fn hierarchy_zone(&self, thing_id: &str) -> String {
        const MAX_DEPTH: usize = 10;
        let mut segments = Vec::new();
        let mut cursor = self.cache.get(thing_id).and_then(|t| t.parent_id);
        for _ in 0..MAX_DEPTH {
            let Some(parent_id) = cursor else { break };
            match self.cache.get(&parent_id) {
                Some(parent) => {
                    segments.push(parent.get_display_name().to_string());
                    cursor = parent.parent_id;
                }
                None => {
                    segments.push(parent_id);
                    break;
                }
            }
        }
        segments.reverse();
        segments.join("/")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tinyiothub_core::models::thing::Thing;
    use tinyiothub_core::models::thing_property::ThingProperty;

    /// 内存桩：实现 ThingCacheSource 的最小可读缓存。
    struct StubCache {
        things: Mutex<HashMap<String, Thing>>,
    }

    impl StubCache {
        fn with_things(things: Vec<Thing>) -> Self {
            Self {
                things: Mutex::new(things.into_iter().map(|t| (t.id.clone(), t)).collect()),
            }
        }
    }

    impl ThingCacheSource for StubCache {
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

    /// 造一个在线物，带一个当前值属性，可挂父。
    fn thing_with_prop(id: &str, prop: &str, value: &str, parent: Option<&str>) -> Thing {
        Thing {
            id: id.into(),
            name: id.into(),
            status: ThingStatus::Online,
            parent_id: parent.map(str::to_string),
            properties: Some(vec![ThingProperty {
                id: format!("{id}-{prop}"),
                thing_id: id.into(),
                name: prop.into(),
                display_name: None,
                description: None,
                data_type: Some("float".into()),
                unit: None,
                min_value: None,
                max_value: None,
                default_value: None,
                is_read_only: 1,
                source: None,
                expression: None,
                refs: None,
                created_at: None,
                updated_at: None,
                current_value: Some(value.into()),
                alarm_status: None,
                derived_status: None,
            }]),
            ..Thing::default()
        }
    }

    #[tokio::test]
    async fn lagged_triggers_full_resync_from_cache() {
        let cache = Arc::new(StubCache::with_things(vec![thing_with_prop(
            "t1",
            "温度",
            "31.5",
            /*parent=*/ Some("楼A"),
        )]));
        let mut live = LiveState::new(cache.clone());
        // 空洞：live 里 t1 无值
        assert_eq!(live.summary("t1").await, None);
        live.handle_lagged();
        let s = live.summary("t1").await.unwrap();
        assert_eq!(s.values.get("温度").map(String::as_str), Some("31.5"));
        assert_eq!(s.zone, "楼A");
        assert_eq!(s.health, Health::Ok);
    }

    /// health v1 降级顺序：Bad > Attention > Ok；cache 真相压过可用性信号。
    #[tokio::test]
    async fn health_degrades_bad_over_attention_over_ok() {
        let base = |id: &str, status: ThingStatus| Thing {
            id: id.into(),
            name: id.into(),
            status,
            ..Thing::default()
        };
        let cache = Arc::new(StubCache::with_things(vec![
            base("t-ok", ThingStatus::Online),
            base("t-off", ThingStatus::Offline),
            base("t-err", ThingStatus::Error),
        ]));
        let mut live = LiveState::new(cache);
        live.handle_lagged();
        assert_eq!(live.summary("t-ok").await.unwrap().health, Health::Ok);
        assert_eq!(live.summary("t-off").await.unwrap().health, Health::Attention);
        assert_eq!(live.summary("t-err").await.unwrap().health, Health::Bad);

        // error 物收到在线信号后，下一次属性事件刷新回 cache 真相仍 Bad。
        live.set_availability("t-err", true);
        live.apply_property_change("t-err", "温度", "30");
        assert_eq!(live.summary("t-err").await.unwrap().health, Health::Bad);
    }

    /// zone v1：多级祖先链按根在前拼接，用显示名。
    #[tokio::test]
    async fn zone_joins_ancestor_chain_root_first() {
        let base = |id: &str, display: &str, parent: Option<&str>| Thing {
            id: id.into(),
            name: id.into(),
            display_name: Some(display.into()),
            parent_id: parent.map(str::to_string),
            ..Thing::default()
        };
        let cache = Arc::new(StubCache::with_things(vec![
            base("p1", "园区A", None),
            base("p2", "3号楼", Some("p1")),
            thing_with_prop("t1", "温度", "31.5", Some("p2")),
        ]));
        let mut live = LiveState::new(cache);
        live.handle_lagged();
        assert_eq!(live.summary("t1").await.unwrap().zone, "园区A/3号楼");
    }

    /// zone v1：无祖先=空串。
    #[tokio::test]
    async fn zone_empty_for_root_thing() {
        let cache = Arc::new(StubCache::with_things(vec![thing_with_prop(
            "t1", "温度", "31.5", None,
        )]));
        let mut live = LiveState::new(cache);
        live.handle_lagged();
        assert_eq!(live.summary("t1").await.unwrap().zone, "");
    }

    /// 空洞期（事件先于 cache 真相到达）→Unknown；cache 补上后重同步恢复。
    #[tokio::test]
    async fn unknown_health_recovers_after_resync() {
        let cache = Arc::new(StubCache::with_things(vec![]));
        let mut live = LiveState::new(cache.clone());
        live.apply_property_change("ghost", "温度", "20");
        assert_eq!(live.summary("ghost").await.unwrap().health, Health::Unknown);

        cache.insert(thing_with_prop("ghost", "温度", "21", None));
        live.handle_lagged();
        let s = live.summary("ghost").await.unwrap();
        assert_eq!(s.health, Health::Ok);
        assert_eq!(s.values.get("温度").map(String::as_str), Some("21"));
    }

    /// 可用性事件即可建条目：online 标记与离线 Attention 派生。
    #[tokio::test]
    async fn availability_alone_creates_entry() {
        let cache = Arc::new(StubCache::with_things(vec![]));
        let mut live = LiveState::new(cache);
        live.set_availability("t1", true);
        let s = live.summary("t1").await.unwrap();
        assert!(s.online);
        assert_eq!(s.health, Health::Ok);
        live.set_availability("t1", false);
        let s = live.summary("t1").await.unwrap();
        assert!(!s.online);
        assert_eq!(s.health, Health::Attention);
    }

    /// Error 物收到可用性事件仍 Bad：cache 真相压过可用性信号，
    /// 不得被无条件覆写成 Online/Offline（与 apply_property_change 对称）。
    #[tokio::test]
    async fn error_thing_stays_bad_on_availability_event() {
        let cache = Arc::new(StubCache::with_things(vec![Thing {
            id: "t-err".into(),
            name: "t-err".into(),
            status: ThingStatus::Error,
            ..Thing::default()
        }]));
        let mut live = LiveState::new(cache);
        live.handle_lagged();
        assert_eq!(live.summary("t-err").await.unwrap().health, Health::Bad);
        live.set_availability("t-err", true);
        assert_eq!(live.summary("t-err").await.unwrap().health, Health::Bad);
        live.set_availability("t-err", false);
        assert_eq!(live.summary("t-err").await.unwrap().health, Health::Bad);
    }

    // ── E4d zone 权威切换 ────────────────────────────────────────────

    use std::time::Duration;

    use async_trait::async_trait;

    use crate::maps::{
        Geometry, LocationTrust, MapBounds, MapRecord, PlaceKind, PlaceRecord, ThingPosition, TrustThresholds,
        ZoneService,
    };
    use crate::ports::MapStore;

    /// 可切换 zone_source 的 MapStore 桩。
    struct StubMapStore {
        source: Mutex<String>,
        maps: Vec<MapRecord>,
        places: Vec<PlaceRecord>,
        positions: HashMap<String, ThingPosition>,
        zone_refs: Mutex<HashMap<String, String>>,
        /// 模拟 list_positions（快照批量预热）读失败。
        fail_list_positions: std::sync::atomic::AtomicBool,
    }

    impl StubMapStore {
        fn with_zone_location() -> Self {
            Self {
                source: Mutex::new("maps".into()),
                maps: vec![MapRecord {
                    id: "m1".into(),
                    workspace_id: "ws1".into(),
                    name: "一楼".into(),
                    bounds: MapBounds {
                        x: 0.0,
                        y: 0.0,
                        w: 100.0,
                        h: 100.0,
                    },
                    anchor_map_id: None,
                    is_default: true,
                    created_at: "2026-10-22T00:00:00Z".into(),
                    updated_at: "2026-10-22T00:00:00Z".into(),
                }],
                places: vec![PlaceRecord {
                    id: "z1".into(),
                    map_id: "m1".into(),
                    name: "车间A".into(),
                    kind: PlaceKind::Zone,
                    geometry: Geometry::Polygon {
                        points: vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)],
                    },
                    heading: None,
                    created_at: "2026-10-22T00:00:00Z".into(),
                    updated_at: "2026-10-22T00:00:00Z".into(),
                }],
                positions: HashMap::from([(
                    "t1".to_string(),
                    ThingPosition {
                        map_id: None,
                        x: Some(1.0),
                        y: Some(1.0),
                        level: None,
                        heading: None,
                        trust: LocationTrust::Trusted,
                        // 静态位置（fixed/declared）：读时恒 Trusted，不随
                        // 测试运行时刻 aging。
                        updated_at: None,
                    },
                )]),
                zone_refs: Mutex::new(HashMap::new()),
                fail_list_positions: std::sync::atomic::AtomicBool::new(false),
            }
        }

        fn without_position(mut self) -> Self {
            self.positions.clear();
            self
        }

        fn fail_position_reads(&self) {
            self.fail_list_positions
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }

        fn set_source(&self, source: &str) {
            *self.source.lock().unwrap() = source.into();
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

        async fn set_reading_position(
            &self,
            _workspace_id: &str,
            _thing_id: &str,
            _position: &ThingPosition,
        ) -> Result<(), String> {
            Ok(())
        }

        async fn list_positions(&self, _workspace_id: &str) -> Result<Vec<(String, ThingPosition)>, String> {
            if self.fail_list_positions.load(std::sync::atomic::Ordering::Relaxed) {
                return Err("db down".into());
            }
            Ok(self
                .positions
                .iter()
                .map(|(thing_id, position)| (thing_id.clone(), position.clone()))
                .collect())
        }

        async fn get_zone_ref(&self, _workspace_id: &str, thing_id: &str) -> Result<Option<String>, String> {
            Ok(self.zone_refs.lock().unwrap().get(thing_id).cloned())
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

        async fn zone_source(&self, _workspace_id: &str) -> Result<String, String> {
            Ok(self.source.lock().unwrap().clone())
        }
    }

    /// 带层级（p1←t1）且属于 ws1 的缓存。
    fn hierarchy_cache() -> Arc<StubCache> {
        let parent = Thing {
            id: "p1".into(),
            name: "p1".into(),
            display_name: Some("楼A".into()),
            workspace_id: Some("ws1".into()),
            ..Thing::default()
        };
        let mut child = thing_with_prop("t1", "温度", "31.5", Some("p1"));
        child.workspace_id = Some("ws1".into());
        Arc::new(StubCache::with_things(vec![parent, child]))
    }

    fn zone_service(store: StubMapStore, ttl: Duration) -> Arc<ZoneService> {
        Arc::new(ZoneService::with_ttl(Arc::new(store), TrustThresholds::default(), ttl))
    }

    #[tokio::test]
    async fn maps_zone_authority_replaces_hierarchy() {
        let cache = hierarchy_cache();
        let mut live = LiveState::with_zone_service(
            cache,
            zone_service(StubMapStore::with_zone_location(), Duration::from_secs(60)),
        );
        live.handle_lagged();
        assert_eq!(live.summary("t1").await.unwrap().zone, "车间A");

        let snapshot = live.workspace_snapshot("ws1").await;
        let t1 = snapshot
            .devices
            .iter()
            .find(|device| device.thing_id == "t1")
            .expect("t1 在快照内");
        assert_eq!(t1.zone, "车间A");
    }

    /// maps 模式无位置 → 空串，**不回退**层级（spec §2.2）。
    #[tokio::test]
    async fn maps_mode_without_position_does_not_fall_back_to_hierarchy() {
        let cache = hierarchy_cache();
        let store = StubMapStore::with_zone_location().without_position();
        let mut live = LiveState::with_zone_service(cache, zone_service(store, Duration::from_secs(60)));
        live.handle_lagged();
        assert_eq!(live.summary("t1").await.unwrap().zone, "");
    }

    /// zone_source 切回 hierarchy 后恢复 E1b 祖先链（迁移期回退）。
    #[tokio::test]
    async fn switching_zone_source_back_to_hierarchy_restores_ancestor_chain() {
        let cache = hierarchy_cache();
        let store = Arc::new(StubMapStore::with_zone_location());
        // TTL 0：每次摘要都重读 flag，模拟切换即时生效。
        let zones = Arc::new(ZoneService::with_ttl(
            store.clone(),
            TrustThresholds::default(),
            Duration::ZERO,
        ));
        let mut live = LiveState::with_zone_service(cache, zones);
        live.handle_lagged();
        assert_eq!(live.summary("t1").await.unwrap().zone, "车间A");

        store.set_source("hierarchy");
        assert_eq!(live.summary("t1").await.unwrap().zone, "楼A");
    }

    /// 未注入 ZoneService 时保持 hierarchy（降级形态）。
    #[tokio::test]
    async fn absent_zone_service_keeps_hierarchy() {
        let mut live = LiveState::new(hierarchy_cache());
        live.handle_lagged();
        assert_eq!(live.summary("t1").await.unwrap().zone, "楼A");
    }

    /// 快照批量预热读失败（MapStore error）→ 快照降级 hierarchy，不返回
    /// 错误/空 zone（可用性优先，Task 3 评审 carry-forward）。
    #[tokio::test]
    async fn snapshot_degrades_to_hierarchy_when_map_store_errors() {
        let cache = hierarchy_cache();
        let store = StubMapStore::with_zone_location();
        store.fail_position_reads();
        let mut live = LiveState::with_zone_service(cache, zone_service(store, Duration::from_secs(60)));
        live.handle_lagged();
        let snapshot = live.workspace_snapshot("ws1").await;
        let t1 = snapshot
            .devices
            .iter()
            .find(|device| device.thing_id == "t1")
            .expect("t1 在快照内");
        assert_eq!(t1.zone, "楼A", "MapStore 读失败必须降级 hierarchy");
    }

    /// 显式 zoneRef 物化后无坐标也参与摘要（ZoneService 内部优先解析）。
    #[tokio::test]
    async fn snapshot_uses_materialized_zone_ref_without_position() {
        let cache = hierarchy_cache();
        let zones = zone_service(
            StubMapStore::with_zone_location().without_position(),
            Duration::from_secs(60),
        );
        zones.set_zone_ref("ws1", "t1", Some("车间A")).await.unwrap();
        let mut live = LiveState::with_zone_service(cache, zones);
        live.handle_lagged();
        assert_eq!(live.summary("t1").await.unwrap().zone, "车间A");
    }
}
