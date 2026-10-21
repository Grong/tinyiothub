//! E1 值守中枢：LiveState——实时态内存镜像 + Lagged 重同步 + health/zone 派生。
//!
//! 不变量（data_server.rs `cache_and_publish`）：device_cache 先写、EventBus
//! 后发。因此事件流出现 Lagged 空洞时，清空内部态后从 `ThingCacheSource`
//! 全量重读即得完整真相。
//!
//! health v1（spec §2.2，E4 声明制后细化）：ThingStatus error→Bad；
//! offline→Attention；否则 Ok；空洞期（状态未知）→Unknown。
//! zone v1：沿 parent_id 读 cache 拼祖先链名（根在前，`/` 连接），无祖先=空串。

use std::collections::HashMap;
use std::sync::Arc;

use tinyiothub_core::models::thing::ThingStatus;

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
    pub values: Vec<(String, String)>,
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

/// 实时态中枢。纯逻辑、零 sqlx；持久化读面只经 `ThingCacheSource`。
pub struct LiveState {
    cache: Arc<dyn ThingCacheSource>,
    states: HashMap<String, ThingLive>,
}

impl LiveState {
    pub fn new(cache: Arc<dyn ThingCacheSource>) -> Self {
        Self {
            cache,
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
    pub fn summary(&self, thing_id: &str) -> Option<ThingLiveSummary> {
        let live = self.states.get(thing_id)?;
        Some(ThingLiveSummary {
            health: Self::derive_health(live),
            zone: self.derive_zone(thing_id),
            online: live.online,
            values: live.values.clone(),
        })
    }

    /// E2：工作区实时快照。只含 cache 确知属于该工作区的物
    /// （Lagged 重同步读全局 cache，必须在此按工作区隔离）。
    pub fn workspace_snapshot(&self, workspace_id: &str) -> WorkspaceSnapshot {
        let mut devices: Vec<DeviceSnapshot> = self
            .states
            .iter()
            .filter_map(|(id, live)| {
                let cached = self.cache.get(id)?;
                if cached.workspace_id.as_deref() != Some(workspace_id) {
                    return None;
                }
                Some(DeviceSnapshot {
                    thing_id: id.clone(),
                    name: cached.get_display_name().to_string(),
                    health: Self::derive_health(live),
                    online: live.online,
                    zone: self.derive_zone(id),
                    values: live.values.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                })
            })
            .collect();
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

    /// zone v1：沿 parent_id 读 cache 拼祖先链名（根在前，`/` 连接），无祖先=空串。
    /// 祖先不在 cache（悬空 parent）时以 id 原文充段；深度上限防环。
    fn derive_zone(&self, thing_id: &str) -> String {
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

    #[test]
    fn lagged_triggers_full_resync_from_cache() {
        let cache = Arc::new(StubCache::with_things(vec![thing_with_prop(
            "t1",
            "温度",
            "31.5",
            /*parent=*/ Some("楼A"),
        )]));
        let mut live = LiveState::new(cache.clone());
        // 空洞：live 里 t1 无值
        assert_eq!(live.summary("t1"), None);
        live.handle_lagged();
        let s = live.summary("t1").unwrap();
        assert_eq!(s.values.get("温度").map(String::as_str), Some("31.5"));
        assert_eq!(s.zone, "楼A");
        assert_eq!(s.health, Health::Ok);
    }

    /// health v1 降级顺序：Bad > Attention > Ok；cache 真相压过可用性信号。
    #[test]
    fn health_degrades_bad_over_attention_over_ok() {
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
        assert_eq!(live.summary("t-ok").unwrap().health, Health::Ok);
        assert_eq!(live.summary("t-off").unwrap().health, Health::Attention);
        assert_eq!(live.summary("t-err").unwrap().health, Health::Bad);

        // error 物收到在线信号后，下一次属性事件刷新回 cache 真相仍 Bad。
        live.set_availability("t-err", true);
        live.apply_property_change("t-err", "温度", "30");
        assert_eq!(live.summary("t-err").unwrap().health, Health::Bad);
    }

    /// zone v1：多级祖先链按根在前拼接，用显示名。
    #[test]
    fn zone_joins_ancestor_chain_root_first() {
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
        assert_eq!(live.summary("t1").unwrap().zone, "园区A/3号楼");
    }

    /// zone v1：无祖先=空串。
    #[test]
    fn zone_empty_for_root_thing() {
        let cache = Arc::new(StubCache::with_things(vec![thing_with_prop(
            "t1", "温度", "31.5", None,
        )]));
        let mut live = LiveState::new(cache);
        live.handle_lagged();
        assert_eq!(live.summary("t1").unwrap().zone, "");
    }

    /// 空洞期（事件先于 cache 真相到达）→Unknown；cache 补上后重同步恢复。
    #[test]
    fn unknown_health_recovers_after_resync() {
        let cache = Arc::new(StubCache::with_things(vec![]));
        let mut live = LiveState::new(cache.clone());
        live.apply_property_change("ghost", "温度", "20");
        assert_eq!(live.summary("ghost").unwrap().health, Health::Unknown);

        cache.insert(thing_with_prop("ghost", "温度", "21", None));
        live.handle_lagged();
        let s = live.summary("ghost").unwrap();
        assert_eq!(s.health, Health::Ok);
        assert_eq!(s.values.get("温度").map(String::as_str), Some("21"));
    }

    /// 可用性事件即可建条目：online 标记与离线 Attention 派生。
    #[test]
    fn availability_alone_creates_entry() {
        let cache = Arc::new(StubCache::with_things(vec![]));
        let mut live = LiveState::new(cache);
        live.set_availability("t1", true);
        let s = live.summary("t1").unwrap();
        assert!(s.online);
        assert_eq!(s.health, Health::Ok);
        live.set_availability("t1", false);
        let s = live.summary("t1").unwrap();
        assert!(!s.online);
        assert_eq!(s.health, Health::Attention);
    }

    /// Error 物收到可用性事件仍 Bad：cache 真相压过可用性信号，
    /// 不得被无条件覆写成 Online/Offline（与 apply_property_change 对称）。
    #[test]
    fn error_thing_stays_bad_on_availability_event() {
        let cache = Arc::new(StubCache::with_things(vec![Thing {
            id: "t-err".into(),
            name: "t-err".into(),
            status: ThingStatus::Error,
            ..Thing::default()
        }]));
        let mut live = LiveState::new(cache);
        live.handle_lagged();
        assert_eq!(live.summary("t-err").unwrap().health, Health::Bad);
        live.set_availability("t-err", true);
        assert_eq!(live.summary("t-err").unwrap().health, Health::Bad);
        live.set_availability("t-err", false);
        assert_eq!(live.summary("t-err").unwrap().health, Health::Bad);
    }
}
