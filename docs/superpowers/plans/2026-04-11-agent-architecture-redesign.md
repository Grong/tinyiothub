# AI Agent Architecture Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Simplify AI Agent architecture by removing external gateway code, fixing security vulnerabilities, and establishing clear DDD 4-layer architecture with session-aware memory.

**Architecture:** 
- 4-layer DDD: API → Application → Infrastructure → Domain
- Remove external gateway (`ZeroClawAgentClient`, `FallbackAgentClient`)
- Keep zeroclaw as底层引擎, rename upper layers to remove "zeroclaw" references
- Add workspace isolation security for all MCP tools

**Tech Stack:** Rust 2021, Tokio, Axum, zeroclaw (底层), SQLite, SQLx

---

## Phase 1: Security Fixes + Code Cleanup

### Task 1: Complete Remaining MCP Tool Security Audits

**Files to audit:**
- `api/src/api/mcp/tools/device.rs`
- `api/src/api/mcp/tools/job.rs`
- `api/src/api/mcp/tools/schedule_mcp.rs`
- `api/src/api/mcp/tools/self_heal.rs`
- `api/src/api/mcp/tools/diagnostics.rs`

**For each file, check:**
1. Does handler use `get_mcp_context()` to get authenticated workspace?
2. Does handler verify `claims.workspace_id` matches target resource?
3. Are there any `_claims` (ignored) patterns that need fixing?

- [ ] **Step 1: Audit device.rs for workspace isolation**

Run: `grep -n "get_mcp_context\|workspace_id\|_claims" api/src/api/mcp/tools/device.rs`

Document findings in comments.

- [ ] **Step 2: Audit job.rs for workspace isolation**

Run: `grep -n "get_mcp_context\|workspace_id\|_claims" api/src/api/mcp/tools/job.rs`

Document findings in comments.

- [ ] **Step 3: Audit schedule_mcp.rs for workspace isolation**

Run: `grep -n "get_mcp_context\|workspace_id\|_claims" api/src/api/mcp/tools/schedule_mcp.rs`

Document findings in comments.

- [ ] **Step 4: Audit self_heal.rs for workspace isolation**

Run: `grep -n "get_mcp_context\|workspace_id\|_claims" api/src/api/mcp/tools/self_heal.rs`

Document findings in comments.

- [ ] **Step 5: Audit diagnostics.rs for workspace isolation**

Run: `grep -n "get_mcp_context\|workspace_id\|_claims" api/src/api/mcp/tools/diagnostics.rs`

Document findings in comments.

- [ ] **Step 6: Fix any missing workspace isolation in audited files**

Apply pattern from batch.rs:
```rust
let claims = get_mcp_context().ok_or_else(|| {
    ToolError::Unauthorized("MCP context not initialized".to_string())
})?;

// SECURITY: Verify workspace_id matches authenticated context
if input.workspace_id != claims.workspace_id {
    return Err(ToolError::Forbidden(
        "Access denied: workspace_id does not match authenticated workspace".to_string()
    ));
}
```

- [ ] **Step 7: Run cargo check to verify fixes compile**

Run: `cargo check --lib`

Expected: 0 errors

- [ ] **Step 8: Commit security fixes**

```bash
git add api/src/api/mcp/tools/
git commit -m "fix(security): add workspace isolation to all MCP tools

- Audit and fix device.rs, job.rs, schedule_mcp.rs, self_heal.rs, diagnostics.rs
- Add workspace_id verification to prevent IDOR attacks
- All cross-workspace access returns ToolError::Forbidden"
```

---

### Task 2: Remove External Gateway Code from zeroclaw_agent.rs

**Files:**
- Modify: `api/src/infrastructure/zeroclaw_agent.rs` (remove ~2000 lines)
- Preserve: `AgentConfig`, `AgentInfo`, `AgentError`, `build_tools_catalog_json()`, prompt functions

**What to delete:**
1. `ZeroClawAgentClient` struct and impl (lines ~150-500)
2. `FallbackAgentClient` struct and impl (lines ~928-1100)
3. `ApiResponse`, `ZeroClawIncoming` structs
4. WebSocket/HTTP client code
5. External gateway-specific methods

**What to keep:**
1. `AgentConfig`, `AgentInfo`, `AgentError` structs
2. `build_tools_catalog_json()` function
3. `platform_base_prompt()`, `build_full_system_prompt()`
4. `default_agent_config()`, `compute_hash()`
5. `TinyIoTHubAgentClient` (to be renamed later)

- [ ] **Step 1: Identify and mark code sections for deletion**

Run analysis:
```bash
grep -n "pub struct ZeroClawAgentClient\|pub struct FallbackAgentClient\|pub struct ApiResponse\|pub struct ZeroClawIncoming" api/src/infrastructure/zeroclaw_agent.rs
grep -n "impl AgentClient for ZeroClawAgentClient\|impl AgentClient for FallbackAgentClient" api/src/infrastructure/zeroclaw_agent.rs
```

- [ ] **Step 2: Create backup of current file**

```bash
cp api/src/infrastructure/zeroclaw_agent.rs api/src/infrastructure/zeroclaw_agent.rs.backup
```

- [ ] **Step 3: Delete ZeroClawAgentClient implementation**

Find the line range for `ZeroClawAgentClient` struct and all its impl blocks. Delete them.

- [ ] **Step 4: Delete FallbackAgentClient implementation**

Find the line range for `FallbackAgentClient` struct and all its impl blocks. Delete them.

- [ ] **Step 5: Delete unused structs**

Delete `ApiResponse`, `ZeroClawIncoming`, and other external-gateway-specific structs.

- [ ] **Step 6: Run cargo check**

Run: `cargo check --lib 2>&1 | grep "^error" | head -20`

Expected: May show errors for code that referenced deleted types - we'll fix in next steps.

- [ ] **Step 7: Commit partial cleanup**

```bash
git add api/src/infrastructure/zeroclaw_agent.rs
git commit -m "chore(cleanup): remove external gateway code from zeroclaw_agent.rs

- Remove ZeroClawAgentClient
- Remove FallbackAgentClient
- Remove unused ApiResponse/ZeroClawIncoming structs
- Keep TinyIoTHubAgentClient and utility functions"
```

---

### Task 3: Update workspace.rs to Remove FallbackAgentClient

**Files:**
- Modify: `api/src/api/mcp/tools/workspace.rs`

**Changes needed:**
1. Remove `use crate::infrastructure::zeroclaw_agent::{AgentClient, AgentConfig};`
2. In `CreateWorkspaceHandler.execute()`: Remove agent creation logic
3. In `DeleteWorkspaceHandler.execute()`: Remove agent deletion logic
4. Update workspace creation to not require agent_id

- [ ] **Step 1: Remove FallbackAgentClient import and usage in CreateWorkspaceHandler**

Current code (lines 245-296):
```rust
// Try to create Agent
let client = crate::infrastructure::zeroclaw_agent::FallbackAgentClient::new(db.pool().clone());
let agent_result = client
    .create_agent(&AgentConfig { ... })
    .await;
```

Replace with: Just create workspace, agent creation will be handled separately in Phase 2.

- [ ] **Step 2: Remove FallbackAgentClient usage in DeleteWorkspaceHandler**

Current code (lines 456-467):
```rust
// Try to delete Agent
if let Some(agent_id) = workspace.agent_id {
    let client = crate::infrastructure::zeroclaw_agent::FallbackAgentClient::new(db.pool().clone());
    if let Err(e) = client.delete_agent(&agent_id).await { ... }
}
```

Replace with: Just delete workspace, agent cleanup will be handled separately.

- [ ] **Step 3: Remove unused import**

Delete line 15:
```rust
use crate::infrastructure::zeroclaw_agent::{AgentClient, AgentConfig};
```

- [ ] **Step 4: Run cargo check**

Run: `cargo check --lib`

Expected: 0 errors

- [ ] **Step 5: Commit changes**

```bash
git add api/src/api/mcp/tools/workspace.rs
git commit -m "refactor(workspace): remove FallbackAgentClient dependency

- Remove agent creation from workspace_create handler
- Remove agent deletion from workspace_delete handler
- Simplify workspace management, agent lifecycle handled separately"
```

---

### Task 4: Consolidate AppState Agent Fields

**Files:**
- Modify: `api/src/shared/app_state.rs`

**Current state (to investigate):**
```rust
pub struct AppState {
    // ... other fields ...
    pub agent_client: Arc<dyn AgentClient>,  // To remove
    pub tinyiothub_agent: Arc<TinyIoTHubAgentClient>,  // To rename
}
```

**Target state:**
```rust
pub struct AppState {
    // ... other fields ...
    pub agent_runtime: Arc<dyn AgentRuntime>,  // Single field
}
```

- [ ] **Step 1: Read current AppState definition**

Run: `grep -n "agent_client\|tinyiothub_agent\|pub struct AppState" api/src/shared/app_state.rs | head -30`

- [ ] **Step 2: Define AgentRuntime trait in preparation**

In `api/src/infrastructure/agent/mod.rs` (create if not exists):
```rust
#[async_trait]
pub trait AgentRuntime: Send + Sync {
    async fn turn_streamed(
        &self,
        session: &Session,
        user_message: &str,
        system_prompt: &str,
        history: &[ChatMessage],
    ) -> Result<BoxStream<'static, TurnEvent>, AgentError>;

    async fn refresh_tools(&self) -> anyhow::Result<()>;
}
```

- [ ] **Step 3: Update AppState to use single agent_runtime field**

Replace:
```rust
pub agent_client: Arc<dyn AgentClient>,
pub tinyiothub_agent: Arc<TinyIoTHubAgentClient>,
```

With:
```rust
pub agent_runtime: Arc<dyn AgentRuntime>,
```

- [ ] **Step 4: Update AppState::new() initialization**

Find `AppState::new()` and update agent initialization.

- [ ] **Step 5: Run cargo check and fix all references**

Run: `cargo check --lib 2>&1 | grep "^error"`

Fix all references to old field names throughout codebase.

- [ ] **Step 6: Commit AppState consolidation**

```bash
git add api/src/shared/app_state.rs api/src/infrastructure/agent/mod.rs
git commit -m "refactor(app_state): consolidate agent fields into single agent_runtime

- Remove agent_client and tinyiothub_agent fields
- Add agent_runtime: Arc<dyn AgentRuntime>
- Update all references throughout codebase"
```

---

### Task 5: Rename TinyIoTHubAgentClient to AgentRuntime

**Files:**
- Modify: `api/src/infrastructure/zeroclaw_runtime.rs` → rename to `api/src/infrastructure/agent/runtime.rs`
- Modify: `api/src/infrastructure/mod.rs`

- [ ] **Step 1: Create new agent directory structure**

```bash
mkdir -p api/src/infrastructure/agent
touch api/src/infrastructure/agent/mod.rs
touch api/src/infrastructure/agent/runtime.rs
touch api/src/infrastructure/agent/config.rs
```

- [ ] **Step 2: Move and rename TinyIoTHubAgentClient**

From `zeroclaw_runtime.rs`: Extract `TinyIoTHubAgentClient` and move to `agent/runtime.rs` as `AgentRuntime`.

- [ ] **Step 3: Move AgentConfig/AgentInfo/AgentError to agent/config.rs**

From `zeroclaw_agent.rs`: Extract config types to new file.

- [ ] **Step 4: Update module exports**

In `api/src/infrastructure/agent/mod.rs`:
```rust
pub mod config;
pub mod runtime;

pub use config::{AgentConfig, AgentInfo, AgentError};
pub use runtime::AgentRuntime;
```

- [ ] **Step 5: Update infrastructure/mod.rs**

Replace:
```rust
pub mod zeroclaw_agent;
pub mod zeroclaw_runtime;
```

With:
```rust
pub mod agent;
```

- [ ] **Step 6: Run cargo check**

Run: `cargo check --lib`

Fix any import errors.

- [ ] **Step 7: Commit restructuring**

```bash
git add api/src/infrastructure/agent/
git add api/src/infrastructure/mod.rs
git add api/src/infrastructure/zeroclaw_agent.rs api/src/infrastructure/zeroclaw_runtime.rs
git commit -m "refactor(agent): restructure into DDD infrastructure layer

- Create infrastructure/agent/ directory
- Rename TinyIoTHubAgentClient to AgentRuntime
- Move config types to agent/config.rs
- Update module exports"
```

---

## Phase 2: DDD Layer Implementation

### Task 6: Create Application Layer (ChatService)

**Files:**
- Create: `api/src/application/agent/mod.rs`
- Create: `api/src/application/agent/chat_service.rs`

- [ ] **Step 1: Create application/agent/mod.rs**

```rust
pub mod chat_service;
pub mod session_service;
pub mod memory_service;

pub use chat_service::{ChatService, ChatRequest, ChatEvent, ChatError};
pub use session_service::{SessionService, SessionRepository, Session, ChatMessage};
pub use memory_service::{AgentMemoryService, MemoryContext};
```

- [ ] **Step 2: Create chat_service.rs with basic structure**

```rust
use std::sync::Arc;
use crate::infrastructure::agent::{AgentRuntime, AgentError};
use crate::application::agent::{SessionRepository, Session, AgentMemoryService};

pub struct ChatService {
    runtime: Arc<dyn AgentRuntime>,
    session_repo: Arc<dyn SessionRepository>,
    memory_service: Arc<AgentMemoryService>,
}

pub struct ChatRequest {
    pub session_key: String,
    pub message: String,
    pub run_id: String,
    pub system_prompt_override: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ChatEvent {
    Delta { message: serde_json::Value },
    Thinking { thinking: String },
    ToolCallStart { tool_name: String, tool_args: String, a2ui: Option<String> },
    ToolResult { tool_name: String, result: String },
    Final { message: serde_json::Value },
    Error { error: String },
}

#[derive(Debug, thiserror::Error)]
pub enum ChatError {
    #[error("Session error: {0}")]
    SessionError(String),
    #[error("Runtime error: {0}")]
    RuntimeError(#[from] AgentError),
}

impl ChatService {
    pub fn new(
        runtime: Arc<dyn AgentRuntime>,
        session_repo: Arc<dyn SessionRepository>,
        memory_service: Arc<AgentMemoryService>,
    ) -> Self {
        Self { runtime, session_repo, memory_service }
    }

    pub async fn chat(&self, req: ChatRequest) -> Result<Vec<ChatEvent>, ChatError> {
        // Phase 2 implementation
        todo!("Implement chat flow")
    }
}
```

- [ ] **Step 3: Run cargo check**

Run: `cargo check --lib`

- [ ] **Step 4: Commit**

```bash
git add api/src/application/agent/
git commit -m "feat(application): create ChatService structure

- Add ChatService with basic structure
- Define ChatRequest, ChatEvent, ChatError types
- Setup for Phase 2 implementation"
```

---

### Task 7: Create SessionService and Repository

**Files:**
- Create: `api/src/application/agent/session_service.rs`
- Create: `api/src/infrastructure/persistence/repositories/chat_session_repository.rs`

- [ ] **Step 1: Create SessionRepository trait in session_service.rs**

```rust
use async_trait::async_trait;
use chrono::{DateTime, Utc};

#[async_trait]
pub trait SessionRepository: Send + Sync {
    async fn get_or_create(&self, session_key: &str) -> Result<Session, SessionError>;
    async fn append_message(&self, session_id: &str, role: &str, content: &str) -> Result<(), SessionError>;
    async fn get_history(&self, session_id: &str, limit: usize) -> Result<Vec<ChatMessage>, SessionError>;
}

#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub session_key: String,
    pub workspace_id: String,
    pub agent_id: String,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: i64,
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    pub timestamp: i64,
    pub run_id: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("Database error: {0}")]
    DatabaseError(String),
    #[error("Session not found: {0}")]
    NotFound(String),
}
```

- [ ] **Step 2: Create SQLite implementation**

Create `api/src/infrastructure/persistence/repositories/chat_session_repository.rs`:

```rust
use async_trait::async_trait;
use crate::application::agent::{SessionRepository, Session, ChatMessage, SessionError};
use crate::infrastructure::persistence::database::Database;

pub struct ChatSessionRepository {
    db: Database,
}

impl ChatSessionRepository {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl SessionRepository for ChatSessionRepository {
    async fn get_or_create(&self, session_key: &str) -> Result<Session, SessionError> {
        // Implementation using existing chat_sessions table
        todo!("Implement get_or_create")
    }

    async fn append_message(&self, session_id: &str, role: &str, content: &str) -> Result<(), SessionError> {
        // Implementation using existing chat_messages table
        todo!("Implement append_message")
    }

    async fn get_history(&self, session_id: &str, limit: usize) -> Result<Vec<ChatMessage>, SessionError> {
        // Implementation using existing chat_messages table
        todo!("Implement get_history")
    }
}
```

- [ ] **Step 3: Run cargo check**

Run: `cargo check --lib`

- [ ] **Step 4: Commit**

```bash
git add api/src/application/agent/session_service.rs
git add api/src/infrastructure/persistence/repositories/chat_session_repository.rs
git commit -m "feat(session): add SessionRepository trait and SQLite implementation

- Define SessionRepository trait with get_or_create, append_message, get_history
- Create ChatSessionRepository using existing table structure
- Prepare for chat history persistence"
```

---

## Verification Checkpoints

### Checkpoint 1: Phase 1 Complete

Before moving to Phase 2, verify:

- [ ] All MCP tools have workspace isolation
- [ ] `cargo check --lib` passes with 0 errors
- [ ] `cargo test --lib` passes (existing tests)
- [ ] `cargo clippy --lib` passes with no warnings
- [ ] No references to `ZeroClawAgentClient` or `FallbackAgentClient` remain
- [ ] AppState has single `agent_runtime` field

### Checkpoint 2: Phase 2 Foundation

- [ ] Application layer structure created
- [ ] Domain layer interfaces defined
- [ ] Infrastructure layer reorganized
- [ ] All modules compile independently

---

## Summary

This plan implements the AI Agent Architecture Redesign in two phases:

**Phase 1** focuses on security and cleanup:
1. Complete MCP tool security audits
2. Remove external gateway code
3. Update workspace.rs
4. Consolidate AppState
5. Rename/restructure to AgentRuntime

**Phase 2** implements the DDD layers:
1. Application layer (ChatService, SessionService)
2. Infrastructure layer (repositories, adapters)
3. Domain layer (entities, services)

Each task is bite-sized (2-5 minutes) with clear verification steps.
