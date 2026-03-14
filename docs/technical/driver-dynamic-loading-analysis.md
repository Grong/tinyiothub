# 驱动动态加载可行性分析

## 当前架构分析

### 1. 现有驱动系统

#### 架构特点
```rust
// 当前使用编译时注册
tinyiothub_derive::register_drivers! {
    SimulatedDriver,
    ModbusDriver,
    SnmpDriver,
}
```

**优点**:
- ✅ 类型安全 - 编译时检查
- ✅ 零运行时开销 - 静态分发
- ✅ 简单直接 - 无需复杂的加载逻辑
- ✅ 调试友好 - 完整的堆栈跟踪

**缺点**:
- ❌ 添加驱动需要重新编译
- ❌ 无法热更新驱动
- ❌ 无法按需加载驱动
- ❌ 第三方驱动集成困难

### 2. 驱动接口定义

```rust
pub trait DeviceDriver: Send + Sync {
    fn device(&self) -> &Device;
    fn device_mut(&mut self) -> &mut Device;
    fn read_data(&mut self) -> Result<Vec<ResultValue>, Error>;
    fn execute_command(&mut self, cmd: &DeviceCommand) -> Result<bool, Error>;
    fn retry_config(&self) -> RetryConfig;
    fn retry_policy(&self) -> Box<dyn RetryPolicy>;
    // ... 其他方法
}
```

**关键特征**:
- Trait对象安全 (已使用 `Box<dyn DeviceDriver>`)
- 支持动态分发
- 包含生命周期管理

### 3. 驱动注册机制

```rust
// 宏生成的注册表
static DRIVER_REGISTRY: Lazy<HashMap<String, DriverFactory>> = Lazy::new(|| {
    let mut registry = HashMap::new();
    registry.insert("simulator", Box::new(|device, context| {
        Box::new(SimulatedDriver::new(device, context))
    }));
    // ...
    registry
});
```

## 动态加载方案设计

### 方案1: 基于 libloading 的动态库加载 ⭐ 推荐

#### 架构设计

```rust
// 1. 定义驱动插件接口 (FFI安全)
#[repr(C)]
pub struct DriverPlugin {
    pub name: *const c_char,
    pub version: *const c_char,
    pub create: extern "C" fn(*const Device, *const DataContext) -> *mut c_void,
    pub destroy: extern "C" fn(*mut c_void),
}

// 2. 驱动插件导出函数
#[no_mangle]
pub extern "C" fn _driver_plugin_init() -> *const DriverPlugin {
    &MODBUS_PLUGIN
}

// 3. 动态加载器
pub struct DynamicDriverLoader {
    libraries: HashMap<String, Library>,
    drivers: HashMap<String, Box<dyn DeviceDriver>>,
}

impl DynamicDriverLoader {
    pub fn load_driver(&mut self, path: &Path) -> Result<(), Error> {
        unsafe {
            let lib = Library::new(path)?;
            let init: Symbol<fn() -> *const DriverPlugin> = 
                lib.get(b"_driver_plugin_init")?;
            let plugin = init();
            // 注册驱动...
        }
    }
}
```

#### 优点
- ✅ 真正的动态加载
- ✅ 支持热更新（卸载/重新加载）
- ✅ 第三方驱动友好
- ✅ 按需加载，节省内存
- ✅ 驱动隔离，崩溃不影响主程序

#### 缺点
- ❌ FFI复杂性 - 需要C ABI
- ❌ 类型安全降低 - 运行时检查
- ❌ 调试困难 - 跨库调试
- ❌ 平台差异 - Windows/Linux/macOS
- ❌ 版本兼容性 - ABI稳定性问题

#### 实现复杂度
**高** - 需要处理FFI、内存管理、错误传播等

### 方案2: 基于 WASM 的插件系统

#### 架构设计

```rust
// 1. WASM驱动接口
use wasmtime::*;

pub struct WasmDriver {
    engine: Engine,
    module: Module,
    instance: Instance,
    store: Store<()>,
}

impl WasmDriver {
    pub fn load(wasm_path: &Path) -> Result<Self, Error> {
        let engine = Engine::default();
        let module = Module::from_file(&engine, wasm_path)?;
        // 创建实例...
    }
}

// 2. 驱动实现 (Rust -> WASM)
#[no_mangle]
pub extern "C" fn read_data() -> *const u8 {
    // 返回JSON序列化的数据
}
```

#### 优点
- ✅ 沙箱隔离 - 安全性高
- ✅ 跨平台 - 一次编译到处运行
- ✅ 轻量级 - 比动态库小
- ✅ 热更新友好
- ✅ 资源限制 - 可控制内存/CPU

#### 缺点
- ❌ 性能开销 - WASM运行时
- ❌ 功能受限 - WASI限制
- ❌ 生态不成熟 - 工具链复杂
- ❌ 调试困难
- ❌ 异步支持有限

#### 实现复杂度
**中高** - 需要WASM工具链和运行时

### 方案3: 基于进程的插件系统

#### 架构设计

```rust
// 1. 驱动进程通信
pub struct ProcessDriver {
    child: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
}

impl ProcessDriver {
    pub fn spawn(driver_path: &Path) -> Result<Self, Error> {
        let child = Command::new(driver_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        // IPC通信...
    }
    
    pub async fn read_data(&mut self) -> Result<Vec<ResultValue>, Error> {
        // 通过stdin/stdout或gRPC通信
        let request = DriverRequest::ReadData;
        self.send_request(&request).await?;
        self.receive_response().await
    }
}
```

#### 优点
- ✅ 最强隔离 - 进程级别
- ✅ 崩溃隔离 - 不影响主程序
- ✅ 语言无关 - 任何语言实现
- ✅ 资源限制 - OS级别控制
- ✅ 简单实现 - 标准IPC

#### 缺点
- ❌ 性能开销大 - 进程间通信
- ❌ 资源消耗 - 每个驱动一个进程
- ❌ 复杂的生命周期管理
- ❌ 调试复杂

#### 实现复杂度
**中** - IPC相对简单，但需要进程管理

### 方案4: 混合静态+动态注册 ⭐ 推荐（渐进式）

#### 架构设计

```rust
// 1. 扩展现有注册表支持动态注册
pub struct DriverRegistry {
    static_drivers: HashMap<String, DriverFactory>,
    dynamic_drivers: HashMap<String, Box<dyn DriverFactory>>,
}

impl DriverRegistry {
    // 编译时注册（现有方式）
    pub fn register_static(&mut self, name: &str, factory: DriverFactory) {
        self.static_drivers.insert(name.to_string(), factory);
    }
    
    // 运行时注册（新增）
    pub fn register_dynamic(&mut self, name: String, factory: Box<dyn DriverFactory>) {
        self.dynamic_drivers.insert(name, factory);
    }
    
    // 从配置文件加载驱动
    pub fn load_from_config(&mut self, config_path: &Path) -> Result<(), Error> {
        let config: DriverConfig = load_config(config_path)?;
        for driver_def in config.drivers {
            match driver_def.load_type {
                LoadType::Static => { /* 已编译 */ }
                LoadType::Dynamic => {
                    self.load_dynamic_driver(&driver_def)?;
                }
            }
        }
        Ok(())
    }
}

// 2. 驱动配置文件
// drivers.toml
[[drivers]]
name = "ModbusDriver"
load_type = "static"  # 编译时包含

[[drivers]]
name = "CustomDriver"
load_type = "dynamic"
library_path = "plugins/custom_driver.so"
```

#### 优点
- ✅ 渐进式迁移 - 兼容现有代码
- ✅ 灵活性 - 静态+动态混合
- ✅ 向后兼容 - 不破坏现有驱动
- ✅ 可选功能 - 按需启用动态加载
- ✅ 降低风险 - 逐步实施

#### 缺点
- ❌ 复杂度增加 - 两套机制
- ❌ 仍需FFI - 动态部分

#### 实现复杂度
**中** - 基于现有架构扩展

## 技术实现细节

### 1. FFI安全的驱动接口

```rust
// driver_ffi.rs
use std::os::raw::{c_char, c_void};

#[repr(C)]
pub struct CDevice {
    id: *const c_char,
    name: *const c_char,
    // ... 其他字段
}

#[repr(C)]
pub struct CResultValue {
    name: *const c_char,
    value_type: *const c_char,
    value: *const c_char,
}

#[repr(C)]
pub struct CDriverVTable {
    pub read_data: extern "C" fn(*mut c_void, *mut *mut CResultValue, *mut usize) -> i32,
    pub execute_command: extern "C" fn(*mut c_void, *const c_char) -> i32,
    pub destroy: extern "C" fn(*mut c_void),
}

// 驱动插件必须导出
#[no_mangle]
pub extern "C" fn driver_create(device: *const CDevice) -> *mut c_void {
    // 创建驱动实例
}

#[no_mangle]
pub extern "C" fn driver_vtable() -> *const CDriverVTable {
    // 返回虚函数表
}
```

### 2. 动态加载器实现

```rust
// dynamic_loader.rs
use libloading::{Library, Symbol};

pub struct DynamicDriverLoader {
    plugin_dir: PathBuf,
    loaded_libraries: HashMap<String, Library>,
}

impl DynamicDriverLoader {
    pub fn new(plugin_dir: PathBuf) -> Self {
        Self {
            plugin_dir,
            loaded_libraries: HashMap::new(),
        }
    }
    
    pub fn discover_drivers(&self) -> Result<Vec<DriverInfo>, Error> {
        let mut drivers = Vec::new();
        
        for entry in fs::read_dir(&self.plugin_dir)? {
            let path = entry?.path();
            if path.extension() == Some(OsStr::new(DLL_EXTENSION)) {
                if let Ok(info) = self.probe_driver(&path) {
                    drivers.push(info);
                }
            }
        }
        
        Ok(drivers)
    }
    
    pub unsafe fn load_driver(&mut self, path: &Path) -> Result<Box<dyn DeviceDriver>, Error> {
        let lib = Library::new(path)?;
        
        // 获取驱动元数据
        let get_info: Symbol<extern "C" fn() -> *const DriverInfo> = 
            lib.get(b"driver_get_info")?;
        let info = &*get_info();
        
        // 获取创建函数
        let create: Symbol<extern "C" fn(*const CDevice) -> *mut c_void> = 
            lib.get(b"driver_create")?;
        
        // 包装为Rust trait对象
        let wrapper = DynamicDriverWrapper::new(lib, create, info);
        
        self.loaded_libraries.insert(info.name.clone(), lib);
        
        Ok(Box::new(wrapper))
    }
}

const DLL_EXTENSION: &str = if cfg!(windows) {
    "dll"
} else if cfg!(target_os = "macos") {
    "dylib"
} else {
    "so"
};
```

### 3. 驱动包装器

```rust
// dynamic_wrapper.rs
pub struct DynamicDriverWrapper {
    library: Library,
    driver_ptr: *mut c_void,
    vtable: &'static CDriverVTable,
    device: Device,
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
            let mut data_ptr: *mut CResultValue = std::ptr::null_mut();
            let mut len: usize = 0;
            
            let result = (self.vtable.read_data)(
                self.driver_ptr,
                &mut data_ptr,
                &mut len
            );
            
            if result != 0 {
                return Err(Error::Internal("Driver read_data failed".to_string()));
            }
            
            // 转换C数据到Rust
            let c_slice = std::slice::from_raw_parts(data_ptr, len);
            let rust_data = c_slice.iter()
                .map(|c_val| self.convert_result_value(c_val))
                .collect();
            
            // 释放C内存
            libc::free(data_ptr as *mut c_void);
            
            Ok(rust_data)
        }
    }
    
    fn execute_command(&mut self, cmd: &DeviceCommand) -> Result<bool, Error> {
        unsafe {
            let cmd_json = serde_json::to_string(cmd)?;
            let c_cmd = CString::new(cmd_json)?;
            
            let result = (self.vtable.execute_command)(
                self.driver_ptr,
                c_cmd.as_ptr()
            );
            
            Ok(result == 0)
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

### 4. 驱动插件示例

```rust
// modbus_plugin/src/lib.rs
use iot_edge_driver_ffi::*;

struct ModbusDriverPlugin {
    device: Device,
    // ... 驱动状态
}

#[no_mangle]
pub extern "C" fn driver_get_info() -> *const DriverInfo {
    static INFO: DriverInfo = DriverInfo {
        name: c_str!("ModbusDriver"),
        version: c_str!("1.0.0"),
        description: c_str!("Modbus RTU/TCP Driver"),
    };
    &INFO
}

#[no_mangle]
pub extern "C" fn driver_create(device: *const CDevice) -> *mut c_void {
    let device = unsafe { convert_c_device(device) };
    let driver = Box::new(ModbusDriverPlugin::new(device));
    Box::into_raw(driver) as *mut c_void
}

#[no_mangle]
pub extern "C" fn driver_vtable() -> *const CDriverVTable {
    static VTABLE: CDriverVTable = CDriverVTable {
        read_data: modbus_read_data,
        execute_command: modbus_execute_command,
        destroy: modbus_destroy,
    };
    &VTABLE
}

extern "C" fn modbus_read_data(
    driver: *mut c_void,
    data: *mut *mut CResultValue,
    len: *mut usize
) -> i32 {
    let driver = unsafe { &mut *(driver as *mut ModbusDriverPlugin) };
    
    match driver.read_data_impl() {
        Ok(results) => {
            // 转换为C数据
            let c_results = convert_to_c_results(&results);
            unsafe {
                *data = c_results.as_ptr() as *mut CResultValue;
                *len = c_results.len();
            }
            std::mem::forget(c_results);
            0
        }
        Err(_) => -1,
    }
}

// 编译为动态库
// Cargo.toml
[lib]
crate-type = ["cdylib"]
```

## 安全性考虑

### 1. 内存安全

```rust
// 使用智能指针管理生命周期
pub struct SafeDriverHandle {
    driver: Arc<Mutex<Box<dyn DeviceDriver>>>,
    library: Arc<Library>,  // 保持库加载
}

// 确保驱动在库之前释放
impl Drop for SafeDriverHandle {
    fn drop(&mut self) {
        // driver先drop，然后library
        drop(self.driver.lock().unwrap());
    }
}
```

### 2. 版本兼容性

```rust
#[repr(C)]
pub struct DriverABI {
    pub version: u32,  // ABI版本号
    pub min_host_version: u32,  // 最小主程序版本
}

pub fn check_abi_compatibility(plugin_abi: &DriverABI) -> Result<(), Error> {
    if plugin_abi.version != CURRENT_ABI_VERSION {
        return Err(Error::Unsupported(
            format!("ABI version mismatch: expected {}, got {}", 
                CURRENT_ABI_VERSION, plugin_abi.version)
        ));
    }
    Ok(())
}
```

### 3. 沙箱隔离

```rust
// 使用seccomp限制系统调用（Linux）
#[cfg(target_os = "linux")]
fn apply_sandbox(driver: &mut DynamicDriver) -> Result<(), Error> {
    use seccomp::*;
    
    let mut ctx = Context::default(Action::Allow)?;
    ctx.add_rule(Rule::new(
        Syscall::execve,
        Compare::arg(0).with(0).using(Op::Eq),
        Action::Errno(libc::EPERM),
    ))?;
    ctx.load()?;
    
    Ok(())
}
```

## 性能影响分析

### 静态 vs 动态对比

| 指标 | 静态链接 | 动态加载 | 差异 |
|------|---------|---------|------|
| 调用开销 | ~1ns | ~5-10ns | FFI边界 |
| 内存占用 | 固定 | 按需 | 节省未使用驱动 |
| 启动时间 | 快 | 慢 | 加载+初始化 |
| 热更新 | 不支持 | 支持 | - |
| 类型安全 | 编译时 | 运行时 | - |

### 优化建议

1. **缓存驱动实例** - 避免重复加载
2. **延迟加载** - 首次使用时加载
3. **批量操作** - 减少FFI调用次数
4. **零拷贝** - 使用共享内存传递大数据

## 推荐方案

### 阶段1: 混合架构（短期）⭐

**实施步骤**:
1. 保留现有静态注册机制
2. 添加动态注册API
3. 实现基于libloading的加载器
4. 提供驱动开发SDK

**优点**: 风险低，渐进式，向后兼容

### 阶段2: 完全动态化（长期）

**实施步骤**:
1. 所有驱动改为插件
2. 核心驱动预加载
3. 第三方驱动按需加载
4. 实现驱动市场/仓库

**优点**: 灵活性最大，生态友好

## 实施建议

### 立即可做
1. ✅ 重构驱动注册表为可扩展架构
2. ✅ 定义FFI安全的驱动接口
3. ✅ 实现动态加载器原型

### 需要评估
1. ⚠️ 性能影响测试
2. ⚠️ 安全性审计
3. ⚠️ 跨平台兼容性测试
4. ⚠️ 第三方驱动开发体验

### 暂不推荐
1. ❌ WASM方案 - 生态不成熟
2. ❌ 进程隔离 - 性能开销大
3. ❌ 完全重写 - 风险高

## 结论

**推荐采用"混合静态+动态注册"方案**，理由：

1. **风险可控** - 不破坏现有架构
2. **渐进式** - 可逐步迁移
3. **灵活性** - 支持静态和动态驱动
4. **生态友好** - 第三方驱动开发简单
5. **性能可接受** - FFI开销在可控范围

**实施优先级**:
- P0: 设计FFI接口和ABI规范
- P1: 实现动态加载器和包装器
- P2: 提供驱动开发SDK和文档
- P3: 迁移现有驱动为插件（可选）

**预计工作量**: 2-3周（核心功能）+ 1-2周（测试和文档）
