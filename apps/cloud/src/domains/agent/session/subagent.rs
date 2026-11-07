//! Subagent — Turn 级同步子代理。
//!
//! 子代理拥有隔离的上下文，执行完只把最终报告返回父 Turn；不创建独立 Task。
//! 只允许一层：父 Turn depth=0，子代理 depth=1；depth>=1 的父 Turn 不得再派生。

use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentStatus {
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct SubagentRequest {
    pub workspace_id: String,
    pub parent_agent_id: String,
    pub parent_session_key: String,
    pub subagent_id: String,
    pub subagent_type: String,
    pub description: String,
    pub prompt: String,
    pub depth: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentReport {
    pub subagent_id: String,
    pub status: SubagentStatus,
    pub summary: String,
    pub tool_calls: u32,
    /// 子代理引用的证据（事件 id / 设备 id / 媒体 ref）。
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    /// 子代理建议的后续动作；Plan 1 允许为空。
    #[serde(default)]
    pub recommended_actions: Vec<String>,
}

#[async_trait::async_trait]
pub trait SubagentRunner: Send + Sync {
    async fn run(&self, request: SubagentRequest) -> Result<SubagentReport, String>;
}

#[derive(Default)]
pub struct SubagentSlot {
    inner: RwLock<Option<Arc<dyn SubagentRunner>>>,
}

impl SubagentSlot {
    pub fn set(&self, runner: Arc<dyn SubagentRunner>) {
        *self.inner.write() = Some(runner);
    }

    pub fn get(&self) -> Option<Arc<dyn SubagentRunner>> {
        self.inner.read().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_starts_empty_and_accepts_runner() {
        let slot = SubagentSlot::default();
        assert!(slot.get().is_none());
        let runner: Arc<dyn SubagentRunner> = Arc::new(super::tests::StubRunner);
        slot.set(runner);
        assert!(slot.get().is_some());
    }

    pub(super) struct StubRunner;

    #[async_trait::async_trait]
    impl SubagentRunner for StubRunner {
        async fn run(&self, request: SubagentRequest) -> Result<SubagentReport, String> {
            Ok(SubagentReport {
                subagent_id: request.subagent_id,
                status: SubagentStatus::Completed,
                summary: "subagent done".to_string(),
                tool_calls: 0,
                evidence_refs: vec![],
                recommended_actions: vec![],
            })
        }
    }
}
