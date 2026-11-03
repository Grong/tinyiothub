//! Trigger 接缝契约（spec §2.1）。
//!
//! `Trigger` trait 本身就是接缝（`trigger/mod.rs`），已有四个真实实现：
//! Heartbeat / Timer / ThingEvent / Watch。本节只固化跨实现约定，供新触发源
//! 对照；不新增 trait。
//!
//! 约定：
//! - 输出只允许 `WakeSignal`；`workspace_id` 来自构造参数，不得伪造。
//! - `Priority` 只影响调度语义，不得用于绕过限速/预算（Critical 既有豁免除外）。
//! - `dedup_key` 必须使用自己的前缀，避免跨触发源误合并：
//!   `thing:{id}:event:{name}` / `timer:{ws}` / `heartbeat:{ws}[:{thing}:{alarm}]` /
//!   `watch:{rule_id}`。
//! - 新触发源 = 新增一个 `Trigger` 实现并在 `PipelineDeps` 挂载，不改 scheduler/runner。

/// 既有触发源的 dedup_key 前缀（文档 + 测试断言用）。
pub const DEDUP_PREFIXES: &[&str] = &["thing:", "timer:", "heartbeat:", "watch:"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_are_non_overlapping() {
        for (i, a) in DEDUP_PREFIXES.iter().enumerate() {
            for b in &DEDUP_PREFIXES[i + 1..] {
                assert!(!a.starts_with(b) && !b.starts_with(a), "{a} overlaps {b}");
            }
        }
    }
}
