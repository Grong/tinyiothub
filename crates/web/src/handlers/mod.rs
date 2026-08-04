//! HTTP handlers for TinyIoTHub.
//!
//! Handlers are generic over `impl WebState` so they can be reused across
//! cloud, edge, and test binaries.

pub mod health;
