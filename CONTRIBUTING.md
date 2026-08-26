# Contributing to TinyIoTHub

## Prerequisites

- **Rust** 1.85+ stable (2024 Edition, via [rustup](https://rustup.rs/))
- **Node.js** 20+ and **pnpm** 9+
- **SQLite** (for local development)

## Quick Start

```bash
# Clone and enter the repo
git clone https://github.com/Grong/tinyiothub.git
cd tinyiothub

# Backend
cargo check --workspace
cargo test --workspace

# Frontend
cd web
pnpm install
pnpm dev
```

## Architecture

Read `AGENTS.md` before writing code. Key rules:

- **Dependency direction**: `apps/* → crates/* → core` (one-way, irreversible)
- **Module structure**: `types.rs → service.rs → handler/` (no `dto.rs`, no `application/`)
- **API responses**: Always use `ApiResponseBuilder` from `tinyiothub_web`
- **Database access**: Through the `Db` facade in `crates/db/` (`state.db.*` — all SQL lives in `crates/db`, CI-enforced)
- **No direct SQL** in handlers

## Branch Strategy

- `main` — production-ready code
- `saas` — SaaS cloud version
- Feature branches: `feature/<name>` or `fix/<name>`

## Commit Convention

```
<type>(<scope>): <description>

type: feat | fix | test | chore | docs | refactor | style | perf | ci | build
scope: module name (device, alarm, auth, etc.)
```

Examples:
- `feat(device): add temperature monitoring`
- `fix(auth): handle expired JWT tokens`

## Code Quality

Before submitting a PR:

```bash
# Rust
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test --workspace

# Frontend (if applicable)
cd web
pnpm type-check
pnpm lint
pnpm test
pnpm build
```

## Testing

- **Unit tests**: Add `#[cfg(test)] mod tests` in the same file
- **Integration tests**: Add to `apps/cloud/src/tests/`
- **Handler tests**: Use `tower::ServiceExt::oneshot()` with `setup_test_app()`

## Security

- Never commit secrets (`.env`, API keys, passwords)
- JWT secret must be set via environment variable in production
- All user input must be validated
- SQL queries must use parameterized statements

## Questions?

Open an issue or check the `docs/` directory for detailed documentation.
