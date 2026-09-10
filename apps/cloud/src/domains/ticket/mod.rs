//! TinyIoTHub ticket domain — Agent→人 升级原语（工单模块 M1）。
//!
//! 自治 run 失败（outcome∈{failed, budget_exceeded, rejected}）由
//! `agent::host::ticket_subscriber` 开票；本域提供人工消费面：
//! 列表/详情/认领/开始处理/解决/关闭/放弃认领。
//!
//! 设计契约（设计文档）：
//! - 状态机：open → claimed → in_progress → resolved → closed；
//!   claimed → open（abandon）。M1 无 reopen、无 system 自动关闭。
//! - 全部迁移条件更新（db 层 `WHERE state='预期值'`），冲突返回 409 +
//!   当前态（认领冲突文案「已被 {assignee} 认领」）。
//! - 解决必填 resolution_text（知识闭环的入海口）。
//! - workspace 隔离来自 AuthClaims（auth context），不信任请求参数。

pub mod dto;
pub mod handler;
pub mod service;

pub use dto::*;
pub use service::*;

/// Tickets API router（`/tickets`），与 alarm 域同构。
pub fn router() -> axum::Router<crate::state::AppState> {
    handler::create_ticket_router()
}
