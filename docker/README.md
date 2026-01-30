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

- **小体积**: 后端镜像 ~60MB，前端镜像 ~150MB
- **多阶段构建**: 分离编译和运行环境
- **Alpine 基础**: 轻量级 Linux 发行版
- **Rust 稳定版**: 使用 Rust 1.83+ stable
- **串口支持**: 支持 Modbus RTU 等串口设备
- **数据持久化**: 数据库和日志文件映射到宿主机

### 架构说明

```
┌─────────────────────────────────────────┐
│         Nginx (172.30.0.4:8099)         │
│              反向代理                    │
└────────────┬────────────────────────────┘
             │
    ┌────────┴────────┐
    │                 │
┌───▼────────┐  ┌────▼──────────┐
│ Web 前端   │  │  API 后端     │
│ (172.30.0.3)│  │ (172.30.0.2)  │
│ Next.js    │  │  Rust/Axum    │
└────────────┘  └───────────────┘
                      │
                ┌─────▼──────┐
                │  SQLite DB │
                │  持久化存储 │
                └────────────┘
```

## 前置条件

### 开发机要求

- **Docker**: 用于构建镜像
- **Rust 工具链**: 1.83+ 版本
- **Node.js**: 18+ 版本和 pnpm 包管理器
- **hdc 工具**: 用于与 OpenHarmony 设备通信
- **cross**: 交叉编译工具 (`cargo install cross`)

### 目标设备要求

- **系统**: OpenHarmony (ARM64 架构)
- **Docker**: 已安装并运行
- **内存**: 至少 2GB 可用
- **磁盘**: 至少 5GB 可用空间

## 镜像构建

### 方法一：使用自动化脚本（推荐）

项目提供了自动化构建脚本，一键构建并导出镜像：

```bash
# Windows PowerShell
.\scripts\build-and-export-docker.ps1

# Linux/macOS
./scripts/build-and-export-docker.sh
```

构建完成后，在项目根目录生成：
- `tinyiothub-api-arm64.tar` - 后端 API 镜像
- `tinyiothub-web-arm64.tar` - 前端 Web 镜像

### 方法二：手动构建

#### 1. 构建后端镜像

```bash
# 交叉编译 ARM64 二进制
cross build --target aarch64-unknown-linux-gnu --release

# 构建 Docker 镜像
docker build --platform linux/arm64 -t tinyiothub-api:arm64 -f Dockerfile .

# 导出镜像
docker save tinyiothub-api:arm64 -o tinyiothub-api-arm64.tar
```

#### 2. 构建前端镜像

```bash
cd web
pnpm install
docker build --platform linux/arm64 -t tinyiothub-web:arm64 -f Dockerfile .
docker save tinyiothub-web:arm64 -o ../tinyiothub-web-arm64.tar
cd ..
```

### 验证镜像

```bash
# 查看镜像大小
ls -lh tinyiothub-api-arm64.tar tinyiothub-web-arm64.tar

# 本地测试（可选）
docker load < tinyiothub-api-arm64.tar
docker load < tinyiothub-web-arm64.tar
docker images | grep tinyiothub
```

## 部署到 OpenHarmony

### 目录结构

在设备上创建以下目录结构：

```
/data/tinyiothub/
├── app_settings.toml          # 应用配置文件
├── nginx/
│   └── nginx.conf             # Nginx 配置文件
├── data/                      # 数据目录（数据库）
├── logs/                      # 日志目录
├── start-containers.sh        # 启动脚本
└── stop-containers.sh         # 停止脚本
```

### 快速部署步骤

#### 1. 准备配置文件

```bash
# 创建目录
hdc shell "mkdir -p /data/tinyiothub/nginx /data/tinyiothub/data /data/tinyiothub/logs"

# 传输配置文件
hdc file send app_settings.toml /data/tinyiothub/app_settings.toml
hdc file send docker/nginx/nginx.conf /data/tinyiothub/nginx/nginx.conf
hdc file send docker/start-containers.sh /data/tinyiothub/start-containers.sh
hdc file send docker/stop-containers.sh /data/tinyiothub/stop-containers.sh

# 设置执行权限
hdc shell "chmod +x /data/tinyiothub/start-containers.sh /data/tinyiothub/stop-containers.sh"
```

#### 2. 加载 Docker 镜像

```bash
# 传输镜像文件
hdc file send tinyiothub-api-arm64.tar /data/tinyiothub/
hdc file send tinyiothub-web-arm64.tar /data/tinyiothub/

# 加载镜像
hdc shell "cd /data/tinyiothub && docker load < tinyiothub-api-arm64.tar"
hdc shell "cd /data/tinyiothub && docker load < tinyiothub-web-arm64.tar"

# 验证
hdc shell "docker images | grep tinyiothub"
```

#### 3. 启动服务

```bash
hdc shell "cd /data/tinyiothub && ./start-containers.sh"
```

启动脚本会自动：
- 创建自定义网络 `tinyiothub-net` (172.30.0.0/16)
- 启动 API 容器 (172.30.0.2:3002)
- 启动 Web 容器 (172.30.0.3:3000)
- 启动 Nginx 容器 (172.30.0.4，对外端口 8099)

#### 4. 验证部署

```bash
# 检查容器状态
hdc shell "docker ps"

# 检查端口监听
hdc shell "netstat -tuln | grep 8099"

# 查看日志
hdc shell "docker logs tinyiothub-nginx --tail 20"
```

#### 5. 访问应用

浏览器访问：`http://<设备IP>:8099`

默认登录凭据：
- 用户名：`admin`
- 密码：`admin123`

## 配置说明

### docker-compose.yml

如果使用 docker-compose（开发环境）：

```yaml
services:
  tinyiothub-api:
    image: tinyiothub-api:latest
    container_name: tinyiothub-api
    restart: unless-stopped
    privileged: true              # 串口访问权限
    devices:
      - /dev/ttyUSB0:/dev/ttyUSB0  # 串口设备映射
    environment:
      - RUST_LOG=info
      - TZ=Asia/Shanghai
      - TINYIOTHUB__DATABASE__URL=/app/data/tinyiothub.db
    volumes:
      - ./app_settings.toml:/app/app_settings.toml:ro
      - ./data:/app/data
      - ./logs:/app/logs
    expose:
      - "3002"

  tinyiothub-web:
    image: tinyiothub-web:latest
    container_name: tinyiothub-web
    restart: unless-stopped
    environment:
      - NODE_ENV=production
      - TZ=Asia/Shanghai
    expose:
      - "3000"
    depends_on:
      - tinyiothub-api

  tinyiothub-nginx:
    image: nginx:alpine
    container_name: tinyiothub-nginx
    restart: unless-stopped
    ports:
      - "8080:80"
    volumes:
      - ./nginx/nginx.conf:/etc/nginx/conf.d/default.conf:ro
    depends_on:
      - tinyiothub-api
      - tinyiothub-web
```

### 环境变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `RUST_LOG` | `info` | 日志级别 (error/warn/info/debug/trace) |
| `TZ` | `Asia/Shanghai` | 时区设置 |
| `TINYIOTHUB__DATABASE__URL` | `/app/data/tinyiothub.db` | 数据库路径 |
| `NODE_ENV` | `production` | Node.js 环境 |

### 端口说明

- **8099**: Nginx 对外端口（OpenHarmony 部署）
- **8080**: Nginx 对外端口（开发环境）
- **3002**: 后端 API 内部端口
- **3000**: 前端 Web 内部端口

## 常用命令

### 服务管理

```bash
# 查看服务状态
hdc shell "docker ps --filter 'name=tinyiothub-'"

# 启动服务
hdc shell "cd /data/tinyiothub && ./start-containers.sh"

# 停止服务
hdc shell "cd /data/tinyiothub && ./stop-containers.sh"

# 重启服务
hdc shell "cd /data/tinyiothub && ./stop-containers.sh && ./start-containers.sh"

# 重启单个容器
hdc shell "docker restart tinyiothub-nginx"
```

### 日志查看

```bash
# 查看 API 日志
hdc shell "docker logs tinyiothub-api --tail 50"

# 查看 Web 日志
hdc shell "docker logs tinyiothub-web --tail 50"

# 查看 Nginx 日志
hdc shell "docker logs tinyiothub-nginx --tail 50"

# 实时跟踪日志
hdc shell "docker logs -f tinyiothub-api"

# 查看应用日志文件
hdc shell "tail -f /data/tinyiothub/logs/app.log"
```

### 镜像管理

```bash
# 查看镜像
hdc shell "docker images | grep tinyiothub"

# 删除镜像
hdc shell "docker rmi tinyiothub-api:arm64 tinyiothub-web:arm64"

# 清理未使用的镜像
hdc shell "docker image prune -a"
```

### 容器管理

```bash
# 进入容器
hdc shell "docker exec -it tinyiothub-api sh"

# 查看容器资源使用
hdc shell "docker stats --no-stream"

# 查看容器详细信息
hdc shell "docker inspect tinyiothub-api"
```

### 更新服务

```bash
# 1. 停止旧服务
hdc shell "cd /data/tinyiothub && ./stop-containers.sh"

# 2. 删除旧容器
hdc shell "docker rm tinyiothub-nginx tinyiothub-web tinyiothub-api"

# 3. 传输并加载新镜像
hdc file send tinyiothub-api-arm64.tar /data/tinyiothub/
hdc file send tinyiothub-web-arm64.tar /data/tinyiothub/
hdc shell "cd /data/tinyiothub && docker load < tinyiothub-api-arm64.tar"
hdc shell "cd /data/tinyiothub && docker load < tinyiothub-web-arm64.tar"

# 4. 启动新服务
hdc shell "cd /data/tinyiothub && ./start-containers.sh"
```

## 故障排查

### 容器无法启动

```bash
# 查看详细日志
hdc shell "docker logs tinyiothub-api"

# 检查容器详细信息
hdc shell "docker inspect tinyiothub-api"

# 检查网络连接
hdc shell "docker network inspect tinyiothub-net"

# 检查配置文件
hdc shell "cat /data/tinyiothub/app_settings.toml"
```

### 无法访问服务

```bash
# 检查端口映射
hdc shell "docker ps | grep tinyiothub"

# 检查端口监听（OpenHarmony 会绑定到 IPv6）
hdc shell "netstat -tuln | grep 8099"

# 测试容器内部连接
hdc shell "docker exec tinyiothub-nginx wget -qO- http://tinyiothub-api:3002/api/health"
hdc shell "docker exec tinyiothub-nginx wget -qO- http://tinyiothub-web:3000"
```

### DNS 解析问题

如果 nginx 报错 "no resolver defined"，检查 `nginx.conf`：

```nginx
resolver 127.0.0.11 valid=10s ipv6=off;
resolver_timeout 5s;
```

### 串口访问失败

```bash
# 检查串口设备
hdc shell "ls -l /dev/tty*"

# 检查容器内设备权限
hdc shell "docker exec tinyiothub-api ls -l /dev/ttyUSB0"

# 确认 privileged 模式
hdc shell "docker inspect tinyiothub-api | grep Privileged"
```

### 数据库问题

```bash
# 检查数据库文件
hdc shell "ls -lh /data/tinyiothub/data/tinyiothub.db"

# 检查挂载
hdc shell "docker inspect tinyiothub-api | grep -A 10 Mounts"

# 进入容器检查
hdc shell "docker exec -it tinyiothub-api sh"
```

### 内存不足

```bash
# 查看容器资源使用
hdc shell "docker stats --no-stream"

# 查看系统内存
hdc shell "free -h"
```

## 性能优化

### 镜像体积优化

当前配置已优化：
- 多阶段构建，分离编译和运行环境
- Alpine 基础镜像（最小化）
- Strip 二进制文件
- Next.js standalone 模式

### 运行时优化

在 `start-containers.sh` 中添加资源限制：

```bash
docker run -d \
  --name tinyiothub-api \
  --cpus="2" \
  --memory="512m" \
  --memory-swap="512m" \
  ...
```

### 日志优化

容器日志由 Docker 自动管理，定期清理：

```bash
# 清理未使用的容器和镜像
hdc shell "docker system prune -f"

# 查看磁盘使用
hdc shell "docker system df"
```

## 安全建议

### 1. 修改默认密码

首次登录后立即修改 admin 密码。

### 2. 限制 privileged 模式

生产环境不要使用 `--privileged`，只映射必要的设备：

```bash
--device /dev/ttyUSB0:/dev/ttyUSB0
```

### 3. 配置文件权限

```bash
hdc shell "chmod 600 /data/tinyiothub/app_settings.toml"
```

### 4. 使用环境变量管理敏感信息

不要在配置文件中硬编码密钥：

```bash
export TINYIOTHUB__SECURITY__JWT__SECRET="your-secret-key"
```

### 5. 定期更新镜像

定期更新到最新版本以获取安全补丁。

### 6. 限制网络访问

配置防火墙规则，只允许必要的 IP 访问。

## 备份与恢复

### 备份

```bash
# 备份数据目录
hdc shell "tar -czf /data/tinyiothub-backup-$(date +%Y%m%d).tar.gz /data/tinyiothub/data/"

# 下载备份到本地
hdc file recv /data/tinyiothub-backup-*.tar.gz ./

# 备份数据库文件
hdc file recv /data/tinyiothub/data/tinyiothub.db ./backup/
```

### 恢复

```bash
# 上传备份文件
hdc file send tinyiothub-backup-20260128.tar.gz /data/

# 停止服务
hdc shell "cd /data/tinyiothub && ./stop-containers.sh"

# 恢复数据
hdc shell "tar -xzf /data/tinyiothub-backup-20260128.tar.gz -C /"

# 启动服务
hdc shell "cd /data/tinyiothub && ./start-containers.sh"
```

### 定期备份脚本

在设备上创建定时备份：

```bash
# 创建备份脚本
cat > /data/tinyiothub/backup.sh << 'EOF'
#!/bin/sh
BACKUP_DIR="/data/backups"
DATE=$(date +%Y%m%d)
mkdir -p $BACKUP_DIR
tar -czf $BACKUP_DIR/tinyiothub-$DATE.tar.gz /data/tinyiothub/data/
# 保留最近 7 天的备份
find $BACKUP_DIR -name "tinyiothub-*.tar.gz" -mtime +7 -delete
EOF

chmod +x /data/tinyiothub/backup.sh

# 添加到 crontab（每天凌晨 2 点）
echo "0 2 * * * /data/tinyiothub/backup.sh" | crontab -
```

## OpenHarmony 特殊说明

### IPv6 端口绑定

OpenHarmony 的 Docker 使用 `0.0.0.0:8099:80` 端口映射时会绑定到 IPv6 (`:::8099`)。这是系统行为，不影响使用，IPv4 连接仍可正常访问。

### 重启策略

所有容器配置了 `--restart unless-stopped`，设备重启后自动启动。

### 网络配置

使用自定义网络 `tinyiothub-net` (172.30.0.0/16)，容器固定 IP：
- API: 172.30.0.2
- Web: 172.30.0.3
- Nginx: 172.30.0.4

## 卸载

```bash
# 停止并删除容器
hdc shell "cd /data/tinyiothub && ./stop-containers.sh"
hdc shell "docker rm tinyiothub-nginx tinyiothub-web tinyiothub-api"

# 删除网络
hdc shell "docker network rm tinyiothub-net"

# 删除镜像
hdc shell "docker rmi tinyiothub-api:arm64 tinyiothub-web:arm64"

# 删除数据（谨慎操作）
hdc shell "rm -rf /data/tinyiothub/"
```

## 附录

### A. 镜像压缩传输

如果镜像文件较大，可以压缩后传输：

```bash
# 压缩镜像
gzip tinyiothub-api-arm64.tar
gzip tinyiothub-web-arm64.tar

# 传输压缩文件
hdc file send tinyiothub-api-arm64.tar.gz /data/tinyiothub/
hdc file send tinyiothub-web-arm64.tar.gz /data/tinyiothub/

# 解压并加载
hdc shell "cd /data/tinyiothub && gunzip tinyiothub-api-arm64.tar.gz && docker load < tinyiothub-api-arm64.tar"
hdc shell "cd /data/tinyiothub && gunzip tinyiothub-web-arm64.tar.gz && docker load < tinyiothub-web-arm64.tar"
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

#### 问题1：cross 编译失败

```bash
# 安装 cross
cargo install cross

# 确保 Docker 运行
docker ps

# 清理缓存重新编译
cargo clean
cross build --target aarch64-unknown-linux-gnu --release
```

#### 问题2：前端构建内存不足

```bash
# 增加 Node.js 内存限制
export NODE_OPTIONS="--max-old-space-size=4096"
pnpm build
```

#### 问题3：Docker 构建平台不匹配

```bash
# 启用 buildx
docker buildx create --use

# 构建多平台镜像
docker buildx build --platform linux/arm64 -t tinyiothub-api:arm64 --load .
```

### D. 版本管理

使用版本标签管理镜像：

```bash
# 构建时添加版本标签
docker build --platform linux/arm64 \
  -t tinyiothub-api:arm64 \
  -t tinyiothub-api:v1.0.0 .

# 导出特定版本
docker save tinyiothub-api:v1.0.0 -o tinyiothub-api-v1.0.0-arm64.tar

# 使用特定版本
docker tag tinyiothub-api:v1.0.0 tinyiothub-api:arm64
```

## 技术支持

遇到问题请提供：
1. 设备架构：`hdc shell "uname -a"`
2. Docker 版本：`hdc shell "docker --version"`
3. 容器日志：`hdc shell "docker logs <container-name>"`
4. 系统日志：`hdc shell "dmesg | tail -50"`

官方网站：https://tinyiothub.com
