//! Budget 接缝：run 级预算裁决（spec §2.2）。
//!
//! 默认实现 = E2 前的硬预算：25 次工具调用 / 300s / 单物动作 3 次；
//! token 上限暂未启用（E1d 迁入后设置）。替换实现只能更低。

use std::collections::HashMap;
use std::time::Duration;

use super::Seam;

/// 默认单物动作上限（与 prompt 边界段一致）。
pub const DEFAULT_MAX_ACTIONS_PER_THING: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetReason {
    ToolCalls,
    Duration,
    Tokens,
    PerThingActions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetDecision {
    Continue,
    Truncate(BudgetReason),
}

pub struct BudgetInput<'a> {
    pub workspace_id: &'a str,
    pub run_id: &'a str,
    pub tool_calls: u32,
    pub elapsed: Duration,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub per_thing_actions: &'a HashMap<String, u32>,
}

pub trait BudgetSeam: Seam {
    fn check(&self, input: &BudgetInput<'_>) -> BudgetDecision;

    /// 工具调用上限（报告 `tool_call_budget` 用）；替换实现可覆盖。
    fn tool_call_cap(&self) -> Option<u32> {
        None
    }

    /// 时长上限（Runner 外层兜底超时用）；替换实现可覆盖。
    fn duration_cap(&self) -> Option<Duration> {
        None
    }

    /// 内置实现返回 None；替换实现返回被包装的内建。
    fn next(&self) -> Option<&dyn BudgetSeam> {
        None
    }
}

#[derive(Debug, Clone)]
pub struct DefaultBudget {
    pub max_tool_calls: u32,
    pub max_duration: Duration,
    pub max_tokens: Option<u64>,
    pub max_actions_per_thing: u32,
}

impl Default for DefaultBudget {
    fn default() -> Self {
        Self {
            max_tool_calls: crate::runtime::thing_agent::runner::MAX_TOOL_CALLS_PER_RUN,
            max_duration: crate::runtime::thing_agent::runner::MAX_RUN_DURATION,
            max_tokens: None,
            max_actions_per_thing: DEFAULT_MAX_ACTIONS_PER_THING,
        }
    }
}

impl BudgetSeam for DefaultBudget {
    fn tool_call_cap(&self) -> Option<u32> {
        Some(self.max_tool_calls)
    }

    fn duration_cap(&self) -> Option<Duration> {
        Some(self.max_duration)
    }

    fn check(&self, input: &BudgetInput<'_>) -> BudgetDecision {
        if input.tool_calls > self.max_tool_calls {
            return BudgetDecision::Truncate(BudgetReason::ToolCalls);
        }
        if input.elapsed >= self.max_duration {
            return BudgetDecision::Truncate(BudgetReason::Duration);
        }
        if let Some(max) = self.max_tokens
            && input.input_tokens + input.output_tokens > max
        {
            return BudgetDecision::Truncate(BudgetReason::Tokens);
        }
        if input
            .per_thing_actions
            .values()
            .any(|n| *n > self.max_actions_per_thing)
        {
            return BudgetDecision::Truncate(BudgetReason::PerThingActions);
        }
        BudgetDecision::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input_with<'a>(counts: &'a HashMap<String, u32>) -> BudgetInput<'a> {
        BudgetInput {
            workspace_id: "ws",
            run_id: "run",
            tool_calls: 0,
            elapsed: Duration::ZERO,
            input_tokens: 0,
            output_tokens: 0,
            per_thing_actions: counts,
        }
    }

    #[test]
    fn default_budget_matches_legacy_constants() {
        let b = DefaultBudget::default();
        assert_eq!(b.max_tool_calls, 25);
        assert_eq!(b.max_duration, Duration::from_secs(300));
        assert_eq!(b.max_actions_per_thing, 3);
        assert_eq!(b.max_tokens, None);
    }

    #[test]
    fn tool_call_budget_crosses_at_26th_call() {
        let counts = HashMap::new();
        let b = DefaultBudget::default();
        let mut input = input_with(&counts);
        input.tool_calls = 25;
        assert_eq!(b.check(&input), BudgetDecision::Continue);
        input.tool_calls = 26;
        assert_eq!(b.check(&input), BudgetDecision::Truncate(BudgetReason::ToolCalls));
    }

    #[test]
    fn duration_budget_fires_at_deadline() {
        let counts = HashMap::new();
        let b = DefaultBudget::default();
        let mut input = input_with(&counts);
        input.elapsed = Duration::from_secs(299);
        assert_eq!(b.check(&input), BudgetDecision::Continue);
        input.elapsed = Duration::from_secs(300);
        assert_eq!(b.check(&input), BudgetDecision::Truncate(BudgetReason::Duration));
    }

    #[test]
    fn per_thing_cap_is_inclusive_at_3() {
        let b = DefaultBudget::default();
        let mut counts = HashMap::new();
        counts.insert("t1".to_string(), 3);
        assert_eq!(b.check(&input_with(&counts)), BudgetDecision::Continue);
        counts.insert("t1".to_string(), 4);
        assert_eq!(
            b.check(&input_with(&counts)),
            BudgetDecision::Truncate(BudgetReason::PerThingActions)
        );
    }

    /// 替换证明：半预算实现真实收窄，且 next() 指向内置。
    struct HalfBudget {
        inner: DefaultBudget,
    }

    impl BudgetSeam for HalfBudget {
        fn check(&self, input: &BudgetInput<'_>) -> BudgetDecision {
            if input.tool_calls > self.inner.max_tool_calls / 2 {
                BudgetDecision::Truncate(BudgetReason::ToolCalls)
            } else {
                self.inner.check(input)
            }
        }
        fn tool_call_cap(&self) -> Option<u32> {
            Some(self.inner.max_tool_calls / 2)
        }
        fn next(&self) -> Option<&dyn BudgetSeam> {
            Some(&self.inner)
        }
    }

    #[test]
    fn replacement_narrows_budget() {
        let counts = HashMap::new();
        let half = HalfBudget {
            inner: DefaultBudget::default(),
        };
        let mut input = input_with(&counts);
        input.tool_calls = 12;
        assert_eq!(half.check(&input), BudgetDecision::Continue);
        input.tool_calls = 13;
        assert_eq!(half.check(&input), BudgetDecision::Truncate(BudgetReason::ToolCalls));
        assert!(half.next().is_some());
    }
}
