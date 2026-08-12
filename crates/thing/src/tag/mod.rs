// tag module — Handler → Service → Repo 三层架构

pub mod handler;
pub mod service;
pub mod types;

pub use handler::create_router;
// Repositories live in the db crate (E5 集中化); re-exported for compatibility.
pub use service::TagService;
pub use tinyiothub_storage::tag::{TagBindingRepository, TagRepository};
pub use types::*;
