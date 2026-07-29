//! Periodic timer trigger — emits one [`WakeSignal`] per interval.

use std::time::Duration;

use tokio::sync::mpsc;

use super::Trigger;
use crate::thing_agent::types::{Priority, TriggerSource, WakeSignal};

/// Emits a [`WakeSignal`] with `priority: Normal`, `source: Timer` and
/// `dedup_key: Some("timer:{workspace_id}")` every `interval`.
pub struct TimerTrigger {
    pub workspace_id: String,
    pub interval: Duration,
}

#[async_trait::async_trait]
impl Trigger for TimerTrigger {
    fn name(&self) -> &'static str {
        "timer"
    }

    async fn run(&self, tx: mpsc::Sender<WakeSignal>) -> anyhow::Result<()> {
        let mut ticker = tokio::time::interval(self.interval);
        loop {
            ticker.tick().await;
            let signal = WakeSignal {
                workspace_id: self.workspace_id.clone(),
                priority: Priority::Normal,
                source: TriggerSource::Timer,
                dedup_key: Some(format!("timer:{}", self.workspace_id)),
            };
            if tx.send(signal).await.is_err() {
                // Receiver dropped — agent loop shut down.
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{advance, pause};

    #[tokio::test]
    async fn emits_one_signal_per_interval_with_timer_dedup_key() {
        pause();

        let trigger = TimerTrigger {
            workspace_id: "ws_01".to_string(),
            interval: Duration::from_secs(60),
        };
        assert_eq!(trigger.name(), "timer");

        let (tx, mut rx) = mpsc::channel(16);
        let handle = tokio::spawn(async move { trigger.run(tx).await });

        // First tick of tokio::time::interval fires immediately.
        let first = rx.recv().await.expect("first tick fires immediately");

        advance(Duration::from_secs(120)).await;

        let second = rx.recv().await.expect("tick at +60s");
        let third = rx.recv().await.expect("tick at +120s");

        for signal in [&first, &second, &third] {
            assert_eq!(signal.workspace_id, "ws_01");
            assert_eq!(signal.priority, Priority::Normal);
            assert!(matches!(signal.source, TriggerSource::Timer));
            assert_eq!(signal.dedup_key.as_deref(), Some("timer:ws_01"));
        }

        // No fourth signal without advancing time further.
        assert!(rx.try_recv().is_err());

        drop(rx);
        handle.await.expect("run task").expect("run result");
    }

    #[tokio::test]
    async fn run_returns_ok_when_receiver_dropped() {
        pause();

        let trigger = TimerTrigger {
            workspace_id: "ws_02".to_string(),
            interval: Duration::from_secs(10),
        };
        let (tx, rx) = mpsc::channel(1);
        let handle = tokio::spawn(async move { trigger.run(tx).await });

        // Fill the channel, then drop the receiver while the trigger is
        // blocked on send.
        advance(Duration::from_secs(30)).await;
        drop(rx);

        handle.await.expect("run task").expect("run result");
    }
}
