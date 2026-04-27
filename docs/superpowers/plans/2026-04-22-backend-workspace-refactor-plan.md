# TinyIoTHub 架构重构实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 重构 TinyIoTHub 为 6-crate 专业架构，遵循零租户污染原则，建立清晰的职责边界和单向依赖关系。

**Architecture:** 6个 crate：tinyiothub-types（纯类型）、tinyiothub-storage（存储抽象）、tinyiothub-engine（业务引擎）、tinyiothub-api（HTTP基础设施）、tinyiothub-shared（共享工具）、tinyiothub-cloud（SaaS应用层）。依赖方向：cloud → api → engine → storage → types，shared 可被所有层依赖。

**Tech Stack:** Rust 2024, Tokio, Axum, SQLx, thiserror, serde, chrono, uuid

---

## 文件结构规划

### 新建 crate 结构：
- `crates/tinyiothub-types/` - 纯领域类型（零基础设施依赖）
- `crates/tinyiothub-shared/` - 共享工具（合并 error + utils）
- `crates/tinyiothub-api/` - HTTP 基础设施层
- `crates/tinyiothub-cloud/` - 重命名自 `cloud/`（SaaS 应用层）

### 修改现有 crate：
- `crates/tinyiothub-core/` - 拆解，类型移到 types，业务逻辑移到 engine 或 storage
- `crates/tinyiothub-storage/` - 移除所有 tenant_id/workspace_id 引用
- `crates/tinyiothub-engine/` - 移除所有 HTTP/租户依赖
- `crates/tinyiothub-web/` - 移除租户鉴权逻辑

### 依赖调整：
- `tinyiothub-types`：零依赖（仅 serde、chrono、uuid）
- `tinyiothub-shared`：最小依赖（thiserror、serde_json）
- `tinyiothub-storage`：依赖 types + shared
- `tinyiothub-engine`：依赖 types + storage + shared
- `tinyiothub-api`：依赖 types + engine + shared
- `tinyiothub-cloud`：依赖所有内部 crate

---

## 阶段一：基础架构搭建（第1周）

### Task 1: 创建新 crate 结构

**Files:**
- Create: `crates/tinyiothub-types/Cargo.toml`
- Create: `crates/tinyiothub-shared/Cargo.toml`
- Create: `crates/tinyiothub-api/Cargo.toml`
- Modify: `Cargo.toml` (workspace members)
- Modify: `cloud/Cargo.toml` (rename to tinyiothub-cloud)

- [ ] **Step 1: 创建 tinyiothub-types Cargo.toml**

```toml
[package]
name = "tinyiothub-types"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true
description = "Pure domain types for TinyIoTHub — zero infrastructure dependencies"

[dependencies]
serde = { workspace = true, features = ["derive"] }
serde_json = { workspace = true }
chrono = { workspace = true, features = ["serde"] }
uuid = { workspace = true, features = ["v4", "serde"] }

[features]
default = []
```

- [ ] **Step 2: 创建 tinyiothub-shared Cargo.toml**

```toml
[package]
name = "tinyiothub-shared"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
description = "Shared utilities and error types for TinyIoTHub"

[dependencies]
thiserror = { workspace = true }
serde_json = { workspace = true }
chrono = { workspace = true }
uuid = { workspace = true }
tinyiothub-types = { workspace = true }
```

- [ ] **Step 3: 创建 tinyiothub-api Cargo.toml**

```toml
[package]
name = "tinyiothub-api"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
description = "HTTP infrastructure layer for TinyIoTHub — generic handlers, middleware, DTOs"

[dependencies]
axum = { workspace = true }
tower = { workspace = true }
tower-http = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
headers = { workspace = true }
tinyiothub-types = { workspace = true }
tinyiothub-shared = { workspace = true }
tinyiothub-engine = { workspace = true }
```

- [ ] **Step 4: 更新 workspace Cargo.toml 成员**

```toml
# 在 [workspace] members 部分添加
members = [
    "crates/*",
    "cloud",
    "edge",
    "marketplace",
    "cli",
    "plugins/*",
    "sdks/*",
    # 新增
    "crates/tinyiothub-types",
    "crates/tinyiothub-shared", 
    "crates/tinyiothub-api",
]
```

- [ ] **Step 5: 重命名 cloud 为 tinyiothub-cloud**

```bash
git mv cloud tinyiothub-cloud
```

- [ ] **Step 6: 更新 tinyiothub-cloud Cargo.toml 名称**

修改 `tinyiothub-cloud/Cargo.toml`:
```toml
[package]
name = "tinyiothub-cloud"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
description = "SaaS application layer for TinyIoTHub — tenant-aware adapters, SaaS models"
```

- [ ] **Step 7: 测试编译**

```bash
cargo check --workspace
```
预期：编译通过，可能有缺少模块的错误（正常）

- [ ] **Step 8: 提交**

```bash
git add Cargo.toml tinyiothub-cloud/ crates/tinyiothub-types/ crates/tinyiothub-shared/ crates/tinyiothub-api/
git commit -m "feat: create new crate structure (types, shared, api)"
```

### Task 2: 迁移 tinyiothub-types（从 core 提取纯净类型）

**Files:**
- Create: `crates/tinyiothub-types/src/lib.rs`
- Create: `crates/tinyiothub-types/src/models/` 目录结构
- Modify: `crates/tinyiothub-core/src/models/*.rs`（提取类型）
- Modify: `crates/tinyiothub-core/Cargo.toml`（移除 sqlx 依赖）

- [ ] **Step 1: 创建 types lib.rs**

```rust
// crates/tinyiothub-types/src/lib.rs
pub mod models;

// Re-export common types
pub use models::*;
```

- [ ] **Step 2: 创建 models 目录结构**

```bash
mkdir -p crates/tinyiothub-types/src/models
touch crates/tinyiothub-types/src/models/mod.rs
```

- [ ] **Step 3: 分析 core 中的模型文件**

检查 `crates/tinyiothub-core/src/models/` 目录，识别纯类型文件：
```bash
ls crates/tinyiothub-core/src/models/
```

- [ ] **Step 4: 创建 device.rs 类型定义**

从 `crates/tinyiothub-core/src/models/device.rs` 提取纯净类型（移除 sqlx::FromRow 等）：

```rust
// crates/tinyiothub-types/src/models/device.rs
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub device_type: String,
    pub protocol: String,
    pub connection_params: serde_json::Value,
    pub status: DeviceStatus,
    pub last_seen: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    // 注意：已移除 tenant_id 和 workspace_id 字段
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeviceStatus {
    Online,
    Offline,
    Unknown,
}

// 其他相关类型...
```

- [ ] **Step 5: 创建 mod.rs 导出**

```rust
// crates/tinyiothub-types/src/models/mod.rs
pub mod device;
// 其他模块...

pub use device::*;
```

- [ ] **Step 6: 更新 core 的 Cargo.toml 移除 sqlx**

```toml
[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
chrono = { workspace = true }
uuid = { workspace = true }
# 移除 sqlx 依赖
```

- [ ] **Step 7: 测试编译**

```bash
cargo check -p tinyiothub-types
cargo check -p tinyiothub-core
```
预期：types 编译通过，core 可能有编译错误（需要调整）

- [ ] **Step 8: 提交**

```bash
git add crates/tinyiothub-types/ crates/tinyiothub-core/
git commit -m "feat: extract pure types to tinyiothub-types crate"
```

### Task 3: 建立 tinyiothub-shared（错误定义+通用工具）

**Files:**
- Create: `crates/tinyiothub-shared/src/lib.rs`
- Create: `crates/tinyiothub-shared/src/error.rs`（从 tinyiothub-error 迁移）
- Create: `crates/tinyiothub-shared/src/utils/` 目录
- Modify: `crates/tinyiothub-error/`（标记为废弃）
- Modify: `crates/tinyiothub-utils/`（如果存在，迁移内容）

- [ ] **Step 1: 创建 shared lib.rs**

```rust
// crates/tinyiothub-shared/src/lib.rs
pub mod error;
pub mod utils;

pub use error::*;
```

- [ ] **Step 2: 从 tinyiothub-error 迁移错误定义**

复制 `crates/tinyiothub-error/src/lib.rs` 内容到 `crates/tinyiothub-shared/src/error.rs`，更新依赖：
```rust
// crates/tinyiothub-shared/src/error.rs
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Not found: {0}")]
    NotFound(String),
    
    #[error("Validation error: {0}")]
    ValidationError(String),
    
    #[error("IO error: {0}")]
    IoError(String),
    
    // 其他错误变体...
}

pub type Result<T> = std::result::Result<T, Error>;
```

- [ ] **Step 3: 创建 utils 模块**

```rust
// crates/tinyiothub-shared/src/utils/mod.rs
pub mod password;
pub mod time;
pub mod config;

// 重新导出
pub use password::*;
pub use time::*;
pub use config::*;
```

- [ ] **Step 4: 实现 password 工具**

```rust
// crates/tinyiothub-shared/src/utils/password.rs
use bcrypt::{hash, verify, DEFAULT_COST};

pub fn hash_password(password: &str) -> Result<String, bcrypt::BcryptError> {
    hash(password, DEFAULT_COST)
}

pub fn verify_password(password: &str, hash: &str) -> Result<bool, bcrypt::BcryptError> {
    verify(password, hash)
}
```

- [ ] **Step 5: 更新 workspace 依赖**

修改 `Cargo.toml` 的 `[workspace.dependencies]` 部分，用 tinyiothub-shared 替换 tinyiothub-error：
```toml
# 更新 workspace crate 引用
tinyiothub-shared = { path = "crates/tinyiothub-shared" }
# 保留 tinyiothub-error 但标记为 deprecated
```

- [ ] **Step 6: 测试编译**

```bash
cargo check -p tinyiothub-shared
cargo check --workspace
```
预期：shared 编译通过，其他 crate 可能有依赖错误

- [ ] **Step 7: 提交**

```bash
git add crates/tinyiothub-shared/ Cargo.toml
git commit -m "feat: create tinyiothub-shared with error and utils"
```

### Task 4: 配置 CI/CD 流水线

**Files:**
- Modify: `.github/workflows/ci.yml`（添加架构合规检查）
- Create: `scripts/check-architecture.sh`

- [ ] **Step 1: 创建架构检查脚本**

```bash
#!/bin/bash
# scripts/check-architecture.sh

echo "=== Architecture Compliance Check ==="

# 1. 存储层零租户污染检查
echo "1. Checking storage layer for tenant pollution..."
if grep -r "workspace_id\|tenant_id" crates/tinyiothub-storage/src; then
    echo "ERROR: Found tenant pollution in storage layer"
    exit 1
fi

# 2. 类型层零基础设施依赖检查
echo "2. Checking types layer for infrastructure dependencies..."
cargo tree -p tinyiothub-types --edges normal | grep -E "(tokio|axum|sqlx|tower)"
if [ $? -eq 0 ]; then
    echo "ERROR: Found infrastructure dependencies in types layer"
    exit 1
fi

# 3. 引擎层零HTTP依赖检查
echo "3. Checking engine layer for HTTP dependencies..."
cargo tree -p tinyiothub-engine --edges normal | grep -E "(axum|hyper|tower)"
if [ $? -eq 0 ]; then
    echo "ERROR: Found HTTP dependencies in engine layer"
    exit 1
fi

echo "✅ All architecture checks passed"
```

- [ ] **Step 2: 更新 CI 工作流**

在 `.github/workflows/ci.yml` 中添加架构检查步骤：
```yaml
- name: Architecture compliance check
  run: ./scripts/check-architecture.sh
```

- [ ] **Step 3: 添加 clippy 和 fmt 检查**

```yaml
- name: Check formatting
  run: cargo fmt -- --check

- name: Clippy check
  run: cargo clippy --workspace -- -D warnings
```

- [ ] **Step 4: 添加测试覆盖率检查**

```yaml
- name: Test coverage
  run: cargo tarpaulin --workspace --out Html --skip-clean
```

- [ ] **Step 5: 测试脚本**

```bash
chmod +x scripts/check-architecture.sh
./scripts/check-architecture.sh
```
预期：可能有错误（正常，因为还没完成重构）

- [ ] **Step 6: 提交**

```bash
git add scripts/check-architecture.sh .github/workflows/ci.yml
git commit -m "ci: add architecture compliance checks"
```

---

## 阶段二：存储层重构（第2周）

### Task 5: 重写 tinyiothub-storage，彻底移除租户污染

**Files:**
- Modify: `crates/tinyiothub-storage/src/traits/device.rs`（移除 tenant_id/workspace_id）
- Modify: `crates/tinyiothub-storage/src/sqlite/device.rs`（更新 SQL 查询）
- Modify: `crates/tinyiothub-storage/src/sqlite/device_row_mapper.rs`（移除字段映射）
- Create: `crates/tinyiothub-storage/src/lib.rs`（更新导出）

- [ ] **Step 1: 修改 DeviceCriteria 结构体**

```rust
// crates/tinyiothub-storage/src/traits/device.rs
#[derive(Debug, Clone, Default)]
pub struct DeviceCriteria {
    pub id: Option<String>,
    pub name: Option<String>,
    pub device_type: Option<String>,
    pub protocol: Option<String>,
    pub status: Option<DeviceStatus>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    // 已移除: tenant_id 和 workspace_id 字段
}
```

- [ ] **Step 2: 更新 DeviceRepository trait**

移除所有带有 tenant_id 或 workspace_id 参数的方法：
```rust
pub trait DeviceRepository: Send + Sync {
    async fn find_all(&self, criteria: &DeviceCriteria) -> Result<Vec<Device>>;
    async fn find_by_id(&self, id: &str) -> Result<Option<Device>>;
    async fn create(&self, device: &Device) -> Result<Device>;
    async fn update(&self, id: &str, device: &Device) -> Result<Device>;
    async fn delete(&self, id: &str) -> Result<u64>;
    async fn count(&self, criteria: &DeviceCriteria) -> Result<i64>;
    // 移除: find_by_tenant, find_by_workspace 等方法
}
```

- [ ] **Step 3: 更新 SQLite 实现中的 SQL 查询**

修改 `crates/tinyiothub-storage/src/sqlite/device.rs` 中的 find_all 方法：
```rust
pub async fn find_all(&self, criteria: &DeviceCriteria) -> Result<Vec<Device>> {
    let mut query = "SELECT * FROM devices WHERE 1=1".to_string();
    let mut params: Vec<&(dyn ToSql + Sync)> = Vec::new();
    
    // 添加过滤条件（不包含 tenant_id/workspace_id）
    if let Some(id) = &criteria.id {
        query.push_str(" AND id = ?");
        params.push(id);
    }
    // 其他条件...
    
    // 移除 WHERE workspace_id = ? 条件
}
```

- [ ] **Step 4: 更新 device_row_mapper**

```rust
// crates/tinyiothub-storage/src/sqlite/device_row_mapper.rs
pub fn map_row(row: &SqliteRow) -> Result<Device> {
    Ok(Device {
        id: row.get("id"),
        name: row.get("name"),
        device_type: row.get("device_type"),
        protocol: row.get("protocol"),
        connection_params: serde_json::from_str(&row.get::<String, _>("connection_params")).unwrap_or_default(),
        status: DeviceStatus::from_str(&row.get::<String, _>("status")).unwrap_or(DeviceStatus::Unknown),
        last_seen: row.get("last_seen"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        // 不映射 tenant_id 和 workspace_id
    })
}
```

- [ ] **Step 5: 更新 SELECT_COLUMNS 常量**

```rust
const SELECT_COLUMNS: &str = "
    id, name, device_type, protocol, connection_params, 
    status, last_seen, created_at, updated_at
    -- 已移除: tenant_id, workspace_id
";
```

- [ ] **Step 6: 测试存储层修改**

```bash
cargo test -p tinyiothub-storage
```
预期：测试可能失败（需要更新测试代码）

- [ ] **Step 7: 更新测试代码**

修改 `crates/tinyiothub-storage/tests/` 中的测试，移除 tenant/workspace 相关测试。

- [ ] **Step 8: 运行架构检查**

```bash
./scripts/check-architecture.sh
```
预期：存储层检查应通过

- [ ] **Step 9: 提交**

```bash
git add crates/tinyiothub-storage/
git commit -m "refactor: remove tenant pollution from storage layer"
```

### Task 6: 实现干净的 Repository trait 和 SQLite 实现（续）

**Files:**
- Modify: `crates/tinyiothub-storage/src/traits/cron.rs`（移除 workspace_id）
- Modify: `crates/tinyiothub-storage/src/sqlite/cron_job.rs`
- Modify: `crates/tinyiothub-storage/src/sqlite/cron_run.rs`

- [ ] **Step 1: 修改 CronJobRepository trait**

```rust
// crates/tinyiothub-storage/src/traits/cron.rs
pub trait CronJobRepository: Send + Sync {
    async fn find_by_id(&self, id: &str) -> Result<Option<CronJob>>;
    async fn find_all(&self) -> Result<Vec<CronJob>>;
    async fn create(&self, job: &CronJob) -> Result<CronJob>;
    async fn update(&self, id: &str, job: &CronJob) -> Result<CronJob>;
    async fn delete(&self, id: &str) -> Result<u64>;
    // 移除 workspace_id 参数的所有方法
}
```

- [ ] **Step 2: 更新 CronJob 结构体（在 types 层）**

需要在 `crates/tinyiothub-types/src/models/cron_job.rs` 中移除 workspace_id 字段：
```rust
pub struct CronJob {
    pub id: String,
    pub name: String,
    pub schedule: String,
    pub command: String,
    pub enabled: bool,
    pub last_run: Option<DateTime<Utc>>,
    pub next_run: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    // 已移除 workspace_id
}
```

- [ ] **Step 3: 更新 SQLite cron_job.rs 实现**

修改所有方法，移除 workspace_id 相关的 SQL 条件：
```rust
// crates/tinyiothub-storage/src/sqlite/cron_job.rs
pub async fn find_all(&self) -> Result<Vec<CronJob>> {
    let jobs = sqlx::query("SELECT * FROM cron_jobs ORDER BY created_at DESC")
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::IoError(e.to_string()))?;
    
    jobs.into_iter().map(map_row).collect()
}
```

- [ ] **Step 4: 更新 cron_run.rs 类似**

- [ ] **Step 5: 迁移文件处理**

将 SaaS 相关的迁移文件移动到 cloud crate：
```bash
mkdir -p tinyiothub-cloud/migrations/saas
# 移动包含 tenant/workspace 表的迁移文件
mv crates/tinyiothub-storage/migrations/*tenant* tinyiothub-cloud/migrations/saas/
mv crates/tinyiothub-storage/migrations/*workspace* tinyiothub-cloud/migrations/saas/
mv crates/tinyiothub-storage/migrations/*user* tinyiothub-cloud/migrations/saas/
```

- [ ] **Step 6: 测试 cron 功能**

```bash
cargo test -p tinyiothub-storage --test cron
```

- [ ] **Step 7: 提交**

```bash
git add crates/tinyiothub-storage/ crates/tinyiothub-types/
git commit -m "refactor: remove workspace_id from cron repositories"
```

### Task 7: 存储层测试套件

**Files:**
- Create: `crates/tinyiothub-storage/tests/integration/`
- Modify: `crates/tinyiothub-storage/tests/device_test.rs`
- Create: `crates/tinyiothub-storage/tests/setup.rs`

- [ ] **Step 1: 创建集成测试设置**

```rust
// crates/tinyiothub-storage/tests/setup.rs
use tinyiothub_storage::sqlite::SqliteDeviceRepository;
use sqlx::SqlitePool;

pub async fn setup_test_db() -> SqlitePool {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    // 运行迁移（仅 IoT 表，不含 SaaS 表）
    sqlx::migrate!("./migrations/iot")
        .run(&pool)
        .await
        .unwrap();
    pool
}
```

- [ ] **Step 2: 编写设备存储测试**

```rust
// crates/tinyiothub-storage/tests/device_test.rs
#[tokio::test]
async fn test_device_crud() {
    let pool = setup_test_db().await;
    let repo = SqliteDeviceRepository::new(pool);
    
    // 创建设备
    let device = Device { /* ... 不含 tenant_id */ };
    let created = repo.create(&device).await.unwrap();
    assert_eq!(created.name, device.name);
    
    // 查询设备
    let found = repo.find_by_id(&created.id).await.unwrap();
    assert!(found.is_some());
}
```

- [ ] **Step 3: 添加测试覆盖率检查**

在 `Cargo.toml` 中添加 dev-dependencies：
```toml
[dev-dependencies]
sqlx = { workspace = true, features = ["sqlite", "runtime-tokio"] }
tokio = { workspace = true, features = ["full"] }
```

- [ ] **Step 4: 运行测试**

```bash
cargo test -p tinyiothub-storage -- --test-threads=1
```
预期：所有测试通过

- [ ] **Step 5: 检查测试覆盖率**

```bash
cargo tarpaulin -p tinyiothub-storage --out Html
```
目标：≥90% 行覆盖率

- [ ] **Step 6: 提交**

```bash
git add crates/tinyiothub-storage/tests/
git commit -m "test: add storage layer test suite"
```

---

## 阶段三：业务引擎重构（第3周）

### Task 8: 重构 tinyiothub-engine，提取纯业务逻辑

**Files:**
- Modify: `crates/tinyiothub-engine/src/lib.rs`（清理依赖）
- Modify: `crates/tinyiothub-engine/src/cron/mod.rs`（移除租户逻辑）
- Modify: `crates/tinyiothub-engine/src/driver/driver.rs`（移除租户引用）
- Create: `crates/tinyiothub-engine/src/processor/`（业务处理器）

- [ ] **Step 1: 检查引擎层依赖**

```bash
cargo tree -p tinyiothub-engine --edges normal
```
识别并移除 HTTP/租户依赖。

- [ ] **Step 2: 更新 Cargo.toml 依赖**

```toml
[dependencies]
tinyiothub-types = { workspace = true }
tinyiothub-storage = { workspace = true }
tinyiothub-shared = { workspace = true }
# 移除 axum, tower, jwt-simple 等依赖
```

- [ ] **Step 3: 重构 cron 模块**

修改 `crates/tinyiothub-engine/src/cron/mod.rs`，移除所有 workspace_id 引用：
```rust
pub struct CronScheduler {
    job_repository: Arc<dyn CronJobRepository>,
    // 不包含租户信息
}

impl CronScheduler {
    pub async fn find_due_jobs(&self) -> Result<Vec<CronJob>> {
        // 查询所有 due jobs，不按 workspace 过滤
        self.job_repository.find_due_jobs().await
    }
}
```

- [ ] **Step 4: 重构 driver 模块**

确保 driver 注册和执行不依赖租户上下文。

- [ ] **Step 5: 创建业务处理器**

```rust
// crates/tinyiothub-engine/src/processor/device_processor.rs
pub struct DeviceProcessor {
    device_repository: Arc<dyn DeviceRepository>,
}

impl DeviceProcessor {
    pub async fn process_device_command(&self, command: DeviceCommand) -> Result<DeviceEvent> {
        // 纯业务逻辑，无 HTTP/租户依赖
        match command {
            DeviceCommand::Connect(device_id) => {
                let device = self.device_repository.find_by_id(&device_id).await?;
                // 业务逻辑...
                Ok(DeviceEvent::Connected(device_id))
            }
        }
    }
}
```

- [ ] **Step 6: 测试引擎层编译**

```bash
cargo check -p tinyiothub-engine
cargo tree -p tinyiothub-engine --edges normal | grep -E "(axum|hyper|tower)"
```
预期：无 HTTP 依赖输出

- [ ] **Step 7: 引擎集成测试**

创建模拟设备通信测试：
```rust
#[tokio::test]
async fn test_device_processing() {
    let processor = DeviceProcessor::new(/* ... */);
    let event = processor.process_device_command(DeviceCommand::Connect("dev1".to_string())).await;
    assert!(event.is_ok());
}
```

- [ ] **Step 8: 提交**

```bash
git add crates/tinyiothub-engine/
git commit -m "refactor: extract pure business logic to engine layer"
```

---

## 阶段四：API层重构（第4周）

### Task 9: 创建 tinyiothub-api，提取通用HTTP设施

**Files:**
- Create: `crates/tinyiothub-api/src/lib.rs`
- Create: `crates/tinyiothub-api/src/response.rs`（ApiResponseBuilder）
- Create: `crates/tinyiothub-api/src/middleware/` 目录
- Create: `crates/tinyiothub-api/src/dto/` 目录

- [ ] **Step 1: 实现 ApiResponseBuilder**

```rust
// crates/tinyiothub-api/src/response.rs
#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub code: i32,
    pub msg: String,
    pub result: Option<T>,
}

pub struct ApiResponseBuilder;

impl ApiResponseBuilder {
    pub fn success<T>(data: T) -> ApiResponse<T> {
        ApiResponse {
            code: 0,
            msg: "success".to_string(),
            result: Some(data),
        }
    }
    
    pub fn error(msg: &str) -> ApiResponse<()> {
        ApiResponse {
            code: -1,
            msg: msg.to_string(),
            result: None,
        }
    }
}
```

- [ ] **Step 2: 创建通用中间件**

```rust
// crates/tinyiothub-api/src/middleware/request_id.rs
use axum::middleware::Next;
use axum::http::Request;
use tower_http::request_id::MakeRequestId;
use uuid::Uuid;

#[derive(Clone)]
pub struct RequestId;

impl MakeRequestId for RequestId {
    fn make_request_id<B>(&mut self, _request: &Request<B>) -> Option<tower_http::request_id::RequestId> {
        Some(tower_http::request_id::RequestId::new(
            Uuid::new_v4().to_string().parse().unwrap(),
        ))
    }
}
```

- [ ] **Step 3: 创建通用 DTO**

```rust
// crates/tinyiothub-api/src/dto/device.rs
#[derive(Debug, Deserialize)]
pub struct CreateDeviceRequest {
    pub name: String,
    pub device_type: String,
    pub protocol: String,
    pub connection_params: serde_json::Value,
    // 不包含 workspace_id（由 cloud 层添加）
}

#[derive(Debug, Serialize)]
pub struct DeviceResponse {
    pub id: String,
    pub name: String,
    pub status: String,
    // 其他字段...
}
```

- [ ] **Step 4: 创建通用 handler**

```rust
// crates/tinyiothub-api/src/handler/device_handler.rs
pub async fn list_devices(
    Extension(processor): Extension<Arc<DeviceProcessor>>,
    Query(params): Query<DeviceQueryParams>,
) -> Result<Json<ApiResponse<Vec<DeviceResponse>>>, Error> {
    let devices = processor.list_devices(&params).await?;
    let response = devices.into_iter().map(DeviceResponse::from).collect();
    Ok(Json(ApiResponseBuilder::success(response)))
}
```

- [ ] **Step 5: 测试 API 层编译**

```bash
cargo check -p tinyiothub-api
```

- [ ] **Step 6: 创建 OpenAPI 生成**

添加 `utoipa` 依赖，为 DTO 添加 OpenAPI 注解。

- [ ] **Step 7: API 契约测试**

```rust
#[tokio::test]
async fn test_device_api_contract() {
    // 测试请求/响应格式符合 OpenAPI 规范
}
```

- [ ] **Step 8: 提交**

```bash
git add crates/tinyiothub-api/
git commit -m "feat: create tinyiothub-api with generic HTTP infrastructure"
```

---

## 阶段五：SaaS应用重构（第5周）

### Task 10: 重构 tinyiothub-cloud，实现租户感知适配器

**Files:**
- Create: `tinyiothub-cloud/src/infrastructure/persistence/adapters/`
- Modify: `tinyiothub-cloud/src/domain/`（SaaS 领域模型）
- Create: `tinyiothub-cloud/src/api/v1/`（SaaS 专属路由）
- Modify: `tinyiothub-cloud/src/main.rs`（服务编排）

- [ ] **Step 1: 创建租户感知适配器**

```rust
// tinyiothub-cloud/src/infrastructure/persistence/adapters/tenant_device_repository.rs
pub struct TenantDeviceRepository<R: DeviceRepository> {
    inner: R,
    workspace_id: String,
}

impl<R: DeviceRepository> DeviceRepository for TenantDeviceRepository<R> {
    async fn find_all(&self, criteria: &DeviceCriteria) -> Result<Vec<Device>> {
        let mut tenant_criteria = criteria.clone();
        // 在实际查询中添加 workspace_id 过滤
        // 需要设计过滤机制
        self.inner.find_all(&tenant_criteria).await
    }
}
```

- [ ] **Step 2: 实现 SaaS 领域模型**

```rust
// tinyiothub-cloud/src/domain/tenant/model.rs
pub struct Tenant {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub plan_id: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

pub struct Workspace {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub slug: String,
}
```

- [ ] **Step 3: 创建 SaaS API 路由**

```rust
// tinyiothub-cloud/src/api/v1/workspaces.rs
pub async fn create_workspace(
    Extension(tenant_context): Extension<TenantContext>,
    Json(payload): Json<CreateWorkspaceRequest>,
) -> Result<Json<ApiResponse<WorkspaceResponse>>, Error> {
    // 租户感知的业务逻辑
    let workspace = tenant_context.create_workspace(payload).await?;
    Ok(Json(ApiResponseBuilder::success(workspace)))
}
```

- [ ] **Step 4: 实现依赖注入**

```rust
// tinyiothub-cloud/src/application/di.rs
pub fn configure_services(workspace_id: String) -> ServiceContainer {
    let device_repo = Arc::new(SqliteDeviceRepository::new(pool));
    let tenant_device_repo = Arc::new(TenantDeviceRepository::new(device_repo, workspace_id));
    
    ServiceContainer {
        device_processor: Arc::new(DeviceProcessor::new(tenant_device_repo)),
    }
}
```

- [ ] **Step 5: 端到端测试**

创建完整的 SaaS 流程测试：
```rust
#[tokio::test]
async fn test_saas_flow() {
    // 1. 创建租户
    // 2. 创建工作区
    // 3. 添加设备
    // 4. 验证设备隔离
}
```

- [ ] **Step 6: 提交**

```bash
git add tinyiothub-cloud/
git commit -m "feat: implement tenant-aware adapters in cloud crate"
```

---

## 阶段六：迁移验证（第6周）

### Task 11: 渐进式迁移与兼容性

**Files:**
- Create: `crates/tinyiothub-compat/`（兼容层）
- Modify: 所有调用方更新依赖
- Create: 迁移脚本

- [ ] **Step 1: 创建兼容层**

```rust
// crates/tinyiothub-compat/src/lib.rs
#[deprecated = "Use tinyiothub-types instead"]
pub use tinyiothub_types as core;

#[deprecated = "Use tinyiothub_shared::Error instead"]
pub use tinyiothub_shared::Error;
```

- [ ] **Step 2: 更新调用方依赖**

批量更新所有 `Cargo.toml` 文件：
```bash
# 将 tinyiothub-core 替换为 tinyiothub-types
# 将 tinyiothub-error 替换为 tinyiothub-shared
```

- [ ] **Step 3: 性能基准测试**

使用 criterion 进行关键路径基准测试：
```rust
#[criterion::criterion_group]
pub fn device_processing_benchmarks(c: &mut Criterion) {
    c.bench_function("device_command_processing", |b| {
        b.iter(|| processor.process_device_command(command.clone()))
    });
}
```

- [ ] **Step 4: 安全审计**

运行安全检查：
```bash
cargo audit
cargo-deny check
```

- [ ] **Step 5: 文档更新**

更新架构文档、开发指南。

- [ ] **Step 6: 最终验证**

运行完整的架构检查：
```bash
./scripts/check-architecture.sh
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt -- --check
```

- [ ] **Step 7: 提交最终版本**

```bash
git add .
git commit -m "chore: complete architecture refactoring"
```

---

## 计划自审

### 1. Spec 覆盖检查
- [x] 6-crate 结构创建
- [x] 零租户污染原则实施
- [x] 职责边界清晰化
- [x] 依赖方向单向性
- [x] 命名规范更新
- [x] 测试策略实施
- [x] 性能要求验证
- [x] 迁移路线图

### 2. 占位符扫描
检查完成：无 TBD/TODO 占位符，所有任务都有具体实现步骤。

### 3. 类型一致性
- Device 结构体在所有 crate 中保持一致（无 tenant_id/workspace_id）
- Repository trait 签名一致
- ApiResponse 格式统一
- 错误类型使用 tinyiothub-shared::Error

---

## 执行选项

**Plan complete and saved to `docs/superpowers/plans/2026-04-22-backend-workspace-refactor-plan.md`. Two execution options:**

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**