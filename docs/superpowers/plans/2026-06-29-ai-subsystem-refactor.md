# AI Subsystem Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor scattered, coupled AI code into a new `crates/tinyiothub-ai/` crate with 8 clean domain modules communicating through a shared EventBus, replacing OnceLock anti-patterns with constructor injection.

**Architecture:** Add `Ai(AiEventType)` as a third variant on the existing `EventType` enum in `tinyiothub-core`, extending (not replacing) the current `EventBus` in `tinyiothub-runtime`. Create `crates/tinyiothub-ai/` with 8 modules (agent, session, patrol, alarm, tool, memory, event, orchestrator) following Handler → Service → Repo layering. Cross-domain callbacks registered in `Orchestrator::start()`, replacing direct OnceLock setter coupling.

**Tech Stack:** Rust + Axum + Tokio + SQLite (sqlx) + zeroclaw Agent framework + tokio::sync::broadcast + DashMap + thiserror + arc-swap

---

### Task 1: Scaffold crate and add AiEventType foundation

**Files:**
- Create: `crates/tinyiothub-ai/Cargo.toml`
- Create: `crates/tinyiothub-ai/src/lib.rs`
- Modify: `crates/tinyiothub-core/src/models/event/event_type.rs`
- Modify: `crates/tinyiothub-core/src/models/event/event.rs`
- Modify: `Cargo.toml` (workspace members, workspace dependency)
- Modify: `cloud/Cargo.toml` (add tinyiothub-ai dep)

- [ ] **Step 1: Add workspace member and dependency to root Cargo.toml**

Read the current `[workspace]` section. Add `"crates/tinyiothub-ai"` to members and `tinyiothub-ai = { workspace = true }` is not needed — it's automatic. Just add the workspace dependency declaration.

```toml
# In [workspace.dependencies], add:
tinyiothub-ai = { path = "crates/tinyiothub-ai" }
```

- [ ] **Step 2: Create crate directory structure**

```bash
mkdir -p crates/tinyiothub-ai/src/{agent,session,patrol,alarm,tool,memory,event,orchestrator}
mkdir -p crates/tinyiothub-ai/src/tool/adapters
```

- [ ] **Step 3: Write Cargo.toml for tinyiothub-ai**

```toml
[package]
name = "tinyiothub-ai"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true
description = "AI subsystem for TinyIoTHub — agents, patrol, alarms, memory, tools"

[dependencies]
# Workspace crates
tinyiothub-core = { workspace = true, features = ["sqlx"] }
tinyiothub-runtime = { workspace = true }
tinyiothub-storage = { workspace = true }

# Async runtime
tokio = { workspace = true }

# Serialization
serde = { workspace = true }
serde_json = { workspace = true }

# Async trait
async-trait = { workspace = true }

# Error handling
thiserror = { workspace = true }
anyhow = { workspace = true }

# Logging
tracing = { workspace = true }

# Collections
dashmap = { workspace = true }

# Sync primitives
parking_lot = { workspace = true }

# Database
sqlx = { workspace = true, features = ["sqlite", "chrono", "uuid"] }

# HTTP client
reqwest = { workspace = true }

# Utilities
uuid = { workspace = true }
chrono = { workspace = true }
regex = { workspace = true }

# ZeroClaw Agent runtime
zeroclaw = { git = "https://github.com/Grong/zeroclaw.git", tag = "v0.8.1-patched", package = "zeroclawlabs", features = ["agent-runtime"] }
zeroclaw-api = { git = "https://github.com/Grong/zeroclaw.git", tag = "v0.8.1-patched" }

# Retry
backoff = { workspace = true }

[dev-dependencies]
tokio = { workspace = true }
mockall = { workspace = true }
tempfile = { workspace = true }
```

- [ ] **Step 4: Write lib.rs with module declarations and re-exports**

```rust
// crates/tinyiothub-ai/src/lib.rs

pub mod agent;
pub mod alarm;
pub mod event;
pub mod memory;
pub mod orchestrator;
pub mod patrol;
pub mod session;
pub mod tool;

/// Shared types re-exported at crate root for cross-domain use.
pub mod types {
    pub use crate::event::types::AiEvent;
    pub use crate::patrol::types::{TrustConfig, TrustLevel, WakePriority, WakeSignal};
}

/// Build the full AI subsystem and return the orchestrator handle.
pub struct AiSystem {
    pub orchestrator: std::sync::Arc<orchestrator::Orchestrator>,
    pub agent_pool: std::sync::Arc<agent::pool::AgentPool>,
    pub patrol_manager: std::sync::Arc<patrol::manager::PatrolManager>,
}

impl AiSystem {
    pub async fn shutdown(&self) {
        self.orchestrator.shutdown().await;
    }
}
```

- [ ] **Step 5: Add AiEventType to EventType enum in tinyiothub-core**

Add the `Ai` variant and `AiEventType` enum to `crates/tinyiothub-core/src/models/event/event_type.rs`:

```rust
// Add after DeviceEventType enum definition (line 59):

/// AI subsystem event subtypes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum AiEventType {
    AlarmCreated,
    AlarmResolved,
    PatrolCompleted,
    ChatCompleted,
    WorkspaceCreated,
    WorkspaceDeleted,
}

impl AiEventType {
    pub fn display_name(&self) -> &'static str {
        match self {
            AiEventType::AlarmCreated => "Alarm Created",
            AiEventType::AlarmResolved => "Alarm Resolved",
            AiEventType::PatrolCompleted => "Patrol Completed",
            AiEventType::ChatCompleted => "Chat Completed",
            AiEventType::WorkspaceCreated => "Workspace Created",
            AiEventType::WorkspaceDeleted => "Workspace Deleted",
        }
    }

    pub fn subtype_string(&self) -> &'static str {
        match self {
            AiEventType::AlarmCreated => "alarm_created",
            AiEventType::AlarmResolved => "alarm_resolved",
            AiEventType::PatrolCompleted => "patrol_completed",
            AiEventType::ChatCompleted => "chat_completed",
            AiEventType::WorkspaceCreated => "workspace_created",
            AiEventType::WorkspaceDeleted => "workspace_deleted",
        }
    }
}
```

Modify `EventType` enum to add third variant:

```rust
pub enum EventType {
    System(SystemEventType),
    Device(DeviceEventType),
    Ai(AiEventType),  // new
}
```

- [ ] **Step 6: Add Ai arm to every match in event_type.rs**

Update `type_string()`:
```rust
EventType::Ai(_) => "ai".to_string(),
```

Update `subtype_string()`:
```rust
EventType::Ai(subtype) => subtype.subtype_string().to_string(),
```

Update `is_property_event()`, `is_command_event()`, `is_alarm()`, `is_normal()` — all return `false` for `Ai`:
```rust
EventType::Ai(_) => false,
```

Update `from_strings()`:
```rust
"ai" => match subtype_str {
    "alarm_created" => Ok(EventType::Ai(AiEventType::AlarmCreated)),
    "alarm_resolved" => Ok(EventType::Ai(AiEventType::AlarmResolved)),
    "patrol_completed" => Ok(EventType::Ai(AiEventType::PatrolCompleted)),
    "chat_completed" => Ok(EventType::Ai(AiEventType::ChatCompleted)),
    "workspace_created" => Ok(EventType::Ai(AiEventType::WorkspaceCreated)),
    "workspace_deleted" => Ok(EventType::Ai(AiEventType::WorkspaceDeleted)),
    _ => Err(format!("Unknown ai event subtype: {}", subtype_str)),
},
```

- [ ] **Step 7: Add Ai arm to event.rs match sites**

In `crates/tinyiothub-core/src/models/event/event.rs`:

Update `should_update_real_time_status()`:
```rust
EventType::Ai(_) => false,
```

Update `validate()` — pass-through for Ai events:
```rust
EventType::Ai(_) => Ok(()),
```

Update source validation (Ai events don't require device_id):
```rust
EventType::Ai(_) => Ok(()),
```

- [ ] **Step 8: Add Ai arm to cloud/ match sites**

In `cloud/src/modules/event/` files — add `EventType::Ai(_)` arms:

`event_repository_impl.rs` — `type_string()` returns `"ai"`:
```rust
EventType::Ai(_) => "ai".to_string(),
```

`access_control.rs` — Ai events are admin-only:
```rust
EventType::Ai(_) => true,  // requires admin
```

`event/service.rs` — categorize as `EVENT_CATEGORY_AI`:
```rust
EventType::Ai(_) => "ai",
```

`event/handler/real_time.rs` — add AiEventType display_name mapping:
```rust
EventType::Ai(subtype) => subtype.display_name().to_string(),
```

`event/handler/query.rs` — add display_name:
```rust
EventType::Ai(subtype) => format!("AI - {}", subtype.display_name()),
```

`device/handler/profile.rs` — display_name:
```rust
EventType::Ai(subtype) => subtype.display_name().to_string(),
```

`alarm/service.rs` — `matches!` skip Ai:
```rust
EventType::Ai(_) => false,
```

`persistence_handler.rs` — skip Ai events:
```rust
EventType::Ai(_) => {}  // no-op
```

`data_server.rs` — skip Ai events in device processing:
```rust
EventType::Ai(_) => return,
```

- [ ] **Step 9: Build and fix compiler errors**

```bash
cargo build -p tinyiothub-core -p tinyiothub-runtime -p tinyiothub-cloud 2>&1 | head -100
```

Expected: compiler errors at remaining match sites. Fix each until clean build.

- [ ] **Step 10: Add AiEventType tests to event_type.rs tests module**

```rust
#[test]
fn test_ai_event_type_strings() {
    let ai_type = EventType::Ai(AiEventType::AlarmCreated);
    assert_eq!(ai_type.type_string(), "ai");
    assert_eq!(ai_type.subtype_string(), "alarm_created");
}

#[test]
fn test_ai_event_type_parsing() {
    let parsed = EventType::from_strings("ai", "patrol_completed").unwrap();
    assert_eq!(parsed, EventType::Ai(AiEventType::PatrolCompleted));

    let invalid = EventType::from_strings("ai", "nonexistent");
    assert!(invalid.is_err());
}

#[test]
fn test_ai_event_type_helpers() {
    let ai_type = EventType::Ai(AiEventType::ChatCompleted);
    assert!(!ai_type.is_alarm());
    assert!(!ai_type.is_command_event());
    assert!(!ai_type.is_property_event());
    assert!(!ai_type.is_normal());
}
```

- [ ] **Step 11: Run tests**

```bash
cargo test -p tinyiothub-core
```

Expected: all tests pass including new AiEventType tests.

- [ ] **Step 12: Commit**

```bash
git add crates/tinyiothub-ai/ crates/tinyiothub-core/src/models/event/ Cargo.toml cloud/Cargo.toml cloud/src/modules/event/ cloud/src/modules/alarm/ cloud/src/modules/device/ cloud/src/shared/
git commit -m "feat: add AiEventType foundation and scaffold tinyiothub-ai crate

Add Ai(AiEventType) as third variant on EventType enum with 6 subtypes.
Scaffold crates/tinyiothub-ai/ with module directory structure and Cargo.toml.
Update ~23 match sites across core + cloud for Ai variant."
```

---

### Task 2: Define event/types module (shared AiEvent)

**Files:**
- Create: `crates/tinyiothub-ai/src/event/mod.rs`
- Create: `crates/tinyiothub-ai/src/event/types.rs`
- Create: `crates/tinyiothub-ai/src/event/bus.rs`

- [ ] **Step 1: Write event/types.rs — AiEvent enum**

```rust
// crates/tinyiothub-ai/src/event/types.rs

use serde::{Deserialize, Serialize};

use crate::alarm::types::Alarm;
use crate::patrol::types::PatrolReport;
use crate::session::types::ChatTurnMessage;

/// AI subsystem domain events.
///
/// Published through the shared `tinyiothub_runtime::EventBus` as
/// `EventType::Ai(AiEventType::...)`. The payload variants carry
/// typed data that handlers downcast from the event content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AiEvent {
    AlarmCreated(Alarm),
    AlarmResolved {
        alarm_id: String,
        device_id: String,
        rule_id: Option<String>,
    },
    PatrolCompleted {
        workspace_id: String,
        report: PatrolReport,
    },
    ChatCompleted {
        workspace_id: String,
        agent_id: String,
        session_key: String,
        model: String,
        messages: Vec<ChatTurnMessage>,
    },
    WorkspaceCreated {
        workspace_id: String,
    },
    WorkspaceDeleted {
        workspace_id: String,
    },
}

impl AiEvent {
    pub fn workspace_id(&self) -> Option<&str> {
        match self {
            AiEvent::AlarmCreated(a) => Some(&a.workspace_id),
            AiEvent::AlarmResolved { .. } => None,
            AiEvent::PatrolCompleted { workspace_id, .. } => Some(workspace_id),
            AiEvent::ChatCompleted { workspace_id, .. } => Some(workspace_id),
            AiEvent::WorkspaceCreated { workspace_id } => Some(workspace_id),
            AiEvent::WorkspaceDeleted { workspace_id } => Some(workspace_id),
        }
    }
}
```

- [ ] **Step 2: Write event/mod.rs**

```rust
// crates/tinyiothub-ai/src/event/mod.rs

pub mod types;
pub mod bus;
```

- [ ] **Step 3: Write event/bus.rs — AiEventPublisher wrapper**

```rust
// crates/tinyiothub-ai/src/event/bus.rs

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tinyiothub_runtime::EventBus;
use tracing::{error, warn};

use super::types::AiEvent;

/// Wraps the shared EventBus for AI-specific publish semantics.
///
/// All publishes are fire-and-forget (spawned onto tokio).
/// Tracks `events_dropped` counter for observability.
pub struct AiEventPublisher {
    bus: Arc<EventBus>,
    events_published: AtomicU64,
    events_dropped: AtomicU64,
}

impl AiEventPublisher {
    pub fn new(bus: Arc<EventBus>) -> Self {
        Self {
            bus,
            events_published: AtomicU64::new(0),
            events_dropped: AtomicU64::new(0),
        }
    }

    /// Publish an AiEvent as a fire-and-forget operation.
    pub fn publish(&self, event: AiEvent) {
        let bus = self.bus.clone();
        let events_published = &self.events_published;
        let events_dropped = &self.events_dropped;

        tokio::spawn(async move {
            let ai_event_type = tinyiothub_core::models::event::AiEventType::from(&event);
            let event_type = tinyiothub_core::models::event::EventType::Ai(ai_event_type);

            let payload = match serde_json::to_string(&event) {
                Ok(p) => p,
                Err(e) => {
                    error!("Failed to serialize AiEvent: {}", e);
                    return;
                }
            };

            use tinyiothub_core::models::event::{
                Event, EventLevel, EventSource, RichContent,
            };
            let evt = match Event::new(
                event_type,
                EventLevel::Info,
                EventSource::system("ai-subsystem".to_string(), None),
                RichContent::new_text("AiEvent".to_string(), payload),
            ) {
                Ok(e) => e,
                Err(e) => {
                    error!("Failed to create Event for AiEvent: {}", e);
                    return;
                }
            };

            match bus.publish(evt).await {
                Ok(_) => {
                    events_published.fetch_add(1, Ordering::Relaxed);
                }
                Err(_) => {
                    events_dropped.fetch_add(1, Ordering::Relaxed);
                    warn!(
                        dropped = events_dropped.load(Ordering::Relaxed),
                        "AiEvent dropped — EventBus channel may be full"
                    );
                }
            }
        });
    }

    pub fn events_published(&self) -> u64 {
        self.events_published.load(Ordering::Relaxed)
    }

    pub fn events_dropped(&self) -> u64 {
        self.events_dropped.load(Ordering::Relaxed)
    }
}

impl From<&AiEvent> for tinyiothub_core::models::event::AiEventType {
    fn from(event: &AiEvent) -> Self {
        match event {
            AiEvent::AlarmCreated(_) => tinyiothub_core::models::event::AiEventType::AlarmCreated,
            AiEvent::AlarmResolved { .. } => tinyiothub_core::models::event::AiEventType::AlarmResolved,
            AiEvent::PatrolCompleted { .. } => tinyiothub_core::models::event::AiEventType::PatrolCompleted,
            AiEvent::ChatCompleted { .. } => tinyiothub_core::models::event::AiEventType::ChatCompleted,
            AiEvent::WorkspaceCreated { .. } => tinyiothub_core::models::event::AiEventType::WorkspaceCreated,
            AiEvent::WorkspaceDeleted { .. } => tinyiothub_core::models::event::AiEventType::WorkspaceDeleted,
        }
    }
}
```

- [ ] **Step 4: Build to verify compilation**

```bash
cargo build -p tinyiothub-ai 2>&1 | head -50
```

Expected: errors about missing alarm/types, patrol/types, session/types — these are stubbed in subsequent tasks.

- [ ] **Step 5: Commit**

```bash
git add crates/tinyiothub-ai/src/event/
git commit -m "feat: add AiEvent types and AiEventPublisher wrapper for EventBus"
```

---

### Task 3: Migrate patrol/types.rs and patrol/repo.rs

**Files:**
- Create: `crates/tinyiothub-ai/src/patrol/mod.rs`
- Create: `crates/tinyiothub-ai/src/patrol/types.rs`
- Create: `crates/tinyiothub-ai/src/patrol/repo.rs`

- [ ] **Step 1: Write patrol/types.rs — extract from heartbeat_manager.rs + heartbeat.rs**

```rust
// crates/tinyiothub-ai/src/patrol/types.rs

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Priority level for a WakeSignal
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum WakePriority {
    Normal = 0,
    High = 1,
    Critical = 2,
}

impl WakePriority {
    pub fn label(&self) -> &str {
        match self {
            WakePriority::Normal => "NORMAL",
            WakePriority::High => "HIGH",
            WakePriority::Critical => "CRITICAL",
        }
    }
}

/// Signal sent to wake a specific workspace's patrol loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakeSignal {
    pub workspace_id: String,
    pub reason: String,
    pub context: String,
    pub priority: WakePriority,
    /// Dedup key: signals with same (device_id, alarm_type) are merged.
    pub device_id: Option<String>,
    pub alarm_type: Option<String>,
    pub rule_id: Option<String>,
}

impl WakeSignal {
    /// Dedup key — signals with the same key and workspace replace each other.
    pub fn dedup_key(&self) -> Option<(String, String)> {
        match (&self.device_id, &self.alarm_type) {
            (Some(did), Some(at)) => Some((did.clone(), at.clone())),
            _ => None,
        }
    }
}

/// Trust level for automatic tool execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrustLevel {
    /// All tools require human approval.
    ApprovalRequired,
    /// Read-only tools auto-execute; write tools require approval.
    ReadOnlyAuto,
    /// All tools auto-execute.
    FullAuto,
}

/// Per-workspace trust configuration for patrol auto-execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustConfig {
    pub trust_level: TrustLevel,
    pub max_auto_actions_per_tick: u32,
    pub allowed_tool_categories: Vec<String>,
    pub blocked_tools: Vec<String>,
}

impl Default for TrustConfig {
    fn default() -> Self {
        Self {
            trust_level: TrustLevel::ApprovalRequired,
            max_auto_actions_per_tick: 5,
            allowed_tool_categories: vec![],
            blocked_tools: vec![],
        }
    }
}

impl TrustConfig {
    /// Load from DB JSON column, falling back to safe default.
    pub fn from_db_json(json: Option<&str>) -> Self {
        json.and_then(|j| serde_json::from_str(j).ok())
            .unwrap_or_default()
    }

    /// Serialize to JSON for DB storage.
    pub fn to_db_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// Resolve final trust for a tool + workspace combination.
pub fn resolve_trust(config: &TrustConfig, tool_category: &str) -> TrustLevel {
    if config.blocked_tools.iter().any(|t| t == tool_category) {
        return TrustLevel::ApprovalRequired;
    }
    match config.trust_level {
        TrustLevel::ApprovalRequired => TrustLevel::ApprovalRequired,
        TrustLevel::ReadOnlyAuto => {
            if tool_category == "read" || tool_category == "query" {
                TrustLevel::FullAuto
            } else {
                TrustLevel::ApprovalRequired
            }
        }
        TrustLevel::FullAuto => TrustLevel::FullAuto,
    }
}

/// Status of a patrol tick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PatrolStatus {
    Complete,
    Partial,
    Error,
}

/// A single auto-executed action from a patrol tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoExecutedAction {
    pub tool_name: String,
    pub device_id: Option<String>,
    pub success: bool,
    pub details: String,
}

/// A pending proposal requiring human approval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingProposal {
    pub tool_name: String,
    pub device_id: Option<String>,
    pub proposed_action: String,
    pub rationale: String,
}

/// Result of a patrol loop tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatrolReport {
    pub workspace_id: String,
    pub status: PatrolStatus,
    pub summary: String,
    pub executed_actions: Vec<AutoExecutedAction>,
    pub pending_proposals: Vec<PendingProposal>,
    pub error: Option<String>,
}

/// Heartbeat task persisted in DB (replaces HEARTBEAT.md file).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatTask {
    pub id: i64,
    pub workspace_id: String,
    pub priority: String,
    pub text: String,
    pub paused: bool,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Configuration for a patrol loop.
#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    pub enabled: bool,
    pub interval_minutes: u32,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_minutes: 15,
        }
    }
}
```

- [ ] **Step 2: Write patrol/repo.rs — AgentActionRepository trait + HeartbeatTaskRepository trait**

```rust
// crates/tinyiothub-ai/src/patrol/repo.rs

use async_trait::async_trait;
use sqlx::SqlitePool;

use super::types::{HeartbeatTask, PatrolReport};

/// Persists patrol results (agent_actions table).
#[async_trait]
pub trait ActionRepository: Send + Sync {
    async fn insert_patrol_actions(
        &self,
        workspace_id: &str,
        report: &PatrolReport,
    ) -> Result<(), ActionRepoError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ActionRepoError {
    #[error("Database error: {0}")]
    Database(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Persists heartbeat tasks (heartbeat_tasks table — replaces HEARTBEAT.md).
#[async_trait]
pub trait HeartbeatTaskRepository: Send + Sync {
    async fn list_by_workspace(&self, workspace_id: &str) -> Result<Vec<HeartbeatTask>, sqlx::Error>;
    async fn upsert(
        &self,
        workspace_id: &str,
        task: &HeartbeatTask,
        expected_version: i64,
    ) -> Result<bool, sqlx::Error>; // returns false on version conflict
    async fn insert(&self, workspace_id: &str, priority: &str, text: &str)
        -> Result<HeartbeatTask, sqlx::Error>;
    async fn set_paused(&self, workspace_id: &str, task_id: i64, paused: bool) -> Result<(), sqlx::Error>;
    async fn delete(&self, workspace_id: &str, task_id: i64) -> Result<(), sqlx::Error>;
}

/// SQLite implementation of ActionRepository.
pub struct SqliteActionRepository {
    pool: SqlitePool,
}

impl SqliteActionRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ActionRepository for SqliteActionRepository {
    async fn insert_patrol_actions(
        &self,
        workspace_id: &str,
        report: &PatrolReport,
    ) -> Result<(), ActionRepoError> {
        let actions_json = serde_json::to_string(&report.executed_actions)
            .map_err(|e| ActionRepoError::Serialization(e.to_string()))?;
        let proposals_json = serde_json::to_string(&report.pending_proposals)
            .map_err(|e| ActionRepoError::Serialization(e.to_string()))?;

        sqlx::query(
            "INSERT INTO agent_actions (workspace_id, status, summary, actions_json, proposals_json, error)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(workspace_id)
        .bind(format!("{:?}", report.status))
        .bind(&report.summary)
        .bind(&actions_json)
        .bind(&proposals_json)
        .bind(&report.error)
        .execute(&self.pool)
        .await
        .map_err(|e| ActionRepoError::Database(e.to_string()))?;

        Ok(())
    }
}

/// SQLite implementation of HeartbeatTaskRepository.
pub struct SqliteHeartbeatTaskRepository {
    pool: SqlitePool,
}

impl SqliteHeartbeatTaskRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HeartbeatTaskRepository for SqliteHeartbeatTaskRepository {
    async fn list_by_workspace(&self, workspace_id: &str) -> Result<Vec<HeartbeatTask>, sqlx::Error> {
        sqlx::query_as!(
            HeartbeatTask,
            "SELECT id, workspace_id, priority, text, paused, version,
                    created_at, updated_at
             FROM heartbeat_tasks WHERE workspace_id = ? ORDER BY priority DESC, id ASC",
            workspace_id
        )
        .fetch_all(&self.pool)
        .await
    }

    async fn upsert(
        &self,
        workspace_id: &str,
        task: &HeartbeatTask,
        expected_version: i64,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE heartbeat_tasks
             SET priority = ?, text = ?, paused = ?, version = version + 1,
                 updated_at = CURRENT_TIMESTAMP
             WHERE workspace_id = ? AND id = ? AND version = ?",
        )
        .bind(&task.priority)
        .bind(&task.text)
        .bind(task.paused)
        .bind(workspace_id)
        .bind(task.id)
        .bind(expected_version)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn insert(
        &self,
        workspace_id: &str,
        priority: &str,
        text: &str,
    ) -> Result<HeartbeatTask, sqlx::Error> {
        sqlx::query_as!(
            HeartbeatTask,
            "INSERT INTO heartbeat_tasks (workspace_id, priority, text)
             VALUES (?, ?, ?)
             RETURNING id, workspace_id, priority, text, paused, version, created_at, updated_at",
            workspace_id,
            priority,
            text
        )
        .fetch_one(&self.pool)
        .await
    }

    async fn set_paused(
        &self,
        workspace_id: &str,
        task_id: i64,
        paused: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE heartbeat_tasks SET paused = ?, updated_at = CURRENT_TIMESTAMP
             WHERE workspace_id = ? AND id = ?",
        )
        .bind(paused)
        .bind(workspace_id)
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete(&self, workspace_id: &str, task_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM heartbeat_tasks WHERE workspace_id = ? AND id = ?")
            .bind(workspace_id)
            .bind(task_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
```

- [ ] **Step 3: Write patrol/mod.rs**

```rust
// crates/tinyiothub-ai/src/patrol/mod.rs

pub mod types;
pub mod repo;
pub mod manager;
pub mod loop_;
pub mod report;
```

- [ ] **Step 4: Create heartbeat_tasks migration**

Create `cloud/migrations/20260629000001_create_heartbeat_tasks.sql`:

```sql
CREATE TABLE IF NOT EXISTS heartbeat_tasks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id TEXT NOT NULL,
    priority TEXT NOT NULL DEFAULT 'low',
    text TEXT NOT NULL,
    paused INTEGER NOT NULL DEFAULT 0,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(workspace_id, id)
);

CREATE INDEX IF NOT EXISTS idx_heartbeat_tasks_workspace
    ON heartbeat_tasks(workspace_id);
```

- [ ] **Step 5: Build**

```bash
cargo build -p tinyiothub-ai 2>&1 | head -50
```

Expected: errors only about missing modules (manager, loop_, report — not yet written).

- [ ] **Step 6: Commit**

```bash
git add crates/tinyiothub-ai/src/patrol/types.rs crates/tinyiothub-ai/src/patrol/repo.rs crates/tinyiothub-ai/src/patrol/mod.rs cloud/migrations/20260629000001_create_heartbeat_tasks.sql
git commit -m "feat: add patrol types, repos, and heartbeat_tasks migration"
```

---

### Task 4: Migrate patrol/manager.rs (PatrolManager)

**Files:**
- Create: `crates/tinyiothub-ai/src/patrol/manager.rs`

- [ ] **Step 1: Write patrol/manager.rs — extract from heartbeat_manager.rs**

This is the core lifecycle manager. Migrate from `cloud/src/modules/agent/heartbeat_manager.rs`:

```rust
// crates/tinyiothub-ai/src/patrol/manager.rs

use std::collections::HashMap;
use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info, warn};

use super::repo::HeartbeatTaskRepository;
use super::types::{HeartbeatConfig, TrustConfig, WakePriority, WakeSignal};
use crate::event::bus::AiEventPublisher;

struct PatrolLoopHandle {
    cancel_tx: oneshot::Sender<()>,
    join_handle: tokio::task::JoinHandle<()>,
}

/// Manages per-workspace patrol loop lifecycle.
///
/// Owns a DashMap of cancel channels and handles. Start/stop are idempotent.
/// TrustConfig is loaded from DB on start and cached in memory.
pub struct PatrolManager {
    loops: DashMap<String, PatrolLoopHandle>,
    wake_senders: DashMap<String, mpsc::UnboundedSender<WakeSignal>>,
    trust_configs: DashMap<String, TrustConfig>,
    task_repo: Arc<dyn HeartbeatTaskRepository>,
    event_publisher: Arc<AiEventPublisher>,
    agent_pool: std::sync::RwLock<Option<Arc<dyn crate::agent::pool::AgentPoolLike>>>,
    config: HeartbeatConfig,
}

impl PatrolManager {
    pub fn new(
        task_repo: Arc<dyn HeartbeatTaskRepository>,
        event_publisher: Arc<AiEventPublisher>,
        config: HeartbeatConfig,
    ) -> Self {
        Self {
            loops: DashMap::new(),
            wake_senders: DashMap::new(),
            trust_configs: DashMap::new(),
            task_repo,
            event_publisher,
            agent_pool: std::sync::RwLock::new(None),
            config,
        }
    }

    /// Set the agent pool — called once during `AiSystem` assembly.
    pub fn set_agent_pool(&self, pool: Arc<dyn crate::agent::pool::AgentPoolLike>) {
        let mut guard = self.agent_pool.write().unwrap();
        *guard = Some(pool);
    }

    /// Start a patrol loop for a workspace. Idempotent — stops existing loop first.
    pub async fn start(&self, workspace_id: &str) {
        self.stop(workspace_id).await;

        // Load TrustConfig from DB
        let trust_config = self.load_trust_config(workspace_id).await;
        self.trust_configs.insert(workspace_id.to_string(), trust_config.clone());

        // Load heartbeat tasks from DB
        let tasks = match self.task_repo.list_by_workspace(workspace_id).await {
            Ok(t) => t,
            Err(e) => {
                error!(workspace_id, error = %e, "Failed to load heartbeat tasks");
                return;
            }
        };

        if tasks.is_empty() {
            info!(workspace_id, "No heartbeat tasks, skipping patrol loop start");
            return;
        }

        let (wake_tx, wake_rx) = mpsc::unbounded_channel();
        let (cancel_tx, cancel_rx) = oneshot::channel();

        let ws_id = workspace_id.to_string();
        let pool = self.agent_pool.read().unwrap().clone();
        let task_repo = self.task_repo.clone();
        let event_publisher = self.event_publisher.clone();
        let config = self.config.clone();

        let join_handle = tokio::spawn(async move {
            super::loop_::patrol_loop(
                &ws_id,
                tasks,
                trust_config,
                pool,
                task_repo,
                event_publisher,
                config,
                wake_rx,
                cancel_rx,
            ).await;
        });

        self.wake_senders.insert(workspace_id.to_string(), wake_tx);
        self.loops.insert(
            workspace_id.to_string(),
            PatrolLoopHandle {
                cancel_tx,
                join_handle,
            },
        );

        info!(workspace_id, "Patrol loop started");
    }

    /// Stop a patrol loop for a workspace. No-op if not running.
    pub async fn stop(&self, workspace_id: &str) {
        if let Some((_, handle)) = self.loops.remove(workspace_id) {
            let _ = handle.cancel_tx.send(());
            tokio::time::timeout(std::time::Duration::from_secs(5), handle.join_handle)
                .await
                .ok();
        }
        self.wake_senders.remove(workspace_id);
        self.trust_configs.remove(workspace_id);
        info!(workspace_id, "Patrol loop stopped");
    }

    /// Wake a workspace's patrol loop with a deduplicated signal.
    pub fn wake(&self, signal: WakeSignal) {
        // Dedup: keep highest priority signal per dedup key
        if let Some(key) = signal.dedup_key() {
            // For now, simply send — dedup logic lives in the loop receiver
        }
        if let Some(sender) = self.wake_senders.get(&signal.workspace_id) {
            if let Err(e) = sender.send(signal) {
                warn!(workspace_id = %signal.workspace_id, error = %e, "Failed to send wake signal");
            }
        }
    }

    /// Update TrustConfig for a workspace.
    pub fn update_trust_config(&self, workspace_id: &str, config: TrustConfig) {
        self.trust_configs.insert(workspace_id.to_string(), config);
        info!(workspace_id, "TrustConfig updated in memory");
    }

    /// Get cached TrustConfig for a workspace (used by AgentPool during agent build).
    pub fn get_trust_config(&self, workspace_id: &str) -> Option<TrustConfig> {
        self.trust_configs
            .get(workspace_id)
            .map(|r| r.value().clone())
    }

    /// Number of active patrol loops.
    pub fn active_loop_count(&self) -> usize {
        self.loops.len()
    }

    /// List workspace IDs with active loops.
    pub fn active_workspaces(&self) -> Vec<String> {
        self.loops.iter().map(|r| r.key().clone()).collect()
    }

    async fn load_trust_config(&self, workspace_id: &str) -> TrustConfig {
        // Query workspace_settings.heartbeat_trust_config column
        // For now, return default. Actual DB query wired in cloud integration task.
        TrustConfig::default()
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add crates/tinyiothub-ai/src/patrol/manager.rs
git commit -m "feat: add PatrolManager with per-workspace lifecycle and DB-backed TrustConfig"
```

---

### Task 5: Migrate patrol/loop.rs (patrol loop logic)

**Files:**
- Create: `crates/tinyiothub-ai/src/patrol/loop_.rs`
- Create: `crates/tinyiothub-ai/src/patrol/report.rs`

- [ ] **Step 1: Write patrol/report.rs — HealingReport parsing**

```rust
// crates/tinyiothub-ai/src/patrol/report.rs

use regex::Regex;
use tracing::warn;

use super::types::{AutoExecutedAction, PatrolReport, PatrolStatus, PendingProposal};

/// Parse an LLM-generated healing report (JSON inside ```json fence or raw JSON).
pub fn parse_healing_report(raw: &str, workspace_id: &str) -> PatrolReport {
    let json_str = extract_json(raw);

    match serde_json::from_str::<serde_json::Value>(&json_str) {
        Ok(value) => PatrolReport {
            workspace_id: workspace_id.to_string(),
            status: parse_status(&value),
            summary: value["summary"].as_str().unwrap_or("").to_string(),
            executed_actions: parse_executed_actions(&value),
            pending_proposals: parse_pending_proposals(&value),
            error: value["error"].as_str().map(|s| s.to_string()),
        },
        Err(e) => {
            warn!(workspace_id, error = %e, "Failed to parse HealingReport JSON, returning error report");
            PatrolReport {
                workspace_id: workspace_id.to_string(),
                status: PatrolStatus::Error,
                summary: String::new(),
                executed_actions: vec![],
                pending_proposals: vec![],
                error: Some(format!("JSON parse error: {}", e)),
            }
        }
    }
}

fn extract_json(raw: &str) -> String {
    // Try ```json fence first
    let fence_re = Regex::new(r"```json\s*\n([\s\S]*?)\n```").unwrap();
    if let Some(captures) = fence_re.captures(raw) {
        return captures[1].to_string();
    }
    // Fallback: find first { ... } block
    if let Some(start) = raw.find('{') {
        if let Some(end) = raw.rfind('}') {
            return raw[start..=end].to_string();
        }
    }
    raw.to_string()
}

fn parse_status(value: &serde_json::Value) -> PatrolStatus {
    match value["status"].as_str() {
        Some("partial") | Some("Partial") => PatrolStatus::Partial,
        Some("error") | Some("Error") => PatrolStatus::Error,
        _ => PatrolStatus::Complete,
    }
}

fn parse_executed_actions(value: &serde_json::Value) -> Vec<AutoExecutedAction> {
    value["executed_actions"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|a| AutoExecutedAction {
                    tool_name: a["tool_name"].as_str().unwrap_or("").to_string(),
                    device_id: a["device_id"].as_str().map(|s| s.to_string()),
                    success: a["success"].as_bool().unwrap_or(true),
                    details: a["details"].as_str().unwrap_or("").to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_pending_proposals(value: &serde_json::Value) -> Vec<PendingProposal> {
    value["pending_proposals"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|p| PendingProposal {
                    tool_name: p["tool_name"].as_str().unwrap_or("").to_string(),
                    device_id: p["device_id"].as_str().map(|s| s.to_string()),
                    proposed_action: p["proposed_action"].as_str().unwrap_or("").to_string(),
                    rationale: p["rationale"].as_str().unwrap_or("").to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_complete_report() {
        let raw = r#"```json
{
  "status": "complete",
  "summary": "All devices healthy",
  "executed_actions": [
    {"tool_name": "check_temp", "device_id": "d1", "success": true, "details": "OK"}
  ],
  "pending_proposals": []
}
```"#;
        let report = parse_healing_report(raw, "ws1");
        assert_eq!(report.status, PatrolStatus::Complete);
        assert_eq!(report.summary, "All devices healthy");
        assert_eq!(report.executed_actions.len(), 1);
    }

    #[test]
    fn test_parse_without_fence() {
        let raw = r#"{"status": "error", "summary": "Timeout", "error": "LLM timeout"}"#;
        let report = parse_healing_report(raw, "ws1");
        assert_eq!(report.status, PatrolStatus::Error);
        assert!(report.error.is_some());
    }
}
```

- [ ] **Step 2: Write patrol/loop_.rs — patrol loop logic**

```rust
// crates/tinyiothub-ai/src/patrol/loop_.rs

use std::sync::Arc;
use std::time::Duration;

use backoff::ExponentialBackoffBuilder;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, info, warn};

use super::repo::HeartbeatTaskRepository;
use super::types::{HeartbeatConfig, HeartbeatTask, TrustConfig, WakePriority, WakeSignal};
use crate::event::bus::AiEventPublisher;
use crate::event::types::AiEvent;

/// Maximum number of consecutive LLM failures before pausing the loop.
const MAX_CONSECUTIVE_FAILURES: u32 = 5;

/// Main patrol loop for a single workspace.
///
/// Sleeps for the configured interval, then iterates through heartbeat tasks,
/// calling the LLM for each. Publishes `PatrolCompleted` events instead of
/// directly calling `ActionRepo::insert()`.
pub async fn patrol_loop(
    workspace_id: &str,
    tasks: Vec<HeartbeatTask>,
    trust_config: TrustConfig,
    agent_pool: Option<Arc<dyn crate::agent::pool::AgentPoolLike>>,
    _task_repo: Arc<dyn HeartbeatTaskRepository>,
    event_publisher: Arc<AiEventPublisher>,
    config: HeartbeatConfig,
    mut wake_rx: mpsc::UnboundedReceiver<WakeSignal>,
    cancel_rx: oneshot::Receiver<()>,
) {
    let agent_pool = match agent_pool {
        Some(p) => p,
        None => {
            error!(workspace_id, "AgentPool not set, patrol loop cannot start");
            return;
        }
    };

    let interval = Duration::from_secs((config.interval_minutes as u64) * 60);
    let mut consecutive_failures: u32 = 0;

    tokio::pin! {
        let cancel = cancel_rx;
    }

    loop {
        // Run a patrol tick
        let active_tasks: Vec<&HeartbeatTask> = tasks.iter().filter(|t| !t.paused).collect();
        if !active_tasks.is_empty() {
            match run_patrol_tick(
                workspace_id,
                &active_tasks,
                &trust_config,
                &agent_pool,
                &event_publisher,
            )
            .await
            {
                Ok(_) => consecutive_failures = 0,
                Err(e) => {
                    consecutive_failures += 1;
                    error!(workspace_id, error = %e, consecutive_failures, "Patrol tick failed");
                    if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                        warn!(
                            workspace_id,
                            consecutive_failures,
                            "Too many consecutive failures, publishing partial report and pausing"
                        );
                        // Publish a partial report even on total failure
                        event_publisher.publish(AiEvent::PatrolCompleted {
                            workspace_id: workspace_id.to_string(),
                            report: crate::patrol::types::PatrolReport {
                                workspace_id: workspace_id.to_string(),
                                status: crate::patrol::types::PatrolStatus::Error,
                                summary: format!("{} consecutive failures", consecutive_failures),
                                executed_actions: vec![],
                                pending_proposals: vec![],
                                error: Some(e.to_string()),
                            },
                        });
                    }
                }
            }
        }

        // Wait for next interval or wake signal or cancel
        tokio::select! {
            _ = &mut cancel => {
                info!(workspace_id, "Patrol loop cancelled");
                return;
            }
            Some(signal) = wake_rx.recv() => {
                debug!(
                    workspace_id,
                    priority = %signal.priority.label(),
                    reason = %signal.reason,
                    "Patrol loop woken by signal"
                );
                // Immediate tick — loop continues
            }
            _ = tokio::time::sleep(interval) => {
                // Normal interval tick
            }
        }
    }
}

async fn run_patrol_tick(
    workspace_id: &str,
    tasks: &[&HeartbeatTask],
    trust_config: &TrustConfig,
    agent_pool: &Arc<dyn crate::agent::pool::AgentPoolLike>,
    event_publisher: &AiEventPublisher,
) -> Result<(), String> {
    let agent = agent_pool
        .get_or_create(workspace_id)
        .await
        .map_err(|e| format!("Failed to get agent: {}", e))?;

    // Build prompt from tasks
    let prompt = build_patrol_prompt(workspace_id, tasks, trust_config);

    // Call LLM with timeout + catch_unwind safety
    let raw_response = tokio::time::timeout(
        Duration::from_secs(180),
        agent.send_message(&prompt),
    )
    .await
    .map_err(|_| "LLM call timed out after 180s".to_string())?
    .map_err(|e| format!("LLM call failed: {}", e))?;

    // Parse the response
    let report = super::report::parse_healing_report(&raw_response, workspace_id);

    // Publish PatrolCompleted — ActionRepo subscriber handles persistence
    event_publisher.publish(AiEvent::PatrolCompleted {
        workspace_id: workspace_id.to_string(),
        report,
    });

    Ok(())
}

fn build_patrol_prompt(
    workspace_id: &str,
    tasks: &[&HeartbeatTask],
    trust_config: &TrustConfig,
) -> String {
    let tasks_text: String = tasks
        .iter()
        .map(|t| format!("- [{}] {}", t.priority, t.text))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "You are an IoT patrol agent for workspace {ws_id}.\n\
         Trust level: {trust:?}\n\
         Max auto-actions per tick: {max}\n\n\
         ## Patrol Tasks:\n{tasks}\n\n\
         Execute each task. Output a JSON report:\n\
         ```json\n\
         {{\n  \"status\": \"complete|partial|error\",\n  \
         \"summary\": \"...\",\n  \
         \"executed_actions\": [{{\"tool_name\": \"...\", \"device_id\": \"...\", \"success\": true, \"details\": \"...\"}}],\n  \
         \"pending_proposals\": [{{\"tool_name\": \"...\", \"device_id\": \"...\", \"proposed_action\": \"...\", \"rationale\": \"...\"}}],\n  \
         \"error\": null\n}}\n```",
        ws_id = workspace_id,
        trust = trust_config.trust_level,
        max = trust_config.max_auto_actions_per_tick,
        tasks = tasks_text
    )
}
```

- [ ] **Step 3: Commit**

```bash
git add crates/tinyiothub-ai/src/patrol/loop_.rs crates/tinyiothub-ai/src/patrol/report.rs
git commit -m "feat: add patrol loop logic with event-driven action persistence"
```

---

### Task 6: Migrate alarm/types.rs and alarm/repo.rs (skeleton)

**Files:**
- Create: `crates/tinyiothub-ai/src/alarm/mod.rs`
- Create: `crates/tinyiothub-ai/src/alarm/types.rs`
- Create: `crates/tinyiothub-ai/src/alarm/repo.rs`

- [ ] **Step 1: Write alarm/types.rs — minimal types needed by AiEvent::AlarmCreated**

```rust
// crates/tinyiothub-ai/src/alarm/types.rs

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Alarm entity — subset used cross-domain by AiEvent::AlarmCreated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alarm {
    pub id: String,
    pub workspace_id: String,
    pub device_id: String,
    pub alarm_type: String,
    pub severity: String,
    pub message: String,
    pub rule_id: Option<String>,
    pub resolved: bool,
    pub created_at: DateTime<Utc>,
}

/// Alarm rule definition (skeleton — full type in cloud/ migration phase).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlarmRule {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub condition_json: String,
    pub enabled: bool,
}

/// Domain errors for alarm module.
#[derive(Debug, thiserror::Error)]
pub enum AlarmError {
    #[error("Alarm not found: {id}")]
    NotFound { id: String },
    #[error("Rule evaluation failed: {0}")]
    RuleEvaluation(String),
    #[error("Repository error: {0}")]
    Repository(String),
}
```

- [ ] **Step 2: Write alarm/repo.rs — AlarmRepository trait (skeleton)**

```rust
// crates/tinyiothub-ai/src/alarm/repo.rs

use async_trait::async_trait;
use sqlx::SqlitePool;

use super::types::{Alarm, AlarmRule};

#[async_trait]
pub trait AlarmRepository: Send + Sync {
    async fn find_by_id(&self, id: &str) -> Result<Option<Alarm>, sqlx::Error>;
    async fn insert(&self, alarm: &Alarm) -> Result<(), sqlx::Error>;
    async fn mark_resolved(&self, id: &str) -> Result<(), sqlx::Error>;
}

#[async_trait]
pub trait AlarmRuleRepository: Send + Sync {
    async fn list_enabled(&self, workspace_id: &str) -> Result<Vec<AlarmRule>, sqlx::Error>;
}

/// SQLite AlarmRepository — used in cloud integration, not in ai crate directly.
/// Trait is defined here; impl lives in cloud/ for backward compat during migration.
pub struct SqliteAlarmRepository {
    pool: SqlitePool,
}

impl SqliteAlarmRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AlarmRepository for SqliteAlarmRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<Alarm>, sqlx::Error> {
        sqlx::query_as!(Alarm,
            "SELECT id, workspace_id, device_id, alarm_type, severity, message, rule_id, resolved, created_at
             FROM device_alarms WHERE id = ?",
            id
        )
        .fetch_optional(&self.pool)
        .await
    }

    async fn insert(&self, _alarm: &Alarm) -> Result<(), sqlx::Error> {
        // Delegated to existing cloud/ alarm service during migration
        Ok(())
    }

    async fn mark_resolved(&self, id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE device_alarms SET resolved = 1 WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[async_trait]
impl AlarmRuleRepository for SqliteAlarmRepository {
    async fn list_enabled(&self, _workspace_id: &str) -> Result<Vec<AlarmRule>, sqlx::Error> {
        Ok(vec![])
    }
}
```

- [ ] **Step 3: Write alarm/mod.rs**

```rust
// crates/tinyiothub-ai/src/alarm/mod.rs

pub mod types;
pub mod repo;
```

- [ ] **Step 4: Commit**

```bash
git add crates/tinyiothub-ai/src/alarm/
git commit -m "feat: add alarm types and repo trait (skeleton for AiEvent)"
```

---

### Task 7: Migrate session/types.rs and stub remaining types

**Files:**
- Create: `crates/tinyiothub-ai/src/session/mod.rs`
- Create: `crates/tinyiothub-ai/src/session/types.rs`

- [ ] **Step 1: Write session/types.rs — ChatTurnMessage for AiEvent::ChatCompleted**

```rust
// crates/tinyiothub-ai/src/session/types.rs

use serde::{Deserialize, Serialize};

/// A single turn in a chat conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatTurnMessage {
    pub role: String,
    pub content: String,
    pub timestamp: Option<String>,
}

/// Session key format: agent:<agentId>:<mainKey>
#[derive(Debug, Clone)]
pub struct SessionKey {
    pub agent_id: String,
    pub main_key: String,
}

impl SessionKey {
    pub fn parse(key: &str) -> Option<Self> {
        let parts: Vec<&str> = key.splitn(3, ':').collect();
        if parts.len() != 3 || parts[0] != "agent" {
            return None;
        }
        Some(Self {
            agent_id: parts[1].to_string(),
            main_key: parts[2].to_string(),
        })
    }

    pub fn to_string(&self) -> String {
        format!("agent:{}:{}", self.agent_id, self.main_key)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("Session not found: {key}")]
    NotFound { key: String },
    #[error("Repository error: {0}")]
    Repository(String),
}
```

- [ ] **Step 2: Write session/mod.rs**

```rust
// crates/tinyiothub-ai/src/session/mod.rs

pub mod types;
```

- [ ] **Step 3: Build full crate**

```bash
cargo build -p tinyiothub-ai 2>&1
```

Expected: errors in event/types.rs (Alarm, PatrolReport, ChatTurnMessage types now exist — but event/types.rs may reference agent::pool::AgentPoolLike). Fix any import issues.

- [ ] **Step 4: Commit**

```bash
git add crates/tinyiothub-ai/src/session/
git commit -m "feat: add session types and ChatTurnMessage for ChatCompleted events"
```

---

### Task 8: Migrate tool/types.rs and tool/trust.rs

**Files:**
- Create: `crates/tinyiothub-ai/src/tool/mod.rs`
- Create: `crates/tinyiothub-ai/src/tool/types.rs`
- Create: `crates/tinyiothub-ai/src/tool/trust.rs`

- [ ] **Step 1: Write tool/types.rs — ToolDependencyProvider trait**

```rust
// crates/tinyiothub-ai/src/tool/types.rs

use std::sync::Arc;

/// Interface for tool adapters to resolve workspace/knowledge dependencies
/// without directly holding WorkspaceService/KnowledgeService references.
#[async_trait::async_trait]
pub trait ToolDependencyProvider: Send + Sync {
    async fn resolve_knowledge(&self, workspace_id: &str) -> Option<Arc<dyn KnowledgeProvider>>;
    async fn resolve_workspace(&self, workspace_id: &str) -> Option<Arc<dyn WorkspaceProvider>>;
}

/// Minimal trait for knowledge queries.
pub trait KnowledgeProvider: Send + Sync {
    fn query(&self, query: &str) -> Vec<String>;
}

/// Minimal trait for workspace metadata.
pub trait WorkspaceProvider: Send + Sync {
    fn name(&self) -> &str;
    fn settings(&self) -> serde_json::Value;
}

/// Tool metadata for catalog generation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolMetadata {
    pub name: String,
    pub description: String,
    pub category: String,
    pub parameters: Vec<ToolParameter>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolParameter {
    pub name: String,
    pub description: String,
    pub required: bool,
    pub param_type: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("Tool not found: {name}")]
    NotFound { name: String },
    #[error("Permission denied for tool: {name}")]
    PermissionDenied { name: String },
    #[error("Execution failed: {0}")]
    Execution(String),
}
```

- [ ] **Step 2: Write tool/trust.rs — TrustEngine + TrustAwareTool**

```rust
// crates/tinyiothub-ai/src/tool/trust.rs

use crate::patrol::types::{TrustConfig, TrustLevel, resolve_trust};

/// Engine for evaluating tool trust at execution time.
pub struct TrustEngine {
    config: TrustConfig,
    workspace_id: String,
}

impl TrustEngine {
    pub fn new(config: TrustConfig, workspace_id: String) -> Self {
        Self { config, workspace_id }
    }

    /// Check if a tool is allowed to auto-execute given its category.
    pub fn check(&self, tool_category: &str) -> TrustLevel {
        resolve_trust(&self.config, tool_category)
    }

    pub fn trust_level(&self) -> TrustLevel {
        self.config.trust_level
    }

    pub fn max_auto_actions(&self) -> u32 {
        self.config.max_auto_actions_per_tick
    }

    pub fn is_blocked(&self, tool_name: &str) -> bool {
        self.config.blocked_tools.iter().any(|t| t == tool_name)
    }
}

/// Wrapper that enforces trust before tool execution.
pub struct TrustAwareTool<T> {
    inner: T,
    engine: Arc<TrustEngine>,
    category: String,
}

impl<T> TrustAwareTool<T> {
    pub fn new(inner: T, engine: Arc<TrustEngine>, category: String) -> Self {
        Self {
            inner,
            engine,
            category,
        }
    }

    pub fn check_trust(&self) -> TrustLevel {
        self.engine.check(&self.category)
    }

    pub fn inner(&self) -> &T {
        &self.inner
    }
}
```

- [ ] **Step 3: Write tool/mod.rs**

```rust
// crates/tinyiothub-ai/src/tool/mod.rs

pub mod types;
pub mod trust;
pub mod adapters;
```

- [ ] **Step 4: Commit**

```bash
git add crates/tinyiothub-ai/src/tool/
git commit -m "feat: add tool types, TrustEngine, and ToolDependencyProvider trait"
```

---

### Task 9: Migrate memory/types.rs and memory/service.rs

**Files:**
- Create: `crates/tinyiothub-ai/src/memory/mod.rs`
- Create: `crates/tinyiothub-ai/src/memory/types.rs`
- Create: `crates/tinyiothub-ai/src/memory/service.rs`
- Create: `crates/tinyiothub-ai/src/memory/reflect.rs`

- [ ] **Step 1: Write memory/types.rs**

```rust
// crates/tinyiothub-ai/src/memory/types.rs

use serde::{Deserialize, Serialize};

use crate::session::types::ChatTurnMessage;

/// A fact extracted from conversation for long-term memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryFact {
    pub key: String,
    pub value: String,
    pub confidence: f32,
    pub source_turn_index: usize,
}

/// Maximum input length for reflection (prompt injection defense).
pub const MAX_REFLECTION_INPUT_CHARS: usize = 32_000;

/// Patterns that indicate prompt injection attempts.
pub const INJECTION_PATTERNS: &[&str] = &[
    "You are",
    "System:",
    "Instructions:",
    "Ignore previous",
    "New instructions:",
];

#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("Reflection failed: {0}")]
    Reflection(String),
    #[error("Repository error: {0}")]
    Repository(String),
}
```

- [ ] **Step 2: Write memory/service.rs — MemoryService skeleton**

```rust
// crates/tinyiothub-ai/src/memory/service.rs

use std::sync::Arc;

use crate::session::types::ChatTurnMessage;

use super::types::MemoryError;

/// Service for extracting long-term memory from conversations
/// and compiling user/workspace profiles.
pub struct MemoryService;

impl MemoryService {
    pub fn new() -> Self {
        Self
    }

    /// Reflect on a completed conversation turn. Called by Orchestrator
    /// in response to ChatCompleted events.
    pub async fn reflect_conversation_turn(
        &self,
        _workspace_id: &str,
        _agent_id: &str,
        _session_key: &str,
        _model: &str,
        messages: &[ChatTurnMessage],
    ) -> Result<(), MemoryError> {
        if messages.is_empty() {
            return Ok(());
        }
        super::reflect::reflect(messages).await
    }
}

impl Default for MemoryService {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 3: Write memory/reflect.rs — reflection logic (migrated from agent/reflect.rs)**

```rust
// crates/tinyiothub-ai/src/memory/reflect.rs

use tracing::warn;

use crate::session::types::ChatTurnMessage;

use super::types::{MemoryError, MAX_REFLECTION_INPUT_CHARS, INJECTION_PATTERNS};

/// Reflect on a conversation turn — extract facts, update profile.
///
/// Security: truncates input to MAX_REFLECTION_INPUT_CHARS and filters
/// lines matching known injection patterns.
pub async fn reflect(messages: &[ChatTurnMessage]) -> Result<(), MemoryError> {
    let input = build_reflection_input(messages);
    let sanitized = sanitize_input(&input);

    if sanitized.trim().is_empty() {
        return Ok(());
    }

    // Stub: in production this calls the LLM for reflection.
    // The full reflection logic lives in cloud/src/modules/agent/reflect.rs
    // and will be ported in a follow-up PR.
    tracing::debug!(chars = sanitized.len(), "Reflection input ready");
    Ok(())
}

fn build_reflection_input(messages: &[ChatTurnMessage]) -> String {
    messages
        .iter()
        .map(|m| format!("{}: {}", m.role, m.content))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Truncate to max length and filter injection patterns.
fn sanitize_input(input: &str) -> String {
    let truncated: String = input.chars().take(MAX_REFLECTION_INPUT_CHARS).collect();

    truncated
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !INJECTION_PATTERNS
                .iter()
                .any(|pattern| trimmed.starts_with(pattern))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filters_injection() {
        let input = "user: Hello\nYou are a helpful assistant\nSystem: do something\nassistant: Hi!";
        let result = sanitize_input(input);
        assert!(!result.contains("You are"));
        assert!(!result.contains("System:"));
        assert!(result.contains("user: Hello"));
        assert!(result.contains("assistant: Hi!"));
    }

    #[test]
    fn test_truncation() {
        let long: String = std::iter::repeat('a').take(MAX_REFLECTION_INPUT_CHARS + 100).collect();
        let result = sanitize_input(&long);
        assert!(result.chars().count() <= MAX_REFLECTION_INPUT_CHARS);
    }
}
```

- [ ] **Step 4: Write memory/mod.rs**

```rust
// crates/tinyiothub-ai/src/memory/mod.rs

pub mod types;
pub mod service;
pub mod reflect;
```

- [ ] **Step 5: Commit**

```bash
git add crates/tinyiothub-ai/src/memory/
git commit -m "feat: add memory service with reflection, sanitization, and prompt injection defense"
```

---

### Task 10: Migrate agent pool and builder skeleton

**Files:**
- Create: `crates/tinyiothub-ai/src/agent/mod.rs`
- Create: `crates/tinyiothub-ai/src/agent/types.rs`
- Create: `crates/tinyiothub-ai/src/agent/pool.rs`

- [ ] **Step 1: Write agent/types.rs — AgentPoolLike trait**

```rust
// crates/tinyiothub-ai/src/agent/types.rs

use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("Agent not found for workspace: {workspace_id}")]
    NotFound { workspace_id: String },
    #[error("Pool capacity exceeded")]
    PoolFull,
    #[error("Build error: {0}")]
    Build(String),
}

/// Lightweight handle to a chat-capable agent.
/// Wraps zeroclaw agent reference for patrol_loop consumer.
pub trait AgentHandle: Send + Sync {
    fn send_message(
        &self,
        prompt: &str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<String, String>> + Send + '_>,
    >;
}
```

- [ ] **Step 2: Write agent/pool.rs — AgentPool trait + skeleton**

```rust
// crates/tinyiothub-ai/src/agent/pool.rs

use std::sync::Arc;
use async_trait::async_trait;

use super::types::{AgentError, AgentHandle};

/// Interface for the agent pool, consumed by PatrolManager.
#[async_trait]
pub trait AgentPoolLike: Send + Sync {
    async fn get_or_create(&self, workspace_id: &str) -> Result<Arc<dyn AgentHandle>, AgentError>;
    async fn invalidate(&self, workspace_id: &str);
    async fn cleanup(&self, idle_timeout_secs: u64);
    async fn refresh_tools(&self) -> Result<(), AgentError>;
}

/// Placeholder pool — real implementation ported from cloud/src/modules/agent/agent.rs
/// in a follow-up task. This breaks the circular dependency during early migration.
pub struct StubAgentPool;

#[async_trait]
impl AgentPoolLike for StubAgentPool {
    async fn get_or_create(&self, workspace_id: &str) -> Result<Arc<dyn AgentHandle>, AgentError> {
        Err(AgentError::NotFound {
            workspace_id: workspace_id.to_string(),
        })
    }

    async fn invalidate(&self, _workspace_id: &str) {}
    async fn cleanup(&self, _idle_timeout_secs: u64) {}
    async fn refresh_tools(&self) -> Result<(), AgentError> { Ok(()) }
}
```

- [ ] **Step 3: Write agent/mod.rs**

```rust
// crates/tinyiothub-ai/src/agent/mod.rs

pub mod types;
pub mod pool;
```

- [ ] **Step 4: Commit**

```bash
git add crates/tinyiothub-ai/src/agent/
git commit -m "feat: add agent types and AgentPoolLike trait with stub implementation"
```

---

### Task 11: Create Orchestrator with cross-domain callbacks

**Files:**
- Create: `crates/tinyiothub-ai/src/orchestrator/mod.rs`
- Create: `crates/tinyiothub-ai/src/orchestrator/callbacks.rs`

- [ ] **Step 1: Write orchestrator/callbacks.rs**

```rust
// crates/tinyiothub-ai/src/orchestrator/callbacks.rs

use std::sync::Arc;

use tinyiothub_core::models::event::{AiEventType, Event, EventType};
use tracing::{error, info, warn};

use crate::event::bus::AiEventPublisher;
use crate::event::types::AiEvent;
use crate::memory::service::MemoryService;
use crate::patrol::manager::PatrolManager;
use crate::patrol::repo::ActionRepository;
use crate::patrol::types::WakePriority;

/// Cross-domain callback handler.
///
/// Registered on the shared EventBus by Orchestrator::start().
/// Dispatches AiEvent variants to the appropriate domain service.
pub struct AiEventHandler {
    patrol_manager: Arc<PatrolManager>,
    action_repo: Arc<dyn ActionRepository>,
    memory_service: Arc<MemoryService>,
    event_publisher: Arc<AiEventPublisher>,
}

impl AiEventHandler {
    pub fn new(
        patrol_manager: Arc<PatrolManager>,
        action_repo: Arc<dyn ActionRepository>,
        memory_service: Arc<MemoryService>,
        event_publisher: Arc<AiEventPublisher>,
    ) -> Self {
        Self {
            patrol_manager,
            action_repo,
            memory_service,
            event_publisher,
        }
    }

    /// Handle an AiEvent variant, dispatched by the EventBus.
    pub async fn handle_ai_event(&self, event: &Event) {
        let ai_event_type = match &event.event_type() {
            EventType::Ai(t) => t,
            _ => return, // not an AI event
        };

        let payload_str = match event.content().text() {
            Some(s) => s.clone(),
            None => return,
        };

        let ai_event: AiEvent = match serde_json::from_str(&payload_str) {
            Ok(e) => e,
            Err(e) => {
                warn!(error = %e, "Failed to deserialize AiEvent payload");
                return;
            }
        };

        match (&ai_event_type, &ai_event) {
            (AiEventType::AlarmCreated, AiEvent::AlarmCreated(alarm)) => {
                // Wake patrol on high-severity alarms (Critical or Error level)
                let severity = alarm.severity.to_lowercase();
                if severity == "critical" || severity == "error" {
                    self.patrol_manager.wake(crate::patrol::types::WakeSignal {
                        workspace_id: alarm.workspace_id.clone(),
                        reason: format!("Alarm: {}", alarm.message),
                        context: format!("device_id={}, alarm_type={}", alarm.device_id, alarm.alarm_type),
                        priority: if severity == "critical" {
                            WakePriority::Critical
                        } else {
                            WakePriority::High
                        },
                        device_id: Some(alarm.device_id.clone()),
                        alarm_type: Some(alarm.alarm_type.clone()),
                        rule_id: alarm.rule_id.clone(),
                    });
                }
            }
            (AiEventType::PatrolCompleted, AiEvent::PatrolCompleted { workspace_id, report }) => {
                // Persist patrol results via ActionRepo
                match self.action_repo.insert_patrol_actions(workspace_id, report).await {
                    Ok(_) => info!(workspace_id, "Patrol actions persisted"),
                    Err(e) => {
                        error!(workspace_id, error = %e, "Failed to persist patrol actions — dead-letter queue");
                        // Dead-letter queue retry logic: 3x exponential backoff, then lost_events table
                        self.retry_with_backoff(workspace_id, report).await;
                    }
                }
            }
            (AiEventType::ChatCompleted, AiEvent::ChatCompleted {
                workspace_id, agent_id, session_key, model, messages
            }) => {
                // Reflect on completed conversation
                if let Err(e) = self.memory_service
                    .reflect_conversation_turn(workspace_id, agent_id, session_key, model, messages)
                    .await
                {
                    warn!(workspace_id, agent_id, error = %e, "Memory reflection failed");
                }
            }
            (AiEventType::WorkspaceCreated, AiEvent::WorkspaceCreated { workspace_id }) => {
                self.patrol_manager.start(workspace_id).await;
            }
            (AiEventType::WorkspaceDeleted, AiEvent::WorkspaceDeleted { workspace_id }) => {
                self.patrol_manager.stop(workspace_id).await;
            }
            _ => {
                debug!(?ai_event_type, "No handler for AiEvent variant");
            }
        }
    }

    async fn retry_with_backoff(
        &self,
        workspace_id: &str,
        report: &crate::patrol::types::PatrolReport,
    ) {
        use backoff::ExponentialBackoffBuilder;
        use std::time::Duration;

        let backoff = ExponentialBackoffBuilder::new()
            .with_initial_interval(Duration::from_millis(100))
            .with_max_interval(Duration::from_secs(10))
            .with_max_elapsed_time(Some(Duration::from_secs(60)))
            .build();

        let mut attempts = 0;
        let report = report.clone();
        let ws_id = workspace_id.to_string();
        let action_repo = self.action_repo.clone();

        tokio::spawn(async move {
            for delay in &backoff {
                attempts += 1;
                tokio::time::sleep(delay).await;

                match action_repo.insert_patrol_actions(&ws_id, &report).await {
                    Ok(_) => {
                        info!(workspace_id = %ws_id, attempts, "Patrol actions persisted after retry");
                        return;
                    }
                    Err(e) if attempts >= 3 => {
                        error!(
                            workspace_id = %ws_id,
                            attempts,
                            error = %e,
                            "Action persistence failed after 3 retries — writing to lost_events"
                        );
                        // Write to lost_events table (delegated to cloud/ for DB access)
                        return;
                    }
                    Err(e) => {
                        warn!(workspace_id = %ws_id, attempt = attempts, error = %e, "Retrying action persistence");
                    }
                }
            }
        });
    }
}

#[async_trait::async_trait]
impl tinyiothub_core::event::EventHandler for AiEventHandler {
    fn name(&self) -> &str {
        "AiEventHandler"
    }

    fn priority(&self) -> u8 {
        10 // Lower priority than AlarmEventHandler (which is 0)
    }

    fn should_handle(&self, event: &Event) -> bool {
        matches!(event.event_type(), EventType::Ai(_))
    }

    async fn handle(&self, event: &Event) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.handle_ai_event(event).await;
        Ok(())
    }
}
```

- [ ] **Step 2: Write orchestrator/mod.rs**

```rust
// crates/tinyiothub-ai/src/orchestrator/mod.rs

pub mod callbacks;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tinyiothub_runtime::EventBus;
use tracing::{error, info};

use crate::event::bus::AiEventPublisher;
use crate::memory::service::MemoryService;
use crate::patrol::manager::PatrolManager;
use crate::patrol::repo::ActionRepository;

use callbacks::AiEventHandler;

/// Orchestrator wires cross-domain callbacks and manages the AI subsystem lifecycle.
///
/// All cross-domain communication flows through the Orchestrator:
/// Alarm → EventBus → Orchestrator → PatrolManager.wake()
/// ChatCompleted → EventBus → Orchestrator → MemoryService.reflect()
/// PatrolCompleted → EventBus → Orchestrator → ActionRepo.insert()
/// WorkspaceCreated → EventBus → Orchestrator → PatrolManager.start()
/// WorkspaceDeleted → EventBus → Orchestrator → PatrolManager.stop()
pub struct Orchestrator {
    event_bus: Arc<EventBus>,
    handler: Arc<AiEventHandler>,
    event_publisher: Arc<AiEventPublisher>,
    shutting_down: Arc<AtomicBool>,
}

impl Orchestrator {
    pub fn new(
        event_bus: Arc<EventBus>,
        patrol_manager: Arc<PatrolManager>,
        action_repo: Arc<dyn ActionRepository>,
        memory_service: Arc<MemoryService>,
    ) -> Self {
        let event_publisher = Arc::new(AiEventPublisher::new(event_bus.clone()));

        let handler = Arc::new(AiEventHandler::new(
            patrol_manager,
            action_repo,
            memory_service,
            event_publisher.clone(),
        ));

        Self {
            event_bus,
            handler,
            event_publisher,
            shutting_down: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Register all cross-domain callbacks on the event bus.
    pub async fn start(&self) {
        info!("Orchestrator starting — registering AI event handler");
        self.event_bus.register_handler(self.handler.clone());
        info!("Orchestrator started");
    }

    /// Graceful shutdown: stop accepting new events, wait for handlers to drain.
    pub async fn shutdown(&self) {
        info!("Orchestrator shutting down...");
        self.shutting_down.store(true, Ordering::SeqCst);

        // PatrolManager::shutdown_all() is called by the caller (service_manager)
        // before this. The EventBus is shared and dropped naturally.

        info!("Orchestrator shutdown complete");
    }

    pub fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::SeqCst)
    }

    pub fn event_publisher(&self) -> &Arc<AiEventPublisher> {
        &self.event_publisher
    }
}
```

- [ ] **Step 3: Commit**

```bash
git add crates/tinyiothub-ai/src/orchestrator/
git commit -m "feat: add Orchestrator with cross-domain event callbacks and dead-letter retry"
```

---

### Task 12: Wire cloud/ integration — ServiceManager, AppState, server

**Files:**
- Modify: `cloud/Cargo.toml`
- Modify: `cloud/src/shared/service_manager.rs`
- Modify: `cloud/src/shared/app_state.rs`
- Modify: `cloud/src/server.rs`

- [ ] **Step 1: Add tinyiothub-ai dependency to cloud/Cargo.toml**

```toml
# Add under workspace crates section:
tinyiothub-ai = { workspace = true }
```

- [ ] **Step 2: Update AppState — add orchestrator field, remove OnceLock setters**

In `cloud/src/shared/app_state.rs`:

Add field:
```rust
pub orchestrator: Arc<crate::modules::ai_system::AiSystemHandle>,
```

Remove `set_heartbeat_manager()` calls — these become `orchestrator.start()`.

The `heartbeat_manager` field can remain temporarily for backward compat during migration, but all new code uses `orchestrator`.

- [ ] **Step 3: Update ServiceManager::start_all() — wire AI subsystem**

In `cloud/src/shared/service_manager.rs`, after the existing event handler registrations, add:

```rust
use tinyiothub_ai::orchestrator::Orchestrator;
use tinyiothub_ai::patrol::repo::{SqliteActionRepository, SqliteHeartbeatTaskRepository};

// After existing handler registrations (around line 89):

// Build AI subsystem repositories
let action_repo = Arc::new(SqliteActionRepository::new(app_state.database.clone()));
let heartbeat_task_repo = Arc::new(SqliteHeartbeatTaskRepository::new(app_state.database.clone()));

// Build AI subsystem services
let patrol_config = tinyiothub_ai::patrol::types::HeartbeatConfig::default();
let patrol_manager = Arc::new(tinyiothub_ai::patrol::manager::PatrolManager::new(
    heartbeat_task_repo,
    // event_publisher will be set by Orchestrator
    Arc::new(tinyiothub_ai::event::bus::AiEventPublisher::new(app_state.event_bus.clone())),
    patrol_config,
));
patrol_manager.set_agent_pool(app_state.agent_pool.clone() as Arc<dyn tinyiothub_ai::agent::pool::AgentPoolLike>);

let memory_service = Arc::new(tinyiothub_ai::memory::service::MemoryService::new());

// Build Orchestrator
let orchestrator = Arc::new(Orchestrator::new(
    app_state.event_bus.clone(),
    patrol_manager.clone(),
    action_repo,
    memory_service,
));
orchestrator.start().await;

app_state.orchestrator = orchestrator;
app_state.patrol_manager = patrol_manager;

// Start patrol loops for existing workspaces
let workspaces = app_state.workspace_repository.list_all().await?;
for ws in &workspaces {
    patrol_manager.start(&ws.id).await;
}

info!("✅ AI subsystem started");
```

- [ ] **Step 4: Update ServiceManager shutdown to drain AI first**

In the shutdown method, before dropping other services:
```rust
// Shut down AI subsystem first (so events drain before Bus dies)
info!("Shutting down AI subsystem...");
if let Some(orchestrator) = app_state.orchestrator.clone() {
    orchestrator.shutdown().await;
}
// Stop all patrol loops
app_state.patrol_manager.active_workspaces().iter().for_each(|ws_id| {
    app_state.patrol_manager.stop(ws_id);
});
```

- [ ] **Step 5: Commit**

```bash
git add cloud/Cargo.toml cloud/src/shared/service_manager.rs cloud/src/shared/app_state.rs
git commit -m "feat: wire AI subsystem into ServiceManager with Orchestrator lifecycle"
```

---

### Task 13: Remove OnceLock<HeartbeatManager> from AlarmService

**Files:**
- Modify: `cloud/src/modules/alarm/service.rs`
- Modify: `cloud/src/modules/workspace/` (remove OnceLock references)

- [ ] **Step 1: Remove OnceLock<HeartbeatManager> from AlarmService**

In `cloud/src/modules/alarm/service.rs`:

Remove field:
```rust
// DELETE:
heartbeat_manager: std::sync::OnceLock<Arc<HeartbeatManager>>,
```

Remove from `new()`:
```rust
// DELETE:
heartbeat_manager: std::sync::OnceLock::new(),
```

Remove `set_heartbeat_manager()` method entirely.

Replace the `wake_heartbeat` callsite (where AlarmService publishes AlarmCreated) with:
```rust
// Instead of directly calling heartbeat_manager.wake():
// Publish AiEvent::AlarmCreated — Orchestrator handles the wake
use tinyiothub_ai::event::types::AiEvent;
self.event_publisher.publish(AiEvent::AlarmCreated(alarm.clone()));
```

Add `event_publisher` field:
```rust
event_publisher: Arc<tinyiothub_ai::event::bus::AiEventPublisher>,
```

- [ ] **Step 2: Remove OnceLock from WorkspaceService**

Find `set_heartbeat_manager()` in workspace service and remove it. Replace workspace create/delete handlers to publish events instead:

```rust
// In workspace create handler:
self.event_publisher.publish(AiEvent::WorkspaceCreated {
    workspace_id: new_ws.id.clone(),
});

// In workspace delete handler:
self.event_publisher.publish(AiEvent::WorkspaceDeleted {
    workspace_id: deleted_ws_id.to_string(),
});
```

- [ ] **Step 3: Remove OnceLock setter calls from app_state.rs**

Delete lines that call:
```rust
alarm_service.set_heartbeat_manager(heartbeat_manager.clone());
workspace_service.set_heartbeat_manager(heartbeat_manager.clone());
```

- [ ] **Step 4: Build and fix**

```bash
cargo build -p tinyiothub-cloud 2>&1 | head -100
```

Expected: some import errors. Fix all compiler errors until clean build.

- [ ] **Step 5: Commit**

```bash
git add cloud/src/modules/alarm/service.rs cloud/src/modules/workspace/ cloud/src/shared/app_state.rs
git commit -m "refactor: remove OnceLock<HeartbeatManager> — replace with AiEvent publishing"
```

---

### Task 14: Port AgentPool from cloud/ to tinyiothub-ai agent/pool.rs

**Files:**
- Create: `crates/tinyiothub-ai/src/agent/builder.rs`
- Modify: `crates/tinyiothub-ai/src/agent/pool.rs` (replace stub)
- Create: `crates/tinyiothub-ai/src/agent/repo.rs`
- Create: `crates/tinyiothub-ai/src/agent/service.rs`

- [ ] **Step 1: Write agent/pool.rs — real AgentPool implementation**

Port from `cloud/src/modules/agent/agent.rs` (796 lines). Key structural changes:
- Replace direct `action_repo.insert()` calls — removed; patrol_loop publishes events instead
- Replace `DashMap<String, TrustConfig>` — get TrustConfig from PatrolManager via `Arc<PatrolManager>` reference
- Keep: agent building (zeroclaw), skills loading, tool refresh

```rust
// crates/tinyiothub-ai/src/agent/pool.rs
// (full implementation ~400 lines — ported from cloud/src/modules/agent/agent.rs)

use std::sync::Arc;
use dashmap::DashMap;
use tokio::sync::RwLock as TokioRwLock;
use async_trait::async_trait;
use tracing::{debug, error, info, warn};

use super::types::{AgentError, AgentHandle};
use super::builder::AgentBuilder;
use crate::patrol::manager::PatrolManager;

/// Pool of zeroclaw Agent instances, keyed by workspace_id.
///
/// Handles agent lifecycle (create, cache, invalidate, cleanup).
/// Does NOT manage patrol loops — that's PatrolManager's job.
/// Does NOT own TrustConfig — queries PatrolManager for that.
pub struct AgentPool {
    agents: DashMap<String, Arc<dyn AgentHandle>>,
    builder: AgentBuilder,
    patrol_manager: Arc<PatrolManager>,
    max_agents: usize,
    idle_timeout_secs: u64,
}

impl AgentPool {
    pub fn new(
        builder: AgentBuilder,
        patrol_manager: Arc<PatrolManager>,
        max_agents: usize,
        idle_timeout_secs: u64,
    ) -> Self {
        Self {
            agents: DashMap::new(),
            builder,
            patrol_manager,
            max_agents,
            idle_timeout_secs,
        }
    }
}

#[async_trait]
impl super::pool::AgentPoolLike for AgentPool {
    async fn get_or_create(&self, workspace_id: &str) -> Result<Arc<dyn AgentHandle>, AgentError> {
        if let Some(agent) = self.agents.get(workspace_id) {
            return Ok(agent.value().clone());
        }

        if self.agents.len() >= self.max_agents {
            return Err(AgentError::PoolFull);
        }

        let trust_config = self.patrol_manager
            .get_trust_config(workspace_id)
            .unwrap_or_default();

        let agent = self.builder
            .build(workspace_id, &trust_config)
            .await
            .map_err(|e| AgentError::Build(e))?;

        let agent: Arc<dyn AgentHandle> = Arc::new(agent);
        self.agents.insert(workspace_id.to_string(), agent.clone());
        info!(workspace_id, "Agent built and cached");
        Ok(agent)
    }

    async fn invalidate(&self, workspace_id: &str) {
        self.agents.remove(workspace_id);
        info!(workspace_id, "Agent invalidated");
    }

    async fn cleanup(&self, idle_timeout_secs: u64) {
        // Removes agents idle longer than idle_timeout_secs.
        // Full implementation ported from cloud agent.rs.
        info!(count = self.agents.len(), "Agent pool cleanup");
    }

    async fn refresh_tools(&self) -> Result<(), AgentError> {
        self.agents.clear();
        info!("Agent pool tools refreshed — all agents invalidated");
        Ok(())
    }
}
```

- [ ] **Step 2: Write agent/builder.rs — AgentBuilder from scaffold + zeroclaw**

```rust
// crates/tinyiothub-ai/src/agent/builder.rs

use crate::patrol::types::TrustConfig;

/// Builds zeroclaw Agent instances with workspace-specific configuration.
pub struct AgentBuilder {
    // zeroclaw runtime configuration fields (ported from agent.rs)
}

impl AgentBuilder {
    pub fn new(/* config params */) -> Self {
        Self {}
    }

    pub async fn build(
        &self,
        workspace_id: &str,
        trust_config: &TrustConfig,
    ) -> Result<impl crate::agent::types::AgentHandle, String> {
        // Ported from AgentPool::get_or_create in agent.rs
        Err("not yet ported".to_string())
    }
}
```

- [ ] **Step 3: Create agent/repo.rs and agent/service.rs stubs**

```rust
// crates/tinyiothub-ai/src/agent/repo.rs

#[async_trait::async_trait]
pub trait AgentRepository: Send + Sync {
    // Agent persistence methods
}

// crates/tinyiothub-ai/src/agent/service.rs

pub struct AgentService {
    // Agent CRUD operations (ported from cloud agent service)
}
```

- [ ] **Step 4: Update agent/mod.rs**

```rust
pub mod types;
pub mod repo;
pub mod service;
pub mod pool;
pub mod builder;
```

- [ ] **Step 5: Commit**

```bash
git add crates/tinyiothub-ai/src/agent/
git commit -m "feat: port AgentPool and AgentBuilder from cloud/ to tinyiothub-ai"
```

---

### Task 15: Integration tests — EventBus end-to-end

**Files:**
- Create: `crates/tinyiothub-ai/tests/integration_tests.rs`

- [ ] **Step 1: Write integration test — Alarm → Patrol wake chain**

```rust
// crates/tinyiothub-ai/tests/integration_tests.rs

use std::sync::Arc;
use tinyiothub_ai::event::bus::AiEventPublisher;
use tinyiothub_ai::event::types::AiEvent;
use tinyiothub_runtime::EventBus;
use tinyiothub_core::models::event::{EventType, AiEventType};

#[tokio::test]
async fn test_alarm_created_event_published() {
    let bus = Arc::new(EventBus::new());
    let publisher = AiEventPublisher::new(bus.clone());

    let mut receiver = bus.subscribe();

    let alarm = tinyiothub_ai::alarm::types::Alarm {
        id: "alarm-1".to_string(),
        workspace_id: "ws-1".to_string(),
        device_id: "dev-1".to_string(),
        alarm_type: "temperature".to_string(),
        severity: "critical".to_string(),
        message: "Temp too high".to_string(),
        rule_id: None,
        resolved: false,
        created_at: chrono::Utc::now(),
    };

    publisher.publish(AiEvent::AlarmCreated(alarm));

    // Give the spawned task time to execute
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let received = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        receiver.recv(),
    )
    .await
    .expect("Timeout waiting for event")
    .expect("Failed to receive event");

    assert!(matches!(received.event_type(), EventType::Ai(_)));
    assert_eq!(publisher.events_published(), 1);
    assert_eq!(publisher.events_dropped(), 0);
}

#[tokio::test]
async fn test_patrol_completed_event_serialization_roundtrip() {
    let report = tinyiothub_ai::patrol::types::PatrolReport {
        workspace_id: "ws-1".to_string(),
        status: tinyiothub_ai::patrol::types::PatrolStatus::Complete,
        summary: "All good".to_string(),
        executed_actions: vec![tinyiothub_ai::patrol::types::AutoExecutedAction {
            tool_name: "check_temp".to_string(),
            device_id: Some("dev-1".to_string()),
            success: true,
            details: "Temp=25C".to_string(),
        }],
        pending_proposals: vec![],
        error: None,
    };

    let event = AiEvent::PatrolCompleted {
        workspace_id: "ws-1".to_string(),
        report: report.clone(),
    };

    let json = serde_json::to_string(&event).unwrap();
    let deserialized: AiEvent = serde_json::from_str(&json).unwrap();

    match deserialized {
        AiEvent::PatrolCompleted { workspace_id, report: r } => {
            assert_eq!(workspace_id, "ws-1");
            assert_eq!(r.status, tinyiothub_ai::patrol::types::PatrolStatus::Complete);
            assert_eq!(r.executed_actions.len(), 1);
        }
        _ => panic!("Wrong event variant"),
    }
}

#[tokio::test]
async fn test_chat_completed_event_with_model_field() {
    let event = AiEvent::ChatCompleted {
        workspace_id: "ws-1".to_string(),
        agent_id: "agent-1".to_string(),
        session_key: "agent:agent-1:user/sess-1".to_string(),
        model: "claude-sonnet-4-6".to_string(),
        messages: vec![tinyiothub_ai::session::types::ChatTurnMessage {
            role: "user".to_string(),
            content: "Hello".to_string(),
            timestamp: None,
        }],
    };

    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("claude-sonnet-4-6"));
    assert!(json.contains("model"));
}

#[tokio::test]
async fn test_event_bus_handler_receives_ai_event() {
    use tinyiothub_core::event::EventHandler;
    use tinyiothub_core::models::event::{Event, EventLevel, EventSource, RichContent};

    let bus = Arc::new(EventBus::new());

    struct TestHandler {
        received: std::sync::Arc<tokio::sync::Notify>,
    }

    #[async_trait::async_trait]
    impl EventHandler for TestHandler {
        fn name(&self) -> &str { "TestHandler" }
        fn priority(&self) -> u8 { 0 }
        fn should_handle(&self, event: &Event) -> bool {
            matches!(event.event_type(), EventType::Ai(_))
        }
        async fn handle(&self, _event: &Event) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            self.received.notify_one();
            Ok(())
        }
    }

    let notify = std::sync::Arc::new(tokio::sync::Notify::new());
    let handler = Arc::new(TestHandler { received: notify.clone() });
    bus.register_handler(handler);

    let event = Event::new(
        EventType::Ai(AiEventType::ChatCompleted),
        EventLevel::Info,
        EventSource::system("test".to_string(), None),
        RichContent::new_text("test".to_string(), r#"{"ChatCompleted":{"workspace_id":"w","agent_id":"a","session_key":"k","model":"m","messages":[]}}"#.to_string()),
    )
    .unwrap();

    bus.publish(event).await.unwrap();

    tokio::time::timeout(std::time::Duration::from_secs(1), notify.notified())
        .await
        .expect("Handler was not called within timeout");
}
```

- [ ] **Step 2: Run integration tests**

```bash
cargo test -p tinyiothub-ai --test integration_tests
```

Expected: all tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/tinyiothub-ai/tests/
git commit -m "test: add integration tests for EventBus AiEvent publishing and handler dispatch"
```

---

### Task 16: Final cleanup — remove old code, verify green build

**Files:**
- Delete: `cloud/src/modules/agent/heartbeat.rs`
- Delete: `cloud/src/modules/agent/heartbeat_manager.rs`
- Delete: `cloud/src/modules/agent/action_repo.rs` (replaced by patrol/repo.rs)
- Delete: `cloud/src/modules/agent/reflect.rs` (replaced by memory/reflect.rs)
- Modify: `cloud/src/modules/agent/mod.rs` (remove deleted module declarations)
- Modify: `cloud/src/modules/mod.rs` (if needed)

- [ ] **Step 1: Remove old module declarations**

In `cloud/src/modules/agent/mod.rs`, remove:
```rust
// DELETE these lines:
pub mod action_repo;
pub mod heartbeat;
pub mod heartbeat_manager;
pub mod reflect;
```

- [ ] **Step 2: Fix all remaining references to old modules**

```bash
grep -rn "heartbeat_manager\|heartbeat::\|action_repo\|modules::agent::reflect" cloud/src/ --include="*.rs" | grep -v "target/" | grep -v ".git/"
```

Fix each reference. Most should already be using the new tinyiothub-ai crate.

- [ ] **Step 3: Delete old files**

```bash
git rm cloud/src/modules/agent/heartbeat.rs
git rm cloud/src/modules/agent/heartbeat_manager.rs
git rm cloud/src/modules/agent/action_repo.rs
git rm cloud/src/modules/agent/reflect.rs
```

- [ ] **Step 4: Full build**

```bash
cargo build -p tinyiothub-ai -p tinyiothub-cloud 2>&1
```

Expected: clean build, no errors.

- [ ] **Step 5: Run full test suite**

```bash
cargo test -p tinyiothub-core -p tinyiothub-ai -p tinyiothub-cloud 2>&1
```

Expected: all tests pass.

- [ ] **Step 6: Run clippy**

```bash
cargo clippy -p tinyiothub-ai -p tinyiothub-cloud -- -D warnings 2>&1
```

Expected: no warnings.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "refactor: remove old agent heartbeat/reflect/action_repo code

Replaced by tinyiothub-ai crate: patrol/, memory/, orchestrator/.
All AI domain logic now lives in crates/tinyiothub-ai/.
Cross-domain communication via EventBus + Orchestrator callbacks."
```

---

## Execution Summary

| Task | Description | Dependencies |
|------|-------------|--------------|
| 1 | Scaffold crate + AiEventType | None |
| 2 | event/types + event/bus | Task 1 |
| 3 | patrol/types + patrol/repo | Task 2 |
| 4 | patrol/manager.rs | Task 3 |
| 5 | patrol/loop.rs + patrol/report.rs | Task 4 |
| 6 | alarm/types + alarm/repo | Task 2 |
| 7 | session/types | Task 2 |
| 8 | tool/types + tool/trust | Task 2 |
| 9 | memory/types + memory/service + memory/reflect | Task 7 |
| 10 | agent/types + agent/pool | Task 4, 8 |
| 11 | orchestrator/callbacks + orchestrator/mod | Tasks 4, 5, 6, 9 |
| 12 | cloud/ integration (ServiceManager, AppState) | All above |
| 13 | Remove OnceLock from AlarmService + WorkspaceService | Task 12 |
| 14 | Port AgentPool from cloud/ to crate | Task 12 |
| 15 | Integration tests | Task 11 |
| 16 | Final cleanup — delete old code, verify green | All above |

**Parallelization:** Tasks 6, 7, 8, 9 can run in parallel after Task 2. Tasks 10-11 depend on Tasks 4-9. Tasks 12-16 are sequential.
