//! Port traits — runtime's only window onto persistence (D15, Task 11.5).
//!
//! runtime is a framework crate and must not depend on `db`/sqlx; the
//! composition root (apps/cloud) injects db-backed adapters implementing
//! these traits. Method sets cover exactly the current call sites — add a
//! method only when a real caller appears (YAGNI).
//!
//! Error type is `String`: adapters map concrete backend errors (sqlx etc.)
//! at the boundary so the port surface stays backend-agnostic.

use async_trait::async_trait;

use tinyiothub_core::driver::declaration::{AcceptanceReportRecord, DeviceDeclarationRecord, DriverDeclarationRecord};
use tinyiothub_core::driver::manifest::{ManifestTool, StateFieldDecl};
use tinyiothub_core::models::spatial::{MapRecord, PlaceKind, PlaceRecord, ThingPosition};
use tinyiothub_core::models::thing::Thing;
use tinyiothub_core::models::thing_command::ThingCommand;
use tinyiothub_core::models::thing_property::ThingProperty;
use tinyiothub_core::watch::WatchRule;

/// Read-side queries for device commands (cron `device_command` executor).
#[async_trait]
pub trait ThingCommandQueries: Send + Sync {
    /// Find a command by device ID and command name.
    async fn find_by_thing_and_name(&self, thing_id: &str, name: &str) -> Result<Option<ThingCommand>, String>;
}

/// Event-retention writes (cron `event_retention` executor).
#[async_trait]
pub trait EventRetentionStore: Send + Sync {
    /// Delete occurrence-type events (`is_status = 0`) with timestamp older
    /// than `cutoff_rfc3339`. Status rows (`is_status = 1`, live device
    /// state) are exempt. Returns rows deleted.
    async fn delete_occurrence_events_before(&self, cutoff_rfc3339: &str) -> Result<u64, String>;
}

/// Approval-timeout escalations (cron `approval_timeout` executor, T6):
/// awaiting_approval 超过 24h 的判断自动升级为工单（防"审批堆积"——设计
/// 文档状态机）。返回升级的条数。
///
/// 2026-09-14 硬化（E2）：三态 SLA 清扫。
/// - investigating 超 SLA（默认 30min）：dispatch 被拦/调查挂起 → 标
///   dispatch_suppressed 标记（不开票、不计预算；迟到 RunRecorded 恢复路由）
/// - executing 超 SLA（默认 1h）：执行未验证/队列卡死 → escalated + 人工
///   确认工单（T-16/C4：起算点是 state_entered_at，不是批准时刻）
#[async_trait]
pub trait ApprovalTimeoutStore: Send + Sync {
    async fn escalate_stale_approvals(&self, cutoff_rfc3339: &str) -> Result<u64, String>;
    async fn mark_stale_investigating(&self, cutoff_rfc3339: &str) -> Result<u64, String>;
    async fn escalate_stale_executing(&self, cutoff_rfc3339: &str) -> Result<u64, String>;
    /// X5：dismissed 超 cutoff（默认 72h）且关联报警仍 Active → 重浮 investigating 重查。
    async fn resurface_stale_dismissed(&self, cutoff_rfc3339: &str) -> Result<u64, String>;
}

/// 派生属性读取（DerivedDriverManager：启动索引重建 + descendant_count 求值）。
/// spec §三 E1：runtime 禁止直接 sqlx。
#[async_trait]
pub trait DerivedPropertyQueries: Send + Sync {
    /// 全部派生属性定义（expression IS NOT NULL），含 workspace 隔离由调用方按物过滤。
    async fn list_derived(&self) -> Result<Vec<ThingProperty>, String>;
    /// 按 id 加载物（含属性；ensure_owner 热路径自愈用——REST 建物/场景实例化
    /// 不写 device_cache，cache miss 时经此方法从库补载后继续登记，不再静默
    /// 失效）。无 workspace 过滤：调用方（物管理面钩子）已完成作用域鉴权，
    /// thing_id 全局唯一。
    async fn find_thing_with_properties(&self, thing_id: &str) -> Result<Option<Thing>, String>;
    /// root 物的后代计数（沿 parent_id 递归，深度上限 10）；thing_type 为空计全部。
    async fn count_descendants(&self, root_thing_id: &str, thing_type: Option<&str>) -> Result<i64, String>;
}

/// E1 账本写入（spec §4.2）：kind 词表 versioned，payload 含 v=1。
#[async_trait]
pub trait HubLedger: Send + Sync {
    async fn append(
        &self,
        workspace_id: &str,
        kind: &str,
        thing_id: Option<&str>,
        payload: serde_json::Value,
    ) -> Result<(), String>;
}

/// E1 watch 规则持久源（spec §4.1）：启动重织读全量，注册/删除写穿。
#[async_trait]
pub trait WatchRuleStore: Send + Sync {
    async fn list_all(&self) -> Result<Vec<WatchRule>, String>;
    async fn insert(&self, rule: &WatchRule) -> Result<(), String>;
    async fn delete(&self, workspace_id: &str, id: &str) -> Result<bool, String>;
}

/// E4a 包级声明持久源：安装/升级写穿，启动重织读全量。
#[async_trait]
pub trait DriverDeclarationStore: Send + Sync {
    async fn upsert_active(&self, row: &DriverDeclarationRecord) -> Result<(), String>;
    async fn list_active(&self, workspace_id: &str) -> Result<Vec<DriverDeclarationRecord>, String>;
    async fn list_all(&self) -> Result<Vec<DriverDeclarationRecord>, String>;
}

/// E4a 设备级声明持久源（describe(config) 结果）。
#[async_trait]
pub trait DeviceDeclarationStore: Send + Sync {
    async fn upsert(&self, row: &DeviceDeclarationRecord) -> Result<(), String>;
    async fn list_by_workspace(&self, workspace_id: &str) -> Result<Vec<DeviceDeclarationRecord>, String>;
    async fn find(&self, workspace_id: &str, thing_id: &str) -> Result<Option<DeviceDeclarationRecord>, String>;
}

/// E4a 验收报告写入（append-only）。
#[async_trait]
pub trait AcceptanceReportStore: Send + Sync {
    async fn insert(&self, row: &AcceptanceReportRecord) -> Result<(), String>;
}

/// E4a 声明物化（thing_properties / thing_actions 投影；只 upsert + stale）。
#[async_trait]
pub trait DeclarationProjectionStore: Send + Sync {
    async fn upsert_properties(&self, thing_id: &str, fields: &[StateFieldDecl]) -> Result<(), String>;
    async fn upsert_commands(&self, thing_id: &str, tools: &[ManifestTool]) -> Result<(), String>;
    async fn mark_stale(
        &self,
        thing_id: &str,
        keep_properties: &[String],
        keep_commands: &[String],
    ) -> Result<(), String>;
}

/// E4a 物 + 属性加载（绑定/物化前的配置读取）。
#[async_trait]
pub trait ThingDeclarationSource: Send + Sync {
    async fn load_thing(&self, thing_id: &str) -> Result<Option<Thing>, String>;
}

/// A property demanded by a persisted rule (E4b spec §2.1): watch rules,
/// derived-property references, and device alarm rules. `property` is empty
/// for health/availability watches, which demand the thing as a whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DemandPropertyRef {
    pub workspace_id: String,
    pub thing_id: String,
    pub property: String,
}

/// E4b demand snapshot port (spec §2.1): the cloud adapter unions
/// `watch_rules`, derived-property refs and `device_alarm_rules`, polling
/// about once per second. runtime keeps no db dependency; it only unions the
/// snapshot with local subscriptions/reads in `DemandTracker`.
#[async_trait]
pub trait DemandSource: Send + Sync {
    async fn snapshot(&self) -> Result<Vec<DemandPropertyRef>, String>;
}

/// E4d spatial source (ZoneService): maps/places + thing position, workspace-scoped.
/// `zone_source` returns `'hierarchy'` or `'maps'`; unknown/missing normalizes to
/// `hierarchy` so callers keep the migration-era fallback.
#[async_trait]
pub trait MapStore: Send + Sync {
    async fn list_maps(&self, workspace_id: &str) -> Result<Vec<MapRecord>, String>;
    async fn default_map(&self, workspace_id: &str) -> Result<Option<MapRecord>, String>;
    async fn upsert_map(&self, map: &MapRecord) -> Result<(), String>;
    async fn delete_map(&self, workspace_id: &str, id: &str) -> Result<bool, String>;
    async fn list_places(&self, workspace_id: &str, map_id: &str) -> Result<Vec<PlaceRecord>, String>;
    /// `kind` filters zoneRef resolution (`Some(PlaceKind::Zone)`).
    async fn find_place_by_name(
        &self,
        workspace_id: &str,
        name: &str,
        kind: Option<PlaceKind>,
    ) -> Result<Option<PlaceRecord>, String>;
    async fn upsert_place(&self, workspace_id: &str, place: &PlaceRecord) -> Result<(), String>;
    async fn delete_place(&self, workspace_id: &str, map_id: &str, id: &str) -> Result<bool, String>;
    async fn get_position(&self, workspace_id: &str, thing_id: &str) -> Result<Option<ThingPosition>, String>;
    async fn set_position(&self, workspace_id: &str, thing_id: &str, position: &ThingPosition) -> Result<(), String>;
    /// 驱动读数坐标写入：只更新坐标列，不触碰 `map_id` / `zone_ref`
    /// （地图与显式 zone 由 placement/API 维护）。
    async fn set_reading_position(
        &self,
        workspace_id: &str,
        thing_id: &str,
        position: &ThingPosition,
    ) -> Result<(), String>;
    /// 工作区全量坐标（快照批量预热；避免逐物 N 次查询）。
    async fn list_positions(&self, workspace_id: &str) -> Result<Vec<(String, ThingPosition)>, String>;
    /// 物显式 `placement.zoneRef`（物化列）；`None` = 无显式引用。
    async fn get_zone_ref(&self, workspace_id: &str, thing_id: &str) -> Result<Option<String>, String>;
    async fn set_zone_ref(&self, workspace_id: &str, thing_id: &str, zone_ref: Option<&str>) -> Result<(), String>;
    /// 工作区全部非空显式 zone 引用（快照批量预热）。
    async fn list_zone_refs(&self, workspace_id: &str) -> Result<Vec<(String, String)>, String>;
    async fn zone_source(&self, workspace_id: &str) -> Result<String, String>;
}

/// Thing cache used by `DataServer`. Sync because every call site is sync
/// (the backing implementation is an in-memory cache); making this async
/// would add `.await` noise with no benefit.
pub trait ThingCacheSource: Send + Sync {
    fn all(&self) -> Vec<Thing>;
    fn get(&self, id: &str) -> Option<Thing>;
    fn get_by_name(&self, name: &str) -> Option<Thing>;
    fn insert(&self, device: Thing);
    fn update(&self, device: Thing);
    fn remove(&self, id: &str);
}
