//! Thing-agent wake/run loop — types shared by trigger, scheduler, runner, policy.

pub mod active_runs;
pub mod arbitration;
pub mod governor;
pub mod manager;
pub mod pushback;
pub mod registry;
pub mod runner;
pub mod scheduler;
pub mod seams;
pub mod traits;
pub mod trigger;
pub mod types;
pub mod watch_loop;
pub use active_runs::{ActiveRunGuard, ActiveRunRegistry};
pub use arbitration::{
    ArbitrationRegistry, Busy, DEFAULT_LEASE_TTL, DEVICE_RESOURCE, Lease, ResourceLease, ResourceMode, ResourceSpec,
};
pub use manager::{AutonomousAgentProvider, HeartbeatDeps, ThingAgentManager, ThingAgentManagerConfig};
pub use runner::{AgentHandle, RunContext, RunContextInner, RunOutcome, Runner, ToolTraceEntry, TruncationReason};
pub use scheduler::{EnqueueError, Scheduler, SchedulerHandle};
pub use traits::*;
pub use types::*;
pub use watch_loop::{WatchCommand, WatchDeps};
