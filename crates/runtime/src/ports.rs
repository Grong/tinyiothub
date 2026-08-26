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

use tinyiothub_core::models::device::Device;
use tinyiothub_core::models::device_command::DeviceCommand;

/// Read-side queries for device commands (cron `device_command` executor).
#[async_trait]
pub trait DeviceCommandQueries: Send + Sync {
    /// Find a command by device ID and command name.
    async fn find_by_device_and_name(&self, device_id: &str, name: &str) -> Result<Option<DeviceCommand>, String>;
}

/// Event-retention writes (cron `event_retention` executor).
#[async_trait]
pub trait EventRetentionStore: Send + Sync {
    /// Delete occurrence-type events (`is_status = 0`) with timestamp older
    /// than `cutoff_rfc3339`. Status rows (`is_status = 1`, live device
    /// state) are exempt. Returns rows deleted.
    async fn delete_occurrence_events_before(&self, cutoff_rfc3339: &str) -> Result<u64, String>;
}

/// Device cache used by `DataServer`. Sync because every call site is sync
/// (the backing implementation is an in-memory cache); making this async
/// would add `.await` noise with no benefit.
pub trait DeviceCacheSource: Send + Sync {
    fn all(&self) -> Vec<Device>;
    fn get(&self, id: &str) -> Option<Device>;
    fn get_by_name(&self, name: &str) -> Option<Device>;
    fn insert(&self, device: Device);
    fn update(&self, device: Device);
    fn remove(&self, id: &str);
}
