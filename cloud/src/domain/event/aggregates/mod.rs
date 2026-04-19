// Event Domain Aggregates
// Aggregate roots that encapsulate business logic and maintain consistency

pub mod event_aggregate;
pub mod notification_aggregate;

pub use event_aggregate::EventAggregate;
pub use notification_aggregate::{
    NotificationAggregate, NotificationChannelType, NotificationRecord, NotificationRule,
    NotificationStatus,
};
