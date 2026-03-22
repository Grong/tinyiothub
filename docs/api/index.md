# API 参考

TinyIoTHub 提供完整的 REST API，支持设备管理、数据采集、告警监控、自动化规则、通知管理等功能。

## 基础信息

| 项目 | 说明 |
|------|------|
| 基础 URL | `http://localhost:3002/api/v1/` |
| Open API 基础 URL | `http://localhost:3002/open/` |
| 认证方式 | JWT Token / API Key |
| 响应格式 | 统一 JSON 格式 |

## 统一响应格式

```json
// 成功
{
  "success": true,
  "result": { ... }
}

// 错误
{
  "success": false,
  "message": "错误信息"
}
```

## API 文档索引

### 核心模块

| 模块 | 文档 | 说明 |
|------|------|------|
| 认证 | [overview](./overview) | 认证与会话 API 总览 |
| 设备 | [overview](./overview) | 设备管理 API |
| 设备 Profile | [device-profile](./device-profile) | 设备完整配置接口 |
| 设备追踪 | [device-trace](./device-trace) | 设备追踪与调试接口 |
| MQTT | [mqtt](./mqtt) | MQTT 协议通信 |

### 功能模块

| 模块 | 文档 | 说明 |
|------|------|------|
| 告警 | [overview](./overview) | 告警查询 API |
| 告警规则 | [alarm-rules](./alarm-rules) | 告警规则 CRUD |
| 自动化规则 | [automations](./automations) | 自动化规则管理 |
| 设备模板 | [templates](./templates) | 设备模板管理 |
| 驱动管理 | [drivers-api](./drivers-api) | 驱动查询与动态加载 |
| 事件 | [events](./events) | 事件查询与实时推送 |
| 通知管理 | [notifications](./notifications) | 通知规则与历史 |
| 通知渠道 | [notification-channels](./notification-channels) | SMS/邮件/Webhook 渠道 |
| 网关管理 | [gateways](./gateways) | 边缘网关管理 |
| 定时任务 | [jobs](./jobs) | 定时任务管理 |
| 用户管理 | [users-api](./users-api) | 用户、角色、权限管理 |
| 市场 | [marketplace](./marketplace) | 驱动和模板市场 |
| 系统管理 | [system](./system) | 系统配置与产品管理 |
| Open API | [open-api](./open-api) | 面向第三方的开放 API |

### 参考资料

| 文档 | 说明 |
|------|------|
| [设备 API 总结](./DEVICE_API_SUMMARY) | 设备相关 API 实现总结 |

## 认证方式

### JWT Token（受保护 API）

```http
POST /api/v1/auth/login
Content-Type: application/json

{
  "username": "admin",
  "password": "admin123"
}
```

获取 Token 后，在请求头中携带：

```http
Authorization: Bearer <token>
```

### API Key（Open API）

```http
X-API-Key: your_api_key_here
```

## 常见错误码

| HTTP 状态码 | 说明 |
|-------------|------|
| 200 | 请求成功 |
| 400 | 请求参数错误 |
| 401 | 未认证 |
| 403 | 权限不足 |
| 404 | 资源不存在 |
| 500 | 服务器内部错误 |
