//! 会话权限守卫：按 TurnContext.permission_mode 决定 ReadOnly/Ask/FullAccess。
//!
//! 只包裹 Write/Destructive 工具；ReadOnly 直接透传。审批事件进入会话时间线。

use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use tinyiothub_agent::port::attribution::{Attributable, Role};
use tinyiothub_agent::port::tool::{Tool, ToolResult};
use tinyiothub_skills::trust::ToolSafety;
use tokio::time::timeout;

use crate::domains::agent::session::approvals::{ApprovalBroker, ApprovalDecision};
use crate::domains::agent::session::context::TurnContextRegistry;
use crate::domains::agent::session::events::{SessionEventSink, session_event};
use crate::domains::agent::session::types::PermissionMode;

pub const APPROVAL_TIMEOUT: Duration = Duration::from_secs(10 * 60);

pub struct PermissionGuardTool {
    inner: Box<dyn Tool>,
    assistant_id: String,
    contexts: Arc<TurnContextRegistry>,
    approvals: Arc<ApprovalBroker>,
    events: Arc<dyn SessionEventSink>,
    safety: ToolSafety,
    timeout: Duration,
}

impl PermissionGuardTool {
    pub fn new(
        inner: Box<dyn Tool>,
        assistant_id: String,
        contexts: Arc<TurnContextRegistry>,
        approvals: Arc<ApprovalBroker>,
        events: Arc<dyn SessionEventSink>,
        safety: ToolSafety,
        timeout: Duration,
    ) -> Self {
        Self {
            inner,
            assistant_id,
            contexts,
            approvals,
            events,
            safety,
            timeout,
        }
    }

    fn denied(message: impl Into<String>) -> anyhow::Result<ToolResult> {
        Ok(ToolResult {
            success: false,
            output: String::new(),
            error: Some(message.into()),
            attachments: vec![],
        })
    }
}

#[async_trait::async_trait]
impl Tool for PermissionGuardTool {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.inner.parameters_schema()
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        if self.safety == ToolSafety::ReadOnly {
            return self.inner.execute(args).await;
        }
        let Some(context) = self.contexts.get(&self.assistant_id) else {
            tracing::warn!(
                assistant_id = %self.assistant_id,
                tool = self.inner.name(),
                "permission guard: missing turn context; denying non-read-only tool"
            );
            return Self::denied("权限检查失败：缺少 TurnContext，已拒绝执行");
        };
        match context.permission_mode {
            PermissionMode::FullAccess => self.inner.execute(args).await,
            PermissionMode::ReadOnly => Self::denied("只读模式：该操作需要写权限，已拒绝执行"),
            PermissionMode::Ask => {
                let summary = serde_json::to_string(&args).unwrap_or_default();
                let summary = summary.chars().take(200).collect::<String>();
                let (approval_id, receiver) =
                    self.approvals
                        .register(&context.session_key, self.inner.name(), &summary);
                if let Err(error) = self
                    .events
                    .emit(session_event(
                        &context,
                        "approval.requested",
                        json!({
                            "approvalId": approval_id,
                            "action": self.inner.name(),
                            "summary": summary
                        }),
                    ))
                    .await
                {
                    self.approvals.cancel(&approval_id);
                    return Err(error.into());
                }

                let decision = match timeout(self.timeout, receiver).await {
                    Ok(Ok(decision)) => decision,
                    Ok(Err(_)) | Err(_) => {
                        self.approvals.cancel(&approval_id);
                        ApprovalDecision::Denied
                    }
                };
                let resolution = match decision {
                    ApprovalDecision::Approved => "approved",
                    ApprovalDecision::Denied => "denied",
                };
                let _ = self
                    .events
                    .emit(session_event(
                        &context,
                        "approval.resolved",
                        json!({ "approvalId": approval_id, "resolution": resolution }),
                    ))
                    .await;
                match decision {
                    ApprovalDecision::Approved => self.inner.execute(args).await,
                    ApprovalDecision::Denied => Self::denied("操作未获批准，已取消"),
                }
            }
        }
    }
}

impl Attributable for PermissionGuardTool {
    fn role(&self) -> Role {
        self.inner.role()
    }

    fn alias(&self) -> &str {
        self.inner.alias()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::agent::session::approvals::{ApprovalBroker, ApprovalDecision};
    use crate::domains::agent::session::context::{TurnContext, TurnContextRegistry};
    use crate::domains::agent::session::events::test_support::RecordingSink;
    use crate::domains::agent::session::types::PermissionMode;
    use tinyiothub_agent::port::attribution::{Attributable, Role, ToolKind};
    use tinyiothub_agent::port::tool::ToolResult;
    use tinyiothub_skills::trust::ToolSafety;

    use std::sync::atomic::{AtomicUsize, Ordering};

    struct EchoTool {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str {
            "set_device"
        }
        fn description(&self) -> &str {
            "test"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type":"object"})
        }
        async fn execute(&self, _args: serde_json::Value) -> anyhow::Result<ToolResult> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(ToolResult {
                success: true,
                output: "ok".into(),
                error: None,
                attachments: vec![],
            })
        }
    }

    fn echo() -> (Box<dyn Tool>, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        (Box::new(EchoTool { calls: calls.clone() }), calls)
    }

    impl Attributable for EchoTool {
        fn role(&self) -> Role {
            Role::Tool(ToolKind::Plugin)
        }
        fn alias(&self) -> &str {
            "set_device"
        }
    }

    fn context(mode: PermissionMode) -> TurnContext {
        TurnContext::new(
            "ws".into(),
            "agent_main".into(),
            "agent:ws:agent_main/s1".into(),
            "run1".into(),
            0,
        )
        .with_permission_mode(mode)
    }

    /// 有界轮询：等工具把自己的 approval 注册进 broker，避免固定 sleep 的抖动。
    async fn wait_for_approval(broker: &ApprovalBroker, session_key: &str) -> String {
        for _ in 0..100 {
            tokio::time::sleep(Duration::from_millis(5)).await;
            if let Some(pending) = broker.list_for_session(session_key).into_iter().next() {
                return pending.approval_id;
            }
        }
        panic!("approval was never registered");
    }

    #[tokio::test]
    async fn read_only_blocks_write_without_calling_inner() {
        let (inner, calls) = echo();
        let contexts = Arc::new(TurnContextRegistry::default());
        let _guard = contexts.set(context(PermissionMode::ReadOnly));
        let tool = PermissionGuardTool::new(
            inner,
            "agent_main".into(),
            contexts,
            Arc::new(ApprovalBroker::default()),
            Arc::new(RecordingSink::default()),
            ToolSafety::Write,
            std::time::Duration::from_secs(1),
        );
        let result = tool.execute(serde_json::json!({})).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("只读"));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn missing_context_denies_write_tool() {
        let (inner, calls) = echo();
        let contexts = Arc::new(TurnContextRegistry::default());
        let tool = PermissionGuardTool::new(
            inner,
            "agent_main".into(),
            contexts,
            Arc::new(ApprovalBroker::default()),
            Arc::new(RecordingSink::default()),
            ToolSafety::Write,
            std::time::Duration::from_secs(1),
        );
        let result = tool.execute(serde_json::json!({})).await.unwrap();
        assert!(!result.success);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn ask_waits_for_approval_then_executes() {
        let (inner, calls) = echo();
        let contexts = Arc::new(TurnContextRegistry::default());
        let _guard = contexts.set(context(PermissionMode::Ask));
        let broker = Arc::new(ApprovalBroker::default());
        let sink = Arc::new(RecordingSink::default());
        let tool = Arc::new(PermissionGuardTool::new(
            inner,
            "agent_main".into(),
            contexts,
            broker.clone(),
            sink.clone(),
            ToolSafety::Write,
            std::time::Duration::from_secs(5),
        ));
        let handle = tokio::spawn({
            let tool = tool.clone();
            async move { tool.execute(serde_json::json!({})).await }
        });
        let approval_id = wait_for_approval(&broker, "agent:ws:agent_main/s1").await;
        broker
            .answer("agent:ws:agent_main/s1", &approval_id, ApprovalDecision::Approved)
            .unwrap();
        let result = handle.await.unwrap().unwrap();
        assert!(result.success);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let events = sink.events.lock().await;
        let types: Vec<&str> = events.iter().map(|event| event.event_type.as_str()).collect();
        assert_eq!(types, vec!["approval.requested", "approval.resolved"]);
        assert_eq!(events[1].payload_json["resolution"], "approved");
    }

    #[tokio::test]
    async fn ask_denied_does_not_execute_inner() {
        let (inner, calls) = echo();
        let contexts = Arc::new(TurnContextRegistry::default());
        let _guard = contexts.set(context(PermissionMode::Ask));
        let broker = Arc::new(ApprovalBroker::default());
        let sink = Arc::new(RecordingSink::default());
        let tool = Arc::new(PermissionGuardTool::new(
            inner,
            "agent_main".into(),
            contexts,
            broker.clone(),
            sink.clone(),
            ToolSafety::Write,
            std::time::Duration::from_secs(5),
        ));
        let handle = tokio::spawn({
            let tool = tool.clone();
            async move { tool.execute(serde_json::json!({})).await }
        });
        let approval_id = wait_for_approval(&broker, "agent:ws:agent_main/s1").await;
        broker
            .answer("agent:ws:agent_main/s1", &approval_id, ApprovalDecision::Denied)
            .unwrap();
        let result = handle.await.unwrap().unwrap();
        assert!(!result.success);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let events = sink.events.lock().await;
        let types: Vec<&str> = events.iter().map(|event| event.event_type.as_str()).collect();
        assert_eq!(types, vec!["approval.requested", "approval.resolved"]);
        assert_eq!(events[1].payload_json["resolution"], "denied");
    }

    #[tokio::test]
    async fn ask_timeout_does_not_execute_inner() {
        let (inner, calls) = echo();
        let contexts = Arc::new(TurnContextRegistry::default());
        let _guard = contexts.set(context(PermissionMode::Ask));
        let sink = Arc::new(RecordingSink::default());
        let tool = PermissionGuardTool::new(
            inner,
            "agent_main".into(),
            contexts,
            Arc::new(ApprovalBroker::default()),
            sink.clone(),
            ToolSafety::Write,
            Duration::from_millis(20),
        );
        let result = tool.execute(serde_json::json!({})).await.unwrap();
        assert!(!result.success);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let events = sink.events.lock().await;
        let last = events.last().expect("approval.resolved emitted");
        assert_eq!(last.event_type, "approval.resolved");
        assert_eq!(last.payload_json["resolution"], "denied");
    }

    #[tokio::test]
    async fn full_access_runs_inner() {
        let (inner, calls) = echo();
        let contexts = Arc::new(TurnContextRegistry::default());
        let _guard = contexts.set(context(PermissionMode::FullAccess));
        let tool = PermissionGuardTool::new(
            inner,
            "agent_main".into(),
            contexts,
            Arc::new(ApprovalBroker::default()),
            Arc::new(RecordingSink::default()),
            ToolSafety::Destructive,
            std::time::Duration::from_secs(1),
        );
        let result = tool.execute(serde_json::json!({})).await.unwrap();
        assert!(result.success);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
