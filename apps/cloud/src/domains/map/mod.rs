//! E4d maps / places 管理面（`/v1/maps`）。
//!
//! 读取按 JWT 工作区作用域；写入沿用 driver install 的 admin 闸门
//! （spec §2.4 / 裁定 #7）。几何与坐标的合法性在 handler 校验，持久化经
//! `tinyiothub_storage::Db`（workspace 隔离由 db 层 SQL 保证）。

pub mod handler;

pub use handler::create_router;

/// Mount point: `/api/v1/maps`.
pub fn router() -> axum::Router<crate::state::AppState> {
    handler::create_router()
}
