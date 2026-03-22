# 通知渠道 API

## 概述

通知渠道 API 提供 SMS、邮件、Webhook 等通知渠道的增删改查和测试功能。每个渠道对应一种通知发送方式。

## 接口列表

### 获取通知渠道列表

```
GET /api/v1/notification-channels
```

**查询参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| channel_type | string | 否 | 渠道类型：sms、email、webhook |
| enabled | boolean | 否 | 是否启用 |
| page | number | 否 | 页码，默认 1 |
| page_size | number | 否 | 每页数量，默认 20 |

**响应示例：**

```json
{
  "success": true,
  "result": [
    {
      "id": "channel_001",
      "name": "管理员SMS",
      "channel_type": "sms",
      "config": "{\"provider\":\"aliyun\",\"sign_name\":\"TinyIoT\",\"template_code\":\"SMS_12345\"}",
      "is_enabled": true,
      "is_verified": true,
      "last_test_at": "2024-01-07 10:00:00",
      "created_at": "2024-01-01 10:00:00",
      "updated_at": "2024-01-07 10:00:00"
    },
    {
      "id": "channel_002",
      "name": "告警邮件",
      "channel_type": "email",
      "config": "{\"smtp_host\":\"smtp.example.com\",\"smtp_port\":465,\"from\":\"alarm@example.com\"}",
      "is_enabled": true,
      "is_verified": true,
      "last_test_at": "2024-01-06 15:00:00",
      "created_at": "2024-01-01 10:00:00",
      "updated_at": "2024-01-06 15:00:00"
    }
  ]
}
```

---

### 获取渠道详情

```
GET /api/v1/notification-channels/{id}
```

---

### 创建通知渠道

```
POST /api/v1/notification-channels
```

**请求体：**

```json
{
  "name": "管理员SMS",
  "channel_type": "sms",
  "config": "{\"provider\":\"aliyun\",\"access_key\":\"your_access_key\",\"access_secret\":\"your_access_secret\",\"sign_name\":\"TinyIoT\",\"template_code\":\"SMS_12345\"}",
  "is_enabled": true
}
```

**渠道类型说明：**

| 类型 | 说明 | 配置字段 |
|------|------|----------|
| sms | 短信通知 | provider、access_key、sign_name、template_code |
| email | 邮件通知 | smtp_host、smtp_port、from、username、password |
| webhook | Webhook | url、method、headers |

**SMS 配置示例：**

```json
{
  "provider": "aliyun",
  "access_key": "your_access_key",
  "access_secret": "your_access_secret",
  "sign_name": "TinyIoT",
  "template_code": "SMS_12345"
}
```

**Email 配置示例：**

```json
{
  "smtp_host": "smtp.example.com",
  "smtp_port": 465,
  "use_ssl": true,
  "from": "alarm@example.com",
  "username": "alarm@example.com",
  "password": "your_password"
}
```

**Webhook 配置示例：**

```json
{
  "url": "https://hooks.example.com/notify",
  "method": "POST",
  "headers": {
    "Authorization": "Bearer your_token"
  },
  "content_type": "application/json"
}
```

---

### 更新通知渠道

```
PUT /api/v1/notification-channels/{id}
```

**请求体（支持部分更新）：**

```json
{
  "name": "管理员SMS（已修改）",
  "is_enabled": false
}
```

---

### 删除通知渠道

```
DELETE /api/v1/notification-channels/{id}
```

**响应：** `204 No Content`

---

### 启用通知渠道

```
POST /api/v1/notification-channels/{id}/enable
```

---

### 禁用通知渠道

```
POST /api/v1/notification-channels/{id}/disable
```

---

### 测试通知渠道

```
POST /api/v1/notification-channels/{id}/test
```

向指定渠道发送测试消息，验证配置是否正确。

**请求体：**

```json
{
  "recipient": "13800138000",
  "message": "这是一条测试通知，用于验证渠道配置"
}
```

**响应示例（成功）：**

```json
{
  "success": true,
  "result": {
    "success": true,
    "message": "测试消息发送成功"
  }
}
```

**响应示例（失败）：**

```json
{
  "success": true,
  "result": {
    "success": false,
    "error": "短信发送失败：签名验证失败"
  }
}
```

---

### 获取渠道统计

```
GET /api/v1/notification-channels/statistics
```

**响应示例：**

```json
{
  "success": true,
  "result": {
    "total_channels": 5,
    "enabled_channels": 4,
    "disabled_channels": 1,
    "by_type": {
      "sms": 2,
      "email": 2,
      "webhook": 1
    },
    "total_sent": 1250,
    "total_failed": 15,
    "success_rate": 0.988
  }
}
```

## 渠道数据结构

### NotificationChannel

| 字段 | 类型 | 说明 |
|------|------|------|
| id | string | 渠道 ID |
| name | string | 渠道名称 |
| channel_type | string | 渠道类型：sms、email、webhook |
| config | string | 渠道配置（JSON，敏感信息可能加密） |
| is_enabled | boolean | 是否启用 |
| is_verified | boolean | 是否已验证 |
| last_test_at | string? | 最后测试时间 |
| created_at | string | 创建时间 |
| updated_at | string | 更新时间 |

## 使用场景

### 1. 配置阿里云短信通知

```json
POST /api/v1/notification-channels
{
  "name": "阿里云短信",
  "channel_type": "sms",
  "config": JSON.stringify({
    "provider": "aliyun",
    "access_key": "your_access_key",
    "access_secret": "your_access_secret",
    "sign_name": "TinyIoT",
    "template_code": "SMS_123456789"
  }),
  "is_enabled": true
}
```

### 2. 测试并启用 Webhook

```javascript
// 先测试 Webhook 配置
const testResult = await fetch('/api/v1/notification-channels/channel_003/test', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({
    "message": "TinyIoTHub Webhook 测试"
  })
});

// 启用渠道
await fetch('/api/v1/notification-channels/channel_003/enable', {
  method: 'POST'
});
```

## 错误码

| HTTP 状态码 | 说明 |
|-------------|------|
| 200 | 请求成功 |
| 204 | 删除成功（无内容） |
| 400 | 无效的渠道类型或配置 JSON |
| 404 | 渠道不存在 |
| 500 | 服务器内部错误 |
