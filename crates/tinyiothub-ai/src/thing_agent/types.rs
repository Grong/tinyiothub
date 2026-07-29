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
        source: Option<String>, // source: None=chat/API, Some("heartbeat:{tick}")
    },
}

#[derive(Debug, Clone)]
pub struct WakeSignal {
    pub workspace_id: String,
    pub priority: Priority,
    pub source: TriggerSource,
    pub dedup_key: Option<String>, // 事件: thing:{id}:event:{name}; Timer: timer:{ws}; UserDirective: None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Acted,
    NoActionNeeded,
    Failed,
    BudgetExceeded,
    Rejected,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ActionRecord {
    pub thing_id: String,
    pub action_name: String,
    pub params: serde_json::Value,
    pub result: ActionResult,
    pub verified: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionResult {
    Success(serde_json::Value),
    Failed(String),
    UnknownCancelled,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RunReport {
    pub run_id: String,
    pub workspace_id: String,
    pub trigger: String, // TriggerSource 的序列化
    pub outcome: Outcome,
    pub summary: String,
    pub actions: Vec<ActionRecord>,
    pub verified: bool,
    pub duration_ms: u64,
    pub tool_calls: u32,
    pub tokens: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_report_json_round_trip() {
        let report = RunReport {
            run_id: "run_01".to_string(),
            workspace_id: "ws_01".to_string(),
            trigger: "thing:t1:event:temp_high".to_string(),
            outcome: Outcome::Acted,
            summary: "cooled down".to_string(),
            actions: vec![
                ActionRecord {
                    thing_id: "t1".to_string(),
                    action_name: "set_fan".to_string(),
                    params: serde_json::json!({"speed": 3}),
                    result: ActionResult::Success(serde_json::json!({"ok": true})),
                    verified: true,
                },
                ActionRecord {
                    thing_id: "t2".to_string(),
                    action_name: "reboot".to_string(),
                    params: serde_json::json!({}),
                    result: ActionResult::Failed("timeout".to_string()),
                    verified: false,
                },
                ActionRecord {
                    thing_id: "t3".to_string(),
                    action_name: "poll".to_string(),
                    params: serde_json::Value::Null,
                    result: ActionResult::UnknownCancelled,
                    verified: false,
                },
            ],
            verified: true,
            duration_ms: 1234,
            tool_calls: 5,
            tokens: 6789,
        };

        let json = serde_json::to_string(&report).expect("serialize");
        let back: RunReport = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(back.run_id, report.run_id);
        assert_eq!(back.workspace_id, report.workspace_id);
        assert_eq!(back.trigger, report.trigger);
        assert_eq!(back.outcome, report.outcome);
        assert_eq!(back.summary, report.summary);
        assert_eq!(back.actions.len(), 3);
        assert_eq!(back.actions[0].thing_id, "t1");
        assert_eq!(back.actions[0].action_name, "set_fan");
        assert!(back.actions[0].verified);
        assert_eq!(back.verified, report.verified);
        assert_eq!(back.duration_ms, report.duration_ms);
        assert_eq!(back.tool_calls, report.tool_calls);
        assert_eq!(back.tokens, report.tokens);
    }

    #[test]
    fn priority_ordering() {
        assert!(Priority::Critical > Priority::Low);
        assert!(Priority::Critical > Priority::High);
        assert!(Priority::High > Priority::Normal);
        assert!(Priority::Normal > Priority::Low);
    }

    #[test]
    fn outcome_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&Outcome::BudgetExceeded).expect("serialize"),
            "\"budget_exceeded\""
        );
        assert_eq!(
            serde_json::to_string(&Outcome::NoActionNeeded).expect("serialize"),
            "\"no_action_needed\""
        );
        assert_eq!(serde_json::to_string(&Outcome::Acted).expect("serialize"), "\"acted\"");
    }
}
