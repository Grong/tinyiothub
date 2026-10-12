// 值类型住 core（Task 1）；re-export 维持本模块路径组织。
pub use tinyiothub_core::agent_runs::{ActionRecord, ActionResult, Outcome, RunReport};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Low,
    Normal,
    High,
    Critical,
}

#[derive(Debug, Clone)]
pub enum TriggerSource {
    ThingEvent {
        thing_id: String,
        event_name: String,
        event_id: i64,
        level: i32,
        data: serde_json::Value,
    },
    Timer,
    /// 心跳巡检 tick(D11 专用变体——不复用 Timer:丢每工作区可配间隔,
    /// 且 dedup 与 timer:{ws} 碰撞吞 tick)。alarm=None=定时 tick;
    /// Some=告警驱动(优先级映射:Critical→Critical,High/Normal→Normal)。
    Heartbeat {
        alarm: Option<HeartbeatAlarmContext>,
    },
    UserDirective {
        user_id: String,
        text: String,
        session_key: Option<String>,
        source: Option<String>, // source: None=chat/API, Some("heartbeat")=X6 心跳桥
        /// X6 心跳桥投递的指令携带 problem_key（O11 dedup：run 完成时由
        /// manager 记入 RunRegistry problem 元数据，心跳桥据此抑制判定）；
        /// chat/API 用户指令为 None。
        problem_key: Option<String>,
    },
    /// Output of the scheduler's merge window (T8): every non-Critical signal
    /// sharing one `dedup_key` inside a 30s window, in arrival order. T9/T10
    /// prompt assembly recurses into `signals` to build the aggregated context.
    Merged {
        signals: Vec<WakeSignal>,
    },
}

/// 告警驱动心跳 run 的上下文（D11）：定时 tick 为 None；告警触发时携带
/// 命中的物/告警类型/规则与人读原因，`context` 为告警规则评估快照 JSON。
#[derive(Debug, Clone)]
pub struct HeartbeatAlarmContext {
    pub thing_id: Option<String>,
    pub alarm_type: Option<String>,
    pub rule_id: Option<String>,
    pub reason: String,
    pub context: String,
}

#[derive(Debug, Clone)]
pub struct WakeSignal {
    pub workspace_id: String,
    pub priority: Priority,
    pub source: TriggerSource,
    pub dedup_key: Option<String>, // 事件: thing:{id}:event:{name}; Timer: timer:{ws}; Heartbeat: heartbeat:{ws}(:{thing}:{alarm_type}); UserDirective: None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_ordering() {
        assert!(Priority::Critical > Priority::High);
        assert!(Priority::High > Priority::Normal);
        assert!(Priority::Normal > Priority::Low);
        assert!(Priority::Critical > Priority::Low);
    }
}
