# 快速开始

本指南将帮助你快速部署和运行 TinyIoTHub 系统。

## 环境要求

### 后端要求
- **Rust**: 1.70+ (2021 Edition)
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
cd api
cargo run
```

后端服务将在 http://localhost:3002 启动

#### 启动前端

```bash
cd web
pnpm install
pnpm dev
```

前端应用将在 http://localhost:3001 启动

### 方式二：生产模式（单进程部署）

#### 构建后端

```bash
# Windows
.\scripts\build-single-binary.ps1 -Release

# Linux/macOS
./scripts/build-single-binary.sh --release
```

#### 运行

```bash
cd api
.\target\release\tinyiothub.exe  # Windows
./target/release/tinyiothub      # Linux/macOS
```

## 访问服务

启动后访问以下地址：

| 服务 | 地址 |
|------|------|
| Web 管理界面 | http://localhost:3001/ |
| 后端 API | http://localhost:3002/api/v1/ |
| 健康检查 | http://localhost:3002/api/v1/system/health |

## 默认账号

首次启动后，使用以下默认账号登录：

- **用户名**: admin
- **密码**: admin123

> ⚠️ 建议首次登录后立即修改默认密码！

## 下一步

- [安装部署详解 →](/getting-started/installation)
- [配置说明 →](/getting-started/configuration)
- [设备管理 →](/guide/devices)
