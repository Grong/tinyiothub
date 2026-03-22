# 网关管理 API

## 概述

网关管理 API 提供边缘网关设备的注册、配置和状态监控功能。网关是连接设备与云端的核心组件，负责协议转换和数据转发。

## 接口列表

### 获取网关列表

```
GET /api/v1/gateways
```

**响应示例：**

```json
{
  "success": true,
  "result": [
    {
      "id": "gw_001",
      "name": "边缘网关01",
      "sn": "GW-SN-001",
      "description": "一楼边缘计算网关",
      "address": "192.168.1.100",
      "status": "online",
      "version": "1.2.0",
      "connected_devices": 15,
      "last_heartbeat": "2024-01-07 15:30:00",
      "created_at": "2024-01-01 10:00:00",
      "updated_at": "2024-01-07 15:30:00"
    }
  ]
}
```

---

### 获取网关详情

```
GET /api/v1/gateways/{id}
```

**响应示例：**

```json
{
  "success": true,
  "result": {
    "id": "gw_001",
    "name": "边缘网关01",
    "sn": "GW-SN-001",
    "description": "一楼边缘计算网关",
    "address": "192.168.1.100",
    "status": "online",
    "version": "1.2.0",
    "connected_devices": 15,
    "cpu_usage": 25.5,
    "memory_usage": 45.2,
    "disk_usage": 30.1,
    "uptime_seconds": 864000,
    "last_heartbeat": "2024-01-07 15:30:00",
    "tags": ["floor1", "edge"],
    "created_at": "2024-01-01 10:00:00",
    "updated_at": "2024-01-07 15:30:00"
  }
}
```

---

### 创建网关

```
POST /api/v1/gateways
```

**请求体：**

```json
{
  "name": "边缘网关02",
  "sn": "GW-SN-002",
  "description": "二楼边缘计算网关",
  "address": "192.168.1.101",
  "version": "1.2.0",
  "tags": ["floor2", "edge"]
}
```

---

### 更新网关

```
PUT /api/v1/gateways/{id}
```

**请求体：**

```json
{
  "name": "边缘网关02（已修改）",
  "description": "二楼边缘计算网关 - 备用",
  "tags": ["floor2", "edge", "backup"]
}
```

---

### 删除网关

```
DELETE /api/v1/gateways/{id}
```

---

### 获取网关设备列表

```
GET /api/v1/gateways/{id}/devices
```

获取连接到指定网关的所有设备。

**响应示例：**

```json
{
  "success": true,
  "result": [
    {
      "id": "device_001",
      "name": "温度传感器01",
      "device_type": "sensor",
      "driver_name": "modbus_tcp",
      "status": "online",
      "connected_at": "2024-01-01 10:00:00"
    }
  ]
}
```

---

### 更新网关状态

```
PUT /api/v1/gateways/{id}/status
```

更新网关的运行状态（通常由网关心跳自动更新）。

**请求体：**

```json
{
  "status": "online",
  "version": "1.2.0",
  "cpu_usage": 25.5,
  "memory_usage": 45.2,
  "connected_devices": 15
}
```

## 数据结构

### Gateway

| 字段 | 类型 | 说明 |
|------|------|------|
| id | string | 网关 ID |
| name | string | 网关名称 |
| sn | string | 网关序列号 |
| description | string? | 描述 |
| address | string? | IP 地址 |
| status | string | 状态：online、offline、warning |
| version | string | 软件版本 |
| connected_devices | number | 连接的设备数量 |
| cpu_usage | number? | CPU 使用率（%） |
| memory_usage | number? | 内存使用率（%） |
| disk_usage | number? | 磁盘使用率（%） |
| uptime_seconds | number? | 运行时间（秒） |
| last_heartbeat | string? | 最后心跳时间 |
| tags | string? | 标签 |
| created_at | string | 创建时间 |
| updated_at | string | 更新时间 |

### 网关状态说明

| 状态 | 说明 |
|------|------|
| online | 网关在线，正常运行 |
| offline | 网关心跳丢失 |
| warning | 网关异常，需要关注 |

## 使用场景

### 1. 注册新网关

```json
POST /api/v1/gateways
{
  "name": "生产线网关",
  "sn": "GW-PROD-001",
  "description": "生产线边缘网关",
  "address": "192.168.2.100",
  "tags": ["production", "line1"]
}
```

### 2. 监控网关健康状态

```javascript
// 获取网关详情
const gateway = await fetch('/api/v1/gateways/gw_001');

// 检查 CPU 使用率
if (gateway.cpu_usage > 80) {
  console.warn('网关 CPU 使用率过高');
}

// 检查连接设备数
console.log(`网关 ${gateway.name} 连接了 ${gateway.connected_devices} 个设备`);
```

## 错误码

| HTTP 状态码 | 说明 |
|-------------|------|
| 200 | 请求成功 |
| 400 | 请求参数错误 |
| 404 | 网关不存在 |
| 500 | 服务器内部错误 |
