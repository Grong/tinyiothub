// permission module — Handler → Service → Repo 三层架构

pub mod types;
pub mod repo;
pub mod service;
pub mod handler;

pub use types::*;
pub use repo::{PermissionRepository, PermissionGroupRepository, SqlitePermissionRepository, SqlitePermissionGroupRepository};
pub use service::PermissionService;
pub use handler::create_router;
