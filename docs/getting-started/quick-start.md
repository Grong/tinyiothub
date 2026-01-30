# 鸿蒙系统快速部署指南

## 🚀 5分钟快速开始

### 前置条件
- 已安装 DevEco Studio 和鸿蒙SDK
- 鸿蒙设备已启用开发者模式
- 设备与电脑在同一网络

### 一键部署
```bash
# 1. 设置环境变量
export OHOS_NDK_HOME=/path/to/ohos-sdk

# 2. 一键部署到设备
./deploy-to-harmonyos.sh 192.168.1.100 --test

# 3. 访问Web界面
# 浏览器打开: http://192.168.1.100:3002
```

---

## 📋 详细部署步骤

### 步骤1: 环境准备

#### 安装Rust工具链
```bash
# 安装Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 安装鸿蒙目标平台
rustup target add aarch64-unknown-linux-ohos
```

#### 设置鸿蒙SDK
```bash
# Linux/macOS
export OHOS_NDK_HOME=/path/to/DevEco-Studio/sdk/default/openharmony

# Windows
set OHOS_NDK_HOME=C:\Users\YourName\AppData\Local\Huawei\Sdk\openharmony
```

### 步骤2: 构建应用

#### 自动构建
```bash
# Linux/macOS
./build-harmonyos.sh aarch64

# Windows
.\build-harmonyos.bat aarch64
```

#### 手动构建
```bash
# 使用鸿蒙配置
cp Cargo-harmonyos.toml Cargo.toml
cp .cargo/config-harmonyos.toml .cargo/config.toml

# 编译
cargo build --target aarch64-unknown-linux-ohos --release --features harmonyos
```

### 步骤3: 设备连接

#### 连接鸿蒙设备
```bash
# 查看设备列表
hdc list targets

# 连接设备 (如果未自动连接)
hdc tconn 192.168.1.100:5555
```

### 步骤4: 部署应用

#### 使用自动部署脚本
```bash
# 基础部署
./deploy-to-harmonyos.sh 192.168.1.100

# 完整部署 (包含测试)
./deploy-to-harmonyos.sh 192.168.1.100 --backup --test

# 自定义端口
./deploy-to-harmonyos.sh 192.168.1.100 --port 8080
```

#### 手动部署
```bash
# 创建目录
hdc shell mkdir -p /data/local/tmp/iotedge

# 传输文件
hdc file send target/aarch64-unknown-linux-ohos/release/iotedge-rust-harmonyos /data/local/tmp/iotedge/
hdc file send app_settings_harmonyos.toml /data/local/tmp/iotedge/app_settings.toml

# 设置权限
hdc shell chmod +x /data/local/tmp/iotedge/iotedge-rust-harmonyos

# 启动应用
hdc shell "cd /data/local/tmp/iotedge && nohup ./iotedge-rust-harmonyos > app.log 2>&1 &"
```

---

## 🧪 功能测试

### 基础测试
```bash
# 设备IP (替换为实际IP)
DEVICE_IP="192.168.1.100"

# 1. 健康检查
curl http://$DEVICE_IP:3002/api/monitoring/health

# 2. 系统信息
curl http://$DEVICE_IP:3002/api/monitoring/metrics

# 3. Web界面
# 浏览器访问: http://192.168.1.100:3002
```

### API测试
```bash
# 设备管理
curl -X GET http://$DEVICE_IP:3002/api/devices
curl -X POST http://$DEVICE_IP:3002/api/devices \
  -H "Content-Type: application/json" \
  -d '{"name":"test-device","type":"sensor"}'

# MQTT测试
curl -X GET http://$DEVICE_IP:3002/api/monitoring/metrics
```

### 性能测试
```bash
# 并发测试
ab -n 100 -c 10 http://$DEVICE_IP:3002/api/monitoring/health

# 响应时间测试
curl -w "时间: %{time_total}s\n" -o /dev/null -s http://$DEVICE_IP:3002/api/devices
```

---

## 🔧 常见问题

### 构建问题

**Q: 找不到鸿蒙SDK**
```bash
# A: 检查环境变量
echo $OHOS_NDK_HOME
export OHOS_NDK_HOME=/correct/path/to/sdk
```

**Q: 编译失败**
```bash
# A: 清理重新构建
cargo clean
./build-harmonyos.sh aarch64
```

### 部署问题

**Q: 无法连接设备**
```bash
# A: 检查设备连接
hdc list targets
hdc kill-server && hdc start-server
```

**Q: 权限被拒绝**
```bash
# A: 设置正确权限
hdc shell chmod +x /data/local/tmp/iotedge/iotedge-rust-harmonyos
```

### 运行问题

**Q: 端口被占用**
```bash
# A: 检查端口使用
hdc shell netstat -tlnp | grep 3002

# 或修改端口
./deploy-to-harmonyos.sh 192.168.1.100 --port 8080
```

**Q: 应用无法启动**
```bash
# A: 查看日志
hdc shell "cd /data/local/tmp/iotedge && tail -20 app.log"
```

---

## 📊 监控和管理

### 查看应用状态
```bash
# 检查进程
hdc shell ps aux | grep iotedge-rust-harmonyos

# 查看日志
hdc shell "cd /data/local/tmp/iotedge && tail -f app.log"

# 检查端口
hdc shell netstat -tlnp | grep 3002
```

### 应用管理
```bash
# 重启应用
hdc shell pkill -f iotedge-rust-harmonyos
hdc shell "cd /data/local/tmp/iotedge && nohup ./iotedge-rust-harmonyos > app.log 2>&1 &"

# 停止应用
hdc shell pkill -f iotedge-rust-harmonyos

# 查看资源使用
hdc shell top -p $(hdc shell pgrep iotedge-rust-harmonyos)
```

---

## 🎯 生产部署建议

### 系统服务配置
```bash
# 创建systemd服务 (如果支持)
cat > /etc/systemd/system/tinyiothub.service << EOF
[Unit]
Description=TinyIoTHub
After=network.target

[Service]
Type=simple
User=system
WorkingDirectory=/data/local/tmp/iotedge
ExecStart=/data/local/tmp/iotedge/iotedge-rust-harmonyos
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
EOF

systemctl enable iotedge
systemctl start iotedge
```

### 安全配置
```toml
# app_settings_harmonyos.toml
[harmonyos]
# 生产环境配置
[logging]
level = "warn"

[server]
enable_cors = false
cors_origins = ["https://your-domain.com"]

# 性能优化
max_connections = 100
```

### 监控脚本
```bash
#!/bin/bash
# health-monitor.sh
DEVICE_IP="192.168.1.100"

while true; do
    if ! curl -f -s http://$DEVICE_IP:3002/api/monitoring/health > /dev/null; then
        echo "应用异常，重启中..."
        hdc shell pkill -f iotedge-rust-harmonyos
        sleep 5
        hdc shell "cd /data/local/tmp/iotedge && nohup ./iotedge-rust-harmonyos > app.log 2>&1 &"
    fi
    sleep 60
done
```

---

## 📞 获取帮助

### 日志分析
```bash
# 应用日志
hdc shell "cd /data/local/tmp/iotedge && tail -100 app.log"

# 系统日志
hdc shell dmesg | grep iotedge

# 详细调试
RUST_LOG=debug ./iotedge-rust-harmonyos
```

### 性能分析
```bash
# 资源使用
hdc shell "top -p \$(pgrep iotedge-rust-harmonyos)"

# 网络连接
hdc shell "netstat -an | grep 3002"

# 磁盘使用
hdc shell "df -h /data/local/tmp/iotedge"
```

### 技术支持
- 查看完整文档: [鸿蒙系统部署指南](../deployment/harmonyos-deployment.md)
- 构建问题排查: [构建指南](../development/setup.md)
- 使用构建脚本: `./build-harmonyos.sh`
- 检查项目Issues: GitHub项目页面

---

## ✅ 部署检查清单

- [ ] 鸿蒙SDK已安装并配置环境变量
- [ ] Rust工具链和目标平台已安装
- [ ] 设备已启用开发者模式和USB调试
- [ ] 设备与电脑网络连通
- [ ] 应用构建成功
- [ ] 文件部署到设备
- [ ] 应用启动正常
- [ ] Web界面可访问
- [ ] API接口响应正常
- [ ] 日志输出正常

完成以上检查后，你的IoT边缘网关就成功运行在鸿蒙系统上了！🎉