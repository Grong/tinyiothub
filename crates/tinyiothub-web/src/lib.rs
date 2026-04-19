//! TinyIoTHub web layer — HTTP handlers and middleware
//!
//! Currently a placeholder crate. Full handler extraction deferred
//! due to deep coupling between handlers and AppContext (30+ fields).
//!
//! When ready to migrate:
//! 1. Define WebState trait abstracting AppContext
//! 2. Migrate middleware (auth, cors, rate-limit)
//! 3. Migrate handlers domain by domain
//! 4. Update cloud/src/api/ to re-export from this crate

// Placeholder module for future middleware implementations
pub mod middleware {
    //! Tower middleware for authentication, CORS, rate limiting, etc.
    //!
    //! These will be migrated from cloud/src/shared/middleware/
    //! once the WebState trait abstraction is in place.
}

// Placeholder module for shared DTOs
pub mod dto {
    //! Shared request/response DTOs and ApiResponse builder.
    //!
    //! These will be migrated from cloud/src/dto/response/ and cloud/src/dto/request/
}

/// Re-export common dependencies for handlers.
pub use axum;
pub use tower;
pub use tower_http;
