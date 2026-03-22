# 自动化规则 API

## 概述

自动化规则 API 提供自动化规则的完整生命周期管理，包括创建、更新、删除、启用、禁用以及手动执行。通过自动化规则，可以将设备事件、数据变化或定时触发器与一系列动作关联，实现自动化的设备控制和工作流程。

## 接口列表

### 获取自动化列表

```
GET /api/v1/automations
```

**查询参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| name | string | 否 | 按名称模糊筛选 |
| trigger_type | string | 否 | 触发类型：event、schedule、manual |
| enabled | boolean | 否 | 是否启用 |
| page | number | 否 | 页码，默认 1 |
| page_size | number | 否 | 每页数量，默认 20 |

**响应示例：**

```json
{
  "success": true,
  "result": [
    {
      "id": "auto_001",
      "name": "温度超过30°C时关闭空调",
      "description": "当温度传感器超过30°C时，自动关闭空调",
      "trigger_type": "event",
      "event_source_type": "device",
      "event_device_id": "device_001",
      "event_property": "temperature",
      "event_condition": ">",
      "event_value": 30,
      "cron_expression": null,
      "conditions": "[]",
      "actions": "[{\"type\":\"device_command\",\"device_id\":\"device_002\",\"command\":\"off\"}]",
      "timeout_seconds": 30,
      "retry_count": 3,
      "retry_delay_seconds": 5,
      "cooldown_seconds": 300,
      "priority": 100,
      "enabled": true,
      "run_count": 45,
      "success_count": 42,
      "fail_count": 3,
      "last_run_at": "2024-01-07 15:30:00",
      "last_run_status": "success",
      "last_run_error": null,
      "tags": "temperature,ac,automation",
      "created_at": "2024-01-01 10:00:00",
      "updated_at": "2024-01-07 15:30:00"
    }
  ]
}
```

---

### 获取自动化详情

```
GET /api/v1/automations/{id}
```

**响应示例：**

```json
{
  "success": true,
  "result": {
    "id": "auto_001",
    "name": "温度超过30°C时关闭空调",
    "description": "当温度传感器超过30°C时，自动关闭空调",
    "trigger_type": "event",
    "event_source_type": "device",
    "event_device_id": "device_001",
    "event_property": "temperature",
    "event_condition": ">",
    "event_value": 30,
    "event_duration": 0,
    "cron_expression": null,
    "conditions": "[]",
    "actions": "[{\"type\":\"device_command\",\"device_id\":\"device_002\",\"command\":\"off\"}]",
    "timeout_seconds": 30,
    "retry_count": 3,
    "retry_delay_seconds": 5,
    "cooldown_seconds": 300,
    "priority": 100,
    "enabled": true,
    "run_count": 45,
    "success_count": 42,
    "fail_count": 3,
    "last_run_at": "2024-01-07 15:30:00",
    "last_run_status": "success",
    "last_run_error": null,
    "tags": "temperature,ac,automation",
    "created_at": "2024-01-01 10:00:00",
    "updated_at": "2024-01-07 15:30:00"
  }
}
```

---

### 创建自动化规则

```
POST /api/v1/automations
```

**请求体：**

```json
{
  "name": "定时开关灯",
  "description": "每天早上8点开灯，晚上10点关灯",
  "trigger_type": "schedule",
  "cron_expression": "0 8,22 * * *",
  "conditions": "[]",
  "actions": "[{\"type\":\"device_command\",\"device_id\":\"device_003\",\"command\":\"toggle\"}]",
  "timeout_seconds": 30,
  "retry_count": 2,
  "retry_delay_seconds": 5,
  "cooldown_seconds": 0,
  "priority": 100,
  "enabled": true,
  "tags": "lighting,schedule"
}
```

**触发类型：**

| 类型 | 说明 |
|------|------|
| event | 事件触发（设备数据变化、告警等） |
| schedule | 定时触发（Cron 表达式） |
| manual | 手动触发 |

**Cron 表达式示例：**

| 表达式 | 说明 |
|--------|------|
| `0 8 * * *` | 每天早上 8:00 |
| `0 8,22 * * *` | 每天 8:00 和 22:00 |
| `*/5 * * * *` | 每 5 分钟 |
| `0 9 * * 1-5` | 工作日 9:00 |

---

### 更新自动化规则

```
PUT /api/v1/automations/{id}
```

**请求体（支持部分更新）：**

```json
{
  "name": "定时开关灯（已修改）",
  "cron_expression": "0 7,21 * * *",
  "enabled": false
}
```

---

### 删除自动化规则

```
DELETE /api/v1/automations/{id}
```

**响应示例：**

```json
{
  "success": true,
  "result": true
}
```

---

### 启用自动化

```
POST /api/v1/automations/{id}/enable
```

---

### 禁用自动化

```
POST /api/v1/automations/{id}/disable
```

---

### 手动执行自动化

```
POST /api/v1/automations/{id}/run
```

手动触发自动化规则执行。

**响应示例：**

```json
{
  "success": true,
  "result": {
    "message": "自动化已执行",
    "executed_at": "2024-01-07T08:30:00Z"
  }
}
```

---

### 测试自动化条件

```
POST /api/v1/automations/{id}/test
```

测试自动化规则的条件判断逻辑，使用模拟数据进行验证。

**请求体：**

```json
{
  "conditions": "{\"property\":\"temperature\",\"operator\":\">\",\"value\":30}",
  "mock_data": {
    "temperature": 35
  }
}
```

**响应示例：**

```json
{
  "success": true,
  "result": {
    "matched": true,
    "details": "条件匹配：temperature(35) > 30"
  }
}
```

---

### 获取自动化统计

```
GET /api/v1/automations/statistics
```

**响应示例：**

```json
{
  "success": true,
  "result": {
    "total": 15,
    "enabled": 12,
    "disabled": 3
  }
}
```

## 数据结构

### Automation

| 字段 | 类型 | 说明 |
|------|------|------|
| id | string | 自动化 ID（UUID） |
| name | string | 名称 |
| description | string? | 描述 |
| trigger_type | string | 触发类型 |
| event_source_type | string? | 事件源类型 |
| event_device_id | string? | 触发设备 ID |
| event_property | string? | 触发属性 |
| event_condition | string? | 比较条件 |
| event_value | number? | 阈值 |
| cron_expression | string? | Cron 表达式（定时触发） |
| conditions | string | 复杂条件（JSON） |
| actions | string | 执行动作（JSON） |
| timeout_seconds | number | 执行超时时间 |
| retry_count | number | 重试次数 |
| retry_delay_seconds | number | 重试间隔 |
| cooldown_seconds | number | 冷却时间 |
| priority | number | 优先级（越小越高） |
| enabled | boolean | 是否启用 |
| run_count | number | 总执行次数 |
| success_count | number | 成功次数 |
| fail_count | number | 失败次数 |
| last_run_at | string? | 最后执行时间 |
| last_run_status | string? | 最后执行状态 |
| last_run_error | string? | 最后错误信息 |
| tags | string? | 标签 |
| created_at | string | 创建时间 |
| updated_at | string | 更新时间 |

### Actions 格式说明

```json
[
  {
    "type": "device_command",
    "device_id": "device_002",
    "command": "set_temperature",
    "parameters": {
      "value": 25
    }
  },
  {
    "type": "notification",
    "channel": "email",
    "template": "alert"
  },
  {
    "type": "http_request",
    "url": "http://example.com/webhook",
    "method": "POST",
    "body": {}
  }
]
```

## 使用场景

### 1. 环境温度自动控制

```json
POST /api/v1/automations
{
  "name": "温度超过30°C关闭空调",
  "trigger_type": "event",
  "event_device_id": "device_temp_001",
  "event_property": "temperature",
  "event_condition": ">",
  "event_value": 30,
  "event_duration": 60,
  "actions": "[{\"type\":\"device_command\",\"device_id\":\"device_ac_001\",\"command\":\"off\"}]",
  "cooldown_seconds": 300,
  "enabled": true
}
```

### 2. 定时数据采集

```json
POST /api/v1/automations
{
  "name": "每小时数据采集",
  "trigger_type": "schedule",
  "cron_expression": "0 * * * *",
  "actions": "[{\"type\":\"device_command\",\"device_id\":\"device_001\",\"command\":\"read_all\"}]",
  "enabled": true
}
```

## 错误码

| HTTP 状态码 | 说明 |
|-------------|------|
| 200 | 请求成功 |
| 400 | 无效的 Cron 表达式或参数错误 |
| 404 | 自动化不存在 |
| 500 | 服务器内部错误 |
