//! Periodic timer trigger — emits one [`WakeSignal`] per interval.
//!
//! Policy gating (O6/O19): the autonomy policy is re-read before every
//! emission; `mode = off` (or no policy row) suppresses the signal so an
//! off workspace pays zero LLM cost from the periodic patrol. A policy
//! read error fail-closes — no signal.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;

use super::Trigger;
use crate::domains::agent::loop_::thing_agent::types::{Priority, TriggerSource, WakeSignal};
use tinyiothub_policy::autonomy::AutonomyMode;
use tinyiothub_storage::policy::PolicyRepository;

/// Emits a [`WakeSignal`] with `priority: Normal`, `source: Timer` and
/// `dedup_key: Some("timer:{workspace_id}")` every `interval`, unless the
/// workspace autonomy mode is off.
pub struct TimerTrigger {
    pub workspace_id: String,
    pub interval: Duration,
    pub policy_repo: Arc<PolicyRepository>,
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
            // Re-read per tick so the kill switch takes effect without a
            // trigger restart; mode=off / no policy → zero LLM cost (O6/O19).
            match self.policy_repo.load_autonomy(&self.workspace_id).await {
                Ok(Some(policy)) if policy.mode != AutonomyMode::Off => {}
                Ok(_) => continue,
                Err(e) => {
                    // Fail-closed: a policy read error must not wake the loop.
                    tracing::warn!(
                        workspace_id = %self.workspace_id,
                        error = %e,
                        "autonomy policy read failed — skipping timer signal"
                    );
                    continue;
                }
            }
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

    use tinyiothub_policy::autonomy::{AutonomyMode, AutonomyPolicy};
    use tinyiothub_storage::policy::PolicyRepository;

    fn policy(mode: AutonomyMode) -> AutonomyPolicy {
        AutonomyPolicy {
            mode,
            allowed_actions: vec!["*".to_string()],
            denied_actions: vec![],
            max_actions_per_run: 10,
            max_actions_per_hour: 100,
        }
    }

    /// 真实 SQLite 版 policy repo（E3 去 trait 后替代 StubPolicyRepo）：
    /// 内存库跑全量迁移 + save_autonomy 播种指定模式。
    async fn real_repo(ws: &str, mode: AutonomyMode) -> Arc<PolicyRepository> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .acquire_timeout(std::time::Duration::from_secs(86400 * 365)) // 暂停时钟防瞬杀
            .connect(":memory:")
            .await
            .expect("in-memory sqlite");
        tinyiothub_storage::test_helpers::run_all_migrations(&pool)
            .await
            .expect("migrations");
        let repo = Arc::new(PolicyRepository::new(pool));
        repo.save_autonomy(ws, &policy(mode), "test")
            .await
            .expect("seed policy");
        repo
    }

    /// 无 workspace_autonomy_policy 表的空库 —— load_autonomy 必失败，
    /// 等效原 StubPolicyRepo::set_fail(true) 的故障注入。
    async fn broken_repo() -> Arc<PolicyRepository> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .acquire_timeout(std::time::Duration::from_secs(86400 * 365))
            .connect(":memory:")
            .await
            .expect("empty pool");
        Arc::new(PolicyRepository::new(pool))
    }

    fn spawn_trigger(
        workspace_id: &str,
        interval: Duration,
        repo: Arc<PolicyRepository>,
    ) -> (mpsc::Receiver<WakeSignal>, tokio::task::JoinHandle<anyhow::Result<()>>) {
        let trigger = TimerTrigger {
            workspace_id: workspace_id.to_string(),
            interval,
            policy_repo: repo,
        };
        assert_eq!(trigger.name(), "timer");
        let (tx, rx) = mpsc::channel(16);
        let handle = tokio::spawn(async move { trigger.run(tx).await });
        (rx, handle)
    }

    #[tokio::test]
    async fn emits_one_signal_per_interval_with_timer_dedup_key() {
        pause();

        let repo = real_repo("ws_01", AutonomyMode::Act).await;
        let (mut rx, handle) = spawn_trigger("ws_01", Duration::from_secs(60), repo);

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

    // mode=off → zero signals (O19), and the policy is re-read per tick.
    //
    // Under a paused clock, timer ticks fire only while the test task is
    // parked (the idle runtime auto-advances to the next timer), so the
    // test parks on `timeout(recv)` instead of yield-polling: the runtime
    // then runs several 60s ticks, each gated on the policy.
    #[tokio::test]
    async fn mode_off_emits_zero_signals_until_policy_changes() {
        pause();

        let repo = real_repo("ws_off", AutonomyMode::Off).await;
        let (mut rx, handle) = spawn_trigger("ws_off", Duration::from_secs(60), repo.clone());

        // 180s parked → ~3 ticks fire, all gated on Off → not one signal.
        let res = tokio::time::timeout(Duration::from_secs(180), rx.recv()).await;
        assert!(res.is_err(), "mode=off must emit zero signals (O19)");

        // Flip the kill switch on; the next tick must emit — proving the
        // ticks above produced nothing and mode is re-read per tick.
        repo.save_autonomy("ws_off", &policy(AutonomyMode::Act), "test")
            .await
            .expect("flip policy");
        // 同 policy_read_failure：步进泵替代长 timeout，防暂停时钟空转假失败。
        let mut signal = None;
        for _ in 0..240 {
            tokio::time::advance(Duration::from_secs(1)).await;
            tokio::task::yield_now().await;
            if let Ok(sig) = rx.try_recv() {
                signal = Some(sig);
                break;
            }
        }
        let signal = signal.expect("signal after policy flip");
        assert_eq!(signal.workspace_id, "ws_off");
        assert_eq!(signal.dedup_key.as_deref(), Some("timer:ws_off"));

        drop(rx);
        handle.await.expect("run task").expect("run result");
    }

    // mode=diagnose → normal emission (same as act; only off is suppressed).
    #[tokio::test]
    async fn diagnose_mode_emits_signals() {
        pause();

        let repo = real_repo("ws_diag", AutonomyMode::Diagnose).await;
        let (mut rx, handle) = spawn_trigger("ws_diag", Duration::from_secs(60), repo);

        let first = rx.recv().await.expect("first tick fires immediately");
        assert_eq!(first.priority, Priority::Normal);
        assert!(matches!(first.source, TriggerSource::Timer));

        advance(Duration::from_secs(60)).await;
        let second = rx.recv().await.expect("tick at +60s");
        assert_eq!(second.workspace_id, "ws_diag");
        assert_eq!(second.dedup_key.as_deref(), Some("timer:ws_diag"));

        drop(rx);
        handle.await.expect("run task").expect("run result");
    }

    // Policy read failure → fail-closed, no signal; recovers once reads work.
    #[tokio::test]
    async fn policy_read_failure_emits_no_signals() {
        pause();

        let repo = broken_repo().await;
        let (mut rx, handle) = spawn_trigger("ws_err", Duration::from_secs(60), repo);

        // Several ticks fire while reads fail; none may produce a signal.
        let res = tokio::time::timeout(Duration::from_secs(180), rx.recv()).await;
        assert!(res.is_err(), "policy read failure must fail-closed");

        // 故障恢复：换一个可用的 repo 重启 trigger（真实库无法中途恢复闭合的连接）。
        drop(rx);
        handle.abort();
        let repo = real_repo("ws_err", AutonomyMode::Act).await;
        let (mut rx, handle) = spawn_trigger("ws_err", Duration::from_secs(60), repo);
        // 暂停时钟下 park 在长 timeout 上会让运行时在 trigger 的真实 I/O 完成前
        // 空转跳进 120s（Elapsed 假失败）；改为 1s 步进 + try_recv 泵，确定性强。
        let mut signal = None;
        for _ in 0..240 {
            tokio::time::advance(Duration::from_secs(1)).await;
            tokio::task::yield_now().await;
            if let Ok(s) = rx.try_recv() {
                signal = Some(s);
                break;
            }
        }
        let signal = signal.expect("signal after recovery");
        assert_eq!(signal.workspace_id, "ws_err");

        drop(rx);
        handle.await.expect("run task").expect("run result");
    }

    #[tokio::test]
    async fn run_returns_ok_when_receiver_dropped() {
        pause();

        let repo = real_repo("ws_02", AutonomyMode::Act).await;
        let trigger = TimerTrigger {
            workspace_id: "ws_02".to_string(),
            interval: Duration::from_secs(10),
            policy_repo: repo,
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
