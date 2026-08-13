//! authn — 认证机制 crate（G2，buzz-auth 范式）。
//!
//! ## 设计不变量
//! - 纯机制：签发/校验/哈希，零 HTTP handler、零业务流程
//! - 构造注入：JwtService::new(secret)，禁止全局可变状态
//! - 可独立测试：不依赖 apps/*，不感知 AppState

pub mod api_key;
pub mod jwt;
pub mod password;
pub mod sse_token;

pub use jwt::{Claims, JwtService, JwtSettings};
pub use password::{hash_password, verify_password};
pub use sse_token::SseTokenManager;
