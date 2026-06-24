# Changelog

## [0.4.3] - 2026-06-24

### Added — AI Event Integration

- **HeartbeatManager**: per-workspace AI autonomous patrol loops with configurable intervals, DashMap-based channel management, and graceful shutdown
- **heartbeat_loop**: reads HEARTBEAT.md tasks → collects wake signals → queries agent_actions → builds LLM prompt → calls AgentPool → records audit actions, with exponential backoff on failures
- **Alarm→AI wake**: Error/Critical alarms inject urgent context into the next heartbeat tick via mpsc channel, with WakePriority-based dedup and cap (max 5 signals)
- **Post-chat reflection**: simplified single-LLM-call memory extraction from conversation turns, parsing FACT|zone|confidence|fact format with high-confidence auto-accept
- **agent_actions audit table**: SQLite log of all AI decisions with composite index (workspace_id, event_type, created_at)

### Fixed

- **dedup_and_cap sort key**: now sorts by WakePriority (Critical > High > Normal) instead of reason string, ensuring high-priority signals survive truncation
- **heartbeat context query**: includes both "heartbeat" and "alarm" event_types so the AI sees alarm responses in its recent actions
- **stop() orphaned tasks**: AbortHandle aborts stuck heartbeat loops on 5s timeout instead of leaking them
- **interval_minutes=0 rejection**: config validation clamps or rejects zero-interval to prevent infinite LLM-hot-loop
- **Minimax fallback**: create_minimax_provider() uses try_get() to degrade gracefully when [minimax] config is missing instead of panicking

## [0.4.1] - 2026-06-15

### Added

- **AI event integration design spec**: architecture for autonomous AI alarm processing channel, using AlarmService Hook + AutonomousAgentRunner + agent_actions audit log

## [0.4.2] - 2026-06-12

### Added — Alarm System

- **Alarm center page**: industrial control room design with real-time status filter (Active/Acknowledged/Resolved), alarm level indicators, and batch operations
- **Alarm rules engine**: 5 condition types — Threshold, Range, Change (increase/decrease/any), Duration (sustained condition), Composite (AND/OR/NOT)
- **Alarm rules management UI**: create/edit/delete rules per device with condition builder, notification config, and enable/disable toggle
- **Auto-resolve**: alarms auto-resolve when property values return to normal range, with `resolution_type='auto_resolved'` metadata
- **Rule engine**: evaluates property change events against enabled rules, respects workspace scoping and device-level rules
- **Notification dispatch**: Email/SMS/Webhook channel support with per-rule notification config and suppress duration

### Added — Alarm Operations

- **Acknowledge & resolve**: single and batch operations with user attribution and resolution type tracking (Fixed/FalseAlarm/Ignored/AutoResolved)
- **Suppress duplicates**: prevents repeated alarms for the same device+rule while one is still active
- **Oscillation throttle**: DashMap-based per-rule throttle with configurable suppress duration to prevent alarm storms
- **Duration tracking**: sustained-condition evaluation with auto-cleanup of stale tracking entries

### Fixed

- **FK constraint on `resolved_by`**: set to NULL for auto-resolve to avoid `FOREIGN KEY (resolved_by) REFERENCES users(id)` violation — no "system" user exists
- **Workspace filter in batch update**: skip workspace subquery when `workspace_id` is empty to prevent FK errors on unassigned devices
- **Memory leak**: `duration_first_seen` DashMap now cleaned with `retain()` to remove entries older than 24 hours
- **Duplicate AlarmRepository eliminated**: all callers now use shared `Arc<dyn AlarmRepository>` trait object, `AlarmRepositoryImpl` removed

### Changed

- **Device detail alarm tab**: shows device-level alarms with client-side filtering
- **Alarm list**: populated `device_name` via batch device lookup, uses `display_name` over `name`
- **Datetime parsing**: robust multi-format parser (RFC3339, SQLite, ISO 8601 without timezone)
- **Legacy condition support**: backward-compatible parsing of `{"operator":"gt","value":85}` format

### Internal

- 66 alarm-related tests (rule engine unit tests + integration tests)
- FK constraints added to integration test schema matching production
- Database migrations: `resolution_type` column, relaxed FK constraints on alarm rules, `notification_config` column

## [0.4.0] - 2026-05-28

### Added — Workspace Resource Management

- **Workspace resources CRUD**: SQLite-backed storage with `workspace_resources` table, composite indexes, and full-text search
- **Resource types**: scene, device_model, image, document — with metadata, tags, and file path tracking
- **REST API**: `POST/GET/PUT/DELETE /workspaces/{id}/resources` with tenant isolation, plus `GET /workspaces/{id}/resources/search` with relevance-scored keyword search
- **Semantic search**: multi-keyword search across name, description, and JSON tags with `UNION ALL` + `SUM(relevance)` deduplication

### Added — Scene3D A2UI Component

- **Scene3D LitElement**: Three.js-powered 3D building visualization with GLTF/GLB model loading, OrbitControls, and auto-fit camera
- **Device markers**: overlay markers with status colors (online/offline/warning/error), click-to-select, and floor-based filtering
- **Floor management**: configurable floor buttons with clipping-plane-based floor cut visualization
- **A2UI catalog registration**: Scene3D registered as `scene3d` component kind with full canvas tool description

### Added — A2UI Catalog Expansion

- **10 new catalog components**: CheckBox, ChoicePicker, DateTimeInput, Icon, Image, List, Modal, Slider, Tabs, TextField
- **DeviceCard enhancements**: device type-to-icon mapping, signal strength bars, relative time formatting ("刚刚", "N 分钟前")
- **ProgressIndicator**: improved styling and animation

### Added — Agent Tooling

- **`search_workspace_resources` tool**: natural language search for workspace multimedia resources, registered with dependency injection via `Arc<WorkspaceService>`
- **Canvas tool catalog**: expanded to 27 component kinds with complete Scene3D parameter schema

### Fixed

- **Search relevance**: fixed `UNION ALL` duplicate rows by wrapping with `GROUP BY id` and `SUM(relevance)`
- **Database indexing**: added composite `idx_resources_workspace_type` index for efficient type-filtered queries
- **ResizeObserver leak**: cleared observer reference on Scene3D dispose to prevent stale references on retry

### Changed — Workspace UI Redesign

- **Process log panel**: collapsible sections with message-card layout — user bubbles vs AI cards, visually distinct roles
- **Collapsible thinking**: thinking/reasoning content folded by default with expand/collapse toggle and chevron animation
- **Collapsible tool execution**: tool calls show name + status indicator (spinner for in-progress, checkmark for done), expandable to reveal args/results
- **Event-driven updates**: replaced 100ms polling with `onChange` callback on ChatState, reducing CPU usage
- **Glass panel refinement**: `color-mix()` backgrounds with `backdrop-filter`, highlight border, depth shadows for floating panels
- **Empty state redesign**: SVG icons with title, hint text, and clickable example prompt chips for both stage and insight panels
- **Title redesign**: uppercase 13px with accent dot glow and letter-spacing
- **Responsive insight panel**: width uses `clamp(320px, 28vw, 420px)` for viewport-aware sizing
- **Compose bar**: centered single-line glass input with send/abort buttons
- **Scene3D color alignment**: status marker colors now read from CSS variables (`--ok`, `--muted`, `--warn`, `--danger`)

---

## [0.3.0] - 2026-05-21

### Added — AI Agent v0.3

- **Capability-based architecture**: AgentPool, ConfigService, ToolService, ChatService replacing monolithic Agent
- **AgentPool**: lazy agent creation, idle cleanup (30min timeout), DashMap-based concurrent cache
- **ToolService**: MCP tool catalog with `tool_label()` / `tool_group()`, CanvasTool for A2UI, denylist-based tool filtering
- **ConfigService**: DB-backed AgentRuntimeConfig with `AgentConfig` strong-typed struct, hot-reload on next chat
- **SessionKey**: unified `agent:<agentId>:<mainKey>` format with workspace verification

### Added — Agent Workspace & Identity

- Workspace settings tab: SOUL.md, IDENTITY.md, TOOLS.md per-workspace file editing
- Simplified identity model: persona_preset / system_prompt deprecated, SOUL.md as single source of truth
- persona_layer removed from prompt builder — workspace files directly injected
- 4 Chinese workspace template files with comprehensive content

### Added — Agent Self-Evolution (Reflection Engine)

- **tinyiothub-memory crate**: MemoryStore trait, AgentMemory types with zone/confidence/tags/effectiveness
- **Memory Dashboard**: Lit 3 frontend with tabbed layout, search, zone filter, load/supersede/delete
- **Reflection Pipeline**: Analyzer trait + JoinSet-based parallel execution
- **MemoryAnalyzer**: LLM-driven memory extraction from conversation turns
- **SkillAnalyzer**: skill gap detection from workspace prompt files
- **SecurityAnalyzer**: prompt injection detection in conversation context
- **ReflectionService**: micro_reflect (post-turn) + compile_profile (summary) + metrics
- **NotificationService**: SSE broadcast for skill discovery and memory changes
- **Reference detection**: sliding-window probe with 20-char min length guard
- **Superseded filter**: SQL-level transitive closure pushdown with index
- **Rate limiting**: Semaphore-based max-concurrency (default 3) on reflection calls
- DB migrations: `agent_memories`, `reflection_queue`, `reflection_log` tables

### Added — Engineering

- **Justfile**: 18 standardized recipes (fmt, lint, test, ci, ci-full, build, web-*)
- **Git hooks**: pre-commit (gitleaks), pre-push (fmt + clippy + test quality gate)
- **GitHub Issue templates**: YAML forms for bug_report and feature_request
- **CI improvements**: concurrency control (`cancel-in-progress`), architecture check path fix
- **AGENTS.md**: ~230-line cross-tool project instructions (stability tiers, risk tiers, anti-patterns, dev-operational contracts)
- **CLAUDE.md**: slimmed to Claude Code-specific behavior guidelines + skill routing
- **github-pr skill**: project-level Claude Code skill for PR creation and update
- Project cleanup: 7000+ lines of dead code removed, stale paths updated, `.gitignore` hardened

### Changed
- `ApiResponseBuilder` moved to `tinyiothub-web` crate
- Repository implementations migrated from `cloud/src/shared/` to `tinyiothub-storage` crate
- Module structure: `cloud/src/modules/<module>/{types,service,handler/}` three-layer convention
- MQTT default credentials changed to `tinyiothub` / `tinyiothub.123`

### Fixed
- CI frontend architecture check path resolution when running from `web/` working directory
- Orphaned CSS properties in `home.css` causing frontend build failure
- Agent workspace tab infinite loading from stale render state
- Double `/api/v1` prefix in memory API handlers
- `record_reference` race condition: atomic put + load_count increment
- `resolve_queue_item` authorization bypass in reflection queue

## [0.2.1] - 2026-05-11

### Added
- Frontend marketplace UI with templates/drivers tabs, search, pagination, and install flow
- Driver health dashboard frontend with real-time status display
- Device export-as-template and clone actions in device list UI
- Tabbed template detail modal (basic/properties/commands/deviceInfo)
- Integration tests for marketplace and driver-health handlers

### Fixed
- Driver health status now reflects real `ref_count` (active/idle) instead of hardcoded "active"
- Path traversal prevention in marketplace driver/template installation (`sanitize_filename`)
- URL query parameter encoding in marketplace proxy handlers
- Marketplace CSS extracted from inline `<style>` to standalone stylesheet

## [0.2.0] - 2026-05-08

### Added
- C FFI driver hot-loading with `libloading`, `DynamicDeviceDriver`, and `DriverRegistry`
- Per-workspace driver isolation in `DriverRegistry` with `WorkspaceRegistry`
- Driver rehydration on startup from `driver_installations` database records
- `TemplateExporter` — export existing device as reusable `DeviceTemplate`
- `MarketplacePublisher` — publish device templates to marketplace.tinyiothub.com
- `/api/v1/devices/{id}/export-template` endpoint
- `/api/v1/devices/{id}/clone` endpoint
- `/api/v1/marketplace/publish/template` endpoint
- Driver health dashboard module with `/api/v1/driver-health/drivers` endpoint
- Workspace-scoped driver preference support via `workspace_driver_preferences` table
- Driver installation tracking with `driver_installations` table
- Integration test for `DriverRegistry` workspace isolation

### Fixed
- Export-template description handling (plain string vs JSON object)
- Removed raw SQL UPDATE from handler by adding `workspace_id` to `CreateDeviceTemplateRequest`
- Localized marketplace handler error messages to Chinese
- Registry write lock now released between rehydration iterations
- Removed redundant `driver_registry` field from `AppState` (uses global singleton)

## [0.1.3] - 2026-05-07

### Fixed
- Consistent workspace resolution across monitoring and auth handlers (#30)
- Role repository column name mismatch: `IsAdministrator` -> `is_administrator` (#37)
- Security config persistence deduplicated through `SecureEventService` layer (#40)
- Silent failure in `update_security_config` handler — now routes through service (#39)
- `sysinfo::System` cached in `AppState` to avoid per-request allocation (#42)
- Health check uses `count_devices()` instead of loading full device list (#38)
- Removed dead product handler tests from test suite (#37)

### Added
- Role permission handler tests with real permission IDs from migrations (#44)
- Monitoring handler tests for health endpoints with workspace seeding (#43)
- Admin gate tests for system metrics endpoint (#44)
- `SecureEventService.update_config()` and `save_config_to_db()` for atomic config updates

