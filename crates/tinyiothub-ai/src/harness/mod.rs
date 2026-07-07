//! AI Harness Loop — 6-stage unified execution pipeline.
//!
//! All AI execution paths (Chat, Heartbeat, Workspace) flow through harness.
//! The harness provides PreToolUse checks, PostToolUse verification,
//! lie detection, and structured reporting.
//!
//! Pipeline stages: Wake → Load Context → Plan → Execute → Verify → Report → Sleep

pub mod execute;
pub mod orchestrator;
pub mod plan;
pub mod types;
pub mod verify;

pub use types::*;
