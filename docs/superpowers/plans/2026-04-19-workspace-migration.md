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

### 3A: 重写 core error 为 struct + ErrorKind 模式

**Rationale:** 避免在 core 中包装 `std::io::Error` 等外部类型，让上层 (error crate) 做 `From` 转换。

**Files:**
- Modify: `crates/tinyiothub-core/src/error.rs`

**目标结构:**
```rust
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub kind: ErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    NotFound,
    InvalidArgument,
    Internal,
    Unsupported,
    IOError,
    NetworkError,
    ConfigError,
    ValidationError,
    DatabaseError,
    SerializationError,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{:?}] {}", self.kind, self.message)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
```

### 3B: 从 core 移除 thiserror + sqlx

**Files:**
- Modify: `crates/tinyiothub-core/Cargo.toml` — 删除 `thiserror`，删除 `sqlx` feature
- Modify: `crates/tinyiothub-core/src/error.rs` — 删除 `#[cfg(feature = "sqlx")]` 块

**Impact:** `cloud/Cargo.toml` 中 `tinyiothub-core = { features = ["sqlx"] }` → 移除 feature

### 3C: 创建 `tinyiothub-error` crate（用 thiserror 扩展）

**Files:**
- Create: `crates/tinyiothub-error/Cargo.toml` — 依赖 `tinyiothub-core`, `thiserror`
- Create: `crates/tinyiothub-error/src/lib.rs`

**目标结构:**
```rust
use thiserror::Error as ThisError;
use tinyiothub_core::error::{Error as CoreError, ErrorKind};

#[derive(Debug, ThisError)]
pub enum Error {
    #[error("Core error: {0}")]
    Core(#[from] CoreError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("SQLx error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("Serde JSON error: {0}")]
    SerdeJson(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
```

### 3D: 创建 `tinyiothub-config` crate（占位 + 扩展）

**Files:**
- Create: `crates/tinyiothub-config/Cargo.toml`
- Create: `crates/tinyiothub-config/src/lib.rs`

**初始内容:**
```rust
pub use tinyiothub_core::config::*;
// 后续迁移 cloud/src/infrastructure/config/ 的加载逻辑到这里
```

### 3E: 更新 cloud 依赖

**Files:**
- Modify: `cloud/Cargo.toml` — 添加 `tinyiothub-error`, `tinyiothub-config`
- Modify: `cloud/src/shared/error.rs` — `pub use tinyiothub_error::{Error, Result};`
- Modify: `cloud/src/infrastructure/config/mod.rs` — `pub use tinyiothub_config::*;`

### 验证:
```bash
cargo check -p tinyiothub-core      # zero framework deps, passes
cargo check --workspace             # all passes
```

**Commit:** `refactor(core): make core zero-dependency, split error/config crates`  
**Tag:** `phase3-core-lean`

---

## Phase 4: Extract tinyiothub-storage

**Goal:** Repository traits 和 SQLite 实现从 `cloud/` 提取到 `storage/`。

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

### 4D: 迁移 migrations + SQLx 宏配置

```bash
cp -r cloud/migrations crates/tinyiothub-storage/
```

**补充:** 在 `storage/Cargo.toml` 中确保 migrations 在 crate 根目录，并在 `storage/src/sqlite/mod.rs` 中:

```rust
pub async fn run_migrations(pool: &sqlx::SqlitePool) -> Result<()> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await?;
    Ok(())
}
```

### 4E: 提供便捷构造函数

```rust
// crates/tinyiothub-storage/src/sqlite/mod.rs
pub async fn create_repositories(pool: sqlx::SqlitePool) -> (
    Arc<dyn DeviceRepository>,
    Arc<dyn TemplateRepository>,
    // ...
) {
    // 创建并返回所有 repo 实现
}
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

### 前置: 将 Event/EventType 移到 core

**Files:**
- Create: `crates/tinyiothub-core/src/models/event.rs`

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Event {
    pub id: String,
    pub event_type: EventType,
    pub device_id: Option<String>,
    pub timestamp: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EventType {
    DeviceOnline,
    DeviceOffline,
    TelemetryReceived,
    AlarmTriggered,
}
```

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
  - → 使用 `tinyiothub_core::models::event::{Event, EventType}`
  
- `engine.rs` 依赖 `domain/template/repository::TemplateRepository`
  - → 使用 `tinyiothub_storage::traits::TemplateRepository`

### 5C: cloud 中保持原有模块路径

```rust
// cloud/src/domain/alarm/services/mod.rs
pub mod rule_engine {
    pub use tinyiothub_engine::alarm::rule_engine::*;
}
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

### 核心挑战: AppContext 泛型化

当前 handler:
```rust
pub async fn get_device(
    State(ctx): State<AppContext>,  // AppContext 是 cloud 特有
) -> impl IntoResponse { }
```

**解决方案:** Web crate 定义 `WebState` trait

```rust
// crates/tinyiothub-web/src/state.rs
pub trait WebState: Clone + Send + Sync + 'static {
    type DeviceService: DeviceService;
    type AlarmService: AlarmService;
    
    fn device_service(&self) -> &Self::DeviceService;
    fn alarm_service(&self) -> &Self::AlarmService;
}

// handlers 使用泛型
pub async fn get_device<S: WebState>(
    State(state): State<S>,
    Path(id): Path<String>,
) -> Result<Json<DeviceResponse>, AppError> {
    let device = state.device_service().get_device(&id).await?;
    Ok(Json(device.into()))
}
```

### 6A: 迁移 middleware

抽象化，不依赖具体 State:
```rust
// crates/tinyiothub-web/src/middleware/auth.rs
pub fn auth_middleware<S>(secret: String) -> impl Layer<S> + Clone {
    // 返回不依赖具体 State 类型的 Layer
}
```

### 6B: 迁移 shared DTOs / response builder

- `cloud/src/dto/response/` → `crates/tinyiothub-web/src/dto/`
- `cloud/src/dto/request/` → `crates/tinyiothub-web/src/dto/`

### 6C: 迁移 API handlers（按 domain 分批）

每批一个 commit，逐步迁移。

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

### 删除顺序（叶子到根，每步 `cargo check`）

1. 删除 `cloud/src/dto/` 中已迁移的内容
2. 删除 `cloud/src/infrastructure/persistence/repositories/`
3. 删除 `cloud/src/domain/*/services/` 中已迁移的引擎

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
cargo run --bin tinyiothub-cloud -- --help
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
cargo run --bin tinyiothub-cloud &
curl http://localhost:8080/api/v1/health
```

### 8C: 依赖图验证
```bash
cargo tree -p tinyiothub-core     # 确认零框架依赖
cargo tree -p tinyiothub-engine   # 不依赖 axum/sqlx
cargo tree -p tinyiothub-storage  # 依赖 sqlx + core
cargo tree -p tinyiothub-cloud    # 依赖所有 crates

# 验证无循环依赖 + core 零框架
cargo tree -p tinyiothub-core --edges normal | grep -E "(tokio|axum|sqlx|thiserror)" && echo "FAIL" || echo "PASS"
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
| RuleEngine 依赖 Event 类型 | 中 | 中 | Phase 5 开始前将 Event 移到 core |
| Handler 依赖 AppContext | 高 | 中 | Phase 6 前实现 WebState trait PoC |
| 测试在拆分后失效 | 中 | 中 | 每阶段运行 `cargo test`，每步 `cargo check` |
| Plugin SDK ABI 不兼容 | 低 | 高 | 保持现有 C ABI 不变 |

## 决策记录

### ADR-1: 不提取整个 domain/
**Decision:** 只提取可独立编译的纯逻辑引擎（rule_engine, template_engine），保留耦合度高的 service 在 cloud。  
**Rationale:** domain/ 与 application/ 存在循环依赖（AppContext 在 application，但 domain plugin 引用它）。解耦需要先重构 application 层，超出本次范围。

### ADR-2: cloud/ 保留 lib.rs
**Decision:** `cloud/` 保持 `lib.rs`，不纯 binary。  
**Rationale:** 现有 MCP tools、integration tests 依赖 `cloud` as lib。后续可逐步把 lib 内容移到各 crates。

### ADR-3: core error 用 struct + ErrorKind
**Decision:** core error 不用 enum，用 struct { kind: ErrorKind, message: String }。  
**Rationale:** 避免在 core 中包装 `std::io::Error` 等外部类型。`From` 转换放在 `tinyiothub-error` crate。
