//! notify 领域：通知规则、投递历史、渠道管理（email/sms/sse）。
//!
//! F1 试点（relay 范式）：自 crates/notify 回流。行类型与仓储在
//! `tinyiothub_storage::notify`；本模块只有行为（handler/service）与 API DTO。

pub mod channels;
pub mod dto;
pub mod handler;
pub mod service;

pub use dto::*;
pub use service::*;
