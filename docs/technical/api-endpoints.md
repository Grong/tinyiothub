# API 端点总览

本文档列出 TinyIoTHub 后端所有已实现的 API 端点，按模块分组。

## 基础信息

| 项目 | 说明 |
|------|------|
| 基础 URL | `http://localhost:3002/api/v1/` |
| Open API 基础 URL | `http://localhost:3002/open/` |
| 认证方式 | JWT Token（受保护端点）/ API Key（Open API） |
| 响应格式 | JSON |

---

## 认证模块 (`/api/v1/auth/`)

| 方法 | 路径 | 说明 | 认证 |
|------|------|------|------|
| POST | `/api/v1/auth/login` | 用户登录 | 否 |
| POST | `/api/v1/auth/logout` | 用户登出 | JWT |
| GET | `/api/v1/auth/session` | 获取会话信息 | JWT |
| POST | `/api/v1/auth/sms/send` | 发送短信验证码 | 否 |
| POST | `/api/v1/auth/sms/verify` | 验证短信验证码 | 否 |
| POST | `/api/v1/auth/social/{provider}/callback` | 第三方登录回调 | 否 |
| POST | `/api/v1/auth/token/refresh` | 刷新 Token | JWT |

---

## 用户管理模块 (`/api/v1/users/`)

### 用户 CRUD

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/users/management/users` | 获取用户列表 |
| POST | `/api/v1/users/management/users` | 创建用户 |
| GET | `/api/v1/users/management/users/{id}` | 获取用户详情 |
| PUT | `/api/v1/users/management/users/{id}` | 更新用户 |
| DELETE | `/api/v1/users/management/users/{id}` | 删除用户 |

### 角色管理

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/users/roles` | 获取角色列表 |
| POST | `/api/v1/users/roles` | 创建角色 |
| GET | `/api/v1/users/roles/{id}` | 获取角色详情 |
| PUT | `/api/v1/users/roles/{id}` | 更新角色 |
| DELETE | `/api/v1/users/roles/{id}` | 删除角色 |
| GET | `/api/v1/users/roles/{id}/permissions` | 获取角色权限 |

### 权限管理

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/users/permissions` | 获取权限列表 |
| GET | `/api/v1/users/permissions/user/{user_id}` | 获取用户权限 |
| GET | `/api/v1/users/permissions/role/{role_id}` | 获取角色权限 |

---

## 设备管理模块 (`/api/v1/devices/`)

### 设备 CRUD

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/devices` | 获取设备列表 |
| POST | `/api/v1/devices` | 创建设备 |
| POST | `/api/v1/devices/from-template` | 使用模板创建设备 |
| GET | `/api/v1/devices/{id}` | 获取设备详情 |
| PUT | `/api/v1/devices/{id}` | 更新设备 |
| DELETE | `/api/v1/devices/{id}` | 删除设备 |
| POST | `/api/v1/devices/{id}/enable` | 启用设备 |
| POST | `/api/v1/devices/{id}/disable` | 禁用设备 |
| GET | `/api/v1/devices/validate` | 验证设备配置 |
| POST | `/api/v1/devices/preview` | 预览设备配置 |

### 设备属性

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/devices/{id}/properties` | 获取设备属性列表 |
| GET | `/api/v1/devices/{id}/properties/{property_id}` | 获取属性详情 |
| PUT | `/api/v1/devices/{id}/properties/{property_id}` | 更新属性值 |
| GET | `/api/v1/devices/{id}/properties/{property_id}/history` | 获取属性历史 |

### 设备指令

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/devices/{id}/commands` | 获取设备指令列表 |
| POST | `/api/v1/devices/{id}/commands/{command_id}/execute` | 执行设备指令 |
| GET | `/api/v1/devices/{id}/command-executions` | 获取指令执行历史 |

### 设备配置文件

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/devices/{id}/profile` | 获取设备完整配置文件 |

### 设备追踪

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/devices/{id}/traces` | 获取追踪记录 |
| POST | `/api/v1/devices/{id}/traces` | 创建追踪记录 |
| GET | `/api/v1/devices/{id}/traces/statistics` | 获取追踪统计 |
| GET | `/api/v1/devices/{id}/traces/performance` | 获取性能指标 |
| GET | `/api/v1/devices/{id}/traces/export` | 导出追踪记录 |
| POST | `/api/v1/devices/{id}/traces/clear` | 清理追踪记录 |

### 设备数据

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/devices/{id}/data` | 获取设备数据 |
| GET | `/api/v1/devices/{id}/data/latest` | 获取最新数据 |
| GET | `/api/v1/devices/{id}/data/history` | 获取历史数据 |

### 设备监控

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/devices/{id}/status` | 获取设备状态 |
| GET | `/api/v1/devices/{id}/monitoring/realtime` | 获取实时监控数据 |

### 设备仪表盘

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/devices/dashboard/overview` | 获取设备概览 |
| GET | `/api/v1/devices/dashboard/stats` | 获取设备统计 |

---

## 驱动管理模块 (`/api/v1/drivers/`)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/drivers` | 获取驱动列表 |
| GET | `/api/v1/drivers/names` | 获取驱动名称列表 |
| GET | `/api/v1/drivers/{name}` | 获取驱动详情 |
| GET | `/api/v1/drivers/{name}/config` | 获取驱动配置参数 |
| GET | `/api/v1/drivers/{name}/supported` | 检查驱动支持状态 |
| POST | `/api/v1/drivers/dynamic/load` | 动态加载驱动 |
| DELETE | `/api/v1/drivers/dynamic/{name}/unload` | 动态卸载驱动 |
| GET | `/api/v1/drivers/dynamic/list` | 列出所有动态驱动 |
| POST | `/api/v1/drivers/dynamic/reload` | 重新加载驱动目录 |

---

## 告警模块 (`/api/v1/alarms/`)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/alarms` | 获取告警列表 |
| GET | `/api/v1/alarms/statistics` | 获取告警统计 |
| GET | `/api/v1/alarms/{id}` | 获取告警详情 |
| POST | `/api/v1/alarms/{id}/acknowledge` | 确认告警 |
| POST | `/api/v1/alarms/{id}/resolve` | 解决告警 |
| POST | `/api/v1/alarms/batch-acknowledge` | 批量确认告警 |
| POST | `/api/v1/alarms/batch-resolve` | 批量解决告警 |

---

## 告警规则模块 (`/api/v1/alarm-rules/`)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/alarm-rules` | 获取告警规则列表 |
| POST | `/api/v1/alarm-rules` | 创建告警规则 |
| GET | `/api/v1/alarm-rules/{id}` | 获取告警规则详情 |
| PUT | `/api/v1/alarm-rules/{id}` | 更新告警规则 |
| DELETE | `/api/v1/alarm-rules/{id}` | 删除告警规则 |
| POST | `/api/v1/alarm-rules/{id}/toggle` | 启停告警规则 |

---

## 自动化模块 (`/api/v1/automations/`)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/automations` | 获取自动化列表 |
| POST | `/api/v1/automations` | 创建自动化规则 |
| GET | `/api/v1/automations/{id}` | 获取自动化详情 |
| PUT | `/api/v1/automations/{id}` | 更新自动化规则 |
| DELETE | `/api/v1/automations/{id}` | 删除自动化规则 |
| POST | `/api/v1/automations/{id}/enable` | 启用自动化 |
| POST | `/api/v1/automations/{id}/disable` | 禁用自动化 |
| POST | `/api/v1/automations/{id}/run` | 手动执行自动化 |
| POST | `/api/v1/automations/{id}/test` | 测试自动化条件 |
| GET | `/api/v1/automations/statistics` | 获取自动化统计 |

---

## 事件模块 (`/api/v1/events/`)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/events` | 获取事件列表 |
| POST | `/api/v1/events` | 创建事件 |
| GET | `/api/v1/events/real-time` | 获取实时事件 |
| GET | `/api/v1/events/real-time/status` | 获取实时事件状态 |
| POST | `/api/v1/events/real-time/{id}/acknowledge` | 确认实时事件 |
| GET | `/api/v1/events/overview` | 获取事件总览 |
| GET | `/api/v1/events/security/permissions` | 获取用户权限 |
| GET | `/api/v1/events/security/config` | 获取安全配置 |
| PUT | `/api/v1/events/security/config` | 更新安全配置 |
| GET | `/api/v1/events/security/roles` | 获取用户角色 |
| GET | `/api/v1/events/security/audit-logs` | 获取用户审计日志 |
| GET | `/api/v1/events/security/audit-logs/all` | 获取所有审计日志 |
| POST | `/api/v1/events/security/cleanup` | 清理审计日志 |
| GET | `/api/v1/events/performance/...` | 性能监控端点 |
| GET | `/api/v1/events/sse` | SSE 实时推送 |
| GET | `/api/v1/events/sse/overview` | SSE 概览 |
| GET | `/api/v1/events/sse/connections` | SSE 连接列表 |

---

## 网关管理模块 (`/api/v1/gateways/`)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/gateways` | 获取网关列表 |
| POST | `/api/v1/gateways` | 创建网关 |
| GET | `/api/v1/gateways/{id}` | 获取网关详情 |
| PUT | `/api/v1/gateways/{id}` | 更新网关 |
| DELETE | `/api/v1/gateways/{id}` | 删除网关 |
| GET | `/api/v1/gateways/{id}/devices` | 获取网关设备列表 |
| PUT | `/api/v1/gateways/{id}/status` | 更新网关状态 |

---

## 定时任务模块 (`/api/v1/jobs/`)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/jobs` | 获取任务列表 |
| POST | `/api/v1/jobs` | 创建任务 |
| GET | `/api/v1/jobs/{id}` | 获取任务详情 |
| PUT | `/api/v1/jobs/{id}` | 更新任务 |
| DELETE | `/api/v1/jobs/{id}` | 删除任务 |
| POST | `/api/v1/jobs/{id}/enable` | 启用任务 |
| POST | `/api/v1/jobs/{id}/disable` | 禁用任务 |
| POST | `/api/v1/jobs/{id}/run` | 手动执行任务 |
| GET | `/api/v1/jobs/{id}/executions` | 获取任务执行记录 |
| GET | `/api/v1/jobs/statistics` | 获取任务统计 |
| GET | `/api/v1/executions` | 获取全部执行记录 |

---

## 通知管理模块 (`/api/v1/notifications/`)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/notifications/rules` | 获取通知规则列表 |
| POST | `/api/v1/notifications/rules` | 创建通知规则 |
| GET | `/api/v1/notifications/rules/{rule_id}` | 获取通知规则详情 |
| PUT | `/api/v1/notifications/rules/{rule_id}` | 更新通知规则 |
| DELETE | `/api/v1/notifications/rules/{rule_id}` | 删除通知规则 |
| GET | `/api/v1/notifications/history` | 获取通知历史 |
| POST | `/api/v1/notifications/test` | 发送测试通知 |

---

## 通知渠道模块 (`/api/v1/notification-channels/`)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/notification-channels` | 获取渠道列表 |
| POST | `/api/v1/notification-channels` | 创建渠道 |
| GET | `/api/v1/notification-channels/{id}` | 获取渠道详情 |
| PUT | `/api/v1/notification-channels/{id}` | 更新渠道 |
| DELETE | `/api/v1/notification-channels/{id}` | 删除渠道 |
| POST | `/api/v1/notification-channels/{id}/enable` | 启用渠道 |
| POST | `/api/v1/notification-channels/{id}/disable` | 禁用渠道 |
| POST | `/api/v1/notification-channels/{id}/test` | 测试渠道 |
| GET | `/api/v1/notification-channels/statistics` | 获取渠道统计 |

---

## 设备模板模块 (`/api/v1/device-templates/`)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/device-templates` | 获取模板列表 |
| POST | `/api/v1/device-templates` | 创建模板 |
| GET | `/api/v1/device-templates/{id}` | 获取模板详情 |
| PUT | `/api/v1/device-templates/{id}` | 更新模板 |
| DELETE | `/api/v1/device-templates/{id}` | 删除模板 |
| POST | `/api/v1/device-templates/validate` | 验证模板配置 |
| POST | `/api/v1/device-templates/preview` | 预览模板配置 |

---

## 市场模块 (`/api/v1/marketplace/`)

### 模板市场

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/marketplace/templates` | 获取市场模板列表 |
| GET | `/api/v1/marketplace/templates/{id}` | 获取市场模板详情 |
| POST | `/api/v1/marketplace/templates/{id}/install` | 安装市场模板 |

### 驱动市场

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/marketplace/drivers` | 获取市场驱动列表 |
| GET | `/api/v1/marketplace/drivers/{id}` | 获取市场驱动详情 |
| POST | `/api/v1/marketplace/drivers/{id}/install` | 安装市场驱动 |

---

## 系统管理模块 (`/api/v1/system/`)

### 系统配置

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/system/configuration` | 获取系统配置 |
| PUT | `/api/v1/system/configuration` | 更新系统配置 |

### 系统特性

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/system/features` | 获取系统特性列表 |

### 系统初始化

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/v1/system/initialization/init` | 系统初始化 |

### 任务管理

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/system/tasks` | 获取任务列表 |
| GET | `/api/v1/system/tasks/{id}` | 获取任务详情 |

### 产品管理

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/system/products` | 获取产品列表 |
| POST | `/api/v1/system/products` | 创建产品 |
| GET | `/api/v1/system/products/{id}` | 获取产品详情 |
| PUT | `/api/v1/system/products/{id}` | 更新产品 |
| DELETE | `/api/v1/system/products/{id}` | 删除产品 |

---

## 租户管理模块 (`/api/v1/tenants/`)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/tenants` | 获取租户列表 |
| POST | `/api/v1/tenants` | 创建租户 |
| GET | `/api/v1/tenants/{id}` | 获取租户详情 |
| PUT | `/api/v1/tenants/{id}` | 更新租户 |
| DELETE | `/api/v1/tenants/{id}` | 删除租户 |
| GET | `/api/v1/tenants/{id}/api-keys` | 获取 API Key 列表 |
| POST | `/api/v1/tenants/{id}/api-keys` | 创建 API Key |
| DELETE | `/api/v1/tenants/{id}/api-keys/{key_id}` | 撤销 API Key |

### 租户认证

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/v1/tenants/auth/register` | 租户注册 |
| POST | `/api/v1/tenants/auth/login` | 租户登录 |

---

## 监控模块 (`/api/v1/monitoring/`)

### 指标

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/monitoring/metrics` | 获取系统指标 |
| GET | `/api/v1/monitoring/metrics/{name}` | 获取指定指标 |

### 健康检查

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/monitoring/health` | 健康检查 |
| GET | `/api/v1/monitoring/health/{component}` | 组件健康检查 |

### 日志

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/monitoring/logs` | 获取系统日志 |

### 仪表盘

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/monitoring/dashboard` | 获取监控仪表盘数据 |

---

## 标签管理模块 (`/api/v1/tags/`)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/tags` | 获取标签列表 |
| POST | `/api/v1/tags` | 创建标签 |
| PUT | `/api/v1/tags/{id}` | 更新标签 |
| DELETE | `/api/v1/tags/{id}` | 删除标签 |

---

## Open API 模块 (`/open/`)

Open API 使用 API Key 认证，适用于第三方平台集成和 AI 接入。

| 方法 | 路径 | 说明 | 认证 |
|------|------|------|------|
| GET | `/open/health` | 健康检查 | API Key |
| GET | `/open/devices` | 获取设备列表 | API Key |
| GET | `/open/devices/{id}` | 获取设备详情 | API Key |
| GET | `/open/devices/{id}/properties` | 获取设备属性 | API Key |
| GET | `/open/devices/{id}/commands` | 获取设备指令列表 | API Key |
| POST | `/open/devices/{id}/command` | 发送设备指令 | API Key |
| GET | `/open/devices/{id}/events` | 获取设备事件 | API Key |
| GET | `/open/events` | 获取全部事件 | API Key |

---

## 全局端点

| 方法 | 路径 | 说明 | 认证 |
|------|------|------|------|
| GET | `/api/v1/health` | 健康检查 | 否 |
| GET | `/api/v1/test-auth` | 测试认证 | JWT |
| GET | `/api/v1/events/sse/public` | 公开 SSE 端点 | 否 |

---

## HTTP 状态码

| 状态码 | 说明 |
|--------|------|
| 200 | 请求成功 |
| 201 | 资源创建成功 |
| 204 | 删除成功（无内容） |
| 400 | 请求参数错误 |
| 401 | 未认证或认证失败 |
| 403 | 权限不足 |
| 404 | 资源不存在 |
| 409 | 资源冲突（如任务已在运行） |
| 429 | 请求频率超限 |
| 500 | 服务器内部错误 |

---

## 统一响应格式

```json
// 成功响应
{
  "success": true,
  "result": { ... }
}

// 错误响应
{
  "success": false,
  "message": "错误描述"
}
```
