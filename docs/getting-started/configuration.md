# 配置说明

TinyIoTHub 采用层次化配置系统，支持多环境配置和敏感信息管理。

## 配置文件位置

| 环境 | 配置文件 |
|------|----------|
| 后端 | `api/app_settings.toml` |
| 前端 | `web/.env.local` |

## 后端配置

### 基本配置

```toml
[server]
host = "0.0.0.0"      # 监听地址
port = 3002           # 监听端口
```

### 数据库配置

```toml
[database]
url = "tinyiothub.db"  # 数据库文件路径
auto_migrate = true   # 自动迁移
```

### MQTT 配置

```toml
[mqtt.primary]
host = "192.168.1.124"
port = 1883
username = "admin"
password = "password"
qos = 1

[mqtt.backup]  # 备用通道
enabled = true
host = "192.168.1.125"
port = 1883
```

### 安全配置

```toml
[security.jwt]
secret = "your-secret-key-must-be-at-least-32-characters-long"
expiration_secs = 10800  # 3 小时

[security.rate_limit]
enabled = true
max_requests = 100
window_secs = 60
```

### 驱动配置

```toml
[drivers]
path = "./drivers"     # 驱动目录
auto_load = true      # 自动加载

[drivers.retry]
max_attempts = 3      # 最大重试次数
interval_ms = 1000    # 重试间隔
```

## 前端配置

### 环境变量

```env
# API 地址
NEXT_PUBLIC_API_URL=http://localhost:3002

# MQTT 配置
NEXT_PUBLIC_MQTT_WS_URL=ws://localhost:9001
```

## 配置优先级

配置加载优先级（从高到低）：

1. 环境变量
2. 命令行参数
3. 配置文件
4. 默认值

### 示例：通过环境变量覆盖

```bash
# 通过环境变量修改端口
TINYIOTHUB__SERVER__PORT=8080 cargo run
```
