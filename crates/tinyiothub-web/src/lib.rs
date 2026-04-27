//! TinyIoTHub web layer — HTTP handlers and middleware
//!
//! Provides shared HTTP infrastructure: response types, auth claims,
//! rate limiting, and workspace scoping. No cloud-specific dependencies.

pub mod handlers;
pub mod middleware;
pub mod response;
pub mod security;
pub mod state;

pub use state::WebState;

/// Re-export common dependencies for handlers.
pub use axum;
pub use tower;
pub use tower_http;
