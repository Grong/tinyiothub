//! Documented boundary — files/concepts that stay in cloud and why
//! (SEP addendum rule 3).
//!
//! - Per-workspace heartbeat handlers (`/workspaces/{id}/heartbeat/*`) — moved to
//!   the agent crate (`crates/agent/src/host/handler/workspace_heartbeat.rs`, P4-Task22).
//!   They are AI/agent-plane code
//!   (HeartbeatRunner from the agent domain's loop plane, `agent_actions` table, external
//!   tool registry port for proposal execution) that happens to be mounted under workspace paths;
//!   the composition layer nests them at `/workspaces` next to this crate's router (same pattern as
//!   the T14 directive entries). Reclaim when the MCP plane is extracted.
//! - `WorkspaceAgentLifecycle` / `TagSuggester` seams — implemented in
//!   `cloud/src/shared/app_state.rs` over `AgentPool` (agent plane) and the minimax provider
//!   (zeroclaw type), respectively.
//! - `cloud::shared::paths` — stays in cloud (consumed by server, service_manager, agent, system
//!   modules); the crate receives the computed `agents_base_dir` via `AppState` because
//!   `env!("CARGO_MANIFEST_DIR")` would resolve against the crate.
//! - Knowledge resources (`resources` table, formerly `thing_resources`): owned by THIS crate
//!   (`workspace::{repo, types}`). The thing crate only references the table name in a doc comment
//!   (`crates/thing/src/types.rs`); no code consumer exists there — no shared `crates/db` reclaim
//!   needed.
//! - `AuthHelper` (admin checks) — see the user crate's `legacy/mod.rs`; tenant handlers never used
//!   it.
