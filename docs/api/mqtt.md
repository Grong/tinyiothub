# MQTT 协议

TinyIoTHub 通过 MQTT 协议进行消息通信，支持设备数据上报、命令下发等功能。

## 连接地址

### 公共 MQTT Broker

生产环境使用 `mqtt.tinyiothub.com`：

| 连接方式 | 地址 | 端口 | 说明 |
|----------|------|------|------|
| **MQTT over TLS** | `mqtt.tinyiothub.com` | 8883 | 推荐，最安全 |
| **MQTT over WebSocket** | `wss://mqtt.tinyiothub.com/mqtt` | 443 | 浏览器可用 |
| Raw MQTT | `mqtt.tinyiothub.com` | 1883 | 仅开发环境 |

**默认凭据：**
- 用户名: `admin`
- 密码: `admin123`

> 首次部署后请务必修改密码：
> ```bash
> docker exec tinyiothub-mqtt mosquitto_passwd -c /mosquitto/config/pwfile admin your_password
> ```

### 客户端连接示例

**MQTTX (桌面客户端):**
```
Broker: mqtt.tinyiothub.com
Port: 8883 (TLS)
Username: admin
Password: admin123
```

**JavaScript (WebSocket):**
```javascript
const client = mqtt.connect('wss://mqtt.tinyiothub.com/mqtt', {
  username: 'admin',
  password: 'admin123'
});

client.on('connect', () => {
  console.log('Connected to MQTT');
  client.subscribe('tinyiothub/+/data');
});

client.on('message', (topic, message) => {
  console.log('Received:', topic, message.toString());
});
```

**Python:**
```python
import paho.mqtt.client as mqtt

client = mqtt.Client()
client.username_pw_set('admin', 'admin123')
client.tls_set()
client.connect('mqtt.tinyiothub.com', 8883)

client.subscribe('tinyiothub/+/data')
client.loop_forever()
```

## MQTT 配置

### 配置参数

```toml
[mqtt.primary]
host = "192.168.1.124"
port = 1883
username = "admin"
password = "password"
qos = 1

[mqtt.backup]
enabled = true
host = "192.168.1.125"
port = 1883
```

## 主题结构

### 基础主题

```
tinyiothub/{gateway_sn}/{topic}
```

### 主题类型

| 主题 | 说明 | QoS |
|------|------|-----|
| heartbeat | 心跳 | 1 |
| device_register | 设备注册 | 1 |
| data | 数据上报 | 1 |
| alarm | 告警 | 1 |
| command | 命令下发 | 2 |
| device_command | 设备命令 | 2 |

## 消息格式

### 心跳消息

```json
{
  "sn": "GW001",
  "timestamp": 1704067200,
  "status": "online",
  "uptime": 3600,
  "cpu": 25.5,
  "memory": 45.2
}
```

### 数据上报

```json
{
  "device_sn": "DEV001",
  "timestamp": 1704067200,
  "data": {
    "temperature": 25.5,
    "humidity": 60.2
  }
}
```

### 告警消息

```json
{
  "device_sn": "DEV001",
  "timestamp": 1704067200,
  "level": "warning",
  "type": "threshold",
  "message": "温度超过阈值",
  "value": 35.0,
  "threshold": 30.0
}
```

### 命令下发

```json
{
  "command_id": "cmd_001",
  "device_sn": "DEV001",
  "timestamp": 1704067200,
  "action": "set",
  "params": {
    "target": 25.0
  }
}
```

## 主备通道

系统支持 MQTT 主备双通道：

1. **主通道**: 优先使用，主通道故障时自动切换
2. **备通道**: 备用通道，确保消息可靠送达

## 订阅主题

### 网关注册

```
tinyiothub/{sn}/command
```

### 设备命令

```
tinyiothub/{sn}/device_command/{device_sn}
```
