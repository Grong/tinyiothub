//! Repository contracts — traits for data persistence.

pub mod device;

pub use device::{DeviceCriteria, DeviceCriteriaBuilder, DeviceRepository, DeviceSortBy, DeviceSortOrder};
