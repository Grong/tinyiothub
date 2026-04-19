//! Device management engine.
//!
//! Contains device registry and device shadow implementations.
//!
//! TODO: Migrate from `cloud/src/domain/device/`.

pub mod registry;
pub mod shadow;

pub use registry::DeviceRegistry;
pub use shadow::DeviceShadow;
