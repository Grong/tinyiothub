//! TinyIoTHub observability primitives
//!
//! Provides metric types (Counter, Gauge, Histogram), trace spans,
//! and a registry for collecting and exporting telemetry.
//!
//! Design goal: zero framework dependencies. Integration with `tracing`
//! or external exporters is opt-in via feature flags.

pub mod meter;
pub mod registry;
pub mod trace;

pub use meter::{Counter, Gauge, Histogram, MetricValue};
pub use registry::{MetricRegistry, RegistryError};
pub use trace::{Span, SpanContext, SpanStatus};
