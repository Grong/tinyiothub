// 行类型已迁 db（E6b）；re-export 兼容。
pub use tinyiothub_storage::agent_runs::{ActionRecord, ActionResult, Outcome, RunReport};

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
    UserDirective {
        user_id: String,
        text: String,
        session_key: Option<String>,
        source: Option<String>, // source: None=chat/API, Some("heartbeat")=X6 心跳桥
        /// X6 心跳桥投递的指令携带 problem_key（O11 dedup：run 落库后供
        /// `last_problem_run`/`count_problem_runs` 抑制判定）；chat/API
        /// 用户指令为 None。
        problem_key: Option<String>,
    },
    /// Output of the scheduler's merge window (T8): every non-Critical signal
    /// sharing one `dedup_key` inside a 30s window, in arrival order. T9/T10
    /// prompt assembly recurses into `signals` to build the aggregated context.
    Merged {
        signals: Vec<WakeSignal>,
    },
}

#[derive(Debug, Clone)]
pub struct WakeSignal {
    pub workspace_id: String,
    pub priority: Priority,
    pub source: TriggerSource,
    pub dedup_key: Option<String>, // 事件: thing:{id}:event:{name}; Timer: timer:{ws}; UserDirective: None
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
