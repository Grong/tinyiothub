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

## Phase 3: Core 零依赖 + 拆分 error/config (当前)

**Goal:** `tinyiothub-core` 零框架依赖。创建 `tinyiothub-error` 和 `tinyiothub-config` crates。

### 3A: 从 core 移除 thiserror

**Files:**
- Modify: `crates/tinyiothub-core/Cargo.toml` — 删除 `thiserror`
- Modify: `crates/tinyiothub-core/src/error.rs` — 手写 `Display` + `Error`
- Modify: `crates/tinyiothub-core/src/models/template_error.rs` — 已处理

**Impact:** `cloud/` 中 `use crate::shared::error::Error` → `use tinyiothub_core::error::Error`

### 3B: 从 core 移除 sqlx optional feature

**Files:**
- Modify: `crates/tinyiothub-core/Cargo.toml` — 删除 `sqlx` feature
- Modify: `crates/tinyiothub-core/src/error.rs` — 删除 `#[cfg(feature = "sqlx")]` 的 `From<sqlx::Error>`

**Impact:** `cloud/Cargo.toml` 中 `tinyiothub-core = { features = ["sqlx"] }` → 移除 feature

### 3C: 创建 `tinyiothub-error` crate

**Files:**
- Create: `crates/tinyiothub-error/Cargo.toml`
- Create: `crates/tinyiothub-error/src/lib.rs`

**Content:**
```rust
pub use tinyiothub_core::error::{Error, Result};
```

**Why:** 占位 crate，后续可将 domain-specific errors 迁移到这里。

### 3D: 创建 `tinyiothub-config` crate

**Files:**
- Create: `crates/tinyiothub-config/Cargo.toml`
- Create: `crates/tinyiothub-config/src/lib.rs`

**Content:**
```rust
pub use tinyiothub_core::config::*;
```

**Why:** 占位 crate，后续可将配置加载逻辑（当前在 `cloud/src/infrastructure/config/`）迁移到这里。

### 3E: 更新 cloud 依赖

**Files:**
- Modify: `cloud/Cargo.toml` — 添加 `tinyiothub-error`, `tinyiothub-config`
- Modify: `cloud/src/shared/error.rs` — `pub use tinyiothub_error::*;`
- Modify: `cloud/src/infrastructure/config/mod.rs` — `pub use tinyiothub_config::*;`

### 验证:
```bash
cargo check -p tinyiothub-core      # zero deps, passes
cargo check --workspace             # all passes
```

**Commit:** `refactor(core): make core zero-dependency, split error/config crates`
**Tag:** `phase3-core-lean`

---

## Phase 4: Extract tinyiothub-storage

**Goal:** Repository traits 和 SQLite 实现从 `cloud/` 提取到 `storage/`。

### 核心挑战
当前 `cloud/src/domain/*/repository.rs` 定义了 repository traits，且 traits 使用 domain 实体。这些实体已在 `tinyiothub-core` 中，所以 storage crate 可以依赖 core。

### 4A: 迁移 repository traits

对每个 domain:
1. 将 `cloud/src/domain/{X}/repository.rs` 的 trait 定义复制到 `crates/tinyiothub-storage/src/traits/{X}.rs`
2. 替换 `crate::dto::entity::X` → `tinyiothub_core::models::X`
3. 替换 `crate::shared::error::Result` → `tinyiothub_core::error::Result`

### 4B: 迁移 repository impl

1. 将 `cloud/src/infrastructure/persistence/repositories/` 复制到 `crates/tinyiothub-storage/src/sqlite/`
2. 替换 `crate::dto::entity::` → `tinyiothub_core::models::`
3. 替换 `crate::domain::` 的 trait 引用 → `tinyiothub_storage::traits::`

### 4C: cloud 中 re-export（保持向后兼容）

```rust
// cloud/src/domain/*/repository.rs
pub use tinyiothub_storage::traits::*;
```

### 4D: 迁移 migrations

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

## Phase 5: Extract tinyiothub-engine (轻量级)

**Goal:** 提取可独立编译的业务引擎组件。**不提取整个 domain/**。

### 策略：只提取纯逻辑引擎

以下组件无 infrastructure 依赖，可独立提取：
- `domain/alarm/services/rule_engine.rs` — 纯规则评估逻辑
- `domain/template/engine.rs` — 模板应用逻辑
- `domain/template/validator.rs` — 模板验证逻辑
- `domain/automation/executor.rs` — 自动化执行逻辑

### 5A: 创建 engine 模块结构

```
crates/tinyiothub-engine/src/
├── lib.rs
├── alarm/
│   ├── mod.rs
│   └── rule_engine.rs
├── template/
│   ├── mod.rs
│   ├── engine.rs
│   └── validator.rs
└── automation/
    ├── mod.rs
    └── executor.rs
```

### 5B: 处理依赖

- `rule_engine.rs` 依赖 `domain/event::{Event, EventType}`
  - 这些需要移到 core，或在 engine 中重新定义轻量版
  
- `engine.rs` 依赖 `domain/template/repository::TemplateRepository`
  - 使用 `tinyiothub_storage::traits::TemplateRepository`

### 5C: cloud 中保留原有文件但改为 re-export

```rust
// cloud/src/domain/alarm/services/rule_engine.rs
pub use tinyiothub_engine::alarm::rule_engine::*;
```

### 验证:
```bash
cargo check -p tinyiothub-engine
cargo check --workspace
```

**Commit:** `refactor(engine): extract rule/template/automation engines`
**Tag:** `phase5-engine-extracted`

---

## Phase 6: Extract tinyiothub-web

**Goal:** HTTP handlers 和 middleware 从 `cloud/` 提取到 `web/`。

### 6A: 迁移 middleware

```bash
# cloud/src/api/ 中的 middleware 相关
cp cloud/src/api/auth middleware.rs crates/tinyiothub-web/src/middleware/
```

### 6B: 迁移 shared DTOs / response builder

- `cloud/src/dto/response/` → `crates/tinyiothub-web/src/dto/`
- `cloud/src/dto/request/` → `crates/tinyiothub-web/src/dto/`

### 6C: 迁移 API handlers（按 domain 分批）

每批一个 commit：
1. `api/auth/*` handlers
2. `api/devices/*` handlers
3. `api/alarms/*` handlers
4. ...

每个 handler 文件修改：
- `use crate::dto::response::ApiResponseBuilder` → `use tinyiothub_web::dto::ApiResponseBuilder`
- `use crate::application::*` → 保持（cloud 提供这些）
- `use crate::domain::*` → 保持（cloud 提供这些）

### 6D: cloud 路由整合

```rust
// cloud/src/api/mod.rs
pub use tinyiothub_web::api::*;
```

### 验证:
```bash
cargo check -p tinyiothub-web
cargo check --workspace
```

**Commit:** `refactor(web): extract HTTP handlers and middleware`
**Tag:** `phase6-web-extracted`

---

## Phase 7: Cleanup — cloud 瘦身

**Goal:** `cloud/` 仅保留 binary 入口和服务器配置。

### 7A: 评估 cloud 剩余内容

运行：
```bash
find cloud/src -name "*.rs" | wc -l
# 目标: < 20 个文件
```

### 7B: 删除已提取的代码

删除以下目录（内容已在 crates 中）：
- `cloud/src/dto/entity/` (已在 core)
- `cloud/src/dto/response/` (已在 web)
- `cloud/src/infrastructure/persistence/repositories/` (已在 storage)
- `cloud/src/domain/alarm/services/rule_engine.rs` (已在 engine)
- `cloud/src/domain/template/engine.rs` (已在 engine)

### 7C: cloud/src 最终结构

```
cloud/src/
├── main.rs              # tokio::main + 配置加载 + server 启动
├── server.rs            # axum Router 构建 + graceful shutdown
├── lib.rs               # pub mod api; pub mod application; ...
├── api/
│   └── mod.rs           # re-export from tinyiothub_web
├── application/
│   ├── mod.rs           # AppContext, DataContext, DataServer
│   ├── service_manager.rs
│   └── ...
├── domain/
│   └── ...              # 尚未提取到 engine 的 domain 代码
├── infrastructure/
│   └── ...              # 尚未提取到 storage 的 infra 代码
└── shared/
    └── ...              # 尚未提取的共享代码
```

### 验证:
```bash
cargo build --bin tinyiothub-cloud
cargo run --bin tinyiothub-cloud -- --help  # 或测试启动
```

**Commit:** `refactor(cloud): remove extracted code, keep binary only`
**Tag:** `phase7-cloud-slim`

---

## Phase 8: Final Verification

### 8A: 编译验证
```bash
cargo check --workspace --all-targets
cargo build --workspace --release
```

### 8B: 功能验证
```bash
# 启动 cloud 服务，测试基本 API
cargo run --bin tinyiothub-cloud &
curl http://localhost:8080/api/v1/health
```

### 8C: 依赖图验证
```bash
cargo tree -p tinyiothub-core     # 确认零框架依赖
cargo tree -p tinyiothub-engine   # 不依赖 axum/sqlx
cargo tree -p tinyiothub-storage  # 依赖 sqlx + core
cargo tree -p tinyiothub-cloud    # 依赖所有 crates
```

### 8D: 文档更新
- 更新 `ARCHITECTURE_HARNESS.md`
- 更新 `docs/architecture.md`
- 更新各 crate README

**Commit:** `docs: update architecture docs for new workspace structure`
**Tag:** `phase8-complete`

---

## 风险管理

| 风险 | 概率 | 影响 | 缓解措施 |
|------|------|------|----------|
| cloud 内耦合过深无法拆分 | 高 | 高 | 保留未拆分的代码在 cloud，不强行拆 |
| RuleEngine 依赖 Event 类型 | 中 | 中 | 将 Event 轻量版移到 core |
| Handler 依赖 AppContext | 高 | 中 | web crate 通过 generic State 接收 context |
| 测试在拆分后失效 | 中 | 中 | 每阶段运行 `cargo test` 验证 |
| Plugin SDK ABI 不兼容 | 低 | 高 | 保持现有 C ABI 不变 |

## 决策记录

### ADR-1: 不提取整个 domain/
**Decision:** 只提取可独立编译的纯逻辑引擎（rule_engine, template_engine），保留耦合度高的 service 在 cloud。
**Rationale:** domain/ 与 application/ 存在循环依赖（AppContext 在 application，但 domain plugin 引用它）。解耦需要先重构 application 层，超出本次范围。

### ADR-2: cloud/ 保留 lib.rs
**Decision:** `cloud/` 保持 `lib.rs`，不纯 binary。
**Rationale:** 现有 MCP tools、integration tests 依赖 `cloud` as lib。后续可逐步把 lib 内容移到各 crates。

### ADR-3: thiserror 从 core 移除
**Decision:** core 使用手写 `std::error::Error`，不依赖 thiserror。
**Rationale:** 保持 core 零框架依赖原则。thiserror 放在 tinyiothub-error crate（后续创建）。
