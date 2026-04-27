# TinyIoTHub Workspace 重构迁移计划

> **Goal:** 将 monolithic `cloud/` crate 拆分为 product-boundary crates，最终 `cloud/` 仅保留 binary 入口和服务器配置。

**Current branch:** `feature/workspace-refactor`
**Worktree:** `.worktrees/workspace-refactor`

---

## Phase 1: Skeleton Setup ✅

**Status:** DONE
**Commit range:** `57927e8f` ~ `d27d5ac3`

- Workspace root `Cargo.toml`
- Create `crates/tinyiothub-core`, `engine`, `storage`, `plugin`, `macros`
- Create `cloud/`, `edge/`, `marketplace/`, `cli/` binary crates
- Copy `api/` → `cloud/`
- Fix `vendor/onvif-rs` workspace conflict
- Add `deny.toml`
- Fix Rust 2024 `#[unsafe(no_mangle)]`

**Tag:** `phase1-skeleton`

---

## Phase 2: Extract tinyiothub-core ✅

**Status:** DONE
**Commit range:** `7fe4abdf` ~ `fa4db987`

### 已迁移到 core:
- `error.rs` → `crates/tinyiothub-core/src/error.rs`
- `config/` → `crates/tinyiothub-core/src/config/`
- `models/` (全部 entity DTOs) → `crates/tinyiothub-core/src/models/`
- `api_response.rs` → `crates/tinyiothub-core/src/models/api_response.rs`

### 验证:
```bash
cargo check -p tinyiothub-core    # passes
cargo check --workspace            # passes (warnings only)
```

**Tag:** `phase2-core-extracted`

---

## Phase 3: Core 零依赖 + 拆分 error/config ✅

**Status:** DONE
**Commit range:** `fa4db987` ~ `60a2bb34`

### 3A: core error 保留 enum 模式（struct+ErrorKind 尝试失败回退）

最初尝试 struct + ErrorKind 模式，但 cloud 中大量使用 `match error { Error::NotFound => ... }` 等模式匹配，转换成本高且易出错。最终回退到 10-variant hand-written enum。

```rust
pub enum Error {
    Internal(String),
    NotFound,
    InvalidArgument(String),
    Unsupported(String),
    IOError(String),
    NetworkError(String),
    ConfigError(String),
    ValidationError(String),
    DatabaseError(String),
    SerializationError(String),
}
```

### 3B: 从 core 移除 thiserror + sqlx

- `crates/tinyiothub-core/Cargo.toml` — 删除 `thiserror`
- `crates/tinyiothub-core/src/error.rs` — 手动实现 `Display` + `std::error::Error`，无 `#[derive(Error)]`

### 3C: 创建 `tinyiothub-error` crate（已创建但未在 cloud 中使用）

创建了 `crates/tinyiothub-error/`，但 cloud 仍直接 `pub use tinyiothub_core::error::{Error, Result};`。
后续如需扩展 `From<std::io::Error>` 等转换，再考虑切换。

### 3D: 创建 `tinyiothub-config` crate（占位）

```rust
pub use tinyiothub_core::config::*;
```

### 验证:
```bash
cargo check -p tinyiothub-core      # zero framework deps, passes
cargo check --workspace             # all passes
```

**Commit:** `refactor(core): make core zero-dependency, split error/config crates`
**Tag:** `phase3-core-lean`

---

## Phase 4: Extract tinyiothub-storage ✅

**Status:** DONE

### 4A: 迁移 repository traits

将 `cloud/src/domain/*/repository.rs` 的 trait 定义复制到 `crates/tinyiothub-storage/src/traits/`
- 替换 `crate::dto::entity::X` → `tinyiothub_core::models::X`
- 替换 `crate::shared::error::Result` → `tinyiothub_core::error::Result`

### 4B: 迁移 repository impl

将 `cloud/src/infrastructure/persistence/repositories/` 复制到 `crates/tinyiothub-storage/src/sqlite/`
- 替换 `crate::dto::entity::` → `tinyiothub_core::models::`
- 替换 `crate::domain::` trait 引用 → `tinyiothub_storage::traits::`

### 4C: cloud 中 re-export（保持向后兼容）

```rust
// cloud/src/infrastructure/persistence/database.rs
pub use tinyiothub_storage::sqlite::database::*;
```

### 4D: 迁移 migrations + SQLx 宏配置

```bash
cp -r cloud/migrations crates/tinyiothub-storage/
```

### 验证:
```bash
cargo check -p tinyiothub-storage
cargo check --workspace
```

**Commit:** `refactor(storage): extract repository traits and SQLite impl to storage crate`
**Tag:** `phase4-storage-extracted`

---

## Phase 5: Extract tinyiothub-engine（轻量级，仅 cron）✅

**Status:** DONE — 仅迁移了 cron executor，其他引擎保留在 cloud。

### 实际迁移内容:
- `crates/tinyiothub-engine/src/lib.rs`
- `crates/tinyiothub-engine/src/cron/` — cron executor（已迁移）

### 未迁移（保留在 cloud/domain/）:
- `alarm/rule_engine.rs` — 依赖 cloud-specific Event 类型和 domain service
- `template/engine.rs` / `validator.rs` — 依赖 TemplateRepository + domain 类型
- `automation/executor.rs` — 深度耦合 application 层

### 验证:
```bash
cargo check -p tinyiothub-engine
cargo check --workspace
```

**Commit:** `refactor(engine): extract cron executor to engine crate`
**Tag:** `phase5-engine-partial`

---

## Phase 6: Extract tinyiothub-web（placeholder）✅

**Status:** DONE — 仅创建 placeholder crate，handlers 未实际迁移。

### 实际状态:
```rust
// crates/tinyiothub-web/src/lib.rs
pub mod middleware {
    //! Tower middleware for authentication, CORS, rate limiting, etc.
}
pub mod dto {
    //! Shared request/response DTOs and ApiResponse builder.
}
pub use axum;
pub use tower;
pub use tower_http;
```

### 未迁移:
- HTTP handlers — 深度耦合 `AppContext`（含泛型 service），提取需要 `WebState` trait 泛型化
- middleware — 依赖 cloud-specific jwt/auth 逻辑
- DTOs — `cloud/src/dto/response/` 和 `cloud/src/dto/request/` 仍保留在 cloud

**原因:** Handler → AppContext → Application services → Domain services 的依赖链太深。提取 handlers 需要先提取 application 层，而 application 层又与 domain 层存在循环依赖。这是一个更大的重构，超出本次范围。

**Commit:** `refactor(web): create placeholder web crate`
**Tag:** `phase6-web-placeholder`

---

## Phase 7: Cleanup — cloud 保留核心代码 ✅

**Status:** DONE

### 实际保留的内容（原计划删除，实际因耦合过深保留）:

1. **`cloud/src/dto/entity/`** — 采用 hybrid re-export 模式:
   - `pub use tinyiothub_core::models::X::*;` 导出 core 中的数据类型
   - 本地保留 sqlx-dependent 查询函数（如 `find_device_by_id`, `bulk_create_device_commands` 等）
   - 原因: 这些函数被 cloud 中大量代码直接调用，且依赖 sqlx `QueryBuilder` / `query_as!` 宏，不适合放入 core

2. **`cloud/src/infrastructure/persistence/repositories/`** — 部分保留:
   - `Database` 类型已通过 `pub use tinyiothub_storage::sqlite::database::*;` re-export
   - 部分 repo impl 仍保留在 cloud（未完全迁移到 storage）

3. **`cloud/src/domain/`** — 全部保留:
   - `user/service.rs`, `template/engine.rs`, `alarm/services/`, `device/driver/` 等
   - 这些 service 与 application 层深度耦合

### 删除的内容:
- 死代码清理（PR #19）: 9 个 dead dto entity modules、dead job repository traits、dead frontend code、dead automations SQL

### 验证:
```bash
cargo build --bin tinyiothub-cloud
cargo run --bin tinyiothub-cloud -- --help
```

**Commit:** `refactor(cloud): cleanup dead code, preserve hybrid dto/entity`
**Tag:** `phase7-cloud-cleanup`

---

## Phase 8: Final Verification ✅

**Status:** DONE

### 8A: 编译验证
```bash
cargo check --workspace --all-targets     # ✅ passes
cargo build --workspace --release         # ✅ passes
```

### 8B: 功能验证
```bash
cargo run --bin tinyiothub-cloud &
curl http://localhost:8080/api/v1/health   # ✅ responds
```

### 8C: 依赖图验证
```bash
cargo tree -p tinyiothub-core     # ✅ 零框架依赖（无 tokio/axum/sqlx/thiserror）
cargo tree -p tinyiothub-engine   # ✅ 不依赖 axum/sqlx
cargo tree -p tinyiothub-storage  # 依赖 sqlx + core
cargo tree -p tinyiothub-cloud    # 依赖所有 crates
```

### 8D: cargo test
```bash
cargo test --workspace            # ❌ fails on vendor/zeroclaw (pre-existing rcgen issue, unrelated)
```

### 8E: 文档更新
- 计划文档本身已更新（即本文档）
- `ARCHITECTURE_HARNESS.md` — 待后续更新
- 各 crate README — 待后续更新

**Commit:** `refactor(workspace): final verification, compilation fixes`
**Tag:** `phase8-complete`

---

## 风险管理

| 风险 | 概率 | 影响 | 缓解措施 |
|------|------|------|----------|
| cloud 内耦合过深无法拆分 | 高 | 高 | ✅ 保留未拆分的代码在 cloud，不强行拆 |
| RuleEngine 依赖 Event 类型 | 中 | 中 | 保留在 cloud，后续再评估 |
| Handler 依赖 AppContext | 高 | 中 | ✅ Phase 6 未强行提取，保持 placeholder |
| 测试在拆分后失效 | 中 | 中 | 每阶段运行 `cargo check`，测试失败为 vendor pre-existing |
| Plugin SDK ABI 不兼容 | 低 | 高 | 保持现有 C ABI 不变 |

## 决策记录

### ADR-1: 不提取整个 domain/
**Decision:** 只提取可独立编译的纯逻辑引擎（cron executor），保留耦合度高的 service 在 cloud。
**Rationale:** domain/ 与 application/ 存在循环依赖（AppContext 在 application，但 domain plugin 引用它）。解耦需要先重构 application 层，超出本次范围。

### ADR-2: cloud/ 保留 lib.rs
**Decision:** `cloud/` 保持 `lib.rs`，不纯 binary。
**Rationale:** 现有 MCP tools、integration tests 依赖 `cloud` as lib。后续可逐步把 lib 内容移到各 crates。

### ADR-3: core error 保留 enum（struct+ErrorKind 尝试后回退）
**Decision:** core error 使用 10-variant hand-written enum，而非 struct + ErrorKind。
**Rationale:** 尝试 struct+ErrorKind 后，cloud 中大量 `match error { Error::NotFound => ... }` 模式匹配需要全面改写，成本高且易出错。enum 模式在 core 零依赖的前提下仍可工作（手动实现 Display + std::error::Error）。

### ADR-4: dto/entity 采用 hybrid re-export 模式
**Decision:** `cloud/src/dto/entity/X.rs` 保留为 hybrid 文件：通过 `pub use tinyiothub_core::models::X::*;` 导出数据类型，本地仅保留 sqlx-dependent 查询函数。
**Rationale:** dto/entity 文件包含两类内容：(1) 纯数据结构（已在 core）；(2) sqlx 查询函数（依赖 sqlx 宏和 Database 类型，不适合 core）。完全删除 dto/entity 会导致 100+ 编译错误。hybrid 模式在保持 core 为 single source of truth 的同时，避免了大规模重写。

---

## 后续工作（下一轮迭代）

1. **迁移 dto/entity 查询函数到 tinyiothub-storage**: 将 `find_device_by_id` 等函数转换为 storage crate 的 repository trait 方法或 helper，然后从 cloud 的 dto/entity 中移除。

2. **提取更多 engine 组件**: template engine、alarm rule engine、automation executor 等，前提是解决它们对 application/ 层的依赖。

3. **tinyiothub-web 实际化**: 实现 `WebState` trait 泛型化，将 handlers 从 cloud 迁移到 web crate。

4. **tinyiothub-error 启用**: 如果需要在 cloud 中使用 `From<std::io::Error>` 等转换，切换到 tinyiothub-error。

5. **更新 ARCHITECTURE_HARNESS.md**: 反映新的 workspace 结构和模块边界。
