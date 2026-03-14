# 配置文件重构和迁移指南

## 概述

根据项目命名规范（`.kiro/steering/naming.md`），我们重构了配置文件的命名和结构，使其更加专业和易于维护。

## 文件命名变更

### 旧文件名 → 新文件名

```
appSetting.toml              → app_settings.toml
appSetting-harmonyos.toml    → app_settings_harmonyos.toml
```

### 命名规范原则

1. **使用 snake_case 格式**：遵循项目命名规范
2. **描述性命名**：`app_settings` 比 `appSetting` 更清晰
3. **一致性**：所有配置文件使用相同的命名模式

## 配置结构重构

### 旧结构（扁平化）
```toml
# 所有配置项在根级别
auth_time = 3
connection_string = "tinyiothub.db"
mqtt_host = "192.168.1.124"
mqtt_port = 1883
host_port = 3002
```

### 新结构（层次化）
```toml
# 按功能域分组的层次结构
[server]
host = "0.0.0.0"
port = 3002

[database]
url = "tinyiothub.db"

[mqtt.primary]
host = "192.168.1.124"
port = 1883

[security.jwt]
expiration_secs = 10800
```

## 配置键命名规范

### 命名原则

1. **使用 snake_case**：`max_connections` 而不是 `maxConnections`
2. **描述性命名**：`expiration_secs` 而不是 `auth_time`
3. **一致的单位后缀**：
   - `_secs` 表示秒
   - `_ms` 表示毫秒
   - `_mb` 表示兆字节
   - `_percent` 表示百分比

### 键名映射表

| 旧键名 | 新键名 | 说明 |
|--------|--------|------|
| `auth_time` | `security.jwt.expiration_secs` | JWT过期时间（秒） |
| `connection_string` | `database.url` | 数据库连接字符串 |
| `host_port` | `server.port` | HTTP服务器端口 |
| `mqtt_host` | `mqtt.primary.host` | MQTT主服务器地址 |
| `mqtt_port` | `mqtt.primary.port` | MQTT主服务器端口 |
| `mqtt_usr` | `mqtt.primary.username` | MQTT用户名 |
| `mqtt_pwd` | `mqtt.primary.password` | MQTT密码 |
| `mqtt_host_4g` | `mqtt.secondary.host` | MQTT备用服务器地址 |
| `mqtt_port_4g` | `mqtt.secondary.port` | MQTT备用服务器端口 |
| `heartbeat_time` | `mqtt.primary.keep_alive_secs` | MQTT心跳间隔 |
| `upload_time` | `device.data_collection.interval_secs` | 数据上传间隔 |
| `message_max_limit` | `messaging.max_message_limit` | 消息队列最大限制 |
| `app_log_enable` | `logging.file_enabled` | 是否启用文件日志 |
| `app_log_level` | `logging.level` | 日志级别 |

## 向后兼容性

### 自动兼容
- 系统会自动加载新旧两种格式的配置文件
- 旧的配置文件仍然有效，无需立即迁移
- 新配置会覆盖旧配置（优先级更高）

### 加载顺序
1. 默认配置
2. `app_settings.toml`（新格式，优先级最高）
3. `app_settings_harmonyos.toml`（鸿蒙专用）
4. `appSetting.toml`（旧格式，向后兼容）
5. 环境变量覆盖

## 迁移步骤

### 立即可用
✅ **无需任何操作** - 系统已经支持新配置格式

### 推荐迁移（可选）
1. **复制现有配置**：
   ```bash
   cp appSetting.toml appSetting.toml.backup
   ```

2. **使用新配置文件**：
   - 编辑 `app_settings.toml`
   - 根据需要调整配置值

3. **验证配置**：
   ```bash
   cargo run  # 检查是否正常启动
   ```

4. **删除旧文件**（可选）：
   ```bash
   rm appSetting.toml appSetting-harmonyos.toml
   ```

## 配置验证

### 新增验证功能
- **类型检查**：确保配置值类型正确
- **范围验证**：检查端口号、超时时间等是否在合理范围内
- **依赖检查**：验证相关配置的一致性
- **格式验证**：检查URL、路径等格式是否正确

### 错误处理
- 详细的错误信息，指出具体的配置问题
- 建议的修复方案
- 配置文件位置和行号信息

## 环境特定配置

### 开发环境
```toml
[environment]
name = "development"

[features]
debug_mode = true
dev_tools = true
```

### 生产环境
```toml
[environment]
name = "production"

[features]
debug_mode = false
experimental_features = false
```

### 鸿蒙系统
```toml
[environment]
name = "harmonyos"

[harmonyos]
permissions = ["ohos.permission.INTERNET"]
resources.max_memory_mb = 256
```

## 最佳实践

### 配置组织
1. **按功能分组**：相关配置放在同一个section
2. **使用嵌套结构**：避免扁平化的长键名
3. **添加注释**：解释复杂配置的用途
4. **合理默认值**：提供安全的默认配置

### 安全考虑
1. **敏感信息**：使用环境变量存储密码和密钥
2. **权限控制**：配置文件权限设置为600
3. **版本控制**：不要将包含敏感信息的配置文件提交到git

### 维护建议
1. **定期审查**：检查配置是否仍然适用
2. **文档更新**：保持配置文档的最新状态
3. **测试验证**：在不同环境中测试配置

## 故障排除

### 常见问题

1. **配置文件找不到**
   - 检查文件名是否正确
   - 确认文件在项目根目录

2. **配置格式错误**
   - 使用TOML语法检查器
   - 检查引号和括号匹配

3. **配置值类型错误**
   - 检查数字是否加引号
   - 确认布尔值使用true/false

### 调试技巧
```bash
# 检查配置加载情况
RUST_LOG=debug cargo run

# 验证配置文件语法
toml-cli check app_settings.toml
```

## 总结

这次配置文件重构带来了以下改进：

1. **规范化命名**：符合项目命名标准
2. **结构化组织**：层次清晰，易于理解
3. **类型安全**：编译时检查配置类型
4. **向后兼容**：不破坏现有部署
5. **扩展性强**：易于添加新配置选项
6. **环境感知**：支持不同环境的配置

配置系统现在更加专业、可维护，并为未来的扩展奠定了良好基础。