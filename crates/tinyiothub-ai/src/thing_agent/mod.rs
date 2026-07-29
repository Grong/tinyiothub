//! Thing-agent wake/run loop — types shared by trigger, scheduler, runner, policy.

pub mod traits;
pub mod trigger;
pub mod types;
pub use traits::*;
pub use types::*;
