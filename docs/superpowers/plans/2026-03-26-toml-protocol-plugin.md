# 统一插件系统重构 + 多类型 TOML 插件实现计划

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 重构现有驱动系统为**统一的多类型插件架构**，允许通过 TOML 配置实例化各种插件（协议驱动、通知渠道、定时任务、存储后端、集成），同时保留对外部动态驱动（.dll/.so）的支持。

**Architecture:**

```
设备 → 插件消费者（DeviceManager, NotificationManager, Scheduler...）
              ↑
        PluginRegistry  ← 唯一的统一插件注册表
              │
    ┌─────────┼─────────┬──────────┬──────────┬───────────┐
    │         │         │          │          │           │
 Protocol  Notification Scheduler Storage  Integration  Dynamic
 Handler   Handler     Handler    Handler   Handler    (.dll)
    │         │         │          │          │           │
    └─────────┴─────────┴──────────┴──────────┴───────────┘
              │              TOML 配置文件实例化
```

**Tech Stack:** Rust (tokio, serde, toml, reqwest, rumqttc), tinyiothub derive

---

## 插件类型设计

| 类型 | Handler Trait | 说明 | TOML section |
|------|--------------|------|--------------|
| `protocol` | `ProtocolHandler` | 设备数据采集 | `[protocol]` |
| `notification` | `NotificationHandler` | 告警/消息推送 | `[notification]` |
| `scheduler` | `SchedulerHandler` | 定时任务执行 | `[scheduler]` |
| `storage` | `StorageHandler` | 数据持久化 | `[storage]` |
| `integration` | `IntegrationHandler` | 外部系统对接 | `[integration]` |

---

## File Structure

```
api/src/domain/device/driver/
├── mod.rs                              # [重构] 初始化 + 统一入口
├── registry.rs                         # [新建] PluginRegistry 统一注册表
├── protocol/                          # [新建] 协议驱动插件
│   ├── mod.rs
│   ├── config.rs
│   ├── handlers/
│   │   ├── mod.rs
│   │   ├── http_poll.rs
│   │   ├── mqtt_subscribe.rs
│   │   ├── modbus_tcp.rs
│   │   └── snmp_get.rs
│   └── driver.rs

api/src/domain/notification/            # [新建] 通知插件
│   ├── mod.rs
│   ├── config.rs
│   ├── handlers/
│   │   ├── mod.rs
│   │   ├── feishu.rs
│   │   ├── dingtalk.rs
│   │   └── email.rs
│   └── handler.rs

api/src/domain/scheduler/               # [新建] 定时任务插件
│   ├── mod.rs
│   ├── config.rs
│   ├── handlers/
│   │   ├── mod.rs
│   │   └── cron.rs
│   └── handler.rs

api/src/domain/storage/                 # [新建] 存储后端插件
│   ├── mod.rs
│   ├── config.rs
│   ├── handlers/
│   │   ├── mod.rs
│   │   ├── postgres.rs
│   │   └── influxdb.rs
│   └── handler.rs

api/plugins/                           # [新建] TOML 插件配置目录
├── protocol/
│   ├── http_poll_example.toml
│   ├── mqtt_example.toml
│   └── modbus_tcp_example.toml
├── notification/
│   ├── feishu_example.toml
│   └── dingtalk_example.toml
└── scheduler/
    └── cron_sync_example.toml

api/derive/src/lib.rs                   # [重构] register_drivers! 改为注册到全局表
```

---

## Chunk 0: 准备工作 - 添加依赖

### Task 1: 添加 Cargo 依赖

**Files:**
- Modify: `api/Cargo.toml`

- [ ] **Step 1: 添加新依赖**

```toml
# 在 [dependencies] 中添加:
reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }
toml = "0.8"
rumqttc = "0.24"
tokio-cron-scheduler = "0.13"
async-trait = "0.1"
serde_json = "1.0"

# 可选（后续实现）:
# tokio-modbus = "0.10"
# snmp = "0.5"
# tokio-postgres = "0.7"
# influxdb2 = "0.4"
```

- [ ] **Step 2: 验证编译**

```bash
cd api && cargo check 2>&1 | tail -20
```
Expected: 无新增依赖错误

---

## Chunk 1: 统一插件注册表核心

### Task 2: 创建 `registry.rs` - 统一插件注册表

**Files:**
- Create: `api/src/domain/plugin/registry.rs`

- [ ] **Step 1: 实现统一插件注册表**

```rust
//! 统一插件注册表
//!
//! 所有插件（协议驱动、通知渠道、定时任务、存储后端、集成）都通过此注册表管理。

use std::{
    any::Any,
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use dashmap::DashMap;
use tracing::{debug, error, info, warn};

use crate::shared::error::Error;

/// 插件类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PluginType {
    Protocol,
    Notification,
    Scheduler,
    Storage,
    Integration,
}

impl PluginType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "protocol" => Some(Self::Protocol),
            "notification" => Some(Self::Notification),
            "scheduler" => Some(Self::Scheduler),
            "storage" => Some(Self::Storage),
            "integration" => Some(Self::Integration),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Protocol => "protocol",
            Self::Notification => "notification",
            Self::Scheduler => "scheduler",
            Self::Storage => "storage",
            Self::Integration => "integration",
        }
    }
}

/// 插件清单（TOML 文件的 [plugin] 部分）
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: Option<String>,
    #[serde(rename = "type")]
    pub plugin_type: PluginType,
    pub description: Option<String>,
}

/// 插件条目
pub enum PluginEntry {
    /// 静态插件（编译时注册）
    Static {
        manifest: PluginManifest,
        factory: PluginFactory,
    },
    /// 动态插件（.dll/.so 路径）
    Dynamic {
        manifest: PluginManifest,
        path: PathBuf,
    },
    /// 配置插件（TOML 配置）
    Config {
        manifest: PluginManifest,
        config: toml::Value,
    },
}

/// 插件工厂函数
pub type PluginFactory =
    Box<dyn Fn(Arc<crate::application::AppContext>) -> Result<Box<dyn PluginHandler>, Error> + Send + Sync>;

/// 插件处理器接口（所有插件的共同接口）
pub trait PluginHandler: Any + Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn manifest(&self) -> &PluginManifest;
    fn plugin_type(&self) -> PluginType;
}

/// 应用上下文（所有插件共享）
pub struct AppContext {
    pub data_context: Arc<crate::application::data_context::DataContext>,
    // 后续可添加更多共享资源：HTTP client pool, DB pool 等
}

/// 统一插件注册表
pub struct PluginRegistry {
    entries: DashMap<String, PluginEntry>,
    handlers: DashMap<String, Box<dyn PluginHandler>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            entries: DashMap::new(),
            handlers: DashMap::new(),
        }
    }

    /// 注册静态插件
    pub fn register_static(
        &self,
        manifest: PluginManifest,
        factory: PluginFactory,
    ) {
        debug!("Registering static plugin: {} ({})", manifest.name, manifest.plugin_type.as_str());
        self.entries.insert(
            manifest.name.clone(),
            PluginEntry::Static { manifest, factory },
        );
    }

    /// 注册动态插件路径
    pub fn register_dynamic(&self, manifest: PluginManifest, path: PathBuf) {
        debug!("Registering dynamic plugin: {} at {:?}", manifest.name, path);
        self.entries.insert(manifest.name.clone(), PluginEntry::Dynamic { manifest, path });
    }

    /// 注册配置插件
    pub fn register_config(&self, manifest: PluginManifest, config: toml::Value) {
        info!(
            "Registering config plugin: {} ({})",
            manifest.name,
            manifest.plugin_type.as_str()
        );
        self.entries.insert(
            manifest.name.clone(),
            PluginEntry::Config { manifest, config },
        );
    }

    /// 创建插件实例
    pub fn create_plugin(
        &self,
        name: &str,
        context: Arc<AppContext>,
    ) -> Result<Box<dyn PluginHandler>, Error> {
        // 如果已经实例化过，直接返回
        if let Some(handler) = self.handlers.get(name) {
            return Ok(handler.value().as_ref().as_ref());
        }

        let entry = self.entries.get(name)
            .ok_or_else(|| Error::Unsupported(format!("Plugin not found: {}", name)))?;

        let handler: Box<dyn PluginHandler> = match entry.value() {
            PluginEntry::Static { manifest, factory } => {
                debug!("Creating static plugin: {}", name);
                factory(context)?
            }
            PluginEntry::Dynamic { manifest, path } => {
                debug!("Creating dynamic plugin: {} from {:?}", name, path);
                self.create_dynamic_plugin(name, path)?
            }
            PluginEntry::Config { manifest, config } => {
                debug!("Creating config plugin: {}", name);
                self.create_config_plugin(manifest, config, context)?
            }
        };

        // 缓存实例
        self.handlers.insert(name.to_string(), handler.as_ref().as_ref().to_owned());

        Ok(handler)
    }

    fn create_dynamic_plugin(&self, name: &str, path: &Path) -> Result<Box<dyn PluginHandler>, Error> {
        // TODO: 实现动态驱动加载（复用现有的 DynamicDriverLoader）
        Err(Error::Unsupported(format!("Dynamic plugin loading not implemented yet")))
    }

    fn create_config_plugin(
        &self,
        manifest: &PluginManifest,
        config: &toml::Value,
        context: Arc<AppContext>,
    ) -> Result<Box<dyn PluginHandler>, Error> {
        match manifest.plugin_type {
            PluginType::Protocol => {
                use crate::domain::plugin::protocol;
                let handler = protocol::create_handler(config)?;
                Ok(Box::new(handler))
            }
            PluginType::Notification => {
                use crate::domain::plugin::notification;
                let handler = notification::create_handler(config, context)?;
                Ok(Box::new(handler))
            }
            PluginType::Scheduler => {
                use crate::domain::plugin::scheduler;
                let handler = scheduler::create_handler(config)?;
                Ok(Box::new(handler))
            }
            _ => Err(Error::Unsupported(format!(
                "Plugin type {:?} not implemented yet",
                manifest.plugin_type
            ))),
        }
    }

    /// 检查插件是否存在
    pub fn has_plugin(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    /// 获取所有插件名称
    pub fn plugin_names(&self) -> Vec<String> {
        self.entries.iter().map(|e| e.key().clone()).collect()
    }

    /// 获取指定类型的插件名称
    pub fn plugins_by_type(&self, plugin_type: PluginType) -> Vec<String> {
        self.entries
            .iter()
            .filter(|e| match e.value() {
                PluginEntry::Static { manifest, .. } => manifest.plugin_type == plugin_type,
                PluginEntry::Dynamic { manifest, .. } => manifest.plugin_type == plugin_type,
                PluginEntry::Config { manifest, .. } => manifest.plugin_type == plugin_type,
            })
            .map(|e| e.key().clone())
            .collect()
    }

    /// 从目录自动加载（.dll/.so + .toml）
    pub fn load_from_dir<P: AsRef<Path>>(&self, dir: P) -> Result<Vec<String>, Error> {
        let dir = dir.as_ref();
        if !dir.exists() {
            warn!("Plugin directory does not exist: {:?}", dir);
            return Ok(vec![]);
        }

        let mut loaded = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if let Some(ext) = path.extension() {
                let ext_str = ext.to_string_lossy();
                if ext_str == "dll" || ext_str == "so" || ext_str == "dylib" {
                    if let Ok(name) = self.load_dynamic_plugin(&path) {
                        loaded.push(name);
                    }
                } else if ext_str == "toml" {
                    if let Ok(name) = self.load_toml_plugin(&path) {
                        loaded.push(name);
                    }
                }
            }
        }

        info!("Auto-loaded {} plugins from {:?}", loaded.len(), dir);
        Ok(loaded)
    }

    fn load_dynamic_plugin(&self, path: &Path) -> Result<String, Error> {
        // TODO: 复用现有的 DynamicDriverLoader
        Err(Error::Unsupported("Dynamic plugin loading not implemented yet".to_string()))
    }

    fn load_toml_plugin(&self, path: &Path) -> Result<String, Error> {
        let content = std::fs::read_to_string(path)?;
        let value: toml::Value = toml::from_str(&content)
            .map_err(|e| Error::ValidationError(format!("Invalid TOML: {}", e)))?;

        let manifest: PluginManifest = value.get("plugin")
            .ok_or_else(|| Error::ValidationError("Missing [plugin] section".to_string()))?
            .clone()
            .try_into()
            .map_err(|e| Error::ValidationError(format!("Invalid plugin manifest: {}", e)))?;

        let name = manifest.name.clone();
        self.register_config(manifest, value);
        Ok(name)
    }
}

impl Default for PluginRegistry {
    fn default() -> Self { Self::new() }
}

/// 全局注册表实例
static GLOBAL_REGISTRY: once_cell::sync::Lazy<PluginRegistry> =
    once_cell::sync::Lazy::new(PluginRegistry::new);

/// 获取全局注册表
pub fn get_global_registry() -> &'static PluginRegistry {
    &GLOBAL_REGISTRY
}
```

---

### Task 3: 创建 `plugin/mod.rs` - 插件模块入口

**Files:**
- Create: `api/src/domain/plugin/mod.rs`

- [ ] **Step 1: 模块入口**

```rust
//! 统一插件系统
//!
//! 提供多类型插件的支持：协议驱动、通知渠道、定时任务、存储后端、集成。

pub mod registry;
pub mod protocol;
pub mod notification;
pub mod scheduler;

pub use registry::{
    get_global_registry,
    PluginRegistry,
    PluginEntry,
    PluginManifest,
    PluginType,
    PluginHandler,
    PluginFactory,
    AppContext,
};

use crate::shared::error::Error;

/// 初始化所有内置插件
pub fn init_plugins() {
    let registry = get_global_registry();

    // 注册内置静态插件
    // registry.register_static(manifest, factory);

    // 从插件目录自动加载
    let plugins_dir = std::path::PathBuf::from("api/plugins");
    if let Err(e) = registry.load_from_dir(&plugins_dir) {
        tracing::warn!("Failed to auto-load plugins from {:?}: {}", plugins_dir, e);
    }

    tracing::info!("Plugins initialized. Available: {:?}", registry.plugin_names());
}
```

---

## Chunk 2: 协议驱动插件

### Task 4: 创建 `protocol/mod.rs` - 协议驱动模块

**Files:**
- Create: `api/src/domain/plugin/protocol/mod.rs`

- [ ] **Step 1: 模块入口**

```rust
//! 协议驱动插件
//!
//! 支持 HTTP Poller、MQTT 订阅、Modbus TCP、SNMP 等协议。

pub mod handlers;
pub mod config;

pub use config::{ProtocolConfig, HttpPollConfig, MqttConfig, ModbusConfig, SnmpConfig};
pub use handlers::{ProtocolHandler, HttpPollHandler, MqttSubscribeHandler};

use crate::domain::plugin::{PluginHandler, PluginManifest, AppContext};
use crate::shared::error::Error;
use std::sync::Arc;

/// 创建协议处理器
pub fn create_handler(config: &toml::Value) -> Result<Box<dyn PluginHandler>, Error> {
    let protocol_cfg = config.get("protocol")
        .ok_or_else(|| Error::ValidationError("Missing [protocol] section".to_string()))?;

    let manifest = crate::domain::plugin::registry::get_global_registry()
        .entries
        .get(&"")
        .map(|_| PluginManifest {
            name: "protocol_handler".to_string(),
            version: None,
            plugin_type: crate::domain::plugin::PluginType::Protocol,
            description: None,
        })
        .unwrap();

    // 根据配置类型创建对应的处理器
    match protocol_cfg.get("type").and_then(|v| v.as_str()) {
        Some("http_poll") => {
            let cfg: HttpPollConfig = protocol_cfg.try_into()?;
            Ok(Box::new(HttpPollHandler::new(cfg, get_mapping(config)?)))
        }
        Some("mqtt_subscribe") => {
            let cfg: MqttConfig = protocol_cfg.try_into()?;
            Ok(Box::new(MqttSubscribeHandler::new(cfg, get_mapping(config)?)))
        }
        Some("modbus_tcp") => {
            let cfg: ModbusConfig = protocol_cfg.try_into()?;
            Ok(Box::new(handlers::modbus_tcp::ModbusTcpHandler::new(cfg)))
        }
        _ => Err(Error::Unsupported(format!(
            "Unknown protocol type: {:?}",
            protocol_cfg.get("type")
        ))),
    }
}

fn get_mapping(config: &toml::Value) -> Result<std::collections::HashMap<String, String>, Error> {
    config
        .get("mapping")
        .and_then(|v| v.as_table())
        .map(|t| {
            t.iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                .collect()
        })
        .ok_or_else(|| Error::ValidationError("Missing [mapping] section".to_string()))
}
```

---

### Task 5: 创建 `protocol/config.rs` - 协议配置结构体

**Files:**
- Create: `api/src/domain/plugin/protocol/config.rs`

- [ ] **Step 1: 配置结构体**

```rust
//! 协议驱动配置结构体

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct HttpPollConfig {
    pub base_url: String,
    pub endpoint: String,
    #[serde(default = "default_get")]
    pub method: String,
    pub poll_interval_ms: u64,
    #[serde(default)]
    pub headers: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub auth: Option<HttpAuth>,
}

fn default_get() -> String { "GET".to_string() }

#[derive(Debug, Clone, Deserialize)]
pub struct HttpAuth {
    #[serde(rename = "type")]
    pub auth_type: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub token: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MqttConfig {
    pub broker_url: String,
    pub client_id: Option<String>,
    pub topic: String,
    pub qos: Option<u8>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModbusConfig {
    pub host: String,
    pub port: u16,
    pub slave_id: u8,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
}

fn default_timeout() -> u64 { 5000 }

#[derive(Debug, Clone, Deserialize)]
pub struct SnmpConfig {
    pub host: String,
    pub port: u16,
    pub community: String,
    pub oid: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolType {
    HttpPoll,
    MqttSubscribe,
    ModbusTcp,
    SnmpGet,
}
```

---

### Task 6: 创建 `protocol/handlers/mod.rs` - Handler trait

**Files:**
- Create: `api/src/domain/plugin/protocol/handlers/mod.rs`

- [ ] **Step 1: 定义 ProtocolHandler trait**

```rust
//! 协议处理器 trait

use async_trait::async_trait;
use crate::{
    domain::device::driver::ResultValue,
    dto::entity::Device,
    shared::error::Error,
};

use super::config::{HttpPollConfig, MqttConfig};

/// 协议数据采集处理器
#[async_trait]
pub trait ProtocolHandler: Send + Sync {
    /// 采集设备数据
    async fn read_data(&self, device: &Device) -> Result<Vec<ResultValue>, Error>;

    /// 执行设备命令（可选实现）
    async fn execute_command(
        &self,
        device: &Device,
        command: &str,
        args: &[String],
    ) -> Result<bool, Error> {
        let _ = (device, command, args);
        Err(Error::Unsupported("Command not supported".to_string()))
    }

    /// 处理器名称
    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }
}

pub mod http_poll;
pub mod mqtt_subscribe;

pub use http_poll::HttpPollHandler;
pub use mqtt_subscribe::MqttSubscribeHandler;
```

---

### Task 7: 创建 `protocol/handlers/http_poll.rs`

**Files:**
- Create: `api/src/domain/plugin/protocol/handlers/http_poll.rs`

- [ ] **Step 1: 实现 HTTP 轮询处理器**

```rust
//! HTTP 轮询协议处理器

use async_trait::async_trait;
use reqwest::Client;
use std::collections::HashMap;
use std::time::Duration;
use tracing::debug;

use super::ProtocolHandler;
use crate::{
    domain::device::driver::ResultValue,
    dto::entity::Device,
    shared::error::Error,
};

use super::super::config::HttpPollConfig;

pub struct HttpPollHandler {
    config: HttpPollConfig,
    mapping: HashMap<String, String>,
    client: Client,
}

impl HttpPollHandler {
    pub fn new(config: HttpPollConfig, mapping: HashMap<String, String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("HTTP client build failed");

        Self { config, mapping, client }
    }

    fn build_url(&self) -> String {
        format!(
            "{}{}",
            self.config.base_url.trim_end_matches('/'),
            self.config.endpoint
        )
    }
}

#[async_trait]
impl ProtocolHandler for HttpPollHandler {
    async fn read_data(&self, _device: &Device) -> Result<Vec<ResultValue>, Error> {
        let url = self.build_url();
        debug!("HTTP poll: {} {}", self.config.method, url);

        let mut request = self.client.request(
            reqwest::Method::from_bytes(self.config.method.as_bytes())
                .unwrap_or(reqwest::Method::GET),
            &url,
        );

        // 认证
        if let Some(ref auth) = self.config.auth {
            match auth.auth_type.as_str() {
                "basic" => {
                    if let (Some(u), Some(p)) = (&auth.username, &auth.password) {
                        request = request.basic_auth(u, Some(p));
                    }
                }
                "bearer" => {
                    if let Some(ref token) = auth.token {
                        request = request.bearer_auth(token);
                    }
                }
                _ => {}
            }
        }

        // 自定义头
        for (k, v) in &self.config.headers {
            request = request.header(k, v);
        }

        let resp = request.send().await
            .map_err(|e| Error::NetworkError(format!("HTTP request failed: {}", e)))?;

        let body = resp.text().await
            .map_err(|e| Error::IOError(format!("Failed to read response: {}", e)))?;

        let json: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| Error::ValidationError(format!("Invalid JSON: {}", e)))?;

        let mut results = Vec::new();
        for (field_name, path) in &self.mapping {
            if let Some(value) = self.extract_json_path(&json, path) {
                results.push(self.json_to_result_value(field_name.clone(), value));
            }
        }

        Ok(results)
    }
}

impl HttpPollHandler {
    fn extract_json_path(&self, json: &serde_json::Value, path: &str) -> Option<serde_json::Value> {
        let path = path.trim_start_matches("$.吃掉");
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = json;
        for part in parts {
            current = current.get(part)?;
        }
        Some(current.clone())
    }

    fn json_to_result_value(&self, name: String, value: serde_json::Value) -> ResultValue {
        use crate::domain::device::driver::ResultValue;
        match value {
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    ResultValue::integer(name, i)
                } else if let Some(f) = n.as_f64() {
                    ResultValue::float(name, f)
                } else {
                    ResultValue::string(name, n.to_string())
                }
            }
            serde_json::Value::Bool(b) => ResultValue::boolean(name, b),
            serde_json::Value::String(s) => ResultValue::string(name, s),
            _ => ResultValue::string(name, value.to_string()),
        }
    }
}
```

---

### Task 8: 创建 `protocol/handlers/mqtt_subscribe.rs`

**Files:**
- Create: `api/src/domain/plugin/protocol/handlers/mqtt_subscribe.rs`

- [ ] **Step 1: 实现 MQTT 订阅处理器**

```rust
//! MQTT 订阅协议处理器

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

use super::ProtocolHandler;
use crate::{
    domain::device::driver::ResultValue,
    dto::entity::Device,
    shared::error::Error,
};

use super::super::config::MqttConfig;

pub struct MqttSubscribeHandler {
    config: MqttConfig,
    mapping: HashMap<String, String>,
    last_message: Arc<RwLock<Option<String>>>,
}

impl MqttSubscribeHandler {
    pub fn new(config: MqttConfig, mapping: HashMap<String, String>) -> Self {
        Self {
            config,
            mapping,
            last_message: Arc::new(RwLock::new(None)),
        }
    }
}

#[async_trait]
impl ProtocolHandler for MqttSubscribeHandler {
    async fn read_data(&self, _device: &Device) -> Result<Vec<ResultValue>, Error> {
        debug!("MQTT subscribe handler called");

        let last = self.last_message.read().await;
        let body = match last.as_ref() {
            Some(msg) => msg.clone(),
            None => return Ok(vec![]),
        };

        let json: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| Error::ValidationError(format!("Invalid MQTT JSON: {}", e)))?;

        let mut results = Vec::new();
        for (field_name, path) in &self.mapping {
            if let Some(value) = self.extract_json_path(&json, path) {
                results.push(self.json_to_result_value(field_name.clone(), value));
            }
        }

        Ok(results)
    }
}

impl MqttSubscribeHandler {
    fn extract_json_path(&self, json: &serde_json::Value, path: &str) -> Option<serde_json::Value> {
        let path = path.trim_start_matches("$.吃掉");
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = json;
        for part in parts {
            current = current.get(part)?;
        }
        Some(current.clone())
    }

    fn json_to_result_value(&self, name: String, value: serde_json::Value) -> ResultValue {
        use crate::domain::device::driver::ResultValue;
        match value {
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    ResultValue::integer(name, i)
                } else if let Some(f) = n.as_f64() {
                    ResultValue::float(name, f)
                } else {
                    ResultValue::string(name, n.to_string())
                }
            }
            serde_json::Value::Bool(b) => ResultValue::boolean(name, b),
            serde_json::Value::String(s) => ResultValue::string(name, s),
            _ => ResultValue::string(name, value.to_string()),
        }
    }
}
```

---

## Chunk 3: 通知渠道插件

### Task 9: 创建 `notification/mod.rs` - 通知渠道模块

**Files:**
- Create: `api/src/domain/plugin/notification/mod.rs`

- [ ] **Step 1: 模块入口**

```rust
//! 通知渠道插件
//!
//! 支持飞书、钉钉、Email 等通知渠道。

pub mod handlers;
pub mod config;

pub use config::{NotificationConfig, FeishuConfig, DingtalkConfig};
pub use handlers::{NotificationHandler, FeishuHandler, DingtalkHandler};

use crate::domain::plugin::{PluginHandler, PluginManifest, AppContext};
use crate::shared::error::Error;
use std::sync::Arc;

pub struct Notification {
    pub level: String,
    pub title: String,
    pub content: String,
    pub extras: std::collections::HashMap<String, String>,
}

/// 创建通知处理器
pub fn create_handler(
    config: &toml::Value,
    _context: Arc<AppContext>,
) -> Result<Box<dyn PluginHandler>, Error> {
    let notification_cfg = config.get("notification")
        .ok_or_else(|| Error::ValidationError("Missing [notification] section".to_string()))?;

    match notification_cfg.get("type").and_then(|v| v.as_str()) {
        Some("feishu") => {
            let cfg: FeishuConfig = notification_cfg.try_into()?;
            Ok(Box::new(FeishuHandler::new(cfg)))
        }
        Some("dingtalk") => {
            let cfg: DingtalkConfig = notification_cfg.try_into()?;
            Ok(Box::new(DingtalkHandler::new(cfg)))
        }
        _ => Err(Error::Unsupported(format!(
            "Unknown notification type: {:?}",
            notification_cfg.get("type")
        ))),
    }
}
```

---

### Task 10: 创建 `notification/config.rs`

**Files:**
- Create: `api/src/domain/plugin/notification/config.rs`

- [ ] **Step 1: 通知配置结构体**

```rust
//! 通知渠道配置结构体

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct NotificationConfig {
    #[serde(rename = "type")]
    pub notification_type: String,
    #[serde(default)]
    pub levels: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FeishuConfig {
    pub webhook_url: String,
    #[serde(default = "default_secret")]
    pub secret: Option<String>,
    #[serde(default)]
    pub levels: Vec<String>,
}

fn default_secret() -> Option<String> { None }

#[derive(Debug, Clone, Deserialize)]
pub struct DingtalkConfig {
    pub webhook_url: String,
    pub secret: Option<String>,
    #[serde(default)]
    pub levels: Vec<String>,
}
```

---

### Task 11: 创建 `notification/handlers/mod.rs`

**Files:**
- Create: `api/src/domain/plugin/notification/handlers/mod.rs`

- [ ] **Step 1: Handler trait 和实现**

```rust
//! 通知处理器

use async_trait::async_trait;
use std::collections::HashMap;
use tracing::{debug, error};

use crate::domain::plugin::notification::Notification;
use crate::shared::error::Error;

pub trait NotificationHandler: Send + Sync {
    async fn send(&self, notification: &Notification) -> Result<(), Error>;
    fn name(&self) -> &str;
}

pub mod feishu;
pub mod dingtalk;

pub use feishu::FeishuHandler;
pub use dingtalk::DingtalkHandler;
```

---

### Task 12: 创建 `notification/handlers/feishu.rs`

**Files:**
- Create: `api/src/domain/plugin/notification/handlers/feishu.rs`

- [ ] **Step 1: 飞书通知处理器**

```rust
//! 飞书通知处理器

use async_trait::async_trait;
use reqwest::Client;
use tracing::debug;

use super::NotificationHandler;
use crate::domain::plugin::notification::Notification;
use crate::shared::error::Error;

use super::super::config::FeishuConfig;

pub struct FeishuHandler {
    config: FeishuConfig,
    client: Client,
}

impl FeishuHandler {
    pub fn new(config: FeishuConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl NotificationHandler for FeishuHandler {
    async fn send(&self, notification: &Notification) -> Result<(), Error> {
        debug!("Sending Feishu notification: {}", notification.title);

        // 检查级别过滤
        if !self.config.levels.is_empty()
            && !self.config.levels.contains(&notification.level) {
            return Ok(());
        }

        let payload = serde_json::json!({
            "msg_type": "text",
            "content": {
                "text": format!("[{}] {}\n{}", notification.level, notification.title, notification.content)
            }
        });

        let resp = self.client.post(&self.config.webhook_url)
            .json(&payload)
            .send().await
            .map_err(|e| Error::NetworkError(format!("Feishu request failed: {}", e)))?;

        if !resp.status().is_success() {
            error!("Feishu API returned: {}", resp.status());
        }

        Ok(())
    }

    fn name(&self) -> &str {
        "FeishuHandler"
    }
}
```

---

### Task 13: 创建 `notification/handlers/dingtalk.rs`

**Files:**
- Create: `api/src/domain/plugin/notification/handlers/dingtalk.rs`

- [ ] **Step 1: 钉钉通知处理器**

```rust
//! 钉钉通知处理器

use async_trait::async_trait;
use reqwest::Client;
use tracing::debug;

use super::NotificationHandler;
use crate::domain::plugin::notification::Notification;
use crate::shared::error::Error;

use super::super::config::DingtalkConfig;

pub struct DingtalkHandler {
    config: DingtalkConfig,
    client: Client,
}

impl DingtalkHandler {
    pub fn new(config: DingtalkConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl NotificationHandler for DingtalkHandler {
    async fn send(&self, notification: &Notification) -> Result<(), Error> {
        debug!("Sending Dingtalk notification: {}", notification.title);

        let payload = serde_json::json!({
            "msgtype": "text",
            "text": {
                "content": format!("[{}] {}\n{}", notification.level, notification.title, notification.content)
            }
        });

        let resp = self.client.post(&self.config.webhook_url)
            .json(&payload)
            .send().await
            .map_err(|e| Error::NetworkError(format!("Dingtalk request failed: {}", e)))?;

        if !resp.status().is_success() {
            error!("Dingtalk API returned: {}", resp.status());
        }

        Ok(())
    }

    fn name(&self) -> &str {
        "DingtalkHandler"
    }
}
```

---

## Chunk 4: 定时任务插件

### Task 14: 创建 `scheduler/mod.rs` - 定时任务模块

**Files:**
- Create: `api/src/domain/plugin/scheduler/mod.rs`

- [ ] **Step 1: 模块入口**

```rust
//! 定时任务插件
//!
//! 支持 Cron 表达式调度的任务。

pub mod handlers;
pub mod config;

pub use config::SchedulerConfig;
pub use handlers::{SchedulerHandler, CronHandler};

use crate::domain::plugin::{PluginHandler, PluginManifest};
use crate::shared::error::Error;
use std::sync::Arc;

pub struct ScheduledTask {
    pub name: String,
    pub payload: serde_json::Value,
}

/// 创建调度处理器
pub fn create_handler(config: &toml::Value) -> Result<Box<dyn PluginHandler>, Error> {
    let scheduler_cfg = config.get("scheduler")
        .ok_or_else(|| Error::ValidationError("Missing [scheduler] section".to_string()))?;

    let cfg: SchedulerConfig = scheduler_cfg.try_into()?;
    Ok(Box::new(CronHandler::new(cfg)))
}
```

---

### Task 15: 创建 `scheduler/config.rs`

**Files:**
- Create: `api/src/domain/plugin/scheduler/config.rs`

- [ ] **Step 1: 调度配置结构体**

```rust
//! 定时任务配置结构体

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct SchedulerConfig {
    #[serde(rename = "type")]
    pub scheduler_type: String,
    pub cron: String,
    pub enabled: bool,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            scheduler_type: "cron".to_string(),
            cron: "0 * * * * *".to_string(),
            enabled: true,
        }
    }
}
```

---

### Task 16: 创建 `scheduler/handlers/mod.rs`

**Files:**
- Create: `api/src/domain/plugin/scheduler/handlers/mod.rs`

- [ ] **Step 1: Handler trait 和实现**

```rust
//! 调度处理器

use async_trait::async_trait;
use tokio_cron_scheduler::{Scheduler, Job};
use tracing::{debug, info};

use crate::domain::plugin::scheduler::ScheduledTask;
use crate::shared::error::Error;

pub trait SchedulerHandler: Send + Sync {
    async fn execute(&self, task: &ScheduledTask) -> Result<(), Error>;
    fn name(&self) -> &str;
}

pub mod cron;

pub use cron::CronHandler;
```

---

### Task 17: 创建 `scheduler/handlers/cron.rs`

**Files:**
- Create: `api/src/domain/plugin/scheduler/handlers/cron.rs`

- [ ] **Step 1: Cron 调度处理器**

```rust
//! Cron 调度处理器

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_cron_scheduler::{Scheduler, Job};
use tracing::{debug, info, error};

use super::SchedulerHandler;
use crate::domain::plugin::scheduler::ScheduledTask;
use crate::shared::error::Error;

use super::super::config::SchedulerConfig;

pub struct CronHandler {
    config: SchedulerConfig,
    scheduler: Arc<RwLock<Option<Scheduler>>>,
}

impl CronHandler {
    pub fn new(config: SchedulerConfig) -> Self {
        Self {
            config,
            scheduler: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn start(&self) -> Result<(), Error> {
        let scheduler = Scheduler::new()
            .await
            .map_err(|e| Error::Internal(format!("Failed to create scheduler: {}", e)))?;

        let cron_expr = self.config.cron.clone();
        let job = Job::new_async(cron_expr.as_str(), move |_uuid, _l| {
            let cron_expr = cron_expr.clone();
            Box::pin(async move {
                info!("Cron job triggered: {}", cron_expr);
            })
        }).map_err(|e| Error::Internal(format!("Failed to create cron job: {}", e)))?;

        scheduler.add(job)
            .await
            .map_err(|e| Error::Internal(format!("Failed to add job: {}", e)))?;

        scheduler.start()
            .await
            .map_err(|e| Error::Internal(format!("Failed to start scheduler: {}", e)))?;

        *self.scheduler.write().await = Some(scheduler);
        Ok(())
    }
}

#[async_trait]
impl SchedulerHandler for CronHandler {
    async fn execute(&self, task: &ScheduledTask) -> Result<(), Error> {
        debug!("Executing scheduled task: {}", task.name);
        // TODO: 根据 task.payload 执行具体任务
        Ok(())
    }

    fn name(&self) -> &str {
        "CronHandler"
    }
}
```

---

## Chunk 5: 示例 TOML 配置

### Task 18: 创建示例配置文件

**Files:**
- Create: `api/plugins/protocol/http_poll_example.toml`

- [ ] **Step 1: HTTP 轮询示例**

```toml
[plugin]
name = "http_poll_example"
version = "1.0.0"
type = "protocol"
description = "HTTP 轮询采集示例 - 温湿度传感器"

[protocol]
type = "http_poll"
base_url = "http://192.168.1.100:8080"
endpoint = "/api/sensors/data"
method = "GET"
poll_interval_ms = 5000

[protocol.headers]
Content-Type = "application/json"

[protocol.auth]
type = "bearer"
token = "your-token-here"

[mapping]
temperature = "$.data.temperature"
humidity = "$.data.humidity"
status = "$.data.status"
```

- Create: `api/plugins/protocol/mqtt_example.toml`

- [ ] **Step 2: MQTT 订阅示例**

```toml
[plugin]
name = "mqtt_sub_example"
version = "1.0.0"
type = "protocol"
description = "MQTT 订阅示例"

[protocol]
type = "mqtt_subscribe"
broker_url = "mqtt://localhost:1883"
client_id = "tinyiothub_client_001"
topic = "sensors/+/data"
qos = 1

[protocol.username]
username = "user"
password = "pass"

[mapping]
temperature = "$.temperature"
humidity = "$.humidity"
```

- Create: `api/plugins/notification/feishu_example.toml`

- [ ] **Step 3: 飞书通知示例**

```toml
[plugin]
name = "feishu_alert"
version = "1.0.0"
type = "notification"
description = "飞书告警通知"

[notification]
type = "feishu"
webhook_url = "https://open.feishu.cn/open-apis/bot/v2/hook/xxx"
secret = "your-secret"
levels = ["error", "critical", "warning"]
```

- Create: `api/plugins/notification/dingtalk_example.toml`

- [ ] **Step 4: 钉钉通知示例**

```toml
[plugin]
name = "dingtalk_alert"
version = "1.0.0"
type = "notification"
description = "钉钉告警通知"

[notification]
type = "dingtalk"
webhook_url = "https://oapi.dingtalk.com/robot/send?access_token=xxx"
secret = "your-secret"
levels = ["error", "critical"]
```

- Create: `api/plugins/scheduler/cron_sync_example.toml`

- [ ] **Step 5: 定时任务示例**

```toml
[plugin]
name = "cron_sync_data"
version = "1.0.0"
type = "scheduler"
description = "数据同步定时任务"

[scheduler]
type = "cron"
cron = "0 */30 * * * *"
enabled = true

[scheduler.payload]
task_type = "sync_data"
target = "external_api"
```

---

## Chunk 6: 集成现有驱动系统

### Task 19: 重构 `api/src/domain/device/driver/mod.rs`

**Files:**
- Modify: `api/src/domain/device/driver/mod.rs`

- [ ] **Step 1: 重写模块，使用新注册表**

```rust
use std::sync::Arc;

pub use driver::{DeviceDriver, DriverWrapper, ResultValue};
pub use drivers::{snmp_driver::SnmpDriver, ModbusDriver, SimulatedDriver};
pub use status::DeviceOverview;
pub use tinyiothub_driver_sdk::{ComponentInfo, ComponentOption, CreateComponentRequest};

pub mod driver;
pub mod drivers;
pub mod dynamic;
pub mod protocol as driver_protocol;
pub mod retry;
pub mod status;

pub use registry::{get_global_registry, PluginRegistry};

use crate::{application::data_context::DataContext, dto::entity::Device, shared::error::Error};

/// 初始化驱动系统
pub fn init_drivers() {
    let registry = get_global_registry();

    // 注册静态驱动
    registry.register_static(
        crate::domain::plugin::PluginManifest {
            name: "SimulatedDriver".to_string(),
            version: Some("1.0.0".to_string()),
            plugin_type: crate::domain::plugin::PluginType::Protocol,
            description: Some("Simulated Device Driver".to_string()),
        },
        |context: Arc<crate::domain::plugin::AppContext>| {
            Ok(Box::new(crate::domain::device::driver::SimulatedDriver::new(
                context.data_context.clone(),
            )) as Box<dyn crate::domain::plugin::PluginHandler>)
        },
    );

    // 从驱动目录自动加载
    let drivers_dir = std::path::PathBuf::from("api/drivers");
    if let Err(e) = registry.load_from_dir(&drivers_dir) {
        tracing::warn!("Failed to auto-load drivers from {:?}: {}", drivers_dir, e);
    }

    tracing::info!("Drivers initialized. Available: {:?}", registry.plugin_names());
}

/// 创建设备驱动
pub fn create_driver(
    driver_name: &str,
    device: &Device,
    context: Arc<DataContext>,
) -> Result<DriverWrapper, Error> {
    let app_context = Arc::new(crate::domain::plugin::AppContext {
        data_context: context,
    });

    let handler = get_global_registry().create_plugin(driver_name, app_context)?;

    // 转换为 DeviceDriver（如果可能）
    // TODO: 需要实现从 PluginHandler 到 DeviceDriver 的转换

    Err(Error::Unsupported(format!("Driver type conversion not implemented: {}", driver_name)))
}
```

---

## Chunk 7: 废弃旧代码

### Task 20: 标记废弃

**Files:**
- Modify: `api/src/domain/device/driver/dynamic/registry.rs`
- Modify: `api/derive/src/lib.rs`

- [ ] **Step 1: 标记 UnifiedDriverRegistry 为废弃**

```rust
#[deprecated(since = "0.2.0", note = "Use PluginRegistry from domain::plugin instead")]
pub struct UnifiedDriverRegistry { ... }
```

- [ ] **Step 2: 标记 register_drivers! 宏为废弃**

在宏文档中添加 `@deprecated` 说明。

---

## Chunk 8: 测试

### Task 21: 单元测试

**Files:**
- Create: `api/src/domain/plugin/tests.rs`

- [ ] **Step 1: 测试配置解析**

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_parse_protocol_plugin() {
        let toml = r#"
[plugin]
name = "test_protocol"
type = "protocol"

[protocol]
type = "http_poll"
base_url = "http://localhost:8080"
endpoint = "/api/data"
method = "GET"
poll_interval_ms = 1000

[mapping]
temp = "$.temperature"
"#;

        let value: toml::Value = toml::from_str(toml).unwrap();
        assert!(value.get("plugin").is_some());
        assert!(value.get("protocol").is_some());
        assert!(value.get("mapping").is_some());
    }

    #[test]
    fn test_parse_notification_plugin() {
        let toml = r#"
[plugin]
name = "test_feishu"
type = "notification"

[notification]
type = "feishu"
webhook_url = "https://open.feishu.cn/..."
levels = ["error", "critical"]
"#;

        let value: toml::Value = toml::from_str(toml).unwrap();
        assert_eq!(value.get("plugin").unwrap().get("type").unwrap().as_str(), Some("notification"));
    }
}
```

- [ ] **Step 2: 运行测试**

```bash
cd api && cargo test --lib plugin -- --nocapture 2>&1 | tail -30
```
Expected: PASS

---

## Plan Complete

Plan saved to: `docs/superpowers/plans/2026-03-26-toml-protocol-plugin.md`

**Ready to execute?**

执行路径:
- 如果有 subagent 可用 → 使用 `superpowers:subagent-driven-development`
- 否则 → 使用 `superpowers:executing-plans`，每个 Chunk 作为独立任务执行
