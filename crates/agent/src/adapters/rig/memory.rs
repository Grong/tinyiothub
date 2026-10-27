//! port Memory → rig ConversationMemory 桥。

use std::sync::Arc;

use rig_core::memory::{ConversationMemory, MemoryError};
use rig_core::message::Message;
use rig_core::wasm_compat::WasmBoxedFuture;

use crate::port::memory::{Memory, MemoryCategory};

use super::provider::{port_messages_to_rig, rig_messages_to_port};

/// zeroclaw effective_memory_recall_limit 默认值量级的可接受差异（Global
/// Constraints #6）：rig 会话内存按会话 id 召回最近 N 条。
const RECALL_LIMIT: usize = 20;

const CONVERSATION_CONTENT_FORMAT: &str = "tinyiothub.conversation-content/v1";

#[derive(serde::Serialize, serde::Deserialize)]
struct StoredConversationContent {
    format: String,
    content: String,
    attachments: Vec<tinyiothub_core::media::MediaAttachment>,
}

fn encode_conversation_content(message: &crate::port::provider::ChatMessage) -> Result<String, serde_json::Error> {
    if message.attachments.is_empty() {
        return Ok(message.content.clone());
    }
    serde_json::to_string(&StoredConversationContent {
        format: CONVERSATION_CONTENT_FORMAT.into(),
        content: message.content.clone(),
        attachments: message.attachments.clone(),
    })
}

fn decode_conversation_content(
    role: String,
    content: String,
) -> Result<crate::port::provider::ChatMessage, MemoryError> {
    let value = serde_json::from_str::<serde_json::Value>(&content).ok();
    if value
        .as_ref()
        .and_then(|value| value.get("format"))
        .and_then(serde_json::Value::as_str)
        == Some(CONVERSATION_CONTENT_FORMAT)
    {
        let stored: StoredConversationContent = serde_json::from_value(value.unwrap_or(serde_json::Value::Null))
            .map_err(|error| MemoryError::backend(format!("invalid stored conversation content: {error}")))?;
        return Ok(crate::port::provider::ChatMessage {
            role,
            content: stored.content,
            attachments: stored.attachments,
        });
    }
    Ok(crate::port::provider::ChatMessage {
        role,
        content,
        attachments: vec![],
    })
}

/// rig 会话消息在 port Memory 里的 key 约定：key = 角色（user/assistant/…）。
const fn conversation_key(role: &str) -> &str {
    role
}

/// port Memory → rig ConversationMemory。
///
/// 存取语义：
/// - `append`：把 rig 消息拍平为 port canonical ChatMessage，逐条 store
///   （key=角色、category=Conversation、session_id=conversation_id）。
/// - `load`：recall 空查询召回该会话最近 RECALL_LIMIT 条。zeroclaw 各内存
///   后端的空查询返回 `ORDER BY updated_at DESC`（最新在前，
///   sqlite.rs:664 / postgres.rs:549），此处反转为时间正序（最早在前）再
///   复原为 rig 消息；非 canonical 内容回退为纯文本消息（见 provider
///   模块注释）。
/// - `clear`：port Memory 无会话级删除原语（forget 按 key 删、跨会话），
///   保持 no-op —— Global Constraints #6 文档化差异。
pub struct PortMemoryAsConversation {
    inner: Arc<dyn Memory>,
}

impl PortMemoryAsConversation {
    pub fn new(inner: Arc<dyn Memory>) -> Self {
        Self { inner }
    }

    pub fn inner(&self) -> &Arc<dyn Memory> {
        &self.inner
    }
}

impl ConversationMemory for PortMemoryAsConversation {
    fn load<'a>(&'a self, conversation_id: &'a str) -> WasmBoxedFuture<'a, Result<Vec<Message>, MemoryError>> {
        Box::pin(async move {
            let mut entries = self
                .inner
                .recall("", RECALL_LIMIT, Some(conversation_id), None, None)
                .await
                .map_err(|e| MemoryError::backend(e.to_string()))?;
            // zeroclaw 后端空查询返回 DESC（最新在前）——反转为正序，
            // 让注入 rig chat_history 的历史最早在前。
            entries.reverse();
            let port: Vec<crate::port::provider::ChatMessage> = entries
                .into_iter()
                .map(|entry| decode_conversation_content(entry.key, entry.content))
                .collect::<Result<_, _>>()?;
            // recall 窗口切断 tool_call/tool_result 配对会产生孤儿引用，
            // 严格 provider（MiniMax/Anthropic）400 拒整轮——回放前清洗。
            port_messages_to_rig(&super::provider::sanitize_tool_pairing(port))
                .map_err(|error| MemoryError::backend(error.to_string()))
        })
    }

    fn append<'a>(
        &'a self,
        conversation_id: &'a str,
        messages: Vec<Message>,
    ) -> WasmBoxedFuture<'a, Result<(), MemoryError>> {
        Box::pin(async move {
            for port in rig_messages_to_port(&messages).map_err(|error| MemoryError::backend(error.to_string()))? {
                let content =
                    encode_conversation_content(&port).map_err(|error| MemoryError::backend(error.to_string()))?;
                self.inner
                    .store(
                        conversation_key(&port.role),
                        &content,
                        MemoryCategory::Conversation,
                        Some(conversation_id),
                    )
                    .await
                    .map_err(|e| MemoryError::backend(e.to_string()))?;
            }
            Ok(())
        })
    }

    fn clear<'a>(&'a self, _conversation_id: &'a str) -> WasmBoxedFuture<'a, Result<(), MemoryError>> {
        Box::pin(async { Ok(()) })
    }
}
