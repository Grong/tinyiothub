//! AgentError — crates/agent 的公开错误面。
//!
//! 纯运行时语义：不携带 HTTP 状态码（Web 映射属于组合层 apps/cloud 的 host）。
//! 运行时内部多用 anyhow 传播；本类型供需要结构化错误分类的跨 crate 调用方
//! 使用。
//!
//! Task 14 合并：host 侧原 `shared::config::AgentError` 的变体（RequestFailed /
//! ApiError / Timeout / Unavailable / NotFound / BuildError / StreamError）并入本
//! 类型，全 workspace 只有这一个 agent 错误类型。

/// Agent 运行时公开错误类型。
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    /// LLM 提供方调用失败
    #[error("llm error: {0}")]
    Llm(String),
    /// 工具执行失败
    #[error("tool error: {0}")]
    Tool(String),
    /// 策略/自治 gate 拒绝
    #[error("policy error: {0}")]
    Policy(String),
    /// 会话生命周期错误
    #[error("session error: {0}")]
    Session(String),
    /// 其他内部错误
    #[error("internal error: {0}")]
    Internal(String),

    // ------------------------------------------------------------------
    // host 面变体（Task 14 自 apps/cloud host 并入；组合层按语义映射 HTTP）
    // ------------------------------------------------------------------
    /// Agent API 请求失败
    #[error("Agent API request failed: {0}")]
    RequestFailed(String),
    /// Agent API 返回错误
    #[error("Agent API returned error: {0}")]
    ApiError(String),
    /// Agent API 超时
    #[error("Agent API timeout")]
    Timeout,
    /// Agent 不可用
    #[error("Agent unavailable: {0}")]
    Unavailable(String),
    /// agent 不存在
    #[error("agent not found: {0}")]
    NotFound(String),
    /// agent 构建失败
    #[error("agent build failed: {0}")]
    BuildError(String),
    /// agent 流式错误
    #[error("agent stream error: {0}")]
    StreamError(String),
}
