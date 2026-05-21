# Justfile — Convenient command runner for TinyIoTHub development
# https://github.com/casey/just

# Default recipe to display help
_default:
    @just --list

# Format all code
fmt:
    cargo fmt --all

# Check formatting without making changes
fmt-check:
    cargo fmt --all -- --check

# Run clippy lints (exclude zeroclaw external dep)
lint:
    cargo clippy --workspace --exclude zeroclaw --all-targets -- -D warnings

# Run all tests (exclude zeroclaw + doc tests for CI parity)
test:
    cargo test --workspace --exclude zeroclaw --lib --bins --tests

# Run only unit tests (faster)
test-lib:
    cargo test --workspace --exclude zeroclaw --lib

# Run the full CI quality gate locally
ci: fmt-check lint test
    @echo "✅ All CI checks passed!"

# Build in release mode
build:
    cargo build --release

# Build in debug mode
build-debug:
    cargo build

# Clean build artifacts
clean:
    cargo clean

# Check code without building
check:
    cargo check --workspace --all-targets

# Update dependencies
update:
    cargo update

# Run cargo audit for security vulnerabilities
audit:
    cargo audit

# Run cargo deny checks
deny:
    cargo deny check

# Frontend: install dependencies
web-install:
    cd web && pnpm install --frozen-lockfile

# Frontend: type check
web-typecheck:
    cd web && pnpm type-check

# Frontend: build
web-build:
    cd web && pnpm build

# Frontend: lint
web-lint:
    cd web && pnpm lint

# Frontend: test
web-test:
    cd web && pnpm --if-present test

# Full-stack CI gate
ci-full: ci web-typecheck web-lint web-build web-test
    @echo "✅ Full-stack CI passed!"
