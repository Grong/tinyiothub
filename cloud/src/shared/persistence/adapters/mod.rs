//! Tenant-aware adapters for storage repositories.
//!
//! These adapters wrap the underlying storage repository implementations
//! and automatically add tenant/workspace filtering to enforce isolation.

pub mod device;
pub mod cron_job;
pub mod cron_run;