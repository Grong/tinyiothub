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
cloud/ (bin) → runtime / db / web / ai / memory / plugin / macros → core
```

| Crate (dir = package) | Lib name | Role | Forbidden |
|-------|------|------|-----------|
| `core` | `tinyiothub_core` | Contract traits + value types (DTO/error/config). Absorbed former `tinyiothub-error` and `tinyiothub-config` (`core::error`, `core::config`). **Guardrail: traits + value types only; no logic functions, no I/O. Every new type must justify why it does not belong in a domain crate.** | Logic functions, I/O, DB access |
| `db` | `tinyiothub_storage` | SQLite concrete implementations. (Planned: buzz-style flat per-domain modules, no trait inversion — later task.) | Depending on any crate except `core` |
| `runtime` | `tinyiothub_runtime` | EventBus, DataServer, driver framework, executors, DLQ trait. (Planned: absorb `plugin` loader/registry/sandbox as `runtime::plugin` — P2.) | Depending on web or domain crates |
| `web` | `tinyiothub_web` | HTTP middleware, ApiResponseBuilder, security extractors | Business logic |
| `ai` | `tinyiothub_ai` | Agent loop, LLM orchestration. (Planned: split into llm/policy/skills/agent crates — P2+.) | — |
| `memory` | `tinyiothub_memory` | Agent memory store + reflection pipeline | — |
| `plugin` | `tinyiothub_plugin` | Plugin loader/registry/sandbox. (Planned: merge into `runtime::plugin` — P2.) | — |
| `plugin-sdk` | `tinyiothub_plugin_sdk` | Driver-author SDK; ABI contract single source of truth | Depending on runtime/web |
| `macros` | `tinyiothub_macros` | Proc macros | — |
| `cloud` (root bin) | — | Application orchestration, routing, business modules. (Planned: move to `apps/cloud` as a thin bin — later phase.) | Direct SQL in handlers |

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
| `ai` | Experimental | Agent loop + LLM orchestration — under active development |
| `plugin` | Experimental | Plugin loader/registry — planned merge into `runtime::plugin` (P2) |
| `macros` | Experimental | Internal proc macros |
| `cloud` | Experimental | SaaS application layer — under active development |

**Tiers**: Stable = covered by breaking-change policy. Beta = breaking changes permitted in MINOR with changelog notes. Experimental = no stability guarantee. Tiers are promoted, never demoted, through deliberate team decision.

## Repository Map

```
cloud/                       # SaaS application orchestration (main binary)
  src/
    api/                     # HTTP middleware (WorkspaceScope, auth)
    modules/                 # Business modules (types → service → handler)
      agent/                 # AI Agent (chat, config, tools, session, reflection, memory)
      device/                # Device connection runtime (drivers, telemetry, heartbeat)
      thing/                 # Thing ontology management (CRUD, hierarchy, ontology, resources, summary)
      event/                 # Event pipeline (router, throttle, real-time, SSE, retention job)
      alarm/                 # Alarm rules + notifications (rule_type='device' | 'event')
      plugin/                # Plugin registry
      ...
    shared/                  # Cross-layer (persistence, security, error_handling, utils)
    server.rs                # Axum server startup
crates/
  core/                      # Contracts: traits + value types (lib tinyiothub_core; absorbed error+config)
  db/                        # Data: SQLite implementations (lib tinyiothub_storage)
  runtime/                   # Infrastructure: EventBus, DataServer, drivers, executors (lib tinyiothub_runtime)
  web/                       # HTTP infrastructure: ApiResponseBuilder, middleware (lib tinyiothub_web)
  ai/                        # Agent loop + LLM orchestration (lib tinyiothub_ai)
  memory/                    # Agent memory store + reflection pipeline (lib tinyiothub_memory)
  plugin/                    # Plugin loader/registry/sandbox (lib tinyiothub_plugin; P2: → runtime::plugin)
  plugin-sdk/                # Driver-author SDK (package plugin-sdk, lib tinyiothub_plugin_sdk)
  macros/                    # Proc macros (lib tinyiothub_macros)
# Planned (P2+): domain crates (auth/thing/event/alarm/agent/…) extracted from cloud/,
# scheduler/ standalone crate, and apps/{cloud,edge,marketplace} thin binaries.
web/                          # Lit 3 + Vite frontend
  src/ui/                    # Web Components (pages + components)
  src/api/                   # API client layer
  src/stores/                # nanostore state management
  src/i18n/                  # Internationalization
  src/styles/                # CSS styles
.github/                     # CI, issue/PR templates
docs/                        # Technical docs, guides, specs
```

### cloud/ Module Structure

```
modules/<module>/
  types.rs     # Request/response structs (never dto.rs)
  service.rs   # Business logic (services/mod.rs for multi-service modules)
  handler/     # HTTP handlers (call service, return ApiResponse)
  repo.rs      # Database queries (Repository pattern, no SQL in handlers)
shared/        # Cross-module (persistence, security, middleware)
```

### Thing Ontology Module

The `thing` module (`cloud/src/modules/thing/`) manages the thing (物) management plane:

```
modules/thing/
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
- **Medium risk**: most `cloud/src/modules/*/service.rs` and `cloud/src/modules/*/handler/` behavior changes, `web/src/ui/**` component changes, `web/src/stores/**` state changes
- **High risk**: `cloud/src/shared/security/**`, `cloud/src/shared/persistence/**`, `cloud/migrations/**`, `crates/core/src/**` (contract changes ripple everywhere), `crates/web/src/**`, `.github/workflows/**`, JWT/session boundary code, `cloud/src/modules/agent/**` (AI agent runtime has security implications)

When uncertain, classify as higher risk.

## Workflow

1. **Read before write** — inspect existing module structure, shared/ components, and adjacent tests before creating new code.
2. **Search first** — check `cloud/src/shared/`, existing modules, crates, `web/src/api/`, `web/src/stores/` before creating anything new.
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

- Do not create modules without searching `cloud/src/shared/` for reusable components first.
- Do not use `dto.rs` naming (use `types.rs`).
- Do not create `application/` subdirectories in modules (use `service.rs`).
- Do not create scatter-shot `utils/` or `helpers/` in `cloud/src/` or any crate.
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
- Database access must go through Repository (`cloud/src/shared/persistence/repositories/`)
- Shared state uses `Arc<RwLock<T>>` or `DashMap`; never `Rc<RefCell<T>>`
- Migration files in `cloud/migrations/`, named `YYYYMMDDHHMMSS_description.sql`, must be idempotent

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
- [ ] Searched `shared/` to confirm no duplicate implementation?

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
