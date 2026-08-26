//! SSE Token — 短期专用 token 用于 SSE 连接认证
//!
//! SSE 使用 EventSource API 无法自定义 HTTP headers，因此传统上
//! JWT 被放在 URL 查询参数中传递（?token=xxx），这会导致 token
//! 泄露到服务器日志和浏览器历史中。
//!
//! 改进方案：
//! - 客户端通过 POST /api/v1/auth/sse-token 获取一个短期（5分钟）的 SSE token
//! - 在 SSE URL 中使用这个短期 token 替代 JWT
//! - 即使 token 泄露，也会在 5 分钟内过期

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use dashmap::DashMap;
use uuid::Uuid;

/// SSE token 条目
#[derive(Clone)]
pub struct SseTokenEntry {
    /// 关联的用户 ID
    pub user_id: String,
    /// 关联的工作空间 ID
    pub workspace_id: String,
    /// 创建时间，用于判断过期
    pub created_at: Instant,
}

/// SSE token 管理器
pub struct SseTokenManager {
    tokens: Arc<DashMap<String, SseTokenEntry>>,
    ttl: Duration,
}

impl Default for SseTokenManager {
    fn default() -> Self {
        Self::new(Duration::from_secs(300)) // 5分钟 TTL
    }
}

impl SseTokenManager {
    pub fn new(ttl: Duration) -> Self {
        Self {
            tokens: Arc::new(DashMap::new()),
            ttl,
        }
    }

    /// 为用户生成一个新的 SSE token
    pub fn generate_token(&self, user_id: &str, workspace_id: &str) -> String {
        let token = Uuid::new_v4().to_string();
        self.tokens.insert(
            token.clone(),
            SseTokenEntry {
                user_id: user_id.to_string(),
                workspace_id: workspace_id.to_string(),
                created_at: Instant::now(),
            },
        );
        token
    }

    /// 验证并消费一个 SSE token
    /// 返回 (user_id, workspace_id) 如果 token 有效
    pub fn validate_and_consume(&self, token: &str) -> Option<(String, String)> {
        if let Some((_, entry)) = self.tokens.remove(token)
            && entry.created_at.elapsed() < self.ttl
        {
            return Some((entry.user_id, entry.workspace_id));
        }
        None
    }

    /// 清理过期 token
    pub fn cleanup_expired(&self) {
        let now = Instant::now();
        self.tokens
            .retain(|_, entry| now.duration_since(entry.created_at) < self.ttl);
    }

    /// 获取当前 token 数量
    pub fn token_count(&self) -> usize {
        self.tokens.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_validate() {
        let manager = SseTokenManager::new(Duration::from_secs(300));
        let token = manager.generate_token("user_1", "ws_1");
        let result = manager.validate_and_consume(&token);
        assert_eq!(result, Some(("user_1".to_string(), "ws_1".to_string())));
    }

    #[test]
    fn test_token_one_time_use() {
        let manager = SseTokenManager::new(Duration::from_secs(300));
        let token = manager.generate_token("user_1", "ws_1");
        // First use — valid
        assert!(manager.validate_and_consume(&token).is_some());
        // Second use — consumed, invalid
        assert!(manager.validate_and_consume(&token).is_none());
    }

    #[test]
    fn test_expired_token() {
        // TTL of 0 means tokens expire immediately
        let manager = SseTokenManager::new(Duration::from_secs(0));
        let token = manager.generate_token("user_1", "ws_1");
        // Even on creation, a 0-second TTL should expire
        // due to the Instant::now() comparison being >=
        let result = manager.validate_and_consume(&token);
        assert!(result.is_none());
    }

    #[test]
    fn test_cleanup_expired() {
        let manager = SseTokenManager::new(Duration::from_secs(0));
        let _token = manager.generate_token("user_1", "ws_1");
        assert_eq!(manager.token_count(), 1);
        manager.cleanup_expired();
        assert_eq!(manager.token_count(), 0);
    }
}
