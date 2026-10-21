//! Policy 接缝：自治动作门控（spec §2.4）。
//!
//! 默认实现 = `tinyiothub_policy::autonomy::gate_check` 六步顺序原样；
//! 替换实现只能追加拒绝，不能把 Deny 变 Allow。

use tinyiothub_policy::autonomy::{AutonomyPolicy, GateVerdict, gate_check};

use super::Seam;

pub struct PolicyInput<'a> {
    pub workspace_id: &'a str,
    pub action_name: &'a str,
    pub actions_this_run: u32,
    pub actions_last_hour: u32,
    pub policy: Option<&'a AutonomyPolicy>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    Allow,
    Deny { code: String, message: String },
}

pub trait PolicySeam: Seam {
    fn evaluate(&self, input: &PolicyInput<'_>) -> PolicyDecision;

    fn next(&self) -> Option<&dyn PolicySeam> {
        None
    }
}

pub struct DefaultPolicy;

impl PolicySeam for DefaultPolicy {
    fn evaluate(&self, input: &PolicyInput<'_>) -> PolicyDecision {
        match gate_check(
            input.policy,
            input.action_name,
            input.actions_this_run,
            input.actions_last_hour,
        ) {
            GateVerdict::Allow => PolicyDecision::Allow,
            GateVerdict::Deny { reason } => PolicyDecision::Deny {
                code: reason.clone(),
                message: reason,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use tinyiothub_policy::autonomy::AutonomyMode;

    use super::*;

    fn policy() -> AutonomyPolicy {
        AutonomyPolicy {
            mode: AutonomyMode::Act,
            allowed_actions: vec!["reboot".to_string()],
            denied_actions: vec!["wipe".to_string()],
            max_actions_per_run: 3,
            max_actions_per_hour: 30,
        }
    }

    fn input<'a>(p: Option<&'a AutonomyPolicy>, action: &'a str, run: u32, hour: u32) -> PolicyInput<'a> {
        PolicyInput {
            workspace_id: "ws",
            action_name: action,
            actions_this_run: run,
            actions_last_hour: hour,
            policy: p,
        }
    }

    fn code(d: PolicyDecision) -> String {
        match d {
            PolicyDecision::Deny { code, .. } => code,
            PolicyDecision::Allow => panic!("expected deny"),
        }
    }

    #[test]
    fn default_policy_matches_gate_check_reasons() {
        let p = policy();
        let seam = DefaultPolicy;
        assert_eq!(seam.evaluate(&input(None, "reboot", 0, 0)), PolicyDecision::Deny {
            code: "autonomy_not_act".into(),
            message: "autonomy_not_act".into()
        });
        assert_eq!(code(seam.evaluate(&input(Some(&p), "wipe", 0, 0))), "action_denied");
        assert_eq!(code(seam.evaluate(&input(Some(&p), "open", 0, 0))), "action_not_allowed");
        assert_eq!(code(seam.evaluate(&input(Some(&p), "reboot", 3, 0))), "run_action_cap");
        assert_eq!(code(seam.evaluate(&input(Some(&p), "reboot", 0, 30))), "hourly_fuse");
        assert_eq!(seam.evaluate(&input(Some(&p), "reboot", 0, 0)), PolicyDecision::Allow);
        assert!(seam.next().is_none());
    }

    /// 替换证明：额外拒绝一个白名单内动作（只能收窄）。
    struct DenyExtraAction {
        inner: DefaultPolicy,
    }

    impl PolicySeam for DenyExtraAction {
        fn evaluate(&self, input: &PolicyInput<'_>) -> PolicyDecision {
            if input.action_name == "open_valve" {
                return PolicyDecision::Deny {
                    code: "seam_extra_deny".into(),
                    message: "open_valve is blocked by workspace overlay".into(),
                };
            }
            self.inner.evaluate(input)
        }
        fn next(&self) -> Option<&dyn PolicySeam> {
            Some(&self.inner)
        }
    }

    #[test]
    fn replacement_only_narrows() {
        let p = policy();
        let seam = DenyExtraAction {
            inner: DefaultPolicy,
        };
        assert_eq!(seam.evaluate(&input(Some(&p), "reboot", 0, 0)), PolicyDecision::Allow);
        assert_eq!(code(seam.evaluate(&input(Some(&p), "open_valve", 0, 0))), "seam_extra_deny");
        assert!(seam.next().is_some());
    }
}
