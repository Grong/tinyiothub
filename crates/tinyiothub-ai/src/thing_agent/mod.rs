//! Thing-agent wake/run loop — types shared by trigger, scheduler, runner, policy.

pub mod manager;
pub mod prompt;
pub mod pushback;
pub mod report;
pub mod runner;
pub mod scheduler;
pub mod traits;
pub mod trigger;
pub mod types;
pub use manager::{AutonomousAgentProvider, ThingAgentManager, ThingAgentManagerConfig};
pub use report::*;
pub use runner::{AgentHandle, RunContext, RunContextInner, RunOutcome, Runner, ToolTraceEntry, TruncationReason};
pub use scheduler::{EnqueueError, Scheduler, SchedulerHandle};
pub use traits::*;
pub use types::*;
