// tag module — Handler → Service → Repo 三层架构

pub mod types;
pub mod repo;
pub mod service;
pub mod handler;

pub use types::*;
pub use repo::{TagRepository, TagBindingRepository, SqliteTagRepository, SqliteTagBindingRepository};
pub use service::TagService;
pub use handler::create_router;
