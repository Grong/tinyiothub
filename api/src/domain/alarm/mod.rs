/// 报警领域模块
///
/// 包含报警相关的业务逻辑：
/// - 报警规则判断
/// - 报警记录管理
/// - 报警状态跟踪
/// - 报警通知触发
#[cfg(test)]
mod tests;

pub mod entity;
pub mod errors;
pub mod handlers;
pub mod repository;
pub mod services;
pub mod specifications;
pub mod value_objects;

pub use entity::{Alarm, AlarmRule, RuleType};
pub use errors::{AlarmError, AlarmResult};
pub use handlers::AlarmEventHandler;
pub use repository::{
    AlarmQueryCriteria, AlarmRepository, AlarmRuleRepository, SortOrder, TimeRange,
};
pub use services::{AlarmService, AlarmStatistics};
pub use value_objects::*;
