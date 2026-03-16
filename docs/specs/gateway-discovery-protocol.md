# 网关自动发现协议 - 规格说明

> 版本：1.0.0
> 更新日期：2026-03-16

---

## 1. 概述

### 1.1 目标

定义 TinyIoTHub 云端与硬件网关之间的通信协议，实现：
- 网关主动注册并保持在线
- 网关上报其发现的设备列表
- 实时数据转发到云端

### 1.2 设计原则

- **扁平化管理** - 网关和设备都在同一层级管理
- **简单易用** - 不引入复杂的父子设备关系
- **MQTT 为主** - 使用现有 MQTT 协议

---

## 2. 协议设计

### 2.1 认证方式

使用 **Token 认证**：
- 网关在首次连接时使用 API Key 换取 Token
- Token 有效期 7 天，到期自动续期
- Token 存储在网关本地

### 2.2 MQTT 主题设计

扁平化主题结构：

```
# 网关注册与认证
tinyiothub/gateway/{gateway_id}/auth/register     # 网关注册 (POST)
tinyiothub/gateway/{gateway_id}/auth/refresh     # Token 刷新 (POST)
tinyiothub/gateway/{gateway_id}/auth/status      # 认证状态 (GET)

# 网关状态
tinyiothub/gateway/{gateway_id}/status           # 网关状态上报
tinyiothub/gateway/{gateway_id}/online           # 网关上线
tinyiothub/gateway/{gateway_id}/offline          # 网关离线

# 设备管理
tinyiothub/gateway/{gateway_id}/devices/list     # 设备列表上报
tinyiothub/gateway/{gateway_id}/devices/add      # 新增设备
tinyiothub/gateway/{gateway_id}/devices/remove  # 移除设备
tinyiothub/gateway/{gateway_id}/devices/update   # 设备更新

# 实时数据
tinyiothub/gateway/{gateway_id}/data             # 设备数据上报

# 云端指令
tinyiothub/gateway/{gateway_id}/command          # 下发命令到网关
tinyiothub/gateway/{gateway_id}/command/response # 命令响应
```

---

## 3. 消息格式

### 3.1 JSON 通用格式

```json
{
  "timestamp": "2026-03-16T20:00:00Z",
  "message_id": "uuid",
  "payload": { ... }
}
```

### 3.2 网关注册

**请求 (网关 -> 云端)**
Topic: `tinyiothub/gateway/{gateway_id}/auth/register`

```json
{
  "timestamp": "2026-03-16T20:00:00Z",
  "message_id": "msg-001",
  "payload": {
    "api_key": "your-api-key",
    "gateway_name": "我的网关",
    "gateway_type": "esp32-s3",
    "firmware_version": "1.0.0",
    " Capabilities": ["wifi_scan", "ble_scan", "mqtt_bridge"]
  }
}
```

**响应 (云端 -> 网关)**
Topic: `tinyiothub/gateway/{gateway_id}/auth/register/response`

```json
{
  "timestamp": "2026-03-16T20:00:01Z",
  "message_id": "msg-001",
  "payload": {
    "success": true,
    "token": "eyJhbGciOiJIUzI1NiIs...",
    "expires_at": "2026-03-23T20:00:00Z",
    "gateway_id": "gw-xxx-yyy"
  }
}
```

### 3.3 设备列表上报

**网关 -> 云端**
Topic: `tinyiothub/gateway/{gateway_id}/devices/list`

```json
{
  "timestamp": "2026-03-16T20:00:00Z",
  "message_id": "msg-002",
  "payload": {
    "devices": [
      {
        "device_id": "dev-001",
        "name": "客厅灯",
        "type": "light",
        "protocol": "mqtt",
        "online": true,
        "properties": {
          "power": true,
          "brightness": 80
        }
      },
      {
        "device_id": "dev-002",
        "name": "温湿度传感器",
        "type": "sensor",
        "protocol": "wifi",
        "online": true,
        "properties": {
          "temperature": 25.5,
          "humidity": 60
        }
      }
    ]
  }
}
```

### 3.4 实时数据上报

**网关 -> 云端**
Topic: `tinyiothub/gateway/{gateway_id}/data`

```json
{
  "timestamp": "2026-03-16T20:00:00Z",
  "message_id": "msg-003",
  "payload": {
    "device_id": "dev-001",
    "properties": [
      {
        "name": "temperature",
        "value": 25.5,
        "type": "number",
        "unit": "℃"
      },
      {
        "name": "humidity",
        "value": 60,
        "type": "number",
        "unit": "%"
      }
    ]
  }
}
```

### 3.5 网关状态上报

**网关 -> 云端**
Topic: `tinyiothub/gateway/{gateway_id}/status`

```json
{
  "timestamp": "2026-03-16T20:00:00Z",
  "message_id": "msg-004",
  "payload": {
    "status": "online",
    "uptime": 3600,
    "memory_usage": 65,
    "cpu_usage": 30,
    "wifi_signal": -45,
    "connected_devices": 5
  }
}
```

---

## 4. API 设计

### 4.1 REST API (用于管理)

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | /api/v1/gateways | 创建网关 |
| GET | /api/v1/gateways | 网关列表 |
| GET | /api/v1/gateways/:id | 网关详情 |
| PUT | /api/v1/gateways/:id | 更新网关 |
| DELETE | /api/v1/gateways/:id | 删除网关 |
| GET | /api/v1/gateways/:id/devices | 网关下的设备 |
| POST | /api/v1/gateways/:id/command | 下发命令 |

### 4.2 数据库设计

```sql
-- 网关表
CREATE TABLE gateways (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    token TEXT,
    token_expires_at TEXT,
    status TEXT DEFAULT 'offline', -- online, offline
    gateway_type TEXT,
    firmware_version TEXT,
    last_seen TEXT,
    created_at TEXT,
    updated_at TEXT
);

-- 网关设备关联表 (扁平化)
CREATE TABLE gateway_devices (
    id TEXT PRIMARY KEY,
    gateway_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    created_at TEXT,
    FOREIGN KEY (gateway_id) REFERENCES gateways(id) ON DELETE CASCADE,
    FOREIGN KEY (device_id) REFERENCES devices(id) ON DELETE CASCADE,
    UNIQUE(gateway_id, device_id)
);
```

---

## 5. 简单设计说明

### 5.1 扁平化管理

- **不区分网关设备和直连设备**
- 所有设备都在同一列表展示
- 网关只是一个"通道"，不单独管理

### 5.2 设备归属

- 设备通过 `gateway_id` 标记来源
- 在设备列表中显示"所属网关"
- 可以按网关筛选设备

### 5.3 数据流

```
设备 -> 网关 -> 云端 MQTT -> TinyIoTHub
                ↓
          实时展示 / 存储历史
```

---

## 6. 安全考虑

- Token 认证确保网关身份
- MQTT over TLS 加密传输
- API Key 仅用于首次注册
- Token 定期续期

---

## 7. 实施计划

### Phase 1: 基础功能
- [ ] 数据库表设计
- [ ] 网关注册 API
- [ ] MQTT 主题定义
- [ ] 设备列表上报处理

### Phase 2: 数据流转
- [ ] 实时数据接收
- [ ] 网关状态监控
- [ ] 命令下发通道

### Phase 3: 管理功能
- [ ] 网关管理页面
- [ ] 设备归属展示
- [ ] 网关筛选

---

*规格说明 - 等待审批后实施*
