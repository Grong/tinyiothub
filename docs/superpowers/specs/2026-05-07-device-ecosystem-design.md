# Device Ecosystem v0.2 设计文档

生成日期: 2026-05-07
分支: main
版本: v0.2
状态: REVIEWED - 已修复 Eng Review 发现的 5 个严重缺口

---

## 1. 概述

TinyIoTHub v0.2 的核心目标是打通设备管理的生态闭环：

- **正向**：Marketplace 下载驱动/模板 → 热加载驱动 → 用模板创建设备
- **反向**：配置好的设备 → 导出为模板 → 发布到 Marketplace

涉及三个子系统的深度整合：Driver 热加载、Marketplace 双向发布、模板导出。

---

## 2. 目标

- 支持外部驱动动态加载/热加载（无需重启服务）
- Marketplace 支持双向流通（下载安装 + 发布上传）
- 设备配置可反向导出为模板
- 所有资源按 Workspace 隔离
- 设备表结构保持不变

---

## 3. 现状分析

### 3.1 当前模块状态

| 模块 | 状态 | 问题 |
|------|------|------|
| **template** | CRUD 完整，TemplateEngine 集成 | 设备不记录 template_id；查询未按 workspace 过滤 |
| **marketplace** | 仅下载安装（模板可用，驱动 stub） | 无发布能力；驱动安装返回"不支持动态加载" |
| **drivers** | 只读静态注册表 | 无持久层；无热加载；无 workspace 隔离 |
| **device** | 核心表稳定 | `driver_name` 是字符串，无验证；无模板关联 |

### 3.2 当前驱动注册机制

通过 `register_drivers!` 宏编译时注册到静态 HashMap：

```rust
tinyiothub_macros::register_drivers! {
    SimulatedDriver,
    ModbusDriver,
    SnmpDriver,
}
```

工厂函数：`Fn(Device) -> Box<dyn DeviceDriver>`

---

## 4. 设计方案

### 4.1 架构总览

```
┌─────────────────────────────────────────────────────────────┐
│                     Device Ecosystem v0.2                    │
├─────────────────────────────────────────────────────────────┤
│  ┌──────────┐    ┌──────────┐    ┌──────────────────────┐  │
│  │ Template │◄──►│  Device  │◄──►│   Driver Registry    │  │
│  │ JSON文件 │    │  (SQLite)│    │   (内存 + 持久元数据)│  │
│  │ + 缓存   │    │          │    │   按 workspace 隔离  │  │
│  └────┬─────┘    └────┬─────┘    └──────────┬───────────┘  │
│       │               │                     │              │
│       │    create     │    bind/unbind      │              │
│       └──────────────►│◄────────────────────┘              │
│                       │                                     │
│  ┌──────────┐        │        ┌──────────────────────┐     │
│  │Marketplace│◄───────┴───────►│   Driver Loader      │     │
│  │(双向API)  │   publish/export  │   (NEW: hot-reload)  │     │
│  └──────────┘                  └──────────────────────┘     │
└─────────────────────────────────────────────────────────────┘
```

**核心原则**：设备表完全不动，闭环信息通过独立机制和运行时层管理。

### 4.2 Driver 热加载系统

#### 4.2.1 问题

Rust `dyn Trait` 不能跨动态库边界（vtable 布局不稳定）。需要定义 C-compatible 接口。

#### 4.2.2 C-Compatible 驱动接口

```rust
// tinyiothub-core/src/driver/dynamic.rs

#[repr(C)]
pub struct DriverVTable {
    pub version: u32,
    
    /// 读取数据：config_json → result_json（C 分配，Rust 通过 free_string 释放）
    pub read_data: extern "C" fn(
        ctx: *mut c_void,
        config_json: *const c_char,
        out_json: *mut *mut c_char,
    ) -> i32,
    
    /// 执行命令：config_json + cmd_json → result_json（C 分配，Rust 通过 free_string 释放）
    pub execute_command: extern "C" fn(
        ctx: *mut c_void,
        config_json: *const c_char,
        cmd_json: *const c_char,
        out_json: *mut *mut c_char,
    ) -> i32,
    
    /// 获取配置 Schema JSON（C 分配，Rust 通过 free_string 释放）
    pub get_schema: extern "C" fn(out_json: *mut *mut c_char) -> i32,
    
    /// 释放驱动内部分配的字符串
    pub free_string: extern "C" fn(s: *mut c_char),
}

pub type DriverInitFn = unsafe extern "C" fn() -> *mut c_void;
pub type DriverVTableFn = unsafe extern "C" fn() -> *const DriverVTable;
pub type DriverDestroyFn = unsafe extern "C" fn(ctx: *mut c_void);
```

**内存安全设计：**
- 输出字符串由驱动（C 侧）通过 `malloc`/`calloc` 分配，Rust 侧读取后通过 `free_string` 释放
- 彻底消除固定大小缓冲区的溢出风险（原 64KB 缓冲区方案已废弃）
- `catch_unwind` 跨越 FFI 边界是 Undefined Behavior，不可用于捕获驱动 panic。驱动必须保证不 panic/segfault
- 加载前通过 `DriverValidator` 在独立进程中测试驱动（见 4.2.8）

#### 4.2.3 DynamicDeviceDriver 适配器

```rust
/// 包装外部动态库，使其符合现有 DeviceDriver trait
pub struct DynamicDeviceDriver {
    ctx: *mut c_void,
    vtable: &'static DriverVTable,
    library: Arc<Library>,
    device: Device,
}

impl DeviceDriver for DynamicDeviceDriver {
    fn read_data(&mut self) -> Result<Vec<ResultValue>, Error> {
        let config = serde_json::to_string(self.device.driver_options.as_ref()?)?;
        let result_json = unsafe {
            let mut out_ptr: *mut c_char = std::ptr::null_mut();
            let ret = (self.vtable.read_data)(
                self.ctx,
                config.as_ptr() as *const c_char,
                &mut out_ptr,
            );
            if ret != 0 {
                return Err(Error::DriverError(format!("read_data failed: {}", ret)));
            }
            if out_ptr.is_null() {
                return Err(Error::DriverError("read_data returned null".into()));
            }
            let s = CStr::from_ptr(out_ptr).to_string_lossy().to_string();
            (self.vtable.free_string)(out_ptr);
            s
        };
        serde_json::from_str(&result_json).map_err(|e| Error::DriverError(e.to_string()))
    }

    fn execute_command(&mut self, cmd: &DeviceCommand) -> Result<bool, Error> {
        let config = serde_json::to_string(self.device.driver_options.as_ref()?)?;
        let cmd_json = serde_json::to_string(cmd)?;
        let result_json = unsafe {
            let mut out_ptr: *mut c_char = std::ptr::null_mut();
            let ret = (self.vtable.execute_command)(
                self.ctx,
                config.as_ptr() as *const c_char,
                cmd_json.as_ptr() as *const c_char,
                &mut out_ptr,
            );
            if ret != 0 {
                return Err(Error::DriverError(format!("execute_command failed: {}", ret)));
            }
            if out_ptr.is_null() {
                return Err(Error::DriverError("execute_command returned null".into()));
            }
            let s = CStr::from_ptr(out_ptr).to_string_lossy().to_string();
            (self.vtable.free_string)(out_ptr);
            s
        };
        serde_json::from_str(&result_json).map_err(|e| Error::DriverError(e.to_string()))
    }
}
```

#### 4.2.4 DriverRegistry（按 workspace 隔离）

```rust
pub struct DriverRegistry {
    /// 内置驱动（编译时注册，全局共享，永不动态卸载）
    builtin: HashMap<String, BuiltinEntry>,
    
    /// 外部驱动（按 workspace 隔离）
    dynamic: HashMap<String, WorkspaceDriverRegistry>, // key = workspace_id
}

struct WorkspaceDriverRegistry {
    workspace_id: String,
    drivers: HashMap<String, DynamicEntry>, // key = driver_name
}

struct DynamicEntry {
    name: String,
    version: String,
    path: PathBuf,
    library: Arc<Library>,
    vtable: &'static DriverVTable,
    loaded_at: DateTime<Utc>,
    ref_count: AtomicUsize,
}
```

#### 4.2.5 加载/卸载/重载流程

**加载新驱动（按 workspace）：**
```
Marketplace 下载 .so → 放入 data/drivers/workspaces/{ws_id}/{name}/{version}/driver.so
→ DriverLoader 验证哈希/签名
→ libloading::Library::new(path)
→ 查找符号 "tinyiothub_driver_init" / "tinyiothub_driver_vtable"
→ 注册到 DriverRegistry.dynamic[{ws_id}]
→ 立即可用
```

**卸载驱动：**
```
检查 ref_count == 0？
→ 否：通知 executor 停止该 workspace 下使用该驱动的所有设备采集任务
→ 等待 ref_count 降到 0（或超时强制 unload）
→ drop(DynamicEntry) → Library 卸载
→ 删除文件
```

**重载驱动（更新）：**
```
下载新版到 data/drivers/workspaces/{ws_id}/{name}/{version+1}/driver.so
→ 对同一 (workspace, name)，先 unload 旧版
→ load 新版
→ 通知 executor：该 workspace 下使用该驱动的设备恢复采集
```

#### 4.2.6 与现有代码集成

`create_driver()` 函数修改：

```rust
pub fn create_driver(
    driver_name: &str,
    workspace_id: &str,
    device: &Device,
) -> Result<DriverWrapper, Error> {
    // 1. 先查内置驱动（全局）
    if let Some(factory) = BUILTIN_REGISTRY.get(driver_name) {
        let driver = factory(device.clone());
        return Ok(DriverWrapper::new(driver));
    }
    
    // 2. 再查该 workspace 的外部动态驱动
    if let Some(ws_registry) = DRIVER_REGISTRY.dynamic.get(workspace_id) {
        if let Some(entry) = ws_registry.drivers.get(driver_name) {
            let driver = DynamicDeviceDriver::new(entry, device.clone())?;
            entry.ref_count.fetch_add(1, Ordering::SeqCst);
            return Ok(DriverWrapper::new(Box::new(driver)));
        }
    }
    
    Err(Error::Unsupported(format!("Unknown driver: {}", driver_name)))
}
```

#### 4.2.7 驱动文件目录结构（按 workspace）

```
data/
└── drivers/
    └── workspaces/
        ├── ws-default-001/
        │   ├── modbus/
        │   │   ├── v1.0.0/
        │   │   │   ├── driver.so
        │   │   │   ├── manifest.json
        │   │   │   └── checksum.sha256
        │   │   └── v1.1.0/
        │   │       ├── driver.so
        │   │       ├── manifest.json
        │   │       └── checksum.sha256
        │   └── snmp/
        │       └── v2.0.0/
        │           ├── driver.so
        │           ├── manifest.json
        │           └── checksum.sha256
        └── ws-acme-corp/
            ├── modbus/
            │   └── v1.1.0/
            │       ├── driver.so
            │       ├── manifest.json
            │       └── checksum.sha256
            └── custom-driver/
                └── v1.0.0/
                    ├── driver.so
                    ├── manifest.json
                    └── checksum.sha256
```

#### 4.2.8 驱动验证（DriverValidator）

`catch_unwind` 跨越 FFI 边界是 Undefined Behavior，无法用于捕获驱动 panic。一旦动态库的 `init()` 或 `read_data()` 发生 segfault，整个云进程将崩溃。

**缓解方案：加载前在独立进程中预验证驱动**

```rust
pub struct DriverValidator;

impl DriverValidator {
    /// 在子进程中加载驱动并调用一次 read_data，验证其稳定性
    pub fn validate(driver_path: &Path, test_config: &str) -> Result<(), DriverValidationError> {
        // 使用 std::process::Command 启动验证器子进程
        // 子进程加载 .so，调用 init + read_data，返回退出码
        // 父进程设置超时（5 秒），超时或非正常退出 → 拒绝加载
    }
}
```

**验证流程：**
```
下载 .so → checksum 验证 → DriverValidator 预验证（子进程）
→ 验证通过？→ 注册到 DriverRegistry
→ 验证失败？→ 删除文件，返回"驱动不稳定或包含错误"
```

**限制：**
- 验证覆盖 happy path，无法保证 100% 无 segfault（驱动可能在特定输入下崩溃）
- 作为 SaaS 平台，长期应考虑将驱动执行隔离到独立进程或 WASM sandbox

#### 4.2.9 启动重载（Rehydration）

进程重启后，`DriverRegistry.dynamic` 为空。需在启动时从 `driver_installations` 表恢复：

```rust
impl DriverRegistry {
    pub async fn rehydrate(&mut self, repo: &DriverInstallationRepo) -> Result<(), Error> {
        let installations = repo.find_all().await?;
        for inst in installations {
            // 按 workspace 分组，逐个加载
            match self.load_from_disk(&inst.file_path, &inst.workspace_id).await {
                Ok(_) => info!("rehydrated driver: {}@{}", inst.driver_name, inst.version),
                Err(e) => {
                    error!("failed to rehydrate driver {}: {}", inst.driver_name, e);
                    // 记录到健康面板，但不阻断启动
                }
            }
        }
        Ok(())
    }
}
```

#### 4.2.10 driver_name 验证

`driver_name` 直接用于构造文件路径，必须严格验证以防止路径遍历攻击：

```rust
pub fn validate_driver_name(name: &str) -> Result<(), Error> {
    // 仅允许字母、数字、下划线、连字符
    let re = regex::Regex::new(r"^[a-zA-Z0-9_-]+$").unwrap();
    if !re.is_match(name) {
        return Err(Error::Validation("driver_name contains invalid characters".into()));
    }
    if name.len() > 64 {
        return Err(Error::Validation("driver_name too long (max 64)".into()));
    }
    // 禁止保留名（如 ., .., builtin 等）
    let reserved = [".", "..", "builtin", "system"];
    if reserved.contains(&name) {
        return Err(Error::Validation("driver_name is reserved".into()));
    }
    Ok(())
}
```

### 4.3 Marketplace 双向发布

#### 4.3.1 变更范围

| 项目 | 变更内容 |
|------|----------|
| **Marketplace 服务端** (`../marketplace/`) | 新增 `POST /templates`、`POST /drivers`、API Key 认证、文件存储 |
| **TinyIoTHub Cloud** (`cloud/`) | 新增发布客户端、驱动安装对接 DriverLoader |

#### 4.3.2 Marketplace 服务端变更

**API Key 认证：**

```rust
// marketplace/src/auth.rs

pub struct ApiKeyAuth;

impl ApiKeyAuth {
    pub fn verify(headers: &HeaderMap) -> Result<String, StatusCode> {
        let key = headers
            .get("X-API-Key")
            .and_then(|v| v.to_str().ok())
            .ok_or(StatusCode::UNAUTHORIZED)?;
        
        if is_valid_key(key) {
            Ok(key.to_string())
        } else {
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}
```

API Key 管理：
- Marketplace 超管后台生成 Key（`tk_` 前缀）
- 每个 Key 绑定一个发布者（作者名、邮箱）
- Key 存到 sled：`api_keys/{key} → { author_name, author_email, created_at }`

**模板发布端点：**

```rust
// marketplace/src/handler/templates.rs

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/templates", axum::routing::get(list_templates))
        .route("/templates/{name}", axum::routing::get(get_template))
        .route("/templates", axum::routing::post(publish_template))
}

async fn publish_template(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<PublishTemplateRequest>,
) -> Result<Response, ...> {
    let api_key = ApiKeyAuth::verify(&headers)?;
    validate_template(&req)?;
    
    if template_exists(&state, &req.name) {
        return Err((StatusCode::CONFLICT, ...));
    }
    
    let template = Template {
        id: uuid::Uuid::new_v4().to_string(),
        name: req.name,
        version: req.version,
        description: req.description,
        category: req.category,
        tags: req.tags,
        author_name: get_author_name(&api_key),
        content: req.content,
        updated_at: Utc::now(),
    };
    
    state.cache.save_template(&template).await?;
    Ok(ApiResponseBuilder::success(template).into_response())
}
```

**驱动发布端点：**

```rust
// marketplace/src/handler/drivers.rs

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/drivers", axum::routing::get(list_drivers))
        .route("/drivers/{id}", axum::routing::get(get_driver))
        .route("/drivers", axum::routing::post(publish_driver))
}
```

驱动发布接收 multipart form（`.tar.gz` 包含 `.so` + `manifest.json`），解压存储到 `drivers/{id}/{version}/`。

**Marketplace 服务端存储结构：**

```
marketplace/
├── data/
│   ├── sled/              # sled 数据库（元数据、缓存、API Keys）
│   ├── templates/         # 模板 JSON 文件
│   │   ├── modbus-thermometer/
│   │   │   ├── v1.0.0.json
│   │   │   └── v1.1.0.json
│   │   └── onvif-camera/
│   │       └── v2.0.0.json
│   └── drivers/           # 驱动二进制包
│       ├── modbus/
│       │   └── v1.0.0/
│       │       ├── modbus.so
│       │       └── manifest.json
│       └── snmp/
│           └── v2.0.0/
│               ├── snmp.so
│               └── manifest.json
```

#### 4.3.3 TinyIoTHub Cloud 客户端变更

**发布调用：**

```rust
// cloud/src/modules/marketplace/publisher.rs

pub struct MarketplacePublisher {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
}

impl MarketplacePublisher {
    pub async fn publish_template(
        &self,
        template: &DeviceTemplate,
    ) -> Result<PublishedItem, Error> {
        let api_key = self.api_key.as_ref()
            .ok_or(Error::MissingConfig("MARKETPLACE_API_KEY"))?;
        
        let req = PublishTemplateRequest {
            name: template.name.clone(),
            version: template.version.clone(),
            description: template.description.clone(),
            category: template.category.clone(),
            tags: template.tags.split(',').map(|s| s.trim().to_string()).collect(),
            content: serde_json::to_value(template)?,
        };
        
        let res = self.client
            .post(format!("{}/templates", self.base_url))
            .header("X-API-Key", api_key)
            .json(&req)
            .send()
            .await?;
        
        Ok(res.json().await?)
    }
}
```

端点：
```
POST /api/v1/marketplace/publish/template
POST /api/v1/marketplace/publish/driver
```

**驱动下载安装（按 workspace）：**

```rust
// cloud/src/modules/marketplace/installer/driver_installer.rs

pub async fn install_driver(
    &self,
    driver_id: &str,
    workspace_id: &str,
) -> Result<InstalledDriver, Error> {
    let package = self.download(driver_id).await?;
    package.verify_checksum()?;
    
    let dest = package.extract_to_workspace(&self.drivers_dir, workspace_id).await?;
    self.driver_loader.load(&dest, workspace_id).await?;
    
    Ok(InstalledDriver { ... })
}
```

### 4.4 模板导出（反向闭环）

```rust
// cloud/src/modules/template/exporter.rs

pub struct TemplateExporter;

impl TemplateExporter {
    pub fn export_from_device(device: &Device) -> Result<DeviceTemplate, Error> {
        let template = DeviceTemplate {
            id: format!("tpl_{}", uuid::Uuid::new_v4()),
            name: format!("{}_template", device.name),
            display_name: device.display_name.clone(),
            category: device.category.clone(),
            device_type: device.device_type.clone(),
            protocol_type: device.protocol_type.clone(),
            driver_name: device.driver_name.clone(),
            // driver_options 包含敏感信息（密码、API Key），必须脱敏后才能写入模板
            driver_options: Self::sanitize_driver_options(device.driver_options.as_ref()),
            properties: Self::infer_properties_from_device(device),
            commands: Self::infer_commands_from_device(device),
            workspace_id: Some(device.workspace_id.clone()),
            is_builtin: 0,
            ..Default::default()
        };
        Ok(template)
    }

    /// 脱敏 driver_options：保留结构，移除已知敏感字段的值
    fn sanitize_driver_options(options_json: Option<&str>) -> Option<String> {
        let mut value: serde_json::Value = serde_json::from_str(options_json?).ok()?;
        let sensitive_keys = ["password", "secret", "api_key", "token", "auth"];
        if let serde_json::Value::Object(ref mut map) = value {
            for key in sensitive_keys {
                if map.contains_key(key) {
                    map.insert(key.to_string(), serde_json::Value::String("__REDACTED__".into()));
                }
            }
        }
        serde_json::to_string(&value).ok()
    }
}
```

端点：
```
POST /api/v1/devices/{id}/export-template
→ 导出为模板 → 保存到本 workspace 模板库 → 返回模板 ID

POST /api/v1/marketplace/publish/template
Body: { template_id: "...", marketplace_api_key: "..." }
→ 验证模板属于本 workspace → 调用 Marketplace Publisher → 返回发布结果
```

### 4.5 Workspace 隔离

#### 4.5.1 模板隔离

```rust
// cloud/src/modules/template/repo.rs

pub async fn find_all(
    &self,
    params: &TemplateQueryParams,
    workspace_id: &str,
) -> Result<Vec<DeviceTemplate>, TemplateError> {
    let cache = self.cache.read();
    let mut templates: Vec<DeviceTemplate> = cache
        .iter()
        .filter(|t| t.is_active == 1)
        .filter(|t| {
            t.workspace_id.is_none() || t.workspace_id.as_ref() == Some(workspace_id)
        })
        .cloned()
        .collect();
    ...
}
```

**模板存储目录：**

```
data/
├── templates/
│   ├── builtin/              # 内置模板（全局共享）
│   │   ├── modbus-thermometer.json
│   │   └── onvif-camera.json
│   └── workspaces/
│       ├── ws-default-001/
│       │   ├── custom-sensor.json
│       │   └── exported-from-device.json
│       └── ws-acme-corp/
│           └── acme-modbus.json
```

**数据库查询也加 workspace 过滤：**

```sql
SELECT ... FROM device_templates 
WHERE is_active = 1 
  AND (workspace_id IS NULL OR workspace_id = ?)
```

#### 4.5.2 驱动隔离

驱动二进制按 workspace 存储（见 4.2.7 目录结构）。

每个 workspace 可以设置驱动版本偏好：

```rust
pub struct WorkspaceDriverPreference {
    pub workspace_id: String,
    pub driver_name: String,
    pub preferred_version: String,
    pub auto_update: bool,
}
```

#### 4.5.3 Marketplace 安装隔离

- **模板安装**：下载后标记 `workspace_id = 当前 workspace`，仅本 workspace 可见
- **驱动安装**：下载到 `data/drivers/workspaces/{ws_id}/`，仅本 workspace 可用

#### 4.5.4 发布权限

```rust
// 发布端点
POST /marketplace/publish/template

// Middleware 检查：
// 1. 当前用户是否有 workspace 的 publish 权限？
// 2. API Key 是否配置？
// 3. 发布的模板是否属于当前 workspace？

pub async fn publish_template(
    auth: WorkspaceAuth,
    req: PublishTemplateRequest,
) -> Result<...> {
    let template = template_repo.find_by_id(&req.template_id, &auth.workspace_id).await?;
    marketplace.publish_template(&template, &auth.api_key).await
}
```

---

## 5. 数据模型

### 5.1 Device 表（保持不变）

```rust
pub struct Device {
    // 现有字段全部保留，不新增外键
    pub id: String,
    pub name: String,
    pub driver_name: Option<String>,        // 保持字符串
    pub driver_options: Option<String>,     // JSON 字符串
    pub protocol_type: Option<String>,      // 保持字符串
    // ... 其他现有字段
}
```

### 5.2 新增：Driver 元数据表

```sql
CREATE TABLE driver_installations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id TEXT NOT NULL,
    driver_name TEXT NOT NULL,
    version TEXT NOT NULL,
    file_path TEXT NOT NULL,
    checksum TEXT NOT NULL,
    protocol_type TEXT,
    installed_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(workspace_id, driver_name, version)
);
```

### 5.3 新增：Workspace 驱动偏好表

```sql
CREATE TABLE workspace_driver_preferences (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id TEXT NOT NULL,
    driver_name TEXT NOT NULL,
    preferred_version TEXT NOT NULL,
    auto_update BOOLEAN DEFAULT 0,
    UNIQUE(workspace_id, driver_name)
);
```

### 5.4 Template 表（已有 workspace_id 字段）

确认 `device_templates` 表已有 `workspace_id` 字段，查询时增加过滤条件。

---

## 6. API 设计

### 6.1 TinyIoTHub Cloud 新增端点

| 方法 | 路径 | 描述 |
|------|------|------|
| POST | `/api/v1/marketplace/drivers/{id}/install` | 安装驱动到当前 workspace |
| POST | `/api/v1/marketplace/publish/template` | 发布模板到 Marketplace |
| POST | `/api/v1/marketplace/publish/driver` | 发布驱动到 Marketplace |
| POST | `/api/v1/devices/{id}/export-template` | 从设备导出模板 |

### 6.2 Marketplace 服务端新增端点

| 方法 | 路径 | 描述 |
|------|------|------|
| POST | `/templates` | 发布模板（需 X-API-Key） |
| POST | `/drivers` | 发布驱动（需 X-API-Key，multipart） |

---

## 7. 错误处理

| 场景 | 错误码 | 处理 |
|------|--------|------|
| 驱动加载失败（ABI 不兼容） | 500 | 回滚：不注册到 Registry，删除文件，返回详细错误 |
| 驱动加载失败（workspace 已有旧版运行中） | 409 | 先 unload 旧版 → load 新版 → 通知 executor 恢复 |
| 模板导出失败（设备配置不完整） | 422 | 返回缺少的必填字段列表 |
| Marketplace 发布失败（名称冲突） | 409 | 透传 Marketplace 错误，提示更换名称 |
| Marketplace 发布失败（认证失败） | 401 | 提示检查 API Key 配置 |
| workspace 隔离越权 | 403 | 返回权限错误，记录审计日志 |
| 驱动卸载时 ref_count > 0 | 409 | 返回"驱动正在使用中，请先停止相关设备" |

---

## 8. 测试策略

### 8.1 单元测试

- **DriverLoader**：加载测试 `.so` → 验证接口调用 → unload → 验证内存释放
- **DriverRegistry**：按 workspace 隔离的增删查
- **TemplateExporter**：从设备导出 → 验证字段完整性

### 8.2 集成测试

- **Workspace 隔离**：workspace A 安装驱动 → workspace B 查询不到；workspace A 卸载 → workspace B 不受影响
- **Marketplace 客户端**：Mock Marketplace 服务端，测试发布/下载全流程
- **热加载端到端**：安装驱动 → 创建设备使用驱动 → 更新驱动版本 → 验证设备使用新版

### 8.3 E2E 测试

- 正向闭环：Marketplace 下载模板 + 驱动 → 用模板创建设备 → 设备正常采集数据
- 反向闭环：配置设备 → 导出模板 → 发布到 Marketplace → 另一个 workspace 下载使用

---

## 9. 实施顺序

建议按以下顺序实现，每步可独立测试：

1. **Driver 热加载基础**（~3 天）
   - C FFI 接口定义
   - DriverLoader 实现
   - DriverRegistry 实现
   - 测试用动态库

2. **Workspace 隔离**（~2 天）
   - 模板查询加 workspace 过滤
   - 驱动安装按 workspace 隔离
   - 目录结构调整

3. **Marketplace 服务端发布端点**（~2 天）
   - API Key 认证
   - POST /templates
   - POST /drivers

4. **TinyIoTHub Cloud 客户端对接**（~2 天）
   - 驱动安装对接 DriverLoader
   - 发布客户端
   - 模板导出

5. **集成测试 & 文档**（~2 天）

**总计：约 11 天**

---

## 10. 风险评估

| 风险 | 等级 | 缓解措施 |
|------|------|----------|
| Rust FFI 内存安全问题 | 高 | 使用 JSON 字符串传递数据，最小化 unsafe 代码；extensive review |
| 驱动卸载时 ref_count 管理复杂 | 中 | 强制 unload 前停止所有相关设备；超时机制 |
| 多 workspace 驱动重复存储 | 低 | 磁盘成本低；未来可优化为 copy-on-write |
| Marketplace 服务端 API 变更 | 低 | 版本化 API；Cloud 端做好兼容性处理 |

---

*文档状态: DRAFT - 等待 review*
