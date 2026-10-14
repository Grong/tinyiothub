//! 周期心跳触发器:每工作区可配间隔(intervals 表每轮重读,
//! 对齐 interval_minutes 热改);enabled=false 抑制(终审 I-1,缺省 true);
//! policy mode=off 抑制(O6/O19);
//! governor 暂停抑制(连败治理);reload_notify 到达立即 tick
//! (行为保持清单第 3 行:ReloadTasks/ReloadConfig 写穿+立即 tick,
//! 对齐 loop_.rs:146-150);dedup heartbeat:{ws}。

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;

use super::Trigger;
use crate::runtime::thing_agent::governor::HeartbeatGovernor;
use crate::runtime::thing_agent::traits::AutonomyPolicyReader;
use crate::runtime::thing_agent::types::{Priority, TriggerSource, WakeSignal};
use tinyiothub_core::heartbeat::{HeartbeatTask, MIN_HEARTBEAT_INTERVAL_MINUTES};
use tinyiothub_policy::autonomy::AutonomyMode;

/// 周期心跳触发器:每工作区可配间隔(intervals 表每轮重读,
/// 对齐 interval_minutes 热改);policy mode=off 抑制(O6/O19);
/// governor 暂停抑制(连败治理);reload_notify 到达立即 tick
/// (行为保持清单第 3 行:ReloadTasks/ReloadConfig 写穿+立即 tick,
/// 对齐 loop_.rs:146-150);dedup heartbeat:{ws}。
pub struct HeartbeatTrigger {
    pub workspace_id: String,
    pub default_interval: Duration,
    /// 每工作区间隔(分钟)共享表——cloud 侧 update_config 写穿(D11-⑤ 写序)。
    pub intervals: Arc<dashmap::DashMap<String, u32>>,
    /// 每工作区开关共享表(终审 I-1):false 抑制定时 tick 与 reload_notify
    /// 立即 tick;缺省(表无条目)=true,对齐 HeartbeatConfig::default
    /// enabled:true。restore 预热自 DB heartbeat_config,update_config 写穿。
    pub enabled: Arc<dashmap::DashMap<String, bool>>,
    /// 任务表共享内存源(空任务/全暂停不 tick,对齐 loop_.rs:63)。
    pub tasks_source: Arc<dashmap::DashMap<String, Vec<HeartbeatTask>>>,
    pub policy_repo: Arc<dyn AutonomyPolicyReader>,
    pub governor: HeartbeatGovernor,
    /// 热更新信号:handler update_tasks/update_config 写穿 DB+内存表后 notify,
    /// trigger 立即醒来一次(数据已在共享表,notify 只承担"立即 tick")。
    pub reload_notify: Arc<tokio::sync::Notify>,
}

#[async_trait::async_trait]
impl Trigger for HeartbeatTrigger {
    fn name(&self) -> &'static str {
        "heartbeat"
    }

    async fn run(&self, tx: mpsc::Sender<WakeSignal>) -> anyhow::Result<()> {
        loop {
            // 抑制路径(空任务/paused)不发信号,走不到 send 失败分支;
            // 收端已关则直接退出,否则循环空转(暂停时钟下表现为挂死)。
            if tx.is_closed() {
                return Ok(());
            }
            // 空任务/全暂停 = 不 tick(loop_.rs:63 语义)
            let has_active_tasks = self
                .tasks_source
                .get(&self.workspace_id)
                .map(|ts| ts.iter().any(|t| !t.paused))
                .unwrap_or(false);

            // enabled 开关(终审 I-1):缺省 true(对齐 HeartbeatConfig::default
            // enabled:true);false 时定时 tick 与 notify 立即 tick 双双抑制
            // ——抑制在判定集,notify 只承担"立即醒",不醒出 tick。
            let enabled = self.enabled.get(&self.workspace_id).map(|v| *v).unwrap_or(true);

            // policy kill switch(O6/O19,fail-closed)
            let policy_ok = match self.policy_repo.load_autonomy(&self.workspace_id).await {
                Ok(Some(policy)) => policy.mode != AutonomyMode::Off,
                Ok(_) => false,
                Err(e) => {
                    tracing::warn!(workspace_id = %self.workspace_id, error = %e,
                        "autonomy policy read failed — skipping heartbeat tick");
                    false
                }
            };

            // 每轮重读间隔(interval_minutes 热改生效);clamp 下限对齐
            // runner.rs:404 / db validated(T5 评审结转,T6 补)。
            let interval = self
                .intervals
                .get(&self.workspace_id)
                .map(|m| Duration::from_secs(u64::from((*m).max(MIN_HEARTBEAT_INTERVAL_MINUTES)) * 60))
                .unwrap_or(self.default_interval);

            if enabled && has_active_tasks && policy_ok && !self.governor.is_paused(&self.workspace_id, interval) {
                let signal = WakeSignal {
                    workspace_id: self.workspace_id.clone(),
                    priority: Priority::Normal,
                    source: TriggerSource::Heartbeat { alarm: None },
                    dedup_key: Some(format!("heartbeat:{}", self.workspace_id)),
                };
                if tx.send(signal).await.is_err() {
                    return Ok(()); // receiver dropped — agent loop shut down
                }
            }
            // 等待下一轮:interval 到点 OR reload_notify 立即醒(热更新立即 tick,
            // 对齐 loop_.rs ReloadConfig"forces an immediate tick"语义)。
            tokio::select! {
                _ = tokio::time::sleep(interval) => {}
                _ = self.reload_notify.notified() => {
                    tracing::info!(workspace_id = %self.workspace_id, "heartbeat reload — immediate tick");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tokio::time::{advance, pause};

    use crate::runtime::thing_agent::traits::test_stubs::StubAutonomyPolicyReader;

    /// 内存桩(照 timer.rs tests):播种指定模式。
    fn stub_repo(ws: &str, mode: AutonomyMode) -> Arc<StubAutonomyPolicyReader> {
        Arc::new(StubAutonomyPolicyReader::with_policy(ws, mode))
    }

    fn seeded_tasks_source(ws: &str) -> Arc<dashmap::DashMap<String, Vec<HeartbeatTask>>> {
        let m = dashmap::DashMap::new();
        m.insert(
            ws.to_string(),
            vec![HeartbeatTask {
                id: 1,
                workspace_id: ws.into(),
                priority: "high".into(),
                text: "check devices".into(),
                paused: false,
                version: 1,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            }],
        );
        Arc::new(m)
    }

    #[tokio::test]
    async fn emits_heartbeat_signal_with_workspace_dedup_key() {
        pause();
        let repo = stub_repo("ws_01", AutonomyMode::Act);
        let governor = HeartbeatGovernor::default();
        let trigger = HeartbeatTrigger {
            workspace_id: "ws_01".into(),
            default_interval: Duration::from_secs(60),
            intervals: Arc::new(dashmap::DashMap::new()),
            enabled: Arc::new(dashmap::DashMap::new()), // 缺省=true
            tasks_source: seeded_tasks_source("ws_01"),
            policy_repo: repo,
            governor,
            reload_notify: Arc::new(tokio::sync::Notify::new()),
        };
        let (tx, mut rx) = mpsc::channel(16);
        let handle = tokio::spawn(async move { trigger.run(tx).await });

        // 启动即首 tick(对齐 loop_.rs:56-110 先执行后等待的顺序)
        let first = rx.recv().await.expect("startup tick fires");
        assert!(matches!(first.source, TriggerSource::Heartbeat { alarm: None }));
        assert_eq!(first.dedup_key.as_deref(), Some("heartbeat:ws_01"));
        assert_eq!(first.priority, Priority::Normal);

        drop(rx);
        handle.await.expect("run task").expect("run result");
    }

    #[tokio::test]
    async fn per_workspace_interval_override_wins_over_default() {
        pause();
        let repo = stub_repo("ws_02", AutonomyMode::Act);
        let intervals = Arc::new(dashmap::DashMap::new());
        intervals.insert("ws_02".to_string(), 5u32); // 300s 覆盖 default 3600s
        let trigger = HeartbeatTrigger {
            workspace_id: "ws_02".into(),
            default_interval: Duration::from_secs(3600),
            intervals,
            enabled: Arc::new(dashmap::DashMap::new()), // 缺省=true
            tasks_source: seeded_tasks_source("ws_02"),
            policy_repo: repo,
            governor: HeartbeatGovernor::default(),
            reload_notify: Arc::new(tokio::sync::Notify::new()),
        };
        let (tx, mut rx) = mpsc::channel(16);
        let handle = tokio::spawn(async move { trigger.run(tx).await });

        let first = rx.recv().await.expect("startup tick");
        assert_eq!(first.dedup_key.as_deref(), Some("heartbeat:ws_02"));

        // 步进泵(timer.rs:155-163 模式,暂停时钟下确定性强):
        // 前 299s 不得有第二 tick;累计 300s 必须到点。
        for i in 1..=299 {
            advance(Duration::from_secs(1)).await;
            tokio::task::yield_now().await;
            assert!(rx.try_recv().is_err(), "第 {i}s 不应有 tick(覆盖间隔=300s)");
        }
        // 累计 300s 到点;醒来到 send 之间有 await 链(policy load),
        // 单次 yield 未必跑完——步进泵兜底(上限远小于下一个 300s 周期,不会多吃 tick)。
        let mut second = None;
        for _ in 0..10 {
            advance(Duration::from_secs(1)).await;
            tokio::task::yield_now().await;
            if let Ok(s) = rx.try_recv() {
                second = Some(s);
                break;
            }
        }
        let second = second.expect("第 300s 到点出 tick");
        assert_eq!(second.dedup_key.as_deref(), Some("heartbeat:ws_02"));

        drop(rx);
        handle.await.expect("run task").expect("run result");
    }

    #[tokio::test]
    async fn paused_governor_suppresses_ticks_until_resume() {
        pause();
        let repo = stub_repo("ws_03", AutonomyMode::Act);
        let governor = HeartbeatGovernor::default();
        for _ in 0..5 {
            governor.record_failure("ws_03");
        }
        let trigger = HeartbeatTrigger {
            workspace_id: "ws_03".into(),
            default_interval: Duration::from_secs(60),
            intervals: Arc::new(dashmap::DashMap::new()),
            enabled: Arc::new(dashmap::DashMap::new()), // 缺省=true
            tasks_source: seeded_tasks_source("ws_03"),
            policy_repo: repo,
            governor: governor.clone(),
            reload_notify: Arc::new(tokio::sync::Notify::new()),
        };
        let (tx, mut rx) = mpsc::channel(16);
        let handle = tokio::spawn(async move { trigger.run(tx).await });

        // paused:启动检查与 +60s 检查都被抑制(泵 70s 无信号)
        let mut got = None;
        for _ in 0..70 {
            advance(Duration::from_secs(1)).await;
            tokio::task::yield_now().await;
            if let Ok(s) = rx.try_recv() {
                got = Some(s);
                break;
            }
        }
        assert!(got.is_none(), "paused 工作区不得发 tick");

        // resume 后下一轮(+120s 处)出信号
        governor.resume("ws_03");
        let mut got = None;
        for _ in 0..70 {
            advance(Duration::from_secs(1)).await;
            tokio::task::yield_now().await;
            if let Ok(s) = rx.try_recv() {
                got = Some(s);
                break;
            }
        }
        assert_eq!(
            got.expect("resume 后恢复 tick").dedup_key.as_deref(),
            Some("heartbeat:ws_03")
        );

        drop(rx);
        handle.await.expect("run task").expect("run result");
    }

    // 空任务抑制(对齐 loop_.rs:63 空任务列表不 tick)
    #[tokio::test]
    async fn empty_or_all_paused_tasks_emit_nothing() {
        pause();
        let repo = stub_repo("ws_04", AutonomyMode::Act);
        let trigger = HeartbeatTrigger {
            workspace_id: "ws_04".into(),
            default_interval: Duration::from_secs(60),
            intervals: Arc::new(dashmap::DashMap::new()),
            enabled: Arc::new(dashmap::DashMap::new()),      // 缺省=true
            tasks_source: Arc::new(dashmap::DashMap::new()), // 无任务
            policy_repo: repo,
            governor: HeartbeatGovernor::default(),
            reload_notify: Arc::new(tokio::sync::Notify::new()),
        };
        let (tx, mut rx) = mpsc::channel(16);
        let handle = tokio::spawn(async move { trigger.run(tx).await });
        for _ in 0..70 {
            advance(Duration::from_secs(1)).await;
            tokio::task::yield_now().await;
            assert!(rx.try_recv().is_err(), "空任务工作区不发 tick(loop_.rs:63)");
        }
        drop(rx);
        handle.await.expect("run task").expect("run result");
    }

    // 热更新测试(行为保持清单第 3 行)
    #[tokio::test]
    async fn reload_notify_forces_immediate_tick() {
        pause();
        let repo = stub_repo("ws_05", AutonomyMode::Act);
        let reload = Arc::new(tokio::sync::Notify::new());
        let trigger = HeartbeatTrigger {
            workspace_id: "ws_05".into(),
            default_interval: Duration::from_secs(3600), // 长间隔:不靠到点
            intervals: Arc::new(dashmap::DashMap::new()),
            enabled: Arc::new(dashmap::DashMap::new()), // 缺省=true
            tasks_source: seeded_tasks_source("ws_05"),
            policy_repo: repo,
            governor: HeartbeatGovernor::default(),
            reload_notify: reload.clone(),
        };
        let (tx, mut rx) = mpsc::channel(16);
        let handle = tokio::spawn(async move { trigger.run(tx).await });

        let _first = rx.recv().await.expect("startup tick");
        reload.notify_one(); // update_config/update_tasks 写穿后调用
        let second = rx.recv().await.expect("notify 立即 tick,不等 3600s");
        assert_eq!(second.dedup_key.as_deref(), Some("heartbeat:ws_05"));

        drop(rx);
        handle.await.expect("run task").expect("run result");
    }

    // 终审 I-1：enabled=false 对统一循环生效——定时 tick 与 reload_notify
    // 立即 tick 双双抑制;翻回 true 恢复。缺省(表无条目)=true,对齐
    // HeartbeatConfig::default enabled:true。
    #[tokio::test]
    async fn disabled_workspace_suppresses_ticks_until_re_enabled() {
        pause();
        let repo = stub_repo("ws_06", AutonomyMode::Act);
        let enabled = Arc::new(dashmap::DashMap::new());
        enabled.insert("ws_06".to_string(), false); // update_config(enabled=false) 写穿
        let reload = Arc::new(tokio::sync::Notify::new());
        let trigger = HeartbeatTrigger {
            workspace_id: "ws_06".into(),
            default_interval: Duration::from_secs(60),
            intervals: Arc::new(dashmap::DashMap::new()),
            enabled: enabled.clone(),
            tasks_source: seeded_tasks_source("ws_06"),
            policy_repo: repo,
            governor: HeartbeatGovernor::default(),
            reload_notify: reload.clone(),
        };
        let (tx, mut rx) = mpsc::channel(16);
        let handle = tokio::spawn(async move { trigger.run(tx).await });

        // 禁用:启动检查与 +60s 到点检查都被抑制(泵 70s 无信号)。
        for _ in 0..70 {
            advance(Duration::from_secs(1)).await;
            tokio::task::yield_now().await;
            assert!(rx.try_recv().is_err(), "enabled=false 不得发 tick");
        }
        // notify 立即 tick 同样被抑制(update_config(false) 写穿后 handler
        // 仍会 notify_one——抑制在判定集,不在 notify)。
        reload.notify_one();
        for _ in 0..10 {
            tokio::task::yield_now().await;
            assert!(rx.try_recv().is_err(), "enabled=false 时 notify 不得补 tick");
        }

        // 翻回 true:notify 立即 tick 恢复。
        enabled.insert("ws_06".to_string(), true);
        reload.notify_one();
        let tick = rx.recv().await.expect("enabled=true 后 notify 立即 tick");
        assert_eq!(tick.dedup_key.as_deref(), Some("heartbeat:ws_06"));

        drop(rx);
        handle.await.expect("run task").expect("run result");
    }
}
