# 驱动动态加载实现文档

## 实现状态

✅ **已完成**：核心架构和SDK实现  
⚠️ **待完成**：动态加载集成和测试

## 架构概览

```
┌─────────────────────────────────────────────────────────────┐
│                      主程序 (iot-edge)                        │
├─────────────────────────────────────────────────────────────┤
│  ┌──────────────────┐  ┌──────────────────────────────┐    │
│  │  静态驱动        │  │  动态驱动加载器               │    │
│  │  (编译时链接)    │  │  (运行时加载)                 │    │
│  ├──────────────────┤  ├──────────────────────────────┤    │
│  │ SimulatedDriver  │  │ DynamicDriverLoader          │    │
│  │ ModbusDriver     │  │ DynamicDriverWrapper         │    │
│  │ SnmpDriver       │  │ UnifiedDriverRegistry        │    │
│  └──────────────────┘  └──────────────────────────────┘    │
│           │                        │                         │
│           └────────────┬───────────┘                         │
│                        │                                     │
│              ┌─────────▼─────────┐                          │
│              │  DeviceDriver     │                          │
│              │  (统一接口)       │                          │
│              └───────────────────┘                          │
└─────────────────────────────────────────────────────────────┘
                         │
                         │ 依赖
                         ▼
┌─────────────────────────────────────────────────────────────┐
│                  Driver SDK (iot-edge-driver-sdk)            │
├─────────────────────────────────────────────────────────────┤
│  • DeviceDriver trait                                        │
│  • 类型定义 (Device, DeviceCommand, ResultValue)            │
│  • 错误处理 (DriverError, Result)                           │
│  • FFI辅助函数                                               │
│  • export_driver! 宏                                         │
└─────────────────────────────────────────────────────────────┘
                         │
                         │ 使用
                         ▼
┌─────────────────────────────────────────────────────────────┐
│                  动态驱动插件 (.dll/.so)                      │
├─────────────────────────────────────────────────────────────┤
│  • 实现 DeviceDriver trait                                   │
│  • 提供 get_driver_info() 方法                              │
│  • 使用 export_driver! 导出FFI接口                          │
└─────────────────────────────────────────────────────────────┘
```

## 已实现的组件

### 1. Driver SDK (`sdks/driver-sdk/`)

**文件结构**：
```
sdks/driver-sdk/
├── Cargo.toml
├── README.md
└── src/
    ├── lib.rs          # SDK入口
    ├── types.rs        # 核心类型定义
    ├── driver.rs       # DeviceDriver trait
    ├── error.rs        # 错误类型
    ├── config.rs       # 配置解析
    ├── ffi.rs          # FFI辅助函数
    └── macros.rs       # export_driver! 宏
```

**核心功能**：
- ✅ 定义统一的 `DeviceDriver` trait
- ✅ 提供完整的类型系统 (`Device`, `DeviceCommand`, `ResultValue`, `ComponentInfo`)
- ✅ 错误处理 (`DriverError`, `Result`)
- ✅ FFI导出宏 (`export_driver!`)
- ✅ 配置解析工具 (`DriverConfig`)

### 2. 动态加载模块 (`src/domain/device/driver/dynamic/`)

**文件结构**：
```
src/domain/device/driver/dynamic/
├── mod.rs          # 模块入口
├── loader.rs       # 动态库加载器
├── wrapper.rs      # 动态驱动包装器
└── registry.rs     # 统一驱动注册表
```

**核心功能**：
- ✅ `DynamicDriverLoader`: 加载.dll/.so文件，调用FFI函数
- ✅ `DynamicDriverWrapper`: 包装动态驱动，实现 `DeviceDriver` trait
- ✅ `UnifiedDriverRegistry`: 统一管理静态和动态驱动

### 3. 示例插件 (`examples/example-plugin/`)

**文件结构**：
```
examples/example-plugin/
├── Cargo.toml
└── src/
    └── lib.rs      # 示例驱动实现
```

**功能**：
- ✅ 完整的驱动实现示例
- ✅ 演示如何使用SDK
- ✅ 可编译为动态库 (.dll/.so)

### 4. Derive宏更新 (`derive/src/lib.rs`)

**更新内容**：
- ✅ 使用主程序的 `Component` 类型（而非SDK类型）
- ✅ 保持与现有静态驱动的兼容性
- ✅ 生成正确的驱动信息

## FFI接口设计

### 导出函数

动态驱动必须导出以下3个C函数：

```rust
// 1. 获取驱动信息（JSON格式）
#[no_mangle]
pub extern "C" fn iot_edge_driver_info() -> *const c_char;

// 2. 创建驱动实例
#[no_mangle]
pub extern "C" fn iot_edge_driver_create(
    device_json: *const c_char,
    context_json: *const c_char,
) -> *mut c_void;

// 3. 销毁驱动实例
#[no_mangle]
pub extern "C" fn iot_edge_driver_destroy(driver: *mut c_void);
```

### 数据传递

- **输入**: JSON字符串 (C字符串指针)
- **输出**: JSON字符串 (C字符串指针) 或 不透明指针 (void*)
- **优点**: 简单、跨语言、易于调试

## 待完成的工作

### 1. 完善动态驱动包装器

**文件**: `src/domain/device/driver/dynamic/wrapper.rs`

**待实现**：
```rust
impl DeviceDriver for DynamicDriverWrapper {
    fn read_data(&mut self) -> Result<Vec<ResultValue>, Error> {
        // TODO: 通过FFI调用驱动的read_data方法
        // 1. 定义FFI函数类型: type ReadDataFn = unsafe extern "C" fn(*mut c_void) -> *const c_char;
        // 2. 从库中获取函数: library.get(b"iot_edge_driver_read_data\0")
        // 3. 调用函数获取JSON结果
        // 4. 解析JSON为Vec<ResultValue>
    }

    fn execute_command(&mut self, command: &DeviceCommand) -> Result<bool, Error> {
        // TODO: 通过FFI调用驱动的execute_command方法
        // 1. 序列化command为JSON
        // 2. 调用FFI函数
        // 3. 解析返回结果
    }
}
```

### 2. 扩展SDK的export_driver!宏

**文件**: `sdks/driver-sdk/src/macros.rs`

**待添加**：
```rust
#[macro_export]
macro_rules! export_driver {
    ($driver_type:ty) => {
        // 现有的3个函数...

        // 新增: 读取数据
        #[no_mangle]
        pub extern "C" fn iot_edge_driver_read_data(driver: *mut c_void) -> *const c_char {
            unsafe {
                let driver_ref = &mut *(driver as *mut $driver_type);
                match driver_ref.read_data() {
                    Ok(data) => {
                        let json = serde_json::to_string(&data).unwrap();
                        $crate::ffi::to_c_string(&json)
                    }
                    Err(e) => {
                        let error = serde_json::json!({"error": e.to_string()});
                        $crate::ffi::to_c_string(&error.to_string())
                    }
                }
            }
        }

        // 新增: 执行命令
        #[no_mangle]
        pub extern "C" fn iot_edge_driver_execute_command(
            driver: *mut c_void,
            command_json: *const c_char,
        ) -> bool {
            unsafe {
                let driver_ref = &mut *(driver as *mut $driver_type);
                let cmd_str = $crate::ffi::from_c_string(command_json);
                let command: $crate::DeviceCommand = serde_json::from_str(&cmd_str).unwrap();
                driver_ref.execute_command(&command).unwrap_or(false)
            }
        }
    };
}
```

### 3. 集成到主程序

**文件**: `src/domain/device/driver/mod.rs`

**待实现**：
```rust
/// 创建驱动实例（统一入口，支持静态和动态驱动）
pub fn create_driver(
    driver_name: &str,
    device: &Device,
    context: Arc<DataContext>,
) -> Result<DriverWrapper, Error> {
    // 优先使用静态驱动
    if is_driver_supported(driver_name) {
        let base_driver = create_driver_by_name(driver_name, device, context)?;
        return Ok(DriverWrapper::new(base_driver));
    }

    // 使用动态驱动
    let registry = dynamic::get_global_registry();
    if registry.has_driver(driver_name) {
        let base_driver = registry.create_driver(driver_name, device, context)?;
        return Ok(DriverWrapper::new(base_driver));
    }

    Err(Error::Unsupported(format!("Unknown driver: {}", driver_name)))
}
```

### 4. 添加驱动管理API

**新文件**: `src/api/drivers/dynamic.rs`

**功能**：
```rust
// 加载动态驱动
POST /api/v1/drivers/dynamic/load
{
    "path": "/path/to/driver.dll"
}

// 卸载动态驱动
DELETE /api/v1/drivers/dynamic/{name}

// 列出所有动态驱动
GET /api/v1/drivers/dynamic

// 获取动态驱动信息
GET /api/v1/drivers/dynamic/{name}
```

### 5. 添加配置支持

**文件**: `app_settings.toml`

**新增配置**：
```toml
[drivers]
# 动态驱动目录
dynamic_driver_path = "./drivers"

# 启动时自动加载的动态驱动
auto_load = [
    "custom_driver.dll",
    "third_party_driver.so"
]

# 是否允许运行时加载驱动
allow_runtime_load = true
```

## 测试计划

### 1. 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_dynamic_driver() {
        let loader = DynamicDriverLoader::load("path/to/driver.dll").unwrap();
        assert_eq!(loader.driver_name(), "ExampleDriver");
    }

    #[test]
    fn test_create_driver_instance() {
        let loader = DynamicDriverLoader::load("path/to/driver.dll").unwrap();
        let device = Device { /* ... */ };
        let wrapper = DynamicDriverWrapper::new(Arc::new(loader), device).unwrap();
        // 测试驱动功能
    }
}
```

### 2. 集成测试

```rust
#[tokio::test]
async fn test_dynamic_driver_integration() {
    // 1. 加载动态驱动
    let registry = UnifiedDriverRegistry::new();
    registry.load_dynamic("./target/release/example_plugin.dll").unwrap();

    // 2. 创建设备
    let device = Device { /* ... */ };
    let context = Arc::new(DataContext::new());

    // 3. 创建驱动实例
    let driver = registry.create_driver("ExampleDriver", &device, context).unwrap();

    // 4. 测试读取数据
    let data = driver.read_data().unwrap();
    assert!(!data.is_empty());

    // 5. 测试执行命令
    let command = DeviceCommand { /* ... */ };
    let result = driver.execute_command(&command).unwrap();
    assert!(result);
}
```

### 3. 性能测试

- 动态加载时间
- 驱动调用开销
- 内存使用情况
- 并发性能

## 使用示例

### 开发动态驱动

```bash
# 1. 创建新项目
cargo new --lib my-driver
cd my-driver

# 2. 配置Cargo.toml
[lib]
crate-type = ["cdylib"]

[dependencies]
iot-edge-driver-sdk = { path = "../../sdks/driver-sdk" }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# 3. 实现驱动 (src/lib.rs)
# 参考 examples/example-plugin/src/lib.rs

# 4. 编译
cargo build --release

# 5. 输出文件
# Windows: target/release/my_driver.dll
# Linux: target/release/libmy_driver.so
```

### 加载和使用

```rust
// 方式1: 启动时自动加载（配置文件）
// app_settings.toml:
// [drivers]
// auto_load = ["my_driver.dll"]

// 方式2: 运行时动态加载（API）
POST /api/v1/drivers/dynamic/load
{
    "path": "./drivers/my_driver.dll"
}

// 方式3: 代码中加载
let registry = dynamic::get_global_registry();
registry.load_dynamic("./drivers/my_driver.dll")?;

// 使用驱动（与静态驱动完全相同）
let driver = create_driver("MyDriver", &device, context)?;
let data = driver.read_data()?;
```

## 安全考虑

1. **路径验证**: 只允许从指定目录加载驱动
2. **签名验证**: 验证驱动文件的数字签名（可选）
3. **沙箱隔离**: 考虑使用进程隔离运行不受信任的驱动
4. **权限控制**: 限制驱动的系统访问权限
5. **错误隔离**: 驱动崩溃不应影响主程序

## 性能优化

1. **延迟加载**: 只在需要时加载驱动
2. **缓存**: 缓存驱动信息，避免重复解析
3. **连接池**: 复用驱动实例
4. **异步调用**: 使用异步FFI调用（如果需要）

## 兼容性

- **Rust版本**: 2021 edition
- **平台支持**: Windows, Linux, macOS
- **ABI稳定性**: 使用C ABI确保跨版本兼容

## 文档

- ✅ SDK使用文档: `sdks/driver-sdk/README.md`
- ✅ 设计文档: `docs/driver-dynamic-loading-final-design.md`
- ✅ 实现文档: 本文档
- ⚠️ API文档: 待完成
- ⚠️ 用户手册: 待完成

## 下一步行动

1. **完善FFI接口**: 添加 `read_data` 和 `execute_command` 的FFI导出
2. **实现包装器**: 完成 `DynamicDriverWrapper` 的方法实现
3. **集成测试**: 编译示例插件并测试加载
4. **添加API**: 实现驱动管理的REST API
5. **文档完善**: 编写用户手册和API文档
6. **性能测试**: 测试动态加载的性能影响
