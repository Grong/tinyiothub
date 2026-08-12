// permission module — Handler → Service → Repo 三层架构

pub mod handler;
pub mod service;
pub mod types;

pub use handler::create_router;
pub use service::PermissionService;
pub use tinyiothub_storage::permission::{PermissionGroupRepository, PermissionRepository};
pub use types::*;
