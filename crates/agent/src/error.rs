//! AgentError — crates/agent 的公开错误面。
//!
//! 纯运行时语义：不携带 HTTP 状态码（Web 映射属于 apps/cloud 的 host 层）。
//! 运行时内部多用 anyhow 传播；本类型供需要结构化错误分类的跨 crate 调用方
//! 使用。

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
}
