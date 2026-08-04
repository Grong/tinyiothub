//! Autonomy policy gate for the thing-agent loop.
//!
//! Three-state autonomy (off / diagnose / act) with allow/deny lists and
//! rate fuses. `gate_check` runs before every `invoke_action` in L4 mode and
//! is fail-closed: any doubt (no policy, unknown mode, DB error mapped by the
//! caller) results in Deny.

/// Three-state autonomy mode for a workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutonomyMode {
    Off,
    Diagnose,
    Act,
}

impl AutonomyMode {
    /// Stable string stored in `workspace_autonomy_policy.mode`.
    pub fn as_str(&self) -> &'static str {
        match self {
            AutonomyMode::Off => "off",
            AutonomyMode::Diagnose => "diagnose",
            AutonomyMode::Act => "act",
        }
    }

    /// Inverse of `as_str`; unknown values return None (treat as fail-closed).
    pub fn from_db(s: &str) -> Option<Self> {
        match s {
            "off" => Some(AutonomyMode::Off),
            "diagnose" => Some(AutonomyMode::Diagnose),
            "act" => Some(AutonomyMode::Act),
            _ => None,
        }
    }
}

/// Workspace-level autonomy policy for the thing-agent loop.
#[derive(Debug, Clone)]
pub struct AutonomyPolicy {
    pub mode: AutonomyMode,
    /// Allowed action names; `["*"]` means all actions.
    pub allowed_actions: Vec<String>,
    /// Denied action names (exact match); checked before the allowlist.
    pub denied_actions: Vec<String>,
    pub max_actions_per_run: u32,
    pub max_actions_per_hour: u32,
}

/// Persistence for autonomy policy and action-rate reads.
///
/// Implemented per-backend (e.g. `SqlitePolicyRepository` in the cloud crate).
/// Callers must map any Err to fail-closed `GateVerdict::Deny("policy_read_failed")`.
#[async_trait::async_trait]
pub trait PolicyRepository: Send + Sync {
    async fn load_autonomy(&self, workspace_id: &str) -> anyhow::Result<Option<AutonomyPolicy>>;
    async fn save_autonomy(&self, workspace_id: &str, policy: &AutonomyPolicy, updated_by: &str) -> anyhow::Result<()>;
    async fn count_actions_last_hour(&self, workspace_id: &str) -> anyhow::Result<u32>;
}

/// Verdict of the autonomy policy gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateVerdict {
    Allow,
    Deny { reason: String },
}

/// Policy gate decision, evaluated before every `invoke_action` (O7: re-read
/// each time so the kill switch takes effect immediately).
///
/// Order of checks (first hit decides):
/// 1. no policy or mode != Act -> Deny("autonomy_not_act")
/// 2. action in `denied_actions` -> Deny("action_denied")
/// 3. allowlist has no `*` and not the action -> Deny("action_not_allowed")
/// 4. `actions_this_run >= max_actions_per_run` -> Deny("run_action_cap")
/// 5. `actions_last_hour >= max_actions_per_hour` -> Deny("hourly_fuse")
/// 6. otherwise Allow
pub fn gate_check(
    policy: Option<&AutonomyPolicy>,
    action_name: &str,
    actions_this_run: u32,
    actions_last_hour: u32,
) -> GateVerdict {
    fn deny(reason: &str) -> GateVerdict {
        GateVerdict::Deny {
            reason: reason.to_string(),
        }
    }

    let Some(policy) = policy else {
        return deny("autonomy_not_act");
    };
    if policy.mode != AutonomyMode::Act {
        return deny("autonomy_not_act");
    }
    if policy.denied_actions.iter().any(|a| a == action_name) {
        return deny("action_denied");
    }
    if !policy.allowed_actions.iter().any(|a| a == "*" || a == action_name) {
        return deny("action_not_allowed");
    }
    if actions_this_run >= policy.max_actions_per_run {
        return deny("run_action_cap");
    }
    if actions_last_hour >= policy.max_actions_per_hour {
        return deny("hourly_fuse");
    }
    GateVerdict::Allow
}

#[cfg(test)]
mod tests {
    use super::*;

    fn act_policy() -> AutonomyPolicy {
        AutonomyPolicy {
            mode: AutonomyMode::Act,
            allowed_actions: vec!["reboot_device".to_string()],
            denied_actions: vec!["wipe_device".to_string()],
            max_actions_per_run: 3,
            max_actions_per_hour: 30,
        }
    }

    fn deny_reason(v: &GateVerdict) -> &str {
        match v {
            GateVerdict::Deny { reason } => reason,
            GateVerdict::Allow => panic!("expected Deny, got Allow"),
        }
    }

    #[test]
    fn none_policy_denies_fail_closed() {
        let v = gate_check(None, "reboot_device", 0, 0);
        assert_eq!(deny_reason(&v), "autonomy_not_act");
    }

    #[test]
    fn mode_off_denies() {
        let mut p = act_policy();
        p.mode = AutonomyMode::Off;
        let v = gate_check(Some(&p), "reboot_device", 0, 0);
        assert_eq!(deny_reason(&v), "autonomy_not_act");
    }

    #[test]
    fn mode_diagnose_denies() {
        let mut p = act_policy();
        p.mode = AutonomyMode::Diagnose;
        let v = gate_check(Some(&p), "reboot_device", 0, 0);
        assert_eq!(deny_reason(&v), "autonomy_not_act");
    }

    #[test]
    fn denylist_hit_denies_even_with_star_allowlist() {
        let mut p = act_policy();
        p.allowed_actions = vec!["*".to_string()];
        let v = gate_check(Some(&p), "wipe_device", 0, 0);
        assert_eq!(deny_reason(&v), "action_denied");
    }

    #[test]
    fn action_not_in_allowlist_denies() {
        let p = act_policy();
        let v = gate_check(Some(&p), "update_firmware", 0, 0);
        assert_eq!(deny_reason(&v), "action_not_allowed");
    }

    #[test]
    fn star_allowlist_allows() {
        let mut p = act_policy();
        p.allowed_actions = vec!["*".to_string()];
        assert_eq!(gate_check(Some(&p), "anything", 0, 0), GateVerdict::Allow);
    }

    #[test]
    fn exact_allowlist_allows() {
        let p = act_policy();
        assert_eq!(gate_check(Some(&p), "reboot_device", 0, 0), GateVerdict::Allow);
    }

    #[test]
    fn run_cap_reached_denies() {
        let p = act_policy();
        let v = gate_check(Some(&p), "reboot_device", 3, 0);
        assert_eq!(deny_reason(&v), "run_action_cap");
    }

    #[test]
    fn run_cap_boundary_allows() {
        let p = act_policy();
        assert_eq!(gate_check(Some(&p), "reboot_device", 2, 0), GateVerdict::Allow);
    }

    #[test]
    fn hourly_fuse_reached_denies() {
        let p = act_policy();
        let v = gate_check(Some(&p), "reboot_device", 0, 30);
        assert_eq!(deny_reason(&v), "hourly_fuse");
    }

    #[test]
    fn hourly_fuse_boundary_allows() {
        let p = act_policy();
        assert_eq!(gate_check(Some(&p), "reboot_device", 0, 29), GateVerdict::Allow);
    }

    #[test]
    fn verdict_order_mode_before_denylist() {
        // mode != Act short-circuits before list checks
        let mut p = act_policy();
        p.mode = AutonomyMode::Diagnose;
        let v = gate_check(Some(&p), "wipe_device", 0, 0);
        assert_eq!(deny_reason(&v), "autonomy_not_act");
    }

    #[test]
    fn verdict_order_lists_before_caps() {
        // denylist fires even when both caps are already exhausted
        let p = act_policy();
        let v = gate_check(Some(&p), "wipe_device", 3, 30);
        assert_eq!(deny_reason(&v), "action_denied");
    }

    #[test]
    fn autonomy_mode_db_roundtrip() {
        for (s, m) in [
            ("off", AutonomyMode::Off),
            ("diagnose", AutonomyMode::Diagnose),
            ("act", AutonomyMode::Act),
        ] {
            assert_eq!(AutonomyMode::from_db(s), Some(m));
            assert_eq!(m.as_str(), s);
        }
        assert_eq!(AutonomyMode::from_db("bogus"), None);
    }
}
