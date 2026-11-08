//! TurnContext — 当前 Turn 的会话身份，供工具在 execute 时读取。
//!
//! Registry 以 assistant_id 为键；TurnGuard 在 Drop 时只清除自己安装的
//! turn_id，避免慢 Turn 清掉后来者的上下文。Registry 本身不做串行化：同一
//! assistant 的 Turn 由调用方（agent lock）串行；这里只保证并发的 set/drop
//! 之间不会误删较新的 Turn。

use std::sync::Arc;

use dashmap::DashMap;
use uuid::Uuid;

use super::types::PermissionMode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnContext {
    pub workspace_id: String,
    /// 领域内读作 assistant_id；协议层仍叫 agent_id。
    pub assistant_id: String,
    /// "agent:<workspace>:<assistant>/<session>"
    pub session_key: String,
    pub turn_id: String,
    pub run_id: String,
    /// 0 = 主 Assistant；1 = 子代理；>=2 拒绝。
    pub depth: u8,
    /// 本 Turn 的权限模式；由 Session 行注入，子代理继承父 Turn。
    pub permission_mode: PermissionMode,
}

impl TurnContext {
    pub fn new(workspace_id: String, assistant_id: String, session_key: String, run_id: String, depth: u8) -> Self {
        Self {
            workspace_id,
            assistant_id,
            session_key,
            run_id,
            depth,
            turn_id: Uuid::new_v4().to_string(),
            permission_mode: PermissionMode::Ask,
        }
    }

    pub fn with_permission_mode(mut self, mode: PermissionMode) -> Self {
        self.permission_mode = mode;
        self
    }
}

#[derive(Clone, Default)]
pub struct TurnContextRegistry {
    inner: Arc<DashMap<String, TurnContext>>,
}

pub struct TurnGuard {
    registry: TurnContextRegistry,
    assistant_id: String,
    turn_id: String,
}

impl TurnContextRegistry {
    pub fn set(&self, context: TurnContext) -> TurnGuard {
        let assistant_id = context.assistant_id.clone();
        let turn_id = context.turn_id.clone();
        self.inner.insert(assistant_id.clone(), context);
        TurnGuard {
            registry: self.clone(),
            assistant_id,
            turn_id,
        }
    }

    pub fn get(&self, assistant_id: &str) -> Option<TurnContext> {
        self.inner.get(assistant_id).map(|entry| entry.value().clone())
    }

    /// HUD 用：当前 workspace 正在执行的 Turn 数。
    pub fn count_for_workspace(&self, workspace_id: &str) -> usize {
        self.inner
            .iter()
            .filter(|entry| entry.value().workspace_id == workspace_id)
            .count()
    }

    fn clear_if_current(&self, assistant_id: &str, turn_id: &str) {
        self.inner
            .remove_if(assistant_id, |_, context| context.turn_id == turn_id);
    }
}

impl Drop for TurnGuard {
    fn drop(&mut self) {
        self.registry.clear_if_current(&self.assistant_id, &self.turn_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(run_id: &str) -> TurnContext {
        TurnContext::new(
            "ws".to_string(),
            "agent_main".to_string(),
            "agent:ws:agent_main/s1".to_string(),
            run_id.to_string(),
            0,
        )
    }

    #[test]
    fn turn_context_defaults_to_ask_and_overrides() {
        let context = ctx("run1");
        assert_eq!(context.permission_mode, PermissionMode::Ask);
        let readonly = context.with_permission_mode(PermissionMode::ReadOnly);
        assert_eq!(readonly.permission_mode, PermissionMode::ReadOnly);
    }

    #[test]
    fn set_and_get_round_trip() {
        let registry = TurnContextRegistry::default();
        let context = ctx("run1");
        let _guard = registry.set(context.clone());
        let found = registry.get("agent_main").expect("context installed");
        assert_eq!(found.session_key, context.session_key);
        assert_eq!(found.turn_id, context.turn_id);
    }

    #[test]
    fn guard_clears_installed_turn() {
        let registry = TurnContextRegistry::default();
        {
            let _guard = registry.set(ctx("run1"));
        }
        assert!(registry.get("agent_main").is_none());
    }

    #[test]
    fn stale_guard_does_not_clear_newer_turn() {
        let registry = TurnContextRegistry::default();
        let first = ctx("run1");
        let guard = registry.set(first);
        let second = ctx("run2");
        let _second_guard = registry.set(second.clone());
        drop(guard);
        let found = registry.get("agent_main").expect("newer context survives");
        assert_eq!(found.turn_id, second.turn_id);
    }

    #[test]
    fn contexts_are_isolated_by_assistant() {
        let registry = TurnContextRegistry::default();
        let _a = registry.set(ctx("run_a"));
        let mut other = ctx("run_b");
        other.assistant_id = "agent_other".to_string();
        let _b = registry.set(other);
        assert_eq!(registry.get("agent_main").unwrap().run_id, "run_a");
        assert_eq!(registry.get("agent_other").unwrap().run_id, "run_b");
    }

    #[test]
    fn counts_active_turns_by_workspace() {
        let registry = TurnContextRegistry::default();
        let _a = registry.set(ctx("run1"));
        let mut other_ws = ctx("run2");
        other_ws.workspace_id = "ws-other".to_string();
        other_ws.assistant_id = "agent_other".to_string();
        let _b = registry.set(other_ws);
        assert_eq!(registry.count_for_workspace("ws"), 1);
        assert_eq!(registry.count_for_workspace("ws-other"), 1);
        assert_eq!(registry.count_for_workspace("ws-missing"), 0);
    }
}
