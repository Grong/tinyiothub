// Self-healing module — 3-layer architecture
// types: entity types (SeverityLevel, RecoveryAction, ProbeResult, etc.)
// repo:  HealingExecutionRepository
// service: PolicyEvaluator + ActionExecutor
// errors: SelfHealingError
// handler: HTTP routes (delegates to api/self_healing/)

pub mod types;
pub mod errors;
pub mod repo;
pub mod service;
pub mod probe_scheduler;
pub mod handler;

pub use probe_scheduler::ProbeScheduler;

pub use types::*;
pub use errors::*;
pub use repo::*;
pub use service::*;
pub use handler::*;
