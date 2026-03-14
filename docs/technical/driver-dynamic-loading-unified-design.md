# 驱动动态加载统一设计（与现有架构一致）

## 设计原则

**完全复用现有的驱动架构**，动态驱动和静态驱动使用相同的：
- `ComponentInfo` 元数据结构
- `ComponentOption` 配置选项
- `DeviceDriver` trait接口
- `get_driver_info()` 方法
- 数据库存储机制

## 核心设计

### 1. 保持现有接口不变

```rust
// 现有的驱动定义方式（静态）
#[derive(DeviceDriver)]
#[driver(name = "ModbusDriver", version = "1.0.0")]
#[driver_option(label = "Serial Port", name = "serial_port", default = "/dev/ttyS1")]
pub struct ModbusDriver {
    pub device: Device,
}

impl ModbusDriver {
    pub fn new(device: Device, context: Arc<DataContext>) -> Self {
        Self { device }
    }
}

impl DeviceDriver for ModbusDriver {
    fn device(&self) -> &Device { &self.device }
    fn read_data(&mut self) -> Result<Vec<ResultValue>, Error> { /* ... */ }
    fn execute_command(&mut self, cmd: &DeviceCommand) -> Result<bool, Error> { /* ... */ }
}
```

### 2. 动态驱动使用相同的接口

```rust
// 动态驱动插件（编译为.so/.dll）
// custom_driver/src/lib.rs

#[derive(DeviceDriver)]
#[driver(name = "CustomDriver", version = "1.0.0")]
#[driver_option(label = "API Key", name = "api_key", default = "")]
pub struct CustomDriver {
    pub device: Device,
}

impl CustomDriver {
    pub fn new(device: Device, _context: Arc<DataContext>) -> Self {
        Self { device }
    }
}

impl DeviceDriver for CustomDriver {
    fn device(&self) -> &Device { &self.device }
    fn read_data(&mut self) -> Result<Vec<ResultValue>, Error> {
        // 自定义实现
        Ok(vec![])
    }
    fn execute_command(&mut self, cmd: &DeviceCommand) -> Result<bool, Error> {
        Ok(true)
    }
}

// 插件导出函数（唯一的FFI部分）
#[no_mangle]
pub extern "C" fn iot_edge_driver_create(
    device_json: *const c_char,
    context_json: *const c_char,
) -> *mut c_void {
    // 反序列化并创建驱动
    let device: Device = /* 从JSON解析 */;
    let driver = Box::new(CustomDriver::new(device, /* context */));
    Box::into_raw(driver) as *mut c_void
}

#[no_mangle]
pub extern "C" fn iot_edge_driver_info() -> *const c_char {
    // 返回ComponentInfo的JSON
    let info = CustomDriver::get_driver_info();
    let json = serde_json::to_string(&info).unwrap();
    CString::new(json).unwrap().into_raw()
}
```

## 简化的FFI接口

### 只需要3个导出函数

```rust
// 1. 获取驱动信息（返回JSON）
#[no_mangle]
pub extern "C" fn iot_edge_driver_info() -> *const c_char;

// 2. 创建驱动实例（接收JSON，返回指针）
#[no_mangle]
pub extern "C" fn iot_edge_driver_create(
    device_json: *const c_char,
    context_json: *const c_char,
) -> *mut c_void;

// 3. 销毁驱动实例
#[no_mangle]
pub extern "C" fn iot_edge_driver_destroy(driver: *mut c_void);
```

### 为什么这样设计？

1. **最小FFI接口** - 只有3个函数，简单可靠
2. **使用JSON传递数据** - 避免复杂的C结构体
3. **复用现有trait** - 驱动实例仍然实现 `DeviceDriver` trait
4. **类型安全** - 插件内部是完整的Rust代码

## 统一注册表实现

```rust
// src/domain/device/driver/dynamic/mod.rs

use libloading::{Library, Symbol};

pub struct DynamicDriverRegistry {
    // 静态驱动工厂
    static_factories: HashMap<String, StaticFactory>,
    // 动态驱动加载器
    dynamic_loaders: HashMap<String, DynamicLoader>,
}

struct StaticFactory {
    info: ComponentInfo,
    create_fn: Box<dyn Fn(Device, Arc<DataContext>) -> Box<dyn DeviceDriver>>,
}

struct DynamicLoader {
    library: Arc<Library>,
    info: ComponentInfo,
}

impl DynamicDriverRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            static_factories: HashMap::new(),
            dynamic_loaders: HashMap::new(),
        };
        
        // 注册静态驱动（现有方式）
        registry.register_static(
            "simulator",
            SimulatedDriver::get_driver_info(),
            |device, context| Box::new(SimulatedDriver::new(device, context)),
        );
        
        registry.register_static(
            "ModbusDriver",
            ModbusDriver::get_driver_info(),
            |device, context| Box::new(ModbusDriver::new(device, context)),
        );
        
        // 自动发现动态驱动
        if let Ok(plugin_dir) = std::env::var("DRIVER_PLUGIN_DIR") {
            let _ = registry.discover_plugins(&plugin_dir);
        }
        
        registry
    }
    
    /// 注册静态驱动
    pub fn register_static<F>(
        &mut self,
        name: &str,
        info: ComponentInfo,
        create_fn: F,
    ) where
        F: Fn(Device, Arc<DataContext>) -> Box<dyn DeviceDriver> + 'static,
    {
        self.static_factories.insert(
            name.to_string(),
            StaticFactory {
                info,
                create_fn: Box::new(create_fn),
            },
        );
    }
    
    /// 发现并注册动态驱动
    pub fn discover_plugins(&mut self, plugin_dir: &str) -> Result<(), Error> {
        let path = Path::new(plugin_dir);
        if !path.exists() {
            return Ok(());
        }
        
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            
            if is_plugin_file(&path) {
                match self.load_plugin(&path) {
                    Ok(_) => tracing::info!("Loaded plugin: {:?}", path),
                    Err(e) => tracing::warn!("Failed to load plugin {:?}: {}", path, e),
                }
            }
        }
        
        Ok(())
    }
    
    /// 加载动态驱动插件
    fn load_plugin(&mut self, path: &Path) -> Result<(), Error> {
        unsafe {
            let lib = Library::new(path)?;
            
            // 获取驱动信息
            let get_info: Symbol<extern "C" fn() -> *const c_char> = 
                lib.get(b"iot_edge_driver_info")?;
            
            let info_json_ptr = get_info();
            let info_json = CStr::from_ptr(info_json_ptr).to_str()?;
            let info: ComponentInfo = serde_json::from_str(info_json)?;
            
            // 验证必需的导出函数
            let _: Symbol<extern "C" fn(*const c_char, *const c_char) -> *mut c_void> = 
                lib.get(b"iot_edge_driver_create")?;
            let _: Symbol<extern "C" fn(*mut c_void)> = 
                lib.get(b"iot_edge_driver_destroy")?;
            
            // 注册到动态加载器
            self.dynamic_loaders.insert(
                info.name.clone(),
                DynamicLoader {
                    library: Arc::new(lib),
                    info,
                },
            );
            
            Ok(())
        }
    }
    
    /// 创建驱动实例（统一入口）
    pub fn create_driver(
        &self,
        name: &str,
        device: Device,
        context: Arc<DataContext>,
    ) -> Result<Box<dyn DeviceDriver>, Error> {
        // 优先使用静态驱动
        if let Some(factory) = self.static_factories.get(name) {
            return Ok((factory.create_fn)(device, context));
        }
        
        // 使用动态驱动
        if let Some(loader) = self.dynamic_loaders.get(name) {
            return self.create_dynamic_driver(loader, device, context);
        }
        
        Err(Error::NotFound)
    }
    
    /// 创建动态驱动实例
    fn create_dynamic_driver(
        &self,
        loader: &DynamicLoader,
        device: Device,
        context: Arc<DataContext>,
    ) -> Result<Box<dyn DeviceDriver>, Error> {
        unsafe {
            let create_fn: Symbol<extern "C" fn(*const c_char, *const c_char) -> *mut c_void> = 
                loader.library.get(b"iot_edge_driver_create")?;
            
            // 序列化参数为JSON
            let device_json = serde_json::to_string(&device)?;
            let context_json = "{}"; // 简化的context
            
            let device_cstr = CString::new(device_json)?;
            let context_cstr = CString::new(context_json)?;
            
            // 调用插件创建函数
            let driver_ptr = create_fn(device_cstr.as_ptr(), context_cstr.as_ptr());
            
            if driver_ptr.is_null() {
                return Err(Error::Internal("Failed to create driver".into()));
            }
            
            // 包装为trait对象
            Ok(Box::new(DynamicDriverWrapper {
                library: loader.library.clone(),
                driver_ptr,
                device: device.clone(),
            }))
        }
    }
    
    /// 获取所有驱动信息（静态+动态）
    pub fn list_drivers(&self) -> Vec<ComponentInfo> {
        let mut drivers = Vec::new();
        
        // 静态驱动
        for factory in self.static_factories.values() {
            drivers.push(factory.info.clone());
        }
        
        // 动态驱动
        for loader in self.dynamic_loaders.values() {
            drivers.push(loader.info.clone());
        }
        
        drivers
    }
}
```

## 动态驱动包装器

```rust
/// 动态驱动包装器 - 将C指针包装为DeviceDriver trait
struct DynamicDriverWrapper {
    library: Arc<Library>,
    driver_ptr: *mut c_void,
    device: Device,
}

unsafe impl Send for DynamicDriverWrapper {}
unsafe impl Sync for DynamicDriverWrapper {}

impl DeviceDriver for DynamicDriverWrapper {
    fn device(&self) -> &Device {
        &self.device
    }
    
    fn device_mut(&mut self) -> &mut Device {
        &mut self.device
    }
    
    fn read_data(&mut self) -> Result<Vec<ResultValue>, Error> {
        unsafe {
            // 通过虚函数表调用插件的read_data
            // 插件内部是完整的Rust DeviceDriver实现
            let driver = &mut *(self.driver_ptr as *mut dyn DeviceDriver);
            driver.read_data()
        }
    }
    
    fn execute_command(&mut self, cmd: &DeviceCommand) -> Result<bool, Error> {
        unsafe {
            let driver = &mut *(self.driver_ptr as *mut dyn DeviceDriver);
            driver.execute_command(cmd)
        }
    }
}

impl Drop for DynamicDriverWrapper {
    fn drop(&mut self) {
        unsafe {
            let destroy: Symbol<extern "C" fn(*mut c_void)> = 
                self.library.get(b"iot_edge_driver_destroy").unwrap();
            destroy(self.driver_ptr);
        }
    }
}
```

## 插件开发模板

```rust
// custom_driver/Cargo.toml
[package]
name = "custom_driver"
version = "1.0.0"

[lib]
crate-type = ["cdylib"]  # 编译为动态库

[dependencies]
iot-edge-driver-sdk = { path = "../driver-sdk" }  # 驱动SDK
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

// custom_driver/src/lib.rs
use iot_edge_driver_sdk::*;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

#[derive(DeviceDriver)]
#[driver(name = "CustomDriver", version = "1.0.0", description = "Custom Device Driver")]
#[driver_option(label = "API Endpoint", name = "endpoint", default = "http://localhost", option_type = "string", required = true)]
pub struct CustomDriver {
    pub device: Device,
    endpoint: String,
}

impl CustomDriver {
    pub fn new(device: Device, _context: Arc<DataContext>) -> Self {
        let config = DriverConfig::from_device(&device);
        let endpoint = config.get_string("endpoint", "http://localhost");
        
        Self { device, endpoint }
    }
}

impl DeviceDriver for CustomDriver {
    fn device(&self) -> &Device {
        &self.device
    }
    
    fn device_mut(&mut self) -> &mut Device {
        &mut self.device
    }
    
    fn read_data(&mut self) -> Result<Vec<ResultValue>, Error> {
        // 自定义实现
        Ok(vec![
            ResultValue::float("temperature".to_string(), 25.5),
            ResultValue::integer("humidity".to_string(), 60),
        ])
    }
    
    fn execute_command(&mut self, cmd: &DeviceCommand) -> Result<bool, Error> {
        tracing::info!("Executing command: {:?}", cmd);
        Ok(true)
    }
}

// FFI导出函数
#[no_mangle]
pub extern "C" fn iot_edge_driver_info() -> *const c_char {
    let info = CustomDriver::get_driver_info();
    let json = serde_json::to_string(&info).unwrap();
    CString::new(json).unwrap().into_raw()
}

#[no_mangle]
pub extern "C" fn iot_edge_driver_create(
    device_json: *const c_char,
    _context_json: *const c_char,
) -> *mut c_void {
    unsafe {
        let device_str = CStr::from_ptr(device_json).to_str().unwrap();
        let device: Device = serde_json::from_str(device_str).unwrap();
        
        let driver = Box::new(CustomDriver::new(device, Arc::new(DataContext::mock())));
        Box::into_raw(driver) as *mut c_void
    }
}

#[no_mangle]
pub extern "C" fn iot_edge_driver_destroy(driver: *mut c_void) {
    unsafe {
        let _ = Box::from_raw(driver as *mut CustomDriver);
    }
}
```

## 驱动SDK包

```rust
// driver-sdk/src/lib.rs
// 提供给插件开发者的SDK

pub use iot_edge_gateway::{
    domain::device::driver::{DeviceDriver, ResultValue, DriverConfig},
    dto::entity::{Device, DeviceCommand, component::*},
    shared::error::Error,
    application::context::DataContext,
};

pub use tinyiothub_derive::DeviceDriver;

// 重新导出常用类型
pub use std::sync::Arc;
pub use std::collections::HashMap;
```

## 集成到现有系统

```rust
// src/domain/device/driver/mod.rs

pub mod dynamic;  // 新增动态加载模块

use once_cell::sync::Lazy;
use dynamic::DynamicDriverRegistry;

// 全局驱动注册表
static DRIVER_REGISTRY: Lazy<DynamicDriverRegistry> = Lazy::new(|| {
    DynamicDriverRegistry::new()
});

/// 创建驱动实例（统一入口，兼容现有代码）
pub fn create_driver(
    driver_name: &str,
    device: &Device,
    context: Arc<DataContext>,
) -> Result<DriverWrapper, Error> {
    let base_driver = DRIVER_REGISTRY.create_driver(driver_name, device.clone(), context)?;
    Ok(DriverWrapper::new(base_driver))
}

/// 获取所有驱动信息（静态+动态）
pub fn get_driver_list() -> Vec<ComponentInfo> {
    DRIVER_REGISTRY.list_drivers()
}
```

## 优势总结

### 1. 完全兼容现有架构
- ✅ 使用相同的 `ComponentInfo` 元数据
- ✅ 使用相同的 `DeviceDriver` trait
- ✅ 使用相同的配置机制
- ✅ 现有代码无需修改

### 2. 最小FFI接口
- ✅ 只有3个导出函数
- ✅ 使用JSON传递数据（简单可靠）
- ✅ 避免复杂的C结构体

### 3. 开发体验好
- ✅ 插件是完整的Rust代码
- ✅ 可以使用相同的宏和工具
- ✅ 类型安全
- ✅ 调试友好

### 4. 性能可接受
- ✅ 只在创建时有FFI开销
- ✅ 运行时直接调用trait方法
- ✅ 无额外的序列化开销

## 实施计划

### Phase 1: 基础架构 (2-3天)
- [ ] 创建 `dynamic` 模块
- [ ] 实现 `DynamicDriverRegistry`
- [ ] 实现 `DynamicDriverWrapper`
- [ ] 定义FFI导出函数规范

### Phase 2: SDK和工具 (2天)
- [ ] 创建 `driver-sdk` 包
- [ ] 创建插件模板
- [ ] 编写开发文档

### Phase 3: 测试验证 (2-3天)
- [ ] 创建示例插件
- [ ] 集成测试
- [ ] 性能测试

## 下一步

请确认此设计是否符合要求：

1. ✅ 是否完全复用了现有架构？
2. ✅ FFI接口是否足够简单？
3. ✅ 开发体验是否友好？
4. ✅ 是否满足动态加载需求？

确认后即可开始实施。


## 公共库架构设计

### 问题分析

**当前问题**：
- 插件如果依赖整个 `tinyiothub`，会导致：
  - 编译时间长
  - 依赖过重
  - 版本耦合严重
  - 可能的循环依赖

**解决方案**：
创建独立的 `iot-edge-driver-api` 公共库，只包含驱动开发必需的接口定义。

### 项目结构

```
tinyiothub/
├── Cargo.toml                    # 主程序
├── driver-api/                   # 公共API库 ⭐ 新增
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── driver.rs            # DeviceDriver trait
│       ├── types.rs             # 基础类型定义
│       ├── error.rs             # 错误类型
│       └── ffi.rs               # FFI辅助函数
├── derive/                       # 宏库（已存在）
│   ├── Cargo.toml
│   └── src/lib.rs
├── driver-sdk/                   # 驱动开发SDK ⭐ 新增
│   ├── Cargo.toml
│   └── src/lib.rs
└── src/                          # 主程序
    └── ...
```

### 1. driver-api 公共库

```toml
# driver-api/Cargo.toml
[package]
name = "iot-edge-driver-api"
version = "1.0.0"
edition = "2021"

[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
chrono = "0.4"

# 最小依赖，不依赖主程序
```

```rust
// driver-api/src/lib.rs

pub mod driver;
pub mod types;
pub mod error;
pub mod ffi;

pub use driver::*;
pub use types::*;
pub use error::*;
pub use ffi::*;
```

```rust
// driver-api/src/types.rs

use serde::{Deserialize, Serialize};

/// 设备基础信息（精简版）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub display_name: Option<String>,
    pub protocol_type: Option<String>,
    pub driver_options: Option<String>,  // JSON字符串
    pub address: Option<String>,
    pub enabled: bool,
}

/// 设备命令
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCommand {
    pub id: String,
    pub name: String,
    pub command_type: String,
    pub parameters: Option<String>,  // JSON字符串
}

/// 读取结果值
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultValue {
    pub name: String,
    pub value_type: String,
    pub value: Option<String>,
}

impl ResultValue {
    pub fn new(name: String, value_type: String, value: Option<String>) -> Self {
        Self { name, value_type, value }
    }
    
    pub fn integer(name: String, value: i64) -> Self {
        Self::new(name, "int".to_string(), Some(value.to_string()))
    }
    
    pub fn float(name: String, value: f64) -> Self {
        Self::new(name, "float".to_string(), Some(value.to_string()))
    }
    
    pub fn string(name: String, value: String) -> Self {
        Self::new(name, "string".to_string(), Some(value))
    }
    
    pub fn boolean(name: String, value: bool) -> Self {
        Self::new(name, "boolean".to_string(), Some(value.to_string()))
    }
}

/// 驱动配置管理器
#[derive(Debug, Clone)]
pub struct DriverConfig {
    config: std::collections::HashMap<String, String>,
}

impl DriverConfig {
    pub fn from_device(device: &Device) -> Self {
        let mut config = std::collections::HashMap::new();
        
        if let Some(ref driver_options) = device.driver_options {
            if let Ok(parsed) = serde_json::from_str::<std::collections::HashMap<String, serde_json::Value>>(driver_options) {
                for (key, value) in parsed {
                    config.insert(key, value.to_string().trim_matches('"').to_string());
                }
            }
        }
        
        Self { config }
    }
    
    pub fn get_string(&self, key: &str, default: &str) -> String {
        self.config.get(key).cloned().unwrap_or_else(|| default.to_string())
    }
    
    pub fn get_integer(&self, key: &str, default: i64) -> i64 {
        self.config.get(key)
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(default)
    }
    
    pub fn get_float(&self, key: &str, default: f64) -> f64 {
        self.config.get(key)
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(default)
    }
    
    pub fn get_boolean(&self, key: &str, default: bool) -> bool {
        self.config.get(key)
            .and_then(|v| match v.to_lowercase().as_str() {
                "true" | "1" | "yes" => Some(true),
                "false" | "0" | "no" => Some(false),
                _ => v.parse::<bool>().ok(),
            })
            .unwrap_or(default)
    }
}

/// 组件选项（驱动配置项）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentOption {
    pub label: String,
    pub name: String,
    pub default_value: String,
    pub option_type: String,
    pub required: bool,
}

impl ComponentOption {
    pub fn new(
        label: String,
        name: String,
        default_value: String,
        option_type: String,
        required: bool,
    ) -> Self {
        Self {
            label,
            name,
            default_value,
            option_type,
            required,
        }
    }
}

/// 组件信息（驱动元数据）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentInfo {
    pub name: String,
    pub version: String,
    pub class_name: String,
    pub description: Option<String>,
    pub options_descriptors: Vec<ComponentOption>,
}
```

```rust
// driver-api/src/error.rs

use std::fmt;

/// 驱动错误类型
#[derive(Debug, Clone)]
pub enum DriverError {
    /// 网络错误
    NetworkError(String),
    /// IO错误
    IOError(String),
    /// 配置错误
    ConfigError(String),
    /// 验证错误
    ValidationError(String),
    /// 不支持的操作
    Unsupported(String),
    /// 内部错误
    Internal(String),
}

impl fmt::Display for DriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriverError::NetworkError(msg) => write!(f, "Network error: {}", msg),
            DriverError::IOError(msg) => write!(f, "IO error: {}", msg),
            DriverError::ConfigError(msg) => write!(f, "Config error: {}", msg),
            DriverError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
            DriverError::Unsupported(msg) => write!(f, "Unsupported: {}", msg),
            DriverError::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl std::error::Error for DriverError {}

pub type Result<T> = std::result::Result<T, DriverError>;
```

```rust
// driver-api/src/driver.rs

use crate::{Device, DeviceCommand, ResultValue, DriverError, Result};
use std::collections::HashMap;

/// 设备驱动trait（核心接口）
pub trait DeviceDriver: Send + Sync {
    /// 获取设备引用
    fn device(&self) -> &Device;
    
    /// 获取设备可变引用
    fn device_mut(&mut self) -> &mut Device;
    
    /// 读取设备数据
    fn read_data(&mut self) -> Result<Vec<ResultValue>>;
    
    /// 执行设备命令
    fn execute_command(&mut self, cmd: &DeviceCommand) -> Result<bool>;
    
    /// 获取驱动默认配置（可选实现）
    fn default_config(&self) -> HashMap<String, String> {
        HashMap::new()
    }
}
```

```rust
// driver-api/src/ffi.rs

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

/// FFI辅助函数：将Rust字符串转换为C字符串指针
pub fn to_c_string(s: &str) -> *const c_char {
    CString::new(s).unwrap().into_raw()
}

/// FFI辅助函数：从C字符串指针读取Rust字符串
pub unsafe fn from_c_string(ptr: *const c_char) -> String {
    CStr::from_ptr(ptr).to_string_lossy().into_owned()
}

/// FFI辅助函数：释放C字符串
pub unsafe fn free_c_string(ptr: *const c_char) {
    if !ptr.is_null() {
        let _ = CString::from_raw(ptr as *mut c_char);
    }
}
```

### 2. driver-sdk 开发SDK

```toml
# driver-sdk/Cargo.toml
[package]
name = "iot-edge-driver-sdk"
version = "1.0.0"
edition = "2021"

[dependencies]
iot-edge-driver-api = { path = "../driver-api" }
edge-derive = { path = "../derive" }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
```

```rust
// driver-sdk/src/lib.rs

// 重新导出公共API
pub use iot_edge_driver_api::*;

// 重新导出宏
pub use tinyiothub_derive::DeviceDriver;

// 提供便捷的宏和辅助函数
pub mod macros;
pub mod helpers;

/// 插件导出宏
#[macro_export]
macro_rules! export_driver {
    ($driver_type:ty) => {
        use std::ffi::{CStr, CString};
        use std::os::raw::c_char;
        use $crate::*;

        #[no_mangle]
        pub extern "C" fn iot_edge_driver_info() -> *const c_char {
            let info = <$driver_type>::get_driver_info();
            let json = serde_json::to_string(&info).unwrap();
            to_c_string(&json)
        }

        #[no_mangle]
        pub extern "C" fn iot_edge_driver_create(
            device_json: *const c_char,
            _context_json: *const c_char,
        ) -> *mut std::ffi::c_void {
            unsafe {
                let device_str = from_c_string(device_json);
                let device: Device = serde_json::from_str(&device_str).unwrap();
                
                let driver = Box::new(<$driver_type>::new(device));
                Box::into_raw(driver) as *mut std::ffi::c_void
            }
        }

        #[no_mangle]
        pub extern "C" fn iot_edge_driver_destroy(driver: *mut std::ffi::c_void) {
            unsafe {
                let _ = Box::from_raw(driver as *mut $driver_type);
            }
        }
    };
}
```

### 3. 更新derive宏

```toml
# derive/Cargo.toml
[package]
name = "edge-derive"
version = "1.0.0"
edition = "2021"

[lib]
proc-macro = true

[dependencies]
syn = "2.0"
quote = "1.0"
proc-macro2 = "1.0"

# 不依赖主程序，只依赖公共API
```

```rust
// derive/src/lib.rs (更新)

// 生成的代码引用公共API
let expanded = quote! {
    impl #name {
        pub fn get_driver_info() -> iot_edge_driver_api::ComponentInfo {
            let opts = vec![
                #(#options_code),*
            ];

            iot_edge_driver_api::ComponentInfo {
                name: #driver_name.to_string(),
                version: #version.to_string(),
                class_name: #class_name.to_string(),
                description: #description_code,
                options_descriptors: opts,
            }
        }

        pub fn get_default_config() -> std::collections::HashMap<String, String> {
            let mut config = std::collections::HashMap::new();
            #(#default_config_entries)*
            config
        }
    }
};
```

### 4. 主程序依赖关系

```toml
# Cargo.toml (主程序)
[package]
name = "tinyiothub"
version = "1.0.0"

[dependencies]
# 依赖公共API
iot-edge-driver-api = { path = "driver-api" }
edge-derive = { path = "derive" }

# 其他依赖...
tokio = { version = "1", features = ["full"] }
axum = "0.7"
# ...

[workspace]
members = [
    ".",
    "driver-api",      # 公共API库
    "driver-sdk",      # 驱动SDK
    "derive",          # 宏库
]
```

### 5. 插件项目结构

```
custom-driver/
├── Cargo.toml
└── src/
    └── lib.rs

# custom-driver/Cargo.toml
[package]
name = "custom-driver"
version = "1.0.0"

[lib]
crate-type = ["cdylib"]

[dependencies]
# 只依赖SDK，不依赖主程序 ⭐
iot-edge-driver-sdk = { path = "../driver-sdk" }
```

```rust
// custom-driver/src/lib.rs

use iot_edge_driver_sdk::*;

#[derive(DeviceDriver)]
#[driver(name = "CustomDriver", version = "1.0.0")]
#[driver_option(label = "API Key", name = "api_key", default = "")]
pub struct CustomDriver {
    device: Device,
    api_key: String,
}

impl CustomDriver {
    pub fn new(device: Device) -> Self {
        let config = DriverConfig::from_device(&device);
        let api_key = config.get_string("api_key", "");
        
        Self { device, api_key }
    }
}

impl DeviceDriver for CustomDriver {
    fn device(&self) -> &Device {
        &self.device
    }
    
    fn device_mut(&mut self) -> &mut Device {
        &mut self.device
    }
    
    fn read_data(&mut self) -> Result<Vec<ResultValue>> {
        Ok(vec![
            ResultValue::float("temperature".to_string(), 25.5),
        ])
    }
    
    fn execute_command(&mut self, cmd: &DeviceCommand) -> Result<bool> {
        Ok(true)
    }
}

// 使用宏导出驱动
export_driver!(CustomDriver);
```

## 依赖关系图

```
┌─────────────────────────────────────────────────────────┐
│                                                          │
│  ┌──────────────────┐         ┌──────────────────┐     │
│  │  driver-api      │◄────────│  derive          │     │
│  │  (公共接口)      │         │  (宏库)          │     │
│  └──────────────────┘         └──────────────────┘     │
│           ▲                            ▲                │
│           │                            │                │
│           │                            │                │
│  ┌────────┴────────┐          ┌───────┴────────┐       │
│  │  driver-sdk     │          │  主程序         │       │
│  │  (开发SDK)      │          │  iot-edge       │       │
│  └─────────────────┘          └─────────────────┘       │
│           ▲                                              │
│           │                                              │
│           │                                              │
│  ┌────────┴────────┐                                    │
│  │  custom-driver  │                                    │
│  │  (插件)         │                                    │
│  └─────────────────┘                                    │
│                                                          │
└─────────────────────────────────────────────────────────┘

依赖方向：
- 插件只依赖 driver-sdk
- driver-sdk 依赖 driver-api + derive
- 主程序依赖 driver-api + derive
- derive 依赖 driver-api（生成代码时引用）
- driver-api 无外部依赖（最小化）
```

## 优势总结

### 1. 依赖单一
- ✅ 插件只需依赖 `driver-sdk`
- ✅ 不需要依赖整个主程序
- ✅ 编译快速

### 2. 版本独立
- ✅ API版本独立于主程序版本
- ✅ 向后兼容性好
- ✅ 插件可以独立发布

### 3. 接口稳定
- ✅ 公共API变化少
- ✅ 主程序内部重构不影响插件
- ✅ ABI稳定

### 4. 开发体验好
- ✅ SDK提供便捷宏
- ✅ 类型安全
- ✅ 文档完整

## 实施步骤

### Phase 1: 创建公共库 (1天)
1. 创建 `driver-api` 包
2. 提取核心类型定义
3. 定义 `DeviceDriver` trait
4. 实现FFI辅助函数

### Phase 2: 创建SDK (1天)
1. 创建 `driver-sdk` 包
2. 实现 `export_driver!` 宏
3. 提供辅助函数和工具

### Phase 3: 更新derive宏 (半天)
1. 修改宏生成代码引用公共API
2. 测试宏功能

### Phase 4: 集成测试 (1-2天)
1. 创建示例插件
2. 测试编译和加载
3. 验证功能完整性

这个设计是否满足您的要求？
