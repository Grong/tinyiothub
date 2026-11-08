//! Policy 领域值类型：工作区自治策略 + 行动提案（自 db/policy.rs 归位，Task 1）。
//!
//! AutonomyPolicy/PermissionMode/Proposal/ProposalStatus 为共享值类型；
//! 全部 SQL（含 agent_runs 动作频率读取）留在 db，经 Db 门面委托暴露。

use serde::{Deserialize, Serialize};

/// 会话/工作区的权限模式。
///
/// 值类型住 core：既被 cloud 的 Session/TurnContext 使用，也被
/// `AutonomyPolicy` 与 `assistant_schedules` 持久化读取。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    ReadOnly,
    #[default]
    Ask,
    FullAccess,
}

impl PermissionMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::Ask => "ask",
            Self::FullAccess => "full_access",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "read_only" => Some(Self::ReadOnly),
            "ask" => Some(Self::Ask),
            "full_access" => Some(Self::FullAccess),
            _ => None,
        }
    }
}

/// Workspace-level autonomy policy for the thing-agent loop.
#[derive(Debug, Clone)]
pub struct AutonomyPolicy {
    pub permission_mode: PermissionMode,
    /// Allowed action names; `["*"]` means all actions.
    pub allowed_actions: Vec<String>,
    /// Denied action names (exact match); checked before the allowlist.
    pub denied_actions: Vec<String>,
    pub max_actions_per_run: u32,
    pub max_actions_per_hour: u32,
}

impl AutonomyPolicy {
    /// Parse a persisted policy JSON blob.
    ///
    /// Accepts both the new `permission_mode` key and the legacy three-state
    /// `mode` key (`off`/`diagnose` → `ReadOnly`, `act` → `Ask`). Missing
    /// limits and lists default to fail-closed zeros/empty vectors.
    pub fn from_db_json(json: &str) -> Option<Self> {
        let raw: serde_json::Value = serde_json::from_str(json).ok()?;
        let permission_mode = match raw.get("permission_mode").and_then(|v| v.as_str()) {
            Some("read_only") => PermissionMode::ReadOnly,
            // 旧数据不允许 workspace 级 full_access：降级为 Ask（不放大权限）。
            Some("full_access") => PermissionMode::Ask,
            Some("ask") => PermissionMode::Ask,
            _ => match raw.get("mode").and_then(|v| v.as_str()) {
                Some("diagnose") => PermissionMode::ReadOnly,
                Some("off") => PermissionMode::ReadOnly,
                _ => PermissionMode::Ask,
            },
        };
        Some(Self {
            permission_mode,
            allowed_actions: raw
                .get("allowed_actions")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            denied_actions: raw
                .get("denied_actions")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            max_actions_per_run: json_u32(&raw, "max_actions_per_run"),
            max_actions_per_hour: json_u32(&raw, "max_actions_per_hour"),
        })
    }
}

fn json_u32(raw: &serde_json::Value, key: &str) -> u32 {
    raw.get(key)
        .and_then(|v| v.as_u64())
        .map(|n| n.min(u64::from(u32::MAX)) as u32)
        .unwrap_or(0)
}

// ── 行动提案 ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub id: String,
    pub workspace_id: String,
    pub agent_id: String,
    /// Tool or action name being proposed.
    pub tool_name: String,
    /// Target device or resource.
    pub thing_id: Option<String>,
    /// Human-readable summary of what this will do.
    pub summary: String,
    /// Why the agent wants to take this action.
    pub reason: String,
    /// Risk assessment (low/medium/high).
    pub risk: String,
    /// Proposed parameters (tool-specific).
    pub parameters: Option<serde_json::Value>,
    /// ISO 8601 timestamp.
    pub created_at: String,
    /// Status lifecycle: Pending → Approved / Rejected.
    pub status: ProposalStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProposalStatus {
    Pending,
    Approved,
    Rejected,
}

impl std::fmt::Display for ProposalStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProposalStatus::Pending => write!(f, "pending"),
            ProposalStatus::Approved => write!(f, "approved"),
            ProposalStatus::Rejected => write!(f, "rejected"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_mode_round_trips() {
        for mode in [
            PermissionMode::ReadOnly,
            PermissionMode::Ask,
            PermissionMode::FullAccess,
        ] {
            assert_eq!(PermissionMode::parse(mode.as_str()), Some(mode));
        }
        assert_eq!(PermissionMode::parse("allow_all"), None);
        assert_eq!(PermissionMode::default(), PermissionMode::Ask);
    }

    #[test]
    fn from_db_json_reads_new_permission_mode() {
        let policy = AutonomyPolicy::from_db_json(
            r#"{"permission_mode":"read_only","allowed_actions":["reboot_thing"],
                "denied_actions":["wipe_thing"],"max_actions_per_run":3,"max_actions_per_hour":30}"#,
        )
        .expect("parse");
        assert_eq!(policy.permission_mode, PermissionMode::ReadOnly);
        assert_eq!(policy.allowed_actions, vec!["reboot_thing"]);
        assert_eq!(policy.denied_actions, vec!["wipe_thing"]);
        assert_eq!(policy.max_actions_per_run, 3);
        assert_eq!(policy.max_actions_per_hour, 30);
    }

    #[test]
    fn from_db_json_accepts_legacy_mode() {
        let cases = [
            (r#"{"mode":"diagnose"}"#, PermissionMode::ReadOnly),
            (r#"{"mode":"off"}"#, PermissionMode::ReadOnly),
            (r#"{"mode":"act"}"#, PermissionMode::Ask),
            (r#"{"mode":"bogus"}"#, PermissionMode::Ask),
            ("{}", PermissionMode::Ask),
        ];
        for (json, expected) in cases {
            assert_eq!(
                AutonomyPolicy::from_db_json(json).expect("parse").permission_mode,
                expected,
                "legacy JSON {json}"
            );
        }
    }

    #[test]
    fn from_db_json_never_promotes_full_access() {
        let policy = AutonomyPolicy::from_db_json(r#"{"permission_mode":"full_access"}"#).expect("parse");
        assert_eq!(policy.permission_mode, PermissionMode::Ask);
    }

    #[test]
    fn from_db_json_defaults_limits_fail_closed() {
        let policy = AutonomyPolicy::from_db_json("{}").expect("parse");
        assert!(policy.allowed_actions.is_empty());
        assert!(policy.denied_actions.is_empty());
        assert_eq!(policy.max_actions_per_run, 0);
        assert_eq!(policy.max_actions_per_hour, 0);
    }

    #[test]
    fn from_db_json_rejects_malformed_json() {
        assert!(AutonomyPolicy::from_db_json("not json").is_none());
    }
}
