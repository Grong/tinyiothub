# 通知管理 API

## 概述

通知管理 API 提供通知规则的增删改查和通知历史查询功能。通知规则定义了告警触发后如何通过各种渠道发送通知。

## 接口列表

### 获取通知规则列表

```
GET /api/v1/notifications/rules
```

**响应示例：**

```json
{
  "success": true,
  "result": [
    {
      "id": "rule_001",
      "name": "高温告警通知",
      "description": "当温度超过阈值时发送通知",
      "alarm_rule_id": "alarm_rule_001",
      "channel_ids": ["channel_001", "channel_002"],
      "template_id": "template_001",
      "conditions": "{\"severity\":[\"critical\",\"error\"]}",
      "enabled": true,
      "created_at": "2024-01-01 10:00:00",
      "updated_at": "2024-01-01 10:00:00"
    }
  ]
}
```

---

### 创建通知规则

```
POST /api/v1/notifications/rules
```

**请求体：**

```json
{
  "name": "高温告警通知",
  "description": "当温度超过阈值时发送通知",
  "alarm_rule_id": "alarm_rule_001",
  "channel_ids": ["channel_001"],
  "template_id": "template_001",
  "conditions": "{\"severity\":[\"critical\",\"error\"]}",
  "enabled": true
}
```

---

### 获取通知规则详情

```
GET /api/v1/notifications/rules/{rule_id}
```

---

### 更新通知规则

```
PUT /api/v1/notifications/rules/{rule_id}
```

---

### 删除通知规则

```
DELETE /api/v1/notifications/rules/{rule_id}
```

---

### 获取通知历史

```
GET /api/v1/notifications/history
```

查询已发送的通知历史记录。

**查询参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| rule_id | string | 否 | 通知规则 ID |
| channel_id | string | 否 | 通知渠道 ID |
| status | string | 否 | 发送状态：success、failed、pending |
| start_time | string | 否 | 开始时间 |
| end_time | string | 否 | 结束时间 |
| page | number | 否 | 页码，默认 1 |
| page_size | number | 否 | 每页数量，默认 20 |

**响应示例：**

```json
{
  "success": true,
  "result": [
    {
      "id": "history_001",
      "rule_id": "rule_001",
      "channel_id": "channel_001",
      "alarm_id": "alarm_001",
      "recipient": "13800138000",
      "status": "success",
      "sent_at": "2024-01-07 15:30:00",
      "error_message": null
    }
  ]
}
```

---

### 发送测试通知

```
POST /api/v1/notifications/test
```

向指定渠道发送测试通知。

**请求体：**

```json
{
  "channel_id": "channel_001",
  "message": "这是一条测试通知"
}
```

**响应示例：**

```json
{
  "success": true,
  "result": {
    "sent": true,
    "message": "测试通知发送成功"
  }
}
```

## 通知数据结构

### NotificationRule

| 字段 | 类型 | 说明 |
|------|------|------|
| id | string | 规则 ID |
| name | string | 规则名称 |
| description | string? | 描述 |
| alarm_rule_id | string? | 关联的告警规则 ID |
| channel_ids | string[] | 通知渠道 ID 列表 |
| template_id | string? | 通知模板 ID |
| conditions | string | 触发条件（JSON） |
| enabled | boolean | 是否启用 |
| created_at | string | 创建时间 |
| updated_at | string | 更新时间 |

### NotificationHistory

| 字段 | 类型 | 说明 |
|------|------|------|
| id | string | 历史记录 ID |
| rule_id | string | 通知规则 ID |
| channel_id | string | 通知渠道 ID |
| alarm_id | string? | 关联告警 ID |
| recipient | string | 接收人 |
| status | string | 发送状态 |
| sent_at | string | 发送时间 |
| error_message | string? | 错误信息 |

## 错误码

| HTTP 状态码 | 说明 |
|-------------|------|
| 200 | 请求成功 |
| 400 | 请求参数错误 |
| 404 | 规则不存在 |
| 500 | 服务器内部错误 |
