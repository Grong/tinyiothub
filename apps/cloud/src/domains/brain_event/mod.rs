//! TinyIoTHub brain_event 域 — AI 大脑事件流只读面（大脑主干化 P0 Task 3）。
//!
//! brain_events 视图（judgments ∪ agent_actions ∪ agent_runs）的人工消费面：
//! feed 列表（tab 筛选 + 游标分页）、头部摘要、单条详情（含证据）。只读——
//! 写路径（反馈/审批）仍走 /judgments。

pub mod dto;
pub mod handler;

pub use dto::*;

/// Brain events API router（`/brain-events`），与 judgment 域同构。
pub fn router() -> axum::Router<crate::state::AppState> {
    handler::create_brain_event_router()
}
