//! rig 适配器桥接测试。

use std::sync::{Arc, Mutex};

use crate::port::attribution::{Attributable, Role, ToolKind};
use crate::port::provider::ModelProvider;
use crate::port::tool::{Tool, ToolResult};

#[test]
fn rig_forwards_real_image_and_trace_keeps_only_reference() {
    use chrono::{TimeZone, Utc};
    use rig_core::message::{DocumentSourceKind, ToolResultContent};
    use tinyiothub_core::media::{EvidenceRef, MediaAttachment};

    let bytes = vec![0x89, b'P', b'N', b'G', 1, 2, 3];
    let attachment = MediaAttachment {
        evidence_id: "evidence_1".into(),
        mime: "image/png".into(),
        bytes: bytes.clone(),
        source_seq: 7,
        media_time: Utc
            .with_ymd_and_hms(2026, 10, 27, 10, 0, 0)
            .single()
            .expect("fixed time"),
    };
    let result = ToolResult {
        success: true,
        output: "image captured".into(),
        error: None,
        attachments: vec![attachment],
    };
    let content = crate::adapters::rig::provider::tool_result_to_rig(&result).expect("image mapping");
    let Some(image) = content.iter().find_map(|item| match item {
        ToolResultContent::Image(image) => Some(image),
        _ => None,
    }) else {
        panic!("image attachment missing")
    };
    assert_eq!(image.data, DocumentSourceKind::Raw(bytes));
    let rig_result = rig_core::message::ToolResult {
        call: rig_core::message::ToolCallId::new_or_mint("call_1"),
        provider: None,
        name: "read_device".into(),
        content,
    };
    let round_tripped = crate::adapters::rig::provider::tool_result_attachments(&rig_result).expect("attachments");
    assert_eq!(round_tripped[0].evidence_id, "evidence_1");
    assert_eq!(round_tripped[0].source_seq, 7);
    assert_eq!(round_tripped[0].media_time, result.attachments[0].media_time);
    let trace = EvidenceRef::from(&result.attachments[0]);
    assert!(!serde_json::to_string(&trace).expect("trace").contains("base64"));
    let mut two = result;
    two.attachments.push(two.attachments[0].clone());
    assert!(matches!(
        crate::adapters::rig::provider::tool_result_to_rig(&two),
        Err(tinyiothub_core::media::MediaError::TooManyImages)
    ));
}

#[tokio::test]
async fn non_vision_provider_returns_image_unavailable() {
    use crate::port::attribution::ProviderKind;
    use crate::port::provider::{ChatMessage, ChatRequest, ChatResponse, ModelProvider, ProviderError};
    use tinyiothub_core::media::MediaAttachment;

    struct TextOnly;
    impl Attributable for TextOnly {
        fn role(&self) -> Role {
            Role::Provider(ProviderKind::Model(
                crate::port::attribution::ModelProviderKind::Minimax,
            ))
        }
        fn alias(&self) -> &str {
            "text-only"
        }
    }
    #[async_trait::async_trait]
    impl ModelProvider for TextOnly {
        async fn chat(
            &self,
            _request: ChatRequest<'_>,
            _model: &str,
            _temperature: Option<f64>,
        ) -> anyhow::Result<ChatResponse> {
            panic!("non-vision provider must not receive image bytes")
        }
    }
    let provider = TextOnly;
    let mut message = ChatMessage::user("inspect");
    message.attachments.push(MediaAttachment {
        evidence_id: "e".into(),
        mime: "image/png".into(),
        bytes: vec![1],
        source_seq: 1,
        media_time: chrono::Utc::now(),
    });
    let messages = vec![message];
    let error =
        crate::adapters::rig::provider::check_vision_request(&provider, &messages).expect_err("vision required");
    assert_eq!(
        error.downcast_ref::<ProviderError>(),
        Some(&ProviderError::ImageUnavailable)
    );
}

#[tokio::test]
async fn vision_rig_provider_receives_exact_image_bytes() {
    use crate::port::provider::{ChatMessage, ChatRequest, ModelProvider};
    use rig_core::message::{DocumentSourceKind, UserContent};
    use tinyiothub_core::media::MediaAttachment;

    let (model, provider) = recording_rig_model(vec![]);
    let bytes = vec![0x89, b'P', b'N', b'G', 4, 3, 2, 1];
    let mut message = ChatMessage::user("inspect");
    message.attachments.push(MediaAttachment {
        evidence_id: "e".into(),
        mime: "image/png".into(),
        bytes: bytes.clone(),
        source_seq: 1,
        media_time: chrono::Utc::now(),
    });
    let messages = vec![message];
    provider
        .chat(
            ChatRequest {
                messages: &messages,
                tools: None,
                max_tokens: None,
            },
            "m",
            None,
        )
        .await
        .expect("vision request");
    let seen = model.seen.lock().unwrap().take().expect("request recorded");
    let rig_core::message::Message::User { content } = &seen.chat_history[0] else {
        panic!("user message")
    };
    let image = content
        .iter()
        .find_map(|block| match block {
            UserContent::Image(image) => Some(image),
            _ => None,
        })
        .expect("image attachment");
    assert_eq!(image.data, DocumentSourceKind::Raw(bytes));
    assert_eq!(image.additional_params, None);
}

#[tokio::test]
async fn direct_provider_validation_does_not_own_run_budget() {
    use crate::port::provider::{ChatMessage, ChatRequest, ModelProvider};
    use tinyiothub_core::media::MediaAttachment;

    let (model, provider) = recording_rig_model(vec![]);
    let messages: Vec<_> = (0..5)
        .map(|index| {
            let mut message = ChatMessage::user("inspect");
            message.attachments.push(MediaAttachment {
                evidence_id: format!("e{index}"),
                mime: "image/png".into(),
                bytes: vec![0; 2 * 1024 * 1024],
                source_seq: index,
                media_time: chrono::Utc::now(),
            });
            message
        })
        .collect();
    provider
        .chat(
            ChatRequest {
                messages: &messages,
                tools: None,
                max_tokens: None,
            },
            "m",
            None,
        )
        .await
        .expect("provider validation is stateless; the Run hook owns cumulative accounting");
    assert!(model.seen.lock().unwrap().is_some());
}

#[test]
fn malformed_and_multiple_rig_images_are_conversion_errors() {
    use crate::adapters::rig::provider::rig_messages_to_port;
    use rig_core::message::{DocumentSourceKind, Image, ImageMediaType, Message, ToolResultContent, UserContent};
    use tinyiothub_core::media::MediaError;

    let malformed = Image {
        data: DocumentSourceKind::Url("file:///not-raw".into()),
        media_type: Some(ImageMediaType::PNG),
        detail: None,
        additional_params: None,
    };
    assert!(matches!(
        rig_messages_to_port(&[Message::User {
            content: vec![UserContent::Image(malformed.clone())],
        }]),
        Err(MediaError::InvalidImage)
    ));

    let raw = Image {
        data: DocumentSourceKind::Raw(vec![1]),
        media_type: Some(ImageMediaType::PNG),
        detail: None,
        additional_params: None,
    };
    let result = rig_core::message::ToolResult {
        call: rig_core::message::ToolCallId::new_or_mint("call_1"),
        provider: None,
        name: "read_device".into(),
        content: vec![ToolResultContent::Image(raw.clone()), ToolResultContent::Image(raw)],
    };
    assert!(matches!(
        rig_messages_to_port(&[Message::User {
            content: vec![UserContent::ToolResult(result)],
        }]),
        Err(MediaError::TooManyImages)
    ));
}

#[test]
fn invalid_port_attachment_is_not_silently_dropped() {
    use crate::adapters::rig::provider::port_messages_to_rig;
    use tinyiothub_core::media::{MediaAttachment, MediaError};

    let mut message = crate::port::provider::ChatMessage::user("inspect");
    message.attachments.push(MediaAttachment {
        evidence_id: "e".into(),
        mime: "image/svg+xml".into(),
        bytes: vec![1],
        source_seq: 1,
        media_time: chrono::Utc::now(),
    });
    assert!(matches!(
        port_messages_to_rig(&[message]),
        Err(MediaError::UnsupportedMime(mime)) if mime == "image/svg+xml"
    ));
}

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

    let rig = port_messages_to_rig(&port).expect("rig history");

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
    let back = rig_messages_to_port(&rig).expect("port history");
    assert_eq!(back.len(), port.len());
    for (original, round_tripped) in port.iter().zip(back.iter()) {
        assert_eq!(original.role, round_tripped.role, "role must survive round-trip");
        assert_eq!(
            original.content, round_tripped.content,
            "canonical content must survive round-trip"
        );
    }
}

// ── 工具配对清洗（recall 窗口切对事故，2026-09-16）──────────────

/// 孤儿 tool_result（配对的 tool_call 掉出 recall 窗口）整条丢弃——
/// 实测：MiniMax 400 "tool result's tool id not found"，所有 run turn 1 全灭。
#[test]
fn sanitize_drops_orphan_tool_result() {
    use crate::adapters::rig::provider::sanitize_tool_pairing;
    use crate::port::provider::ChatMessage;

    let port = vec![
        ChatMessage::user("调查报警"),
        ChatMessage::tool(r#"{"content":"{\"value\":21}","tool_call_id":"call_orphan"}"#),
        ChatMessage::assistant(r#"{"content":"结论","tool_calls":[]}"#),
    ];
    let out = sanitize_tool_pairing(port);
    assert_eq!(out.len(), 2, "孤儿 tool_result 必须丢弃");
    assert!(out.iter().all(|m| m.role != "tool"));
}

/// 孤儿 tool_call（窗口内无对应 result）从 assistant 消息摘除；
/// 消息仍有文本时保留，只剩孤儿 call 时整条丢弃。
#[test]
fn sanitize_prunes_orphan_tool_call() {
    use crate::adapters::rig::provider::sanitize_tool_pairing;
    use crate::port::provider::ChatMessage;

    // 有文本的 assistant：摘掉孤儿 call，消息保留
    let with_text = vec![
        ChatMessage::user("调查报警"),
        ChatMessage::assistant(
            r#"{"content":"先查一下","tool_calls":[{"arguments":"{}","id":"call_gone","name":"read_property"}]}"#,
        ),
    ];
    let out = sanitize_tool_pairing(with_text);
    assert_eq!(out.len(), 2, "user + 摘除孤儿 call 后的 assistant 都保留");
    let v: serde_json::Value = serde_json::from_str(&out[1].content).unwrap();
    assert_eq!(v["tool_calls"].as_array().unwrap().len(), 0, "孤儿 call 被摘除");
    assert_eq!(v["content"].as_str(), Some("先查一下"));

    // 只剩孤儿 call 的空壳：整条丢弃
    let shell_only = vec![ChatMessage::assistant(
        r#"{"content":null,"tool_calls":[{"arguments":"{}","id":"call_gone","name":"read_property"}]}"#,
    )];
    assert_eq!(sanitize_tool_pairing(shell_only).len(), 0);
}

/// 完整配对的 call+result 原样保留（不误伤）。
#[test]
fn sanitize_keeps_intact_pairs() {
    use crate::adapters::rig::provider::sanitize_tool_pairing;
    use crate::port::provider::ChatMessage;

    let port = vec![
        ChatMessage::assistant(
            r#"{"content":"thinking","tool_calls":[{"arguments":"{}","id":"call_1","name":"read_property"}]}"#,
        ),
        ChatMessage::tool(r#"{"content":"{\"value\":21}","tool_call_id":"call_1"}"#),
    ];
    assert_eq!(sanitize_tool_pairing(port).len(), 2, "完整配对不得误伤");
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

    let port = rig_messages_to_port(&rig).expect("port history");

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

    let port = rig_messages_to_port(&rig).expect("port history");

    assert_eq!(port.len(), 1);
    assert_eq!(port[0].role, "tool");
    assert_eq!(port[0].content, r#"{"content":"ok output","tool_call_id":"call_9"}"#);
}

// ── RigMinimaxAsPort round-trip ──────────────────────────────

/// 记录收到的 CompletionRequest 并回放固定 CompletionResponse 的假 rig 模型。
struct RecordingRigModel {
    seen: Mutex<Option<rig_core::completion::CompletionRequest>>,
    response: Vec<rig_core::message::AssistantContent>,
    usage: rig_core::completion::Usage,
}

impl rig_core::completion::CompletionModel for RecordingRigModel {
    async fn completion(
        &self,
        request: rig_core::completion::CompletionRequest,
    ) -> Result<rig_core::completion::CompletionResponse, rig_core::completion::CompletionError> {
        *self.seen.lock().unwrap() = Some(request);
        Ok(rig_core::completion::CompletionResponse::new(
            self.response.clone(),
            self.usage,
            "recording",
        ))
    }

    async fn stream(
        &self,
        request: rig_core::completion::CompletionRequest,
    ) -> Result<rig_core::streaming::StreamingCompletionResponse, rig_core::completion::CompletionError> {
        let resp = self.completion(request).await?;
        Ok(rig_core::streaming::StreamingCompletionResponse::stream(
            "recording".to_string(),
            Box::pin(futures::stream::iter([Ok(
                rig_core::streaming::RawStreamingChoice::FinalResponse(rig_core::streaming::StreamFinal::new(
                    "recording".to_string(),
                    resp.usage,
                )),
            )])),
        ))
    }
}

fn recording_rig_model(
    response: Vec<rig_core::message::AssistantContent>,
) -> (
    Arc<RecordingRigModel>,
    crate::adapters::rig::provider::RigMinimaxAsPort<Arc<RecordingRigModel>>,
) {
    let model = Arc::new(RecordingRigModel {
        seen: Mutex::new(None),
        response,
        usage: rig_core::completion::Usage {
            input_tokens: 11,
            output_tokens: 4,
            total_tokens: 15,
            cached_input_tokens: 2,
            cache_creation_input_tokens: 0,
            tool_use_prompt_tokens: 0,
            reasoning_tokens: 0,
        },
    });
    let port = crate::adapters::rig::provider::RigMinimaxAsPort::new(Arc::clone(&model));
    (model, port)
}

/// port ChatRequest（含 assistant tool_call canonical JSON）→ rig 请求结构化
/// 还原；rig 响应 ToolCall → port ChatResponse.tool_calls。
#[tokio::test]
async fn rig_minimax_as_port_round_trip_tool_call() {
    use crate::port::provider::ModelProvider;
    use rig_core::message::{AssistantContent, Message, ToolCallId, ToolFunction, UserContent};

    let (model, port) = recording_rig_model(vec![AssistantContent::ToolCall(rig_core::message::ToolCall::new(
        ToolCallId::new_or_mint("call_out"),
        ToolFunction::new(
            "write_property".into(),
            serde_json::json!({"thingId": "t1", "value": 42}),
        ),
    ))]);

    let messages = vec![
        crate::port::provider::ChatMessage::system("sys"),
        crate::port::provider::ChatMessage::user("u1"),
        crate::port::provider::ChatMessage::assistant(
            r#"{"content":"thinking","tool_calls":[{"arguments":"{\"thingId\":\"t1\"}","id":"call_1","name":"read_property"}]}"#,
        ),
    ];
    let specs = vec![crate::port::tool::ToolSpec {
        name: "read_property".into(),
        description: "read a thing property".into(),
        parameters: serde_json::json!({"type": "object", "properties": {"thingId": {"type": "string"}}}),
    }];
    let request = crate::port::provider::ChatRequest {
        messages: &messages,
        tools: Some(&specs),
        max_tokens: None,
    };

    let resp = port.chat(request, "MiniMax-M2.7", Some(0.3)).await.expect("chat");

    // 出方向：canonical assistant JSON 还原为结构化 ToolCall。
    let seen = model.seen.lock().unwrap().take().expect("request recorded");
    assert_eq!(seen.model.as_deref(), Some("MiniMax-M2.7"));
    assert_eq!(seen.temperature, Some(0.3));
    assert!(matches!(&seen.chat_history[0], Message::System { content } if content == "sys"));
    assert!(
        matches!(&seen.chat_history[1], Message::User { content } if matches!(&content[0], UserContent::Text(t) if t.text == "u1"))
    );
    match &seen.chat_history[2] {
        Message::Assistant { content, .. } => {
            let tc = content
                .iter()
                .find_map(|b| match b {
                    AssistantContent::ToolCall(tc) => Some(tc),
                    _ => None,
                })
                .expect("tool call block");
            assert_eq!(tc.id.to_string(), "call_1");
            assert_eq!(tc.function.name, "read_property");
            assert_eq!(tc.function.arguments, serde_json::json!({"thingId": "t1"}));
            assert!(
                content
                    .iter()
                    .any(|b| matches!(b, AssistantContent::Text(t) if t.text == "thinking"))
            );
        }
        other => panic!("expected assistant message, got {other:?}"),
    }
    // 工具规范透传。
    assert_eq!(seen.tools.len(), 1);
    assert_eq!(seen.tools[0].name, "read_property");
    assert_eq!(seen.tools[0].description, "read a thing property");
    assert_eq!(
        seen.tools[0].parameters,
        serde_json::json!({"type": "object", "properties": {"thingId": {"type": "string"}}})
    );

    // 回方向：rig ToolCall → port tool_calls。
    assert_eq!(resp.text, None);
    assert_eq!(resp.tool_calls.len(), 1);
    assert_eq!(resp.tool_calls[0].id, "call_out");
    assert_eq!(resp.tool_calls[0].name, "write_property");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&resp.tool_calls[0].arguments).unwrap(),
        serde_json::json!({"thingId": "t1", "value": 42})
    );
    let usage = resp.usage.expect("usage");
    assert_eq!(usage.input_tokens, Some(11));
    assert_eq!(usage.output_tokens, Some(4));
    assert_eq!(usage.cached_input_tokens, Some(2));

    // 响应回拍平为 canonical（与 Task 7 编码互逆闭合）。
    let flat = crate::adapters::rig::provider::rig_messages_to_port(&[Message::Assistant {
        id: None,
        content: vec![AssistantContent::ToolCall(rig_core::message::ToolCall::new(
            ToolCallId::new_or_mint("call_out"),
            ToolFunction::new(
                "write_property".into(),
                serde_json::json!({"thingId": "t1", "value": 42}),
            ),
        ))],
    }])
    .expect("flattened response");
    assert_eq!(flat.len(), 1);
    assert_eq!(flat[0].role, "assistant");
    let v: serde_json::Value = serde_json::from_str(&flat[0].content).unwrap();
    assert_eq!(v["tool_calls"][0]["id"], "call_out");
    assert_eq!(v["tool_calls"][0]["name"], "write_property");
}

/// port ChatRequest（含 role=tool canonical JSON）→ rig 请求结构化 ToolResult；
/// rig 文本响应 → port ChatResponse.text。
#[tokio::test]
async fn rig_minimax_as_port_round_trip_tool_result() {
    use crate::port::provider::ModelProvider;
    use rig_core::message::{AssistantContent, Message, Reasoning, Text, UserContent};

    let (model, port) = recording_rig_model(vec![
        AssistantContent::Reasoning(Reasoning::new("because")),
        AssistantContent::Text(Text::new("done")),
    ]);

    let messages = vec![
        crate::port::provider::ChatMessage::user("u1"),
        crate::port::provider::ChatMessage::tool(r#"{"content":"{\"value\":21}","tool_call_id":"call_1"}"#),
    ];
    let request = crate::port::provider::ChatRequest {
        messages: &messages,
        tools: None,
        max_tokens: None,
    };

    let resp = port.chat(request, "m", None).await.expect("chat");

    // 出方向：canonical tool JSON 还原为结构化 ToolResult。
    let seen = model.seen.lock().unwrap().take().expect("request recorded");
    assert!(seen.tools.is_empty());
    match &seen.chat_history[1] {
        Message::User { content } => match &content[0] {
            UserContent::ToolResult(tr) => {
                assert_eq!(tr.call.to_string(), "call_1");
                assert_eq!(tr.content[0].as_text(), Some(r#"{"value":21}"#));
            }
            other => panic!("expected tool result, got {other:?}"),
        },
        other => panic!("expected user message, got {other:?}"),
    }

    // 回方向：rig Text/Reasoning → port text/reasoning_content。
    assert_eq!(resp.text.as_deref(), Some("done"));
    assert_eq!(resp.reasoning_content.as_deref(), Some("because"));
    assert!(resp.tool_calls.is_empty());
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
            attachments: vec![],
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

    let tool_calls: Arc<Mutex<Vec<serde_json::Value>>> = Arc::new(Mutex::new(vec![]));
    let tool = RecordingTool {
        calls: Arc::clone(&tool_calls),
    };
    let dir = tempfile::tempdir().expect("tempdir");
    let tool_call_ids: crate::port::tool::ToolCallIdSlot = Arc::new(Mutex::new(None));
    let cfg = crate::port::runtime::AgentLoopConfig {
        model_name: "script-model".into(),
        prompt_builder: crate::port::prompt::SystemPromptBuilder::with_defaults(),
        tools: vec![Box::new(tool)],
        memory: Arc::new(crate::port::memory::NoopMemory),
        observer: Arc::new(crate::port::observer::NoopObserver),
        workspace_dir: dir.path().to_path_buf(),
        security_summary: None,
        require_context_budget: false,
        provider_factory: Arc::new(|| Ok(Box::new(ScriptModel { calls: Mutex::new(0) }))),
        conversation_memory: false,
        available_capabilities: vec![],
        tool_call_ids: Some(tool_call_ids.clone()),
        checkpoint_before_tool: false,
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
    assert_eq!(
        tool_call_ids.lock().unwrap().as_deref(),
        Some("call_1"),
        "BudgetHook::on_tool_call must fill the provider tool-call id before execution"
    );
    assert_eq!(name, "recording_tool");
    assert_eq!(*args, serde_json::json!({"x": 1}));

    let tool_result = events.iter().find_map(|e| match e {
        TurnEvent::ToolResult { id, name, output, .. } => Some((id, name, output)),
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

/// 命名可配的记录工具（用于验证旧名修复到指定现行工具）。
struct NamedRecordingTool {
    name: &'static str,
    calls: Arc<Mutex<Vec<serde_json::Value>>>,
}

impl Attributable for NamedRecordingTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Plugin)
    }
    fn alias(&self) -> &str {
        self.name
    }
}

#[async_trait::async_trait]
impl Tool for NamedRecordingTool {
    fn name(&self) -> &str {
        self.name
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
            attachments: vec![],
        })
    }
}

/// 第一次调用返回给定工具名，之后返回最终文本；记录每次请求的消息供断言。
struct InvalidToolModel {
    first_tool: &'static str,
    calls: Mutex<usize>,
    seen: Arc<Mutex<Vec<String>>>,
}

impl Attributable for InvalidToolModel {
    fn role(&self) -> Role {
        Role::Provider(crate::port::attribution::ProviderKind::Model(
            crate::port::attribution::ModelProviderKind::Custom,
        ))
    }
    fn alias(&self) -> &str {
        "InvalidToolModel"
    }
}

#[async_trait::async_trait]
impl crate::port::provider::ModelProvider for InvalidToolModel {
    async fn chat(
        &self,
        request: crate::port::provider::ChatRequest<'_>,
        _model: &str,
        _temperature: Option<f64>,
    ) -> anyhow::Result<crate::port::provider::ChatResponse> {
        self.seen
            .lock()
            .unwrap()
            .extend(request.messages.iter().map(|m| format!("{}:{}", m.role, m.content)));
        let mut calls = self.calls.lock().unwrap();
        *calls += 1;
        if *calls == 1 {
            return Ok(crate::port::provider::ChatResponse {
                text: None,
                tool_calls: vec![crate::port::provider::ToolCall {
                    id: "call_bad".into(),
                    name: self.first_tool.into(),
                    arguments: r#"{"thingId":"d1"}"#.into(),
                }],
                usage: None,
                reasoning_content: None,
            });
        }
        Ok(crate::port::provider::ChatResponse {
            text: Some("recovered".into()),
            tool_calls: vec![],
            usage: None,
            reasoning_content: None,
        })
    }
}

fn invalid_tool_loop(
    first_tool: &'static str,
    tool: Box<dyn Tool>,
    seen: Arc<Mutex<Vec<String>>>,
) -> crate::port::runtime::AgentLoopHandle {
    use crate::adapters::rig::loop_::rig_loop_factory;
    use crate::port::runtime::AgentLoopConfig;

    let dir = tempfile::tempdir().expect("tempdir");
    rig_loop_factory(AgentLoopConfig {
        model_name: "invalid-tool-model".into(),
        prompt_builder: crate::port::prompt::SystemPromptBuilder::with_defaults(),
        tools: vec![tool],
        memory: Arc::new(crate::port::memory::NoopMemory),
        observer: Arc::new(crate::port::observer::NoopObserver),
        workspace_dir: dir.path().to_path_buf(),
        security_summary: None,
        require_context_budget: false,
        provider_factory: Arc::new(move || {
            Ok(Box::new(InvalidToolModel {
                first_tool,
                calls: Mutex::new(0),
                seen: Arc::clone(&seen),
            }))
        }),
        conversation_memory: false,
        available_capabilities: vec![],
        tool_call_ids: None,
        checkpoint_before_tool: false,
    })
    .expect("factory builds loop")
}

/// 旧工具名（read_property）修复为现行 read_device 并真正执行，
/// 不因为模型沿用旧名而丢掉整轮工具调用。
#[tokio::test]
async fn legacy_tool_alias_is_repaired_and_executed() {
    use crate::port::events::TurnEvent;

    let calls: Arc<Mutex<Vec<serde_json::Value>>> = Arc::new(Mutex::new(vec![]));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let loop_ = invalid_tool_loop(
        "read_property",
        Box::new(NamedRecordingTool {
            name: "read_device",
            calls: Arc::clone(&calls),
        }),
        Arc::clone(&seen),
    );

    let (tx, _rx) = tokio::sync::mpsc::channel::<TurnEvent>(64);
    let final_text = loop_
        .lock()
        .await
        .turn_streamed("go", tx, None)
        .await
        .expect("repaired call must not fail the turn");
    assert_eq!(final_text, "recovered");
    assert_eq!(
        calls.lock().unwrap().as_slice(),
        &[serde_json::json!({"thingId": "d1"})],
        "the repaired call must execute against read_device"
    );
}

/// 未知工具名不再终结整轮：跳过该调用并把纠正反馈交还模型。
#[tokio::test]
async fn unknown_tool_call_is_skipped_instead_of_failing_the_turn() {
    use crate::port::events::TurnEvent;

    let calls: Arc<Mutex<Vec<serde_json::Value>>> = Arc::new(Mutex::new(vec![]));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let loop_ = invalid_tool_loop(
        "totally_made_up_tool",
        Box::new(RecordingTool {
            calls: Arc::clone(&calls),
        }),
        Arc::clone(&seen),
    );

    let (tx, _rx) = tokio::sync::mpsc::channel::<TurnEvent>(64);
    let final_text = loop_
        .lock()
        .await
        .turn_streamed("go", tx, None)
        .await
        .expect("unknown tool must not fail the turn");
    assert_eq!(final_text, "recovered");
    assert!(
        calls.lock().unwrap().is_empty(),
        "the skipped call must not reach any tool"
    );
    // 第二次请求必须带上 skip 反馈，模型才有机会自我纠正。
    let seen = seen.lock().unwrap();
    assert!(
        seen.iter().any(|m| m.contains("未知工具 `totally_made_up_tool`")),
        "{seen:?}"
    );
    assert!(seen.iter().any(|m| m.contains("recording_tool")), "{seen:?}");
}

#[tokio::test]
async fn run_budget_rejects_fifth_distinct_image_before_provider_delivery() {
    use crate::adapters::rig::loop_::rig_loop_factory;
    use crate::port::provider::{ChatResponse, ModelProvider};
    use crate::port::runtime::AgentLoopConfig;
    use tinyiothub_core::media::MediaAttachment;

    struct ImageTool;
    crate::mock_tool_attribution!(ImageTool);
    #[async_trait::async_trait]
    impl Tool for ImageTool {
        fn name(&self) -> &str {
            "image_tool"
        }
        fn description(&self) -> &str {
            "Returns one image"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type":"object"})
        }
        async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
            let index = args["index"].as_u64().unwrap_or(0);
            Ok(ToolResult {
                success: true,
                output: "image".into(),
                error: None,
                attachments: vec![MediaAttachment {
                    evidence_id: format!("e{index}"),
                    mime: "image/png".into(),
                    bytes: vec![index as u8; 2 * 1024 * 1024],
                    source_seq: index,
                    media_time: chrono::Utc::now(),
                }],
            })
        }
    }

    struct ImageModel {
        calls: Arc<Mutex<usize>>,
    }
    impl Attributable for ImageModel {
        fn role(&self) -> Role {
            Role::Provider(crate::port::attribution::ProviderKind::Model(
                crate::port::attribution::ModelProviderKind::Custom,
            ))
        }
        fn alias(&self) -> &str {
            "ImageModel"
        }
    }
    #[async_trait::async_trait]
    impl ModelProvider for ImageModel {
        fn supports_vision(&self) -> bool {
            true
        }
        async fn chat(
            &self,
            _request: crate::port::provider::ChatRequest<'_>,
            _model: &str,
            _temperature: Option<f64>,
        ) -> anyhow::Result<ChatResponse> {
            let mut calls = self.calls.lock().expect("model calls");
            *calls += 1;
            let index = *calls as u64;
            Ok(ChatResponse {
                text: None,
                tool_calls: vec![crate::port::provider::ToolCall {
                    id: format!("call_{index}"),
                    name: "image_tool".into(),
                    arguments: serde_json::json!({"index":index}).to_string(),
                }],
                usage: None,
                reasoning_content: None,
            })
        }
    }

    let calls = Arc::new(Mutex::new(0));
    let provider_calls = Arc::clone(&calls);
    let dir = tempfile::tempdir().expect("tempdir");
    let loop_ = rig_loop_factory(AgentLoopConfig {
        model_name: "image-model".into(),
        prompt_builder: crate::port::prompt::SystemPromptBuilder::with_defaults(),
        tools: vec![Box::new(ImageTool)],
        memory: Arc::new(crate::port::memory::NoopMemory),
        observer: Arc::new(crate::port::observer::NoopObserver),
        workspace_dir: dir.path().to_path_buf(),
        security_summary: None,
        require_context_budget: false,
        provider_factory: Arc::new(move || {
            Ok(Box::new(ImageModel {
                calls: Arc::clone(&provider_calls),
            }))
        }),
        conversation_memory: false,
        available_capabilities: vec![],
        tool_call_ids: None,
        checkpoint_before_tool: false,
    })
    .expect("loop");
    let (tx, _rx) = tokio::sync::mpsc::channel(64);
    let error = loop_
        .lock()
        .await
        .turn_streamed("inspect", tx, None)
        .await
        .expect_err("fifth image exceeds the Run budget");
    assert!(error.to_string().contains("8 MiB"), "{error}");
    assert_eq!(
        *calls.lock().expect("calls"),
        5,
        "the sixth completion must not reach the provider"
    );
}

/// 不走 rig 错误重试路径；模型照常收 tool result 并给出最终文本。
#[tokio::test]
async fn failing_tool_result_is_delivered_as_text_not_retried() {
    use crate::adapters::rig::loop_::rig_loop_factory;

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
                attachments: vec![],
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
    let cfg = crate::port::runtime::AgentLoopConfig {
        model_name: "assert-model".into(),
        prompt_builder: crate::port::prompt::SystemPromptBuilder::with_defaults(),
        tools: vec![Box::new(FailingTool)],
        memory: Arc::new(crate::port::memory::NoopMemory),
        observer: Arc::new(crate::port::observer::NoopObserver),
        workspace_dir: dir.path().to_path_buf(),
        security_summary: None,
        require_context_budget: false,
        provider_factory: Arc::new(|| Ok(Box::new(AssertModel { calls: Mutex::new(0) }))),
        conversation_memory: false,
        available_capabilities: vec![],
        tool_call_ids: None,
        checkpoint_before_tool: false,
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
    entries: Mutex<Vec<crate::port::memory::MemoryEntry>>,
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
        key: &str,
        content: &str,
        _category: crate::port::memory::MemoryCategory,
        _session_id: Option<&str>,
    ) -> anyhow::Result<()> {
        self.entries.lock().unwrap().insert(0, desc_entry(key, content));
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
        Ok(self.entries.lock().unwrap().clone())
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
        Ok(self.entries.lock().unwrap().len())
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
        entries: Mutex::new(vec![
            desc_entry("user", "third"),
            desc_entry("assistant", r#"{"content":"second","tool_calls":[]}"#),
            desc_entry("user", "first"),
        ]),
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

#[tokio::test]
async fn conversation_append_load_preserves_exact_image_evidence() {
    use crate::adapters::rig::memory::PortMemoryAsConversation;
    use crate::adapters::rig::provider::{rig_messages_to_port, tool_result_to_rig};
    use chrono::{TimeZone, Utc};
    use rig_core::memory::ConversationMemory;
    use rig_core::message::{
        AssistantContent, Message, ToolCall, ToolCallId, ToolFunction, ToolResult as RigToolResult, UserContent,
    };
    use tinyiothub_core::media::MediaAttachment;

    let memory = Arc::new(DescMemory {
        entries: Mutex::new(vec![]),
    });
    let bridge = PortMemoryAsConversation::new(memory);
    let expected = MediaAttachment {
        evidence_id: "evidence-camera-1".into(),
        mime: "image/jpeg".into(),
        bytes: vec![0xff, 0xd8, 0xff, 1, 2, 3, 0xff, 0xd9],
        source_seq: 42,
        media_time: Utc
            .with_ymd_and_hms(2026, 10, 27, 10, 0, 0)
            .single()
            .expect("fixed media time"),
    };
    let content = tool_result_to_rig(&ToolResult {
        success: true,
        output: "captured".into(),
        error: None,
        attachments: vec![expected.clone()],
    })
    .expect("tool image content");
    let messages = vec![
        Message::Assistant {
            id: None,
            content: vec![AssistantContent::ToolCall(ToolCall::new(
                ToolCallId::new_or_mint("call-camera"),
                ToolFunction::new("read_device".into(), serde_json::json!({})),
            ))],
        },
        Message::User {
            content: vec![UserContent::ToolResult(RigToolResult {
                call: ToolCallId::new_or_mint("call-camera"),
                provider: None,
                name: "read_device".into(),
                content,
            })],
        },
    ];

    bridge.append("default", messages).await.expect("append image history");
    let loaded = bridge.load("default").await.expect("load image history");
    let port = rig_messages_to_port(&loaded).expect("reload as port history");
    let actual = &port
        .iter()
        .find(|message| message.role == "tool")
        .expect("stored tool result")
        .attachments[0];

    assert_eq!(actual.bytes, expected.bytes);
    assert_eq!(actual.mime, expected.mime);
    assert_eq!(actual.evidence_id, expected.evidence_id);
    assert_eq!(actual.source_seq, expected.source_seq);
    assert_eq!(actual.media_time, expected.media_time);
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

    let token = tokio_util::sync::CancellationToken::new();
    let model = CancellingModel {
        calls: Mutex::new(0),
        token: token.clone(),
    };
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = crate::port::runtime::AgentLoopConfig {
        model_name: "cancelling-model".into(),
        prompt_builder: crate::port::prompt::SystemPromptBuilder::with_defaults(),
        tools: vec![],
        memory: Arc::new(crate::port::memory::NoopMemory),
        observer: Arc::new(crate::port::observer::NoopObserver),
        workspace_dir: dir.path().to_path_buf(),
        security_summary: None,
        require_context_budget: false,
        provider_factory: Arc::new(move || {
            Ok(Box::new(CancellingModel {
                calls: Mutex::new(0),
                token: model.token.clone(),
            }))
        }),
        conversation_memory: false,
        available_capabilities: vec![],
        tool_call_ids: None,
        checkpoint_before_tool: false,
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

// ── ToolDescriptor capability gate ───────────────────────────────────────────

struct VisionTool;

impl Attributable for VisionTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Plugin)
    }
    fn alias(&self) -> &str {
        "VisionTool"
    }
}

#[async_trait::async_trait]
impl Tool for VisionTool {
    fn name(&self) -> &str {
        "capture_frame"
    }

    fn description(&self) -> &str {
        "Capture a frame"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {}})
    }

    fn descriptor(&self) -> tinyiothub_skills::registry::ToolDescriptor {
        let mut descriptor = tinyiothub_skills::registry::ToolDescriptor::declared(
            self.name(),
            self.description(),
            self.parameters_schema(),
        );
        descriptor.required_capabilities = vec![tinyiothub_skills::registry::Capability::Video];
        descriptor
    }

    async fn execute(&self, _args: serde_json::Value) -> anyhow::Result<ToolResult> {
        Ok(ToolResult {
            success: true,
            output: "frame".into(),
            error: None,
            attachments: vec![],
        })
    }
}

struct NonVisionModel {
    calls: Arc<Mutex<usize>>,
}

impl Attributable for NonVisionModel {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Plugin)
    }
    fn alias(&self) -> &str {
        "NonVisionModel"
    }
}

#[async_trait::async_trait]
impl crate::port::provider::ModelProvider for NonVisionModel {
    fn supports_vision(&self) -> bool {
        false
    }

    async fn chat(
        &self,
        _request: crate::port::provider::ChatRequest<'_>,
        _model: &str,
        _temperature: Option<f64>,
    ) -> anyhow::Result<crate::port::provider::ChatResponse> {
        *self.calls.lock().unwrap() += 1;
        Ok(crate::port::provider::ChatResponse::default())
    }
}

/// A tool requiring a vision capability must fail before the model is called;
/// the composition gate is driven by the same descriptor that produces the
/// LLM tool subset.
#[test]
fn tools_descriptor_capability_gate_fails_before_model_call() {
    use crate::adapters::rig::loop_::rig_loop_factory;
    use crate::port::runtime::AgentLoopConfig;

    let calls = Arc::new(Mutex::new(0));
    let provider_calls = Arc::clone(&calls);
    let dir = tempfile::tempdir().expect("tempdir");
    let result = rig_loop_factory(AgentLoopConfig {
        model_name: "non-vision-model".into(),
        prompt_builder: crate::port::prompt::SystemPromptBuilder::with_defaults(),
        tools: vec![Box::new(VisionTool)],
        memory: Arc::new(crate::port::memory::NoopMemory),
        observer: Arc::new(crate::port::observer::NoopObserver),
        workspace_dir: dir.path().to_path_buf(),
        security_summary: None,
        require_context_budget: false,
        provider_factory: Arc::new(move || {
            Ok(Box::new(NonVisionModel {
                calls: Arc::clone(&provider_calls),
            }))
        }),
        conversation_memory: false,
        available_capabilities: vec![],
        tool_call_ids: None,
        checkpoint_before_tool: false,
    });
    let error = match result {
        Ok(_) => panic!("missing video/vision capability must fail closed"),
        Err(error) => error,
    };

    assert!(
        error.to_string().contains("capability.unavailable"),
        "error must name capability.unavailable: {error}"
    );
    assert_eq!(*calls.lock().unwrap(), 0, "the model must not be called");
}

// ── ToolDescriptor redaction at the public event boundary ────────────────────

struct SecretTool {
    received: Arc<Mutex<Vec<serde_json::Value>>>,
}

impl Attributable for SecretTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Plugin)
    }
    fn alias(&self) -> &str {
        "SecretTool"
    }
}

#[async_trait::async_trait]
impl Tool for SecretTool {
    fn name(&self) -> &str {
        "secret_tool"
    }
    fn description(&self) -> &str {
        "Handles credentials"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object"})
    }
    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        self.received.lock().unwrap().push(args);
        Ok(ToolResult {
            success: true,
            output: "ok".into(),
            error: None,
            attachments: vec![],
        })
    }
}

struct SecretArgsModel {
    calls: Mutex<usize>,
}

impl Attributable for SecretArgsModel {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Plugin)
    }
    fn alias(&self) -> &str {
        "SecretArgsModel"
    }
}

#[async_trait::async_trait]
impl crate::port::provider::ModelProvider for SecretArgsModel {
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
                text: None,
                tool_calls: vec![crate::port::provider::ToolCall {
                    id: "call_secret".into(),
                    name: "secret_tool".into(),
                    arguments: serde_json::json!({
                        "thingId": "camera-1",
                        "credentials": {
                            "password": "nested-password",
                            "access_token": "nested-token"
                        }
                    })
                    .to_string(),
                }],
                usage: None,
                reasoning_content: None,
            });
        }
        Ok(crate::port::provider::ChatResponse {
            text: Some("done".into()),
            ..crate::port::provider::ChatResponse::default()
        })
    }
}

/// The tool receives raw arguments, but anything observable (TurnEvent/SSE)
/// carries only the descriptor-redacted view. This is the public boundary that
/// must never expose nested password/token/secret fields.
#[tokio::test]
async fn tools_descriptor_redacts_nested_secrets_in_public_events() {
    use crate::adapters::rig::loop_::rig_loop_factory;
    use crate::port::events::TurnEvent;
    use crate::port::runtime::AgentLoopConfig;

    let received = Arc::new(Mutex::new(Vec::new()));
    let dir = tempfile::tempdir().expect("tempdir");
    let loop_ = rig_loop_factory(AgentLoopConfig {
        model_name: "secret-model".into(),
        prompt_builder: crate::port::prompt::SystemPromptBuilder::with_defaults(),
        tools: vec![Box::new(SecretTool {
            received: Arc::clone(&received),
        })],
        memory: Arc::new(crate::port::memory::NoopMemory),
        observer: Arc::new(crate::port::observer::NoopObserver),
        workspace_dir: dir.path().to_path_buf(),
        security_summary: None,
        require_context_budget: false,
        provider_factory: Arc::new(|| Ok(Box::new(SecretArgsModel { calls: Mutex::new(0) }))),
        conversation_memory: false,
        available_capabilities: vec![],
        tool_call_ids: None,
        checkpoint_before_tool: false,
    })
    .expect("loop");

    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<TurnEvent>(64);
    loop_
        .lock()
        .await
        .turn_streamed("go", event_tx, None)
        .await
        .expect("turn");

    let raw = received.lock().unwrap().clone();
    assert_eq!(raw.len(), 1);
    assert_eq!(
        raw[0]["credentials"]["password"], "nested-password",
        "the tool itself must receive the raw arguments"
    );

    let mut public_args = None;
    while let Some(event) = event_rx.recv().await {
        if let TurnEvent::ToolCall { args, .. } = event {
            public_args = Some(args);
        }
    }
    let public_args = public_args.expect("ToolCall event");
    let serialized = public_args.to_string();
    assert!(!serialized.contains("nested-password"), "{serialized}");
    assert!(!serialized.contains("nested-token"), "{serialized}");
    assert_eq!(public_args["credentials"]["password"], "[REDACTED]");
    assert_eq!(public_args["thingId"], "camera-1");
}

#[tokio::test]
async fn rig_loop_streams_tokens_and_tool_calls_from_real_sse() {
    // The first model call requests a tool; the second returns the final text.
    // This exercises real SSE deltas through the rig loop instead of the old
    // completion-then-split adapter.
    let requests = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let requests_for_server = requests.clone();
    let request_bodies = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    let bodies_for_server = request_bodies.clone();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = axum::Router::new().route(
        "/v1/chat/completions",
        axum::routing::post(move |request: axum::extract::Request| {
            let requests = requests_for_server.clone();
            let bodies = bodies_for_server.clone();
            async move {
                let bytes = axum::body::to_bytes(request.into_body(), 1024 * 1024)
                    .await
                    .expect("request body");
                bodies
                    .lock()
                    .unwrap()
                    .push(serde_json::from_slice(&bytes).expect("json request"));
                let request_number = requests.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let mut response = String::new();
                if request_number == 0 {
                    for text in ["先", "看", "设备"] {
                        let payload = serde_json::json!({"choices": [{"delta": {"content": text}}]});
                        response.push_str(&format!("data: {payload}\n\n"));
                    }
                    let tool_call = serde_json::json!({
                        "choices": [{
                            "delta": {
                                "tool_calls": [{
                                    "index": 0,
                                    "id": "call_1",
                                    "function": {"name": "recording_tool", "arguments": "{\"x\":1}"}
                                }]
                            }
                        }]
                    });
                    response.push_str(&format!("data: {tool_call}\n\n"));
                    response.push_str("data: {\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5}}\n\n");
                } else {
                    response.push_str("data: {\"choices\":[{\"delta\":{\"content\":\"final answer\"}}]}\n\n");
                    response.push_str("data: {\"usage\":{\"prompt_tokens\":20,\"completion_tokens\":7}}\n\n");
                }
                response.push_str("data: [DONE]\n\n");
                response
            }
        }),
    );
    let server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
            .unwrap();
    });

    let provider = super::provider::StreamingOpenAiPort::new(format!("http://{addr}/v1"), "test-key".into());
    let tool = RecordingTool {
        calls: Arc::new(Mutex::new(Vec::new())),
    };
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = crate::port::runtime::AgentLoopConfig {
        model_name: "script-model".into(),
        prompt_builder: crate::port::prompt::SystemPromptBuilder::with_defaults(),
        tools: vec![Box::new(tool)],
        memory: Arc::new(crate::port::memory::NoopMemory),
        observer: Arc::new(crate::port::observer::NoopObserver),
        workspace_dir: dir.path().to_path_buf(),
        security_summary: None,
        require_context_budget: false,
        provider_factory: Arc::new(move || Ok(Box::new(provider.clone()))),
        conversation_memory: false,
        available_capabilities: vec![],
        tool_call_ids: None,
        checkpoint_before_tool: false,
    };
    let loop_handle = crate::adapters::rig::loop_::rig_loop_factory(cfg).expect("loop");
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    let result = loop_handle
        .lock()
        .await
        .turn_streamed("go", tx, None)
        .await
        .expect("turn completes");
    let _ = shutdown_tx.send(());
    server.await.unwrap();
    assert_eq!(result, "final answer");
    assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 2);

    {
        let bodies = request_bodies.lock().unwrap();
        let second = &bodies[1];
        let assistant = second["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|message| message["role"] == "assistant")
            .expect("assistant tool-call message");
        assert_eq!(assistant["tool_calls"][0]["id"], "call_1");
        let tool_result = second["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|message| message["role"] == "tool")
            .expect("tool result message");
        assert_eq!(tool_result["tool_call_id"], "call_1");
    }

    let mut chunks = String::new();
    let mut saw_tool = false;
    let mut saw_result = false;
    while let Some(event) = rx.recv().await {
        match event {
            crate::port::events::TurnEvent::Chunk { delta } => chunks.push_str(&delta),
            crate::port::events::TurnEvent::ToolCall { name, .. } => {
                saw_tool = name == "recording_tool";
            }
            crate::port::events::TurnEvent::ToolResult { name, output, .. } => {
                saw_result = name == "recording_tool" && output == "recorded";
            }
            _ => {}
        }
    }
    assert_eq!(chunks, "先看设备final answer");
    assert!(saw_tool, "tool call from SSE must reach the loop");
    assert!(saw_result, "tool result must be correlated back to the call");
}

struct GatedProvider {
    release: Arc<tokio::sync::Notify>,
}

impl Attributable for GatedProvider {
    fn role(&self) -> Role {
        Role::Provider(crate::port::attribution::ProviderKind::Model(
            crate::port::attribution::ModelProviderKind::Custom,
        ))
    }
    fn alias(&self) -> &str {
        "gated"
    }
}

#[async_trait::async_trait]
impl crate::port::provider::ModelProvider for GatedProvider {
    async fn chat(
        &self,
        _request: crate::port::provider::ChatRequest<'_>,
        _model: &str,
        _temperature: Option<f64>,
    ) -> anyhow::Result<crate::port::provider::ChatResponse> {
        unreachable!("streaming test uses chat_stream")
    }

    async fn chat_stream(
        &self,
        _request: crate::port::provider::ChatRequest<'_>,
        _model: &str,
        _temperature: Option<f64>,
        on_delta: &(dyn Fn(crate::port::provider::ChatStreamDelta) + Send + Sync),
    ) -> anyhow::Result<crate::port::provider::ChatResponse> {
        on_delta(crate::port::provider::ChatStreamDelta {
            text: "first".into(),
            reasoning: false,
        });
        self.release.notified().await;
        Ok(crate::port::provider::ChatResponse {
            text: Some("first".into()),
            ..Default::default()
        })
    }
}

#[tokio::test]
async fn adapter_emits_delta_before_provider_completion() {
    let release = Arc::new(tokio::sync::Notify::new());
    let provider = GatedProvider {
        release: release.clone(),
    };
    let adapter = super::provider::PortModelAsRig::new(Arc::new(provider), "gated".into());
    let request = rig_core::completion::CompletionRequest {
        model: Some("gated".into()),
        preamble: None,
        chat_history: vec![rig_core::message::Message::user("go")],
        documents: vec![],
        tools: vec![],
        temperature: None,
        max_tokens: None,
        tool_choice: None,
        additional_params: None,
        output_schema: None,
        record_telemetry_content: false,
    };
    let mut stream = rig_core::completion::CompletionModel::stream(&adapter, request)
        .await
        .expect("stream opens");
    let first = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        futures::StreamExt::next(&mut stream),
    )
    .await
    .expect("delta arrives before provider completion")
    .expect("first item exists")
    .expect("first item is valid");
    assert!(matches!(
        first,
        rig_core::streaming::StreamedAssistantContent::Text(text) if text.text == "first"
    ));
    release.notify_one();
    while futures::StreamExt::next(&mut stream).await.is_some() {}
}

struct BudgetProvider {
    window: Option<u64>,
    calls: Arc<std::sync::atomic::AtomicUsize>,
}

#[async_trait::async_trait]
impl crate::port::provider::ModelProvider for BudgetProvider {
    async fn measure_context(
        &self,
        _request: crate::port::provider::ChatRequest<'_>,
        _model: &str,
        _temperature: Option<f64>,
    ) -> anyhow::Result<Option<crate::port::provider::ContextMeasurement>> {
        Ok(self
            .window
            .map(|context_window| crate::port::provider::ContextMeasurement {
                context_window,
                counted_input_tokens: 100,
                counting_method: "fixture-exact-v1".into(),
            }))
    }

    async fn chat(
        &self,
        _request: crate::port::provider::ChatRequest<'_>,
        _model: &str,
        _temperature: Option<f64>,
    ) -> anyhow::Result<crate::port::provider::ChatResponse> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(crate::port::provider::ChatResponse::default())
    }
}

impl Attributable for BudgetProvider {
    fn role(&self) -> Role {
        Role::Provider(crate::port::attribution::ProviderKind::Model(
            crate::port::attribution::ModelProviderKind::Custom,
        ))
    }

    fn alias(&self) -> &str {
        "BudgetProvider"
    }
}

#[tokio::test]
async fn context_budget_unavailable_prevents_model_call() {
    let provider = BudgetProvider {
        window: None,
        calls: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
    };
    let calls = provider.calls.clone();
    let adapter =
        super::provider::PortModelAsRig::new(Arc::new(provider), "budget".into()).with_context_budget_required(true);
    let error = collect_retry_stream(adapter)
        .await
        .expect_err("unknown budget must fail");
    assert!(error.contains("context_budget_unavailable"), "{error}");
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[tokio::test]
async fn context_too_large_prevents_model_call() {
    let provider = BudgetProvider {
        window: Some(1),
        calls: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
    };
    let calls = provider.calls.clone();
    let adapter =
        super::provider::PortModelAsRig::new(Arc::new(provider), "budget".into()).with_context_budget_required(true);
    let error = collect_retry_stream(adapter).await.expect_err("overflow must fail");
    assert!(error.contains("context_too_large"), "{error}");
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[tokio::test]
async fn exact_provider_budget_allows_model_call() {
    let provider = BudgetProvider {
        window: Some(1_000_000),
        calls: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
    };
    let calls = provider.calls.clone();
    let adapter =
        super::provider::PortModelAsRig::new(Arc::new(provider), "budget".into()).with_context_budget_required(true);
    collect_retry_stream(adapter).await.expect("within budget");
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test]
async fn remote_tokenizer_measures_the_exact_openai_request() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept tokenizer request");
        let mut buffer = [0u8; 16_384];
        let read = std::io::Read::read(&mut stream, &mut buffer).expect("read request");
        let request = String::from_utf8_lossy(&buffer[..read]).to_string();
        assert!(request.contains("\"max_tokens\":7"), "{request}");
        assert!(request.contains("\"temperature\":0.7"), "{request}");
        assert!(request.contains("\"tools\""), "{request}");
        let payload = "{\"inputTokens\":100,\"countingMethod\":\"fixture-v1\"}";
        let body = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            payload.len(),
            payload
        );
        std::io::Write::write_all(&mut stream, body.as_bytes()).expect("write response");
    });
    let provider = super::provider::StreamingOpenAiPort::new("http://unused.invalid".into(), "test-key".into())
        .with_context_budget(1000, format!("http://{address}/tokenize"), "fixture-v1".into());
    let messages = vec![crate::port::provider::ChatMessage::system("budget")];
    let tools = vec![crate::port::tool::ToolSpec {
        name: "read_device".into(),
        description: "Read one device".into(),
        parameters: serde_json::json!({"type":"object"}),
    }];
    let measurement = provider
        .measure_context(
            crate::port::provider::ChatRequest {
                messages: &messages,
                tools: Some(&tools),
                max_tokens: Some(7),
            },
            "minimax-m2",
            Some(0.7),
        )
        .await
        .expect("measure")
        .expect("configured measurement");
    assert_eq!(measurement.context_window, 1000);
    assert_eq!(measurement.counted_input_tokens, 100);
    assert_eq!(measurement.counting_method, "fixture-v1");
    server.join().expect("join tokenizer server");
}

#[derive(Clone)]
struct RetryProvider {
    calls: Arc<std::sync::atomic::AtomicUsize>,
    failures: usize,
    error: fn(usize) -> anyhow::Error,
    partial: bool,
    tool_after_retry: bool,
}

impl Attributable for RetryProvider {
    fn role(&self) -> Role {
        Role::Provider(crate::port::attribution::ProviderKind::Model(
            crate::port::attribution::ModelProviderKind::Custom,
        ))
    }
    fn alias(&self) -> &str {
        "retry-test"
    }
}

#[async_trait::async_trait]
impl crate::port::provider::ModelProvider for RetryProvider {
    async fn chat(
        &self,
        _request: crate::port::provider::ChatRequest<'_>,
        _model: &str,
        _temperature: Option<f64>,
    ) -> anyhow::Result<crate::port::provider::ChatResponse> {
        unreachable!("retry tests use streaming")
    }

    async fn chat_stream(
        &self,
        _request: crate::port::provider::ChatRequest<'_>,
        _model: &str,
        _temperature: Option<f64>,
        on_delta: &(dyn Fn(crate::port::provider::ChatStreamDelta) + Send + Sync),
    ) -> anyhow::Result<crate::port::provider::ChatResponse> {
        let call = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        if call <= self.failures {
            if self.partial {
                on_delta(crate::port::provider::ChatStreamDelta {
                    text: format!("partial-{call}"),
                    reasoning: false,
                });
            }
            return Err((self.error)(call));
        }
        if self.tool_after_retry && call == self.failures + 1 {
            on_delta(crate::port::provider::ChatStreamDelta {
                text: "before".into(),
                reasoning: false,
            });
            return Ok(crate::port::provider::ChatResponse {
                text: Some("before".into()),
                tool_calls: vec![crate::port::provider::ToolCall {
                    id: "retry-tool".into(),
                    name: "recording_tool".into(),
                    arguments: "{}".into(),
                }],
                ..Default::default()
            });
        }
        on_delta(crate::port::provider::ChatStreamDelta {
            text: "recovered".into(),
            reasoning: false,
        });
        Ok(crate::port::provider::ChatResponse {
            text: Some("recovered".into()),
            ..Default::default()
        })
    }
}

fn retry_request() -> rig_core::completion::CompletionRequest {
    rig_core::completion::CompletionRequest {
        model: Some("retry-test".into()),
        preamble: None,
        chat_history: vec![rig_core::message::Message::user("go")],
        documents: vec![],
        tools: vec![],
        temperature: None,
        max_tokens: None,
        tool_choice: None,
        additional_params: None,
        output_schema: None,
        record_telemetry_content: false,
    }
}

async fn collect_retry_stream(adapter: super::provider::PortModelAsRig) -> Result<Vec<String>, String> {
    use futures::StreamExt;
    use rig_core::completion::CompletionModel;

    let mut stream = adapter
        .stream(retry_request())
        .await
        .map_err(|error| error.to_string())?;
    let mut text = Vec::new();
    while let Some(item) = stream.next().await {
        match item {
            Ok(rig_core::streaming::StreamedAssistantContent::Text(delta)) => text.push(delta.text),
            Ok(_) => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(text)
}

fn retry_adapter(
    failures: usize,
    error: fn(usize) -> anyhow::Error,
) -> (super::provider::PortModelAsRig, Arc<RetryProvider>) {
    let provider = Arc::new(RetryProvider {
        calls: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        failures,
        error,
        partial: false,
        tool_after_retry: false,
    });
    let adapter = super::provider::PortModelAsRig::new(provider.clone(), "retry-test".into());
    (adapter, provider)
}

#[tokio::test(start_paused = true)]
async fn retry_recovers_after_two_temporary_failures() {
    let events = super::provider::RetryEvents::default();
    let (adapter, provider) = retry_adapter(2, |call| {
        crate::port::provider::ProviderError::Internal(format!("temporary-{call}")).into()
    });
    let adapter = adapter.with_retry_events(events.clone());
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let _guard = events.install(tx, None);
    let driver = tokio::spawn(collect_retry_stream(adapter));

    let first = rx.recv().await.expect("first retry notice");
    assert_eq!((first.attempt, first.delay_ms), (1, 1000));
    tokio::time::advance(std::time::Duration::from_secs(1)).await;
    let second = rx.recv().await.expect("second retry notice");
    assert_eq!((second.attempt, second.delay_ms), (2, 2000));
    tokio::time::advance(std::time::Duration::from_secs(2)).await;

    assert_eq!(driver.await.unwrap().unwrap(), vec!["recovered"]);
    assert_eq!(provider.calls.load(std::sync::atomic::Ordering::SeqCst), 3);
}

#[tokio::test(start_paused = true)]
async fn retry_stops_after_three_retries_with_the_real_error() {
    let events = super::provider::RetryEvents::default();
    let (adapter, provider) = retry_adapter(4, |call| {
        crate::port::provider::ProviderError::Internal(format!("temporary-{call}")).into()
    });
    let adapter = adapter.with_retry_events(events.clone());
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let _guard = events.install(tx, None);
    let driver = tokio::spawn(collect_retry_stream(adapter));

    for (attempt, seconds) in [(1, 1), (2, 2), (3, 4)] {
        let notice = rx.recv().await.expect("retry notice");
        assert_eq!(
            (notice.attempt, notice.max_retries, notice.delay_ms),
            (attempt, 3, seconds * 1000)
        );
        tokio::time::advance(std::time::Duration::from_secs(seconds)).await;
    }
    let error = driver.await.unwrap().expect_err("all attempts fail");
    assert!(error.contains("temporary-4"), "{error}");
    assert_eq!(provider.calls.load(std::sync::atomic::Ordering::SeqCst), 4);
}

#[tokio::test]
async fn retry_does_not_repeat_an_unrecoverable_request() {
    let (adapter, provider) = retry_adapter(1, |_| {
        crate::port::provider::ProviderError::Http {
            status: 400,
            message: "invalid request".into(),
        }
        .into()
    });
    let error = collect_retry_stream(adapter).await.expect_err("invalid request");
    assert!(error.contains("invalid request"), "{error}");
    assert_eq!(provider.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn retry_notice_follows_failed_attempt_deltas() {
    use futures::StreamExt;
    use rig_core::completion::CompletionModel;

    let events = super::provider::RetryEvents::default();
    let provider = Arc::new(RetryProvider {
        calls: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        failures: 1,
        error: |call| crate::port::provider::ProviderError::Internal(format!("temporary-{call}")).into(),
        partial: true,
        tool_after_retry: false,
    });
    let adapter = super::provider::PortModelAsRig::new(provider, "retry-test".into()).with_retry_events(events.clone());
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let _guard = events.install(tx, None);
    let mut stream = adapter.stream(retry_request()).await.expect("stream opens");

    let first = stream.next().await.expect("partial delta").expect("valid delta");
    assert!(matches!(
        first,
        rig_core::streaming::StreamedAssistantContent::Text(text) if text.text == "partial-1"
    ));
    assert!(
        matches!(rx.try_recv(), Err(tokio::sync::mpsc::error::TryRecvError::Empty)),
        "retry must not overtake the failed attempt's streamed text"
    );
}

#[tokio::test(start_paused = true)]
async fn retry_after_partial_text_does_not_duplicate_text_or_replay_tools() {
    use crate::port::events::TurnEvent;
    use crate::port::runtime::AgentLoopConfig;

    let provider = Arc::new(RetryProvider {
        calls: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        failures: 1,
        error: |call| crate::port::provider::ProviderError::Internal(format!("temporary-{call}")).into(),
        partial: true,
        tool_after_retry: true,
    });
    let tool = RecordingTool {
        calls: Arc::new(Mutex::new(Vec::new())),
    };
    let tool_calls = tool.calls.clone();
    let dir = tempfile::tempdir().expect("tempdir");
    let loop_handle = crate::adapters::rig::loop_::rig_loop_factory(AgentLoopConfig {
        model_name: "retry-test".into(),
        prompt_builder: crate::port::prompt::SystemPromptBuilder::with_defaults(),
        tools: vec![Box::new(tool)],
        memory: Arc::new(crate::port::memory::NoopMemory),
        observer: Arc::new(crate::port::observer::NoopObserver),
        workspace_dir: dir.path().to_path_buf(),
        security_summary: None,
        require_context_budget: false,
        provider_factory: Arc::new(move || Ok(Box::new(provider.as_ref().clone()))),
        conversation_memory: false,
        available_capabilities: vec![],
        tool_call_ids: None,
        checkpoint_before_tool: false,
    })
    .expect("loop");
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    let result = loop_handle
        .lock()
        .await
        .turn_streamed("go", tx, None)
        .await
        .expect("retry recovers");

    let mut retries = 0;
    let mut streamed = String::new();
    while let Some(event) = rx.recv().await {
        match event {
            TurnEvent::Retry(notice) => {
                retries += 1;
                assert_eq!(notice.attempt, 1);
                assert_eq!(notice.error, "temporary-1");
            }
            TurnEvent::Chunk { delta } => streamed.push_str(&delta),
            _ => {}
        }
    }
    assert_eq!(retries, 1);
    assert_eq!(result, "recovered");
    assert_eq!(streamed, "partial-1beforerecovered");
    assert_eq!(tool_calls.lock().unwrap().len(), 1, "successful tool executes once");
}

#[tokio::test(start_paused = true)]
async fn retry_backoff_stops_when_the_turn_is_cancelled() {
    use crate::port::events::TurnEvent;
    use crate::port::runtime::AgentLoopConfig;

    let provider = Arc::new(RetryProvider {
        calls: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        failures: 4,
        error: |call| crate::port::provider::ProviderError::Internal(format!("temporary-{call}")).into(),
        partial: false,
        tool_after_retry: false,
    });
    let observed = provider.clone();
    let dir = tempfile::tempdir().expect("tempdir");
    let loop_handle = crate::adapters::rig::loop_::rig_loop_factory(AgentLoopConfig {
        model_name: "retry-test".into(),
        prompt_builder: crate::port::prompt::SystemPromptBuilder::with_defaults(),
        tools: vec![],
        memory: Arc::new(crate::port::memory::NoopMemory),
        observer: Arc::new(crate::port::observer::NoopObserver),
        workspace_dir: dir.path().to_path_buf(),
        security_summary: None,
        require_context_budget: false,
        provider_factory: Arc::new(move || Ok(Box::new(provider.as_ref().clone()))),
        conversation_memory: false,
        available_capabilities: vec![],
        tool_call_ids: None,
        checkpoint_before_tool: false,
    })
    .expect("loop");
    let cancel = tokio_util::sync::CancellationToken::new();
    let driver_cancel = cancel.clone();
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    let driver = tokio::spawn(async move {
        loop_handle
            .lock()
            .await
            .turn_streamed("go", tx, Some(driver_cancel))
            .await
    });
    while let Some(event) = rx.recv().await {
        if matches!(event, TurnEvent::Retry(_)) {
            cancel.cancel();
            break;
        }
    }
    let error = driver.await.unwrap().expect_err("cancelled during backoff");
    assert!(crate::port::outcome::is_tool_loop_cancelled(&error));
    assert_eq!(observed.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[derive(Clone)]
struct InFlightRetryProvider {
    calls: Arc<std::sync::atomic::AtomicUsize>,
    release: Arc<tokio::sync::Notify>,
}

impl Attributable for InFlightRetryProvider {
    fn role(&self) -> Role {
        Role::Provider(crate::port::attribution::ProviderKind::Model(
            crate::port::attribution::ModelProviderKind::Custom,
        ))
    }
    fn alias(&self) -> &str {
        "in-flight-retry"
    }
}

#[async_trait::async_trait]
impl crate::port::provider::ModelProvider for InFlightRetryProvider {
    async fn chat(
        &self,
        _request: crate::port::provider::ChatRequest<'_>,
        _model: &str,
        _temperature: Option<f64>,
    ) -> anyhow::Result<crate::port::provider::ChatResponse> {
        unreachable!()
    }

    async fn chat_stream(
        &self,
        _request: crate::port::provider::ChatRequest<'_>,
        _model: &str,
        _temperature: Option<f64>,
        _on_delta: &(dyn Fn(crate::port::provider::ChatStreamDelta) + Send + Sync),
    ) -> anyhow::Result<crate::port::provider::ChatResponse> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.release.notified().await;
        Err(crate::port::provider::ProviderError::Internal("late temporary failure".into()).into())
    }
}

#[tokio::test(start_paused = true)]
async fn cancelling_an_in_flight_request_prevents_detached_retries() {
    use crate::port::runtime::AgentLoopConfig;

    let provider = Arc::new(InFlightRetryProvider {
        calls: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        release: Arc::new(tokio::sync::Notify::new()),
    });
    let observed = provider.clone();
    let dir = tempfile::tempdir().expect("tempdir");
    let loop_handle = crate::adapters::rig::loop_::rig_loop_factory(AgentLoopConfig {
        model_name: "retry-test".into(),
        prompt_builder: crate::port::prompt::SystemPromptBuilder::with_defaults(),
        tools: vec![],
        memory: Arc::new(crate::port::memory::NoopMemory),
        observer: Arc::new(crate::port::observer::NoopObserver),
        workspace_dir: dir.path().to_path_buf(),
        security_summary: None,
        require_context_budget: false,
        provider_factory: Arc::new(move || Ok(Box::new(provider.as_ref().clone()))),
        conversation_memory: false,
        available_capabilities: vec![],
        tool_call_ids: None,
        checkpoint_before_tool: false,
    })
    .expect("loop");
    let cancel = tokio_util::sync::CancellationToken::new();
    let driver_cancel = cancel.clone();
    let (tx, _rx) = tokio::sync::mpsc::channel(32);
    let driver = tokio::spawn(async move {
        loop_handle
            .lock()
            .await
            .turn_streamed("go", tx, Some(driver_cancel))
            .await
    });
    while observed.calls.load(std::sync::atomic::Ordering::SeqCst) == 0 {
        tokio::task::yield_now().await;
    }
    cancel.cancel();
    let error = driver.await.unwrap().expect_err("cancelled in flight");
    assert!(crate::port::outcome::is_tool_loop_cancelled(&error));
    observed.release.notify_one();
    tokio::time::advance(std::time::Duration::from_secs(10)).await;
    for _ in 0..100 {
        tokio::task::yield_now().await;
    }
    assert_eq!(observed.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[derive(Clone)]
struct ReasoningProvider;

impl Attributable for ReasoningProvider {
    fn role(&self) -> Role {
        Role::Provider(crate::port::attribution::ProviderKind::Model(
            crate::port::attribution::ModelProviderKind::Custom,
        ))
    }
    fn alias(&self) -> &str {
        "reasoning-test"
    }
}

#[async_trait::async_trait]
impl crate::port::provider::ModelProvider for ReasoningProvider {
    async fn chat(
        &self,
        _request: crate::port::provider::ChatRequest<'_>,
        _model: &str,
        _temperature: Option<f64>,
    ) -> anyhow::Result<crate::port::provider::ChatResponse> {
        unreachable!()
    }

    async fn chat_stream(
        &self,
        _request: crate::port::provider::ChatRequest<'_>,
        _model: &str,
        _temperature: Option<f64>,
        on_delta: &(dyn Fn(crate::port::provider::ChatStreamDelta) + Send + Sync),
    ) -> anyhow::Result<crate::port::provider::ChatResponse> {
        on_delta(crate::port::provider::ChatStreamDelta {
            text: "check device".into(),
            reasoning: true,
        });
        on_delta(crate::port::provider::ChatStreamDelta {
            text: "device online".into(),
            reasoning: false,
        });
        Ok(crate::port::provider::ChatResponse {
            text: Some("device online".into()),
            reasoning_content: Some("check device".into()),
            ..Default::default()
        })
    }
}

#[tokio::test]
async fn reasoning_deltas_stay_out_of_assistant_answer_text() {
    use crate::port::events::TurnEvent;
    use crate::port::runtime::AgentLoopConfig;

    let dir = tempfile::tempdir().expect("tempdir");
    let loop_handle = crate::adapters::rig::loop_::rig_loop_factory(AgentLoopConfig {
        model_name: "reasoning-test".into(),
        prompt_builder: crate::port::prompt::SystemPromptBuilder::with_defaults(),
        tools: vec![],
        memory: Arc::new(crate::port::memory::NoopMemory),
        observer: Arc::new(crate::port::observer::NoopObserver),
        workspace_dir: dir.path().to_path_buf(),
        security_summary: None,
        require_context_budget: false,
        provider_factory: Arc::new(|| Ok(Box::new(ReasoningProvider))),
        conversation_memory: false,
        available_capabilities: vec![],
        tool_call_ids: None,
        checkpoint_before_tool: false,
    })
    .expect("loop");
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    let result = loop_handle
        .lock()
        .await
        .turn_streamed("go", tx, None)
        .await
        .expect("turn completes");
    let mut text = String::new();
    let mut thinking = String::new();
    while let Some(event) = rx.recv().await {
        match event {
            TurnEvent::Chunk { delta } => text.push_str(&delta),
            TurnEvent::Thinking { delta } => thinking.push_str(&delta),
            _ => {}
        }
    }
    assert_eq!(thinking, "check device");
    assert_eq!(text, "device online");
    assert_eq!(result, "device online");
}

#[tokio::test]
async fn production_provider_preserves_retryable_http_errors() {
    use crate::port::provider::{ChatMessage, ChatRequest, ProviderError};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = axum::Router::new().route(
        "/v1/chat/completions",
        axum::routing::post(|| async { (axum::http::StatusCode::SERVICE_UNAVAILABLE, "upstream unavailable") }),
    );
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let provider = crate::provider::create_minimax_provider_with(&crate::provider::MinimaxSettings {
        base_url: format!("http://{addr}/v1"),
        auth_token: "test-key".into(),
        model: "test-model".into(),
    })
    .expect("production provider");
    let messages = vec![ChatMessage::user("go")];
    let error = provider
        .chat_stream(
            ChatRequest {
                messages: &messages,
                tools: None,
                max_tokens: None,
            },
            "test-model",
            None,
            &|_| {},
        )
        .await
        .expect_err("HTTP 503");
    assert!(matches!(
        error.downcast_ref::<ProviderError>(),
        Some(ProviderError::Http { status: 503, .. })
    ));
    server.abort();
}
