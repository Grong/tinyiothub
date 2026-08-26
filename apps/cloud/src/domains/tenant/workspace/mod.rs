// workspace submodule — workspaces CRUD + knowledge resources

pub mod handler;
pub mod service;

pub use handler::create_router;
pub use service::WorkspaceService;
