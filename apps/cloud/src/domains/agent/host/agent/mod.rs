// AgentPool — central agent lifecycle manager
//
// Key design decisions:
//   - Storage-free (Task 7 fix round 1): no database or storage-crate
//     references anywhere in this module. Agent CRUD, config CRUD and chat
//     send/history are db-backed and live as cloud-side free functions
//     (`host::config::service`, `host::tools::service`, `host::chat::*`);
//     callers resolve data first, then call the pool's pure methods.
//   - Explicit creation: cloud resolves config + tools, then `create`;
//     `get_cached` covers the fast path
//   - WorkspaceScopedMemory: workspace-level isolation via namespace wrapper
//   - Invalidation: remove from pool on config change, rebuild on next access
//
// G9 layout (one responsibility group per file):
//   pool.rs   — AgentPool struct, lifecycle (get_cached/create/invalidate/
//               cleanup), agent builder, skills prompt section
//   chat.rs   — chat abort + heartbeat runs (run_single/run_streaming)
//               + streaming result types

mod chat;
mod pool;

pub use chat::{StreamingRunResult, StreamingToolCall, heartbeat_agent_id};
pub use pool::{Agent, AgentPool};
pub(crate) use pool::load_workspace_skills;
