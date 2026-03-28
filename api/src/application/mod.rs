// Application Layer
// This module contains application services and orchestration logic

pub mod data_context;
pub mod data_server;
pub mod message_server;
pub mod scheduler;
pub mod service_manager;

pub use data_context::DataContext;

use std::sync::Arc;

/// 应用上下文（所有插件共享）
pub struct AppContext {
    pub data_context: Arc<DataContext>,
}
pub use data_server::DataServer;
pub use service_manager::ServiceManager;
