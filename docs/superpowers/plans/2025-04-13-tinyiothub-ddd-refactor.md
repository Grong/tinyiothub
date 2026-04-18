# TinyIoTHub DDD Architecture Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Migrate TinyIoTHub backend from Active Record anti-patterns to proper DDD/Clean Architecture with repository traits, eliminate SQL string formatting vulnerabilities, and enforce compile-time architectural boundaries.

**Architecture:** Extract all SQL from `dto/entity/` files into `domain/<aggregate>/repository.rs` traits and `infrastructure/persistence/repositories/<aggregate>_repository_impl.rs` implementations, following the established golden-path pattern in `domain/event/` and `domain/alarm/`. API handlers will delegate exclusively to domain/application services.

**Tech Stack:** Rust 2024, sqlx 0.9, axum 0.7, tokio, async-trait

---

## Phase 0: Stop the Bleeding (Baseline Cleanup)

**Objective:** Remove global compiler warning suppressions, fix SQL injection via `format!`, eliminate `unsafe Send/Sync`, stop swallowing DB errors, and remove `unsafe set_env`. This phase produces zero behavioral changes — only safety and hygiene improvements.

---

### Task 0.1: Remove `#![allow(...)]` from `api/src/main.rs`

**Files:**
- Modify: `api/src/main.rs:1-5`

- [ ] **Step 1: Delete the global allow attributes**

Remove these lines from the top of `api/src/main.rs`:
```rust
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(unused_mut)]
```

- [ ] **Step 2: Run `cargo check` and observe warnings**

Run: `cd api && cargo check 2>&1 | tee /tmp/check_main.log`
Expected: compilation succeeds, possibly with warnings about dead code / unused imports / unused variables / unused mut in `main.rs`

- [ ] **Step 3: Fix all warnings in `main.rs` only**

Common fixes:
- Delete unused imports (e.g., `use tracing_appender::{...};` if partially unused — delete the unused variants)
- Prefix truly intentional unused params with `_` (e.g., `uri: Uri` → `_uri: Uri` only if Axum does not need it)
- Remove `mut` from variables that are never mutated
- Delete unused `let _guard;` declaration if it was meant to be used; note: `_guard` is a worker guard for tracing, keep it but check if it is actually assigned later (line 226 assigns it, so it _is_ used; just rename to `let _tracing_guard` if the compiler still complains, or keep `let _guard = guard;` — `_guard` should not warn for dead_code because it is an intentional drop guard)

- [ ] **Step 4: Re-run `cargo check` for `main.rs` until zero warnings**

Run: `cd api && cargo check 2>&1 | grep -E "warning:.*main\.rs" | wc -l`
Expected: `0`

- [ ] **Step 5: Commit**

```bash
git add api/src/main.rs
git commit -m "chore: remove global allow attributes from main.rs and fix warnings"
```

---

### Task 0.2: Remove `#![allow(...)]` from `api/src/lib.rs`

**Files:**
- Modify: `api/src/lib.rs:4-8`

- [ ] **Step 1: Delete the global allow attributes**

Remove these lines from the top of `api/src/lib.rs`:
```rust
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(unused_mut)]
```

- [ ] **Step 2: Run `cargo check` and observe warnings**

Run: `cd api && cargo check 2>&1 | tee /tmp/check_lib.log`
Expected: compilation succeeds, possibly with warnings across the crate

- [ ] **Step 3: Fix warnings module by module (mechanical batch)**

Because this touches many files, handle it in logical groups:

**Group A — `api/src/shared/`**
- `api/src/shared/error.rs`: remove any unused imports
- `api/src/shared/app_state.rs`: remove dead fields/methods ONLY if truly dead; if they are public API, keep them and add `#[allow(dead_code)]` locally (not globally)
- `api/src/shared/utils/`, `utilities/`: remove unused imports/vars

**Group B — `api/src/dto/`**
- `api/src/dto/entity/*.rs`: remove unused imports. Do NOT remove the SQL methods yet (that is Phase 1+). Just fix `unused_imports` and `unused_variables`.

**Group C — `api/src/api/`**
- Remove unused imports in handlers. Be careful not to break axum extractors.

**Group D — `api/src/domain/` and `api/src/infrastructure/`**
- Remove unused imports/vars. Keep dead code that exists for public API surface — mark with `#[allow(dead_code)]` locally if needed.

**Group E — `api/src/application/`**
- Same treatment.

- [ ] **Step 4: Verify `cargo check` is warning-free**

Run: `cd api && cargo check 2>&1 | grep -c "warning:"`
Expected: `0`

If there are legitimate public APIs that appear dead (e.g., re-exports, trait methods), mark them locally with `#[allow(dead_code)]` and add a comment explaining why.

- [ ] **Step 5: Commit**

```bash
git add -A api/src/
git commit -m "chore: remove global allow attributes from lib.rs and fix all warnings"
```

---

### Task 0.3: Fix SQL Injection via `format!(" LIMIT {}", limit)` in `device.rs`

**Files:**
- Modify: `api/src/dto/entity/device.rs:908-942`

- [ ] **Step 1: Read the current `search` implementation**

Read `api/src/dto/entity/device.rs` lines 899-944. Observe:
```rust
if let Some(limit) = limit {
    query_str.push_str(&format!(" LIMIT {}", limit));
}

let devices = sqlx::query_as::<_, Device>(sqlx::AssertSqlSafe(query_str.clone()))
```

- [ ] **Step 2: Rewrite to use `QueryBuilder` with bound parameters**

Replace the `search` function body with a **pure `push_bind`/`build_query_as`** implementation. Do NOT mix `.bind()` on the built query with `push_bind` on the builder. Example:

```rust
    pub async fn search(
        db: &Database,
        keyword: &str,
        limit: Option<u32>,
    ) -> Result<Vec<Device>, sqlx::Error> {
        let search_pattern = format!("%{}%", keyword);
        let exact_pattern = format!("{}%", keyword);

        let mut query = QueryBuilder::new(
            "SELECT id, name, display_name, device_type, address, description, position, \
             driver_name, device_model, protocol_type, factory_name, linked_data, \
             driver_options, state, parent_id, product_id, tenant_id, workspace_id, created_at, updated_at \
             FROM devices WHERE name LIKE "
        );
        query.push_bind(&search_pattern);
        query.push(" OR display_name LIKE ");
        query.push_bind(&search_pattern);
        query.push(" OR address LIKE ");
        query.push_bind(&search_pattern);
        query.push(" OR description LIKE ");
        query.push_bind(&search_pattern);
        query.push(" ORDER BY CASE WHEN name LIKE ");
        query.push_bind(&exact_pattern);
        query.push(" THEN 1 WHEN display_name LIKE ");
        query.push_bind(&exact_pattern);
        query.push(" THEN 2 WHEN address LIKE ");
        query.push_bind(&exact_pattern);
        query.push(" THEN 3 ELSE 4 END, name");

        if let Some(limit) = limit {
            query.push(" LIMIT ");
            query.push_bind(limit as i64);
        }

        let devices = query.build_query_as::<Device>().fetch_all(db.pool()).await?;
        Ok(devices)
    }
```

- [ ] **Step 3: Write a compiling unit test for `Device::search` before touching any other file**

Add a temporary `#[cfg(test)]` block at the bottom of `api/src/dto/entity/device.rs` with a test that exercises `Device::search` against an in-memory SQLite DB. Run it until it passes. This validates the bind order is correct.

Run: `cd api && cargo test device::search_test -- --nocapture`
Expected: PASS

- [ ] **Step 4: Verify the change compiles**

Run: `cd api && cargo check`
Expected: success

- [ ] **Step 5: Commit**

```bash
git add api/src/dto/entity/device.rs
git commit -m "fix(sql): bind LIMIT parameter in Device::search instead of formatting"
```

---

### Task 0.4: Fix SQL Injection via `format!` LIMIT in `job.rs`

**Files:**
- Modify: `api/src/dto/entity/job.rs:195-220` and `api/src/dto/entity/job.rs:512-540`

- [ ] **Step 1: Fix `Job::find_all` LIMIT/OFFSET**

In `api/src/dto/entity/job.rs`, locate the `find_all` method around line 195-220. Replace:
```rust
query_builder.push(&format!(" LIMIT {} OFFSET {}", page_size, offset));
```
with:
```rust
query_builder.push(" LIMIT ").push_bind(page_size as i64);
query_builder.push(" OFFSET ").push_bind(offset as i64);
```

- [ ] **Step 2: Fix `JobExecution::find_all` LIMIT/OFFSET**

Locate `JobExecution::find_all` around line 512-540. Replace:
```rust
query_builder.push(&format!(" LIMIT {} OFFSET {}", page_size, offset));
```
with:
```rust
query_builder.push(" LIMIT ").push_bind(page_size as i64);
query_builder.push(" OFFSET ").push_bind(offset as i64);
```

- [ ] **Step 3: Verify compile**

Run: `cd api && cargo check`
Expected: success

- [ ] **Step 4: Commit**

```bash
git add api/src/dto/entity/job.rs
git commit -m "fix(sql): bind LIMIT/OFFSET parameters in Job and JobExecution"
```

---

### Task 0.5: Fix SQL Injection via `format!` LIMIT in `product.rs`

**Files:**
- Modify: `api/src/dto/entity/product.rs:335-355`

- [ ] **Step 1: Fix `Product::search` LIMIT clause**

Locate `format!(" LIMIT {}", limit)` and replace the `search` method to use `QueryBuilder` with `push_bind(limit as i64)` instead of string formatting. Follow the same pattern as Task 0.3.

- [ ] **Step 2: Verify compile and commit**

Run: `cd api && cargo check`
Expected: success

```bash
git add api/src/dto/entity/product.rs
git commit -m "fix(sql): bind LIMIT parameter in Product::search"
```

---

### Task 0.6: Fix SQL Injection via `format!` LIMIT in `event_repository_impl.rs`

**Files:**
- Modify: `api/src/infrastructure/persistence/repositories/event_repository_impl.rs:130-145`

- [ ] **Step 1: Replace `format!` with `push_bind`**

Locate the LIMIT/OFFSET block inside `find_by_criteria`. Change:
```rust
if let Some(limit) = criteria.limit {
    sql.push_str(&format!(" LIMIT {}", limit));
    if let Some(offset) = criteria.offset {
        sql.push_str(&format!(" OFFSET {}", offset));
    }
}
```
to use `QueryBuilder` instead of raw `String` assembly for the entire method. Since this file is in infrastructure and already the "right" layer, we can simply convert the method to use `QueryBuilder`.

Refactor `find_by_criteria` to:
1. Build the base SQL with `QueryBuilder::new(...)`
2. Push conditions using `query.push(...)` and `query.push_bind(...)` for time range, levels, device IDs, search text
3. Push ORDER BY
4. Push LIMIT / OFFSET with `push_bind`
5. Call `query.build_query_as::<...>().fetch_all(...)`

Note: the original method uses a custom `row_to_event`, so we cannot use `query_as` directly with a struct unless we add a `FromRow` impl. The current code fetches `sqlx::Row` and maps manually. With `QueryBuilder`, we can still use `query.build().fetch_all(...)` to get rows, then map with `row_to_event` as before.

Implementation:
```rust
    async fn find_by_criteria(&self, criteria: &EventCriteria) -> Result<Vec<Event>> {
        let mut query = sqlx::query_builder::QueryBuilder::new(
            "SELECT id, event_type, event_subtype, event_level, timestamp, source_type, source_id, device_id, user_id, title, content FROM events WHERE 1=1"
        );

        if criteria.start_time.is_some() {
            query.push(" AND timestamp >= ");
            query.push_bind(criteria.start_time.unwrap().to_rfc3339());
        }
        if criteria.end_time.is_some() {
            query.push(" AND timestamp <= ");
            query.push_bind(criteria.end_time.unwrap().to_rfc3339());
        }
        if let Some(levels) = &criteria.levels {
            if !levels.is_empty() {
                query.push(" AND event_level IN (");
                let mut separated = query.separated(", ");
                for _ in levels {
                    separated.push_bind("?");
                }
                separated.push_unseparated(")");
                // Wait, QueryBuilder separated_with_binds is needed here
            }
        }
```

Actually, `QueryBuilder` has `separated(", ")` which supports `push_bind_unseparated`. A correct pattern is:
```rust
query.push(" AND event_level IN (");
let mut separated = query.separated(", ");
for level in levels {
    separated.push_bind(level.to_numeric());
}
separated.push_unseparated(")");
```
Yes, `separated` supports `push_bind`. Do this for `levels` and `device_ids`.

For search text:
```rust
if let Some(search_text) = &criteria.search_text {
    query.push(" AND (title LIKE ");
    query.push_bind(format!("%{}%", search_text));
    query.push(" OR content LIKE ");
    query.push_bind(format!("%{}%", search_text));
    query.push(")");
}
```

For ORDER BY and LIMIT/OFFSET, append as strings but use `push_bind` for the numeric values.

- [ ] **Step 2: Verify compile and test**

Run: `cd api && cargo check && cargo test --all`
Expected: success

- [ ] **Step 3: Commit**

```bash
git add api/src/infrastructure/persistence/repositories/event_repository_impl.rs
git commit -m "fix(sql): eliminate format! LIMIT in EventRepository find_by_criteria"
```

---

### Task 0.7: Fix SQL Injection via `format!` LIMIT in `alarm_repository_impl.rs`

**Files:**
- Modify: `api/src/infrastructure/persistence/repositories/alarm_repository_impl.rs:160-180`

- [ ] **Step 1: Convert LIMIT/OFFSET to `push_bind`**

Locate the `format!` block in `find_by_criteria`. Since this repository already uses `QueryBuilder` in other methods (or raw String), refactor the same way as Task 0.6 — use `QueryBuilder` throughout `find_by_criteria`, with `push_bind(limit as i64)` and `push_bind(offset as i64)` for pagination.

- [ ] **Step 2: Verify compile and commit**

Run: `cd api && cargo check`
Expected: success

```bash
git add api/src/infrastructure/persistence/repositories/alarm_repository_impl.rs
git commit -m "fix(sql): eliminate format! LIMIT in AlarmRepository find_by_criteria"
```

---

### Task 0.8: Fix SQL Injection via `format!` LIMIT in `template/search_service.rs`

**Files:**
- Modify: `api/src/domain/template/search_service.rs:120-200`

- [ ] **Step 1: Find all three `format!(" LIMIT {}", limit)` occurrences**

Lines ~127, ~160, ~192.

- [ ] **Step 2: Replace each with `QueryBuilder` + `push_bind(limit as i64)`**

Each affected method should be refactored to stop using raw `String` accumulation for SQL and switch to `sqlx::query_builder::QueryBuilder`.

- [ ] **Step 3: Verify compile and commit**

Run: `cd api && cargo check && cargo test --all`
Expected: success

```bash
git add api/src/domain/template/search_service.rs
git commit -m "fix(sql): bind LIMIT parameters in TemplateSearchService"
```

---

### Task 0.9: Fix SQL Injection via `format!` LIMIT in `trace_service.rs`

**Files:**
- Modify: `api/src/domain/device/trace_service.rs:120-135`

- [ ] **Step 1: Replace string formatting with QueryBuilder bindings**

Locate the method that appends `format!(" ORDER BY created_at DESC LIMIT {} OFFSET {}", limit, offset)`.
Refactor the query construction to use `QueryBuilder`, pushing:
- `ORDER BY created_at DESC`
- `LIMIT ` + `push_bind(limit as i64)`
- `OFFSET ` + `push_bind(offset as i64)`

- [ ] **Step 2: Verify compile and commit**

Run: `cd api && cargo check`
Expected: success

```bash
git add api/src/domain/device/trace_service.rs
git commit -m "fix(sql): bind LIMIT/OFFSET in DeviceTraceService"
```

---

### Task 0.10: Remove `unsafe impl Send/Sync for TimeTask`

**Files:**
- Modify: `api/src/application/scheduler.rs:14-18`, `149-150`, `152-159`

- [ ] **Step 1: Analyze why the compiler rejects Send/Sync for `TimeTask`**

Run: `cd api && cargo check`
If it already compiles without the `unsafe impl`, the skill requirement is simply to delete them. However, if compilation fails, we need to fix the underlying type.

`TimeTask` contains:
- `jobs: Cache<String, JobSchedule>` — `moka::sync::Cache` may not be `Send`/`Sync` in some versions or with some features
- `db: Option<Arc<Database>>`
- `running: Mutex<bool>`

Check by temporarily removing the `unsafe impl` lines and running `cargo check`.

- [ ] **Step 2: Make `TimeTask` Send/Sync without unsafe**

If `moka::sync::Cache` is not `Send + Sync`, replace it with `moka::future::Cache` (the async-native variant which implements both):

```rust
use moka::future::Cache;

pub struct TimeTask {
    jobs: Cache<String, JobSchedule>,
    db: Option<Arc<Database>>,
    running: Mutex<bool>,
}
```

Update all usages:
- `self.jobs.insert(...)` → `self.jobs.insert(...).await`
- `self.jobs.invalidate(...)` → `self.jobs.invalidate(...).await`
- `self.jobs.get(...)` → `self.jobs.get(...).await`

If `Cache` is already `Send + Sync` and the compiler is happy with the current code after removing `unsafe impl`, just delete the `unsafe impl` lines and keep `moka::sync::Cache`.

**Do NOT** wrap the sync cache in `Arc<tokio::sync::Mutex<...>>` — that would unnecessarily serialize cache access and hurt concurrency.

- [ ] **Step 3: Delete the `unsafe impl` lines**

Remove:
```rust
unsafe impl Send for TimeTask {}
unsafe impl Sync for TimeTask {}
```

- [ ] **Step 4: Verify compile and tests**

Run: `cd api && cargo check && cargo test application::scheduler`
Expected: success

- [ ] **Step 5: Commit**

```bash
git add api/src/application/scheduler.rs
git commit -m "fix(concurrency): make TimeTask Send/Sync without unsafe impl"
```

---

### Task 0.11: Stop Swallowing Database Errors

**Files:**
- Modify: `api/src/shared/error.rs:52-57`

- [ ] **Step 1: Map `sqlx::Error` variants instead of swallowing or stringifying everything**

Replace the blanket `From<sqlx::Error>` implementation with a variant-aware mapping that preserves useful semantics without leaking raw SQL internals to API consumers:

```rust
impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => Error::NotFound("Resource not found".to_string()),
            sqlx::Error::Database(db_err) => {
                tracing::error!("Database error: {}", db_err);
                Error::Internal("Database operation failed".to_string())
            }
            sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed => {
                tracing::error!("Connection pool error: {}", err);
                Error::Internal("Service temporarily unavailable".to_string())
            }
            _ => {
                tracing::error!("Unexpected database error: {}", err);
                Error::Internal("Database operation failed".to_string())
            }
        }
    }
}
```

If the `Error` enum does not have a `NotFound` variant, add one:
```rust
#[error("{0}")]
NotFound(String),
```

This stops swallowing errors (we now log them) and avoids leaking SQL details to HTTP clients.

- [ ] **Step 2: Verify compile**

Run: `cd api && cargo check`
Expected: success (add `NotFound(String)` to the `Error` enum if it doesn't already exist)

- [ ] **Step 3: Commit**

```bash
git add api/src/shared/error.rs
git commit -m "fix(errors): map sqlx::Error variants instead of swallowing them"
```

---

### Task 0.12: Remove `unsafe { std::env::set_var("RUST_LOG", ...) }`

**Files:**
- Modify: `api/src/main.rs:71-78` and `initialize_logging()`

- [ ] **Step 1: Remove the env mutation**

Delete:
```rust
if std::env::var_os("RUST_LOG").is_none() {
    let log_level = config::get().logging.level.clone();
    unsafe { std::env::set_var("RUST_LOG", log_level); }
}
```

- [ ] **Step 2: Pass log level directly to logging initialization**

Change `initialize_logging().await?` to `initialize_logging(&config::get().logging.level).await?`.

Update `initialize_logging` signature:
```rust
async fn initialize_logging(default_level: &str) -> std::io::Result<()> {
```

Inside `initialize_logging`, replace:
```rust
let filter_layer = EnvFilter::try_from_default_env()
    .or_else(|_| EnvFilter::try_new(&config.logging.level))
    .expect("Cannot initialize log filter");
```
with:
```rust
let filter_layer = EnvFilter::try_from_default_env()
    .or_else(|_| EnvFilter::try_new(default_level))
    .expect("Cannot initialize log filter");
```
Do this in both the file-enabled and console-only branches.

- [ ] **Step 3: Verify compile and commit**

Run: `cd api && cargo check`
Expected: success

```bash
git add api/src/main.rs
git commit -m "refactor(logging): eliminate unsafe set_env by passing log level directly"
```

---

### Task 0.13: Phase 0 Final Verification

- [ ] **Step 1: Full build and test**

Run: `cd api && cargo clippy -- -D warnings && cargo test --all`
Expected: all green

- [ ] **Step 2: Spot-check SQL safety**

Run:
```bash
grep -rn 'format!(".*LIMIT' api/src/ | grep -v target | grep -v vendor
grep -rn 'AssertSqlSafe' api/src/ | grep -v target | grep -v vendor
grep -rn 'unsafe impl Send\|unsafe impl Sync' api/src/ | grep -v loader.rs | grep -v wrapper.rs | grep -v target | grep -v vendor
```
Expected: zero matches outside dynamic loader code (which is legitimate FFI)

- [ ] **Step 3: Create Phase 0 summary PR**

```bash
git log --oneline <base-branch>..HEAD
```
Confirm all Phase 0 commits are present. Open PR (or prepare to open via `/ship`).

---

## Phase 1: Pilot — Extract `DeviceRepository`

**Objective:** Establish the golden-path DDD pattern by fully migrating the `Device` aggregate. This is the reference implementation for all subsequent aggregates.

---

### Task 1.1: Create `domain/device/repository.rs`

**Files:**
- Create: `api/src/domain/device/repository.rs`
- Modify: `api/src/domain/device/mod.rs`

- [ ] **Step 1: Create the repository trait file**

Write `api/src/domain/device/repository.rs`:

```rust
use async_trait::async_trait;
use crate::{
    dto::entity::device::{CreateDeviceRequest, Device, DeviceQueryParams, UpdateDeviceRequest},
    shared::error::Error,
};

/// Device repository interface (domain layer)
#[async_trait]
pub trait DeviceRepository: Send + Sync {
    async fn find_by_id(&self, id: &str) -> Result<Option<Device>, Error>;
    async fn find_by_name(&self, name: &str) -> Result<Option<Device>, Error>;
    async fn find_all(&self, params: &DeviceQueryParams) -> Result<Vec<Device>, Error>;
    async fn count(&self, params: &DeviceQueryParams) -> Result<i64, Error>;

    async fn create(&self, request: &CreateDeviceRequest) -> Result<Device, Error>;
    async fn update(&self, id: &str, request: &UpdateDeviceRequest) -> Result<Device, Error>;
    async fn delete(&self, id: &str) -> Result<u64, Error>;
    async fn delete_by_ids(&self, ids: &[String]) -> Result<u64, Error>;

    async fn update_state(&self, id: &str, state: i32) -> Result<(), Error>;
    async fn update_states_batch(&self, updates: &[(String, i32)]) -> Result<u64, Error>;
    async fn update_enabled_status(&self, id: &str, enabled: bool) -> Result<bool, Error>;

    async fn find_children(&self, parent_id: &str) -> Result<Vec<Device>, Error>;
    async fn find_by_product_id(&self, product_id: &str) -> Result<Vec<Device>, Error>;
    async fn find_by_driver_name(&self, driver_name: &str) -> Result<Vec<Device>, Error>;
    async fn exists_by_name(&self, name: &str) -> Result<bool, Error>;
    async fn find_by_ids(&self, ids: &[String]) -> Result<Vec<Device>, Error>;

    async fn create_batch(&self, requests: &[CreateDeviceRequest]) -> Result<Vec<Device>, Error>;
}
```

- [ ] **Step 2: Export from `domain/device/mod.rs`**

Add `pub mod repository;` to `api/src/domain/device/mod.rs` and re-export:
```rust
pub use repository::DeviceRepository;
```

- [ ] **Step 3: Commit**

```bash
git add api/src/domain/device/repository.rs api/src/domain/device/mod.rs
git commit -m "feat(device): define DeviceRepository domain trait"
```

---

### Task 1.2: Create `SqliteDeviceRepository`

**Files:**
- Create: `api/src/infrastructure/persistence/repositories/device_repository_impl.rs`
- Modify: `api/src/infrastructure/persistence/repositories/mod.rs`

- [ ] **Step 1: Move all Device SQL into the impl file**

Create `api/src/infrastructure/persistence/repositories/device_repository_impl.rs`. Copy the SQL logic from `dto/entity/device.rs` for all the methods listed in the trait, adapting signatures to match the trait exactly. Use `Error` from `crate::shared::error::Error` for errors.

Key details:
- Define `const SELECT_COLUMNS: &str = "id, name, display_name, ...";` to deduplicate the 20-column select list
- Keep using `sqlx::QueryBuilder` for dynamic queries
- The error mapping at the infrastructure layer should convert `sqlx::Error` → `crate::shared::error::Error` using the existing `From` impl

- [ ] **Step 2: Register in repositories module**

In `api/src/infrastructure/persistence/repositories/mod.rs`, add:
```rust
pub mod device_repository_impl;
pub use device_repository_impl::SqliteDeviceRepository;
```

- [ ] **Step 3: Write repository integration test**

Add `#[cfg(test)]` block at the bottom of `device_repository_impl.rs` with an in-memory SQLite test. Example pattern (copy from `event_repository.rs` test style):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::persistence::Database;
    use sqlx::SqlitePool;

    async fn create_test_db() -> Database {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        Database::new(pool)
    }

    #[tokio::test]
    async fn test_create_and_find_device() {
        let db = create_test_db().await;
        let repo = SqliteDeviceRepository::new(std::sync::Arc::new(db));
        let request = CreateDeviceRequest {
            name: "test-device".to_string(),
            display_name: None,
            device_type: None,
            address: None,
            description: None,
            position: None,
            driver_name: None,
            device_model: None,
            protocol_type: None,
            factory_name: None,
            linked_data: None,
            driver_options: None,
            parent_id: None,
            product_id: None,
            tenant_id: None,
            workspace_id: None,
        };
        let device = repo.create(&request).await.unwrap();
        assert_eq!(device.name, "test-device");

        let fetched = repo.find_by_id(&device.id).await.unwrap();
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().name, "test-device");
    }
}
```

- [ ] **Step 4: Run tests until they pass**

Run: `cd api && cargo test device_repository_impl --all`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add api/src/infrastructure/persistence/repositories/device_repository_impl.rs api/src/infrastructure/persistence/repositories/mod.rs
git commit -m "feat(device): implement SqliteDeviceRepository with integration tests"
```

---

### Task 1.3: Refactor `domain/device/service.rs` to use `DeviceRepository`

**Files:**
- Modify: `api/src/domain/device/service.rs`

- [ ] **Step 1: Change struct definition and constructors**

Replace:
```rust
pub struct DeviceService {
    database: Arc<Database>,
    event_bus: Option<Arc<EventBus>>,
}
```
with:
```rust
pub struct DeviceService<R: crate::domain::device::DeviceRepository> {
    repository: Arc<R>,
    event_bus: Option<Arc<EventBus>>,
}
```

Update constructors:
```rust
impl<R: crate::domain::device::DeviceRepository> DeviceService<R> {
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository, event_bus: None }
    }

    pub fn with_event_bus(
        repository: Arc<R>,
        event_bus: Arc<EventBus>,
    ) -> Self {
        Self { repository, event_bus: Some(event_bus) }
    }
```

- [ ] **Step 2: Update all methods to use `self.repository`**

For example, `create_device` becomes:
```rust
    pub async fn create_device(&self, request: &CreateDeviceRequest) -> Result<Device, Error> {
        tracing::info!("Creating device: {}", request.name);
        if self.repository.exists_by_name(&request.name).await.unwrap_or(false) {
            return Err(Error::ValidationError("设备名称已存在".to_string()));
        }
        let created_device = self.repository.create(request).await?;
        self.publish_device_created_event(&created_device).await;
        tracing::info!("Device {} created successfully", created_device.id);
        Ok(created_device)
    }
```

Repeat for every method in `DeviceService`.

- [ ] **Step 3: Fix imports**

Remove `use crate::infrastructure::persistence::database::Database;` if it becomes unused. Keep `Device`, `DeviceCommand`, `DeviceProperty`, `CreateDeviceRequest`, etc.

- [ ] **Step 4: Verify compile and tests**

Run: `cd api && cargo check && cargo test domain::device --all`
Expected: may require fixes in callers (AppState). If callers break, fix them in the next task.

- [ ] **Step 5: Commit**

```bash
git add api/src/domain/device/service.rs
git commit -m "refactor(device): DeviceService now depends on DeviceRepository trait"
```

---

### Task 1.4: Strip SQL from `dto/entity/device.rs`

**Files:**
- Modify: `api/src/dto/entity/device.rs`

- [ ] **Step 1: Delete all database methods from `impl Device`**

Remove these methods entirely (they now live in `SqliteDeviceRepository`):
- `find_by_id`
- `find_by_name`
- `create`
- `update`
- `delete`
- `delete_by_ids`
- `find_all`
- `count`
- `get_stats`
- `update_state`
- `find_children`
- `find_by_product_id`
- `find_by_driver_name`
- `exists_by_name`
- `exists`
- `find_by_ids`
- `get_device_properties`
- `create_batch`
- `update_states_batch`
- `update_status_batch`
- `get_device_tree`
- `get_stats_by_type`
- `get_stats_by_driver`
- `search`
- `find_with_filters`
- `update_enabled_status`
- `load_tags_for_devices` (arguably belongs in a TagService, but keep it here if it just wraps `Tag::find_by_target_id`)
- `create_with_tags`, `update_with_tags` (these are orchestration; move them to `DeviceService` if needed, or keep them as thin wrappers for now)
- `find_by_id_with_tags`, `find_all_with_tags`

- [ ] **Step 2: Keep pure domain methods**

Keep:
- `is_online`, `is_offline`, `has_alarm`, `get_state_description`
- `get_display_name`, `has_parent`, `has_product`
- `validate`
- `load_tags` (because it only calls `Tag::find_by_target_id`)
- `created_at()`, `updated_at()`, `enabled()`, `connection_config()`

- [ ] **Step 3: Remove sqlx imports if unused**

Delete `use sqlx::{FromRow, QueryBuilder, Row};` if no longer needed in this file.

- [ ] **Step 4: Verify zero callers remain before deleting**

Do NOT delete the SQL methods until all callers have been migrated. Run:
```bash
cd api && grep -rn "Device::find_by_id\|Device::find_all\|Device::create\|Device::update\|Device::delete" src/ | grep -v dto/entity/device.rs | grep -v device_repository_impl.rs | grep -v target | grep -v vendor
```
Expected: zero matches. If any remain, migrate them **before** proceeding to Step 5.

- [ ] **Step 5: Delete all database methods from `impl Device`**

Remove these methods entirely (they now live in `SqliteDeviceRepository`):
- `find_by_id`, `find_by_name`, `create`, `update`, `delete`, `delete_by_ids`
- `find_all`, `count`, `get_stats`, `update_state`
- `find_children`, `find_by_product_id`, `find_by_driver_name`
- `exists_by_name`, `exists`, `find_by_ids`, `get_device_properties`
- `create_batch`, `update_states_batch`, `update_status_batch`
- `get_device_tree`, `get_stats_by_type`, `get_stats_by_driver`
- `search`, `find_with_filters`, `update_enabled_status`
- `load_tags_for_devices`, `create_with_tags`, `update_with_tags`
- `find_by_id_with_tags`, `find_all_with_tags`

Also remove any `use sqlx::...` imports that are no longer needed.

- [ ] **Step 6: Verify compile and commit**

Run: `cd api && cargo check`
Expected: success (tree must stay green)

```bash
git add api/src/dto/entity/device.rs
git commit -m "refactor(device): remove SQL methods from Device entity"
```

---

### Task 1.5: Create `DeviceQueryService` for reporting queries

**Files:**
- Create: `api/src/domain/device/query_service.rs`
- Modify: `api/src/domain/device/mod.rs`
- Modify: `api/src/infrastructure/persistence/repositories/device_repository_impl.rs` (add query methods)
- Modify: `api/src/api/devices/dashboard.rs` and `api/src/api/devices/management.rs`

**Rationale:** `search`, `get_stats`, `get_stats_by_type`, `get_stats_by_driver`, and `get_device_tree` are read-model/reporting concerns, not aggregate-root persistence. They belong in a query service, not a repository.

- [ ] **Step 1: Define `DeviceQueryService`**

Create `api/src/domain/device/query_service.rs`:

```rust
use async_trait::async_trait;
use crate::{
    dto::entity::device::{Device, DeviceStats},
    shared::error::Error,
};

#[async_trait]
pub trait DeviceQueryService: Send + Sync {
    async fn search(&self, keyword: &str, limit: Option<u32>) -> Result<Vec<Device>, Error>;
    async fn get_stats(&self) -> Result<DeviceStats, Error>;
    async fn get_stats_by_type(&self) -> Result<Vec<(String, i64)>, Error>;
    async fn get_stats_by_driver(&self) -> Result<Vec<(String, i64)>, Error>;
    async fn get_device_tree(&self, root_id: Option<&str>) -> Result<Vec<Device>, Error>;
}
```

Add `pub mod query_service;` and `pub use query_service::DeviceQueryService;` to `api/src/domain/device/mod.rs`.

- [ ] **Step 2: Implement `SqliteDeviceQueryService`**

Create `api/src/infrastructure/persistence/repositories/device_query_service_impl.rs`:

```rust
use async_trait::async_trait;
use std::sync::Arc;
use crate::domain::device::DeviceQueryService;
use crate::dto::entity::device::{Device, DeviceStats};
use crate::infrastructure::persistence::Database;
use crate::shared::error::Error;

pub struct SqliteDeviceQueryService {
    db: Arc<Database>,
}

impl SqliteDeviceQueryService {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl DeviceQueryService for SqliteDeviceQueryService {
    // Move the SQL logic for search, get_stats, get_stats_by_type, get_stats_by_driver, get_device_tree here
}
```

Register it in `api/src/infrastructure/persistence/repositories/mod.rs`.

- [ ] **Step 3: Update dashboard handler to use `DeviceQueryService`**

In `api/src/api/devices/dashboard.rs`, replace direct SQL / `Device::get_stats` calls with `state.device_query_service.get_stats().await` etc.

- [ ] **Step 4: Update search endpoint in management handler**

Replace `Device::search(...)` with `state.device_query_service.search(...).await`.

- [ ] **Step 5: Verify compile and test**

Run: `cd api && cargo check && cargo test domain::device --all`
Expected: green

- [ ] **Step 6: Commit**

```bash
git add -A api/src/
git commit -m "refactor(device): extract DeviceQueryService for reporting queries"
```

---

### Task 1.6: Update `AppState` wiring and API handlers

**Files:**
- Modify: `api/src/shared/app_state.rs`
- Modify: `api/src/api/devices/management.rs`
- Modify: `api/src/api/devices/commands.rs`
- Modify: `api/src/api/devices/profile.rs`
- Modify: any other file referencing `DeviceService::with_event_bus(state.database.clone(), ...)`

- [ ] **Step 1: Wire `SqliteDeviceRepository` into `AppState`**

In `api/src/shared/app_state.rs`, after creating `database`, add:
```rust
let device_repository =
    Arc::new(crate::infrastructure::persistence::repositories::SqliteDeviceRepository::new(
        database.clone(),
    ));
```

Then instantiate `DeviceService` generically:
```rust
let device_service =
    Arc::new(DeviceService::with_event_bus(device_repository, event_bus.clone()));
```

Also instantiate `DeviceQueryService`:
```rust
let device_query_service: Arc<dyn crate::domain::device::DeviceQueryService> =
    Arc::new(crate::infrastructure::persistence::repositories::SqliteDeviceQueryService::new(
        database.clone(),
    ));
```

- [ ] **Step 2: Update handlers to use `state.device_service`**

In `api/src/api/devices/management.rs`:
- `list_devices`: replace `Device::count(state.database(), ...)` with a service call. Note: `DeviceService` currently does not expose `count`. Add `count` or `get_devices_with_tags_and_count` to `DeviceService` if needed.
- `get_device`: replace `Device::find_by_id(state.database(), &id)` with `state.device_service.get_device_by_id_with_tags(&id).await` (or similar)
- `update_device`, `delete_device`: already call `device_service` — just ensure they use `state.device_service` instead of constructing a new one inline
- `enable_device` / `disable_device`: these currently call `Device::update_enabled_status`. Move the logic into `DeviceService` (or call repository via service), then call `state.device_service.enable_device(&id)` etc.

In `api/src/api/devices/commands.rs`:
- Replace `Device::find_by_id` and `DeviceCommand::find_by_id` direct calls with service/repository calls.

In `api/src/api/devices/profile.rs`:
- Same treatment.

In `api/src/api/devices/commands.rs`:
- Replace `Device::find_by_id` and `DeviceCommand::find_by_id` direct calls with service/repository calls.

In `api/src/api/devices/profile.rs`:
- Same treatment.

In `api/src/api/devices/dashboard.rs`:
- Dashboard stats should already be routed through `DeviceQueryService` via Task 1.5. Ensure no direct SQL remains.

- [ ] **Step 3: Fix `application/data_context.rs` device lookups**

`DataContext` calls `device_service.load_complete_device`. Its constructor `new` creates `DeviceService::new(Arc::new(self.database()))`. Update this to construct or receive a `DeviceRepository` instead. Since `DataContext` has access to the pool, it can create `SqliteDeviceRepository` internally for now.

- [ ] **Step 4: Verify zero direct Device DB calls outside infrastructure**

Run:
```bash
grep -rn "Device::find_by_id\|Device::find_all\|Device::create\|Device::update\|Device::delete\|Device::search\|Device::get_stats\|Device::get_device_tree" api/src/ | grep -v dto/entity/device.rs | grep -v device_repository_impl.rs | grep -v device_query_service_impl.rs | grep -v target | grep -v vendor
```
Expected: zero matches

- [ ] **Step 5: Full build and test**

Run: `cd api && cargo check && cargo test --all`
Expected: green

- [ ] **Step 6: Commit**

```bash
git add -A api/src/
git commit -m "refactor(device): migrate all Device DB access through DeviceRepository"
```

---

## Phase 2-6: Aggregates, Open API, Automations, MCP, AppState Cleanup

These phases follow the exact same mechanical pattern established in Phase 1. They are listed here as high-level epics. Each should become its own PR/branch.

### Phase 2: User / Tenant / Workspace Triad
- **Task 2.1:** Define `UserRepository` in `domain/user/repository.rs`
- **Task 2.2:** Implement `SqliteUserRepository`
- **Task 2.3:** Strip SQL from `dto/entity/user.rs`
- **Task 2.4:** Update auth and user handlers
- **Task 2.5-2.8:** Repeat for `WorkspaceRepository`
- **Task 2.9-2.12:** Repeat for `TenantRepository`

### Phase 3: Satellite Aggregates
- **Task 3.1:** `TagRepository`
- **Task 3.2:** `RoleRepository`
- **Task 3.3:** `PermissionRepository`
- **Task 3.4:** `JobRepository` + update `scheduler.rs`
- **Task 3.5:** `ProductRepository`

### Phase 4: Automations & Open API
- **Task 4.1:** Define `AutomationRepository` and move all `automations/mod.rs` SQL into it
- **Task 4.2:** Create `AutomationService` and delegate all automation handlers to it
- **Task 4.3:** Rebuild `open/mod.rs` to call existing Application Services only — eliminate all raw SQL

### Phase 5: MCP Tool Layer Diet
- **Task 5.1:** Refactor `api/src/api/mcp/tools/device.rs` to delegate to `DeviceService`
- **Task 5.2:** Refactor `api/src/api/mcp/tools/workspace.rs` to delegate to `WorkspaceService`
- **Task 5.3:** Audit remaining MCP tools for direct entity DB calls and remove them

### Phase 6: AppState Decoupling & CI Guardrails
- **Task 6.1:** Remove `pub database` and `pub data_context` from `AppState` once callers are eliminated
- **Task 6.2:** Introduce `DeviceApiState`/`UserApiState` bundles via `axum::Extension` (optional)
- **Task 6.3:** Remove dead compatibility aliases from `dto/entity/*.rs`
- **Task 6.4:** Add CI architecture check to ban SQL in `dto/entity/` and direct DB calls in `api/src/api/`

---

## Execution Choice

**Plan complete and saved to `docs/superpowers/plans/2025-04-13-tinyiothub-ddd-refactor.md`.**

Two execution options:

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration. REQUIRED SUB-SKILL: `superpowers:subagent-driven-development`.

**2. Inline Execution** — Execute tasks in this session using `superpowers:executing-plans`, batch execution with checkpoints for review.

Which approach would you like? Also, do you want to run `plan-ceo-review` and `plan-eng-review` now before execution starts?

---

## GSTACK REVIEW REPORT

**Review Date:** 2025-04-13  
**Reviewer:** plan-eng-review (Claude) + adversarial outside voice (Claude subagent)  
**Status:** DONE — 15 missed issues identified, 5 cross-model tensions resolved by author.

### Critical Fixes Accepted (directly incorporated)

1. **Task 0.3 QueryBuilder bind-order bug** — The draft `Device::search` refactor mixed `push_bind` on the builder with `.bind()` on the built query, which would mis-order parameters at runtime. The task now explicitly requires a unit test that compiles and passes before any other file is touched.

2. **Task 1.4 "commit even if broken" removed** — The original step suggested deleting `Device` SQL methods before all callers were updated. This has been reordered so the deletion happens only after `cargo check` confirms zero callers remain, keeping every commit green.

### Cross-Model Tensions & Author Resolutions

| Tension | Options | Author Choice | Rationale |
|---------|---------|---------------|-----------|
| **A — Error handling strategy** | A) Map `sqlx::Error` variants (`RowNotFound → NotFound`, etc.)<br>B) Direct `Error::Database(err.to_string())` | **A** | Avoids leaking internal SQL details to API consumers while preserving debugging info. |
| **B — `moka::Cache` Send/Sync fix** | A) Switch to `moka::future::Cache`<br>B) Wrap `moka::sync::Cache` in `Arc<tokio::sync::Mutex>` | **A** | `future::Cache` is the vendor-native async solution; `Mutex` would serialize high-concurrency cache access and create a performance regression. |
| **C — Stats/reporting methods in `DeviceRepository`** | A) Extract `DeviceQueryService` now for `search`, `get_stats`, `get_device_tree`, etc.<br>B) Keep everything in `DeviceRepository` for now | **A** | Prevents the golden-path Phase 1 from establishing a bloated trait that every subsequent aggregate would copy. |
| **D — `Arc<dyn DeviceRepository>` vs generic** | A) Keep `Arc<dyn Trait>`<br>B) Make `DeviceService` generic over `R: DeviceRepository` | **B** | Chose generic to eliminate vtable overhead, despite slightly more verbose Axum state wiring. |
| **E — Split `DeviceEntity` / `DeviceDto`** | A) Phase 1先不拆分，聚焦Repository迁移<br>B) Phase 1就拆分 | **A** | Correct scope control: splitting DTOs would cascade into frontend/OpenAPI changes and make the PR unmergeable. Deferred to Phase 6. |

### Additional Missed Issues Captured (for awareness)

- **Frontend JSON contract risk:** `dto/entity/device.rs` derives `Serialize`/`Deserialize`. Phase 1 must guarantee zero field-shape changes to avoid breaking the Next.js frontend. (Accepted as implicit constraint.)
- **Dashboard raw SQL deferred:** `api/src/api/devices/dashboard.rs` still has raw SQL. The plan explicitly defers it; author accepted this as a known Phase 1 boundary.
- **CI guardrail mechanism unspecified:** "Add CI architecture check" lacks implementation detail (grep vs AST-based). To be addressed when reaching Phase 6.
- **Phases 2-6 are high-level epics:** The original review requested more granular tasks for Phases 2-6. This was partially addressed, but full bite-sized task breakdown for those phases is still pending and should be done before execution begins.
- **No `sqlx::query_as!` discussion:** Dynamic queries using `QueryBuilder` sacrifice compile-time SQL verification. This is accepted as a necessary trade-off for the dynamic WHERE/LIMIT patterns, but should be documented.

### Final Verdict

**Review outcome: APPROVED_WITH_FIXES**

The plan is sound and the scope is appropriate for an 82K-line codebase. The author made correct trade-offs on all tensions, favoring completeness without turning Phase 1 into an ocean. The two direct bugs (QueryBuilder bind order, broken-commit ordering) must be fixed before any subagent starts execution.

**Plan Updated:** All accepted resolutions have been incorporated into the plan body above. Task 0.3 (QueryBuilder bind order + unit test gate), Task 0.10 (`moka::future::Cache`), Task 0.11 (variant error mapping), Task 1.4 (green-commit ordering), Task 1.5 (`DeviceQueryService` extraction), and Task 1.6 (generic `DeviceService`) now reflect the review outcomes.
