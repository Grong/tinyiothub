# AGENTS.md — TinyIoTHub

Cross-tool agent instructions for any AI coding assistant working on this repository.

## Commands

```bash
# Rust
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --lib --bins --tests
cargo build --release

# Frontend
cd web && pnpm install --frozen-lockfile
cd web && pnpm type-check
cd web && pnpm build
cd web && pnpm lint

# Full CI gate (requires just)
just ci        # Rust only
just ci-full   # Rust + frontend
```

Docs-only changes: skip Rust/frontend batteries; run markdown lint and link-integrity checks.

## Project Snapshot

TinyIoTHub is an **IoT agent harness**, built in Rust + Lit 3. Device access is the substrate; the product itself is the agent runtime layer on top of it. Things (物) — devices, spaces, buildings, production lines — are modeled in a hierarchical ontology with properties, events, actions, and knowledge resources, and accessed over multiple protocols (Modbus, ONVIF, SNMP, MQTT).

Three layers — value concentrates in the middle:

| Layer | Contents | Note |
|---|---|---|
| Application | 巡检 / 应急 / 节能 / user-authored `SKILL.md` | scenario-specific — no Rust change needed |
| **Harness** | Session Loop · Skill · Todo · Ask · Subagent · safety layer | **the product** |
| Device | MQTT / Modbus / ONVIF / SNMP · thing ontology | substrate, must be sufficient but is not the differentiator |

Core promise: give an AI a hand that can safely operate the physical world. `SessionRunner` is the single execution entry for an Assistant turn; `TriggerRouter` only emits wakes; every device action passes policy / approval / Effect / unknown reconciliation and is verified by read-back.

Architecture follows the **relay paradigm** (buzz-relay model): one application crate owns all behavior; capability crates provide isolated subsystems.

**Tech stack**: Rust 2024, Tokio, Axum, Tower, SQLx + SQLite | Lit 3 + Vite + TypeScript, nanostore

### Harness Invariants (2026-11 定位：harness 是产品本体)

These are constraints, not suggestions. A change that violates one is wrong even if it compiles and passes tests.

1. **One loop** — chat, scheduled inspection, and event handling all execute through `SessionRunner`. Do not add a second executor or a parallel loop.
2. **Trigger ≠ execution** — `TriggerRouter` produces wakes and nothing else: no prompt assembly, no turn execution.
3. **Scenario lives in `SKILL.md`** — a new scenario (e.g. 空压机巡检) adds a skill plus a prompt section, not a Rust branch. If a change makes the loop scenario-aware, it is wrong.
4. **Safety does not move** — policy / trust / approval / Effect / Decision / unknown reconciliation stay in the tool layer. No permission mode (`read_only` / `ask` / `full_access`) bypasses interlocks or e-stop.
5. **Two ledgers, one timeline** — `session_events` is the only UI timeline; `hub_ledger` `runtime.v1` is the safety/audit ledger and must never feed the UI.
6. **Tool presentation is canonical** — every model-visible tool has one canonical name and one `viewKey`; UI renders tools by `viewKey`, never by switching on raw tool result JSON.

Where the primitives live:

| Primitive | Location |
|---|---|
| Session Loop / turn entry | `apps/cloud/src/domains/agent/session/runner.rs` |
| Trigger (wake only) | `apps/cloud/src/domains/agent/session/trigger.rs` |
| `todo_write` / `ask_user` / `spawn_subagent` | `apps/cloud/src/domains/agent/session/tools/` |
| Session state (todo / question / approval) | `session/todos.rs` · `questions.rs` · `approvals.rs` |
| Global HUD projection | `session/hud.rs` |
| Port / adapters / memory | `crates/agent/src/{port,adapters,memory}` |
| Design spec | `docs/superpowers/specs/2026-11-07-assistant-harness-design.md` |
| Tool catalog / aliases / viewKey | `apps/cloud/src/domains/agent/tools/catalog.rs` |
| ToolView registry | `web/src/ui/tool-views/` |

### Tool Catalog & Presentation

- The canonical tool table lives in `apps/cloud/src/domains/agent/tools/catalog.rs`. Model-visible names are frozen; old names are hidden aliases for approval replay only. Do not expose an alias to the model.
- Every canonical/alias entry has a stable `viewKey` (for example `read_device` → `device.read`). `PresentationDescriptor` carries the same key in the tool catalog.
- `tool/call` and `tool/result` carry `viewKey`. Historical rows without it are projected at read time in `session/handler.rs`; do not mutate stored payloads or their digests for presentation.
- Frontend tool presentation belongs in `web/src/ui/tool-views/`, keyed by `viewKey`. Story/chat must call `presentTool()`; do not add per-tool `switch`/`detailOf` parsing in `story-model.ts` or `timeline-render.ts`. Unknown tools use the generic view.
- Tool-owned presentation may include `summary`, `metrics`, `details`, and evidence/source facts. Concrete fields (device state, alarm severity, action target, etc.) belong in the tool view, not in business pages.
- Prompts and skill templates under `apps/cloud/templates/skills/` must use canonical model-visible names (`list_devices`, `read_device`, `set_device`, `call_device`, …), never hidden aliases.

### The Relay Paradigm (2026-08 范式统一，buzz-relay 模型)

Three axioms:

1. **One application owns all behavior** — `apps/cloud` holds every HTTP handler, every service, every orchestration. There are no domain crates.
2. **Capability crates provide capabilities, never orchestrate** — `crates/*` are isolated subsystems. They never import `apps/*` and never coordinate each other (coordination happens only in the application).
3. **core holds only values** — zero I/O, zero API semantics, zero logic functions.

### Dependency Direction (one-way, irreversible)

```
apps/* (cloud/edge/marketplace/cli) → crates/* (capability libs) → core
```

**Only rule**: capability crates must not depend on `apps/*` or on each other's orchestration. There is no edge allowlist — without domain crates there are no cross-domain edges. `db` depends only on `core`.

| Crate (dir = package) | Lib name | Role | Forbidden |
|-------|------|------|-----------|
| `core` | `tinyiothub_core` | Value types only (models/config/error/共享写入契约). **Guardrail: no logic functions, no I/O, no API semantics. Every new type must justify why it is shared across layers.** | Logic functions, I/O, DB access |
| `db` | `tinyiothub_storage` | ALL SQL: `Db` facade + flat per-domain files (row types + `pub(crate)` query functions + `impl Db` delegates) + `migrations/` (baseline + DDL-only) | Depending on any crate except `core`; trait inversion |
| `runtime` | `tinyiothub_runtime` | EventBus, DataServer, driver framework, executors, `runtime::plugin` (loader/registry/sandbox, FFI glue) | Depending on web or apps |
| `web` | `tinyiothub_web` | HTTP middleware, ApiResponseBuilder, security extractors | Business logic |
| `scheduler` | `tinyiothub_scheduler` | Cron engine + scheduler | Depending on apps |
| `llm` | `tinyiothub_llm` | LLM provider contract, prompt, session | Provider implementations (live in apps) |
| `memory` | `tinyiothub_memory` | Agent memory engine (MemoryService 持久化引擎；纯逻辑在 `agent`) | HTTP；禁止依赖 apps/*（db/llm 为例外） |
| `policy` | `tinyiothub_policy` | Policy gate evaluation (pure logic) | HTTP/SQL |
| `skills` | `tinyiothub_skills` | Skill/tool registries, trust engine | HTTP/SQL |
| `agent` | `tinyiothub_agent` | Agent 共性能力运行时（loop/pool/tools-framework/session/prompt + memory 纯逻辑；事件溯源契约）。引擎：**rig 0.42**（rig-core/rig-agent，MIT 库），业务代码只依赖反腐败层 `crates/agent/src/port`（Tool/ModelProvider/AgentLoop/Memory/Observer/PromptSection/TurnEvent），rig 桥接集中在 `adapters/rig/`；迁移决策见 `docs/designs/agent-engine-rig-migration.md` | axum, sqlx, tinyiothub_storage, apps/* |
| `authn` | `tinyiothub_authn` | 认证机制（JWT/SSE token/密码哈希；纯机制，构造注入零全局态） | axum, sqlx, db, tokio 依赖 |
| `plugin-sdk` | `tinyiothub_plugin_sdk` | Driver-author SDK; ABI contract single source of truth | Depending on runtime/web |
| `macros` | `tinyiothub_macros` | Proc macros | — |
| `apps/cloud` (bin) | — | **The relay**: all handlers, all services, all orchestration. `domains/` per business domain | Direct SQL in handlers (use the `db` facade) |
| `apps/edge` (bin) | — | Edge gateway application (same paradigm) | — |
| `apps/marketplace` / `apps/cli` | — | Marketplace service / CLI | — |

**Type ownership uniqueness**: every type has exactly ONE home — DB rows and query functions live in `db`; shared write contracts (handler↔db shared inputs) live in `core::models`; API-only shapes (Response DTOs, view models) live in `apps/cloud/src/domains/<domain>/dto.rs`. **Re-export shims are forbidden** — import from the type's real home.

**Naming rule**: crate directories and package names use short names (`core`, `db`, …); `[lib] name` is pinned to `tinyiothub_*` so `use tinyiothub_core::…` imports stay stable across directory moves.

## Stability Tiers

| Crate | Tier | Notes |
|-------|------|-------|
| `core` | Stable | Value contracts — breaking changes require MAJOR version bump |
| `plugin-sdk` | Stable | Driver ABI contract — breaking changes require MAJOR version bump |
| `web` | Beta | HTTP infrastructure — breaking changes permitted in MINOR with changelog |
| `db` | Beta | SQLite implementation — schema changes require migration |
| `runtime` | Beta | EventBus, DataServer — breaking changes permitted in MINOR |
| `memory`, `scheduler`, `llm`, `policy`, `skills`, `authn` | Beta | Capability engines |
| `agent` | Beta | Agent 共性能力运行时（loop/pool/tools/session/prompt） |
| `macros` | Experimental | Internal proc macros |
| `apps/*` | Experimental | Deployable binaries (cloud/edge/marketplace/cli) |

**Tiers**: Stable = covered by breaking-change policy. Beta = breaking changes permitted in MINOR with changelog notes. Experimental = no stability guarantee. Tiers are promoted, never demoted, through deliberate team decision.

## Repository Map

```
apps/
  cloud/                     # The relay — ALL behavior lives here (main binary)
    src/
      main.rs                # Thin entry (<200 lines): config → AppState → router → serve
      lib.rs                 # Module registry (deny(unsafe_code), module docs)
      bootstrap.rs           # Startup logic (logging, driver rehydrate, device cache)
      state.rs               # AppState — single shared state struct (buzz-relay state.rs)
      router.rs              # build_router: ALL route mounting in one place
      api/                   # HTTP middleware + extractors (WorkspaceScope, auth)
      domains/               # All business behavior, one dir per domain
        agent/               #   harness orchestration: session/ · prompt/ · tools/
        thing/               #   {mod.rs, handler.rs, service.rs, dto.rs}
        auth/  user/  tenant/  event/  alarm/  driver/
        notify/  mcp/  admin/
      shared/                # Composition-private glue (service_manager, paths, initialization)
      tests/                 # Integration tests
    templates/               # Skill templates
  edge/                      # Edge gateway application (same paradigm)
  marketplace/               # Marketplace service binary
  cli/                       # CLI binary
crates/                      # Capability libs — isolated, never orchestrate
  core/                      # Value types only (lib tinyiothub_core; zero I/O, zero API semantics)
  db/                        # ALL SQL: Db facade + flat per-domain files (lib tinyiothub_storage)
    migrations/              # SQL migrations — baseline + incremental, DDL-only (no seed data)
  runtime/                   # EventBus, DataServer, driver framework, plugin loader (lib tinyiothub_runtime)
  web/                       # HTTP infrastructure: ApiResponseBuilder, middleware (lib tinyiothub_web)
  scheduler/                 # Cron engine + scheduler (lib tinyiothub_scheduler)
  llm/                       # LLM provider contract, prompt, session (lib tinyiothub_llm)
  memory/                    # Agent memory engine — MemoryService 持久化引擎 (lib tinyiothub_memory)
  policy/                    # Policy gate evaluation — pure logic (lib tinyiothub_policy)
  skills/                    # Skill/tool registries, trust engine (lib tinyiothub_skills)
  agent/                     # Agent 共性能力运行时 (lib tinyiothub_agent；零 axum/零 sqlx/零存储依赖)
  authn/                     # 认证机制 — JWT/SSE token/密码哈希 (lib tinyiothub_authn；纯机制零 HTTP)
  plugin-sdk/                # Driver-author SDK (package plugin-sdk, lib tinyiothub_plugin_sdk)
  macros/                    # Proc macros (lib tinyiothub_macros)
drivers/                     # Dynamic driver stubs (NOT workspace members; cdylib)
web/                          # Lit 3 + Vite frontend
  src/ui/                    # Web Components (pages + components)
  src/ui/tool-views/         # viewKey → tool presentation (device, alarm, events, …)
  src/api/                   # API client layer
  src/stores/                # nanostore state management
  src/i18n/                  # Internationalization
  src/styles/                # CSS styles
.github/                     # CI, issue/PR templates
docs/                        # Technical docs, guides, specs
```

### Domain Module Structure (apps/cloud/src/domains/<domain>/)

```
domains/<domain>/
  mod.rs       # Module docs + pub items
  handler.rs   # HTTP handlers (call service, return ApiResponse; State<AppState> direct)
  service.rs   # Business logic (services/ for multi-service domains)
  dto.rs       # API-only shapes (Response DTOs, view models — never DB rows)
```

Hard rules:
- Handlers never write SQL — data access goes through the `Db` facade (`state.db.<method>(...)`).
- DB rows and query functions live in `crates/db/src/<domain>.rs`; domains never redefine them.
- Cross-domain calls are plain module calls (`crate::domains::alarm::...`) — no hooks, no ports, no edges.

### Harness write ownership

Each Harness fact has exactly one writer. New code must use these paths:

| Fact | Owner |
|------|-------|
| Session events | `Db::append_session_event` / `Db::apply_workspace_transition` |
| Effects | Harness transitions in `crates/db/src/harness_effect.rs` |
| Tasks / Runs / Wakes (scheduling) | The owning workspace generation: `crates/db/src/harness_task.rs` (TaskStore port) plus the Task runtime in `apps/cloud/src/domains/agent/thing_agent/task/coordinator.rs`; `harness_effect.rs` and `harness_command.rs` own Effect and decision-command transitions |
| Task pause / resume / cancel | Retained Task-control writer `crates/db/src/harness_workflow.rs` (`pause_workflow_task` / `resume_workflow_task` / `cancel_workflow_task`), called by `apps/cloud/src/domains/agent/runtime/controls.rs` |
| Device jobs | The owning workspace generation: `crates/db/src/harness_effect.rs` / `harness_job.rs` |
| Approvals / decisions | `decisions` (workspace DB) |
| Commands / wakes | Workspace `commands` and `task_wake_outbox` |

Platform runtime tables are retired legacy in the platform database (Task 19C4). `agent_tasks`,
`task_wake_outbox`, `device_jobs`, and `task_approvals` have **no production
reader or writer**; they remain only as schema for pre-Harness generations and
test fixtures. `crates/db/src/device_job.rs` keeps its platform Job writer
under `#[cfg(any(test, feature = "testing"))]` for parity fixtures only.
`crates/db/src/agent_runs.rs` still owns the autonomous agent-loop run history
(`trigger_type`/`outcome` rows), which is not the Task plane; `policy.rs` and
`ticket.rs` read that same history. That reconcile no longer joins
`agent_tasks`/`task_wake_outbox`/`task_finalize_intents` and writes no
`runtime.v1` Run lifecycle event; Task/Run/Wake crash recovery is
`Db::harness_interrupt_unfinished_runs` in the workspace generation.

Unapproved autonomy is **Job-only** (L3): an auto-approved
`require_confirm=false` `invoke_action` submits a workspace Job directly and
does not create a Prepared Effect. Confirmed actions do traverse
`effect_intents`. Treat "every device effect goes through the Harness Effect
state machine" as the target, not as a current invariant.

`hub_ledger` (`runtime.v1`) remains telemetry: watch-rule, media-evidence and
device-read facts only. No Task/Run/Job lifecycle may be written there — those
are workspace Harness events. `scripts/guards/legacy-writers.sh` enforces the
boundary with a production-scope scan. A lexical scanner masks strings, raw
strings, char literals and comments, then skips every `#[cfg(test)]` item or
statement for its own scope: block items through their balanced braces,
`name!()/[]/{}` macro items through their own delimiter, multi-line statements
to their terminating `;`, and `if`/`else` chains through the whole construct.
Production code after a test module is still checked. Its
allow map is per file and per table: any production INSERT/UPDATE/DELETE or
SELECT against `agent_tasks`, `agent_runs`, `task_wake_outbox`,
`task_approvals`, `task_finalize_intents` or `device_jobs` outside the exact
Harness owner for that table fails, including quoted and multi-line SQL. It
also rejects new frontend tool/event/status allowlists (status copy is
single-sourced in `web/src/ui/components/global-hud.ts` and
`web/src/ui/chat/timeline-render.ts`).

### Thing Ontology Module

The `thing` domain module (`apps/cloud/src/domains/thing/`) manages the thing (物) management plane:

```
domains/thing/
  types.rs                       # ThingType, SummaryStatus, DTOs (ThingResponse, ThingTreeNode, etc.)
  errors.rs                      # ThingError → HTTP status codes
  summary.rs                     # SummaryComputer: dirty markers, single-flight, LLM fencing
  # (持久化已收编 crates/db：thing.rs / thing_template.rs，经 Db 门面调用)
  service/
    mod.rs                       # ThingService: list/get/profile/tree CRUD + resource attach
    import_export.rs             # DTDL/WoT Thing Description import/export
  handler/
    mod.rs                       # Router at /api/v1/things
    crud.rs                      # CRUD + ontology + profile + tree handlers
    actions.rs                   # invoke/confirm endpoints (invoke_action confirmation)
    import_export.rs             # DTDL/WoT import/export HTTP handlers
    resources.rs                 # Resource attach/detach + unassigned list
```

**Key design decisions:**
- `devices` table IS the `things` table (not a separate table). Device is the default `thing_type`.
- `thing_type` distinguishes device/space/line/building. Non-device things have no connection state.
- `parent_id` forms a hierarchical tree (RESTRICT on delete). `tags` provide flat multi-dimensional classification.
- LLM summary is lazily computed on read (10s timeout, single-flight dedup, `<user_document>` fencing).
- All name lookups are workspace-scoped via expression index `COALESCE(workspace_id, ''), name`.
- Agent consumes things entirely through tools (no system prompt injection).

**Removed:** Knowledge graph (`knowledge_entities`, `knowledge_relations`, `knowledge_parse_jobs`), `resources` table (→ `thing_resources`), `products` table (→ `thing_templates`), `real_time_events`/`lost_events`/`event_performance_metrics` (→ unified `events` table).

## Risk Tiers

- **Low risk**: docs only, pure chore/ci changes without behavior impact, test-only changes
- **Medium risk**: most `apps/cloud/src/domains/*/service.rs` and `apps/cloud/src/domains/*/handler.rs` behavior changes, `web/src/ui/**` component changes, `web/src/stores/**` state changes
- **High risk**: `apps/cloud/src/state.rs`/`router.rs`（组合根）, `crates/db/**`, `crates/db/migrations/**`, `crates/core/src/**` (contract changes ripple everywhere), `crates/web/src/**`, `.github/workflows/**`, JWT/session boundary code (`apps/cloud/src/domains/auth/**`), `crates/agent/**`（AI agent 运行时，安全敏感）与 `apps/cloud/src/domains/agent/**`（agent 域 handler/数据实现，运行时已迁入 crates/agent）

When uncertain, classify as higher risk.

## Workflow

1. **Read before write** — inspect existing module structure, shared/ components, and adjacent tests before creating new code.
2. **Search first** — check `crates/web/` infra, existing domain crates, `web/src/api/`, `web/src/stores/` before creating anything new.
3. **One concern per PR** — avoid mixed feature+refactor+infra patches.
4. **Implement minimal patch** — no speculative abstractions, no config keys without a concrete use case.
5. **Validate by risk tier** — docs-only: lightweight checks. Code changes: full `just ci`.
6. **Surgical changes only** — touch only what you must. Don't "improve" adjacent code, comments, or formatting.
7. **Queue hygiene** — stacked PR: declare `Depends on #...`. Replacing old PR: declare `Supersedes #...`.

Branch/commit/PR rules:
- Work from a non-`main` branch. Open a PR to `main`; do not push directly.
- Use conventional commit titles: `type(scope): description` (types: feat, fix, test, chore, docs, refactor, style, perf, ci, build).
- Prefer small PRs.
- Follow `.github/pull_request_template.md` fully.
- Never commit secrets, personal data, or real identity information.

## Anti-Patterns

### Structural (enforced by CI architecture checks — guard scripts live in `scripts/guards/` with deliberate-violation selftests)

- Do not create modules without searching existing domain crates and `crates/web/` for reusable components first.
- Do not use `dto.rs` naming (use `types.rs`; `apps/cloud/src/domains/marketplace/dto.rs` is a grandfathered external-API contract exception).
- Do not create `application/` subdirectories in domain crates (use `service.rs`).
- Do not create scatter-shot `utils/` or `helpers/` in `apps/cloud/src/` or any crate.
- Do not call `fetch()` directly in front-end components (must use `web/src/api/` layer).
- Do not write SQL outside `crates/db` (production `sqlx::query` in `apps/*/src` fails the CI SQL-residence guard; use the `Db` facade).
- Do not bypass `ApiResponseBuilder` — all responses must use the standard `{ code, msg, result }` format.

### Code quality

- Do not add heavy dependencies for minor convenience.
- Do not add speculative config/feature flags "just in case".
- Do not mix massive formatting-only changes with functional changes.
- Do not modify unrelated modules "while here".
- Do not bypass failing checks without explicit explanation.
- Do not hide behavior-changing side effects in refactor commits.
- Do not suppress unused production code with underscore prefixes or `#[allow(dead_code)]`; delete it. Reserve underscore names for intentionally unused trait/callback parameters.
- Do not leave `unwrap()` / `expect()` in production paths; propagate errors or document the invariant.
- Do not include personal identity or sensitive information in test data, examples, docs, or commits.

### AI-specific

- Do not create planning/decision/analysis documents unless asked.
- Do not add comments explaining WHAT code does (well-named identifiers already do that).
- Do not add error handling for scenarios that can't happen.

## API Conventions

- **Path prefix**: `/api/v1/`
- **Response format**: `{ "code": 0, "msg": "", "result": T | null }` — use `ApiResponseBuilder` from the `web` crate (`tinyiothub_web`)
- **Naming**: RESTful, snake_case in Rust, camelCase in TypeScript
- **Auth**: JWT + session management via Tower middleware

## Naming Conventions

| Context | Format | Example |
|---------|--------|---------|
| Rust files/modules | snake_case | `device_service.rs` |
| Rust structs/enums | PascalCase | `DeviceStatus` |
| Rust functions | snake_case | `get_device_by_id` |
| TypeScript files | kebab-case | `device-list.ts` |
| Lit component classes | PascalCase | `DeviceList` |
| Custom element names | kebab-case | `<device-list>` |
| TypeScript variables | camelCase | `deviceData` |
| nanostore atoms | `$` prefix | `$currentRoute` |

## Frontend Development (Lit 3 + nanostore)

- **API calls**: Must go through `web/src/api/` layer; no direct `fetch()` in components
- **State management**: nanostore — save `subscribe()` return value, clean up in `disconnectedCallback()`
- **Lifecycle**: Data loading in `firstUpdated()`, cleanup in `disconnectedCallback()`
- **Routing**: Use `navigate()` function, never `window.location` directly
- **Shadow DOM**: Use `:host` selector; global CSS does not penetrate Shadow DOM
- **Type definitions**: `web/src/types/` is single source of truth
- **Event listeners**: Use arrow function properties, never `.bind(this)`
- **Tool presentation**: render tool calls through `web/src/ui/tool-views/` and the event `viewKey`; never parse a tool result ad hoc in a view component

## Async & Data Access (Rust)

- All I/O must be `async/await` (`tokio::fs`, `tokio::net`); no blocking code in async fn
- Database access must go through the `Db` facade in `crates/db/src/` (buzz pattern, no SQL outside `crates/db`)
- Shared state uses `Arc<RwLock<T>>` or `DashMap`; never `Rc<RefCell<T>>`
- Migration files in `crates/db/migrations/`, named `YYYYMMDDHHMMSS_description.sql`, are DDL-only — no `INSERT INTO` (CI-enforced); seed data belongs in `crates/db/src/seed.rs`

## Design Docs

```
docs/superpowers/plans/   # AI-assisted architecture design
docs/superpowers/specs/   # AI-assisted detailed design
docs/designs/             # Historical design records (some superseded)
docs/api/                 # API documentation
docs/guide/               # User guide
```

## Database

- **SQLite** primary database
- **SQLx** (runtime-checked queries) for data access
- **migrations/** baseline + incremental, DDL-only

### Db 门面规则（crates/db，2026-08 整改后）

1. **Db 是唯一存储实例**：state 及各域 state 切片只持 `Arc<Db>`（`crates/db/src/database.rs`）。禁止在组合点另建 Repository struct / 第二套存储入口。业务层唯一调用形态：`state.db.<method>(...)`。
2. **领域文件三段式**（`crates/db/src/<domain>.rs`，平铺 + 领域前缀命名如 `find_device_properties` / `insert_agent_run`）：
   - ① Row 类型（`pub`，`FromRow`）
   - ② SQL 自由函数（`pub(crate)` —— crate 内唯一写 SQL 的地方）
   - ③ `impl Db` 委托方法（`pub`，分散在各领域文件；跨领域组合逻辑也写在这里）
3. **SQL 唯一住所是 crates/db**：cloud/edge 生产代码出现 `sqlx::query` 即 CI 失败（SQL-residence guard；测试与 `guard-exempt` 注释项豁免）。`Db::pool()` 保持 `pub` 仅供基础设施接线共享连接池，不是 SQL 出口。
4. **事务形态**：单语句函数接 `&SqlitePool`；多语句事务函数接 `&mut Transaction<'static, Sqlite>` 参数（buzz 先例），由 `Db::begin_transaction()` 开启。**不给门面方法开 `&mut Transaction` 后门**——事务内组合的整体迁入 db 领域函数。
5. **迁移 DDL-only**：`migrations/` = `20260819000001_baseline.sql`（全量 DDL 快照）+ 正常递增迁移；非 baseline 迁移出现 `INSERT INTO` 即 CI 失败（DDL-only guard）。
6. **种子两档**（`crates/db/src/seed.rs`，幂等）：`seed_system()` 生产必需行（admin/RBAC/内置模板/默认租户等），bootstrap 无条件调用；`seed_demo()` 演示设备/属性/命令，由配置 `[seed] demo_data` 开关（默认开）。种子一律走 seed.rs，不进迁移。
7. **testing feature**：`db` crate 的 `testing` feature 暴露测试夹具（`fixture_pool_with_db` / `fixture_pool_seeded`），cloud `dev-dependencies` 启用；`test_helpers::test_pool()` 直建基线（不跑迁移链）。
8. **edge 暂留 TODO（D6）**：edge 的自建本地表（`apps/edge/src/shared/storage.rs` 的 `ensure_devices_table` 等）是过渡形态——后期 edge 直接只复用 db baseline（删库重建，另立项）。

## Docker

- Multi-arch builds (linux/amd64 + linux/arm64)
- Docker Hub: `grong/tinyiothub`

## Pre-Commit Checklist

- [ ] Dependency direction correct? (no reverse dependency)
- [ ] Follows `types → service → handler` three-layer architecture?
- [ ] Uses `ApiResponseBuilder` for responses?
- [ ] Database access through the `Db` facade (`state.db.*`)?
- [ ] No blocking code in async fn?
- [ ] Corresponding tests exist?
- [ ] Searched existing domain crates / `crates/web/` to confirm no duplicate implementation?

## Dev-Operational Contracts

Protected files — consumed by AI coding skills and development tooling. Do not move, rename, or delete without updating all consuming skills and AGENTS.md:

| Protected file | Consuming skill / tool |
|---|---|
| `.github/pull_request_template.md` | `github-pr` — PR body structure |
| `.github/ISSUE_TEMPLATE/bug_report.yml` | `github-issue` — bug report fields |
| `.github/ISSUE_TEMPLATE/feature_request.yml` | `github-issue` — feature request fields |
| `FRONTEND_LAYERING_GUIDE.md` | `review` — frontend architecture check |
| `.github/workflows/ci.yml` | CI pipeline — architecture checks, commit message validation |
| `Justfile` | All skills — canonical command recipes |
