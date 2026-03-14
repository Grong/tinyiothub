# Docker 部署

使用 Docker 快速部署 TinyIoTHub。

## 前置要求

- Docker 20.10+
- Docker Compose 2.0+

## 快速开始

```bash
# 克隆项目
git clone https://github.com/tinyiothub/tinyiothub.git
cd tinyiothub

# 启动所有服务
docker-compose up -d
```

## 配置说明

### docker-compose.yml

```yaml
version: '3.8'

services:
  tinyiothub:
    build: .
    container_name: tinyiothub
    ports:
      - "3001:3001"
      - "3002:3002"
    volumes:
      - ./data:/app/data
      - ./logs:/app/logs
    environment:
      - DATABASE_URL=tinyiothub.db
      - RUST_LOG=info
    restart: unless-stopped
```

### 构建镜像

```bash
# 构建镜像
docker build -t tinyiothub:latest .

# 运行
docker run -d -p 3001:3001 -p 3002:3002 tinyiothub:latest
```

## 数据持久化

推荐将数据目录挂载到宿主机：

```yaml
volumes:
  - ./data:/app/data
```

## 日志管理

查看容器日志：

```bash
docker logs -f tinyiothub
```

## 网络配置

如需连接外部 MQTT broker：

```yaml
environment:
  - MQTT__PRIMARY__HOST=192.168.1.124
  - MQTT__PRIMARY__PORT=1883
```

## 健康检查

```bash
curl http://localhost:3002/api/v1/system/health
```
