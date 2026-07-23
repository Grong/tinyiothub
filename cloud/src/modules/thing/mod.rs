// Thing module — 3-layer architecture (Handler → Service → Repo)
// Provides the management API for Things (物), the universal entity model
// superseding the old device-centric model.

pub mod errors;
pub mod handler;
pub mod repo;
pub mod service;
pub mod summary;
pub mod types;

pub use types::*;
