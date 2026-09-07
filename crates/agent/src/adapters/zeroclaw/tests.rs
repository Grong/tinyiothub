//! zeroclaw 适配器桥接测试。

use std::sync::{Arc, Mutex};

use crate::adapters::zeroclaw::tools::PortToolAsZeroclaw;
use crate::port::attribution::{Attributable, Role, ToolKind};
use crate::port::tool::{Tool, ToolResult};

struct DummyTool;

impl Attributable for DummyTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Plugin)
    }
    fn alias(&self) -> &str {
        "dummy"
    }
}

#[async_trait::async_trait]
impl Tool for DummyTool {
    fn name(&self) -> &str {
        "dummy_tool"
    }
    fn description(&self) -> &str {
        "A dummy tool for bridge tests"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": { "x": { "type": "string" } },
        })
    }
    async fn execute(&self, _args: serde_json::Value) -> anyhow::Result<ToolResult> {
        Ok(ToolResult {
            success: true,
            output: "ok".into(),
            error: None,
        })
    }
}

#[test]
fn port_tool_bridge_preserves_identity() {
    let bridge = PortToolAsZeroclaw(Box::new(DummyTool));

    // zeroclaw 侧视图
    use zeroclaw::tools::Tool as ZcTool;
    assert_eq!(bridge.name(), "dummy_tool");
    assert_eq!(bridge.description(), "A dummy tool for bridge tests");
    assert_eq!(
        bridge.parameters_schema(),
        serde_json::json!({"type":"object","properties":{"x":{"type":"string"}}})
    );
    let spec = bridge.spec();
    assert_eq!(spec.name, "dummy_tool");
    assert_eq!(spec.description, "A dummy tool for bridge tests");

    // port 侧视图不变
    assert_eq!(bridge.0.name(), "dummy_tool");
}

#[test]
fn kind_conversion_round_trips_port_side() {
    use crate::adapters::zeroclaw::tools::{port_memory_kind, port_tool_kind, zc_memory_kind, zc_role, zc_tool_kind};
    // port → zc → port 恒等（变体名/顺序两侧一致是宏的编译期前提）
    for kind in [ToolKind::Plugin, ToolKind::Search, ToolKind::Shell] {
        assert_eq!(port_tool_kind(zc_tool_kind(kind)), kind);
    }
    let mk = crate::port::attribution::MemoryKind::None;
    assert_eq!(port_memory_kind(zc_memory_kind(mk)), mk);

    // zc_role 冒烟：结构映射到 zeroclaw 侧对应变体
    match zc_role(&Role::Tool(ToolKind::Search)) {
        zeroclaw_api::attribution::Role::Tool(zeroclaw_api::attribution::ToolKind::Search) => {}
        other => panic!("unexpected bridged role: {other:?}"),
    }
}

// ── normalize_zeroclaw_messages 恒等 round-trip ───────────────

#[test]
fn normalize_zeroclaw_messages_is_identity_for_json_content() {
    use crate::adapters::zeroclaw::provider::normalize_zeroclaw_messages;

    // assistant 工具调用 JSON 与 role=tool JSON 两种消息 —— role/content
    // 必须逐字保留（归一化只换载体，不动内容）。
    let zc = vec![
        zeroclaw::providers::traits::ChatMessage {
            role: "assistant".into(),
            content: r#"{"tool_calls":[{"id":"call_1","name":"read_property","arguments":"{\"thingId\":\"t1\"}"}]}"#
                .into(),
        },
        zeroclaw::providers::traits::ChatMessage {
            role: "tool".into(),
            content: r#"{"value":21}"#.into(),
        },
    ];

    let port = normalize_zeroclaw_messages(&zc);

    assert_eq!(port.len(), 2);
    assert_eq!(port[0].role, "assistant");
    assert_eq!(
        port[0].content,
        r#"{"tool_calls":[{"id":"call_1","name":"read_property","arguments":"{\"thingId\":\"t1\"}"}]}"#
    );
    assert_eq!(port[1].role, "tool");
    assert_eq!(port[1].content, r#"{"value":21}"#);
}

// ── ZeroclawAgentLoop 事件转发 ────────────────────────────────

/// ScriptModel：第一次 chat 返回工具调用，第二次返回最终文本 + usage。
struct ScriptModel {
    calls: Mutex<usize>,
}

#[async_trait::async_trait]
impl crate::port::provider::ModelProvider for ScriptModel {
    async fn chat(
        &self,
        _request: crate::port::provider::ChatRequest<'_>,
        _model: &str,
        _temperature: Option<f64>,
    ) -> anyhow::Result<crate::port::provider::ChatResponse> {
        let mut calls = self.calls.lock().unwrap();
        *calls += 1;
        if *calls == 1 {
            return Ok(crate::port::provider::ChatResponse {
                text: Some(String::new()),
                tool_calls: vec![crate::port::provider::ToolCall {
                    id: "call_1".into(),
                    name: "recording_tool".into(),
                    arguments: r#"{"x":1}"#.into(),
                }],
                usage: Some(crate::port::provider::TokenUsage {
                    input_tokens: Some(10),
                    output_tokens: Some(5),
                    cached_input_tokens: None,
                }),
                reasoning_content: None,
            });
        }
        Ok(crate::port::provider::ChatResponse {
            text: Some("final answer".into()),
            tool_calls: vec![],
            usage: Some(crate::port::provider::TokenUsage {
                input_tokens: Some(20),
                output_tokens: Some(7),
                cached_input_tokens: Some(3),
            }),
            reasoning_content: None,
        })
    }
}

impl Attributable for ScriptModel {
    fn role(&self) -> Role {
        Role::Provider(crate::port::attribution::ProviderKind::Model(
            crate::port::attribution::ModelProviderKind::Custom,
        ))
    }
    fn alias(&self) -> &str {
        "ScriptModel"
    }
}

/// RecordingTool：记录收到的参数，返回固定输出。
struct RecordingTool {
    calls: Arc<Mutex<Vec<serde_json::Value>>>,
}

impl Attributable for RecordingTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Plugin)
    }
    fn alias(&self) -> &str {
        "RecordingTool"
    }
}

#[async_trait::async_trait]
impl Tool for RecordingTool {
    fn name(&self) -> &str {
        "recording_tool"
    }
    fn description(&self) -> &str {
        "Records its arguments"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object"})
    }
    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        self.calls.lock().unwrap().push(args);
        Ok(ToolResult {
            success: true,
            output: "recorded".into(),
            error: None,
        })
    }
}

/// 端到端：port 零件（ScriptModel + RecordingTool）经 zeroclaw_loop_factory
/// 组 loop，turn_streamed 的事件流必须逐变体正确转发（ToolCall/ToolResult/
/// Usage），最终文本透传。
#[tokio::test]
async fn zeroclaw_loop_forwards_tool_and_usage_events() {
    use crate::adapters::zeroclaw::loop_::zeroclaw_loop_factory;
    use crate::port::events::TurnEvent;
    use crate::port::runtime::AgentLoopConfig;

    let tool_calls: Arc<Mutex<Vec<serde_json::Value>>> = Arc::new(Mutex::new(vec![]));
    let tool = RecordingTool {
        calls: Arc::clone(&tool_calls),
    };
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = AgentLoopConfig {
        model_name: "script-model".into(),
        prompt_builder: crate::port::prompt::SystemPromptBuilder::with_defaults(),
        tools: vec![Box::new(tool)],
        memory: Arc::new(crate::port::memory::NoopMemory),
        observer: Arc::new(crate::port::observer::NoopObserver),
        workspace_dir: dir.path().to_path_buf(),
        security_summary: None,
        provider_factory: Arc::new(|| Ok(Box::new(ScriptModel { calls: Mutex::new(0) }))),
        conversation_memory: false,
    };
    let loop_ = zeroclaw_loop_factory(cfg).expect("factory builds loop");

    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<TurnEvent>(64);
    let final_text = loop_
        .lock()
        .await
        .turn_streamed("go", event_tx, None)
        .await
        .expect("turn completes");

    // 转发任务在 turn_streamed 返回前已排空事件；收集至 channel 关闭。
    let mut events = Vec::new();
    while let Some(evt) = event_rx.recv().await {
        events.push(evt);
    }

    assert_eq!(final_text, "final answer");
    assert_eq!(
        tool_calls.lock().unwrap().as_slice(),
        &[serde_json::json!({"x": 1})],
        "tool must receive the LLM's arguments verbatim"
    );

    let tool_call = events.iter().find_map(|e| match e {
        TurnEvent::ToolCall { id, name, args } => Some((id, name, args)),
        _ => None,
    });
    let (id, name, args) = tool_call.expect("a ToolCall event");
    assert_eq!(id, "call_1");
    assert_eq!(name, "recording_tool");
    assert_eq!(*args, serde_json::json!({"x": 1}));

    let tool_result = events.iter().find_map(|e| match e {
        TurnEvent::ToolResult { id, name, output } => Some((id, name, output)),
        _ => None,
    });
    let (rid, rname, output) = tool_result.expect("a ToolResult event");
    assert_eq!(rid, "call_1");
    assert_eq!(rname, "recording_tool");
    assert_eq!(output, "recorded");

    // 两次模型调用各产生一个 Usage 事件，token 数逐字段转发。
    let usages: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            TurnEvent::Usage {
                input_tokens,
                cached_input_tokens,
                output_tokens,
                ..
            } => Some((*input_tokens, *cached_input_tokens, *output_tokens)),
            _ => None,
        })
        .collect();
    assert_eq!(usages, vec![(Some(10), None, Some(5)), (Some(20), Some(3), Some(7))]);
}
