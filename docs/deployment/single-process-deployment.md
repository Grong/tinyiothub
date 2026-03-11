# 单进程部署方案（最终方案）

## 方案说明

Next.js 前端 + Rust 后端的单进程部署方案。前端文件存放在 `wwwroot/` 目录，由 Rust 后端的 Axum 服务器提供静态文件服务。

## 架构

```
┌──────────────────────────────────┐
│   单个 Rust 进程                  │
│  ┌────────────────────────────┐  │
│  │  Axum Web Server           │  │
│  │  - API 路由 (/api/*)       │  │
│  │  - 静态文件服务 (/)        │  │
│  └────────────────────────────┘  │
└──────────────────────────────────┘

外部文件:
api/wwwroot/
├── .next/          # Next.js 构建输出
└── public/         # 静态资源
```

## 优势

- ✅ 单进程管理（无需 Node.js 运行时）
- ✅ 内存占用低（~80MB vs ~200MB）
- ✅ 启动快速（<2s vs ~5s）
- ✅ 简化部署（无需 nginx）
- ✅ 支持动态路由
- ✅ 前端可独立更新

## 构建

### Windows
```powershell
.\scripts\build-single-binary.ps1          # 开发版本
.\scripts\build-single-binary.ps1 -Release # 生产版本
```

### 构建产物

```
api/
├── target/
│   └── debug/tinyiothub.exe    # 二进制文件 (~33MB)
└── wwwroot/                     # 前端文件 (~15MB)
    ├── .next/
    └── public/
```

## 部署

### 单机部署

```bash
# 复制文件到服务器
scp -r api/target/release/tinyiothub user@server:/opt/tinyiothub/
scp -r api/wwwroot user@server:/opt/tinyiothub/
scp api/app_settings.toml user@server:/opt/tinyiothub/

# 启动
ssh user@server
cd /opt/tinyiothub
./tinyiothub
```

### 目录结构

```
/opt/tinyiothub/
├── tinyiothub          # 二进制
├── wwwroot/            # 前端文件
├── app_settings.toml   # 配置
├── tinyiothub.db       # 数据库
└── logs/               # 日志
```

## 配置

`app_settings.toml`:
```toml
[server]
host = "0.0.0.0"
port = 3002
```

访问: http://localhost:3002

## 更新

### 仅更新前端
```bash
cd web && pnpm build
scp -r .next user@server:/opt/tinyiothub/wwwroot/
ssh user@server "systemctl restart tinyiothub"
```

### 仅更新后端
```bash
cd api && cargo build --release
scp target/release/tinyiothub user@server:/opt/tinyiothub/
ssh user@server "systemctl restart tinyiothub"
```

## 性能

| 指标 | 分离部署 | 单进程方案 |
|------|---------|-----------|
| 内存 | ~200MB | ~80MB |
| 启动 | ~5s | <2s |
| 进程数 | 2+ | 1 |
| Node.js | 需要 | 不需要 |

## 技术细节

### 静态文件服务

使用 `tower-http` 的 `ServeDir`:

```rust
let serve_dir = ServeDir::new("wwwroot")
    .append_index_html_on_directories(true)
    .not_found_service(ServeFile::new("wwwroot/index.html"));

router.fallback_service(serve_dir)
```

### SPA 路由支持

所有未匹配的路由返回 `index.html`，由前端路由处理。

### API 路由优先级

```
/api/v1/*  → Axum API 路由
/*         → 静态文件 fallback
```

## 为什么不用完全静态导出？

项目使用了动态路由（如 `/device/[deviceId]`），Next.js 静态导出不支持。

## 为什么不用 standalone 模式？

Windows 下 standalone 模式有 symlink 权限问题，且仍需要 Node.js 运行时。

## 结论

单进程方案是最佳选择：
- 支持动态路由
- 无需 Node.js
- 部署简单
- 性能优异
