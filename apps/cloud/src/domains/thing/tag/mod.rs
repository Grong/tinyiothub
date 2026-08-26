// tag module — Handler → Service → Repo 三层架构

pub mod handler;
pub mod service;

pub use handler::create_router;
pub use service::TagService;
