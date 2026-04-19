//! Data processing pipeline — decodes, transforms, and routes telemetry.
//!
//! TODO: Migrate from `cloud/src/domain/device/driver/` pipeline logic.

pub mod decoder;
pub mod router;
pub mod transformer;

pub use decoder::ProtocolDecoder;
pub use router::DataRouter;
pub use transformer::DataTransformer;
