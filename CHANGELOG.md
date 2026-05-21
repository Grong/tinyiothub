# Changelog

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

