# 安装部署

本文档详细介绍 TinyIoTHub 的各种部署方式。

## Docker 部署

### 前置要求

- Docker 20.10+
- Docker Compose 2.0+

### 快速启动

```bash
# 克隆项目
git clone https://github.com/tinyiothub/tinyiothub.git
cd tinyiothub

# 使用 Docker Compose 启动
docker-compose up -d
```

### 配置说明

编辑 `docker-compose.yml` 文件配置服务：

```yaml
services:
  tinyiothub:
    image: tinyiothub:latest
    ports:
      - "3001:3001"  # 前端
      - "3002:3002"  # 后端
    volumes:
      - ./data:/app/data
    environment:
      - DATABASE_URL=tinyiothub.db
      - MQTT_HOST=broker
```

## 手动部署

### 后端部署

```bash
# 1. 安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 2. 克隆项目
git clone https://github.com/tinyiothub/tinyiothub.git
cd tinyiothub/api

# 3. 构建 release 版本
cargo build --release

# 4. 运行
./target/release/tinyiothub
```

### 前端部署

```bash
cd web

# 安装依赖
pnpm install

# 构建生产版本
pnpm build

# 启动生产服务器
pnpm start
```

## 鸿蒙系统部署

详见 [鸿蒙部署指南](/deployment/harmonyos)

## 验证部署

访问健康检查接口：

```bash
curl http://localhost:3002/api/v1/system/health
```

响应示例：

```json
{
  "code": 0,
  "msg": "",
  "result": {
    "status": "healthy",
    "version": "1.0.0",
    "uptime": "24h30m15s"
  }
}
```
