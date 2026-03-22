# 告警规则 API

## 概述

告警规则 API 提供告警规则的增删改查和启停控制功能。通过告警规则，可以基于设备数据点配置自动触发告警的条件。

## 接口列表

### 获取告警规则列表

```
GET /api/v1/alarm-rules
```

获取所有告警规则，支持分页和筛选。

**查询参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| name | string | 否 | 按名称模糊筛选 |
| trigger_type | string | 否 | 触发类型：threshold、offline、custom |
| enabled | boolean | 否 | 是否启用 |
| page | number | 否 | 页码，默认 1 |
| page_size | number | 否 | 每页数量，默认 20 |

**响应示例：**

```json
{
  "success": true,
  "result": [
    {
      "id": "rule_001",
      "name": "温度过高告警",
      "description": "当温度超过 30°C 时触发",
      "trigger_type": "threshold",
      "event_source_type": "device",
      "event_device_id": "device_001",
      "event_property": "temperature",
      "event_condition": ">",
      "event_value": 30,
      "event_duration": 60,
      "conditions": "[]",
      "actions": "[{\"type\":\"notification\",\"channel\":\"sms\"}]",
      "severity": "warning",
      "enabled": true,
      "created_at": "2024-01-01 10:00:00",
      "updated_at": "2024-01-01 10:00:00"
    }
  ]
}
```

---

### 获取告警规则详情

```
GET /api/v1/alarm-rules/{id}
```

**路径参数：**

| 参数 | 类型 | 说明 |
|------|------|------|
| id | string | 告警规则 ID |

**响应示例：**

```json
{
  "success": true,
  "result": {
    "id": "rule_001",
    "name": "温度过高告警",
    "description": "当温度超过 30°C 时触发",
    "trigger_type": "threshold",
    "event_source_type": "device",
    "event_device_id": "device_001",
    "event_property": "temperature",
    "event_condition": ">",
    "event_value": 30,
    "event_duration": 60,
    "conditions": "[]",
    "actions": "[{\"type\":\"notification\",\"channel\":\"sms\"}]",
    "severity": "warning",
    "enabled": true,
    "tags": "temperature,alarm",
    "created_at": "2024-01-01 10:00:00",
    "updated_at": "2024-01-01 10:00:00"
  }
}
```

---

### 创建告警规则

```
POST /api/v1/alarm-rules
```

**请求体：**

```json
{
  "name": "温度过高告警",
  "description": "当温度超过 30°C 时触发",
  "trigger_type": "threshold",
  "event_source_type": "device",
  "event_device_id": "device_001",
  "event_property": "temperature",
  "event_condition": ">",
  "event_value": 30,
  "event_duration": 60,
  "conditions": "[]",
  "actions": "[{\"type\":\"notification\",\"channel\":\"sms\"}]",
  "severity": "warning",
  "enabled": true,
  "tags": "temperature,alarm"
}
```

**触发类型说明：**

| 类型 | 说明 |
|------|------|
| threshold | 阈值触发（数据点超过/低于阈值） |
| offline | 设备离线触发 |
| custom | 自定义条件触发 |

**条件运算符：**

| 运算符 | 说明 |
|--------|------|
| > | 大于 |
| < | 小于 |
| >= | 大于等于 |
| <= | 小于等于 |
| == | 等于 |
| != | 不等于 |

---

### 更新告警规则

```
PUT /api/v1/alarm-rules/{id}
```

**请求体（支持部分更新）：**

```json
{
  "name": "温度告警（已修改）",
  "event_value": 35,
  "enabled": false
}
```

---

### 删除告警规则

```
DELETE /api/v1/alarm-rules/{id}
```

**响应：** `204 No Content`

---

### 启停告警规则

```
POST /api/v1/alarm-rules/{id}/toggle
```

切换告警规则的启用/禁用状态。

**响应示例：**

```json
{
  "success": true,
  "result": {
    "id": "rule_001",
    "enabled": false,
    "updated_at": "2024-01-07 16:00:00"
  }
}
```

## 数据结构

### AlarmRule

| 字段 | 类型 | 说明 |
|------|------|------|
| id | string | 规则 ID（UUID） |
| name | string | 规则名称 |
| description | string? | 规则描述 |
| trigger_type | string | 触发类型 |
| event_source_type | string? | 事件源类型 |
| event_device_id | string? | 触发设备 ID |
| event_property | string? | 触发属性 |
| event_condition | string? | 比较条件 |
| event_value | number? | 阈值 |
| event_duration | number? | 持续时间（秒） |
| conditions | string | 复杂条件（JSON） |
| actions | string | 触发动作（JSON） |
| severity | string? | 严重程度 |
| enabled | boolean | 是否启用 |
| tags | string? | 标签 |
| created_at | string | 创建时间 |
| updated_at | string | 更新时间 |

## 使用场景

### 1. 创建温度阈值告警

```json
POST /api/v1/alarm-rules
{
  "name": "车间温度过高告警",
  "trigger_type": "threshold",
  "event_device_id": "device_001",
  "event_property": "temperature",
  "event_condition": ">",
  "event_value": 35,
  "event_duration": 60,
  "severity": "critical",
  "enabled": true
}
```

### 2. 批量禁用告警规则

```json
POST /api/v1/alarm-rules/rule_001/toggle
```

## 错误码

| HTTP 状态码 | 说明 |
|-------------|------|
| 200 | 请求成功 |
| 400 | 请求参数错误 |
| 404 | 规则不存在 |
| 500 | 服务器内部错误 |
