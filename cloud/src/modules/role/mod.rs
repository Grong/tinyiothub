// role module — Handler → Service → Repo 三层架构

pub mod handler;
pub mod repo;
pub mod service;
pub mod types;

pub use handler::create_router;
pub use repo::{RoleRepository, SqliteRoleRepository};
pub use service::RoleService;
pub use types::*;
