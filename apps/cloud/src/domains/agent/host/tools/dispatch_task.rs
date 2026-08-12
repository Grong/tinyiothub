// dispatch_thing_task — chat 工具（T14）：用户指令入口之一
//
// chat Agent 判断用户意图为"去执行/去处理"时调用本工具，投递
// TriggerSource::UserDirective 信号到 DirectiveSink（T15 接线
// ThingAgentManager），随后立即回复"已受理，完成后回报"——执行结果
// 由 thing-agent loop 完成后主动回推（T13 pushback），本工具不等待。
//
// sink 解析：构造注入（生产由 AgentPool 经 ToolRuntimeContext 传入；
// P4-Task22 移除了 AppState 回退路径）。

use std::sync::Arc;

use crate::domains::agent::loop_::thing_agent::{DirectiveSink, EnqueueError, Priority, TriggerSource, WakeSignal};
use async_trait::async_trait;
use serde_json::{Value, json};
use zeroclaw::tools::{Tool, ToolResult};
use zeroclaw_api::attribution::{Attributable, Role, ToolKind};

use super::thing::{tool_err, tool_ok};

pub struct DispatchThingTaskTool {
    workspace_id: String,
    sink: Option<Arc<dyn DirectiveSink>>,
}

impl DispatchThingTaskTool {
    pub fn new(workspace_id: &str, sink: Option<Arc<dyn DirectiveSink>>) -> Self {
        Self {
            workspace_id: workspace_id.to_string(),
            sink,
        }
    }

    fn resolve_sink(&self) -> Option<Arc<dyn DirectiveSink>> {
        self.sink.clone()
    }
}

impl Attributable for DispatchThingTaskTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Plugin)
    }
    fn alias(&self) -> &str {
        self.name()
    }
}

#[async_trait]
impl Tool for DispatchThingTaskTool {
    fn name(&self) -> &str {
        "dispatch_thing_task"
    }

    fn description(&self) -> &str {
        "将用户指令派发给工作空间的 Thing Agent 异步执行。\
         仅当用户明确要求去执行/去处理某项任务（如“把 3 号产线温度调到 25 度”、\
         “重启网关”），而不是仅查询信息或闲聊时使用。\
         调用成功后立即回复用户“已受理，完成后回报”，不要等待执行结果；\
         任务完成后系统会主动向用户回报。"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "用户指令原文（必需，非空）"
                }
            },
            "required": ["text"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        let text = args
            .get("text")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .ok_or_else(|| anyhow::anyhow!("缺少必需参数: text（非空字符串）"))?;

        let Some(sink) = self.resolve_sink() else {
            return tool_err("Agent 任务服务未就绪，请稍后重试");
        };

        let signal = WakeSignal {
            workspace_id: self.workspace_id.clone(),
            priority: Priority::High,
            source: TriggerSource::UserDirective {
                user_id: "chat".to_string(),
                text: text.to_string(),
                session_key: None,
                source: None, // None = chat/API 用户指令（不节流、不去 merge 窗口）
                problem_key: None,
            },
            dedup_key: None,
        };

        match sink.enqueue(signal) {
            Ok(()) => tool_ok(json!({
                "status": "accepted",
                "taskId": uuid::Uuid::new_v4().to_string(),
                "message": "已受理，完成后回报",
            })),
            Err(EnqueueError::Rejected) => tool_err("任务队列已满（上限 50 条），请稍后重试"),
            Err(EnqueueError::Duplicate) => tool_err("相同指令已在队列中（60 秒内去重），无需重复投递"),
            Err(EnqueueError::Closed) => tool_err("Agent 任务服务已停止"),
            Err(other) => tool_err(format!("指令投递失败: {}", other)),
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use crate::domains::agent::loop_::thing_agent::EnqueueError;

    use super::*;
    use crate::domains::agent::host::directive_sink::StubDirectiveSink;

    fn tool_with(sink: Arc<StubDirectiveSink>) -> DispatchThingTaskTool {
        DispatchThingTaskTool::new("ws_1", Some(sink))
    }

    // ── 参数校验 ────────────────────────────────

    #[tokio::test]
    async fn missing_text_is_rejected() {
        let tool = tool_with(Arc::new(StubDirectiveSink::default()));
        // thing.rs 先例：缺少必需参数 → execute 返回 Err
        let err = tool.execute(json!({})).await.expect_err("missing text must error");
        assert!(err.to_string().contains("text"));
    }

    #[tokio::test]
    async fn blank_text_is_rejected() {
        let tool = tool_with(Arc::new(StubDirectiveSink::default()));
        for args in [json!({"text": ""}), json!({"text": "   "}), json!({"text": 42})] {
            let result = tool.execute(args).await;
            assert!(result.is_err(), "args must be rejected: text blank/非字符串");
        }
    }

    // ── 投递 ────────────────────────────────

    #[tokio::test]
    async fn valid_text_dispatches_user_directive_signal() {
        let stub = Arc::new(StubDirectiveSink::default());
        let tool = tool_with(stub.clone());

        let result = tool
            .execute(json!({"text": "  把 3 号产线温度调到 25 度 "}))
            .await
            .expect("execute");
        assert!(result.success, "dispatch must succeed: {:?}", result.error);
        let output: Value = serde_json::from_str(&result.output).expect("output json");
        assert_eq!(output["status"], "accepted");
        assert!(output["taskId"].as_str().is_some_and(|t| !t.is_empty()));
        assert_eq!(output["message"], "已受理，完成后回报");

        let signals = stub.signals();
        assert_eq!(signals.len(), 1);
        let sig = &signals[0];
        assert_eq!(sig.workspace_id, "ws_1");
        assert_eq!(sig.priority, Priority::High);
        assert!(sig.dedup_key.is_none());
        match &sig.source {
            TriggerSource::UserDirective { text, source, .. } => {
                // 原文 trim 后投递
                assert_eq!(text, "把 3 号产线温度调到 25 度");
                assert!(source.is_none(), "chat 指令 source 必须为 None（不节流）");
            }
            other => panic!("expected UserDirective, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn queue_full_maps_to_retry_message() {
        let tool = tool_with(Arc::new(StubDirectiveSink::failing(EnqueueError::Rejected)));
        let result = tool.execute(json!({"text": "重启网关"})).await.expect("execute");
        assert!(!result.success);
        assert!(result.error.unwrap().contains("队列已满"));
    }

    #[tokio::test]
    async fn duplicate_maps_to_dedup_message() {
        let tool = tool_with(Arc::new(StubDirectiveSink::failing(EnqueueError::Duplicate)));
        let result = tool.execute(json!({"text": "重启网关"})).await.expect("execute");
        assert!(!result.success);
        assert!(result.error.unwrap().contains("去重"));
    }

    #[tokio::test]
    async fn no_sink_reports_service_unavailable() {
        // 构造注入 None —— 与生产启动前（sink 未接线）行为一致。
        let tool = DispatchThingTaskTool::new("ws_1", None);
        let result = tool.execute(json!({"text": "重启网关"})).await.expect("execute");
        assert!(!result.success);
        assert!(result.error.unwrap().contains("未就绪"));
    }
}
