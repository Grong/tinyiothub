# Zeroclaw Cron Scheduler Integration Plan

## Context

TinyIoTHub currently has a simple scheduler (`TimeTask` in `application/scheduler.rs`) that:
- Polls every 60 seconds
- Uses in-memory Moka cache
- Has no retry/security
- `device_control` job is a **STUB** that only logs

Zeroclaw has a production-ready cron scheduler with:
- SQLite persistence (`cron_jobs`, `cron_runs` tables)
- Retry with exponential backoff
- Security validation (command allowlist, path validation)
- Support for `Shell` and `Agent` job types
- Delivery config for announcements (Telegram, Discord, Slack, etc.)

**User Goal**: Replace TinyIoTHub's scheduler with zeroclaw's, and have device_control handled through zeroclaw Agent jobs.

## The Core Challenge

Zeroclaw's cron scheduler (`cron::scheduler::run()`) calls `crate::agent::run()` internally for Agent jobs. This is **zeroclaw's own agent runtime**, not TinyIoTHub's `AgentRuntimeImpl`.

TinyIoTHub's IoT tools (`device_list`, `device_command`, etc.) are loaded through TinyIoTHub's **MCP registry** (`api::mcp::get_mcp_registry()`) and wrapped as `IoTToolAdapter` in `AgentRuntimeImpl.refresh_tools_impl()`.

**Problem**: When zeroclaw's cron scheduler runs an Agent job, it uses zeroclaw's native agent which doesn't have access to TinyIoTHub's IoT tools.

## Architecture Analysis

```
┌─────────────────────────────────────────────────────────────────┐
│                    TinyIoTHub Application                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────────┐    ┌──────────────────────────────────┐  │
│  │  TimeTask       │    │  Zeroclaw Cron Scheduler         │  │
│  │  (Current)      │    │  (cron::scheduler::run())        │  │
│  │  - 60s poll    │    │  - SQLite persistence             │  │
│  │  - In-memory   │    │  - Retry + backoff                │  │
│  │  - No retry    │    │  - Security validation           │  │
│  └────────┬────────┘    └──────────────┬───────────────────────┘  │
│           │                            │                         │
│           │ calls                      │ calls                   │
│           ▼                            ▼                         │
│  ┌─────────────────┐    ┌──────────────────────────────────┐  │
│  │ JobService      │    │ crate::agent::run()               │  │
│  │ (Direct exec)   │    │ (Zeroclaw's agent runtime)       │  │
│  └─────────────────┘    └──────────────┬───────────────────┘   │
│           ▲                            │                       │
│           │                            │ loads                 │
│           │                     ┌──────┴────────┐             │
│           │                     │ tools::        │             │
│           │                     │ all_tools_with_│             │
│           │                     │ runtime()      │             │
│           │                     └───────────────┘             │
│           │                            ▲                       │
│           │                            │                       │
│  ┌────────┴────────┐                   │                       │
│  │ IoT Tools       │                   │                       │
│  │ (STUB only)     │          ┌─────────┴─────────┐            │
│  └─────────────────┘          │ TinyIoTHub's     │            │
│                               │ MCP Registry     │            │
│                               │ (device_list,   │            │
│                               │  device_command)│            │
│                               └──────────────────┘            │
└─────────────────────────────────────────────────────────────────┘
```

## Integration Options

### Option 1: Deep Integration (Recommended for Production)

Wire TinyIoTHub's MCP tools into zeroclaw's agent tool system.

**Approach**:
1. Create a `TinyIoTHubToolLoader` that implements zeroclaw's tool loading interface
2. Hook into `Agent::from_config()` or create a custom agent builder
3. Merge TinyIoTHub's IoT tools with zeroclaw's native tools

**Pros**:
- Clean architecture - tools unified in one agent
- Full zeroclaw scheduler features

**Cons**:
- Requires modifying zeroclaw vendor code or creating a complex adapter
- High coupling between TinyIoTHub and zeroclaw internals

### Option 2: Thin Wrapper (Quick Win)

Create a TinyIoTHub cron service that:
1. Uses zeroclaw's cron **store** (SQLite persistence, job CRUD)
2. Uses TinyIoTHub's own agent for job execution
3. Translates zeroclaw job definitions to TinyIoTHub job types

**Approach**:
1. Replace `TimeTask` with a new `ZeroclawCronService`
2. Use `zeroclaw::cron::store::*` for job persistence
3. Execute Agent jobs via `AgentRuntimeImpl` (not `crate::agent::run()`)
4. Shell jobs via direct command execution

**Pros**:
- Minimal changes to zeroclaw vendor code
- TinyIoTHub tools automatically available
- Preserves TinyIoTHub's existing tool implementations

**Cons**:
- Need to implement job execution loop manually (similar to zeroclaw's but using TinyIoTHub agent)

### Option 3: Hybrid - Keep TimeTask, Enhance with Zeroclaw Store

Keep TinyIoTHub's `TimeTask` scheduler but swap the persistence layer.

**Approach**:
1. Keep `TimeTask` polling loop
2. Replace in-memory Moka cache with zeroclaw's SQLite store
3. Use `zeroclaw::cron::store::list_jobs()`, `due_jobs()`, etc.

**Pros**:
- Minimal changes
- Gets SQLite persistence + job CRUD API

**Cons**:
- Doesn't get zeroclaw's retry/security
- Still polling-based

## Recommended Approach: Option 2 (Thin Wrapper)

Given the complexity of deep integration and the fact that device_control is currently a stub, Option 2 provides the best balance:

1. **Persistence**: Use zeroclaw's SQLite store for jobs/runs
2. **Execution**: Route Agent jobs to TinyIoTHub's `AgentRuntimeImpl`
3. **Delivery**: Implement announcement delivery via TinyIoTHub's notification system

## Implementation Status

### ✅ Phase 1: COMPLETED - Replace TimeTask with CronService

**Files Created:**
- `src/application/cron.rs` - New CronService

**Files Modified:**
- `src/application/mod.rs` - Added `zeroclaw_cron` module
- `src/application/service_manager.rs` - Replaced `TimeTask` with `ZeroclawCronService`
- `src/infrastructure/agent/mod.rs` - Added `run_single` to `AgentRuntime` trait
- `src/infrastructure/agent/runtime.rs` - Implemented `run_single_impl()` and added trait method

**What Works:**
- Zeroclaw SQLite store for job persistence (`cron_jobs`, `cron_runs` tables)
- Agent jobs executed via TinyIoTHub's `AgentRuntimeImpl::run_single()`
- Shell jobs executed via direct command execution
- 15-second polling interval for due jobs
- Run history recorded via `record_run()`

**What's Missing (Phase 2):**
- No API endpoints for managing zeroclaw cron jobs
- No migration of existing TinyIoTHub jobs to zeroclaw schema
- No declarative job sync from config files
- `delete_after_run` not implemented (jobs never auto-delete)
- Health tracking not wired up

### Phase 2: Map Job Types

| TinyIoTHub Job | Zeroclaw Job Type | Execution |
|----------------|-------------------|-----------|
| webhook/http | Shell | Direct HTTP call |
| script | Shell | Command execution |
| device_control | Agent | TinyIoTHub agent with IoT tools |
| notification | Agent | Agent with notification prompt |
| sql | Shell | Direct SQL (dangerous!) |

### Phase 3: Migrate Existing Jobs

- Read existing jobs from TinyIoTHub's `jobs` table
- Create corresponding zeroclaw cron jobs
- Mark TinyIoTHub jobs as migrated

## Files Modified/Created

### Completed (Phase 1)
- **`src/application/mod.rs`** - Added `cron` module
- **`src/application/cron.rs`** (NEW) - CronService implementation
- **`src/application/service_manager.rs`** - Wired up `CronService` instead of `TimeTask`
- **`src/infrastructure/agent/mod.rs`** - Added `run_single` to `AgentRuntime` trait
- **`src/infrastructure/agent/runtime.rs`** - Implemented `run_single_impl()` method

### Remaining (Phase 2)
- API endpoints for cron job CRUD
- Migration of existing TinyIoTHub jobs to zeroclaw schema
- Declarative job sync from config files
- Health tracking integration

## Testing Strategy

1. Unit test job execution for each type
2. Integration test with SQLite store
3. Manual test: create a cron job, trigger execution, verify output

## Open Questions

1. Should we keep TinyIoTHub's `jobs` table or migrate entirely to zeroclaw's schema?
2. How to handle `delete_after_run` for one-shot jobs?
3. Should we keep TinyIoTHub's job execution service or replace entirely?

---

*Created: 2026-04-16*
*Status: Phase 1 Complete - Phase 2 Pending*
