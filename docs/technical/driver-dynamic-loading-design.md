# 驱动动态加载原型设计

## 设计目标

1. **向后兼容** - 现有静态驱动无需修改
2. **类型安全** - 尽可能保持Rust类型安全
3. **简单易用** - 第三方开发者容易上手
4. **性能可控** - FFI开销最小化
5. **安全可靠** - 内存安全和错误隔离

## 架构设计

### 1. 整体架构图

```
┌─────────────────────────────────────────────────────────┐
│                      TinyIoTHub                          │
│                                                          │
│  ┌────────────────────────────────────────────────┐    │
│  │         Driver Registry (统一注册表)            │    │
│  │  ┌──────────────┐    ┌──────────────────────┐ │    │
│  │  │ Static       │    │ Dynamic              │ │    │
│  │  │ Drivers      │    │ Drivers              │ │    │
│  │  │ (编译时)     │    │ (运行时)             │ │    │
│  │  └──────────────┘    └──────────────────────┘ │    │
│  └────────────────────────────────────────────────┘    │
│           │                        │                     │
│           ▼                        ▼                     │
│  ┌─────────────────┐    ┌──────────────────────┐      │
│  │ SimulatedDriver │    │ DynamicDriverLoader  │      │
│  │ ModbusDriver    │    │  - libloading        │      │
│  │ SnmpDriver      │    │  - FFI Bridge        │      │
│  └─────────────────┘    │  - Safety Wrapper    │      │
│                          └──────────────────────┘      │
│                                   │                     │
│                                   ▼                     │
│                          ┌──────────────────────┐      │
│                          │  Plugin Directory    │      │
│                          │  plugins/*.so/dll    │      │
│                          └──────────────────────┘      │
└─────────────────────────────────────────────────────────┘
```

### 2. 模块划分

```
src/domain/device/driver/
├── mod.rs                    # 现有入口，保持不变
├── driver.rs                 # DeviceDriver trait
├── drivers/                  # 静态驱动实现
│   ├── mod.rs
│   ├── simulated_driver.rs
│   ├── modbus_driver.rs
│   └── snmp_driver.rs
└── dynamic/                  # 新增：动态加载模块
    ├── mod.rs               # 动态加载入口
    ├── ffi.rs               # FFI接口定义
    ├── loader.rs            # 动态加载器
    ├── wrapper.rs           # 驱动包装器
    ├── registry.rs          # 统一注册表
    └── safety.rs            # 安全检查
```

## 核心接口设计

### 1. FFI接口层 (ffi.rs)

```rust
// FFI安全的数据结构
use std::os::raw::{c_char, c_void, c_int};

/// 驱动ABI版本
pub const DRIVER_ABI_VERSION: u32 = 1;

/// 驱动元数据 (FFI安全)
#[repr(C)]
pub struct DriverMetadata {
    pub abi_version: u32,
    pub name: *const c_char,
    pub version: *const c_char,
    pub description: *const c_char,
    pub author: *const c_char,
}

/// 设备数据 (FFI安全)
#[repr(C)]
pub struct CDevice {
    pub id: *const c_char,
    pub name: *const c_char,
    pub protocol_type: *const c_char,
    pub driver_options: *const c_char,  // JSON字符串
}

/// 读取结果 (FFI安全)
#[repr(C)]
pub struct CResultValue {
    pub name: *const c_char,
    pub value_type: *const c_char,
    pub value: *const c_char,
}

/// 错误信息 (FFI安全)
#[repr(C)]
pub struct CError {
    pub code: c_int,
    pub message: *const c_char,
}

/// 驱动虚函数表
#[repr(C)]
pub struct DriverVTable {
    /// 读取设备数据
    /// 返回: 0=成功, 非0=错误码
    pub read_data: extern "C" fn(
        driver: *mut c_void,
        results: *mut *mut CResultValue,
        count: *mut usize,
        error: *mut CError,
    ) -> c_int,
    
    /// 执行设备命令
    pub execute_command: extern "C" fn(
        driver: *mut c_void,
        command_json: *const c_char,
        error: *mut CError,
    ) -> c_int,
    
    /// 销毁驱动实例
    pub destroy: extern "C" fn(driver: *mut c_void),
}

/// 驱动插件导出的主接口
#[repr(C)]
pub struct DriverPlugin {
    pub metadata: DriverMetadata,
    pub vtable: DriverVTable,
    
    /// 创建驱动实例
    pub create: extern "C" fn(
        device: *const CDevice,
        context_json: *const c_char,
    ) -> *mut c_void,
}

// 插件必须导出的符号
pub const PLUGIN_ENTRY_SYMBOL: &[u8] = b"iot_edge_driver_plugin\0";
```

### 2. 统一注册表 (registry.rs)

```rust
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// 驱动工厂trait
pub trait DriverFactory: Send + Sync {
    fn create(
        &self,
        device: Device,
        context: Arc<DataContext>,
    ) -> Result<Box<dyn DeviceDriver>, Error>;
    
    fn metadata(&self) -> DriverInfo;
}

/// 静态驱动工厂
struct StaticDriverFactory<F>
where
    F: Fn(Device, Arc<DataContext>) -> Box<dyn DeviceDriver> + Send + Sync,
{
    factory_fn: F,
    info: DriverInfo,
}

/// 动态驱动工厂
struct DynamicDriverFactory {
    loader: Arc<DynamicDriverLoader>,
    plugin_path: PathBuf,
    info: DriverInfo,
}

/// 统一驱动注册表
pub struct UnifiedDriverRegistry {
    factories: RwLock<HashMap<String, Arc<dyn DriverFactory>>>,
}

impl UnifiedDriverRegistry {
    pub fn new() -> Self {
        Self {
            factories: RwLock::new(HashMap::new()),
        }
    }
    
    /// 注册静态驱动
    pub fn register_static<F>(
        &self,
        name: String,
        info: DriverInfo,
        factory: F,
    ) where
        F: Fn(Device, Arc<DataContext>) -> Box<dyn DeviceDriver> + Send + Sync + 'static,
    {
        let factory = Arc::new(StaticDriverFactory {
            factory_fn: factory,
            info,
        });
        self.factories.write().unwrap().insert(name, factory);
    }
    
    /// 注册动态驱动
    pub fn register_dynamic(
        &self,
        name: String,
        plugin_path: PathBuf,
    ) -> Result<(), Error> {
        let loader = Arc::new(DynamicDriverLoader::new());
        let info = loader.probe_plugin(&plugin_path)?;
        
        let factory = Arc::new(DynamicDriverFactory {
            loader,
            plugin_path,
            info,
        });
        
        self.factories.write().unwrap().insert(name, factory);
        Ok(())
    }
    
    /// 创建驱动实例
    pub fn create_driver(
        &self,
        name: &str,
        device: Device,
        context: Arc<DataContext>,
    ) -> Result<Box<dyn DeviceDriver>, Error> {
        let factories = self.factories.read().unwrap();
        let factory = factories.get(name)
            .ok_or_else(|| Error::NotFound)?;
        
        factory.create(device, context)
    }
    
    /// 获取所有驱动信息
    pub fn list_drivers(&self) -> Vec<DriverInfo> {
        self.factories.read().unwrap()
            .values()
            .map(|f| f.metadata())
            .collect()
    }
}
```

### 3. 动态加载器 (loader.rs)

```rust
use libloading::{Library, Symbol};

pub struct DynamicDriverLoader {
    // 保持库引用，防止卸载
    libraries: RwLock<HashMap<PathBuf, Arc<Library>>>,
}

impl DynamicDriverLoader {
    pub fn new() -> Self {
        Self {
            libraries: RwLock::new(HashMap::new()),
        }
    }
    
    /// 探测插件信息（不加载）
    pub fn probe_plugin(&self, path: &Path) -> Result<DriverInfo, Error> {
        unsafe {
            let lib = Library::new(path)?;
            let get_plugin: Symbol<extern "C" fn() -> *const DriverPlugin> = 
                lib.get(PLUGIN_ENTRY_SYMBOL)?;
            
            let plugin = &*get_plugin();
            self.validate_abi(&plugin.metadata)?;
            
            let info = self.convert_metadata(&plugin.metadata);
            Ok(info)
        }
    }
    
    /// 加载驱动插件
    pub fn load_plugin(
        &self,
        path: &Path,
        device: &Device,
        context: Arc<DataContext>,
    ) -> Result<Box<dyn DeviceDriver>, Error> {
        let lib = self.get_or_load_library(path)?;
        
        unsafe {
            let get_plugin: Symbol<extern "C" fn() -> *const DriverPlugin> = 
                lib.get(PLUGIN_ENTRY_SYMBOL)?;
            
            let plugin = &*get_plugin();
            self.validate_abi(&plugin.metadata)?;
            
            // 转换设备数据为C格式
            let c_device = self.convert_device(device)?;
            let context_json = self.serialize_context(&context)?;
            
            // 创建驱动实例
            let driver_ptr = (plugin.create)(&c_device, context_json.as_ptr());
            if driver_ptr.is_null() {
                return Err(Error::Internal("Failed to create driver".into()));
            }
            
            // 包装为Rust trait对象
            let wrapper = DynamicDriverWrapper::new(
                lib.clone(),
                driver_ptr,
                plugin.vtable,
                device.clone(),
            );
            
            Ok(Box::new(wrapper))
        }
    }
    
    fn validate_abi(&self, metadata: &DriverMetadata) -> Result<(), Error> {
        if metadata.abi_version != DRIVER_ABI_VERSION {
            return Err(Error::Unsupported(
                format!("ABI version mismatch: expected {}, got {}",
                    DRIVER_ABI_VERSION, metadata.abi_version)
            ));
        }
        Ok(())
    }
    
    fn get_or_load_library(&self, path: &Path) -> Result<Arc<Library>, Error> {
        let mut libs = self.libraries.write().unwrap();
        
        if let Some(lib) = libs.get(path) {
            return Ok(lib.clone());
        }
        
        let lib = Arc::new(unsafe { Library::new(path)? });
        libs.insert(path.to_path_buf(), lib.clone());
        Ok(lib)
    }
}
```

### 4. 驱动包装器 (wrapper.rs)

```rust
/// 动态驱动包装器 - 实现DeviceDriver trait
pub struct DynamicDriverWrapper {
    library: Arc<Library>,
    driver_ptr: *mut c_void,
    vtable: DriverVTable,
    device: Device,
}

unsafe impl Send for DynamicDriverWrapper {}
unsafe impl Sync for DynamicDriverWrapper {}

impl DynamicDriverWrapper {
    pub fn new(
        library: Arc<Library>,
        driver_ptr: *mut c_void,
        vtable: DriverVTable,
        device: Device,
    ) -> Self {
        Self {
            library,
            driver_ptr,
            vtable,
            device,
        }
    }
}

impl DeviceDriver for DynamicDriverWrapper {
    fn device(&self) -> &Device {
        &self.device
    }
    
    fn device_mut(&mut self) -> &mut Device {
        &mut self.device
    }
    
    fn read_data(&mut self) -> Result<Vec<ResultValue>, Error> {
        unsafe {
            let mut results_ptr: *mut CResultValue = std::ptr::null_mut();
            let mut count: usize = 0;
            let mut error = CError {
                code: 0,
                message: std::ptr::null(),
            };
            
            let ret = (self.vtable.read_data)(
                self.driver_ptr,
                &mut results_ptr,
                &mut count,
                &mut error,
            );
            
            if ret != 0 {
                let err_msg = if !error.message.is_null() {
                    CStr::from_ptr(error.message).to_string_lossy().into_owned()
                } else {
                    format!("Driver error: {}", ret)
                };
                return Err(Error::Internal(err_msg));
            }
            
            // 转换C数据为Rust
            let c_slice = std::slice::from_raw_parts(results_ptr, count);
            let results: Result<Vec<_>, _> = c_slice.iter()
                .map(|c_val| self.convert_result_value(c_val))
                .collect();
            
            // 释放C内存（由插件分配）
            libc::free(results_ptr as *mut c_void);
            
            results
        }
    }
    
    fn execute_command(&mut self, cmd: &DeviceCommand) -> Result<bool, Error> {
        unsafe {
            let cmd_json = serde_json::to_string(cmd)?;
            let c_cmd = CString::new(cmd_json)?;
            let mut error = CError {
                code: 0,
                message: std::ptr::null(),
            };
            
            let ret = (self.vtable.execute_command)(
                self.driver_ptr,
                c_cmd.as_ptr(),
                &mut error,
            );
            
            if ret != 0 {
                let err_msg = if !error.message.is_null() {
                    CStr::from_ptr(error.message).to_string_lossy().into_owned()
                } else {
                    format!("Command failed: {}", ret)
                };
                return Err(Error::Internal(err_msg));
            }
            
            Ok(true)
        }
    }
    
    fn convert_result_value(&self, c_val: &CResultValue) -> Result<ResultValue, Error> {
        unsafe {
            let name = CStr::from_ptr(c_val.name).to_string_lossy().into_owned();
            let value_type = CStr::from_ptr(c_val.value_type).to_string_lossy().into_owned();
            let value = if !c_val.value.is_null() {
                Some(CStr::from_ptr(c_val.value).to_string_lossy().into_owned())
            } else {
                None
            };
            
            Ok(ResultValue {
                name,
                value_type,
                value,
            })
        }
    }
}

impl Drop for DynamicDriverWrapper {
    fn drop(&mut self) {
        unsafe {
            (self.vtable.destroy)(self.driver_ptr);
        }
    }
}
```

## 使用示例

### 1. 主程序集成

```rust
// src/domain/device/driver/mod.rs

use dynamic::{UnifiedDriverRegistry, DynamicDriverLoader};

// 全局注册表
static DRIVER_REGISTRY: Lazy<UnifiedDriverRegistry> = Lazy::new(|| {
    let registry = UnifiedDriverRegistry::new();
    
    // 注册静态驱动（现有方式）
    registry.register_static(
        "simulator".to_string(),
        SimulatedDriver::get_driver_info(),
        |device, context| Box::new(SimulatedDriver::new(device, context)),
    );
    
    registry.register_static(
        "ModbusDriver".to_string(),
        ModbusDriver::get_driver_info(),
        |device, context| Box::new(ModbusDriver::new(device, context)),
    );
    
    // 自动发现并注册动态驱动
    if let Ok(plugin_dir) = std::env::var("DRIVER_PLUGIN_DIR") {
        if let Err(e) = load_dynamic_drivers(&registry, &plugin_dir) {
            tracing::warn!("Failed to load dynamic drivers: {}", e);
        }
    }
    
    registry
});

fn load_dynamic_drivers(
    registry: &UnifiedDriverRegistry,
    plugin_dir: &str,
) -> Result<(), Error> {
    let path = Path::new(plugin_dir);
    if !path.exists() {
        return Ok(());
    }
    
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        
        if is_plugin_file(&path) {
            match registry.register_dynamic(
                path.file_stem().unwrap().to_string_lossy().into_owned(),
                path,
            ) {
                Ok(_) => tracing::info!("Loaded plugin: {:?}", path),
                Err(e) => tracing::warn!("Failed to load plugin {:?}: {}", path, e),
            }
        }
    }
    
    Ok(())
}

fn is_plugin_file(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        ext == "so" || ext == "dll" || ext == "dylib"
    } else {
        false
    }
}

/// 创建驱动实例（统一入口）
pub fn create_driver(
    driver_name: &str,
    device: &Device,
    context: Arc<DataContext>,
) -> Result<DriverWrapper, Error> {
    let base_driver = DRIVER_REGISTRY.create_driver(driver_name, device.clone(), context)?;
    Ok(DriverWrapper::new(base_driver))
}
```

### 2. 插件开发示例

```rust
// custom_driver_plugin/src/lib.rs

use iot_edge_driver_ffi::*;
use std::ffi::{CStr, CString};

// 驱动实现
struct CustomDriver {
    device_id: String,
    // ... 驱动状态
}

impl CustomDriver {
    fn new(device: &CDevice) -> Self {
        unsafe {
            let id = CStr::from_ptr(device.id).to_string_lossy().into_owned();
            Self {
                device_id: id,
            }
        }
    }
    
    fn read_data_impl(&mut self) -> Result<Vec<CResultValue>, String> {
        // 实现数据读取逻辑
        Ok(vec![
            CResultValue {
                name: CString::new("temperature").unwrap().into_raw(),
                value_type: CString::new("float").unwrap().into_raw(),
                value: CString::new("25.5").unwrap().into_raw(),
            }
        ])
    }
}

// FFI导出函数
extern "C" fn custom_read_data(
    driver: *mut c_void,
    results: *mut *mut CResultValue,
    count: *mut usize,
    error: *mut CError,
) -> c_int {
    let driver = unsafe { &mut *(driver as *mut CustomDriver) };
    
    match driver.read_data_impl() {
        Ok(data) => {
            let len = data.len();
            let ptr = Box::into_raw(data.into_boxed_slice()) as *mut CResultValue;
            
            unsafe {
                *results = ptr;
                *count = len;
            }
            0
        }
        Err(e) => {
            unsafe {
                (*error).code = -1;
                (*error).message = CString::new(e).unwrap().into_raw();
            }
            -1
        }
    }
}

extern "C" fn custom_execute_command(
    driver: *mut c_void,
    command_json: *const c_char,
    error: *mut CError,
) -> c_int {
    // 实现命令执行
    0
}

extern "C" fn custom_destroy(driver: *mut c_void) {
    unsafe {
        let _ = Box::from_raw(driver as *mut CustomDriver);
    }
}

extern "C" fn custom_create(
    device: *const CDevice,
    _context_json: *const c_char,
) -> *mut c_void {
    let driver = Box::new(CustomDriver::new(unsafe { &*device }));
    Box::into_raw(driver) as *mut c_void
}

// 插件入口
#[no_mangle]
pub extern "C" fn iot_edge_driver_plugin() -> *const DriverPlugin {
    static METADATA: DriverMetadata = DriverMetadata {
        abi_version: DRIVER_ABI_VERSION,
        name: c_str!("CustomDriver"),
        version: c_str!("1.0.0"),
        description: c_str!("Custom Device Driver"),
        author: c_str!("Your Name"),
    };
    
    static VTABLE: DriverVTable = DriverVTable {
        read_data: custom_read_data,
        execute_command: custom_execute_command,
        destroy: custom_destroy,
    };
    
    static PLUGIN: DriverPlugin = DriverPlugin {
        metadata: METADATA,
        vtable: VTABLE,
        create: custom_create,
    };
    
    &PLUGIN
}

// Cargo.toml
// [lib]
// crate-type = ["cdylib"]
```

## 配置文件设计

```toml
# config/drivers.toml

# 驱动插件目录
plugin_dir = "plugins"

# 自动加载所有插件
auto_load = true

# 显式配置的驱动
[[drivers]]
name = "simulator"
type = "static"  # 静态编译

[[drivers]]
name = "ModbusDriver"
type = "static"

[[drivers]]
name = "CustomDriver"
type = "dynamic"
plugin_path = "plugins/custom_driver.so"
enabled = true

[[drivers]]
name = "ThirdPartyDriver"
type = "dynamic"
plugin_path = "plugins/third_party.dll"
enabled = false  # 禁用
```

## 安全性设计

### 1. ABI版本检查

```rust
fn validate_abi(metadata: &DriverMetadata) -> Result<(), Error> {
    if metadata.abi_version != DRIVER_ABI_VERSION {
        return Err(Error::Unsupported(
            format!("ABI version mismatch")
        ));
    }
    Ok(())
}
```

### 2. 内存安全

- 使用 `Arc<Library>` 确保库在驱动之前不被卸载
- 所有C指针转换都在 `unsafe` 块中
- 使用 `Drop` trait 确保资源释放

### 3. 错误隔离

```rust
// 捕获插件panic
fn safe_call<F, R>(f: F) -> Result<R, Error>
where
    F: FnOnce() -> R + std::panic::UnwindSafe,
{
    std::panic::catch_unwind(f)
        .map_err(|_| Error::Internal("Plugin panicked".into()))
}
```

## 性能优化

1. **延迟加载** - 首次使用时才加载插件
2. **库缓存** - 同一插件只加载一次
3. **零拷贝** - 尽量使用指针传递
4. **批量操作** - 减少FFI调用次数

## 测试策略

### 1. 单元测试

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_static_driver_registration() {
        let registry = UnifiedDriverRegistry::new();
        registry.register_static(/* ... */);
        assert!(registry.list_drivers().len() > 0);
    }
    
    #[test]
    fn test_abi_version_check() {
        // 测试版本不匹配
    }
}
```

### 2. 集成测试

```rust
#[test]
fn test_load_sample_plugin() {
    let loader = DynamicDriverLoader::new();
    let plugin = loader.load_plugin(
        Path::new("tests/fixtures/sample_plugin.so"),
        &test_device(),
        test_context(),
    );
    assert!(plugin.is_ok());
}
```

## 文档和工具

### 1. 开发者文档

- FFI接口规范
- 插件开发指南
- 示例代码
- 最佳实践

### 2. 开发工具

- 插件模板生成器
- ABI兼容性检查工具
- 插件测试框架

## 实施计划

### Phase 1: 基础架构 (1周)
- [ ] 定义FFI接口
- [ ] 实现统一注册表
- [ ] 实现动态加载器
- [ ] 实现驱动包装器

### Phase 2: 集成测试 (3-4天)
- [ ] 创建示例插件
- [ ] 集成到现有系统
- [ ] 性能测试
- [ ] 安全性测试

### Phase 3: 文档和工具 (3-4天)
- [ ] 编写开发者文档
- [ ] 创建插件模板
- [ ] 提供示例代码

## 风险评估

| 风险 | 影响 | 概率 | 缓解措施 |
|------|------|------|----------|
| ABI不稳定 | 高 | 中 | 版本检查+向后兼容 |
| 内存泄漏 | 高 | 低 | 严格测试+工具检查 |
| 性能下降 | 中 | 低 | 基准测试+优化 |
| 安全漏洞 | 高 | 低 | 代码审查+沙箱 |

## 下一步

请审查此设计，确认以下方面：

1. ✅ FFI接口设计是否合理？
2. ✅ 统一注册表架构是否满足需求？
3. ✅ 安全性措施是否充分？
4. ✅ 性能影响是否可接受？
5. ✅ 实施计划是否可行？

确认后即可开始实施。
