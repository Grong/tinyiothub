# 快速开始

本指南将帮助你快速部署和运行 TinyIoTHub 系统。

## 环境要求

### 后端要求
- **Rust**: 1.85+ (2024 Edition)
- **操作系统**: Linux, Windows, HarmonyOS
- **数据库**: SQLite (内置)
- **网络**: MQTT Broker (可选)

### 前端要求
- **Node.js**: 18+
- **pnpm**: 8+ (推荐)
- **浏览器**: Chrome, Firefox, Safari, Edge

## 安装部署

### 方式一：开发模式（分离部署）

#### 启动后端

```bash
cd cloud
cargo run
```

后端服务将在 http://localhost:3002 启动

#### 启动前端

```bash
cd web
pnpm install
pnpm dev
```

前端应用将在 http://localhost:5173 启动（Vite 开发服务器，API 请求自动代理到后端）

### 方式二：生产模式（单进程部署）

#### 构建后端

```bash
# 前端静态资源 + 后端发布构建（仓库根目录）
cd web && pnpm build && cd ..
cargo build --release -p tinyiothub-cloud
```

或使用 Just 快捷命令：`just build`（后端 release）；容器化一体构建见根目录 `Dockerfile`（`docker build -t tinyiothub .`）。

> 注：`scripts/build-static.sh` 与 `scripts/build-single-binary.ps1` 基于旧的 api/ 目录布局，已废弃，勿使用。

#### 运行

```bash
./target/release/tinyiothub-cloud
```

生产模式下后端直接托管前端页面（SPA），无需单独启动前端。

## 访问服务

启动后访问以下地址：

| 服务 | 地址 |
|------|------|
| Web 管理界面（开发模式） | http://localhost:5173/ |
| Web 管理界面（生产模式） | http://localhost:3002/ |
| 后端 API | http://localhost:3002/api/v1/ |
| 健康检查 | http://localhost:3002/api/health |

## 默认账号

首次启动后，使用以下默认账号登录：

- **用户名**: admin
- **密码**: admin123

> ⚠️ 建议首次登录后立即修改默认密码！

## 下一步

- [安装部署详解 →](/getting-started/installation)
- [配置说明 →](/getting-started/configuration)
- [物管理 →](/guide/devices)
