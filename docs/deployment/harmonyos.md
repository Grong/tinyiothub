# 鸿蒙系统部署

TinyIoTHub 针对鸿蒙系统进行了深度优化，可直接部署在鸿蒙设备上。

## 支持的鸿蒙版本

- HarmonyOS 3.0+
- OpenHarmony 3.2+

## 构建

### Linux/macOS

```bash
./build-harmonyos.sh
```

### Windows

```powershell
.\build-harmonyos.bat
```

## 部署

### 1. 复制二进制

将构建好的二进制文件复制到鸿蒙设备：

```bash
scp target/release/tinyiothub root@<device_ip>:/usr/local/bin/
```

### 2. 创建数据目录

```bash
ssh root@<device_ip> mkdir -p /data/tinyiothub
```

### 3. 配置

编辑配置文件 `app_settings.toml`：

```toml
[server]
host = "0.0.0.0"
port = 3002

[database]
url = "/data/tinyiothub/tinyiothub.db"

[hardware]
enabled = true
```

## 硬件抽象层

TinyIoTHub 提供鸿蒙硬件抽象层，支持：

- GPIO 控制
- 串口通信
- 网络接口
- 传感器数据采集

## 性能优化

针对鸿蒙设备进行了以下优化：

- 内存池优化
- 减少系统调用
- 低功耗模式支持

## 故障排查

### 查看日志

```bash
journalctl -u tinyiothub -f
```

### 性能监控

```bash
curl http://localhost:3002/api/v1/monitoring/metrics
```

## 了解更多

- [鸿蒙部署指南详细版](../HARMONYOS_DEPLOYMENT_GUIDE.md)
- [快速开始鸿蒙版](../QUICK_START_HARMONYOS.md)
