# 单进程部署

单进程部署将前端和后端打包成一个可执行文件，简化部署流程。

## 优势

- ✅ 单进程部署，无需 Node.js
- ✅ 内存占用低（~80MB vs ~200MB）
- ✅ 启动快速（<2s vs ~5s）
- ✅ 支持动态路由

## 构建

### Windows

```powershell
.\scripts\build-single-binary.ps1 -Release
```

### Linux/macOS

```bash
# 构建前端（输出到 dist/ui）
cd web && pnpm run build && cd ..

# 构建后端（静态文件通过 server.static_files_dir 指向前端产物）
cargo build --release --bin tinyiothub-cloud
```

## 运行

```bash
.\target\release\tinyiothub-cloud.exe  # Windows
./target/release/tinyiothub-cloud      # Linux/macOS
```

启动时会自动执行数据库迁移（`crates/db/migrations/`）：迁移前自动 VACUUM 备份到 `data/backups/`，
并在启动前做 `foreign_key_check`；迁移失败会中止启动，请按提示从备份恢复。

## 配置

单进程模式下，前端静态文件嵌入到二进制中：

```toml
[server]
host = "0.0.0.0"
port = 3002
static_files_dir = "wwwroot"
```

## 访问

所有服务通过单一端口访问：

- Web 界面: http://localhost:3002/
- API: http://localhost:3002/api/v1/
- 健康检查: http://localhost:3002/api/health

## 性能对比

| 指标 | 分离部署 | 单进程部署 |
|------|----------|------------|
| 内存占用 | ~200MB | ~80MB |
| 启动时间 | ~5s | <2s |
| 进程数 | 2+ | 1 |
