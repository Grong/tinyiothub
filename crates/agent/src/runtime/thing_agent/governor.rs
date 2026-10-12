//! 心跳连败治理(行为保持清单第 1 行):heartbeat 源 run 连败达阈暂停
/// 该工作区定时触发;cooldown 或告警信号恢复。原 heartbeat/loop_.rs:81-105
/// 语义的统一循环落点——暂停的是"定时触发",告警入口(桥)不受影响。
use std::sync::Arc;
use std::time::{Duration, Instant};

const MAX_CONSECUTIVE_FAILURES: u32 = 5;

#[derive(Clone, Default)]
pub struct HeartbeatGovernor {
    inner: Arc<dashmap::DashMap<String, GovernorState>>,
}

#[derive(Default)]
struct GovernorState {
    failures: u32,
    paused_at: Option<Instant>,
}

impl HeartbeatGovernor {
    pub fn record_success(&self, workspace_id: &str) {
        self.inner.remove(workspace_id);
    }

    pub fn record_failure(&self, workspace_id: &str) {
        let mut e = self.inner.entry(workspace_id.to_string()).or_default();
        e.failures += 1;
        if e.failures >= MAX_CONSECUTIVE_FAILURES && e.paused_at.is_none() {
            e.paused_at = Some(Instant::now());
        }
    }

    pub fn is_paused(&self, workspace_id: &str, cooldown: Duration) -> bool {
        let Some(e) = self.inner.get(workspace_id) else { return false };
        match e.paused_at {
            None => false,
            Some(at) if at.elapsed() >= cooldown => {
                drop(e);
                // cooldown 到点自动恢复(对齐 loop_.rs:160-168 的 interval 冷却)
                self.inner.remove(workspace_id);
                false
            }
            Some(_) => true,
        }
    }

    /// 告警信号/外部唤醒恢复——仅暂停时清态(对齐 loop_.rs:128-133:外部信号
    /// 仅在 paused 时清连败);未暂停时调用是 no-op,不动连败计数。
    pub fn resume(&self, workspace_id: &str) {
        let paused = self
            .inner
            .get(workspace_id)
            .map(|e| e.paused_at.is_some())
            .unwrap_or(false);
        if paused {
            self.inner.remove(workspace_id);
        }
    }

    pub fn consecutive_failures(&self, workspace_id: &str) -> u32 {
        self.inner.get(workspace_id).map(|e| e.failures).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn five_consecutive_failures_pause_then_cooldown_resumes() {
        let g = HeartbeatGovernor::default();
        for _ in 0..4 {
            g.record_failure("ws");
        }
        assert!(!g.is_paused("ws", Duration::from_secs(60)), "4 连败未达阈");
        g.record_failure("ws");
        assert!(g.is_paused("ws", Duration::from_secs(60)), "5 连败暂停");
        assert!(g.is_paused("ws", Duration::from_secs(60)));
        // Duration::ZERO 恒过 cooldown=覆盖自动恢复分支:
        assert!(!g.is_paused("ws", Duration::ZERO));
        assert_eq!(g.consecutive_failures("ws"), 0, "cooldown 到点自动清态");
        // 恢复路径再由 record_failure/resume 测:
        for _ in 0..5 {
            g.record_failure("ws");
        }
        assert!(g.is_paused("ws", Duration::from_secs(60)));
        g.resume("ws");
        assert!(!g.is_paused("ws", Duration::from_secs(60)));
        assert_eq!(g.consecutive_failures("ws"), 0);
    }

    #[test]
    fn resume_when_not_paused_keeps_failure_count() {
        let g = HeartbeatGovernor::default();
        for _ in 0..4 {
            g.record_failure("ws");
        }
        g.resume("ws");
        assert_eq!(g.consecutive_failures("ws"), 4, "未暂停时 resume 是 no-op");
        g.record_failure("ws");
        assert!(g.is_paused("ws", Duration::from_secs(60)), "连败计数续跑到阈值");
    }

    #[test]
    fn success_resets_failure_count() {
        let g = HeartbeatGovernor::default();
        for _ in 0..4 {
            g.record_failure("ws");
        }
        g.record_success("ws");
        g.record_failure("ws");
        assert!(!g.is_paused("ws", Duration::from_secs(60)), "成功清零后重新计数");
    }
}
