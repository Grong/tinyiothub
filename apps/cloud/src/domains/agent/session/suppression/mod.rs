//! Generic source/revision/work-item suppression contract.
//!
//! The Harness only understands versions, canonical bytes and domain-separated
//! hashes here. Business semantics stay in Source adapters.

pub mod canonical;
pub mod contract;
pub mod error;
pub mod preflight;
pub mod registry;

pub use preflight::{PreflightDecision, SkipVeto, SuppressionService};
pub use registry::{AssistantSource, SourceContext, SourceRegistry, WorkspaceSnapshotProvider};
// R12: one workspace mode enum, owned by the durable storage layer.
pub use tinyiothub_storage::suppression::SuppressionMode;
