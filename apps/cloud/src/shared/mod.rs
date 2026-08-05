pub mod ai_adapter;
pub mod llm_provider;

pub mod command;

pub mod error;

pub mod identifier;

/// System bootstrap / initialization (default admin user, default tenant,
/// per-user workspace + Agent provisioning). Stayed in the composition
/// layer at P4-Task24 — entangled with the agent plane
/// (tinyiothub_agent::host::scaffold / AgentPool / shared::paths);
/// boundary documented in `tinyiothub_admin::legacy`.
pub mod initialization;

pub mod network;

pub mod paths;

pub mod utils;

pub mod app_state;

pub mod error_handling;

pub mod performance;

pub mod api_response;
pub mod pagination;
pub mod service_manager;
pub mod sse_token;

pub mod config;
pub mod event;
pub mod hardware;
pub mod mqtt_client;
