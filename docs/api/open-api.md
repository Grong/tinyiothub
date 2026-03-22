# Open API（开放 API）

## 概述

Open API 是面向外部平台和 AI 系统的公开接口，使用 API Key 进行身份验证，无需 JWT Token。适用于第三方应用集成、物联网数据对接和 AI 平台接入等场景。

## 认证方式

Open API 使用 `X-API-Key` 请求头进行身份验证：

```http
GET /open/devices
X-API-Key: your_api_key_here
```

## 基础信息

| 项目 | 说明 |
|------|------|
| 基础 URL | `http://localhost:3002/open/` |
| 认证方式 | API Key（请求头） |
| 响应格式 | JSON |

## 接口列表

### 健康检查

```
GET /open/health
```

**响应示例：**

```json
{
  "status": "ok",
  "service": "TinyIoTHub Open API",
  "version": "1.0.0",
  "timestamp": "2024-01-07T10:00:00Z"
}
```

---

### 获取设备列表

```
GET /open/devices
```

获取当前租户下的设备列表。

**响应示例：**

```json
[
  {
    "id": "device_001",
    "name": "温度传感器01",
    "display_name": "一楼温度传感器",
    "device_type": "sensor",
    "state": 1,
    "created_at": "2024-01-01 10:00:00"
  }
]
```

---

### 获取设备详情

```
GET /open/devices/{id}
```

**响应示例：**

```json
{
  "id": "device_001",
  "name": "温度传感器01",
  "display_name": "一楼温度传感器",
  "device_type": "sensor",
  "address": "192.168.1.100",
  "state": 1,
  "protocol_type": "modbus_tcp",
  "created_at": "2024-01-01 10:00:00",
  "updated_at": "2024-01-07 15:30:00"
}
```

---

### 获取设备属性

```
GET /open/devices/{id}/properties
```

获取设备的当前属性值。

**响应示例：**

```json
[
  {
    "name": "temperature",
    "display_name": "温度",
    "data_type": "float",
    "value": "25.5",
    "unit": "°C",
    "updated_at": "2024-01-07 15:30:00"
  },
  {
    "name": "humidity",
    "display_name": "湿度",
    "data_type": "float",
    "value": "60.2",
    "unit": "%RH",
    "updated_at": "2024-01-07 15:30:00"
  }
]
```

---

### 获取设备指令列表

```
GET /open/devices/{id}/commands
```

**响应示例：**

```json
[
  {
    "id": "cmd_001",
    "name": "reset",
    "display_name": "重置设备",
    "description": "将设备重置为默认状态",
    "command_type": "system"
  },
  {
    "id": "cmd_002",
    "name": "calibrate",
    "display_name": "校准",
    "description": "校准传感器",
    "command_type": "custom"
  }
]
```

---

### 发送设备指令

```
POST /open/devices/{id}/command
```

向设备下发控制指令。

**请求体：**

```json
{
  "command": "set_temperature",
  "params": {
    "value": 25
  }
}
```

**响应示例：**

```json
{
  "command_id": "cmd_exec_001",
  "status": "pending",
  "message": "命令已发送"
}
```

---

### 获取设备事件

```
GET /open/devices/{id}/events
```

获取指定设备的事件历史记录。

**响应示例：**

```json
[
  {
    "id": "event_001",
    "event_type": "alarm",
    "event_level": "error",
    "message": "温度超过阈值",
    "created_at": "2024-01-07 15:30:00"
  }
]
```

---

### 获取全部事件

```
GET /open/events
```

获取所有设备的事件记录。

**响应示例：**

```json
[
  {
    "id": "event_001",
    "event_type": "alarm",
    "event_level": "error",
    "message": "温度超过阈值",
    "device_id": "device_001",
    "created_at": "2024-01-07 15:30:00"
  }
]
```

## 错误响应

```json
{
  "error": "Not Found",
  "message": "Device not found"
}
```

| HTTP 状态码 | 说明 |
|-------------|------|
| 200 | 请求成功 |
| 400 | 请求参数错误 |
| 401 | API Key 无效或未提供 |
| 403 | API Key 已禁用或过期 |
| 404 | 资源不存在 |
| 429 | 请求频率超限 |
| 500 | 服务器内部错误 |

## API Key 管理

API Key 通过租户（Tenant）管理，每个租户可以拥有多个 API Key。

**API Key 权限控制：**
- 每个 Key 可以设置访问权限范围
- 支持设置 Key 的有效期
- 支持撤销 Key

## 使用示例

### cURL

```bash
# 获取设备列表
curl -X GET http://localhost:3002/open/devices \
  -H "X-API-Key: your_api_key_here"

# 发送设备命令
curl -X POST http://localhost:3002/open/devices/device_001/command \
  -H "X-API-Key: your_api_key_here" \
  -H "Content-Type: application/json" \
  -d '{"command":"reset","params":{}}'
```

### JavaScript

```javascript
const API_KEY = 'your_api_key_here';
const BASE_URL = 'http://localhost:3002/open';

async function fetchDevices() {
  const response = await fetch(`${BASE_URL}/devices`, {
    headers: { 'X-API-Key': API_KEY }
  });
  return response.json();
}

async function sendCommand(deviceId, command, params = {}) {
  const response = await fetch(`${BASE_URL}/devices/${deviceId}/command`, {
    method: 'POST',
    headers: {
      'X-API-Key': API_KEY,
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({ command, params })
  });
  return response.json();
}
```

### Python

```python
import requests

API_KEY = 'your_api_key_here'
BASE_URL = 'http://localhost:3002/open'
headers = {'X-API-Key': API_KEY}

# 获取设备列表
devices = requests.get(f'{BASE_URL}/devices', headers=headers).json()

# 发送命令
result = requests.post(
    f'{BASE_URL}/devices/device_001/command',
    headers=headers,
    json={'command': 'reset', 'params': {}}
).json()
```

## 限流说明

- 每个 API Key 默认有调用频率限制
- 超出限制返回 `429 Too Many Requests`
- 建议在调用端做好重试和缓存策略
