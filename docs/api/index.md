# API 参考

TinyIoTHub 提供完整的 REST API，支持设备管理、数据采集、告警监控等功能。

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

## 设备管理

### 获取设备列表

```http
GET /api/v1/devices
```

### 创建设备

```http
POST /api/v1/devices
Authorization: Bearer <token>

{
  "name": "温度传感器",
  "sn": "SN001",
  "driver": "modbus_tcp",
  "config": {}
}
```

### 获取设备详情

```http
GET /api/v1/devices/{id}
```

### 更新设备

```http
PUT /api/v1/devices/{id}
```

### 删除设备

```http
DELETE /api/v1/devices/{id}
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
