# TinyIoTHub Driver SDK

驱动开发SDK，用于开发TinyIoTHub的设备驱动（支持静态和动态加载）。

## 功能特性

- **统一接口**: 提供标准的`DeviceDriver` trait
- **类型安全**: 完整的Rust类型定义
- **FFI支持**: 自动生成C FFI接口用于动态加载
- **零依赖**: 插件只需依赖SDK，不依赖主程序

## 快速开始

### 1. 添加依赖

在你的`Cargo.toml`中添加：

```toml
[dependencies]
iot-edge-driver-sdk = { path = "../../sdks/driver-sdk" }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# 如果要编译为动态库
[lib]
crate-type = ["cdylib"]
```

### 2. 实现驱动

```rust
use iot_edge_driver_sdk::*;

pub struct MyDriver {
    device: Device,
}

impl MyDriver {
    pub fn new(device: Device) -> Self {
        Self { device }
    }

    // 提供驱动信息
    pub fn get_driver_info() -> ComponentInfo {
        ComponentInfo {
            name: "MyDriver".to_string(),
            version: "1.0.0".to_string(),
            class_name: "MyDriver".to_string(),
            device_num: 0,
            description: Some("My custom driver".to_string()),
            options_descriptors: vec![],
            location: None,
        }
    }
}

impl DeviceDriver for MyDriver {
    fn device(&self) -> &Device {
        &self.device
    }

    fn device_mut(&mut self) -> &mut Device {
        &mut self.device
    }

    fn read_data(&mut self) -> Result<Vec<ResultValue>> {
        // 读取设备数据
        Ok(vec![
            ResultValue::integer("temperature".to_string(), 25),
            ResultValue::float("humidity".to_string(), 60.5),
        ])
    }

    fn execute_command(&mut self, command: &DeviceCommand) -> Result<bool> {
        // 执行设备命令
        println!("Executing: {}", command.name);
        Ok(true)
    }
}

// 导出FFI接口（用于动态加载）
export_driver!(MyDriver);
```

### 3. 编译

```bash
# 编译为动态库
cargo build --release

# 输出文件位于 target/release/
# Windows: my_driver.dll
# Linux: libmy_driver.so
# macOS: libmy_driver.dylib
```

## API文档

### 核心类型

#### `Device`
设备信息结构：
```rust
pub struct Device {
    pub id: String,
    pub name: String,
    pub display_name: Option<String>,
    pub protocol_type: Option<String>,
    pub driver_options: Option<String>,
    pub address: Option<String>,
    pub enabled: bool,
}
```

#### `DeviceCommand`
设备命令：
```rust
pub struct DeviceCommand {
    pub id: String,
    pub name: String,
    pub command_type: String,
    pub parameters: Option<String>,
}
```

#### `ResultValue`
数据读取结果：
```rust
pub struct ResultValue {
    pub name: String,
    pub value_type: String,
    pub value: Option<String>,
}

// 便捷构造方法
ResultValue::integer(name, value)
ResultValue::float(name, value)
ResultValue::string(name, value)
ResultValue::boolean(name, value)
```

#### `ComponentInfo`
驱动元数据：
```rust
pub struct ComponentInfo {
    pub name: String,
    pub version: String,
    pub class_name: String,
    pub device_num: u32,
    pub description: Option<String>,
    pub options_descriptors: Vec<ComponentOption>,
    pub location: Option<String>,
}
```

### DeviceDriver Trait

所有驱动必须实现此trait：

```rust
pub trait DeviceDriver: Send + Sync {
    fn device(&self) -> &Device;
    fn device_mut(&mut self) -> &mut Device;
    fn read_data(&mut self) -> Result<Vec<ResultValue>>;
    fn execute_command(&mut self, command: &DeviceCommand) -> Result<bool>;
}
```

### 错误处理

SDK提供统一的错误类型：

```rust
pub type Result<T> = std::result::Result<T, DriverError>;

pub enum DriverError {
    ConnectionFailed(String),
    ReadFailed(String),
    WriteFailed(String),
    InvalidData(String),
    Timeout(String),
    NotSupported(String),
    Other(String),
}
```

## 示例

完整示例请参考 `examples/example-plugin/`

## FFI接口

`export_driver!` 宏会自动生成以下C FFI函数：

- `iot_edge_driver_info()` - 获取驱动信息（JSON）
- `iot_edge_driver_create(device_json, context_json)` - 创建驱动实例
- `iot_edge_driver_destroy(driver_ptr)` - 销毁驱动实例

这些函数由主程序的动态加载器调用。

## 开发建议

1. **错误处理**: 使用`Result`类型，提供清晰的错误信息
2. **线程安全**: 驱动必须实现`Send + Sync`
3. **资源管理**: 在`Drop` trait中清理资源
4. **日志记录**: 使用`tracing`或`log` crate记录日志
5. **测试**: 编写单元测试和集成测试

## 许可证

MIT
