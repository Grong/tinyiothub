# 配置系统架构

## 概述

TinyIoTHub 采用现代化的层次配置系统，支持多源配置加载、环境感知、类型安全验证等特性。配置系统基于 Rust 的类型系统设计，提供编译时和运行时的双重安全保障。

## 架构设计

### 🏗️ 系统架构

```
┌─────────────────────────────────────────────────────────────┐
│                    Configuration System                     │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │   Sources   │  │ Validation  │  │   Environment       │  │
│  │             │  │             │  │   Detection         │  │
│  │ • Files     │  │ • Types     │  │                     │  │
│  │ • Env Vars  │  │ • Ranges    │  │ • Development       │  │
│  │ • Defaults  │  │ • Formats   │  │ • Production        │  │
│  │             │  │ • Security  │  │ • HarmonyOS         │  │
│  └─────────────┘  └─────────────┘  └─────────────────────┘  │
├─────────────────────────────────────────────────────────────┤
│                    Settings Structure                       │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │   Server    │  │  Database   │  │       MQTT          │  │
│  │   Config    │  │   Config    │  │      Config         │  │
│  └─────────────┘  └─────────────┘  └─────────────────────┘  │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │  Security   │  │   Device    │  │    Monitoring       │  │
│  │   Config    │  │   Config    │  │      Config         │  │
│  └─────────────┘  └─────────────┘  └─────────────────────┘  │
├─────────────────────────────────────────────────────────────┤
│                    Legacy Compatibility                     │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │  Legacy API (config_util::get_str, get_num, etc.)      │ │
│  │  • Backward compatibility with old configuration       │ │
│  │  • Automatic mapping from new to old format            │ │
│  └─────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

### 🔧 核心组件

#### 1. 配置源 (Sources)
- **默认配置**: 内置的安全默认值
- **文件配置**: TOML/JSON 配置文件
- **环境变量**: 运行时环境覆盖
- **命令行参数**: 启动时参数覆盖

#### 2. 配置验证 (Validation)
- **类型检查**: 编译时类型安全
- **范围验证**: 端口号、超时时间等范围检查
- **格式验证**: URL、路径、邮箱等格式验证
- **依赖检查**: 配置项之间的依赖关系验证

#### 3. 环境感知 (Environment)
- **开发环境**: 调试友好的配置
- **生产环境**: 性能和安全优化的配置
- **鸿蒙系统**: 资源受限环境的优化配置

## 配置结构

### 📁 文件组织

```
src/infrastructure/config/
├── mod.rs              # 主模块，全局配置管理
├── settings.rs         # 配置结构定义
├── sources.rs          # 多配置源支持
├── validation.rs       # 配置验证系统
└── environment.rs      # 环境变量处理
```

### 🏗️ 配置层次结构

```toml
# app_settings.toml
[server]                    # HTTP 服务器配置
host = "0.0.0.0"
port = 3002
max_connections = 1000

[database]                  # 数据库配置
url = "tinyiothub.db"
max_connections = 10
auto_migrate = true

[mqtt]                      # MQTT 配置
[mqtt.primary]              # 主 MQTT 服务器
host = "192.168.1.124"
port = 1883
username = "admin"
password = "password"

[mqtt.secondary]            # 备用 MQTT 服务器
host = "175.178.49.5"
port = 9990

[security]                  # 安全配置
[security.jwt]              # JWT 配置
secret = "your-secret-key-must-be-at-least-32-characters-long"
expiration_secs = 10800

[logging]                   # 日志配置
level = "info"
console_enabled = true
file_enabled = true

[device]                    # 设备配置
serial_number = ""
name = "TinyIoTHub"

[monitoring]                # 监控配置
[monitoring.health_check]
enabled = true
interval_secs = 30
```

## 配置加载

### 🔄 加载顺序

配置系统按以下优先级加载配置（后加载的覆盖先加载的）：

1. **默认配置** (最低优先级)
2. **app_settings.toml** (新格式)
3. **app_settings_harmonyos.toml** (鸿蒙专用)
4. **appSetting.toml** (旧格式，向后兼容)
5. **环境变量** (最高优先级)

### 📝 使用示例

#### 现代 API
```rust
use crate::infrastructure::config;

// 获取全局配置
let config = config::get();

// 访问配置值
let mqtt_host = &config.mqtt.primary.host;
let jwt_secret = &config.security.jwt.secret;
let server_port = config.server.port;

// 检查功能开关
if config.is_feature_enabled("debug_mode") {
    // 调试模式逻辑
}

// 环境检查
if config.is_development() {
    // 开发环境逻辑
}
```

#### 兼容 API (向后兼容)
```rust
use crate::infrastructure::config::legacy;

// 旧的配置访问方式仍然有效
let mqtt_host = legacy::get_str("mqtt_host");
let mqtt_port = legacy::get_num("mqtt_port");
let auth_time = legacy::get_num_or_def("auth_time", 3600);
```

## 环境特定配置

### 🔧 开发环境
```toml
[environment]
name = "development"

[features]
debug_mode = true
dev_tools = true
experimental_features = true

[logging]
level = "debug"
console_enabled = true

[security.jwt]
expiration_secs = 86400  # 24小时，开发便利
```

### 🚀 生产环境
```toml
[environment]
name = "production"

[features]
debug_mode = false
experimental_features = false

[logging]
level = "warn"
file_enabled = true

[security.jwt]
expiration_secs = 3600   # 1小时，安全优先
```

### 📱 鸿蒙系统
```toml
[environment]
name = "harmonyos"

[server]
max_connections = 500    # 资源限制

[database]
max_connections = 5      # 资源限制

[mqtt.client]
message_queue_size = 500 # 降低内存使用

[harmonyos]
permissions = ["ohos.permission.INTERNET"]
resources.max_memory_mb = 256
```

## 配置验证

### ✅ 验证规则

#### 1. 类型验证
```rust
// 编译时类型检查
pub struct ServerConfig {
    pub host: String,           // 必须是字符串
    pub port: u16,              // 必须是有效端口号
    pub max_connections: usize, // 必须是正整数
}
```

#### 2. 范围验证
```rust
impl ConfigValidator {
    fn validate_server_config(&self, config: &ServerConfig) -> Result<(), ConfigError> {
        // 端口范围检查
        if config.port < 1024 || config.port > 65535 {
            return Err(ConfigError::ValidationError(
                "端口号必须在 1024-65535 范围内".to_string()
            ));
        }
        
        // 连接数检查
        if config.max_connections == 0 || config.max_connections > 10000 {
            return Err(ConfigError::ValidationError(
                "最大连接数必须在 1-10000 范围内".to_string()
            ));
        }
        
        Ok(())
    }
}
```

#### 3. 安全验证
```rust
impl ConfigValidator {
    fn validate_jwt_config(&self, config: &JwtConfig) -> Result<(), ConfigError> {
        // JWT 密钥长度检查
        if config.secret.len() < 32 {
            return Err(ConfigError::ValidationError(
                "JWT 密钥长度必须至少 32 个字符".to_string()
            ));
        }
        
        // 过期时间检查
        if config.expiration_secs < 300 || config.expiration_secs > 86400 * 7 {
            return Err(ConfigError::ValidationError(
                "JWT 过期时间必须在 5分钟 到 7天 之间".to_string()
            ));
        }
        
        Ok(())
    }
}
```

## 环境变量覆盖

### 🌍 环境变量命名规则

环境变量使用 `TINYIOTHUB_` 前缀，层次结构用双下划线分隔：

```bash
# 服务器配置
export TINYIOTHUB_SERVER__HOST="0.0.0.0"
export TINYIOTHUB_SERVER__PORT="3002"

# 数据库配置
export TINYIOTHUB_DATABASE__URL="sqlite:///data/tinyiothub.db"
export TINYIOTHUB_DATABASE__MAX_CONNECTIONS="20"

# MQTT 配置
export TINYIOTHUB_MQTT__PRIMARY__HOST="mqtt.example.com"
export TINYIOTHUB_MQTT__PRIMARY__PORT="1883"
export TINYIOTHUB_MQTT__PRIMARY__USERNAME="iot_user"
export TINYIOTHUB_MQTT__PRIMARY__PASSWORD="secure_password"

# JWT 配置
export TINYIOTHUB_SECURITY__JWT__SECRET="production-jwt-secret-key-must-be-at-least-32-characters-long"
export TINYIOTHUB_SECURITY__JWT__EXPIRATION_SECS="3600"

# 日志配置
export IOT_EDGE_LOGGING__LEVEL="warn"
export IOT_EDGE_LOGGING__FILE_ENABLED="true"
```

### 📋 使用示例

```bash
# 开发环境
export IOT_EDGE_ENVIRONMENT__NAME="development"
export IOT_EDGE_FEATURES__DEBUG_MODE="true"
export IOT_EDGE_LOGGING__LEVEL="debug"

# 生产环境
export IOT_EDGE_ENVIRONMENT__NAME="production"
export IOT_EDGE_FEATURES__DEBUG_MODE="false"
export IOT_EDGE_LOGGING__LEVEL="warn"
export IOT_EDGE_SECURITY__JWT__SECRET="production-secret-key"

# 启动应用
cargo run
```

## 错误处理

### 🚨 错误类型

```rust
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("配置文件未找到: {0}")]
    FileNotFound(String),
    
    #[error("配置解析失败: {0}")]
    ParseError(String),
    
    #[error("配置验证错误: {0}")]
    ValidationError(String),
    
    #[error("环境变量错误: {0}")]
    EnvError(String),
    
    #[error("配置初始化失败")]
    InitializationFailed,
}
```

### 🔍 错误诊断

```rust
// 详细的错误信息
match config::initialize() {
    Ok(_) => println!("配置加载成功"),
    Err(ConfigError::ValidationError(msg)) => {
        eprintln!("配置验证失败: {}", msg);
        eprintln!("请检查配置文件中的相关设置");
    },
    Err(ConfigError::FileNotFound(file)) => {
        eprintln!("配置文件未找到: {}", file);
        eprintln!("请确保配置文件存在于项目根目录");
    },
    Err(e) => eprintln!("配置错误: {}", e),
}
```

## 最佳实践

### 🎯 配置组织

1. **按功能分组**: 相关配置放在同一个 section
2. **使用嵌套结构**: 避免扁平化的长键名
3. **提供合理默认值**: 确保系统能够开箱即用
4. **添加注释**: 解释复杂配置的用途和影响

### 🔒 安全考虑

1. **敏感信息**: 使用环境变量存储密码和密钥
2. **权限控制**: 配置文件权限设置为 600
3. **版本控制**: 不要将包含敏感信息的配置文件提交到 git
4. **密钥轮换**: 定期更换 JWT 密钥和其他敏感配置

### 🚀 性能优化

1. **一次加载**: 配置在启动时加载一次，全局共享
2. **零拷贝访问**: 配置读取无额外内存分配
3. **类型安全**: 编译时优化，运行时无类型转换开销
4. **缓存友好**: 配置结构设计考虑 CPU 缓存效率

## 故障排除

### 🔧 常见问题

#### 1. 配置文件找不到
```bash
# 检查文件是否存在
ls -la app_settings.toml

# 检查文件权限
chmod 644 app_settings.toml
```

#### 2. 配置格式错误
```bash
# 使用 TOML 检查器
toml-cli check app_settings.toml

# 检查 JSON 格式
jq . app_settings.json
```

#### 3. 环境变量不生效
```bash
# 检查环境变量
env | grep IOT_EDGE_

# 验证变量名格式
echo $IOT_EDGE_SERVER__PORT
```

### 📊 调试技巧

```bash
# 启用配置调试日志
RUST_LOG=iot_edge_gateway::infrastructure::config=trace cargo run

# 检查配置加载过程
RUST_LOG=debug cargo run 2>&1 | grep -i config

# 验证最终配置
curl http://localhost:3002/api/system/configuration
```

## 扩展指南

### 🔧 添加新配置项

1. **更新配置结构**:
```rust
// src/infrastructure/config/settings.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MyNewConfig {
    pub enabled: bool,
    pub timeout_secs: u64,
    pub endpoints: Vec<String>,
}

// 添加到主配置结构
pub struct ApplicationSettings {
    // ... 其他配置
    pub my_new_feature: MyNewConfig,
}
```

2. **添加验证规则**:
```rust
// src/infrastructure/config/validation.rs
impl ConfigValidator {
    fn validate_my_new_config(&self, config: &MyNewConfig) -> Result<(), ConfigError> {
        if config.timeout_secs == 0 {
            return Err(ConfigError::ValidationError(
                "超时时间不能为0".to_string()
            ));
        }
        Ok(())
    }
}
```

3. **更新默认配置**:
```rust
// 在 create_default_settings() 中添加默认值
my_new_feature: MyNewConfig {
    enabled: false,
    timeout_secs: 30,
    endpoints: vec![],
},
```

4. **添加环境变量支持**:
```rust
// 环境变量: IOTEDGE_MY_NEW_FEATURE__ENABLED
// 环境变量: IOTEDGE_MY_NEW_FEATURE__TIMEOUT_SECS
```

### 🔄 配置热重载 (未来功能)

```rust
// 未来可能的热重载实现
pub async fn reload_configuration() -> Result<(), ConfigError> {
    let new_settings = load_configuration()?;
    
    // 验证新配置
    new_settings.validate()?;
    
    // 原子性更新全局配置
    // 注意: 当前使用 OnceLock，不支持热重载
    // 未来可能使用 RwLock<ApplicationSettings>
    
    tracing::info!("配置热重载完成");
    Ok(())
}
```

---

**维护团队**: TinyIoTHub 开发团队  
**最后更新**: 2025-01-03  
**文档版本**: v1.0.0