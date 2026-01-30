# TinyIoTHub v1.0.0 - 部署指南

## 目录

- [系统要求](#系统要求)
- [构建发布包](#构建发布包)
- [部署步骤](#部署步骤)
- [生产环境配置](#生产环境配置)
- [反向代理配置](#反向代理配置)
- [系统服务配置](#系统服务配置)
- [监控和维护](#监控和维护)
- [故障排查](#故障排查)

## 系统要求

### 最低配置
- CPU: 2 核
- 内存: 2GB
- 磁盘: 10GB 可用空间
- 操作系统: Linux (Ubuntu 20.04+, CentOS 7+) / macOS / Windows Server

### 推荐配置
- CPU: 4 核
- 内存: 4GB
- 磁盘: 20GB SSD
- 操作系统: Ubuntu 22.04 LTS

### 软件依赖
- 无需额外依赖（所有依赖已静态编译）
- 可选: Nginx/Apache（用于反向代理）
- 可选: Systemd（用于服务管理）

## 构建发布包

### Linux/macOS

```bash
# 克隆代码
git clone <repository-url>
cd tinyiothub

# 运行构建脚本
./scripts/build-release.sh
```

### Windows

```cmd
REM 克隆代码
git clone <repository-url>
cd tinyiothub

REM 运行构建脚本
scripts\build-release.bat
```

构建完成后，发布包位于 `dist/tinyiothub-v1.0.0.tar.gz` (Linux/macOS) 或 `dist/tinyiothub-v1.0.0.zip` (Windows)。

## 部署步骤

### 1. 上传发布包到服务器

```bash
# 使用 scp 上传
scp dist/tinyiothub-v1.0.0.tar.gz user@server:/opt/

# 或使用 rsync
rsync -avz dist/tinyiothub-v1.0.0.tar.gz user@server:/opt/
```

### 2. 解压发布包

```bash
cd /opt
tar -xzf tinyiothub-v1.0.0.tar.gz
cd tinyiothub-v1.0.0
```

### 3. 配置应用

```bash
# 编辑配置文件
nano app_settings.toml
```

关键配置项：

```toml
[server]
host = "0.0.0.0"  # 监听所有网络接口
port = 3002       # 后端端口

[database]
url = "sqlite:data/tinyiothub.db"  # 数据库路径

[mqtt]
broker_address = "mqtt://localhost:1883"  # MQTT 代理地址
username = "your_username"
password = "your_password"

[security]
jwt_secret = "CHANGE_THIS_IN_PRODUCTION"  # 必须修改！
jwt_expiration_hours = 24

[logging]
level = "info"  # 生产环境使用 info 或 warn
```

### 4. 设置环境变量（可选）

```bash
# 编辑 .env 文件
nano .env
```

```env
RUST_LOG=info
JWT_SECRET=your-super-secret-key-change-this
DATABASE_URL=sqlite:data/tinyiothub.db
```

### 5. 初始化数据库

数据库会在首次启动时自动初始化。

### 6. 启动应用

```bash
# 使用启动脚本
./start.sh

# 或手动启动
./iot-edge &
cd web && PORT=3001 node server.js &
```

### 7. 验证部署

```bash
# 检查后端
curl http://localhost:3002/api/v1/health

# 检查前端
curl http://localhost:3001
```

## 生产环境配置

### 安全配置

1. **修改默认密码**
   - 首次登录后立即修改 admin 密码
   - 默认用户名: `admin`
   - 默认密码: `admin123`

2. **JWT 密钥**
   ```toml
   [security]
   jwt_secret = "使用强随机字符串，至少32字符"
   ```

3. **CORS 配置**
   ```toml
   [server]
   cors_origins = ["https://yourdomain.com"]  # 限制允许的来源
   ```

4. **文件权限**
   ```bash
   chmod 600 app_settings.toml  # 保护配置文件
   chmod 600 .env               # 保护环境变量
   chmod 755 iot-edge           # 可执行权限
   ```

### 性能优化

1. **日志配置**
   ```toml
   [logging]
   level = "warn"  # 生产环境减少日志量
   max_size_mb = 100
   max_backups = 10
   ```

2. **数据库优化**
   ```bash
   # 定期优化数据库
   sqlite3 data/tinyiothub.db "VACUUM;"
   sqlite3 data/tinyiothub.db "ANALYZE;"
   ```

3. **资源限制**
   ```bash
   # 在 systemd 服务中设置
   LimitNOFILE=65536
   LimitNPROC=4096
   ```

## 反向代理配置

### Nginx 配置

```nginx
# /etc/nginx/sites-available/tinyiothub

upstream backend {
    server 127.0.0.1:3002;
}

upstream frontend {
    server 127.0.0.1:3001;
}

server {
    listen 80;
    server_name yourdomain.com;

    # 重定向到 HTTPS
    return 301 https://$server_name$request_uri;
}

server {
    listen 443 ssl http2;
    server_name yourdomain.com;

    # SSL 证书
    ssl_certificate /etc/ssl/certs/yourdomain.com.crt;
    ssl_certificate_key /etc/ssl/private/yourdomain.com.key;

    # SSL 配置
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_ciphers HIGH:!aNULL:!MD5;
    ssl_prefer_server_ciphers on;

    # 前端
    location / {
        proxy_pass http://frontend;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection 'upgrade';
        proxy_set_header Host $host;
        proxy_cache_bypass $http_upgrade;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    # 后端 API
    location /api/ {
        proxy_pass http://backend;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        
        # WebSocket 支持
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        
        # 超时设置
        proxy_connect_timeout 60s;
        proxy_send_timeout 60s;
        proxy_read_timeout 60s;
    }

    # 静态文件缓存
    location ~* \.(jpg|jpeg|png|gif|ico|css|js)$ {
        proxy_pass http://frontend;
        expires 1y;
        add_header Cache-Control "public, immutable";
    }

    # 日志
    access_log /var/log/nginx/iot-edge-access.log;
    error_log /var/log/nginx/iot-edge-error.log;
}
```

启用配置：

```bash
sudo ln -s /etc/nginx/sites-available/tinyiothub /etc/nginx/sites-enabled/
sudo nginx -t
sudo systemctl reload nginx
```

### Apache 配置

```apache
# /etc/apache2/sites-available/tinyiothub.conf

<VirtualHost *:80>
    ServerName yourdomain.com
    Redirect permanent / https://yourdomain.com/
</VirtualHost>

<VirtualHost *:443>
    ServerName yourdomain.com

    SSLEngine on
    SSLCertificateFile /etc/ssl/certs/yourdomain.com.crt
    SSLCertificateKeyFile /etc/ssl/private/yourdomain.com.key

    # 前端
    ProxyPass / http://127.0.0.1:3001/
    ProxyPassReverse / http://127.0.0.1:3001/

    # 后端 API
    ProxyPass /api/ http://127.0.0.1:3002/api/
    ProxyPassReverse /api/ http://127.0.0.1:3002/api/

    # WebSocket 支持
    RewriteEngine On
    RewriteCond %{HTTP:Upgrade} =websocket [NC]
    RewriteRule /(.*)           ws://127.0.0.1:3002/$1 [P,L]

    ErrorLog ${APACHE_LOG_DIR}/iot-edge-error.log
    CustomLog ${APACHE_LOG_DIR}/iot-edge-access.log combined
</VirtualHost>
```

## 系统服务配置

### Systemd 服务（推荐）

#### 后端服务

```ini
# /etc/systemd/system/iot-edge-backend.service

[Unit]
Description=TinyIoTHub Backend
After=network.target

[Service]
Type=simple
User=iotedge
Group=iotedge
WorkingDirectory=/opt/tinyiothub-v1.0.0
Environment="RUST_LOG=info"
ExecStart=/opt/tinyiothub-v1.0.0/tinyiothub
Restart=always
RestartSec=10

# 安全设置
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/opt/tinyiothub-v1.0.0/data /opt/tinyiothub-v1.0.0/logs

# 资源限制
LimitNOFILE=65536
LimitNPROC=4096

[Install]
WantedBy=multi-user.target
```

#### 前端服务

```ini
# /etc/systemd/system/iot-edge-frontend.service

[Unit]
Description=TinyIoTHub Frontend
After=network.target tinyiothub-backend.service

[Service]
Type=simple
User=iotedge
Group=iotedge
WorkingDirectory=/opt/tinyiothub-v1.0.0/web
Environment="PORT=3001"
Environment="NODE_ENV=production"
ExecStart=/usr/bin/node server.js
Restart=always
RestartSec=10

# 安全设置
NoNewPrivileges=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

#### 启用服务

```bash
# 创建用户
sudo useradd -r -s /bin/false iotedge

# 设置权限
sudo chown -R iotedge:iotedge /opt/tinyiothub-v1.0.0

# 重载 systemd
sudo systemctl daemon-reload

# 启用并启动服务
sudo systemctl enable iot-edge-backend
sudo systemctl enable iot-edge-frontend
sudo systemctl start iot-edge-backend
sudo systemctl start iot-edge-frontend

# 检查状态
sudo systemctl status iot-edge-backend
sudo systemctl status iot-edge-frontend
```

## 监控和维护

### 日志管理

```bash
# 查看应用日志
tail -f /opt/tinyiothub-v1.0.0/logs/app.log

# 查看系统日志
sudo journalctl -u iot-edge-backend -f
sudo journalctl -u iot-edge-frontend -f

# 日志轮转配置
# /etc/logrotate.d/tinyiothub
/opt/tinyiothub-v1.0.0/logs/*.log {
    daily
    rotate 30
    compress
    delaycompress
    notifempty
    create 0640 iotedge iotedge
    sharedscripts
    postrotate
        systemctl reload iot-edge-backend > /dev/null 2>&1 || true
    endscript
}
```

### 数据库备份

```bash
# 创建备份脚本
# /opt/tinyiothub-v1.0.0/backup.sh

#!/bin/bash
BACKUP_DIR="/opt/backups/iot-edge"
DATE=$(date +%Y%m%d_%H%M%S)

mkdir -p $BACKUP_DIR

# 备份数据库
cp /opt/tinyiothub-v1.0.0/data/tinyiothub.db \
   $BACKUP_DIR/tinyiothub-$DATE.db

# 压缩备份
gzip $BACKUP_DIR/iot-edge-$DATE.db

# 删除30天前的备份
find $BACKUP_DIR -name "*.db.gz" -mtime +30 -delete

echo "Backup completed: iot-edge-$DATE.db.gz"
```

```bash
# 添加到 crontab
chmod +x /opt/tinyiothub-v1.0.0/backup.sh
crontab -e

# 每天凌晨2点备份
0 2 * * * /opt/tinyiothub-v1.0.0/backup.sh
```

### 健康检查

```bash
# 创建健康检查脚本
# /opt/tinyiothub-v1.0.0/healthcheck.sh

#!/bin/bash

# 检查后端
BACKEND_STATUS=$(curl -s -o /dev/null -w "%{http_code}" http://localhost:3002/api/v1/health)
if [ "$BACKEND_STATUS" != "200" ]; then
    echo "Backend unhealthy, restarting..."
    systemctl restart iot-edge-backend
fi

# 检查前端
FRONTEND_STATUS=$(curl -s -o /dev/null -w "%{http_code}" http://localhost:3001)
if [ "$FRONTEND_STATUS" != "200" ]; then
    echo "Frontend unhealthy, restarting..."
    systemctl restart iot-edge-frontend
fi
```

## 故障排查

### 常见问题

#### 1. 端口被占用

```bash
# 查找占用端口的进程
sudo lsof -i :3002
sudo lsof -i :3001

# 修改配置文件中的端口
nano app_settings.toml
```

#### 2. 数据库锁定

```bash
# 检查数据库连接
sqlite3 data/tinyiothub.db ".timeout 5000"

# 如果数据库损坏，从备份恢复
cp /opt/backups/tinyiothub/tinyiothub-latest.db.gz .
gunzip tinyiothub-latest.db.gz
mv tinyiothub-latest.db data/tinyiothub.db
```

#### 3. 内存不足

```bash
# 检查内存使用
free -h
ps aux --sort=-%mem | head

# 增加 swap
sudo fallocate -l 2G /swapfile
sudo chmod 600 /swapfile
sudo mkswap /swapfile
sudo swapon /swapfile
```

#### 4. 权限问题

```bash
# 修复文件权限
sudo chown -R iotedge:iotedge /opt/tinyiothub-v1.0.0
sudo chmod 755 /opt/tinyiothub-v1.0.0/tinyiothub
sudo chmod 644 /opt/tinyiothub-v1.0.0/data/tinyiothub.db
```

### 性能问题

```bash
# 检查系统资源
top
htop
iotop

# 检查网络连接
netstat -tunlp | grep -E '3001|3002'

# 检查磁盘 I/O
iostat -x 1

# 数据库性能分析
sqlite3 data/tinyiothub.db "EXPLAIN QUERY PLAN SELECT * FROM devices;"
```

### 日志分析

```bash
# 查找错误
grep -i error logs/app.log | tail -50

# 统计错误类型
grep -i error logs/app.log | awk '{print $5}' | sort | uniq -c | sort -rn

# 查看最近的警告
grep -i warn logs/app.log | tail -20
```

## 升级指南

### 升级步骤

1. **备份当前版本**
   ```bash
   cd /opt
   tar -czf tinyiothub-backup-$(date +%Y%m%d).tar.gz tinyiothub-v1.0.0/
   ```

2. **停止服务**
   ```bash
   sudo systemctl stop iot-edge-frontend
   sudo systemctl stop iot-edge-backend
   ```

3. **部署新版本**
   ```bash
   tar -xzf tinyiothub-v1.1.0.tar.gz
   ```

4. **迁移配置和数据**
   ```bash
   cp tinyiothub-v1.0.0/app_settings.toml tinyiothub-v1.1.0/
   cp -r tinyiothub-v1.0.0/data/* tinyiothub-v1.1.0/data/
   ```

5. **更新服务配置**
   ```bash
   sudo nano /etc/systemd/system/iot-edge-backend.service
   # 更新 WorkingDirectory 和 ExecStart 路径
   sudo systemctl daemon-reload
   ```

6. **启动新版本**
   ```bash
   sudo systemctl start iot-edge-backend
   sudo systemctl start iot-edge-frontend
   ```

7. **验证升级**
   ```bash
   curl http://localhost:3002/api/v1/health
   sudo systemctl status iot-edge-backend
   ```

## 安全最佳实践

1. **定期更新**
   - 及时应用安全补丁
   - 订阅安全公告

2. **访问控制**
   - 使用防火墙限制访问
   - 配置 IP 白名单
   - 启用 HTTPS

3. **密码策略**
   - 强制使用强密码
   - 定期更换密码
   - 启用双因素认证（如果支持）

4. **审计日志**
   - 启用详细的审计日志
   - 定期审查日志
   - 设置异常告警

5. **数据保护**
   - 加密敏感数据
   - 定期备份
   - 测试恢复流程

## 支持和帮助

- 查看 CHANGELOG.md 了解版本变更
- 查看 README.md 了解功能说明
- 检查日志文件排查问题
- 联系技术支持团队
