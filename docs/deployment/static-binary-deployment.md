# 静态单二进制部署方案

## 概述

将 Next.js 前端构建为静态文件并嵌入 Rust 后端，生成单个可执行文件，简化部署流程。

## 架构变更

### 之前（开发模式）
```
┌─────────────┐      ┌─────────────┐
│  Next.js    │      │   Axum      │
│  (Node.js)  │─────▶│  (Rust)     │
│  Port 3001  │      │  Port 3002  │
└─────────────┘      └─────────────┘
```

### 之后（生产模式）
```
┌──────────────────────────────┐
│     单个二进制文件            │
│  ┌────────────────────────┐  │
│  │  嵌入的静态文件         │  │
│  │  (HTML/CSS/JS)         │  │
│  └────────────────────────┘  │
│  ┌────────────────────────┐  │
│  │  Axum Web Server       │  │
│  │  - API 路由 (/api/*)   │  │
│  │  - 静态文件服务 (/)    │  │
│  └────────────────────────┘  │
└──────────────────────────────┘
```

## 技术实现

### 1. 前端静态导出

**配置文件**: `web/next.config.static.js`

关键配置：
- `output: 'export'` - 启用静态导出
- `images.unoptimized: true` - 禁用图片优化
- `trailingSlash: true` - 支持静态服务器

### 2. 静态文件嵌入

**实现文件**: `api/src/infrastructure/static_files.rs`

使用 `include_dir!` 宏在编译时将静态文件嵌入二进制：

```rust
static STATIC_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../web/out");
```

### 3. 路由配置

**主路由**: `api/src/main.rs`

```rust
Router::new()
    .nest("/api", api_router)           // API 路由
    .fallback(serve_static_file)        // 静态文件 fallback
```

## 构建流程

### Windows
```powershell
# 开发版本
.\scripts\build-static.ps1

# 生产版本（优化）
.\scripts\build-static.ps1 -Release

# 交叉编译（ARM）
.\scripts\build-static.ps1 -Target armv7-unknown-linux-gnueabihf -Release
```

### Linux/macOS
```bash
# 开发版本
./scripts/build-static.sh

# 生产版本
./scripts/build-static.sh --release

# 交叉编译
./scripts/build-static.sh --target armv7-unknown-linux-gnueabihf --release
```

### 构建步骤

1. **构建前端** (`pnpm build:static`)
   - 生成静态 HTML/CSS/JS 到 `web/out/`
   - 所有资源预渲染和优化

2. **复制静态文件**
   - 将 `web/out/` 复制到 `api/web_out/`
   - 供 `include_dir!` 宏读取

3. **构建后端**
   - Cargo 编译时嵌入静态文件
   - 生成单个可执行文件

## 部署方式

### 单文件部署

```bash
# 1. 复制二进制文件到目标服务器
scp api/target/release/tinyiothub user@server:/opt/tinyiothub/

# 2. 复制配置文件
scp api/app_settings.toml user@server:/opt/tinyiothub/

# 3. 启动服务
ssh user@server
cd /opt/tinyiothub
./tinyiothub
```

### 优势

1. **简化部署**
   - 无需 Node.js 运行时
   - 无需 npm/pnpm 依赖
   - 单个文件包含所有内容

2. **减少依赖**
   - 不依赖外部 web 服务器
   - 不需要反向代理配置
   - 减少运维复杂度

3. **性能优化**
   - 静态文件直接从内存服务
   - 无需文件系统 I/O
   - 更快的响应速度

4. **安全性**
   - 减少攻击面
   - 文件无法被外部修改
   - 统一的版本管理

## 开发模式

开发时仍然使用分离模式：

```bash
# 终端 1: 启动前端开发服务器
cd web
pnpm dev

# 终端 2: 启动后端
cd api
cargo run
```

前端代理配置（`next.config.js`）：
```javascript
async rewrites() {
  if (isDev) {
    return [{
      source: '/api/v1/:path*',
      destination: 'http://localhost:3002/api/v1/:path*',
    }]
  }
  return []
}
```

## 配置说明

### 环境变量

生产环境不需要 `NEXT_PUBLIC_API_PREFIX`，API 请求直接发送到同域：

```bash
# 开发模式
NEXT_PUBLIC_API_PREFIX=http://localhost:3002

# 生产模式（静态导出）
# 不需要设置，使用相对路径 /api/v1
```

### API 客户端

前端 API 客户端自动处理：

```typescript
// lib/api-client.ts
const baseURL = process.env.NEXT_PUBLIC_API_PREFIX || '/api/v1'
```

## 文件大小优化

### 当前大小（估算）

- 前端静态文件: ~5-10 MB
- Rust 二进制: ~20-30 MB
- 总计: ~25-40 MB

### 优化建议

1. **前端优化**
   - 启用代码分割
   - 压缩图片资源
   - 移除未使用的依赖

2. **后端优化**
   ```toml
   [profile.release]
   opt-level = "z"     # 优化大小
   lto = true          # 链接时优化
   codegen-units = 1   # 单个代码生成单元
   strip = true        # 移除调试符号
   ```

3. **压缩**
   ```bash
   # 使用 UPX 压缩
   upx --best --lzma tinyiothub
   ```

## 故障排查

### 静态文件未嵌入

**症状**: 运行时提示 "Using external static files"

**原因**: 构建时 `web/out` 目录不存在

**解决**:
```bash
# 确保先构建前端
cd web && pnpm build:static && cd ..
# 再构建后端
cd api && cargo build --release
```

### 404 错误

**症状**: 访问前端路由返回 404

**原因**: SPA fallback 未正确配置

**解决**: 检查 `static_files.rs` 中的 fallback 逻辑

### API 请求失败

**症状**: 前端无法调用 API

**原因**: CORS 配置或路由冲突

**解决**: 检查 `create_app_router` 中的路由顺序

## 性能测试

### 启动时间

```bash
time ./tinyiothub
```

预期: < 1 秒

### 内存占用

```bash
ps aux | grep tinyiothub
```

预期: 50-100 MB

### 响应时间

```bash
# API 请求
curl -w "@curl-format.txt" http://localhost:3002/api/v1/health

# 静态文件
curl -w "@curl-format.txt" http://localhost:3002/
```

预期: < 50ms

## 未来改进

1. **增量构建**
   - 仅在前端变更时重新构建
   - 缓存静态文件

2. **多环境支持**
   - 开发/测试/生产配置
   - 环境特定的优化

3. **自动化部署**
   - CI/CD 集成
   - 自动版本管理

4. **监控集成**
   - 嵌入式指标收集
   - 健康检查端点
