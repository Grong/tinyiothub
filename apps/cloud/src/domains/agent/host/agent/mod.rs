// AgentPool — central agent lifecycle manager
//
// Composes capability services (Chat, Config, Tools) into a unified Agent API.
// Key design decisions:
//   - Lazy creation: agents built on first access, config injected by the
//     cloud caller (Task 7 — the pool holds no storage handles)
//   - Tool denylist: resolved at build time from AgentRuntimeConfig
//   - WorkspaceScopedMemory: workspace-level isolation via namespace wrapper
//   - Invalidation: remove from pool on config change, rebuild on next access
//
// G9 layout (one responsibility group per file):
//   pool.rs   — AgentPool struct, lifecycle (get_or_create/invalidate/cleanup),
//               agent builder, skills prompt section
//   config.rs — agent CRUD + config CRUD + tools catalog/toggle
//   chat.rs   — chat session forwarding (send/history/abort) + heartbeat runs
//               (run_single/run_streaming) + streaming result types

mod chat;
mod config;
mod pool;

pub use chat::{StreamingRunResult, StreamingToolCall};
pub use pool::{Agent, AgentPool};
pub(crate) use pool::load_workspace_skills;
