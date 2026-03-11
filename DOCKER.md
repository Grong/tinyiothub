# TinyIoTHub Docker 部署指南

## 快速开始

### 1. 构建镜像

#### 快速测试（推荐）

```bash
# Windows PowerShell - 一键构建、启动并测试
.\scripts\test-docker.ps1
```

此脚本会自动完成：
- 检查并构建镜像
- 停止旧容器
- 启动新容器
- 检查服务状态
- 显示运行日志

#### 本地构建（x86_64）

```bash
# Windows PowerShell - 快速构建（使用缓存）
.\scripts\docker-build-fast.ps1

# 传统构建
.\scripts\docker-build.ps1 -Tag test

# Linux/Mac
docker build -t tinyiothub:latest -f Dockerfile .
```

**构建时间说明：**
- 首次构建：8-10 分钟（编译所有依赖）
- 后续构建：1-2 分钟（只编译修改的代码）
- 详见 [构建优化指南](docs/docker-build-optimization.md)

#### ARM64 构建（鸿蒙设备）

```bash
# Windows PowerShell - 快速构建 ARM64
.\scripts\docker-build-fast.ps1 -Platform linux/arm64 -Tag arm64

# 多架构构建（需要推送到仓库）
.\scripts\docker-build-multiarch.ps1

# 手动构建
docker buildx build --platform linux/arm64 -t tinyiothub:arm64 -f Dockerfile . --load
```

**注意：** ARM64 构建需要 QEMU 模拟，首次构建可能需要 15-20 分钟。

### 2. 启动服务

#### 本地开发

```bash
docker-compose up -d
```

#### 鸿蒙设备部署

详见 [docker/README.md](docker/README.md) 完整部署指南。

### 3. 访问应用

- 本地开发：http://localhost:3002
- 鸿蒙设备：http://<设备IP>:3002

默认账号：
- 用户名：`admin`
- 密码：`admin123`

## 镜像说明

### 架构特点

- **一体化镜像**：前后端集成在单个容器中
- **小体积**：完整镜像约 100MB
- **多阶段构建**：分离编译和运行环境
- **Alpine 基础**：轻量级 Linux 发行版
- **内置静态服务**：Axum 直接提供前端文件

### 镜像内容

```
/app/
├── tinyiothub              # Rust 后端可执行文件
├── wwwroot/                # Next.js 静态文件
├── migrations/             # 数据库迁移文件
├── templates/              # 设备模板
├── app_settings.toml       # 默认配置
├── data/                   # 数据目录（挂载点）
└── logs/                   # 日志目录（挂载点）
```

## 配置说明

### 环境变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `RUST_LOG` | `info` | 日志级别 |
| `TZ` | `Asia/Shanghai` | 时区 |
| `TINYIOTHUB__DATABASE__URL` | `/app/data/tinyiothub.db` | 数据库路径 |
| `JWT_SECRET` | (内置) | JWT 密钥 |
| `MQTT_USERNAME` | `admin` | MQTT 用户名 |
| `MQTT_PASSWORD` | `admin123` | MQTT 密码 |

### 数据持久化

默认挂载以下目录：

- `./data` - 数据库文件
- `./logs` - 应用日志

### 自定义配置

```bash
# 1. 复制配置模板
cp api/app_settings.example.toml app_settings.toml

# 2. 修改配置文件

# 3. 启动时挂载配置
docker run -d \
  --name tinyiothub \
  -p 3002:3002 \
  -v $(pwd)/app_settings.toml:/app/app_settings.toml:ro \
  -v $(pwd)/data:/app/data \
  -v $(pwd)/logs:/app/logs \
  tinyiothub:latest
```

## 常用命令

### 查看日志

```bash
# 查看实时日志
docker-compose logs -f

# 查看最近100行日志
docker-compose logs --tail=100
```

### 停止服务

```bash
docker-compose down
```

### 重启服务

```bash
docker-compose restart
```

### 更新镜像

```bash
# 重新构建
.\scripts\docker-build.ps1

# 重启服务
docker-compose up -d
```

### 清理数据

```bash
# 停止并删除容器
docker-compose down

# 删除数据（谨慎操作）
rm -rf data logs
```

## 健康检查

容器内置健康检查，每30秒检查一次服务状态：

```bash
# 查看健康状态
docker ps
```

健康状态说明：
- `healthy` - 服务正常
- `unhealthy` - 服务异常
- `starting` - 启动中

## 故障排查

### 容器无法启动

1. 查看日志：
```bash
docker-compose logs
```

2. 检查端口占用：
```bash
netstat -ano | findstr :3002
```

### JWT 配置错误

如果看到 "JWT secret must be at least 32 characters long"：

- 检查 `app_settings.toml` 中的 `security.jwt.secret` 配置
- 或通过环境变量设置：`-e JWT_SECRET='your-32-char-secret'`

### 数据库错误

1. 检查数据目录权限
2. 删除损坏的数据库文件（会丢失数据）：
```bash
rm data/tinyiothub.db*
docker-compose restart
```

### 前端无法访问

1. 确认容器正在运行：
```bash
docker ps
```

2. 检查健康状态：
```bash
docker inspect tinyiothub | grep Health
```

## 生产环境建议

1. **修改默认密码**：首次登录后立即修改 admin 密码

2. **设置 JWT 密钥**：生产环境必须设置自定义 JWT 密钥

3. **定期备份**：备份 `data` 目录

4. **监控日志**：定期检查 `logs` 目录

5. **资源限制**：在 `docker-compose.yml` 中添加资源限制：
```yaml
deploy:
  resources:
    limits:
      cpus: '1'
      memory: 512M
```

## 鸿蒙设备部署

TinyIoTHub 支持在 OpenHarmony (鸿蒙) 设备上运行。

详细部署步骤请参考：[docker/README.md](docker/README.md)

### 快速部署

```bash
# 1. 构建 ARM64 镜像
.\scripts\docker-build-multiarch.ps1

# 2. 导出镜像
docker save tinyiothub:arm64 -o tinyiothub-arm64.tar

# 3. 传输到设备
hdc -t <DEVICE_ID> file send tinyiothub-arm64.tar /data/tinyiothub/

# 4. 加载并运行
hdc -t <DEVICE_ID> shell "docker load < /data/tinyiothub/tinyiothub-arm64.tar"
hdc -t <DEVICE_ID> shell "docker run -d --name tinyiothub -p 3002:3002 tinyiothub:arm64"
```

## 网络模式

默认使用 bridge 网络模式。如需访问宿主机串口设备，可修改为 host 模式：

```yaml
network_mode: host
privileged: true
devices:
  - /dev/ttyUSB0:/dev/ttyUSB0
```
