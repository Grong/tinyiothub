//! Approval 接缝：动作审批裁决 + 原因六分类（spec §2.3 / §5）。
//!
//! 六分类是**终态原因**：`Pending` 是等待人工的状态，不是原因。
//! `Unavailable`/`Expired`/`Cancelled` 都不是否决（`is_denial() == false`），
//! 且都不执行——"没人可问"不能记成"拒绝了"。

use tinyiothub_skills::trust::ToolSafety;

use super::Seam;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalReason {
    /// 策略/信任自动通过（无人工参与）。
    AutoApproved,
    /// 人工批准。
    HumanApproved,
    /// 人工明确否决。
    HumanRejected,
    /// 无人可问 / 审批通道不可用——不是否决，且不执行。
    Unavailable,
    /// 审批窗口过期——不是否决，且不执行。
    Expired,
    /// run/会话取消或已被替代——不是否决，且不执行。
    Cancelled,
}

impl ApprovalReason {
    /// 只有这两种原因会执行动作。
    pub fn executes(&self) -> bool {
        matches!(self, Self::AutoApproved | Self::HumanApproved)
    }

    /// 只有人工明确否决才是 denial（unavailable/expired/cancelled 不是）。
    pub fn is_denial(&self) -> bool {
        matches!(self, Self::HumanRejected)
    }

    /// 落库/API 的稳定词表。
    pub fn code(&self) -> &'static str {
        match self {
            Self::AutoApproved => "auto_approved",
            Self::HumanApproved => "human_approved",
            Self::HumanRejected => "human_rejected",
            Self::Unavailable => "unavailable",
            Self::Expired => "expired",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Some(match code {
            "auto_approved" => Self::AutoApproved,
            "human_approved" => Self::HumanApproved,
            "human_rejected" => Self::HumanRejected,
            "unavailable" => Self::Unavailable,
            "expired" => Self::Expired,
            "cancelled" => Self::Cancelled,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalDecision {
    /// 需要人工确认；token 由工具层 mint（接缝不持存储）。
    Pending { message: String },
    Decided(ApprovalReason),
}

pub struct ApprovalInput<'a> {
    pub workspace_id: &'a str,
    pub tool_name: &'a str,
    pub args: &'a serde_json::Value,
    pub thing_id: Option<&'a str>,
    pub trigger: &'a str,
    /// 工作区是否要求动作确认（调用方新鲜读档；读失败 fail-closed 为 true）。
    pub require_confirm: bool,
    pub safety: ToolSafety,
}

pub trait ApprovalSeam: Seam {
    fn decide(&self, input: &ApprovalInput<'_>) -> ApprovalDecision;

    fn next(&self) -> Option<&dyn ApprovalSeam> {
        None
    }
}

/// 默认实现 = E2 前语义：需要确认且动作是写/破坏性 → Pending；否则自动通过。
pub struct DefaultApproval;

impl ApprovalSeam for DefaultApproval {
    fn decide(&self, input: &ApprovalInput<'_>) -> ApprovalDecision {
        let needs_human = input.require_confirm && matches!(input.safety, ToolSafety::Write | ToolSafety::Destructive);
        if needs_human {
            ApprovalDecision::Pending {
                message: "该操作需要用户确认后才能执行。".to_string(),
            }
        } else {
            ApprovalDecision::Decided(ApprovalReason::AutoApproved)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input<'a>(require_confirm: bool, safety: ToolSafety) -> ApprovalInput<'a> {
        static ARGS: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
        let args = ARGS.get_or_init(|| serde_json::json!({"thingId": "t1"}));
        ApprovalInput {
            workspace_id: "ws",
            tool_name: "call_device",
            args,
            thing_id: Some("t1"),
            trigger: "chat",
            require_confirm,
            safety,
        }
    }

    #[test]
    fn six_reason_semantics() {
        assert!(ApprovalReason::AutoApproved.executes());
        assert!(ApprovalReason::HumanApproved.executes());
        for r in [
            ApprovalReason::HumanRejected,
            ApprovalReason::Unavailable,
            ApprovalReason::Expired,
            ApprovalReason::Cancelled,
        ] {
            assert!(!r.executes(), "{r:?} must not execute");
        }
        assert!(ApprovalReason::HumanRejected.is_denial());
        for r in [ApprovalReason::Unavailable, ApprovalReason::Expired, ApprovalReason::Cancelled] {
            assert!(!r.is_denial(), "{r:?} is not a denial");
        }
        assert_eq!(ApprovalReason::from_code("unavailable"), Some(ApprovalReason::Unavailable));
        assert_eq!(ApprovalReason::Unavailable.code(), "unavailable");
        assert_eq!(ApprovalReason::from_code("nope"), None);
    }

    #[test]
    fn default_approval_matches_legacy_gate() {
        let d = DefaultApproval;
        assert_eq!(
            d.decide(&input(false, ToolSafety::Destructive)),
            ApprovalDecision::Decided(ApprovalReason::AutoApproved)
        );
        assert_eq!(
            d.decide(&input(true, ToolSafety::ReadOnly)),
            ApprovalDecision::Decided(ApprovalReason::AutoApproved)
        );
        assert!(matches!(
            d.decide(&input(true, ToolSafety::Write)),
            ApprovalDecision::Pending { .. }
        ));
        assert!(d.next().is_none());
    }

    /// 替换证明：写动作一律人工（无视工作区 confirm 开关）。
    struct StrictApproval {
        inner: DefaultApproval,
    }

    impl ApprovalSeam for StrictApproval {
        fn decide(&self, input: &ApprovalInput<'_>) -> ApprovalDecision {
            if matches!(input.safety, ToolSafety::Write | ToolSafety::Destructive) {
                return ApprovalDecision::Pending {
                    message: "全部写操作需人工确认".to_string(),
                };
            }
            self.inner.decide(input)
        }
        fn next(&self) -> Option<&dyn ApprovalSeam> {
            Some(&self.inner)
        }
    }

    #[test]
    fn replacement_only_narrows() {
        let s = StrictApproval {
            inner: DefaultApproval,
        };
        assert!(matches!(
            s.decide(&input(false, ToolSafety::Write)),
            ApprovalDecision::Pending { .. }
        ));
        assert_eq!(
            s.decide(&input(true, ToolSafety::ReadOnly)),
            ApprovalDecision::Decided(ApprovalReason::AutoApproved)
        );
        assert!(s.next().is_some());
    }
}
