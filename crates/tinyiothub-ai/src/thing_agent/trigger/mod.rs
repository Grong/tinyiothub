//! Trigger abstraction — sources that wake the thing-agent run loop.

use tokio::sync::mpsc;

use crate::thing_agent::types::WakeSignal;

pub mod thing_event;
pub mod timer;

pub use thing_event::ThingEventTrigger;
pub use timer::TimerTrigger;

/// A source of [`WakeSignal`]s. Implementations run until the channel closes
/// or the task is cancelled by the caller.
#[async_trait::async_trait]
pub trait Trigger: Send + Sync {
    /// Stable identifier of this trigger kind, e.g. `"timer"`.
    fn name(&self) -> &'static str;

    /// Emit wake signals into `tx` until the receiver is dropped.
    async fn run(&self, tx: mpsc::Sender<WakeSignal>) -> anyhow::Result<()>;
}
