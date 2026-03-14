# 驱动动态加载最终设计（简化版）

## 设计原则

**最简化架构**：只需要一个 `driver-sdk` 包，包含所有驱动开发需要的内容。

## 项目结构

```
tinyiothub/
├── Cargo.toml                    # 主程序
├── sdks/                         # SDK目录 ⭐
│   ├── driver-sdk/              # 驱动开发SDK
│   │   ├── Cargo.toml
│   │   ├── README.md
│   │   └── src/
│   │       ├── lib.rs           # SDK入口
│   │       ├── driver.rs        # DeviceDriver trait
│   │       ├── types.rs         # 基础类型
│   │       ├── error.rs         # 错误类型
│   │       ├── config.rs        # 配置管理
│   │       ├── ffi.rs           # FFI辅助
│   │       └── macros.rs        # 导出宏
│   └── ...                      # 未来的其他SDK
├── derive/                       # 宏库（已存在）
│   ├── Cargo.toml
│   └── src/lib.rs
└── src/                          # 主程序
    ├── domain/
    │   └── device/
    │       └── driver/
    │           ├── mod.rs
    │           ├── driver.rs    # 引用SDK的trait
    │           └── dynamic/     # 动态加载模块
    └── ...
```

## 核心设计

### 1. driver-sdk 统一SDK

```toml
# sdks/driver-sdk/Cargo.toml
[package]
name = "iot-edge-driver-sdk"
version = "1.0.0"
edition = "2021"

[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
chrono = "0.4"
edge-derive = { path = "../derive" }

# 最小依赖，不依赖主程序
```

```rust
// sdks/driver-sdk/src/lib.rs

//! TinyIoTHub 驱动开发SDK
//! 
//! 提供驱动开发所需的所有接口和工具

pub mod driver;
pub mod types;
pub mod error;
pub mod config;
pub mod ffi;
pub mod macros;

// 重新导出核心类型
pub use driver::DeviceDriver;
pub use types::*;
pub use error::*;
pub use config::*;

// 重新导出宏
pub use tinyiothub_derive::DeviceDriver as DeviceDriverDerive;

// 导出便捷宏
pub use macros::export_driver;
```

```rust
// sdks/driver-sdk/src/types.rs

use serde::{Deserialize, Serialize};

/// 设备信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub display_name: Option<String>,
    pub protocol_type: Option<String>,
    pub driver_options: Option<String>,
    pub address: Option<String>,
    pub enabled: bool,
}

/// 设备命令
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCommand {
    pub id: String,
    pub name: String,
    pub command_type: String,
    pub parameters: Option<String>,
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
    
    pub fn float_with_precision(name: String, value: f64, decimal_places: u32) -> Self {
        let multiplier = 10_f64.powi(decimal_places as i32);
        let rounded = (value * multiplier).round() / multiplier;
        Self::new(
            name,
            "float".to_string(),
            Some(format!("{:.precision$}", rounded, precision = decimal_places as usize)),
        )
    }
    
    pub fn string(name: String, value: String) -> Self {
        Self::new(name, "string".to_string(), Some(value))
    }
    
    pub fn boolean(name: String, value: bool) -> Self {
        Self::new(name, "boolean".to_string(), Some(value.to_string()))
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
        Self { label, name, default_value, option_type, required }
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
// sdks/driver-sdk/src/error.rs

use std::fmt;

#[derive(Debug, Clone)]
pub enum DriverError {
    NetworkError(String),
    IOError(String),
    ConfigError(String),
    ValidationError(String),
    Unsupported(String),
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
// sdks/driver-sdk/src/config.rs

use crate::Device;
use std::collections::HashMap;

/// 驱动配置管理器
#[derive(Debug, Clone)]
pub struct DriverConfig {
    config: HashMap<String, String>,
}

impl DriverConfig {
    pub fn from_device(device: &Device) -> Self {
        let mut config = HashMap::new();
        
        if let Some(ref driver_options) = device.driver_options {
            if let Ok(parsed) = serde_json::from_str::<HashMap<String, serde_json::Value>>(driver_options) {
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
```

```rust
// sdks/driver-sdk/src/driver.rs

use crate::{Device, DeviceCommand, ResultValue, Result};
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
// sdks/driver-sdk/src/ffi.rs

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

/// 将Rust字符串转换为C字符串指针
pub fn to_c_string(s: &str) -> *const c_char {
    CString::new(s).unwrap().into_raw()
}

/// 从C字符串指针读取Rust字符串
pub unsafe fn from_c_string(ptr: *const c_char) -> String {
    CStr::from_ptr(ptr).to_string_lossy().into_owned()
}

/// 释放C字符串
pub unsafe fn free_c_string(ptr: *const c_char) {
    if !ptr.is_null() {
        let _ = CString::from_raw(ptr as *mut c_char);
    }
}
```

```rust
// sdks/driver-sdk/src/macros.rs

/// 导出驱动的便捷宏
#[macro_export]
macro_rules! export_driver {
    ($driver_type:ty) => {
        use std::ffi::{CStr, CString};
        use std::os::raw::{c_char, c_void};
        use $crate::*;

        /// 获取驱动信息
        #[no_mangle]
        pub extern "C" fn iot_edge_driver_info() -> *const c_char {
            let info = <$driver_type>::get_driver_info();
            let json = serde_json::to_string(&info).unwrap();
            $crate::ffi::to_c_string(&json)
        }

        /// 创建驱动实例
        #[no_mangle]
        pub extern "C" fn iot_edge_driver_create(
            device_json: *const c_char,
            _context_json: *const c_char,
        ) -> *mut c_void {
            unsafe {
                let device_str = $crate::ffi::from_c_string(device_json);
                let device: Device = serde_json::from_str(&device_str).unwrap();
                
                let driver = Box::new(<$driver_type>::new(device));
                Box::into_raw(driver) as *mut c_void
            }
        }

        /// 销毁驱动实例
        #[no_mangle]
        pub extern "C" fn iot_edge_driver_destroy(driver: *mut c_void) {
            unsafe {
                let _ = Box::from_raw(driver as *mut $driver_type);
            }
        }
    };
}
```

### 2. 更新derive宏

```rust
// derive/src/lib.rs (关键部分)

let expanded = quote! {
    impl #name {
        pub fn get_driver_info() -> iot_edge_driver_sdk::ComponentInfo {
            let opts = vec![
                #(#options_code),*
            ];

            iot_edge_driver_sdk::ComponentInfo {
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

### 3. 主程序集成

```toml
# Cargo.toml (主程序)
[package]
name = "tinyiothub"
version = "1.0.0"

[dependencies]
# 依赖SDK（与插件使用相同的SDK）
iot-edge-driver-sdk = { path = "sdks/driver-sdk" }
edge-derive = { path = "derive" }

# 其他依赖...
tokio = { version = "1", features = ["full"] }
axum = "0.7"
libloading = "0.8"
# ...

[workspace]
members = [
    ".",
    "sdks/driver-sdk", # 驱动SDK
    "derive",          # 宏库
]
```

```rust
// src/domain/device/driver/mod.rs

// 重新导出SDK的类型（保持现有代码兼容）
pub use iot_edge_driver_sdk::{
    DeviceDriver, Device, DeviceCommand, ResultValue,
    DriverError, DriverConfig, ComponentInfo, ComponentOption,
};

// 现有的驱动实现
pub mod drivers;
pub mod dynamic;  // 动态加载模块

// 现有的包装器和工具
pub use driver::{DriverWrapper, RetryConfig};
pub mod driver;
pub mod retry;
pub mod status;
```

### 4. 插件开发示例

```
custom-driver/
├── Cargo.toml
└── src/
    └── lib.rs
```

```toml
# custom-driver/Cargo.toml
[package]
name = "custom-driver"
version = "1.0.0"

[lib]
crate-type = ["cdylib"]

[dependencies]
# 只依赖SDK ⭐
iot-edge-driver-sdk = { path = "../sdks/driver-sdk" }
```

```rust
// custom-driver/src/lib.rs

use iot_edge_driver_sdk::*;

#[derive(DeviceDriverDerive)]
#[driver(name = "CustomDriver", version = "1.0.0", description = "Custom Device Driver")]
#[driver_option(label = "API Endpoint", name = "endpoint", default = "http://localhost", option_type = "string", required = true)]
#[driver_option(label = "Timeout (ms)", name = "timeout", default = "5000", option_type = "number", required = false)]
pub struct CustomDriver {
    device: Device,
    endpoint: String,
    timeout: i64,
}

impl CustomDriver {
    pub fn new(device: Device) -> Self {
        let config = DriverConfig::from_device(&device);
        let endpoint = config.get_string("endpoint", "http://localhost");
        let timeout = config.get_integer("timeout", 5000);
        
        Self { device, endpoint, timeout }
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
        // 自定义实现
        Ok(vec![
            ResultValue::float("temperature".to_string(), 25.5),
            ResultValue::integer("humidity".to_string(), 60),
        ])
    }
    
    fn execute_command(&mut self, cmd: &DeviceCommand) -> Result<bool> {
        println!("Executing command: {:?}", cmd);
        Ok(true)
    }
    
    fn default_config(&self) -> std::collections::HashMap<String, String> {
        Self::get_default_config()
    }
}

// 导出驱动（一行代码）
export_driver!(CustomDriver);
```

## 依赖关系图（简化版）

```
┌─────────────────────────────────────────────┐
│                                              │
│  ┌──────────────────┐                       │
│  │  driver-sdk      │◄──────┐               │
│  │  (统一SDK)       │       │               │
│  └──────────────────┘       │               │
│           ▲                 │               │
│           │                 │               │
│           │          ┌──────┴────────┐      │
│           │          │  derive       │      │
│           │          │  (宏库)       │      │
│           │          └───────────────┘      │
│           │                                  │
│  ┌────────┴────────┐    ┌──────────────┐   │
│  │  主程序         │    │ 插件         │   │
│  │  iot-edge       │    │ custom-driver│   │
│  └─────────────────┘    └──────────────┘   │
│                                              │
└─────────────────────────────────────────────┘

依赖关系：
- 主程序 → driver-sdk + derive
- 插件 → driver-sdk
- derive → driver-sdk (生成代码时引用)
- driver-sdk → 无外部依赖（最小化）
```

## 优势总结

### 1. 架构最简
- ✅ 只有一个SDK包
- ✅ 依赖关系清晰
- ✅ 易于理解和维护

### 2. 开发体验极佳
- ✅ 插件只需依赖一个包
- ✅ 使用宏一行导出
- ✅ 完整的类型安全

### 3. 版本管理简单
- ✅ SDK版本独立
- ✅ 主程序和插件使用相同SDK
- ✅ 兼容性好

### 4. 编译快速
- ✅ SDK依赖最小
- ✅ 插件编译快
- ✅ 增量编译友好

## 实施步骤

### Phase 1: 创建SDK (1天)
- [ ] 创建 `sdks/driver-sdk` 包
- [ ] 实现核心类型和trait
- [ ] 实现 `export_driver!` 宏
- [ ] 编写文档

### Phase 2: 更新derive (半天)
- [ ] 修改宏引用SDK类型
- [ ] 测试宏功能

### Phase 3: 主程序集成 (半天)
- [ ] 主程序依赖SDK
- [ ] 重新导出类型保持兼容
- [ ] 实现动态加载模块

### Phase 4: 测试验证 (1天)
- [ ] 创建示例插件
- [ ] 测试编译和加载
- [ ] 验证功能完整性

## 文件清单

需要创建/修改的文件：

```
新增：
- sdks/driver-sdk/Cargo.toml
- sdks/driver-sdk/src/lib.rs
- sdks/driver-sdk/src/types.rs
- sdks/driver-sdk/src/error.rs
- sdks/driver-sdk/src/config.rs
- sdks/driver-sdk/src/driver.rs
- sdks/driver-sdk/src/ffi.rs
- sdks/driver-sdk/src/macros.rs
- sdks/driver-sdk/README.md
- sdks/driver-sdk/examples/  # 示例代码

修改：
- derive/Cargo.toml (添加SDK依赖)
- derive/src/lib.rs (引用SDK类型)
- Cargo.toml (添加SDK依赖和workspace)
- src/domain/device/driver/mod.rs (重新导出SDK类型)
```

这个简化设计是否满足您的要求？确认后我可以开始实施。


## 未来SDK规划

### sdks/ 目录结构

```
sdks/
├── driver-sdk/          # 驱动开发SDK（当前）
│   ├── Cargo.toml
│   ├── README.md
│   ├── examples/
│   │   ├── simple_driver.rs
│   │   └── advanced_driver.rs
│   └── src/
│       └── ...
│
├── event-sdk/           # 事件系统SDK（未来）
│   ├── Cargo.toml
│   ├── README.md
│   └── src/
│       ├── lib.rs
│       ├── event.rs
│       └── handler.rs
│
├── alarm-sdk/           # 告警系统SDK（未来）
│   ├── Cargo.toml
│   ├── README.md
│   └── src/
│       ├── lib.rs
│       ├── rule.rs
│       └── trigger.rs
│
├── notification-sdk/    # 通知系统SDK（未来）
│   ├── Cargo.toml
│   ├── README.md
│   └── src/
│       ├── lib.rs
│       └── channel.rs
│
└── README.md           # SDK总览文档
```

### 各SDK职责

#### 1. driver-sdk（当前实施）
- 设备驱动开发接口
- 数据读取和命令执行
- 配置管理
- FFI支持

#### 2. event-sdk（未来）
- 事件定义和发布
- 事件处理器接口
- 事件过滤和路由

#### 3. alarm-sdk（未来）
- 告警规则定义
- 告警触发器接口
- 告警级别管理

#### 4. notification-sdk（未来）
- 通知渠道接口
- 消息模板
- 发送策略

### SDK设计原则

1. **独立性** - 每个SDK独立发布和版本管理
2. **最小依赖** - 只依赖必要的外部库
3. **向后兼容** - API变更遵循语义化版本
4. **文档完整** - 每个SDK包含完整文档和示例
5. **易于测试** - 提供测试工具和mock

### Workspace配置

```toml
# Cargo.toml (根目录)
[workspace]
members = [
    ".",
    "sdks/driver-sdk",
    # "sdks/event-sdk",      # 未来添加
    # "sdks/alarm-sdk",      # 未来添加
    # "sdks/notification-sdk", # 未来添加
    "derive",
]

# 共享依赖版本
[workspace.dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1", features = ["full"] }
```

### SDK发布策略

#### 版本管理
- 每个SDK独立版本号
- 遵循语义化版本（SemVer）
- 主程序可以依赖不同版本的SDK

#### 发布流程
1. SDK代码变更
2. 更新CHANGELOG
3. 更新版本号
4. 发布到crates.io（可选）
5. 更新主程序依赖

#### 兼容性保证
- 主版本号变更：破坏性变更
- 次版本号变更：新增功能
- 修订版本号：bug修复

这个目录结构是否符合您的规划？确认后我可以开始实施driver-sdk。
