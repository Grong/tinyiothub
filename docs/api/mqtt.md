# MQTT 协议

TinyIoTHub 通过 MQTT 协议实现平台与边缘网关之间的双向通信，包括网关配对注册、数据上报、指令下发等功能。

## 连接信息

### 公共 MQTT Broker

生产环境使用 `mqtt.tinyiothub.com`：

| 连接方式 | 地址 | 端口 | 说明 |
|----------|------|------|------|
| MQTT over TLS | `mqtt.tinyiothub.com` | 8883 | 推荐，最安全 |
| MQTT over WebSocket | `wss://mqtt.tinyiothub.com/mqtt` | 443 | 浏览器可用 |
| Raw MQTT | `mqtt.tinyiothub.com` | 1883 | 仅开发环境 |

### 认证方式

平台通过环境变量注入 MQTT 凭据，不硬编码：

```bash
# Docker Compose 环境变量
TINYIOTHUB__MQTT__PRIMARY__USERNAME=${MQTT_USERNAME}
TINYIOTHUB__MQTT__PRIMARY__PASSWORD=${MQTT_PASSWORD}
```

> 所有敏感凭据通过环境变量设置，配置文件中不包含明文密码。

## MQTT 配置

配置文件 `app_settings.toml` 中的 MQTT 相关配置：

### 主通道 (primary)

```toml
[mqtt.primary]
host = "mqtt.tinyiothub.com"
port = 1883
# username/password 通过环境变量设置
# username = ""
# password = ""
use_tls = false
connect_timeout_secs = 30
keep_alive_secs = 60
```

### 备用通道 (secondary，可选)

```toml
[mqtt.secondary]
host = "175.178.49.5"
port = 9990
# username/password 通过环境变量设置
# username = ""
# password = ""
use_tls = false
connect_timeout_secs = 30
keep_alive_secs = 60
```

### 客户端配置 (client)

```toml
[mqtt.client]
client_id = "iot-edge"
clean_session = true
auto_reconnect = true
max_reconnect_attempts = 5
reconnect_delay_secs = 5
message_queue_size = 1000
```

### 主题配置 (topics)

```toml
[mqtt.topics]
prefix = "gateway"
heartbeat = "heartbeat"
device_registration = "device_regist"
command = "command"
data_upload = "data"
alarm = "alarm"
publish_qos = 1
subscribe_qos = 1
```

### 配置结构体参考

| 配置节 | Rust 结构体 | 说明 |
|--------|-------------|------|
| `[mqtt]` | `MqttConfig` | 顶层 MQTT 配置 |
| `[mqtt.primary]` | `MqttBrokerConfig` | 主 MQTT broker，必填 |
| `[mqtt.secondary]` | `Option<MqttBrokerConfig>` | 备用 broker，可选 |
| `[mqtt.client]` | `MqttClientConfig` | 客户端行为配置 |
| `[mqtt.topics]` | `MqttTopicConfig` | 主题名称配置 |

## Topic 架构

### 完整 Topic 树

```
tinyiothub/
├── pairing/
│   ├── announce                       # 网关→平台：宣告配对码 (QoS 1)
│   └── {code}/response                # 平台→网关：凭证下发 (QoS 1)
│
└── {ws_id}/
    └── gateway/{gw_id}/
        ├── status                     # 网关→平台：在线状态/心跳 (QoS 0)
        ├── telemetry                  # 网关→平台：遥测数据 (QoS 0)
        ├── event                      # 网关→平台：告警事件 (QoS 1)
        ├── command                    # 平台→网关：指令下发 (QoS 1)
        ├── config                     # 平台→网关：配置下发 (QoS 1)
        └── device/
            ├── discover               # 网关→平台：子设备发现 (QoS 1)
            └── {sub_id}/
                ├── telemetry          # 子设备→平台（经网关转发）(QoS 0)
                ├── event              # 子设备→平台（经网关转发）(QoS 1)
                └── command            # 平台→子设备（经网关转发）(QoS 1)
```

### 平台订阅主题

平台启动后订阅以下通配符主题以接收所有网关消息：

| 订阅主题 | 用途 |
|----------|------|
| `tinyiothub/pairing/announce` | 接收网关配对宣告 |
| `tinyiothub/+/gateway/+/status` | 接收所有网关状态 |
| `tinyiothub/+/gateway/+/telemetry` | 接收所有网关遥测 |
| `tinyiothub/+/gateway/+/event` | 接收所有网关事件 |
| `tinyiothub/+/gateway/+/device/discover` | 接收所有子设备发现 |
| `tinyiothub/+/gateway/+/device/+/telemetry` | 接收所有子设备遥测 |

配对成功后，平台为每个已注册网关动态订阅其专属主题。

## 配对协议

网关通过 6 位配对码完成零配置注册，无需手动编辑配置文件或 SSH。

### 配对流程

```
网关开机 → 生成 6 位配对码 → 屏幕显示「482 916」
    ↓
网关通过 MQTT 匿名连接 broker，周期性发布宣告
    ↓
用户在平台输入配对码 → 平台验证 → 创建 Device → 下发凭据
    ↓
网关收到凭据 → 断开匿名连接 → 用正式凭据重连 → 开始数据上报
```

### 配对宣告（网关 → 平台）

- **Topic:** `tinyiothub/pairing/announce`
- **QoS:** 1
- **重发间隔:** 30s，直到配对成功或配对码过期（5 分钟）

```json
{
  "type": "pairing_announce",
  "code": "482916",
  "fingerprint": "aa:bb:cc:dd:ee:ff",
  "hostname": "factory-gw-01",
  "os": "Linux armv7l",
  "ip": "192.168.1.100",
  "hw_model": "Raspberry Pi 5"
}
```

### 配对响应（平台 → 网关）

- **Topic:** `tinyiothub/pairing/{code}/response`
- **QoS:** 1

```json
{
  "type": "pairing_ack",
  "success": true,
  "device_id": "dev_xyz",
  "workspace_id": "ws_abc",
  "credentials": {
    "client_id": "dev_xyz",
    "username": "dev_xyz",
    "password": "dt_abc123..."
  },
  "topics": {
    "status": "tinyiothub/ws_abc/gateway/dev_xyz/status",
    "telemetry": "tinyiothub/ws_abc/gateway/dev_xyz/telemetry",
    "event": "tinyiothub/ws_abc/gateway/dev_xyz/event",
    "command": "tinyiothub/ws_abc/gateway/dev_xyz/command",
    "config": "tinyiothub/ws_abc/gateway/dev_xyz/config",
    "device_discover": "tinyiothub/ws_abc/gateway/dev_xyz/device/discover",
    "device_telemetry": "tinyiothub/ws_abc/gateway/dev_xyz/device/+/telemetry"
  },
  "keepalive": 60
}
```

### 配对码规则 & 安全

| 规则 | 说明 |
|------|------|
| 格式 | 6 位纯数字，随机生成 |
| 有效期 | 5 分钟，到期自动刷新 |
| 一次性 | 配对成功后立即失效 |
| 尝试限制 | 同一用户同一配对码最多 5 次，超限锁定 1 分钟 |
| IP 限流 | 同一 IP 每分钟最多 3 次 |
| 宣告限流 | 每秒最多 20 个宣告，burst 50 |
| 无预绑定 | 配对码不预绑定用户/工作空间，谁输入正确谁配对 |
| 内存缓存 | code → fingerprint 映射仅存内存，不落盘 |

### 配对 API

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/v1/gateway/pair` | 输入配对码，返回配对结果 |

**请求：**
```json
{
  "code": "482916",
  "workspaceId": "ws_abc"
}
```

**响应：**
```json
{
  "code": 0,
  "msg": "",
  "result": {
    "deviceId": "dev_xyz",
    "deviceName": "factory-gw-01",
    "hostname": "factory-gw-01",
    "ip": "192.168.1.100"
  }
}
```

## 数据消息格式

### 网关状态（网关 → 平台）

- **Topic:** `tinyiothub/{ws_id}/gateway/{gw_id}/status`
- **QoS:** 0
- **频率:** 可配置（默认 30s）

```json
{
  "type": "status",
  "status": "online",
  "uptime": 3600,
  "timestamp": 1715432000
}
```

### 网关遥测（网关 → 平台）

- **Topic:** `tinyiothub/{ws_id}/gateway/{gw_id}/telemetry`
- **QoS:** 0

```json
{
  "type": "telemetry",
  "data": {
    "cpu": 23.5,
    "memory": 45.2,
    "disk": 32.1,
    "network_rx": 1024000,
    "network_tx": 512000
  },
  "timestamp": 1715432000
}
```

### 子设备遥测（网关转发 → 平台）

- **Topic:** `tinyiothub/{ws_id}/gateway/{gw_id}/device/{sub_id}/telemetry`
- **QoS:** 0

```json
{
  "type": "device_telemetry",
  "device_id": "dev_sub_001",
  "data": {
    "temperature": 25.3,
    "humidity": 68.5
  },
  "timestamp": 1715432000
}
```

### 子设备发现（网关 → 平台）

- **Topic:** `tinyiothub/{ws_id}/gateway/{gw_id}/device/discover`
- **QoS:** 1

```json
{
  "type": "device_discover",
  "devices": [
    {
      "name": "温度传感器-1",
      "device_type": "sensor",
      "protocol_type": "modbus",
      "address": "192.168.1.10:502",
      "driver_name": "modbus-tcp",
      "driver_options": "{\"register\":40001,\"function\":3}"
    }
  ]
}
```

## 指令下发

### 指令下发（平台 → 网关 → 子设备）

- **Topic:** `tinyiothub/{ws_id}/gateway/{gw_id}/device/{sub_id}/command`
- **QoS:** 1

**API 请求（前端 → 平台）：**
```
POST /api/v1/gateway/command
```

```json
{
  "deviceId": "dev_sub_001",
  "action": "set_value",
  "params": {
    "register": 40002,
    "value": 1
  }
}
```

**MQTT 消息（平台 → 网关）：**
```json
{
  "type": "command",
  "command_id": "cmd_001",
  "device_id": "dev_sub_001",
  "action": "set_value",
  "params": {
    "register": 40002,
    "value": 1
  },
  "timestamp": 1715432000
}
```

### 配置下发（平台 → 网关）

- **Topic:** `tinyiothub/{ws_id}/gateway/{gw_id}/config`
- **QoS:** 1

```json
{
  "type": "config",
  "config": {
    "telemetry_interval": 30,
    "log_level": "debug"
  },
  "timestamp": 1715432000
}
```

## QoS 策略

| Topic | QoS | 方向 | 说明 |
|-------|-----|------|------|
| `pairing/announce` | 1 | 网关→平台 | 保证到达，配对关键消息 |
| `pairing/{code}/response` | 1 | 平台→网关 | 保证到达，凭据不能丢失 |
| `{ws_id}/gateway/{gw_id}/status` | 0 | 网关→平台 | 周期性心跳，丢一次无影响 |
| `{ws_id}/gateway/{gw_id}/telemetry` | 0 | 网关→平台 | 高频数据，允许丢失 |
| `{ws_id}/gateway/{gw_id}/event` | 1 | 网关→平台 | 告警不能丢 |
| `{ws_id}/gateway/{gw_id}/command` | 1 | 平台→网关 | 指令必须送达 |
| `{ws_id}/gateway/{gw_id}/config` | 1 | 平台→网关 | 配置必须送达 |
| `{ws_id}/gateway/{gw_id}/device/discover` | 1 | 网关→平台 | 子设备发现不能丢 |
| `{ws_id}/gateway/{gw_id}/device/+/telemetry` | 0 | 网关→平台 | 子设备高频遥测 |

## 客户端示例

### JavaScript (MQTT.js over WebSocket)

```javascript
import mqtt from 'mqtt';

// 平台客户端：订阅所有网关遥测
const client = mqtt.connect('wss://mqtt.tinyiothub.com/mqtt', {
  username: process.env.MQTT_USERNAME,
  password: process.env.MQTT_PASSWORD,
  clientId: 'tinyiothub-platform-' + crypto.randomUUID(),
  keepalive: 30,
});

client.on('connect', () => {
  console.log('Connected to MQTT broker');

  // 订阅配对宣告
  client.subscribe('tinyiothub/pairing/announce', { qos: 1 });

  // 订阅所有网关状态和遥测
  client.subscribe('tinyiothub/+/gateway/+/status', { qos: 0 });
  client.subscribe('tinyiothub/+/gateway/+/telemetry', { qos: 0 });
  client.subscribe('tinyiothub/+/gateway/+/event', { qos: 1 });
});

client.on('message', (topic, message) => {
  const payload = JSON.parse(message.toString());
  console.log('Received:', topic, payload);
});
```

### Python (paho-mqtt)

```python
import paho.mqtt.client as mqtt
import os
import json

def on_connect(client, userdata, flags, rc):
    print(f"Connected with result code {rc}")
    client.subscribe("tinyiothub/pairing/announce", qos=1)
    client.subscribe("tinyiothub/+/gateway/+/status", qos=0)
    client.subscribe("tinyiothub/+/gateway/+/telemetry", qos=0)

def on_message(client, userdata, msg):
    payload = json.loads(msg.payload)
    print(f"Topic: {msg.topic}, Payload: {payload}")

client = mqtt.Client()
client.username_pw_set(
    os.environ.get("MQTT_USERNAME", ""),
    os.environ.get("MQTT_PASSWORD", ""),
)

client.tls_set()
client.on_connect = on_connect
client.on_message = on_message

client.connect("mqtt.tinyiothub.com", 8883, keepalive=30)
client.loop_forever()
```

### 发布网关遥测示例 (Python)

```python
import paho.mqtt.client as mqtt
import json
import time

client = mqtt.Client(client_id="dev_xyz")
client.username_pw_set("dev_xyz", "dt_abc123...")
client.connect("mqtt.tinyiothub.com", 1883, keepalive=60)
client.loop_start()

while True:
    # 网关状态
    client.publish(
        "tinyiothub/ws_abc/gateway/dev_xyz/status",
        json.dumps({
            "type": "status",
            "status": "online",
            "uptime": int(time.monotonic()),
            "timestamp": int(time.time()),
        }),
        qos=0,
    )

    # 网关遥测
    client.publish(
        "tinyiothub/ws_abc/gateway/dev_xyz/telemetry",
        json.dumps({
            "type": "telemetry",
            "data": {"cpu": 23.5, "memory": 45.2},
            "timestamp": int(time.time()),
        }),
        qos=0,
    )

    time.sleep(30)
```

## 错误处理

### 网关侧

| 场景 | 网关行为 |
|------|----------|
| 宣告发布失败（broker 不可达） | 指数退避重连（1s → 2s → 4s → 8s，最大 30s） |
| 配对响应超时（5 分钟内无响应） | 刷新配对码，重新宣告 |
| 正式凭据连接失败 | 重试 3 次（间隔 5s），失败后回退到匿名配对模式 |
| 配对成功后 | 凭据持久化到本地文件，断电重启后直接读取重连，无需重新配对 |
