# Changelog

## [0.4.6.0] - 2026-08-03

### Added

- **Thing Agent Loop（自治运维闭环）**: AI 不再只回答问题——它被设备事件、定时巡检或用户指令唤醒，查询物本体、自主决策、直接操作设备，行动后回读验证并留下完整审计。温度超限自动调节、设备离线自动诊断这类场景现在开箱即用（演示脚本见 `examples/thing-agent-e2e.md`）
- **三态自治策略门**: 每个工作区独立设置 `off | diagnose | act` 三档自治级别（默认 off，零 LLM 成本）；动作白/黑名单、每小时频率熔断、kill switch 即时生效——你可以在 `PUT /api/workspaces/{id}/agent/policy` 一键收紧或放开
- **流式执行与客观验证**: Agent 运行时框架实时捕获每次工具调用轨迹，25 次调用/5 分钟硬预算防失控；"已验证"标记由回读动作客观判定，不采信 AI 自述
- **统一策略面（X3）**: chat 确认令牌、心跳信任配置、自治策略门收敛为一个策略引擎三个接入面——同一个动作在任意路径上得到一致的裁决
- **心跳桥（X6）**: 心跳巡检发现的问题自动转交自治 Loop 处置，带结构化去重（6 小时窗口内同一问题不重复处理，人工 ack 后 7 天抑制）
- **管理 API**: `POST /agent/tasks`（提交自治指令）、`GET /agent/runs`（运行历史分页）、`POST /agent/runs/{id}/ack`（人工确认）、`GET/PUT /agent/policy`（策略读写），全部工作区隔离 + admin 角色
- **智能防刷**: 同类事件 30 秒合并一次唤醒（告警风暴看全貌而非刷屏）、每小时唤醒熔断、AI 自己动作产生的事件不再唤醒 AI（共振防护）、指令 60 秒去重
- **审计与成本可见**: 每次运行落库（触发源/动作/验证/耗时/token），按工作区按日聚合成本；失败时自动生成人工接管清单推送到 chat

### Changed

- **invoke_action 双轨确认**: chat 对话中的动作仍走人工确认令牌（体验不变）；自治 Loop 中的动作改走策略门（预声明规则替代逐次确认）
- **事件管线**: events 表新增 `actor` 列区分设备/AI 来源，支持进程内广播与游标补偿（重启不丢高级别事件）

### For contributors

- 新模块 `crates/ai/src/thing_agent`（trigger/scheduler/runner/report）+ `cloud/src/modules/agent/autonomous_factory.rs`；`crates/policy` 策略引擎 SQLite 持久化实现
- 设计与裁决记录：`docs/superpowers/specs/2026-07-29-thing-agent-loop-design.md`（O1-O29）、19 任务实现计划同名 plans 目录
- 1421+ 测试全绿；全链路集成测试真实路由进、真实驱动出（仅 LLM 可剧本化）

## [0.4.5.0] - 2026-07-27

### Added

- **Thing Ontology（物本体）**: devices generalized into Things — hierarchy (parent_id tree with cycle protection), thing templates as creation blueprints, per-thing properties/actions, knowledge documents with LLM-generated ontology summaries (lazy compute, dirty marking, single-flight), and full CRUD at `/api/things`
- **Thing event pipeline**: MQTT `thing/{id}/event/{name}` ingest with per-thing throttle (60/min, error/critical exempt), unknown-event degradation, event-sourced alarm rules (`rule_type='event'`), and status-vs-occurrence event semantics with dedup upsert
- **Agent ontology tools**: 9 tools (list_things, get_thing, get_thing_profile, get_thing_tree, read_property, invoke_action, query_events, search_knowledge, read_document) with workspace isolation and invoke_action confirmation flow
- **E1 marketplace**: thing_templates category with name-collision suffix install; **E3**: DTDL / WoT Thing Description import + DTDL export (format_version 2; WoT export tracked in TODOS)
- **Invoke endpoints**: `POST /things/{id}/actions/{name}/invoke` + `/confirm` — UI action execution with workspace-gated confirmation (D13 modal) and fail-closed default
- **Migration safety net**: automatic VACUUM backup before pending migrations, Rust-enforced foreign-key integrity check that aborts startup on violations, occurrence-aware events retention job (daily, status rows never time-purged)
- **Frontend**: things list (table/grid), detail page tabs, create wizard, confirm modal (params table, danger styling, focus trap), first-login upgrade banner, `/devices` → `/things` redirect
- **Tests**: 40+ new integration tests — event full-chain (MQTT→events→alarm), migration upgrade paths, tenant isolation, invoke confirm flow, retention exemption, open-API denied cases

### Changed

- **BREAKING**: `/api/devices` management endpoints removed (use `/api/things`); open API and MCP contracts renamed to thing semantics (adjudicated OV4)
- **Templates are creation-time blueprints**: the thing model lives in per-thing instances; template edits no longer propagate (design v3 V1)
- **Tenant isolation enforced everywhere**: thing CRUD, agent tools, open API, ack API, resource attach/detach, and confirm tokens all workspace-scoped
- **Events unified to a single table**: real_time_events/lost_events/event_performance_metrics dropped; status rows upsert with occurrence_count, occurrence rows append-only
- **Open API key verification**: prefix lookup + constant-time SHA-256 secret comparison + working expiry enforcement (previously every call 401'd and expired keys were accepted)

### Fixed

- **Migration boot loop (DM-1)**: FK repoint no longer commits before real property data exists — inline UNION copy with ID preservation; synthetic seed capabilities removed; upgrade-path regression tests
- **Dead event path recidivism**: event_subtype now stores the event name (alarm matching works), workspace_id resolved from the thing (rules can match), AlarmService wired into MQTT ingest
- **Summary pipeline**: single-flight deadlock, lost-wakeup hang, and UTF-8 slice panic on Chinese documents
- **Open API**: property IDOR, send_command cross-tenant action injection, command list 500 (column mismatch), event wire-shape errors
- **Orphaned tables**: device_properties/device_commands data preserved (IDs intact, alarm rules keep resolving); device_alarms hidden FK repaired
- **N+1**: batched breadcrumb loading, single-CTE cycle check in transaction, covering index for workspace event scans
- **Frontend**: renderPage route regression, dead navigation to removed pages, keyboard accessibility on clickable cards, empty-name thing creation now rejected (422)

### Removed

- **Workspace knowledge graph** (entities/relations/parse jobs, retired permanently)
- **products table** (converged into thing_templates), device_event_triggers
- ~7k lines of dead code: legacy device/knowledge/local-resources views, a2ui.rs, unused confirm-modal wiring, legacy import compat

## [0.4.4.1] - 2026-07-21

### Fixed

- **Chat session isolation**: chat stream, history, and abort now reject session keys belonging to other workspaces — an unscoped token can no longer read or stream into another tenant's conversation
- **Approval transparency**: pending proposals now show the exact tool parameters before approval, so operators no longer blind-sign LLM-generated actions; approved proposals execute under the proper MCP authorization context
- **Abort feedback**: aborting an unknown or already-finished chat run now returns an error instead of silently succeeding
- **Heartbeat tasks source of truth**: tasks live in the database (legacy `HEARTBEAT.md` auto-migrates once), persistence aligned with the agent_actions schema, and new workspaces are seeded with the default task set
- **Trust enforcement**: tool trust is evaluated at execution time with declared safety levels and per-workspace DB configuration, closing paths where untrusted tools could run
- **Event publishing reliability**: AI events publish through a bounded serialized queue with graceful shutdown draining, DLQ failures now surface instead of vanishing, and heartbeat metrics are wired end to end
- **Memory pipeline hardening**: reflection sanitization bypass and memory-poisoning paths closed; dedup tightened
- **Heartbeat lifecycle**: workspace delete ordering, loop signal channels, loop supervisor, and agent-pool locking hardened against races and leaks
- **Run identity**: chat run IDs are server-minted and returned via the first SSE event, so clients can abort the exact run they started
- **Robustness batch**: malformed LLM JSON guarded, skill loader input limits added, paused-task recovery fixed, timeout hierarchy aligned, dead APIs removed, and orchestrator start/shutdown made idempotent

## [0.4.4.0] - 2026-07-02

### Added

- SSE Token authentication: short-lived tokens for SSE connections via `POST /api/v1/auth/sse-token`, keeping JWT out of URL query strings and server logs
- Knowledge resource fallback: when the knowledge graph has no indexed entities, `search_knowledge` now falls back to workspace resource search
- AGENTS.md template loaded into agent system prompt as "Agent Rules" section
- Slash-command skill loading: `/skill-name` prefix detection and `get_skill` blocking requirement in system prompt
- Device description field exposed in MCP device search results

### Changed

- Lint rules tightened from `warn` to `deny` (dead_code, unused_imports, unused_variables, unused_mut, non_snake_case)
- Token blacklist check is now async (`is_token_blacklisted`) to avoid blocking tokio worker threads in middleware
- Tag queries now support empty tenant_id (skip tenant filter for cross-tenant lookups)
- Workspace ID injected into chat system prompt for agent context
- A2UI canvas tool description simplified with clearer surface kind guidance

### Removed

- Performance monitoring module (load balancer, metrics, monitor, optimizer) — unused legacy code

## [0.4.3] - 2026-06-30

### Added — AI Subsystem (tinyiothub-ai crate)

- **Orchestrator**: cross-domain AI event dispatch via EventBus — AlarmCreated→signal, HeartbeatCompleted→persist, WorkspaceCreated→start/stop — with dead-letter queue for failed events
- **HeartbeatRunner**: per-workspace async loops with dynamic task/config refresh, LoopSignal channels (External/ReloadTasks/ReloadConfig), and graceful shutdown
- **Heartbeat loop**: reads shared `Arc<RwLock<Vec<HeartbeatTask>>>` + `Arc<RwLock<TrustConfig>>` on each tick, processes External signals from alarms, reloads tasks on demand
- **PatrolManager**: per-workspace lifecycle management with DB-backed TrustConfig and event-driven action persistence
- **MemoryService**: full reflection pipeline — LLM → parse facts → write MemoryStore — with in-memory dedup, prompt sanitization, and prompt injection defense
- **AiEventPublisher**: fire-and-forget EventBus wrapper with published/dropped counters and DropNotifier alerting
- **AiEvent types**: 10 variants (AlarmCreated, HeartbeatCompleted, ChatCompleted, WorkspaceCreated/Deleted, HeartbeatPersistFailed, ReflectionFailed, ProposalCreated/Resolved)
- **Policy engine**: TrustLevel-based tool gating (read-only/auto/manual) with allow/block lists and destructive tool classification
- **Tool trust system**: classify_tool_safety, evaluate_tool_trust, TrustConfig per workspace
- **A2UI catalog**: 12+ IoT-specific components — DeviceCard, DataChart, ControlPanel, AlarmCard, StatCard, Scene3D, and more
- **AI Ops dashboard**: real-time heartbeat monitor, memory dashboard, agent health view
- **Dead Letter Queue**: SQLite-backed DLQ for AiEvents that exhaust retries, with admin API for inspect/discard
- **LoggingDropNotifier**: production-default DropNotifier using tracing::warn! for dropped AiEvents

### Added — Simulated Driver Upgrade

- **Anomaly engine**: drift, spike, jitter, stuck anomaly types with property-aware category-tuned behavior
- **Signal composition**: periodic, trend, Gaussian noise generators with configurable parameters
- **Pattern matching**: property name pattern matching for 12+ device types (temperature, humidity, pressure, etc.)
- **Tag-based correlation**: device correlation via EnvironmentContext tag matching
- **Simulated device module**: scaffolding for anomaly, correlation, patterns, and signal sub-modules

### Added — Skills & Tools

- **GetSkillTool**: on-demand skill loading from compact skill index
- **SearchResourcesTool**: resource search across workspace
- **KnowledgeTool**: knowledge graph query capability
- **Skill index**: frontmatter-parsed skill catalog with glob-based capability matching

### Fixed

- **Dynamic refresh**: heartbeat loop now re-reads tasks and TrustConfig on each tick instead of snapshotting at startup
- **Shutdown coordination**: retry_with_backoff tasks check AtomicBool, preventing orphaned tasks after shutdown
- **Regex cache**: JSON fence regex uses std::sync::LazyLock, avoiding per-call Regex::new allocation
- **ChatCompleted dead code**: self-referential events now documented with explanatory comments; ChatCompleted reflection handled directly in chat/service.rs
- **ReflectionFailed**: now published to EventBus for observability (previously silently dropped)

### Internal

- **56 unit tests** in tinyiothub-ai (Orchestrator callbacks, EventBus publisher, tool trust, memory reflection, skills parsing)
- Removed `self_healing` module (replaced by AI heartbeat subsystem)
- Removed `reflection/` analyzers (simplified to single MemoryService pipeline)

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

