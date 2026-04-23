pub mod alarm;
pub mod device_template;
pub mod heartbeat;
pub mod product;
pub mod self_healing;
pub mod tag;
pub mod template_error;

pub use alarm::{AlarmDto, AlarmRuleDto, AlarmStatisticsDto};

// SaaS entities - owned by cloud, re-exported from domain
pub use crate::domain::tenant;
pub use crate::domain::user;
pub use crate::domain::workspace;
pub use crate::domain::role;
pub use crate::domain::permission;

// IoT models from core (not SaaS)
pub use tinyiothub_core::models::device;
pub use tinyiothub_core::models::device_property;
pub use tinyiothub_core::models::component;
pub use tinyiothub_core::models::cron_job;
pub use tinyiothub_core::models::device_command;
pub use tinyiothub_core::models::job;
pub use tinyiothub_core::models::notification_channel::*;
