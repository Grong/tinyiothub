//! Global HUD 的响应形状与状态判定（纯逻辑，无 I/O）。

use chrono::{DateTime, Utc};
use serde::Serialize;
use tinyiothub_core::harness::HarnessEvent;

pub const MAX_RECENT_EVENTS: i64 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HudStatusInputs<'a> {
    pub paused: bool,
    pub active_sessions: usize,
    pub pending_questions: usize,
    pub pending_approvals: i64,
    pub last_outcome: Option<&'a str>,
}

/// 优先级：paused > working > waiting_for_user > error > idle。
pub fn assistant_status(inputs: HudStatusInputs<'_>) -> &'static str {
    if inputs.paused {
        "paused"
    } else if inputs.active_sessions > 0 {
        "working"
    } else if inputs.pending_questions > 0 || inputs.pending_approvals > 0 {
        "waiting_for_user"
    } else if inputs.last_outcome == Some("failed") {
        "error"
    } else {
        "idle"
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HudResponse {
    pub assistant: HudAssistant,
    pub devices: HudDevices,
    pub alarms: HudAlarms,
    pub pending: HudPending,
    pub activity: HudActivity,
    pub schedule: HudSchedule,
    pub recent_events: Vec<HarnessEvent>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HudAssistant {
    pub status: &'static str,
    pub last_outcome: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HudDevices {
    pub total: i64,
    pub online: i64,
    pub offline: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HudAlarms {
    pub high_priority: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HudPending {
    pub questions: usize,
    pub approvals: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HudActivity {
    pub active_sessions: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HudSchedule {
    pub enabled: bool,
    pub interval_seconds: u32,
    pub next_wake_at: Option<DateTime<Utc>>,
    pub last_wake_at: Option<DateTime<Utc>>,
    pub last_outcome: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(
        paused: bool,
        active: usize,
        questions: usize,
        approvals: i64,
        last: Option<&str>,
    ) -> HudStatusInputs<'_> {
        HudStatusInputs {
            paused,
            active_sessions: active,
            pending_questions: questions,
            pending_approvals: approvals,
            last_outcome: last,
        }
    }

    #[test]
    fn paused_wins_over_everything() {
        assert_eq!(assistant_status(inputs(true, 2, 1, 1, Some("failed"))), "paused");
    }

    #[test]
    fn working_beats_waiting() {
        assert_eq!(assistant_status(inputs(false, 1, 1, 0, None)), "working");
    }

    #[test]
    fn waiting_when_blocked_on_user() {
        assert_eq!(assistant_status(inputs(false, 0, 1, 0, None)), "waiting_for_user");
        assert_eq!(assistant_status(inputs(false, 0, 0, 2, None)), "waiting_for_user");
    }

    #[test]
    fn error_only_when_last_wake_failed() {
        assert_eq!(assistant_status(inputs(false, 0, 0, 0, Some("failed"))), "error");
        assert_eq!(assistant_status(inputs(false, 0, 0, 0, Some("completed"))), "idle");
        assert_eq!(assistant_status(inputs(false, 0, 0, 0, None)), "idle");
    }
}
