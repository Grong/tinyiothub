# TinyIoTHub API 文档

## 概述

本项目提供了一套完整的云端 SaaS IoT RESTful API，采用业务域驱动的架构设计，支持设备管理、用户认证、告警处理、系统监控等核心功能。

## 架构特点

### 🏗️ 业务域驱动设计
- **模块化架构** - 按业务域组织API端点
- **清晰的职责分离** - 每个模块专注特定业务功能
- **易于扩展** - 新功能可轻松添加到对应业务域

### 🔒 安全认证
- **JWT Token认证** - 基于标准JWT的用户认证
- **中间件保护** - 所有API端点都受认证中间件保护
- **角色权限控制** - 基于角色的访问控制(RBAC)

### 📊 统一响应格式
```json
{
  "code": 0,
  "msg": "",
  "result": { ... }
}
```

- `code`: `0` 表示成功，非零表示错误
- `msg`: 错误信息，成功时为空字符串
- `result`: 实际数据，错误时为 `null`

## API 端点总览

### 🔐 认证相关 (`/api/v1/auth`)

#### POST /api/auth/login
用户登录认证

**请求体:**
```json
{
  "username": "admin",
  "password": "password123"
}
```

**响应:**
```json
{
  "code": 0,
  "msg": "",
  "result": {
    "access_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
    "token_type": "Bearer",
    "expires_in": 86400,
    "user_info": {
      "id": "user-id",
      "name": "管理员",
      "username": "admin",
      "email": "admin@example.com"
    }
  }
}
```

#### POST /api/auth/logout
用户登出

#### GET /api/auth/session
获取当前会话信息

---

### 🔧 设备管理 (`/api/v1/devices`)

#### GET /api/v1/devices
获取设备列表

**查询参数:**
- `name` - 设备名称筛选
- `device_type` - 设备类型筛选
- `driver_name` - 驱动名称筛选
- `state` - 设备状态筛选
- `product_id` - 产品ID筛选
- `enabled` - 是否启用筛选
- `page` - 页码 (默认: 1)
- `page_size` - 每页大小 (默认: 20)

**响应:**
```json
{
  "code": 0,
  "msg": "",
  "result": [
    {
      "id": "device-id",
      "name": "温度传感器01",
      "display_name": "一楼温度传感器",
      "device_type": "sensor",
      "address": "192.168.1.100",
      "driver_name": "modbus_rtu",
      "state": "online",
      "enabled": true,
      "created_at": "2024-01-01 10:00:00"
    }
  ]
}
```

#### POST /api/v1/devices
创建新设备

**请求体:**
```json
{
  "name": "温度传感器02",
  "displayName": "二楼温度传感器",
  "deviceType": "sensor",
  "address": "192.168.1.101",
  "driverName": "modbus_rtu",
  "connectionConfig": "{\"baudRate\": 9600}"
}
```

#### GET /api/v1/devices/:id
获取设备详情

#### PUT /api/v1/devices/:id
更新设备信息

#### DELETE /api/v1/devices/:id
删除设备

#### POST /api/v1/devices/:id/enable
启用设备

#### POST /api/v1/devices/:id/disable
禁用设备

#### GET /api/v1/devices/:id/status
获取设备状态

**响应:**
```json
{
  "code": 0,
  "msg": "",
  "result": {
    "device_id": "device-id",
    "online": true,
    "last_seen": "2024-01-01T10:30:00Z",
    "connection_status": "connected",
    "error_message": null
  }
}
```

#### GET /api/v1/devices/:id/data
读取设备数据

#### POST /api/v1/devices/:id/commands
执行设备命令

**请求体:**
```json
{
  "commandName": "read_temperature",
  "parameters": {
    "register": 1001
  }
}
```

#### GET /api/v1/devices/:id/properties
获取设备属性

---

### 🚨 告警管理 (`/api/v1/alarms`)

#### GET /api/v1/alarms
获取告警列表

**查询参数:**
- `device_id` - 设备ID筛选
- `level` - 告警级别筛选 (info, warning, error, critical)
- `status` - 告警状态筛选 (active, acknowledged, resolved)
- `start_time` - 开始时间筛选
- `end_time` - 结束时间筛选

#### GET /api/v1/alarms/statistics
获取告警统计信息

**响应:**
```json
{
  "code": 0,
  "msg": "",
  "result": {
    "total_count": 150,
    "active_count": 25,
    "acknowledged_count": 100,
    "resolved_count": 25,
    "by_level": {
      "critical": 5,
      "error": 20,
      "warning": 80,
      "info": 45
    }
  }
}
```

#### GET /api/v1/alarms/:id
获取告警详情

#### POST /api/v1/alarms/:id/acknowledge
确认告警

**请求体:**
```json
{
  "acknowledgedBy": "admin",
  "note": "已处理此告警"
}
```

#### POST /api/v1/alarms/batch/acknowledge
批量确认告警

**请求体:**
```json
{
  "alarmIds": ["alarm-1", "alarm-2"],
  "acknowledgedBy": "admin",
  "note": "批量处理告警"
}
```

#### GET /api/v1/alarm-rules
获取告警规则列表

#### POST /api/v1/alarm-rules
创建告警规则

#### GET /api/v1/alarm-rules/:id
获取告警规则详情

#### PUT /api/v1/alarm-rules/:id
更新告警规则

#### DELETE /api/v1/alarm-rules/:id
删除告警规则

#### POST /api/v1/alarm-rules/:id/toggle
启用/禁用告警规则

#### GET /api/v1/automations
获取自动化规则列表

#### POST /api/v1/automations
创建自动化规则

#### PUT /api/v1/automations/:id
更新自动化规则

#### DELETE /api/v1/automations/:id
删除自动化规则

#### GET /api/v1/events
获取事件列表

#### GET /api/v1/events/stream
SSE 事件流订阅

#### GET /api/v1/workspaces
获取工作空间列表

#### POST /api/v1/workspaces
创建工作空间

#### GET /api/v1/workspaces/:id
获取工作空间详情

#### PUT /api/v1/workspaces/:id
更新工作空间

#### POST /api/v1/workspaces/:id/devices
分配设备到工作空间

#### GET /api/v1/jobs
获取定时任务列表

#### POST /api/v1/jobs
创建定时任务

#### GET /api/v1/jobs/:id
获取任务详情

#### PUT /api/v1/jobs/:id
更新任务

#### DELETE /api/v1/jobs/:id
删除任务

#### POST /api/v1/jobs/:id/toggle
启用/禁用任务

#### GET /api/v1/jobs/:id/runs
获取任务执行记录

#### GET /api/v1/self-healing/probes
获取探针列表

#### POST /api/v1/self-healing/probes
创建探针

#### GET /api/v1/self-healing/status
获取自愈状态

#### GET /api/v1/notifications
获取通知列表

#### POST /api/v1/notifications/:id/read
标记通知已读

#### GET /api/v1/notification-channels
获取通知渠道

#### POST /api/v1/notification-channels
创建通知渠道

#### GET /api/v1/tenants
获取租户列表

---

### 👥 用户管理 (`/api/v1/users`)

#### GET /api/v1/users
获取用户列表

**查询参数:**
- `name` - 用户名筛选
- `email` - 邮箱筛选
- `enabled` - 是否启用筛选
- `role_id` - 角色ID筛选

#### POST /api/v1/users
创建新用户

**请求体:**
```json
{
  "name": "张三",
  "username": "zhangsan",
  "email": "zhangsan@example.com",
  "password": "password123",
  "enabled": true,
  "roleIds": ["role-1", "role-2"]
}
```

#### GET /api/v1/users/:id
获取用户详情

#### PUT /api/v1/users/:id
更新用户信息

#### DELETE /api/v1/users/:id
删除用户

#### GET /api/v1/users/roles
获取角色列表

#### POST /api/v1/users/roles
创建新角色

**请求体:**
```json
{
  "name": "设备管理员",
  "description": "负责设备管理的角色",
  "permissionIds": ["perm-1", "perm-2"]
}
```

#### GET /api/v1/users/permissions
获取权限列表

#### GET /api/v1/users/:id/permissions
获取用户权限

---

### ⚙️ 系统管理 (`/api/v1/system`)

#### GET /api/v1/system/products
获取产品列表

#### POST /api/v1/system/products
创建新产品

**请求体:**
```json
{
  "name": "智能温度传感器",
  "model": "TS-2024",
  "manufacturer": "科技公司",
  "description": "高精度温度传感器"
}
```

#### GET /api/v1/system/tasks
获取任务列表

#### GET /api/v1/system/configuration
获取系统配置

---

### 📊 监控相关 (`/api/v1/monitoring`)

#### GET /api/v1/monitoring/health
系统健康检查

**响应:**
```json
{
  "code": 0,
  "msg": "",
  "result": {
    "status": "healthy",
    "timestamp": "2024-01-01T10:00:00Z",
    "services": {
      "database": "healthy",
      "mqtt": "healthy",
      "device_drivers": "healthy"
    },
    "metrics": {
      "uptime": 86400,
      "memory_usage": 0.65,
      "cpu_usage": 0.25
    }
  }
}
```

#### GET /api/v1/monitoring/metrics
获取系统指标

#### GET /api/v1/monitoring/logs
获取系统日志

---

### 🔍 通用端点

#### GET /health
简单健康检查

**响应:** `OK`

## 认证机制

### JWT Token 使用

1. **获取Token**: 通过 `POST /api/auth/login` 获取访问令牌
2. **使用Token**: 在请求头中添加 `Authorization: Bearer <token>`
3. **Token刷新**: Token过期后需要重新登录获取新Token

### 示例请求

```bash
# 登录获取Token
curl -X POST http://localhost:3002/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"password123"}'

# 使用Token访问API
curl -X GET http://localhost:3002/api/v1/devices \
  -H "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..."
```

## 错误处理

### 标准错误响应格式

```json
{
  "code": -1,
  "msg": "错误描述信息",
  "result": null
}
```

### 常见HTTP状态码

- `200 OK` - 请求成功
- `400 Bad Request` - 请求参数错误
- `401 Unauthorized` - 未认证或Token无效
- `403 Forbidden` - 权限不足
- `404 Not Found` - 资源不存在
- `500 Internal Server Error` - 服务器内部错误

## 数据格式

### 时间格式
所有时间字段使用 ISO 8601 格式: `YYYY-MM-DD HH:MM:SS`

### 分页参数
- `page`: 页码，从1开始
- `page_size`: 每页大小，默认20，最大100

### 筛选参数
支持模糊匹配的字段使用 `LIKE %value%` 查询

## 开发指南

### 添加新API端点

1. **选择业务域**: 确定新功能属于哪个业务域
2. **创建处理函数**: 在对应模块中添加处理函数
3. **定义路由**: 在模块的 `create_router()` 中添加路由
4. **实现业务逻辑**: 调用相应的实体方法
5. **错误处理**: 使用统一的错误处理模式
6. **更新文档**: 在此文档中添加API说明

### 命名规范

- **函数名**: 使用动词短语，如 `create_device`, `list_users`
- **结构体**: 使用名词，如 `CreateDeviceRequest`, `UserInfo`
- **字段名**: 使用 `snake_case` (Rust) 和 `camelCase` (JSON)

### 响应格式标准

```rust
// 成功响应
ApiResponse::success(data)

// 错误响应
ApiResponse::error("错误信息".to_string())
```

## 部署说明

### 环境变量

- `RUST_LOG`: 日志级别 (默认: info)
- `DATABASE_URL`: 数据库连接字符串
- `JWT_SECRET`: JWT签名密钥
- `SERVER_PORT`: 服务器端口 (默认: 3002)

### 启动命令

```bash
# 开发环境
cargo run

# 生产环境
cargo build --release
./target/release/tinyiothub
```

## 版本信息

- **当前版本**: v1.1.0
- **API版本**: v1
- **兼容性**: 向后兼容

## 更新日志

### v1.1.0 (2026-04)
- ✅ Cron 定时任务重构：统一 CronSchedulerService，Workspace 隔离
- ✅ 告警规则引擎：支持阈值、范围、变化、持续时间、组合五种条件
- ✅ 工作空间管理：物理环境分组，AI Agent 绑定
- ✅ 事件系统：SSE 流式推送，设备联动
- ✅ 自愈引擎：system/device/task 三级探针
- ✅ 前端迁移：Next.js → Lit 3 + Vite + nanostore
- ✅ AI Agent 架构：内嵌 MCP Server + A2UI 聊天界面

### v1.0.0 (2025-01)
- ✅ 完全重构API架构，采用业务域驱动设计
- ✅ 实现完整的用户认证和权限管理
- ✅ 建立标准化的错误处理和响应格式
- ✅ 支持设备管理、告警处理、系统监控等核心功能

---

**维护团队**: TinyIoTHub 开发团队
**最后更新**: 2026-04-19
**文档版本**: v1.1.0