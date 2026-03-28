// [Comment removed due to encoding issues]
pub mod component;

pub mod device;

pub mod device_alarm;

pub mod device_alarm_rule;

pub mod device_command;

pub mod device_event_trigger;

pub mod device_property;

pub mod job;

pub mod menu;

pub mod message;

pub mod mqtt_info;

pub mod notification_channel;

pub mod opt_record;

pub mod organization;

pub mod permission;

pub mod product;

pub mod role;

pub mod role_permission;

pub mod tag;

pub mod user;

pub mod user_permission;

pub mod user_roles;

pub mod device_template;

pub mod template_error;

pub mod heartbeat;

pub mod self_healing;

// Re-export commonly used types (using actual struct names from the modules)

pub use device::Device;
pub use device_command::DeviceCommand;
pub use device_property::DeviceProperty;

pub mod alarm;

pub use alarm::{AlarmDto, AlarmRuleDto, AlarmStatisticsDto};

pub mod tenant;
