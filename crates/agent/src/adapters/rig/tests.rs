//! rig 适配器桥接测试。

use std::sync::{Arc, Mutex};

use crate::port::attribution::{Attributable, Role, ToolKind};
use crate::port::tool::{Tool, ToolResult};

// ── canonical 拍平 round-trip ───────────────────────────────

/// port ChatMessage 历史 ↔ rig Message 往返：assistant 工具调用 JSON、
/// role=tool JSON、reasoning 附加键、纯文本 user/system 逐字保留。
#[test]
fn canonical_flatten_round_trip_assistant_tool_call_turn() {
    use crate::adapters::rig::provider::{port_messages_to_rig, rig_messages_to_port};
    use rig_core::message::{AssistantContent, Message, UserContent};

    let port = vec![
        crate::port::provider::ChatMessage::system("sys"),
        crate::port::provider::ChatMessage::user("u1"),
        crate::port::provider::ChatMessage::assistant(
            r#"{"content":"thinking text","reasoning_content":"why","tool_calls":[{"arguments":"{\"thingId\":\"t1\"}","id":"call_1","name":"read_property"}]}"#,
        ),
        crate::port::provider::ChatMessage::tool(r#"{"content":"{\"value\":21}","tool_call_id":"call_1"}"#),
    ];

    let rig = port_messages_to_rig(&port);

    assert!(matches!(&rig[0], Message::System { content } if content == "sys"));
    match &rig[1] {
        Message::User { content } => {
            assert!(matches!(&content[0], UserContent::Text(t) if t.text == "u1"));
        }
        other => panic!("expected user message, got {other:?}"),
    }
    match &rig[2] {
        Message::Assistant { content, .. } => {
            let mut text = None;
            let mut reasoning = None;
            let mut calls = 0;
            for block in content {
                match block {
                    AssistantContent::Text(t) => text = Some(t.text.clone()),
                    AssistantContent::Reasoning(r) => {
                        reasoning = Some(match &r.content[0] {
                            rig_core::message::ReasoningContent::Text { text, .. } => text.clone(),
                            other => panic!("unexpected reasoning content: {other:?}"),
                        });
                    }
                    AssistantContent::ToolCall(tc) => {
                        calls += 1;
                        assert_eq!(tc.id.to_string(), "call_1");
                        assert_eq!(tc.function.name, "read_property");
                        assert_eq!(tc.function.arguments, serde_json::json!({"thingId": "t1"}));
                    }
                    other => panic!("unexpected assistant content: {other:?}"),
                }
            }
            assert_eq!(text.as_deref(), Some("thinking text"));
            assert_eq!(reasoning.as_deref(), Some("why"));
            assert_eq!(calls, 1);
        }
        other => panic!("expected assistant message, got {other:?}"),
    }
    match &rig[3] {
        Message::User { content } => match &content[0] {
            UserContent::ToolResult(tr) => {
                assert_eq!(tr.call.to_string(), "call_1");
                assert_eq!(tr.content[0].as_text(), Some(r#"{"value":21}"#));
            }
            other => panic!("expected tool result, got {other:?}"),
        },
        other => panic!("expected user message, got {other:?}"),
    }

    // 回程：rig → port 必须与原始 canonical 字符串逐字一致。
    let back = rig_messages_to_port(&rig);
    assert_eq!(back.len(), port.len());
    for (original, round_tripped) in port.iter().zip(back.iter()) {
        assert_eq!(original.role, round_tripped.role, "role must survive round-trip");
        assert_eq!(
            original.content, round_tripped.content,
            "canonical content must survive round-trip"
        );
    }
}

/// rig 结构化消息 → port canonical：纯文本 assistant 也走 JSON 形状
/// （{"content":..., "tool_calls":[]}），assistant 同轮多条 tool call 合并为一条。
#[test]
fn rig_assistant_flattens_to_canonical_json() {
    use crate::adapters::rig::provider::rig_messages_to_port;
    use rig_core::message::{AssistantContent, Message, Reasoning, Text, ToolCallId, ToolFunction};

    let rig = vec![
        Message::assistant("plain answer"),
        Message::Assistant {
            id: None,
            content: vec![
                AssistantContent::Reasoning(Reasoning::new("r1")),
                AssistantContent::Text(Text::new("partial")),
                AssistantContent::ToolCall(rig_core::message::ToolCall::new(
                    ToolCallId::new_or_mint("call_a"),
                    ToolFunction::new("t1".into(), serde_json::json!({"a": 1})),
                )),
                AssistantContent::ToolCall(rig_core::message::ToolCall::new(
                    ToolCallId::new_or_mint("call_b"),
                    ToolFunction::new("t2".into(), serde_json::json!({"b": 2})),
                )),
            ],
        },
    ];

    let port = rig_messages_to_port(&rig);

    assert_eq!(port.len(), 2);
    assert_eq!(port[0].role, "assistant");
    assert_eq!(port[0].content, r#"{"content":"plain answer","tool_calls":[]}"#);
    assert_eq!(port[1].role, "assistant");
    assert_eq!(
        port[1].content,
        r#"{"content":"partial","reasoning_content":"r1","tool_calls":[{"arguments":"{\"a\":1}","id":"call_a","name":"t1"},{"arguments":"{\"b\":2}","id":"call_b","name":"t2"}]}"#
    );
}

/// rig ToolResult → port tool 消息：content 文本化进 canonical JSON。
#[test]
fn rig_tool_result_flattens_to_canonical_json() {
    use crate::adapters::rig::provider::rig_messages_to_port;
    use rig_core::message::{Message, Text, ToolCallId, ToolResultContent};

    let rig = vec![Message::User {
        content: vec![rig_core::message::UserContent::ToolResult(
            rig_core::message::ToolResult {
                call: ToolCallId::new_or_mint("call_9"),
                provider: None,
                name: "t".into(),
                content: vec![ToolResultContent::Text(Text::new("ok output"))],
            },
        )],
    }];

    let port = rig_messages_to_port(&rig);

    assert_eq!(port.len(), 1);
    assert_eq!(port[0].role, "tool");
    assert_eq!(port[0].content, r#"{"content":"ok output","tool_call_id":"call_9"}"#);
}

// ── RigAgentLoop 事件转发 ────────────────────────────────────

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

/// 端到端：port 零件（ScriptModel + RecordingTool）经 rig_loop_factory
/// 组 loop，turn_streamed 的事件流必须逐变体正确转发（ToolCall/ToolResult/
/// Usage），最终文本透传。
#[tokio::test]
async fn rig_loop_forwards_tool_and_usage_events() {
    use crate::adapters::rig::loop_::rig_loop_factory;
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
    let loop_ = rig_loop_factory(cfg).expect("factory builds loop");

    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<TurnEvent>(64);
    let final_text = loop_
        .lock()
        .await
        .turn_streamed("go", event_tx, None)
        .await
        .expect("turn completes");

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

    // 两次模型调用各产生一个 Usage 事件。rig Usage 是全量 u64，
    // cached 未上报时回落为 Some(0)（zeroclaw 侧为 None —— 文档化差异）。
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
    assert_eq!(usages, vec![(Some(10), Some(0), Some(5)), (Some(20), Some(3), Some(7))]);
}

/// success=false 的 port ToolResult 作为普通 tool result 回传（JSON error 形状），
/// 不走 rig 错误重试路径；模型照常收 tool result 并给出最终文本。
#[tokio::test]
async fn failing_tool_result_is_delivered_as_text_not_retried() {
    use crate::adapters::rig::loop_::rig_loop_factory;
    use crate::port::runtime::AgentLoopConfig;

    struct FailingTool;
    crate::mock_tool_attribution!(FailingTool);
    #[async_trait::async_trait]
    impl Tool for FailingTool {
        fn name(&self) -> &str {
            "recording_tool"
        }
        fn description(&self) -> &str {
            "Always reports failure"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }
        async fn execute(&self, _args: serde_json::Value) -> anyhow::Result<ToolResult> {
            Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("boom".into()),
            })
        }
    }

    /// AssertModel：第二轮必须看到 canonical 形状的 tool result（含 success:false）。
    struct AssertModel {
        calls: Mutex<usize>,
    }
    impl Attributable for AssertModel {
        fn role(&self) -> Role {
            Role::Provider(crate::port::attribution::ProviderKind::Model(
                crate::port::attribution::ModelProviderKind::Custom,
            ))
        }
        fn alias(&self) -> &str {
            "AssertModel"
        }
    }
    #[async_trait::async_trait]
    impl crate::port::provider::ModelProvider for AssertModel {
        async fn chat(
            &self,
            request: crate::port::provider::ChatRequest<'_>,
            _model: &str,
            _temperature: Option<f64>,
        ) -> anyhow::Result<crate::port::provider::ChatResponse> {
            let mut calls = self.calls.lock().unwrap();
            *calls += 1;
            if *calls == 1 {
                return Ok(crate::port::provider::ChatResponse {
                    text: None,
                    tool_calls: vec![crate::port::provider::ToolCall {
                        id: "call_1".into(),
                        name: "recording_tool".into(),
                        arguments: r#"{"x":1}"#.into(),
                    }],
                    usage: None,
                    reasoning_content: None,
                });
            }
            let tool_msg = request
                .messages
                .iter()
                .find(|m| m.role == "tool")
                .expect("tool result must be in history");
            let parsed: serde_json::Value = serde_json::from_str(&tool_msg.content).expect("canonical tool JSON");
            assert_eq!(parsed["tool_call_id"], "call_1");
            let inner: serde_json::Value = serde_json::from_str(parsed["content"].as_str().expect("string content"))
                .expect("tool output is a JSON error envelope");
            assert_eq!(inner["success"], false);
            assert_eq!(inner["error"], "boom");
            Ok(crate::port::provider::ChatResponse {
                text: Some("recovered".into()),
                tool_calls: vec![],
                usage: None,
                reasoning_content: None,
            })
        }
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = AgentLoopConfig {
        model_name: "assert-model".into(),
        prompt_builder: crate::port::prompt::SystemPromptBuilder::with_defaults(),
        tools: vec![Box::new(FailingTool)],
        memory: Arc::new(crate::port::memory::NoopMemory),
        observer: Arc::new(crate::port::observer::NoopObserver),
        workspace_dir: dir.path().to_path_buf(),
        security_summary: None,
        provider_factory: Arc::new(|| Ok(Box::new(AssertModel { calls: Mutex::new(0) }))),
        conversation_memory: false,
    };
    let loop_ = rig_loop_factory(cfg).expect("factory builds loop");

    let (event_tx, _event_rx) = tokio::sync::mpsc::channel::<crate::port::events::TurnEvent>(64);
    let final_text = loop_
        .lock()
        .await
        .turn_streamed("go", event_tx, None)
        .await
        .expect("turn completes despite tool failure");

    assert_eq!(final_text, "recovered");
}

// ── C-1：ConversationMemory::load 顺序 ──────────────────────

/// DescMemory：recall 按 zeroclaw 后端的 `ORDER BY updated_at DESC`
/// 预置 3 条（最新在前）。
struct DescMemory {
    entries: Vec<crate::port::memory::MemoryEntry>,
}

impl Attributable for DescMemory {
    fn role(&self) -> Role {
        Role::Memory(crate::port::attribution::MemoryKind::None)
    }
    fn alias(&self) -> &str {
        "DescMemory"
    }
}

fn desc_entry(key: &str, content: &str) -> crate::port::memory::MemoryEntry {
    crate::port::memory::MemoryEntry {
        id: content.into(),
        key: key.into(),
        content: content.into(),
        category: crate::port::memory::MemoryCategory::Conversation,
        timestamp: String::new(),
        session_id: Some("default".into()),
        score: None,
        namespace: "default".into(),
        importance: None,
        superseded_by: None,
        agent_alias: None,
        agent_id: None,
    }
}

#[async_trait::async_trait]
impl crate::port::memory::Memory for DescMemory {
    fn name(&self) -> &str {
        "desc"
    }
    async fn store(
        &self,
        _key: &str,
        _content: &str,
        _category: crate::port::memory::MemoryCategory,
        _session_id: Option<&str>,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn recall(
        &self,
        _query: &str,
        _limit: usize,
        _session_id: Option<&str>,
        _since: Option<&str>,
        _until: Option<&str>,
    ) -> anyhow::Result<Vec<crate::port::memory::MemoryEntry>> {
        Ok(self.entries.clone())
    }
    async fn get(&self, _key: &str) -> anyhow::Result<Option<crate::port::memory::MemoryEntry>> {
        Ok(None)
    }
    async fn list(
        &self,
        _category: Option<&crate::port::memory::MemoryCategory>,
        _session_id: Option<&str>,
    ) -> anyhow::Result<Vec<crate::port::memory::MemoryEntry>> {
        Ok(vec![])
    }
    async fn forget(&self, _key: &str) -> anyhow::Result<bool> {
        Ok(false)
    }
    async fn forget_for_agent(&self, _key: &str, _agent_id: &str) -> anyhow::Result<bool> {
        Ok(false)
    }
    async fn count(&self) -> anyhow::Result<usize> {
        Ok(self.entries.len())
    }
    async fn health_check(&self) -> bool {
        true
    }
    async fn store_with_agent(
        &self,
        _key: &str,
        _content: &str,
        _category: crate::port::memory::MemoryCategory,
        _session_id: Option<&str>,
        _namespace: Option<&str>,
        _importance: Option<f64>,
        _agent_id: Option<&str>,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn recall_for_agents(
        &self,
        _allowed_agent_ids: &[&str],
        _query: &str,
        _limit: usize,
        _session_id: Option<&str>,
        _since: Option<&str>,
        _until: Option<&str>,
    ) -> anyhow::Result<Vec<crate::port::memory::MemoryEntry>> {
        Ok(vec![])
    }
}

/// load 必须把 DESC（最新在前）的召回结果反转为时间正序（最早在前）。
#[tokio::test]
async fn conversation_load_reverses_desc_recall_to_ascending() {
    use crate::adapters::rig::memory::PortMemoryAsConversation;
    use rig_core::memory::ConversationMemory;
    use rig_core::message::{Message, UserContent};

    let memory = Arc::new(DescMemory {
        entries: vec![
            desc_entry("user", "third"),
            desc_entry("assistant", r#"{"content":"second","tool_calls":[]}"#),
            desc_entry("user", "first"),
        ],
    });
    let bridge = PortMemoryAsConversation::new(memory);

    let messages = bridge.load("default").await.expect("load succeeds");

    let texts: Vec<String> = messages
        .iter()
        .map(|m| match m {
            Message::User { content } => match &content[0] {
                UserContent::Text(t) => t.text.clone(),
                other => panic!("expected text user content, got {other:?}"),
            },
            Message::Assistant { content, .. } => match &content[0] {
                rig_core::message::AssistantContent::Text(t) => t.text.clone(),
                other => panic!("expected text assistant content, got {other:?}"),
            },
            Message::System { content } => content.clone(),
        })
        .collect();
    assert_eq!(texts, vec!["first", "second", "third"]);
}

// ── I-2：stream 结束后 token 已 cancel 的归一化 ─────────────

/// CancellingModel：在最后一次 chat 调用里 cancel token 后返回最终文本，
/// 模拟"stream 已结束但 token 已 cancel"（如 BudgetHook 超预算先 cancel 再 stop）。
struct CancellingModel {
    calls: Mutex<usize>,
    token: tokio_util::sync::CancellationToken,
}

impl Attributable for CancellingModel {
    fn role(&self) -> Role {
        Role::Provider(crate::port::attribution::ProviderKind::Model(
            crate::port::attribution::ModelProviderKind::Custom,
        ))
    }
    fn alias(&self) -> &str {
        "CancellingModel"
    }
}

#[async_trait::async_trait]
impl crate::port::provider::ModelProvider for CancellingModel {
    async fn chat(
        &self,
        _request: crate::port::provider::ChatRequest<'_>,
        _model: &str,
        _temperature: Option<f64>,
    ) -> anyhow::Result<crate::port::provider::ChatResponse> {
        let mut calls = self.calls.lock().unwrap();
        *calls += 1;
        self.token.cancel();
        Ok(crate::port::provider::ChatResponse {
            text: Some("late answer".into()),
            tool_calls: vec![],
            usage: None,
            reasoning_content: None,
        })
    }
}

#[tokio::test]
async fn cancelled_after_stream_end_is_tool_loop_cancelled() {
    use crate::adapters::rig::loop_::rig_loop_factory;
    use crate::port::runtime::AgentLoopConfig;

    let token = tokio_util::sync::CancellationToken::new();
    let model = CancellingModel {
        calls: Mutex::new(0),
        token: token.clone(),
    };
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = AgentLoopConfig {
        model_name: "cancelling-model".into(),
        prompt_builder: crate::port::prompt::SystemPromptBuilder::with_defaults(),
        tools: vec![],
        memory: Arc::new(crate::port::memory::NoopMemory),
        observer: Arc::new(crate::port::observer::NoopObserver),
        workspace_dir: dir.path().to_path_buf(),
        security_summary: None,
        provider_factory: Arc::new(move || {
            Ok(Box::new(CancellingModel {
                calls: Mutex::new(0),
                token: model.token.clone(),
            }))
        }),
        conversation_memory: false,
    };
    let loop_ = rig_loop_factory(cfg).expect("factory builds loop");

    let (event_tx, _event_rx) = tokio::sync::mpsc::channel::<crate::port::events::TurnEvent>(64);
    let err = loop_
        .lock()
        .await
        .turn_streamed("go", event_tx, Some(token))
        .await
        .expect_err("turn must report cancellation, not a successful text end");

    assert!(
        crate::port::outcome::is_tool_loop_cancelled(&err),
        "cancellation after stream end must normalize to ToolLoopCancelled, got {err:?}"
    );
}
