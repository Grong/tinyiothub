# 设备事件上报契约

本文档定义设备/固件开发者向 TinyIoTHub 平台上报事件的协议契约。

## MQTT Topic

```
thing/{thing_id}/event/{event_name}
```

| 参数 | 说明 |
|------|------|
| `thing_id` | 物 ID（平台注册时分配） |
| `event_name` | 事件名称，由设备自定义（如 `temperature_high`、`door_open`） |

## Payload 格式

```json
{
  "level": "info",
  "data": {
    "temperature": 42.5,
    "unit": "celsius"
  },
  "ts": "2026-07-23T10:30:00Z"
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `level` | string | 是 | `info`、`warning`、`error`、`critical` |
| `data` | object | 是 | 事件携带的自定义数据，平台不校验结构 |
| `ts` | string | 否 | ISO 8601 时间戳，不传则使用服务端接收时间 |

## 行为约定

### 未知事件名
- 平台**不会报错**。未知事件名将被降级为 `info` 级别存储，设备端不会收到任何错误响应。

### 畸形 Payload
- 如果 payload 不是合法 JSON、缺少 `level` 字段、或 `level` 值无效，平台会**静默丢弃**该事件。**不会**向设备发送错误消息（避免 MQTT 风暴）。

### 节流（Throttle）
- `info` 和 `warning` 级别事件：**60 条/分钟/物**，超出部分丢弃。
- `error` 和 `critical` 级别事件：**不节流**，保证告警不丢失。

## 示例

```bash
# 使用 mosquitto_pub 发布事件
mosquitto_pub -h localhost -p 1883 \
  -t "thing/thing_abc123/event/temperature_high" \
  -m '{"level":"warning","data":{"temperature":42.5},"ts":"2026-07-23T10:30:00Z"}'
```

```rust
// 使用 rumqttc 发布事件
use rumqttc::{MqttOptions, Client, QoS};

let mut mqttoptions = MqttOptions::new("my-device", "localhost", 1883);
let (mut client, mut connection) = Client::new(mqttoptions, 10);

let payload = r#"{"level":"info","data":{"status":"online"},"ts":"2026-07-23T10:30:00Z"}"#;
client.publish("thing/thing_abc123/event/status_update", QoS::AtLeastOnce, false, payload)?;
```
