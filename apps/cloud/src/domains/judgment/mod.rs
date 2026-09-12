//! TinyIoTHub judgment domain — AI 处置判断（大脑主干化 P0）。
//!
//! 处置流（设计文档状态机）：报警 → thing-agent 调查 → judgment 三出口
//! （噪声归档 / 待审批 / 需人工）。本域提供人工消费面：feed 列表、摘要、
//! 对/错反馈、批准/拒绝。

pub mod dto;
pub mod handler;

pub use dto::*;

/// Judgments API router（`/judgments`），与 ticket 域同构。
pub fn router() -> axum::Router<crate::state::AppState> {
    handler::create_judgment_router()
}
