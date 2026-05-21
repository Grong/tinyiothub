# 驱动开发

TinyIoTHub 支持自定义设备驱动，可以扩展支持更多协议。

## 驱动架构

### 核心 trait

```rust
use async_trait::async_trait;

#[async_trait]
pub trait DeviceDriver {
    /// 连接设备
    async fn connect(&mut self) -> DriverResult<()>;

    /// 断开连接
    async fn disconnect(&mut self) -> DriverResult<()>;

    /// 读取数据
    async fn read_data(&mut self) -> DriverResult<Vec<u8>>;

    /// 写入数据
    async fn write_data(&mut self, data: &[u8]) -> DriverResult<()>;

    /// 获取设备状态
    async fn get_status(&mut self) -> DriverResult<DeviceStatus>;
}
```

## 创建自定义驱动

### 1. 创建驱动文件

在 `crates/tinyiothub-runtime/src/driver/drivers/` 目录下创建新驱动：

```rust
// my_driver.rs

use tinyiothub_core::driver::{DeviceDriver, DriverResult};
use async_trait::async_trait;

pub struct MyDriver {
    config: MyDriverConfig,
}

#[derive(Debug, Deserialize)]
pub struct MyDriverConfig {
    pub host: String,
    pub port: u16,
    // 其他配置...
}

#[async_trait]
impl DeviceDriver for MyDriver {
    async fn connect(&mut self) -> DriverResult<()> {
        // 连接逻辑
        Ok(())
    }

    async fn read_data(&mut self) -> DriverResult<Vec<u8>> {
        // 读取数据
        Ok(vec![])
    }
}
```

### 2. 注册驱动

在 `crates/tinyiothub-runtime/src/driver/mod.rs` 中注册：

```rust
pub mod my_driver;

pub fn get_driver(name: &str, config: Value) -> Option<Box<dyn DeviceDriver>> {
    match name {
        "my_driver" => Some(Box::new(my_driver::MyDriver::new(config)?)),
        _ => None,
    }
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
    ConnectionFailed(String),
    Timeout,
    InvalidData(String),
    NotSupported,
}
```

### 重试机制

驱动内置自动重试机制：

```toml
[drivers.retry]
max_attempts = 3
interval_ms = 1000
backoff_multiplier = 2.0
```

## 示例驱动

参考 `examples/bacnet-driver/` 中的 BACnet 驱动示例。

## 测试驱动

```bash
# 运行驱动测试
cargo test --package driver-sdk
```
