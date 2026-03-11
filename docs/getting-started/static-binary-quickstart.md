# 静态单二进制快速入门

## 5 分钟快速部署

### 步骤 1: 构建

```bash
# Windows
.\scripts\build-static.ps1 -Release

# Linux/macOS  
./scripts/build-static.sh --release
```

### 步骤 2: 配置

```bash
cd api
cp app_settings.example.toml app_settings.toml
# 编辑配置文件（可选）
```

### 步骤 3: 运行

```bash
# Windows
.\target\release\tinyiothub.exe

# Linux/macOS
./target/release/tinyiothub
```

### 步骤 4: 访问

打开浏览器访问: http://localhost:3002

默认账号:
- 用户名: `admin`
- 密码: `admin123`

## 部署到生产环境

### 单机部署

```bash
# 1. 复制文件到服务器
scp api/target/release/tinyiothub user@server:/opt/tinyiothub/
scp api/app_settings.toml user@server:/opt/tinyiothub/

# 2. SSH 登录服务器
ssh user@server

# 3. 启动服务
cd /opt/tinyiothub
./tinyiothub
```

### 使用 systemd（推荐）

创建服务文件 `/etc/systemd/system/tinyiothub.service`:

```ini
[Unit]
Description=TinyIoTHub Service
After=network.target

[Service]
Type=simple
User=tinyiot
WorkingDirectory=/opt/tinyiothub
ExecStart=/opt/tinyiothub/tinyiothub
Restart=always
RestartSec=10

[Install]
WantedBy=multi-tier.target
```

启动服务:
```bash
sudo systemctl daemon-reload
sudo systemctl enable tinyiothub
sudo systemctl start tinyiothub
sudo systemctl status tinyiothub
```

### Docker 部署

创建 `Dockerfile`:

```dockerfile
FROM scratch
COPY tinyiothub /
COPY app_settings.toml /
EXPOSE 3002
CMD ["/tinyiothub"]
```

构建和运行:
```bash
docker build -t tinyiothub .
docker run -d -p 3002:3002 --name tinyiothub tinyiothub
```

## 常见问题

### Q: 如何修改端口？

编辑 `app_settings.toml`:
```toml
[server]
host = "0.0.0.0"
port = 8080  # 修改为你的端口
```

### Q: 如何配置 HTTPS？

使用 nginx 反向代理:
```nginx
server {
    listen 443 ssl;
    server_name your-domain.com;
    
    ssl_certificate /path/to/cert.pem;
    ssl_certificate_key /path/to/key.pem;
    
    location / {
        proxy_pass http://localhost:3002;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }
}
```

### Q: 如何查看日志？

日志位置: `api/logs/app.log`

实时查看:
```bash
tail -f api/logs/app.log
```

### Q: 如何备份数据？

备份数据库文件:
```bash
cp api/tinyiothub.db api/tinyiothub.db.backup
```

### Q: 如何更新版本？

```bash
# 1. 停止服务
sudo systemctl stop tinyiothub

# 2. 备份
cp tinyiothub tinyiothub.old
cp tinyiothub.db tinyiothub.db.backup

# 3. 替换新版本
cp /path/to/new/tinyiothub .

# 4. 启动服务
sudo systemctl start tinyiothub
```

## 性能调优

### 内存优化

编辑 `app_settings.toml`:
```toml
[database]
max_connections = 5  # 减少连接数

[cache]
max_size = 100  # 减少缓存大小
```

### 并发优化

设置环境变量:
```bash
# 增加工作线程
export TOKIO_WORKER_THREADS=4
./tinyiothub
```

## 监控和维护

### 健康检查

```bash
curl http://localhost:3002/api/health
```

### 系统信息

```bash
curl http://localhost:3002/api/v1/system/info
```

### 资源监控

```bash
# CPU 和内存
ps aux | grep tinyiothub

# 网络连接
netstat -an | grep 3002
```

## 下一步

- [配置指南](../configuration/README.md)
- [API 文档](../api/README.md)
- [驱动开发](../driver-development.md)
- [故障排查](../troubleshooting.md)
