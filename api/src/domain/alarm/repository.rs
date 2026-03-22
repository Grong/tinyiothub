use async_trait::async_trait;
use chrono::{DateTime, Utc};

use super::{
    entity::{Alarm, AlarmRule},
    errors::AlarmResult,
    value_objects::{AlarmLevel, AlarmStatus, AlarmType},
};

/// 报警仓储接口
#[async_trait]
pub trait AlarmRepository: Send + Sync {
    /// 创建报警
    async fn create(&self, alarm: &Alarm) -> AlarmResult<()>;

    /// 更新报警
    async fn update(&self, alarm: &Alarm) -> AlarmResult<()>;

    /// 根据ID查询
    async fn find_by_id(&self, id: &str) -> AlarmResult<Option<Alarm>>;

    /// 根据条件查询
    async fn find_by_criteria(&self, criteria: &AlarmQueryCriteria) -> AlarmResult<Vec<Alarm>>;

    /// 查询活跃报警
    async fn find_active(&self, device_id: Option<&str>) -> AlarmResult<Vec<Alarm>>;

    /// 查询未确认报警
    async fn find_unacknowledged(&self, device_id: Option<&str>) -> AlarmResult<Vec<Alarm>>;

    /// 统计报警数量
    async fn count_by_criteria(&self, criteria: &AlarmQueryCriteria) -> AlarmResult<u64>;

    /// 批量更新状态
    async fn batch_update_status(
        &self,
        alarm_ids: &[String],
        status: AlarmStatus,
    ) -> AlarmResult<usize>;

    /// 删除历史报警
    async fn delete_old_alarms(&self, before: DateTime<Utc>) -> AlarmResult<usize>;
}

/// 报警规则仓储接口
#[async_trait]
pub trait AlarmRuleRepository: Send + Sync {
    /// 创建规则
    async fn create(&self, rule: &AlarmRule) -> AlarmResult<()>;

    /// 更新规则
    async fn update(&self, rule: &AlarmRule) -> AlarmResult<()>;

    /// 删除规则
    async fn delete(&self, id: &str) -> AlarmResult<()>;

    /// 根据ID查询
    async fn find_by_id(&self, id: &str) -> AlarmResult<Option<AlarmRule>>;

    /// 查询所有启用的规则
    async fn find_enabled(&self) -> AlarmResult<Vec<AlarmRule>>;

    /// 根据设备查询规则
    async fn find_by_device(&self, device_id: &str) -> AlarmResult<Vec<AlarmRule>>;

    /// 根据属性查询规则
    async fn find_by_property(
        &self,
        device_id: &str,
        property_id: &str,
    ) -> AlarmResult<Vec<AlarmRule>>;

    /// 查询全局规则
    async fn find_global_rules(&self) -> AlarmResult<Vec<AlarmRule>>;

    /// 启用/禁用规则
    async fn set_enabled(&self, id: &str, enabled: bool) -> AlarmResult<()>;
}

/// 报警查询条件
#[derive(Debug, Clone, Default)]
pub struct AlarmQueryCriteria {
    pub device_ids: Option<Vec<String>>,
    pub property_ids: Option<Vec<String>>,
    pub alarm_levels: Option<Vec<AlarmLevel>>,
    pub alarm_types: Option<Vec<AlarmType>>,
    pub statuses: Option<Vec<AlarmStatus>>,
    pub time_range: Option<TimeRange>,
    pub sort_by: Option<String>,
    pub sort_order: Option<SortOrder>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// 时间范围
#[derive(Debug, Clone)]
pub struct TimeRange {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// 排序顺序
#[derive(Debug, Clone, Copy)]
pub enum SortOrder {
    Asc,
    Desc,
}
