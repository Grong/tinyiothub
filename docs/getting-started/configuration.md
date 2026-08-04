# 配置说明

TinyIoTHub 采用层次化配置系统，支持多环境配置和敏感信息管理。

## 配置文件位置

| 环境 | 配置文件 |
|------|----------|
| 后端 | `app_settings.toml`（仓库根目录，参考 `app_settings.example.toml`） |
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

[mqtt.secondary]  # 备用通道
host = "192.168.1.125"
port = 1883
```

### 安全配置

```toml
[security.jwt]
secret = "your-secret-key-must-be-at-least-32-characters-long"
expiration_secs = 10800  # 3 小时

[security.rate_limiting]
enabled = true
requests_per_minute = 60
auth_requests_per_minute = 10
```

## 前端配置

前端基于 Vite 构建，API 代理在 `web/vite.config.ts` 中配置：

```typescript
server: {
  port: 5173,
  proxy: {
    "/api": {
      target: "http://localhost:3002",
      changeOrigin: true,
    },
    "/v1": {
      target: "http://localhost:3002",
      changeOrigin: true,
    }
  }
}
```

开发模式下前端运行在 `http://localhost:5173`，API 请求通过 Vite 代理转发到后端 `http://localhost:3002`。

## 配置优先级

配置加载优先级（从高到低）：

1. 环境变量
2. 命令行参数
3. 配置文件
4. 默认值

### 示例：通过环境变量覆盖

```bash
# 通过环境变量修改端口
TINYIOTHUB__SERVER__PORT=8080 cargo run -p tinyiothub-cloud
```
