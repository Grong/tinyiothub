# TinyIoTHub - Docker 部署指南

本指南提供 TinyIoTHub 的完整 Docker 容器化部署方案，针对 OpenHarmony ARM64 设备优化。

## 目录

- [概述](#概述)
- [前置条件](#前置条件)
- [镜像构建](#镜像构建)
- [部署到 OpenHarmony](#部署到-openharmony)
- [配置说明](#配置说明)
- [常用命令](#常用命令)
- [故障排查](#故障排查)
- [性能优化](#性能优化)
- [安全建议](#安全建议)
- [备份与恢复](#备份与恢复)

## 概述

### 镜像特点

- **一体化镜像**: 前后端集成在单个容器中，简化部署
- **小体积**: 完整镜像 ~100MB
- **多阶段构建**: 分离编译和运行环境
- **Alpine 基础**: 轻量级 Linux 发行版
- **Rust Nightly**: 使用 Rust 1.83+ nightly（支持 edition2024）
- **串口支持**: 支持 Modbus RTU 等串口设备
- **数据持久化**: 数据库和日志文件映射到宿主机
- **内置静态文件服务**: Axum 直接提供前端静态文件

### 架构说明

```
┌─────────────────────────────────────────┐
│      TinyIoTHub 容器 (端口 3002)        │
│  ┌────────────────────────────────────┐ │
│  │   Axum Web Server                  │ │
│  │  ┌──────────┐    ┌──────────────┐ │ │
│  │  │ API 路由 │    │ 静态文件服务 │ │ │
│  │  │ /api/*   │    │ /            │ │ │
│  │  └──────────┘    └──────────────┘ │ │
│  │         │              │           │ │
│  │         └──────┬───────┘           │ │
│  │                │                   │ │
│  │         ┌──────▼──────┐            │ │
│  │         │  业务逻辑   │            │ │
│  │         └──────┬──────┘            │ │
│  │                │                   │ │
│  │         ┌──────▼──────┐            │ │
│  │         │  SQLite DB  │            │ │
│  │         │ (持久化存储) │            │ │
│  │         └─────────────┘            │ │
│  └────────────────────────────────────┘ │
└─────────────────────────────────────────┘
```

## 前置条件

### 开发机要求

- **Docker**: 20.10+ 版本，支持 buildx
- **hdc 工具**: 用于与 OpenHarmony 设备通信

### 目标设备要求

- **系统**: OpenHarmony (ARM64 架构)
- **Docker**: 已安装并运行
- **内存**: 至少 512MB 可用
- **磁盘**: 至少 2GB 可用空间
- **网络**: 可访问外部网络（首次拉取镜像）

## 镜像构建

### 本地构建（x86_64）

用于本地开发测试：

```bash
# 使用构建脚本
.\scripts\docker-build.ps1 -Tag test

# 或手动构建
docker build -t tinyiothub:latest -f Dockerfile .
```

### ARM64 构建（OpenHarmony 设备）

构建适用于鸿蒙设备的镜像：

```bash
# 使用多架构构建脚本
.\scripts\docker-build-multiarch.ps1

# 或手动构建
docker buildx build --platform linux/arm64 -t tinyiothub:arm64 -f Dockerfile . --load
```

### 导出镜像

```bash
# 导出 ARM64 镜像
docker save tinyiothub:arm64 -o tinyiothub-arm64.tar

# 压缩以减小传输大小（可选）
gzip tinyiothub-arm64.tar
```

## 离线部署（无外网环境）

适用于目标机器无法访问 Docker Hub 的场景。

### 1. 构建并导出镜像

```bash
# Windows PowerShell
.\scripts\docker-build-fast.ps1
docker save tinyiothub:latest -o tinyiothub.tar
```

### 2. 复制到目标机器

```bash
scp tinyiothub.tar docker-compose.local.yml user@target-host:~/
```

### 3. 在目标机器上部署

```bash
sudo docker load -i tinyiothub.tar
sudo docker compose -f docker-compose.local.yml up -d
```

> `docker-compose.local.yml` 引用本地镜像 `tinyiothub:latest`，`docker-compose.yml` 引用 Docker Hub 镜像，用于 CI/CD。

## 部署到 OpenHarmony

### 快速部署步骤

#### 1. 准备设备环境

```bash
# 设置设备 ID（替换为你的设备 ID）
$DEVICE_ID = "150100424a54443452025f70fa85c700"

# 创建目录
hdc -t $DEVICE_ID shell "mkdir -p /data/tinyiothub/data /data/tinyiothub/logs"
```

#### 2. 传输镜像

```bash
# 传输镜像文件
hdc -t $DEVICE_ID file send tinyiothub-arm64.tar /data/tinyiothub/

# 如果使用了压缩
hdc -t $DEVICE_ID file send tinyiothub-arm64.tar.gz /data/tinyiothub/
hdc -t $DEVICE_ID shell "cd /data/tinyiothub && gunzip tinyiothub-arm64.tar.gz"
```

#### 3. 加载镜像

```bash
# 加载到 Docker
hdc -t $DEVICE_ID shell "docker load < /data/tinyiothub/tinyiothub-arm64.tar"

# 验证镜像
hdc -t $DEVICE_ID shell "docker images | grep tinyiothub"
```

#### 4. 启动容器

```bash
# 停止旧容器（如果存在）
hdc -t $DEVICE_ID shell "docker stop tinyiothub 2>/dev/null; docker rm tinyiothub 2>/dev/null"

# 启动新容器
hdc -t $DEVICE_ID shell "docker run -d \
  --name tinyiothub \
  --restart unless-stopped \
  -p 3002:3002 \
  -v /data/tinyiothub/data:/app/data \
  -v /data/tinyiothub/logs:/app/logs \
  -e RUST_LOG=info \
  -e TZ=Asia/Shanghai \
  tinyiothub:arm64"
```

#### 5. 验证部署

```bash
# 检查容器状态
hdc -t $DEVICE_ID shell "docker ps | grep tinyiothub"

# 查看日志
hdc -t $DEVICE_ID shell "docker logs tinyiothub --tail 20"

# 测试健康检查
hdc -t $DEVICE_ID shell "wget -qO- http://localhost:3002/api/health"
```

#### 6. 访问应用

获取设备 IP 地址：
```bash
hdc -t $DEVICE_ID shell "ifconfig | grep 'inet addr'"
```

浏览器访问：`http://<设备IP>:3002`

默认登录凭据：
- 用户名：`admin`
- 密码：`admin123`

## 配置说明

### 环境变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `RUST_LOG` | `info` | 日志级别 (error/warn/info/debug/trace) |
| `TZ` | `Asia/Shanghai` | 时区设置 |
| `TINYIOTHUB__DATABASE__URL` | `/app/data/tinyiothub.db` | 数据库路径 |
| `JWT_SECRET` | (内置默认值) | JWT 密钥，生产环境必须修改 |
| `MQTT_USERNAME` | `admin` | MQTT 用户名 |
| `MQTT_PASSWORD` | `admin123` | MQTT 密码 |

### 自定义配置

如需自定义配置，可以挂载配置文件：

```bash
# 1. 复制示例配置
cp api/app_settings.example.toml app_settings.toml

# 2. 修改配置文件（根据需要）
# 编辑 app_settings.toml

# 3. 传输到设备
hdc -t $DEVICE_ID file send app_settings.toml /data/tinyiothub/

# 4. 启动时挂载配置
hdc -t $DEVICE_ID shell "docker run -d \
  --name tinyiothub \
  -p 3002:3002 \
  -v /data/tinyiothub/app_settings.toml:/app/app_settings.toml:ro \
  -v /data/tinyiothub/data:/app/data \
  -v /data/tinyiothub/logs:/app/logs \
  tinyiothub:arm64"
```

### 串口设备访问

如需访问串口设备（Modbus RTU 等）：

```bash
hdc -t $DEVICE_ID shell "docker run -d \
  --name tinyiothub \
  --privileged \
  --device /dev/ttyUSB0:/dev/ttyUSB0 \
  -p 3002:3002 \
  -v /data/tinyiothub/data:/app/data \
  -v /data/tinyiothub/logs:/app/logs \
  tinyiothub:arm64"
```

### docker-compose 部署（开发环境）

本地开发可使用 docker-compose：

```yaml
# docker-compose.yml
version: '3.8'

services:
  tinyiothub:
    image: tinyiothub:latest
    container_name: tinyiothub
    restart: unless-stopped
    ports:
      - "3002:3002"
    volumes:
      - ./data:/app/data
      - ./logs:/app/logs
    environment:
      - RUST_LOG=info
      - TZ=Asia/Shanghai
    healthcheck:
      test: ["CMD", "wget", "--spider", "http://localhost:3002/api/health"]
      interval: 30s
      timeout: 3s
      retries: 3
```

启动：
```bash
docker-compose up -d
```

## 常用命令

### 服务管理

```bash
# 设置设备 ID
$DEVICE_ID = "150100424a54443452025f70fa85c700"

# 查看容器状态
hdc -t $DEVICE_ID shell "docker ps | grep tinyiothub"

# 启动容器
hdc -t $DEVICE_ID shell "docker start tinyiothub"

# 停止容器
hdc -t $DEVICE_ID shell "docker stop tinyiothub"

# 重启容器
hdc -t $DEVICE_ID shell "docker restart tinyiothub"

# 删除容器
hdc -t $DEVICE_ID shell "docker rm tinyiothub"
```

### 日志查看

```bash
# 查看最近日志
hdc -t $DEVICE_ID shell "docker logs tinyiothub --tail 50"

# 实时跟踪日志
hdc -t $DEVICE_ID shell "docker logs -f tinyiothub"

# 查看应用日志文件
hdc -t $DEVICE_ID shell "tail -f /data/tinyiothub/logs/app.log"

# 下载日志到本地
hdc -t $DEVICE_ID file recv /data/tinyiothub/logs/app.log ./
```

### 镜像管理

```bash
# 查看镜像
hdc -t $DEVICE_ID shell "docker images | grep tinyiothub"

# 删除镜像
hdc -t $DEVICE_ID shell "docker rmi tinyiothub:arm64"

# 清理未使用的镜像
hdc -t $DEVICE_ID shell "docker image prune -a -f"
```

### 容器管理

```bash
# 进入容器
hdc -t $DEVICE_ID shell "docker exec -it tinyiothub sh"

# 查看容器资源使用
hdc -t $DEVICE_ID shell "docker stats --no-stream tinyiothub"

# 查看容器详细信息
hdc -t $DEVICE_ID shell "docker inspect tinyiothub"
```

### 更新服务

```bash
# 1. 停止并删除旧容器
hdc -t $DEVICE_ID shell "docker stop tinyiothub && docker rm tinyiothub"

# 2. 删除旧镜像（可选）
hdc -t $DEVICE_ID shell "docker rmi tinyiothub:arm64"

# 3. 传输并加载新镜像
hdc -t $DEVICE_ID file send tinyiothub-arm64.tar /data/tinyiothub/
hdc -t $DEVICE_ID shell "docker load < /data/tinyiothub/tinyiothub-arm64.tar"

# 4. 启动新容器
hdc -t $DEVICE_ID shell "docker run -d \
  --name tinyiothub \
  --restart unless-stopped \
  -p 3002:3002 \
  -v /data/tinyiothub/data:/app/data \
  -v /data/tinyiothub/logs:/app/logs \
  tinyiothub:arm64"
```

## 故障排查

### 容器无法启动

```bash
# 查看详细日志
hdc -t $DEVICE_ID shell "docker logs tinyiothub"

# 检查容器详细信息
hdc -t $DEVICE_ID shell "docker inspect tinyiothub"

# 检查配置文件
hdc -t $DEVICE_ID shell "cat /data/tinyiothub/app_settings.toml"

# 检查数据目录权限
hdc -t $DEVICE_ID shell "ls -la /data/tinyiothub/"
```

### 无法访问服务

```bash
# 检查端口映射
hdc -t $DEVICE_ID shell "docker ps | grep tinyiothub"

# 检查端口监听
hdc -t $DEVICE_ID shell "netstat -tuln | grep 3002"

# 测试容器内部连接
hdc -t $DEVICE_ID shell "docker exec tinyiothub wget -qO- http://localhost:3002/api/health"

# 检查防火墙
hdc -t $DEVICE_ID shell "iptables -L -n | grep 3002"
```

### JWT 配置错误

如果看到 "JWT secret must be at least 32 characters long" 错误：

```bash
# 检查环境变量
hdc -t $DEVICE_ID shell "docker inspect tinyiothub | grep JWT"

# 使用自定义配置文件
# 编辑 app_settings.toml，设置 security.jwt.secret
# 然后挂载配置文件启动容器
```

### 串口访问失败

```bash
# 检查串口设备
hdc -t $DEVICE_ID shell "ls -l /dev/tty*"

# 检查容器内设备权限
hdc -t $DEVICE_ID shell "docker exec tinyiothub ls -l /dev/ttyUSB0"

# 确认 privileged 模式
hdc -t $DEVICE_ID shell "docker inspect tinyiothub | grep Privileged"
```

### 数据库问题

```bash
# 检查数据库文件
hdc -t $DEVICE_ID shell "ls -lh /data/tinyiothub/data/tinyiothub.db"

# 检查挂载
hdc -t $DEVICE_ID shell "docker inspect tinyiothub | grep -A 10 Mounts"

# 进入容器检查
hdc -t $DEVICE_ID shell "docker exec -it tinyiothub sh"
# 在容器内：ls -la /app/data/
```

### 内存不足

```bash
# 查看容器资源使用
hdc -t $DEVICE_ID shell "docker stats --no-stream tinyiothub"

# 查看系统内存
hdc -t $DEVICE_ID shell "free -h"

# 限制容器内存
hdc -t $DEVICE_ID shell "docker run -d \
  --name tinyiothub \
  --memory=512m \
  --memory-swap=512m \
  -p 3002:3002 \
  tinyiothub:arm64"
```

## 性能优化

### 镜像体积优化

当前配置已优化：
- 多阶段构建，分离编译和运行环境
- Alpine 基础镜像（最小化）
- Vite 静态构建
- 单一容器部署，减少网络开销

### 运行时优化

添加资源限制：

```bash
hdc -t $DEVICE_ID shell "docker run -d \
  --name tinyiothub \
  --cpus='1' \
  --memory='512m' \
  --memory-swap='512m' \
  -p 3002:3002 \
  tinyiothub:arm64"
```

### 日志优化

限制日志大小：

```bash
hdc -t $DEVICE_ID shell "docker run -d \
  --name tinyiothub \
  --log-opt max-size=10m \
  --log-opt max-file=3 \
  -p 3002:3002 \
  tinyiothub:arm64"
```

### 清理磁盘空间

```bash
# 清理未使用的容器和镜像
hdc -t $DEVICE_ID shell "docker system prune -f"

# 查看磁盘使用
hdc -t $DEVICE_ID shell "docker system df"
```

## 安全建议

### 1. 修改默认密码

首次登录后立即修改 admin 密码。

### 2. 设置 JWT 密钥

生产环境必须设置自定义 JWT 密钥：

```bash
# 生成随机密钥（至少 32 字符）
openssl rand -base64 32

# 通过环境变量设置
hdc -t $DEVICE_ID shell "docker run -d \
  --name tinyiothub \
  -e JWT_SECRET='your-generated-secret-key-here' \
  -p 3002:3002 \
  tinyiothub:arm64"
```

### 3. 限制 privileged 模式

生产环境不要使用 `--privileged`，只映射必要的设备：

```bash
--device /dev/ttyUSB0:/dev/ttyUSB0
```

### 4. 配置文件权限

```bash
hdc -t $DEVICE_ID shell "chmod 600 /data/tinyiothub/app_settings.toml"
```

### 5. 定期更新镜像

定期更新到最新版本以获取安全补丁。

### 6. 限制网络访问

配置防火墙规则，只允许必要的 IP 访问。

## 备份与恢复

### 备份

```bash
# 备份数据目录
hdc -t $DEVICE_ID shell "tar -czf /data/tinyiothub-backup-\$(date +%Y%m%d).tar.gz /data/tinyiothub/data/"

# 下载备份到本地
hdc -t $DEVICE_ID file recv /data/tinyiothub-backup-*.tar.gz ./

# 备份数据库文件
hdc -t $DEVICE_ID file recv /data/tinyiothub/data/tinyiothub.db ./backup/
```

### 恢复

```bash
# 上传备份文件
hdc -t $DEVICE_ID file send tinyiothub-backup-20260226.tar.gz /data/

# 停止服务
hdc -t $DEVICE_ID shell "docker stop tinyiothub"

# 恢复数据
hdc -t $DEVICE_ID shell "tar -xzf /data/tinyiothub-backup-20260226.tar.gz -C /"

# 启动服务
hdc -t $DEVICE_ID shell "docker start tinyiothub"
```

## OpenHarmony 特殊说明

### 重启策略

容器配置了 `--restart unless-stopped`，设备重启后自动启动。

### 端口访问

应用监听在 `0.0.0.0:3002`，可通过设备 IP 访问。

## 卸载

```bash
# 停止并删除容器
hdc -t $DEVICE_ID shell "docker stop tinyiothub && docker rm tinyiothub"

# 删除镜像
hdc -t $DEVICE_ID shell "docker rmi tinyiothub:arm64"

# 删除数据（谨慎操作）
hdc -t $DEVICE_ID shell "rm -rf /data/tinyiothub/"
```

## 附录

### A. 镜像压缩传输

如果镜像文件较大，可以压缩后传输：

```bash
# 压缩镜像
gzip tinyiothub-arm64.tar

# 传输压缩文件
hdc -t $DEVICE_ID file send tinyiothub-arm64.tar.gz /data/tinyiothub/

# 解压并加载
hdc -t $DEVICE_ID shell "cd /data/tinyiothub && gunzip tinyiothub-arm64.tar.gz && docker load < tinyiothub-arm64.tar"
```

### B. 开发环境部署

在开发环境（非 OpenHarmony）测试：

```bash
# 使用 docker-compose
docker-compose up -d

# 查看日志
docker-compose logs -f

# 停止服务
docker-compose down
```

### C. 常见构建问题

#### 问题1：buildx 不可用

```bash
# 启用 buildx
docker buildx create --use

# 构建多平台镜像
docker buildx build --platform linux/arm64 -t tinyiothub:arm64 --load -f Dockerfile .
```

#### 问题2：前端构建内存不足

```bash
# 增加 Node.js 内存限制
$env:NODE_OPTIONS="--max-old-space-size=4096"
cd web
pnpm build
```

#### 问题3：Docker 构建平台不匹配

确保使用 `--platform linux/arm64` 参数构建 ARM64 镜像。

### D. 版本管理

使用版本标签管理镜像：

```bash
# 构建时添加版本标签
docker build --platform linux/arm64 \
  -t tinyiothub:arm64 \
  -t tinyiothub:v1.0.0 \
  -f Dockerfile .

# 导出特定版本
docker save tinyiothub:v1.0.0 -o tinyiothub-v1.0.0-arm64.tar
```

## 技术支持

遇到问题请提供：
1. 设备架构：`hdc -t $DEVICE_ID shell "uname -a"`
2. Docker 版本：`hdc -t $DEVICE_ID shell "docker --version"`
3. 容器日志：`hdc -t $DEVICE_ID shell "docker logs tinyiothub"`
4. 系统日志：`hdc -t $DEVICE_ID shell "dmesg | tail -50"`
