//! Alarm management engine.
//!
//! TODO: Migrate from `cloud/src/domain/alarm/`.

pub mod manager;
pub mod trigger;

pub use manager::AlarmManager;
pub use trigger::AlarmTrigger;
