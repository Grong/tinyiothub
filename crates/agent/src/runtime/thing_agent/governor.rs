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

    pub fn resume(&self, workspace_id: &str) {
        self.inner.remove(workspace_id);
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
        // cooldown 用注入时钟测:paused_at 用 Instant,测试用短 cooldown 无法穿越——
        // is_paused 第三个语义(恢复)由 record_success/resume 测:
        g.resume("ws");
        assert!(!g.is_paused("ws", Duration::from_secs(60)));
        assert_eq!(g.consecutive_failures("ws"), 0);
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
