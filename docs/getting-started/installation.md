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

# 使用 Docker Compose 启动（compose 文件位于 deploy/docker/）
cd deploy/docker
docker-compose up -d
```

### 配置说明

编辑 `deploy/docker/docker-compose.yml` 文件配置服务。该文件包含完整部署栈（TinyIoTHub 服务、Nginx 反向代理、Mosquitto MQTT Broker 等），端口映射与镜像地址以文件内容为准。

## 手动部署

### 后端部署

```bash
# 1. 安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 2. 克隆项目
git clone https://github.com/tinyiothub/tinyiothub.git
cd tinyiothub

# 3. 构建 release 版本
cargo build --release -p tinyiothub-cloud

# 4. 运行
./target/release/tinyiothub-cloud
```

### 前端部署

生产模式下前端由后端以 SPA 模式托管（`wwwroot/`），无需单独部署。如需单独构建：

```bash
cd web

# 安装依赖
pnpm install

# 构建生产版本（产物在 web/dist）
pnpm build
```

## 鸿蒙系统部署

详见 [鸿蒙部署指南](/deployment/harmonyos)

## 验证部署

访问健康检查接口：

```bash
curl http://localhost:3002/api/health
```

响应示例：

```
OK
```
