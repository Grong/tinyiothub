//! Session protocol v2 value types.
//!
//! The harness exposes one durable event vocabulary and one transient live
//! vocabulary. These types are the shared wire contract between the Rust
//! runtime, storage, and the Web client; they carry no I/O or orchestration.

use serde::{Deserialize, Serialize};

/// Wire version for the clean-slate event/live protocol.
pub const SESSION_PROTOCOL_VERSION: u8 = 2;

/// Durable session event type.
///
/// Names use the slash form so the durable log reads like the lifecycle:
/// `turn/start -> step/start -> tool/call -> assistant/message -> turn/end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SessionEventType {
    TurnStart,
    StepStart,
    ToolCall,
    ToolResult,
    AssistantMessage,
    AssistantAttempt,
    StepEnd,
    TurnEnd,
    TodoWrite,
    SkillLoaded,
    QuestionAsked,
    QuestionAnswered,
    ApprovalRequested,
    ApprovalResolved,
    SubagentStarted,
    SubagentCompleted,
    TriggerReceived,
    SessionStart,
    SessionEnd,
}

impl SessionEventType {
    pub const ALL: &'static [Self] = &[
        Self::TurnStart,
        Self::StepStart,
        Self::ToolCall,
        Self::ToolResult,
        Self::AssistantMessage,
        Self::AssistantAttempt,
        Self::StepEnd,
        Self::TurnEnd,
        Self::TodoWrite,
        Self::SkillLoaded,
        Self::QuestionAsked,
        Self::QuestionAnswered,
        Self::ApprovalRequested,
        Self::ApprovalResolved,
        Self::SubagentStarted,
        Self::SubagentCompleted,
        Self::TriggerReceived,
        Self::SessionStart,
        Self::SessionEnd,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TurnStart => "turn/start",
            Self::StepStart => "step/start",
            Self::ToolCall => "tool/call",
            Self::ToolResult => "tool/result",
            Self::AssistantMessage => "assistant/message",
            Self::AssistantAttempt => "assistant/attempt",
            Self::StepEnd => "step/end",
            Self::TurnEnd => "turn/end",
            Self::TodoWrite => "todo/write",
            Self::SkillLoaded => "skill/loaded",
            Self::QuestionAsked => "question/asked",
            Self::QuestionAnswered => "question/answered",
            Self::ApprovalRequested => "approval/requested",
            Self::ApprovalResolved => "approval/resolved",
            Self::SubagentStarted => "subagent/started",
            Self::SubagentCompleted => "subagent/completed",
            Self::TriggerReceived => "trigger/received",
            Self::SessionStart => "session/start",
            Self::SessionEnd => "session/end",
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|kind| kind.as_str() == value)
    }
}

/// One ordered segment of a model attempt. Consecutive same-kind deltas are
/// coalesced; token boundaries are deliberately not durable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamKind {
    Text,
    Reasoning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantStreamSegment {
    pub kind: StreamKind,
    /// Milliseconds since the attempt started.
    pub at_ms: u64,
    pub text: String,
}

/// Live turn phase, derived from the loop and never persisted per token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnPhase {
    Thinking,
    Requesting,
    Tool,
    WaitingUser,
    Verifying,
    Done,
    Retrying,
}

impl TurnPhase {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Thinking => "thinking",
            Self::Requesting => "requesting",
            Self::Tool => "tool",
            Self::WaitingUser => "waiting_user",
            Self::Verifying => "verifying",
            Self::Done => "done",
            Self::Retrying => "retrying",
        }
    }
}

/// Transient SSE frame kind. Durable frames use [`SessionEventType`] instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveFrameType {
    StreamStart,
    StreamDelta,
    StreamEnd,
    StreamSnapshot,
    Status,
    ToolProgress,
    Resync,
}

impl LiveFrameType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StreamStart => "stream/start",
            Self::StreamDelta => "stream/delta",
            Self::StreamEnd => "stream/end",
            Self::StreamSnapshot => "stream/snapshot",
            Self::Status => "status",
            Self::ToolProgress => "tool/progress",
            Self::Resync => "resync",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_event_type_has_a_unique_stable_wire_name() {
        let mut names = std::collections::HashSet::new();
        for kind in SessionEventType::ALL {
            assert!(names.insert(kind.as_str()), "duplicate {:?}", kind);
            assert_eq!(SessionEventType::from_wire(kind.as_str()), Some(*kind));
        }
        assert_eq!(SessionEventType::TurnStart.as_str(), "turn/start");
        assert_eq!(SessionEventType::AssistantMessage.as_str(), "assistant/message");
    }

    #[test]
    fn live_frame_names_are_stable() {
        assert_eq!(LiveFrameType::StreamDelta.as_str(), "stream/delta");
        assert_eq!(TurnPhase::WaitingUser.as_str(), "waiting_user");
    }

    #[test]
    fn assistant_stream_segment_serializes_camel_case() {
        let value = serde_json::to_value(AssistantStreamSegment {
            kind: StreamKind::Reasoning,
            at_ms: 12,
            text: "check".into(),
        })
        .expect("serialize segment");
        assert_eq!(value["kind"], "reasoning");
        assert_eq!(value["atMs"], 12);
    }
}
