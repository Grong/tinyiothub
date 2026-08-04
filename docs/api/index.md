# API 参考

TinyIoTHub 提供完整的 REST API，支持物（Thing）管理、数据采集、告警监控等功能。

## 基础信息

| 项目 | 说明 |
|------|------|
| 基础 URL | `http://localhost:3002/api/v1/` |
| 认证方式 | JWT Token |
| 响应格式 | 统一 JSON 格式 |

## 统一响应格式

所有 API 响应遵循统一格式：

```json
{
  "code": 0,
  "msg": "",
  "result": { }
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| code | integer | 0=成功，非0=错误 |
| msg | string | 错误信息，成功时为空 |
| result | object | 实际数据 |

## 认证接口

### 登录

```http
POST /api/v1/auth/login
Content-Type: application/json

{
  "username": "admin",
  "password": "admin123"
}
```

### 登出

```http
POST /api/v1/auth/logout
Authorization: Bearer <token>
```

### 获取会话

```http
GET /api/v1/auth/session
Authorization: Bearer <token>
```

## 物管理

> 管理面的"设备"概念已更名为"物"（Thing）。原 `/api/v1/devices` 管理端点已删除（返回 410 Gone），请改用 `/api/v1/things`。运行时数据面（属性、状态、追踪等）仍保留 `/api/v1/devices/{id}/...` 端点。所有管理面 API 按 workspace 作用域。

### 获取物列表

```http
GET /api/v1/things
```

**查询参数：** `thing_type`、`parent_id`、`tags`、`q`（搜索）、`limit`、`offset`

### 创建物

```http
POST /api/v1/things
Authorization: Bearer <token>

{
  "name": "温度传感器",
  "thing_type": "sensor",
  "parent_id": null,
  "driver_name": "modbus_tcp"
}
```

### 获取物详情

```http
GET /api/v1/things/{id}
```

### 更新物

```http
PUT /api/v1/things/{id}
```

### 删除物

```http
DELETE /api/v1/things/{id}
```

### 获取物层级树

```http
GET /api/v1/things/{id}/tree?depth=3
```

### 调用物动作

```http
POST /api/v1/things/{id}/actions/{action_name}/invoke
```

## 驱动管理

### 获取驱动列表

```http
GET /api/v1/drivers
```

### 获取驱动详情

```http
GET /api/v1/drivers/{name}
```

### 获取支持的驱动名称

```http
GET /api/v1/drivers/names
```

## 告警管理

### 获取告警列表

```http
GET /api/v1/alarms
```

### 确认告警

```http
POST /api/v1/alarms/{id}/acknowledge
```

### 获取告警规则

```http
GET /api/v1/alarms/rules
```

## 系统管理

### 健康检查

```http
GET /api/v1/system/health
```

### 获取系统特性

```http
GET /api/v1/system/features
```

### 获取系统配置

```http
GET /api/v1/system/config
```
