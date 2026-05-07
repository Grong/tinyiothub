// Session Service — session lifecycle management and compaction

use std::sync::Arc;

use super::types::{ChatMessage, CompactedSession, Session, SessionError, SessionRepository};

// Compaction constants
const MAX_MESSAGES_IN_MEMORY: usize = 50;
const COMPACT_THRESHOLD_TOKENS: usize = 8000;
const SUMMARY_PREFIX: &str = "[对话历史摘要]";

/// Check if messages need compaction
pub fn should_compact(messages: &[ChatMessage]) -> bool {
    if messages.len() <= MAX_MESSAGES_IN_MEMORY {
        return false;
    }
    messages.len() * 200 > COMPACT_THRESHOLD_TOKENS
}

/// Estimate total token count for a slice of messages
pub fn estimate_tokens(messages: &[ChatMessage]) -> usize {
    messages.iter().map(|m| m.content.len() / 4 + 20).sum()
}

/// Compact messages into system + summary + recent groups
pub fn compact_messages(
    messages: &[ChatMessage],
    summary: &str,
) -> (Vec<ChatMessage>, Option<ChatMessage>, Vec<ChatMessage>) {
    let system_messages: Vec<_> = messages.iter().filter(|m| m.role == "system").cloned().collect();
    let recent: Vec<_> = messages
        .iter()
        .filter(|m| m.role == "user" || m.role == "assistant")
        .rev()
        .take(20)
        .cloned()
        .collect();
    let summary_message = if !summary.is_empty() {
        Some(ChatMessage {
            role: "system".to_string(),
            content: format!("{}\n{}", SUMMARY_PREFIX, summary),
            timestamp: Some(chrono::Utc::now().timestamp_millis()),
            tool_call_id: None,
            tool_name: None,
            run_id: None,
        })
    } else {
        None
    };
    (system_messages, summary_message, recent.into_iter().rev().collect())
}

/// Rebuild a message list from compacted groups
pub fn rebuild_messages(
    system: &[ChatMessage],
    summary: &Option<ChatMessage>,
    recent: &[ChatMessage],
) -> Vec<ChatMessage> {
    let mut result = system.to_vec();
    if let Some(s) = summary {
        result.push(s.clone());
    }
    result.extend(recent.to_vec());
    result
}

/// Generate a default summary from old messages
pub fn generate_default_summary(old_messages: &[ChatMessage]) -> String {
    let user_count = old_messages.iter().filter(|m| m.role == "user").count();
    let assistant_count = old_messages.iter().filter(|m| m.role == "assistant").count();
    let total_tokens = estimate_tokens(old_messages);
    format!(
        "早期对话包含 {} 条用户消息和 {} 条助手消息。总计约 {} tokens。如需了解详情，请询问用户。",
        user_count, assistant_count, total_tokens
    )
}

/// Session service for managing session lifecycle
pub struct SessionService {
    repo: Arc<dyn SessionRepository>,
}

impl SessionService {
    pub fn new(repo: Arc<dyn SessionRepository>) -> Self {
        Self { repo }
    }

    pub async fn get_session(&self, session_key: &str) -> Result<Option<Session>, SessionError> {
        self.repo.get(session_key).await
    }

    pub async fn create_session(
        &self,
        session_key: String,
        workspace_id: String,
        agent_id: String,
    ) -> Result<Session, SessionError> {
        let session = Session::new(session_key, workspace_id, agent_id);
        self.repo.create(&session).await?;
        Ok(session)
    }

    pub async fn update_label(
        &self,
        session_key: &str,
        label: impl Into<String>,
    ) -> Result<Session, SessionError> {
        let mut session = self
            .repo
            .get(session_key)
            .await?
            .ok_or_else(|| SessionError::NotFound(session_key.to_string()))?;
        session.set_label(label);
        self.repo.update(&session).await?;
        Ok(session)
    }

    pub async fn delete_session(&self, session_key: &str) -> Result<(), SessionError> {
        self.repo.delete(session_key).await
    }

    pub async fn list_sessions(
        &self,
        workspace_id: Option<&str>,
        agent_id: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Session>, SessionError> {
        self.repo.list(workspace_id, agent_id, limit, offset).await
    }

    pub async fn add_message(
        &self,
        session_key: &str,
        message: ChatMessage,
    ) -> Result<(), SessionError> {
        if let Some(mut session) = self.repo.get(session_key).await? {
            session.touch();
            self.repo.update(&session).await?;
        }
        self.repo.add_message(session_key, message).await
    }

    pub async fn get_messages(
        &self,
        session_key: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ChatMessage>, SessionError> {
        self.repo.get_messages(session_key, limit, offset).await
    }

    pub async fn get_message_count(&self, session_key: &str) -> Result<usize, SessionError> {
        self.repo.get_message_count(session_key).await
    }

    /// Check if a session needs compaction
    pub async fn check_compaction_needed(&self, session_key: &str) -> Result<bool, SessionError> {
        let messages = self.repo.get_messages(session_key, usize::MAX, 0).await?;
        Ok(should_compact(&messages))
    }

    /// Compact a session's conversation history
    pub async fn compact_session(
        &self,
        session_key: &str,
        summary: impl Into<String>,
    ) -> Result<CompactedSession, SessionError> {
        let messages = self.repo.get_messages(session_key, usize::MAX, 0).await?;
        let original_count = messages.len();

        let (system_messages, summary_message, recent_messages) =
            compact_messages(&messages, &summary.into());

        let compacted_session = CompactedSession {
            session_key: session_key.to_string(),
            system_messages,
            summary_message,
            recent_messages,
            compacted_at: chrono::Utc::now().timestamp_millis(),
            original_message_count: original_count,
        };

        self.repo.save_compacted(&compacted_session).await?;
        Ok(compacted_session)
    }

    pub async fn get_compacted(
        &self,
        session_key: &str,
    ) -> Result<Option<CompactedSession>, SessionError> {
        self.repo.get_compacted(session_key).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_compact() {
        let small = vec![ChatMessage::user("hello"); 10];
        assert!(!should_compact(&small));
        let large = vec![ChatMessage::user("hello"); 60];
        assert!(should_compact(&large));
    }

    #[test]
    fn test_compact_messages() {
        let mut messages = vec![ChatMessage::system("You are a helpful assistant.")];
        for i in 0..50 {
            messages.push(if i % 2 == 0 {
                ChatMessage::user(format!("Message {}", i))
            } else {
                ChatMessage::assistant(format!("Message {}", i))
            });
        }
        let (system, summary, recent) =
            compact_messages(&messages, "Earlier conversation about various topics.");
        assert_eq!(system.len(), 1);
        assert!(summary.is_some());
        assert!(recent.len() <= 20);

        let rebuilt = rebuild_messages(&system, &summary, &recent);
        assert!(rebuilt.len() <= 22);
    }
}
