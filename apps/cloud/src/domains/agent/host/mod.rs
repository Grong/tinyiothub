//! Deprecated compatibility shell (one version).
//!
//! The agent host plane was split into its real homes under
//! `domains::agent::{session, tool, prompt}`. This module only re-exports the
//! new paths so downstream imports keep compiling for one release; it holds no
//! logic and must not gain any.

pub use crate::domains::agent::prompt::scaffold;
pub use crate::domains::agent::session::{
    agent_host, chat, chat_types, config, factory, handlers, index, judge, notify, persist, pool_adapter, ports,
    triggers,
};
pub use crate::domains::agent::tool;

// Legacy module names.
pub use crate::domains::agent::session::agent_host as thing_agent_host;
pub use crate::domains::agent::session::chat_types as types;
pub use crate::domains::agent::session::factory as autonomous_factory;
pub use crate::domains::agent::session::handlers as handler;
pub use crate::domains::agent::session::index as service;
pub use crate::domains::agent::session::triggers as trigger;

pub use crate::domains::agent::session::index::SessionService;
