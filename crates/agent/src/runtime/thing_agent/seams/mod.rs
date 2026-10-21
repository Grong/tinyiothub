//! E2 Brain 层五接缝（spec 2026-10-21-e2 §2）。
//!
//! 每个接缝是链式中间件：内置实现是链尾（`next() == None`），替换实现
//! 持有内置并只在需要时收窄决策（预算更低 / 更多拒绝 / 更多人工审批 /
//! 包装工具执行）。委托时必须原样返回内层结果（R-1）。
//!
//! 默认实现零行为变更：常量、检查顺序、fail-closed 语义与 E2 前一致。

pub mod approval;
pub mod budget;
pub mod composite;
pub mod policy;
pub mod tool_exec;
pub mod trigger;

use std::sync::Arc;

/// 接缝标记 trait（全量空白实现）。各接缝自己的 trait 用更具体的
/// `next()` 返回类型表达链关系。
pub trait Seam: Send + Sync {}
impl<T: Send + Sync + ?Sized> Seam for T {}

/// 组合根注入点：未替换的槽位 = 内置实现。
#[derive(Clone)]
pub struct SeamRegistry {
    pub budget: Arc<dyn budget::BudgetSeam>,
    pub approval: Arc<dyn approval::ApprovalSeam>,
    pub policy: Arc<dyn policy::PolicySeam>,
    pub tool_exec: Arc<dyn tool_exec::ToolExecSeam>,
}

impl SeamRegistry {
    pub fn defaults() -> Self {
        Self {
            budget: Arc::new(budget::DefaultBudget::default()),
            approval: Arc::new(approval::DefaultApproval),
            policy: Arc::new(policy::DefaultPolicy),
            tool_exec: Arc::new(tool_exec::DefaultToolExec),
        }
    }
}

impl Default for SeamRegistry {
    fn default() -> Self {
        Self::defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_registry_has_no_next_links() {
        let r = SeamRegistry::defaults();
        assert!(r.budget.next().is_none());
        assert!(r.approval.next().is_none());
        assert!(r.policy.next().is_none());
        assert!(r.tool_exec.next().is_none());
    }
}
