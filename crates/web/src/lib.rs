//! TinyIoTHub web layer — HTTP handlers and middleware
//!
//! Provides shared HTTP infrastructure: response types, auth claims,
//! rate limiting, and workspace scoping. No cloud-specific dependencies.
//!
//! ## 设计不变量
//! - 无业务逻辑、无 SQL；只做 HTTP 基础设施（middleware/响应构建/安全萃取）

pub mod api_response;
pub mod error_handling;
pub mod handlers;
pub mod middleware;
pub mod pagination;
pub mod response;
pub mod security;
pub mod state;
pub mod validation;

pub use api_response::ApiResponseBuilder;
pub use error_handling::{ErrorCategory, ErrorCode, ErrorContext, ErrorHandler};
pub use state::WebState;

/// Re-export common dependencies for handlers.
pub use axum;
pub use tower;
pub use tower_http;
