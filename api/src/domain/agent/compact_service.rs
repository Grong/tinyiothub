// Compact Service - 对话历史压缩服务
// 参考 Claude Code 的 Auto-Compact 机制

/// 最大保留消息数
const MAX_MESSAGES_IN_MEMORY: usize = 50;
/// 压缩阈值 tokens
const COMPACT_THRESHOLD_TOKENS: usize = 8000;
/// 摘要前缀
const SUMMARY_PREFIX: &str = "[对话历史摘要]";

/// 压缩后的对话消息
#[derive(Debug, Clone)]
pub struct CompactedMessages {
    /// 系统消息
    pub system_messages: Vec<ChatMessage>,
    /// 摘要消息
    pub summary_message: Option<ChatMessage>,
    /// 最近的用户/助手消息
    pub recent_messages: Vec<ChatMessage>,
}

/// 聊天消息结构
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    pub timestamp: Option<i64>,
}

pub struct CompactService;

impl CompactService {
    /// 检查是否需要压缩
    pub fn should_compact(messages: &[ChatMessage]) -> bool {
        if messages.len() <= MAX_MESSAGES_IN_MEMORY {
            return false;
        }
        // 简单估算：平均每条消息 200 tokens
        let estimated_tokens = messages.len() * 200;
        estimated_tokens > COMPACT_THRESHOLD_TOKENS
    }

    /// 估算消息的 token 数（粗略估算）
    pub fn estimate_tokens(messages: &[ChatMessage]) -> usize {
        messages.iter().map(|m| {
            // 粗略估算：内容长度 / 4 + role 和 timestamp 的开销
            m.content.len() / 4 + 20
        }).sum()
    }

    /// 压缩对话：保留系统消息 + 最近 N 条 + 摘要
    pub fn compact(messages: &[ChatMessage], summary: &str) -> CompactedMessages {
        // 保留所有系统消息
        let system_messages: Vec<_> = messages
            .iter()
            .filter(|m| m.role == "system")
            .cloned()
            .collect();

        // 保留最近 20 条用户/助手对话
        let recent: Vec<_> = messages
            .iter()
            .filter(|m| m.role == "user" || m.role == "assistant")
            .rev()
            .take(20)
            .cloned()
            .collect();

        // 创建摘要消息
        let summary_message = if !summary.is_empty() {
            Some(ChatMessage {
                role: "system".to_string(),
                content: format!("{}\n{}", SUMMARY_PREFIX, summary),
                timestamp: Some(chrono::Utc::now().timestamp_millis()),
            })
        } else {
            None
        };

        CompactedMessages {
            system_messages,
            summary_message,
            recent_messages: recent.into_iter().rev().collect(),
        }
    }

    /// 从压缩结果重建消息列表
    pub fn rebuild(compacted: &CompactedMessages) -> Vec<ChatMessage> {
        let mut result = compacted.system_messages.clone();
        if let Some(ref summary) = compacted.summary_message {
            result.push(summary.clone());
        }
        result.extend(compacted.recent_messages.clone());
        result
    }

    /// 生成默认摘要（用于压缩前的旧消息）
    pub fn generate_default_summary(old_messages: &[ChatMessage]) -> String {
        let user_count = old_messages.iter().filter(|m| m.role == "user").count();
        let assistant_count = old_messages.iter().filter(|m| m.role == "assistant").count();
        let total_tokens = Self::estimate_tokens(old_messages);

        format!(
            "早期对话包含 {} 条用户消息和 {} 条助手消息。 \
            总计约 {} tokens。\
            如需了解详情，请询问用户。",
            user_count, assistant_count, total_tokens
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_compact() {
        let small = vec![
            ChatMessage { role: "user".into(), content: "hello".into(), timestamp: None };
            10
        ];
        assert!(!CompactService::should_compact(&small));

        let large = vec![
            ChatMessage { role: "user".into(), content: "hello".into(), timestamp: None };
            60
        ];
        assert!(CompactService::should_compact(&large));
    }

    #[test]
    fn test_compact() {
        let mut messages = vec![
            ChatMessage { role: "system".into(), content: "You are a helpful assistant.".into(), timestamp: Some(1000) },
        ];
        for i in 0..50 {
            messages.push(ChatMessage {
                role: if i % 2 == 0 { "user".into() } else { "assistant".into() },
                content: format!("Message {}", i),
                timestamp: Some(1000 + i as i64),
            });
        }

        let summary = "Earlier conversation about various topics.";
        let compacted = CompactService::compact(&messages, summary);

        assert_eq!(compacted.system_messages.len(), 1);
        assert!(compacted.summary_message.is_some());
        assert!(compacted.recent_messages.len() <= 20);

        let rebuilt = CompactService::rebuild(&compacted);
        // Should have system + summary + recent
        assert!(rebuilt.len() <= 22);
    }
}
