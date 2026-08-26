# 驱动开发

TinyIoTHub 支持自定义设备驱动，可以扩展支持更多协议。驱动 SDK 位于 `crates/plugin-sdk`（包名 `plugin-sdk`）。

## 驱动架构

### 核心 trait

```rust
use plugin_sdk::{Device, DeviceCommand, DeviceDriver, Result, ResultValue};

/// 所有驱动必须实现此 trait
pub trait DeviceDriver: Send + Sync {
    /// 获取设备引用
    fn device(&self) -> &Device;

    /// 获取设备可变引用
    fn device_mut(&mut self) -> &mut Device;

    /// 读取设备数据，返回当前数据点列表
    fn read_data(&mut self) -> Result<Vec<ResultValue>>;

    /// 执行设备命令
    fn execute_command(&mut self, cmd: &DeviceCommand) -> Result<bool>;
}
```

## 创建自定义驱动

### 1. 创建驱动 crate

内置驱动位于 `plugins/` 目录（modbus、onvif、snmp、opcua 等），可参考 `plugins/modbus` 或 `examples/example-plugin/` 创建新驱动：

```rust
// src/lib.rs

use plugin_sdk::{Device, DeviceCommand, DeviceDriver, Result, ResultValue};

pub struct MyDriver {
    device: Device,
    config: MyDriverConfig,
}

#[derive(Debug, Deserialize)]
pub struct MyDriverConfig {
    pub host: String,
    pub port: u16,
    // 其他配置...
}

impl DeviceDriver for MyDriver {
    fn device(&self) -> &Device {
        &self.device
    }

    fn device_mut(&mut self) -> &mut Device {
        &mut self.device
    }

    fn read_data(&mut self) -> Result<Vec<ResultValue>> {
        // 读取数据
        Ok(vec![])
    }

    fn execute_command(&mut self, cmd: &DeviceCommand) -> Result<bool> {
        // 执行命令
        Ok(true)
    }
}
```

### 2. 注册驱动

驱动在编译期静态注册，在 `crates/runtime/src/driver/mod.rs` 的 `register_drivers!` 宏中加入新驱动：

```rust
tinyiothub_macros::register_drivers! {
    // 已有驱动...
    MyDriver,
}
```

## 驱动配置

### 配置参数

驱动支持以下配置参数：

```json
{
  "timeout_ms": 5000,
  "retry": {
    "max_attempts": 3,
    "interval_ms": 1000
  },
  "custom": {
    "host": "192.168.1.100",
    "port": 8080
  }
}
```

## 错误处理

### 错误类型

```rust
pub enum DriverError {
    NetworkError(String),
    IOError(String),
    ConfigError(String),
    ValidationError(String),
    Unsupported(String),
    Internal(String),
}
```

### 重试机制

连接失败的重试由运行时统一管理，驱动只需如实返回错误。

## 示例驱动

参考 `examples/bacnet-driver/` 中的 BACnet 驱动示例。

## 测试驱动

```bash
# 运行驱动 SDK 测试
cargo test --package plugin-sdk
```
