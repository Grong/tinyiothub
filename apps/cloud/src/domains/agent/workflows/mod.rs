//! Retained Harness supervisor and compaction subsystems.
//!
//! The Workflow experiment (definition CRUD, templates, dry-run, and the
//! scheduler/runner) was retired. Supervisor health, projection compaction,
//! and the Task/Effect/Decision runtime remain.

pub mod compaction;
pub mod health;
pub mod projection_contract;

pub use compaction::{
    CompareBarrier, ProjectionComparison, ProjectionComparisonStatus, ProjectionDifference, ProjectionEvidence,
    ProjectionInspector,
};
pub use health::{
    DEFAULT_DUE_GRACE, DEFAULT_TICK_DEADLINE, DeadWakeSummary, RetryOutcome, StalledReason, SupervisorHealth,
    SupervisorHealthSnapshot, SupervisorStatus,
};
