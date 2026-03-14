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
./scripts/build-single-binary.sh --release
```

## 运行

```bash
cd api
.\target\release\tinyiothub.exe  # Windows
./target/release/tinyiothub      # Linux/macOS
```

## 配置

单进程模式下，前端静态文件嵌入到二进制中：

```toml
[server]
host = "0.0.0.0"
port = 3002

[server.static]
enabled = true
path = "static"
```

## 访问

所有服务通过单一端口访问：

- Web 界面: http://localhost:3002/
- API: http://localhost:3002/api/v1/
- 健康检查: http://localhost:3002/api/v1/system/health

## 性能对比

| 指标 | 分离部署 | 单进程部署 |
|------|----------|------------|
| 内存占用 | ~200MB | ~80MB |
| 启动时间 | ~5s | <2s |
| 进程数 | 2+ | 1 |
