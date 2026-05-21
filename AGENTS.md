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

TinyIoTHub is a Rust + Lit 3 SaaS IoT platform for managing edge gateway devices with multi-protocol support (Modbus, ONVIF, SNMP, MQTT). Architecture is DDD + Clean Architecture with a multi-crate workspace.

**Tech stack**: Rust 2024, Tokio, Axum, Tower, SQLx + SQLite | Lit 3 + Vite + TypeScript, nanostore

### Dependency Direction (one-way, irreversible)

```
cloud/ → runtime → core ← storage
```

| Crate | Role | Forbidden |
|-------|------|-----------|
| `tinyiothub-core` | Traits, domain models, repository interfaces, rule engine | No I/O, no DB access |
| `tinyiothub-runtime` | EventBus, DataServer, drivers, executors | No cloud/web dependency |
| `tinyiothub-storage` | SQLite implementations (re-export core traits) | No runtime/cloud dependency |
| `tinyiothub-web` | HTTP middleware, ApiResponseBuilder, security extractors | No business logic |
| `tinyiothub-error` | Error types with `thiserror` derives | — |
| `cloud` | Application orchestration, routing, business modules | No direct SQL in handlers |

**Forbidden dependencies**: core/storage must not depend on runtime; no crate may reverse-depend upward.

## Stability Tiers

| Crate | Tier | Notes |
|-------|------|-------|
| `tinyiothub-core` | Stable | Contract crate — breaking changes require MAJOR version bump |
| `tinyiothub-error` | Stable | Error types used across all crates |
| `tinyiothub-web` | Beta | HTTP infrastructure — breaking changes permitted in MINOR with changelog |
| `tinyiothub-storage` | Beta | SQLite implementation — schema changes require migration |
| `tinyiothub-runtime` | Beta | EventBus, DataServer — breaking changes permitted in MINOR |
| `cloud` | Experimental | SaaS application layer — under active development |

**Tiers**: Stable = covered by breaking-change policy. Beta = breaking changes permitted in MINOR with changelog notes. Experimental = no stability guarantee. Tiers are promoted, never demoted, through deliberate team decision.

## Repository Map

```
cloud/                       # SaaS application orchestration (main binary)
  src/
    api/                     # HTTP middleware (WorkspaceScope, auth)
    modules/                 # Business modules (types → service → handler)
      agent/                 # AI Agent (chat, config, tools, session, reflection, memory)
      device/                # Device CRUD + drivers
      alert/                 # Alert rules + notifications
      plugin/                # Plugin registry
      ...
    shared/                  # Cross-layer (persistence, security, error_handling, utils)
    server.rs                # Axum server startup
crates/
  tinyiothub-core/           # Contracts: traits + domain models + repository interfaces
  tinyiothub-runtime/        # Infrastructure: EventBus, DataServer, drivers, executors
  tinyiothub-storage/        # Data: SQLite implementations
  tinyiothub-web/            # HTTP infrastructure: ApiResponseBuilder, middleware
  tinyiothub-error/          # Error types
  tinyiothub-memory/         # Agent memory store + reflection pipeline
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
  service.rs   # Business logic
  handler/     # HTTP handlers (call service, return ApiResponse)
shared/        # Cross-module (persistence, security, middleware)
```

## Risk Tiers

- **Low risk**: docs only, `.kiro/specs/**`, pure chore/ci changes without behavior impact, test-only changes
- **Medium risk**: most `cloud/src/modules/*/service.rs` and `cloud/src/modules/*/handler/` behavior changes, `web/src/ui/**` component changes, `web/src/stores/**` state changes
- **High risk**: `cloud/src/shared/security/**`, `cloud/src/shared/persistence/**`, `cloud/migrations/**`, `crates/tinyiothub-core/src/**` (contract changes ripple everywhere), `crates/tinyiothub-web/src/**`, `.github/workflows/**`, JWT/session boundary code, `cloud/src/modules/agent/**` (AI agent runtime has security implications)

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
- **Response format**: `{ "code": 0, "msg": "", "result": T | null }` — use `ApiResponseBuilder` from `tinyiothub-web`
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
