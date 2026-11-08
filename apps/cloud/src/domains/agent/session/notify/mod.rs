//! notify/ — 通知链：run 失败/需人工事件的工单订阅与 SSE 装配（ticket_subscriber，
//! 开票 + notify 域 workspace 广播一体）。
//! 谁调用：service_manager 装配 `supervise_ticket_subscriber`；测试经
//! `run_ticket_subscriber`/`ticket_for_run`/`failure_hash` 进入。
//! 依赖谁：runtime 事件总线（AgentEvent 订阅）、ticket 域 escalation seam、
//! event 域 SseConnectionManager、persist/dlq_repo（DLQ 兜底）。

pub mod ticket_subscriber;
