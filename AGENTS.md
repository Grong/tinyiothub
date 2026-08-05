# AGENTS.md — TinyIoTHub

Cross-tool agent instructions for any AI coding assistant working on this repository.

## Commands

```bash
# Rust
cargo fmt --all -- --check
cargo clippy --workspace --exclude zeroclaw --all-targets -- -D warnings
cargo test --workspace --exclude zeroclaw --lib --bins --tests
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

TinyIoTHub is a Rust + Lit 3 SaaS IoT platform for managing things (物) — devices, spaces, buildings, production lines — with multi-protocol support (Modbus, ONVIF, SNMP, MQTT). Architecture is DDD + Clean Architecture with a multi-crate workspace. Things are organized in a hierarchical ontology with properties, events, actions, and knowledge resources.

**Tech stack**: Rust 2024, Tokio, Axum, Tower, SQLx + SQLite | Lit 3 + Vite + TypeScript, nanostore

### Dependency Direction (one-way, irreversible)

```
apps/* → domain crates (thing/auth/user/tenant/event/alarm/driver/notify/agent/mcp/admin)
       → runtime / db / web / llm / memory / policy / skills / scheduler → core
```

Allowed cross-domain edges only: driver→thing, notify→event, alarm→event, agent→{event,thing,tenant,policy,memory,skills,llm}, mcp→{alarm,agent}, user→tenant, auth→{user,tenant}.

| Crate (dir = package) | Lib name | Role | Forbidden |
|-------|------|------|-----------|
| `core` | `tinyiothub_core` | Contract traits + value types (DTO/error/config). Absorbed former `tinyiothub-error` and `tinyiothub-config` (`core::error`, `core::config`). **Guardrail: traits + value types only; no logic functions, no I/O. Every new type must justify why it does not belong in a domain crate.** | Logic functions, I/O, DB access |
| `db` | `tinyiothub_storage` | SQLite concrete implementations (buzz-style flat per-domain modules, no trait inversion) + `migrations/` | Depending on any crate except `core` |
| `runtime` | `tinyiothub_runtime` | EventBus, DataServer, driver framework, executors, `runtime::plugin` (loader/registry/sandbox, FFI glue) | Depending on web or domain crates |
| `web` | `tinyiothub_web` | HTTP middleware, ApiResponseBuilder, security extractors | Business logic |
| `scheduler` | `tinyiothub_scheduler` | Cron engine + scheduler | — |
| `llm` | `tinyiothub_llm` | LLM provider contract, prompt, session | — |
| `memory` | `tinyiothub_memory` | Agent memory store + reflection pipeline + knowledge | — |
| `policy` | `tinyiothub_policy` | Policy engine + proposals | — |
| `skills` | `tinyiothub_skills` | Skill/tool registries | — |
| `plugin-sdk` | `tinyiothub_plugin_sdk` | Driver-author SDK; ABI contract single source of truth | Depending on runtime/web |
| `macros` | `tinyiothub_macros` | Proc macros | — |
| `thing` | `tinyiothub_thing` | Thing ontology domain (+ template, tag, legacy device plane) | Depending on agent/mcp |
| `auth` | `tinyiothub_auth` | Auth/JWT domain | — |
| `user` | `tinyiothub_user` | User/role/permission domain (user→tenant) | — |
| `tenant` | `tinyiothub_tenant` | Tenant/workspace domain | — |
| `event` | `tinyiothub_event` | Event pipeline domain | — |
| `alarm` | `tinyiothub_alarm` | Alarm rules domain (alarm→event) | — |
| `driver` | `tinyiothub_driver` | Driver/gateway/plugin/heartbeat domain (driver→thing) | — |
| `notify` | `tinyiothub_notify` | Notification domain (notify→event) | — |
| `agent` | `tinyiothub_agent` | Agent loop + host + chat unified crate | — |
| `mcp` | `tinyiothub_mcp` | Embedded MCP server (mcp→{alarm,agent}) | — |
| `admin` | `tinyiothub_admin` | System/monitoring/batch/jobs/open domain (admin→scheduler) | — |
| `apps/cloud` (bin) | — | Application composition root: thin `main.rs` + `bootstrap.rs` + router assembly | Direct SQL in handlers; business logic |

**Naming rule**: crate directories and package names use short names (`core`, `db`, …); `[lib] name` is pinned to `tinyiothub_*` so `use tinyiothub_core::…` imports stay stable across directory moves.

**Forbidden dependencies**: core/db must not depend on runtime; no crate may reverse-depend upward.

## Stability Tiers

| Crate | Tier | Notes |
|-------|------|-------|
| `core` | Stable | Contract crate — includes former `tinyiothub-error` + `tinyiothub-config`; breaking changes require MAJOR version bump |
| `plugin-sdk` | Stable | Driver ABI contract — breaking changes require MAJOR version bump |
| `web` | Beta | HTTP infrastructure — breaking changes permitted in MINOR with changelog |
| `db` | Beta | SQLite implementation — schema changes require migration |
| `runtime` | Beta | EventBus, DataServer — breaking changes permitted in MINOR |
| `memory` | Beta | Agent memory store + reflection pipeline |
| `scheduler`, `llm`, `policy`, `skills` | Beta | Supporting crates extracted from `ai`/`runtime` |
| domain crates (`thing`…`admin`) | Experimental | Extracted from former `cloud/src/modules` — no stability guarantee yet |
| `agent` | Experimental | Agent loop + host + chat — under active development |
| `macros` | Experimental | Internal proc macros |
| `apps/*` | Experimental | Deployable binaries (cloud/edge/marketplace/cli) |

**Tiers**: Stable = covered by breaking-change policy. Beta = breaking changes permitted in MINOR with changelog notes. Experimental = no stability guarantee. Tiers are promoted, never demoted, through deliberate team decision.

## Repository Map

```
apps/
  cloud/                     # SaaS composition root (main binary)
    src/
      main.rs                # Thin entry (<200 lines): config → AppState → router → serve
      bootstrap.rs           # Startup logic (logging, driver rehydrate, device cache)
      api/                   # Router mounting + HTTP middleware (WorkspaceScope, auth)
      modules/marketplace/   # Marketplace client (HTTP DTO contract only)
      server.rs              # Axum server startup
      shared/                # Composition-layer glue (app_state, config, service_manager)
    templates/               # Skill templates
  edge/                      # Edge gateway binary
  marketplace/               # Marketplace service binary
  cli/                       # CLI binary
crates/
  core/                      # Contracts: traits + value types (lib tinyiothub_core; absorbed error+config)
  db/                        # Data: SQLite implementations, buzz flat per-domain (lib tinyiothub_storage)
    migrations/              # SQL migration files
  runtime/                   # Infrastructure: EventBus, DataServer, drivers, plugin loader (lib tinyiothub_runtime)
  web/                       # HTTP infrastructure: ApiResponseBuilder, middleware (lib tinyiothub_web)
  scheduler/                 # Cron engine + scheduler (lib tinyiothub_scheduler)
  llm/                       # LLM provider contract, prompt, session (lib tinyiothub_llm)
  memory/                    # Agent memory store + reflection pipeline (lib tinyiothub_memory)
  policy/                    # Policy engine + proposals (lib tinyiothub_policy)
  skills/                    # Skill/tool registries (lib tinyiothub_skills)
  plugin-sdk/                # Driver-author SDK (package plugin-sdk, lib tinyiothub_plugin_sdk)
  macros/                    # Proc macros (lib tinyiothub_macros)
  thing/                     # Thing ontology domain (+ template/tag/legacy device plane)
  auth/                      # Auth/JWT domain
  user/                      # User/role/permission domain
  tenant/                    # Tenant/workspace domain
  event/                     # Event pipeline domain
  alarm/                     # Alarm rules domain
  driver/                    # Driver/gateway/plugin/heartbeat domain
  notify/                    # Notification domain
  agent/                     # Agent loop + host + chat unified domain
  mcp/                       # Embedded MCP server domain
  admin/                     # System/monitoring/batch/jobs/open domain
drivers/                     # Dynamic driver stubs (NOT workspace members; cdylib)
web/                          # Lit 3 + Vite frontend
  src/ui/                    # Web Components (pages + components)
  src/api/                   # API client layer
  src/stores/                # nanostore state management
  src/i18n/                  # Internationalization
  src/styles/                # CSS styles
.github/                     # CI, issue/PR templates
docs/                        # Technical docs, guides, specs
```

### Domain Crate Structure (SEP — standard extraction procedure)

Each domain crate (`crates/<domain>/`) exposes:

```
crates/<domain>/src/
  types.rs     # Request/response structs (never dto.rs)
  service.rs   # Business logic (services/ for multi-service domains)
  handler/     # HTTP handlers (call service, return ApiResponse)
  lib.rs       # <Domain>State + router(); state sliced from AppState via FromRef
```

DB access lives in `crates/db/src/<domain>.rs` (concrete structs, buzz pattern — no trait inversion).

### Thing Ontology Module

The `thing` domain crate (`crates/thing/`) manages the thing (物) management plane:

```
crates/thing/src/
  types.rs                       # ThingType, SummaryStatus, DTOs (ThingResponse, ThingTreeNode, etc.)
  errors.rs                      # ThingError → HTTP status codes
  repo.rs                        # ThingRepo: CRUD, tree, breadcrumb, cycle detection
  summary.rs                     # SummaryComputer: dirty markers, single-flight, LLM fencing
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

- **Low risk**: docs only, `.kiro/specs/**`, pure chore/ci changes without behavior impact, test-only changes
- **Medium risk**: most `crates/<domain>/src/service.rs` and `crates/<domain>/src/handler/` behavior changes, `web/src/ui/**` component changes, `web/src/stores/**` state changes
- **High risk**: `apps/cloud/src/shared/**` (composition glue), `crates/db/**`, `crates/db/migrations/**`, `crates/core/src/**` (contract changes ripple everywhere), `crates/web/src/**`, `.github/workflows/**`, JWT/session boundary code (`crates/auth/**`), `crates/agent/**` (AI agent runtime has security implications)

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

### Structural (enforced by CI architecture checks)

- Do not create modules without searching existing domain crates and `crates/web/` for reusable components first.
- Do not use `dto.rs` naming (use `types.rs`; `modules/marketplace/dto.rs` is a grandfathered external-API contract exception).
- Do not create `application/` subdirectories in domain crates (use `service.rs`).
- Do not create scatter-shot `utils/` or `helpers/` in `apps/cloud/src/` or any crate.
- Do not call `fetch()` directly in front-end components (must use `web/src/api/` layer).
- Do not write SQL in API handlers (must use Repository pattern).
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

## Async & Data Access (Rust)

- All I/O must be `async/await` (`tokio::fs`, `tokio::net`); no blocking code in async fn
- Database access must go through the concrete repositories in `crates/db/src/` (buzz pattern, no SQL in handlers)
- Shared state uses `Arc<RwLock<T>>` or `DashMap`; never `Rc<RefCell<T>>`
- Migration files in `crates/db/migrations/`, named `YYYYMMDDHHMMSS_description.sql`, must be idempotent

## Design Docs

```
.kiro/steering/           # Development standards (naming, API, architecture)
.kiro/specs/              # Feature design documents
docs/superpowers/plans/   # AI-assisted architecture design
docs/superpowers/specs/   # AI-assisted detailed design
docs/api/                 # API documentation
docs/guide/               # User guide
```

## Database

- **SQLite** primary database
- **SQLx** for compile-time query verification
- **migrations/** for SQL migration files

## Docker

- Multi-arch builds (linux/amd64 + linux/arm64)
- Docker Hub: `grong/tinyiothub`

## Pre-Commit Checklist

- [ ] Dependency direction correct? (no reverse dependency)
- [ ] Follows `types → service → handler` three-layer architecture?
- [ ] Uses `ApiResponseBuilder` for responses?
- [ ] Database access through Repository?
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
| `.kiro/steering/` | `review` — naming/API/architecture standards |
| `FRONTEND_LAYERING_GUIDE.md` | `review` — frontend architecture check |
| `.github/workflows/ci.yml` | CI pipeline — architecture checks, commit message validation |
| `Justfile` | All skills — canonical command recipes |
