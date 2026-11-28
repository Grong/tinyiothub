//! Thing-agent wake/run loop — types shared by trigger, runner, policy.

pub mod active_runs;
pub mod approval_port;
/// E4c arbitration registry now lives in `runtime` (pure runtime logic, shared
/// with the acceptance profiles). This module is a path-compatible facade.
pub mod arbitration {
    pub use tinyiothub_runtime::arbitration::*;
}
pub mod governor;
pub mod manager;
pub mod registry;
pub mod runner;
pub mod seams;
pub mod task;
pub mod traits;
pub mod types;
pub mod watch_loop;
pub use active_runs::{ActiveRunGuard, ActiveRunRegistry};
pub use arbitration::{
    ArbitrationRegistry, Busy, DEFAULT_LEASE_TTL, DEVICE_RESOURCE, Lease, ResourceLease, ResourceMode, ResourceSpec,
};
pub use manager::{AutonomousAgentProvider, HeartbeatDeps, ThingAgentManager};
pub use runner::{AgentHandle, RunContext, RunContextInner, RunOutcome, Runner, ToolTraceEntry, TruncationReason};
pub use traits::*;
pub use types::*;
pub use watch_loop::{WatchCommand, WatchDeps};
