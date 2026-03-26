pub mod registry;
pub mod protocol;
pub mod notification;
pub mod scheduler;

pub use registry::{get_global_registry, PluginRegistry, PluginEntry, PluginManifest, PluginType, PluginHandler, PluginFactory};
pub use crate::application::AppContext;
