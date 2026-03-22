# 事件 API

## 概述

事件 API 提供设备事件和系统事件的查询、实时推送、统计和安全审计等功能。

## 接口列表

### 获取事件列表

```
GET /api/v1/events
```

获取设备事件列表，支持多种筛选条件。

**查询参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| device_id | string | 否 | 设备 ID |
| event_type | string | 否 | 事件类型：alarm、status_change、data、system |
| event_level | string | 否 | 事件级别：trace、debug、info、warn、error |
| start_time | string | 否 | 开始时间（YYYY-MM-DD HH:MM:SS） |
| end_time | string | 否 | 结束时间（YYYY-MM-DD HH:MM:SS） |
| page | number | 否 | 页码，默认 1 |
| page_size | number | 否 | 每页数量，默认 20 |

**响应示例：**

```json
{
  "success": true,
  "result": [
    {
      "id": "event_001",
      "device_id": "device_001",
      "event_type": "alarm",
      "event_level": "error",
      "title": "温度过高告警",
      "message": "设备温度超过阈值 85°C，当前温度 92°C",
      "data": {
        "temperature": 92.0,
        "threshold": 85.0,
        "unit": "°C"
      },
      "source": "temperature_sensor",
      "created_at": "2024-01-07 15:30:00",
      "acknowledged_at": null,
      "resolved_at": null,
      "status": "active"
    }
  ]
}
```

---

### 创建事件

```
POST /api/v1/events
```

手动创建一条事件记录。

**请求体：**

```json
{
  "device_id": "device_001",
  "event_type": "alarm",
  "event_level": "warning",
  "title": "设备维护提醒",
  "message": "设备需要定期维护",
  "data": {
    "maintenance_type": "calibration"
  }
}
```

---

### 获取实时事件

```
GET /api/v1/events/real-time
```

获取实时事件流（需要配合 SSE 使用）。

**响应：** 通过 SSE 实时推送事件数据。

---

### 获取实时事件状态摘要

```
GET /api/v1/events/real-time/status
```

**响应示例：**

```json
{
  "success": true,
  "result": {
    "total": 125,
    "active": 15,
    "acknowledged": 80,
    "resolved": 30,
    "by_level": {
      "critical": 3,
      "error": 12,
      "warning": 45,
      "info": 65
    }
  }
}
```

---

### 确认实时事件

```
POST /api/v1/events/real-time/{id}/acknowledge
```

**请求体：**

```json
{
  "acknowledged_by": "admin",
  "note": "已现场处理"
}
```

---

### 获取事件总览

```
GET /api/v1/events/overview
```

获取事件统计总览信息。

**响应示例：**

```json
{
  "success": true,
  "result": {
    "total_events": 1250,
    "active_events": 25,
    "resolved_events": 1200,
    "by_type": {
      "alarm": 450,
      "status_change": 300,
      "data": 400,
      "system": 100
    },
    "by_level": {
      "critical": 50,
      "error": 200,
      "warning": 500,
      "info": 500
    },
    "trend": "increasing"
  }
}
```

---

### 获取用户权限

```
GET /api/v1/events/security/permissions
```

获取当前用户对事件的访问权限。

---

### 获取安全配置

```
GET /api/v1/events/security/config
```

```
PUT /api/v1/events/security/config
```

获取或更新事件安全配置。

---

### 获取用户角色

```
GET /api/v1/events/security/roles
```

获取事件相关的用户角色列表。

---

### 获取事件审计日志

```
GET /api/v1/events/security/audit-logs/{event_id}
```

获取指定事件的审计日志。

```
GET /api/v1/events/security/audit-logs
```

获取当前用户的审计日志。

```
GET /api/v1/events/security/audit-logs/all
```

获取所有审计日志（需要管理员权限）。

---

### 清理审计日志

```
POST /api/v1/events/security/cleanup
```

清理过期的审计日志。

---

### 性能监控路由

```
GET /api/v1/events/performance/...
```

性能监控相关端点，嵌套在 `/api/v1/events/performance/` 下。

---

### SSE 实时推送

```
GET /api/v1/events/sse
```

通过 Server-Sent Events 实时推送事件。

```
GET /api/v1/events/sse/overview
```

获取 SSE 连接概览。

```
GET /api/v1/events/sse/connections
```

获取当前 SSE 连接列表。

## 事件数据结构

### DeviceEvent

| 字段 | 类型 | 说明 |
|------|------|------|
| id | string | 事件 ID |
| device_id | string? | 设备 ID |
| event_type | string | 事件类型 |
| event_level | string | 事件级别 |
| title | string | 事件标题 |
| message | string | 事件消息 |
| data | object? | 附加数据 |
| source | string? | 事件来源 |
| created_at | string | 创建时间 |
| acknowledged_at | string? | 确认时间 |
| resolved_at | string? | 解决时间 |
| status | string | 状态：active、acknowledged、resolved |

### 事件类型

| 类型 | 说明 |
|------|------|
| alarm | 告警事件 |
| status_change | 状态变化事件 |
| data | 数据上报事件 |
| system | 系统事件 |

### 事件级别

| 级别 | 说明 |
|------|------|
| trace | 追踪信息 |
| debug | 调试信息 |
| info | 一般信息 |
| warn | 警告信息 |
| error | 错误信息 |
| critical | 严重错误 |

## 使用场景

### 1. 实时告警监控

使用 SSE 端点获取实时告警：

```javascript
const eventSource = new EventSource('/api/v1/events/sse');

eventSource.onmessage = (event) => {
  const data = JSON.parse(event.data);
  if (data.event_type === 'alarm' && data.event_level === 'critical') {
    showCriticalAlert(data);
  }
};
```

### 2. 历史事件查询

```javascript
const events = await fetch('/api/v1/events?' + new URLSearchParams({
  device_id: 'device_001',
  event_type: 'alarm',
  start_time: '2024-01-01 00:00:00',
  end_time: '2024-01-07 23:59:59',
  page_size: 50
}));
```

## 错误码

| HTTP 状态码 | 说明 |
|-------------|------|
| 200 | 请求成功 |
| 400 | 请求参数错误 |
| 401 | 未认证 |
| 403 | 权限不足 |
| 500 | 服务器内部错误 |
