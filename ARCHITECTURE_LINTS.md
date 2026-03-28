# Rust 架构检查 Clippy Lints

> ⚠️ **Rust 代码必须通过以下 Clippy 检查**。这些 lints 针对 TinyIoTHub 的架构违规模式。

## 在 `api/Cargo.toml` 添加：

```toml
[lints.clippy]
# 架构违规检测
dbg_macro = "warn"                    # 禁止 debug macro
todo = "warn"                         # 禁止 TODO（应用具体的 issue tracker）
unwrap_used = "warn"                  # 禁止 unwrap
expect_used = "warn"                  # 禁止 expect

# 自定义架构 lint（通过 allow 指定）
single_component_imports = "allow"    # 允许单组件 import

[workspace.lints.clippy]
```
> 注意：完整的架构违规检测（如禁止在 handler 里直接 SQL）需要更深入的定制，建议在代码审查阶段由人工完成。

---

## 必须在 api/src/lib.rs 或 main.rs 启用：

```rust
// api/src/lib.rs
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]

// 允许的 lint
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::pub_use)]
#![allow(clippy::unnecessary_wraps)]  // 某些 Error 需要 wrapping
```

---

## 必须使用的模式

### ✅ 错误处理（必须用 thiserror）

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DeviceError {
    #[error("Device not found: {0}")]
    NotFound(String),
    
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("Unauthorized")]
    Unauthorized,
}

// ✅ 正确：所有错误都走这个类型
pub type Result<T> = std::result::Result<T, DeviceError>;
```

### ❌ 禁止的错误处理

```rust
// ❌ 禁止：直接 anyhow
use anyhow::{Context, Result};
fn foo() -> Result<()> { 
    anyhow::bail!("error");  // 禁止
}

// ❌ 禁止：直接 panic
fn foo() { 
    unwrap!();  // 禁止
}

// ❌ 禁止：直接 None unwrap
fn foo() { 
    option.unwrap();  // 禁止
}
```

### ✅ API 响应（必须用 ApiResponseBuilder）

```rust
use crate::dto::response::ApiResponseBuilder;

// ✅ 正确
async fn get_device(
    Path(id): Path<String>,
) -> Json<ApiResponse<DeviceDto>> {
    let device = device_service::find_by_id(&id)
        .await
        .map_err(|e| ApiResponseBuilder::error(e.to_string()))?;
    
    Ok(Json(ApiResponseBuilder::success(device)))
}

// ❌ 禁止：直接返回数据
async fn get_device(Path(id): Path<String>) -> Json<DeviceDto> {
    // ... 禁止这样直接返回
}

// ❌ 禁止：手拼 JSON
async fn get_device(...) -> Json<Value> {
    Json(json!({ "code": 0, "data": device }))
}
```

### ✅ 数据库查询（必须用 SQLx）

```rust
// ✅ 正确：使用 SQLx query builder
let device = sqlx::query_as::<_, Device>(
    "SELECT id, name, status FROM devices WHERE id = ?"
)
.bind(&id)
.fetch_one(&pool)
.await
.map_err(|e| DeviceError::Database(e))?;

// ❌ 禁止：直接字符串拼接 SQL
let device = conn.query_row(
    &format!("SELECT * FROM devices WHERE id = '{}'", id), // SQL 注入风险！
    [],
)?;
```

---

## 项目特定：TinyIoTHub 必须遵守的规则

### 1. 所有 DB 访问必须通过 Repository

```rust
// ✅ 正确：在 infrastructure 层实现 repository trait
// api/src/infrastructure/persistence/repositories/device_repository_impl.rs

pub struct DeviceRepositoryImpl {
    pool: Pool<SqlitePool>,
}

impl DeviceRepository for DeviceRepositoryImpl {
    async fn find_by_id(&self, id: &str) -> Result<Option<Device>> {
        let device = sqlx::query_as::<_, Device>(
            "SELECT * FROM devices WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(device)
    }
}
```

### 2. API Handler 禁止直接调用 DB

```rust
// ❌ 禁止：handler 直接操作数据库
// api/src/api/devices/management.rs
async fn get_device(Path(id): Path<String>, pool: Pool<SqlitePool>) -> ... {
    let device = sqlx::query("SELECT ...").bind(&id).fetch_one(&pool).await?; // 禁止！
}
```

### 3. Domain 层禁止依赖具体技术

```rust
// ❌ 禁止：domain 依赖 sqlx 或具体 DB
// api/src/domain/device/entity.rs
pub struct Device {
    pub id: String,
    pub name: String,
    // ...
    // ❌ 不能有 rusqlite / sqlx 相关的字段
}

// ✅ 正确：domain 只包含业务概念
// api/src/domain/device/entity.rs
pub struct Device {
    pub id: DeviceId,           // 值对象
    pub name: DeviceName,       // 值对象
    pub status: DeviceStatus,  // 值对象
}
```

---

## 运行检查

```bash
# 完整检查
cargo clippy --all-targets --all-features -- -D warnings

# 只检查警告
cargo clippy --all-targets 2>&1 | grep warning

# 检查特定文件
cargo clippy --package tinyiothub --lib 2>&1 | grep architecture
```
