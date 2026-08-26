//! Short-lived SSE token issuer seam.
//!
//! Backed by cloud's `shared::sse_token::SseTokenManager`, which is shared
//! with the event plane (SSE connection handlers validate those tokens), so
//! the manager itself stays in cloud until the event extraction (Task 18).

/// Issues short-lived (5 minute) tokens for SSE connections, replacing JWTs
/// in URL query strings.
pub trait SseTokenIssuer: Send + Sync {
    fn generate_token(&self, user_id: &str, workspace_id: &str) -> String;
}
