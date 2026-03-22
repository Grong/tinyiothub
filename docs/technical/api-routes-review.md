# TinyIoTHub API 路由规范评审报告

> 评审日期：2026-03-22  
> 项目路径：`C:\Users\59328\.openclaw\workspace\tinyiothub`

---

## 一、当前所有 API 路由列表

### 1.1 顶层路由（Root）

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/health` | 健康检查（无需认证） |

### 1.2 V1 版本路由前缀：`/v1`

#### 1.2.1 认证模块 `/v1/auth`

**公开路由（无需认证）：**
| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/v1/auth/login` | 用户登录 |
| POST | `/v1/auth/logout` | 用户登出 |
| POST | `/v1/auth/sms/...` | 短信验证码登录（嵌套路由） |
| POST | `/v1/auth/social/...` | 第三方登录（嵌套路由） |

**会话路由（需认证）：**
| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/v1/auth/session/profile` | 获取当前用户信息 |
| POST | `/v1/auth/session/refresh` | 刷新 Token |
| GET | `/v1/auth/session/validate` | 验证会话有效性 |

#### 1.2.2 租户认证 `/v1/tenants`（公开）

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/v1/tenants/register` | 租户注册 |
| POST | `/v1/tenants/login` | 租户登录 |
| GET | `/v1/tenants/verify` | 验证 Token |
| GET | `/v1/tenants/plans` | 查询订阅套餐 |

#### 1.2.3 系统管理 `/v1/system`

嵌套模块：configuration, features, initialization, tasks, products

#### 1.2.4 标签 `/v1/tags`

#### 1.2.5 事件模块 `/v1/events`

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/v1/events/` | 查询事件列表 |
| POST | `/v1/events/` | 创建事件 |
| GET | `/v1/events/real-time` | 实时事件列表 |
| GET | `/v1/events/real-time/status` | 实时状态摘要 |
| POST | `/v1/events/real-time/:id/acknowledge` | 确认事件 |
| GET | `/v1/events/overview` | 事件概览统计 |
| GET | `/v1/events/security/permissions` | 用户事件权限 |
| GET | `/v1/events/security/config` | 安全配置查询 |
| PUT | `/v1/events/security/config` | 安全配置更新 |
| GET | `/v1/events/security/roles` | 用户角色 |
| GET | `/v1/events/security/audit-logs/:event_id` | 特定事件审计日志 |
| GET | `/v1/events/security/audit-logs` | 当前用户审计日志 |
| GET | `/v1/events/security/audit-logs/all` | 所有审计日志（管理员） |
| POST | `/v1/events/security/cleanup` | 清理审计日志 |
| GET | `/v1/events/performance/metrics` | 性能指标 |
| GET | `/v1/events/performance/summary` | 性能摘要 |
| GET | `/v1/events/performance/alerts` | 性能告警列表 |
| GET | `/v1/events/performance/optimize` | 数据库优化 |
| GET | `/v1/events/performance/load-balancer/stats` | 负载均衡统计 |
| GET | `/v1/events/performance/load-balancer/config` | 负载均衡配置 |
| GET | `/v1/events/performance/thresholds` | 性能阈值 |
| GET | `/v1/events/performance/recommendations` | 优化建议 |
| GET | `/v1/events/performance/query-analysis` | 查询分析 |
| GET | `/v1/events/sse` | SSE 连接 |
| GET | `/v1/events/sse/overview` | SSE 连接概览 |
| GET | `/v1/events/sse/connections` | SSE 连接列表 |
| GET | `/v1/events/sse/public` | 公开 SSE 连接（无需认证） |

嵌套子模块：`overview`, `performance`, `real_time`, `security`, `sse`

#### 1.2.6 设备模块 `/v1/devices`

**管理子模块 `/v1/devices/`：**
| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/v1/devices/` | 获取设备列表 |
| POST | `/v1/devices/` | 创建设备 |
| GET | `/v1/devices/:id` | 获取设备详情 |
| PUT | `/v1/devices/:id` | 更新设备 |
| DELETE | `/v1/devices/:id` | 删除设备 |
| POST | `/v1/devices/:id/enable` | 启用设备 |
| POST | `/v1/devices/:id/disable` | 禁用设备 |
| POST | `/v1/devices/from-template` | 基于模板创建设备 |
| POST | `/v1/devices/from-template/:template_id/preview` | 预览设备创建 |
| POST | `/v1/devices/from-template/:template_id/validate` | 验证设备输入 |
| GET | `/v1/devices/from-template/:template_id/requirements` | 获取模板需求 |
| POST | `/v1/devices/from-template/:template_id/validate-field` | 验证单个字段 |

**属性子模块 `/v1/devices/:device_id/properties`**
**命令子模块 `/v1/devices/:device_id/commands/:command_id/execute`**
**数据子模块 `/v1/devices/:device_id/data*`**
**监控子模块 `/v1/monitoring/:device_id/*`**
**追踪子模块 `/v1/devices/:device_id/traces*`**
**配置子模块 `/v1/devices/:device_id/profile`**

嵌套子模块：management, properties, commands, dashboard, profile, trace, monitoring, data

#### 1.2.7 驱动模块 `/v1/drivers`

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/v1/drivers/` | 获取驱动列表 |
| GET | `/v1/drivers/names` | 获取驱动名称列表 |
| GET | `/v1/drivers/:name` | 获取驱动详情 |
| GET | `/v1/drivers/:name/config` | 获取驱动配置参数 |
| GET | `/v1/drivers/:name/supported` | 检查驱动支持状态 |
| POST | `/v1/drivers/dynamic/load` | 动态加载驱动 |
| DELETE | `/v1/drivers/dynamic/:name/unload` | 动态卸载驱动 |
| GET | `/v1/drivers/dynamic/list` | 动态驱动列表 |
| POST | `/v1/drivers/dynamic/reload` | 重载驱动目录 |

#### 1.2.8 网关模块 `/v1/gateways`

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/v1/gateways/gateways` | 获取网关列表 |
| POST | `/v1/gateways/gateways` | 创建网关 |
| GET | `/v1/gateways/gateways/:id` | 获取网关详情 |
| PUT | `/v1/gateways/gateways/:id` | 更新网关 |
| DELETE | `/v1/gateways/gateways/:id` | 删除网关 |
| GET | `/v1/gateways/gateways/:id/devices` | 获取网关下设备 |
| PUT | `/v1/gateways/gateways/:id/status` | 更新网关状态 |

#### 1.2.9 告警模块 `/v1/alarms`

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/v1/alarms/` | 查询告警列表 |
| GET | `/v1/alarms/:id` | 获取告警详情 |
| GET | `/v1/alarms/statistics` | 告警统计 |
| POST | `/v1/alarms/:id/acknowledge` | 确认告警 |
| POST | `/v1/alarms/:id/resolve` | 解决告警 |
| POST | `/v1/alarms/batch-acknowledge` | 批量确认告警 |
| POST | `/v1/alarms/batch-resolve` | 批量解决告警 |

#### 1.2.10 告警规则模块 `/v1/alarm-rules`

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/v1/alarm-rules/` | 获取告警规则列表 |
| POST | `/v1/alarm-rules/` | 创建告警规则 |
| GET | `/v1/alarm-rules/:id` | 获取告警规则详情 |
| PUT | `/v1/alarm-rules/:id` | 更新告警规则 |
| DELETE | `/v1/alarm-rules/:id` | 删除告警规则 |
| POST | `/v1/alarm-rules/:id/toggle` | 切换告警规则状态 |

#### 1.2.11 监控模块 `/v1/monitoring`

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/v1/monitoring/stats` | Dashboard 统计信息 |
| GET | `/v1/monitoring/metrics` | Dashboard 性能指标 |
| GET | `/v1/monitoring/overview` | 系统概览 |
| GET | `/v1/monitoring/performance/overview` | 系统性能概览 |
| GET | `/v1/monitoring/performance/alerts` | 所有设备性能告警 |
| GET | `/v1/monitoring/:device_id/status` | 设备在线状态 |
| GET | `/v1/monitoring/:device_id/metrics` | 设备指标 |
| GET | `/v1/monitoring/:device_id/performance` | 设备性能指标 |
| GET | `/v1/monitoring/:device_id/performance/history` | 设备性能历史 |
| GET | `/v1/monitoring/:device_id/performance/alerts` | 设备性能告警 |

嵌套子模块：metrics, health, logs, dashboard

#### 1.2.12 用户模块 `/v1/users`

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/v1/users/` | 获取用户列表 |
| POST | `/v1/users/` | 创建用户 |
| GET | `/v1/users/test` | 测试用户端点 |
| GET | `/v1/users/statistics` | 用户统计 |
| GET | `/v1/users/:id` | 获取用户详情 |
| PUT | `/v1/users/:id` | 更新用户 |
| DELETE | `/v1/users/:id` | 删除用户 |
| POST | `/v1/users/:id/enable` | 启用用户 |
| POST | `/v1/users/:id/disable` | 禁用用户 |
| PUT | `/v1/users/:id/password` | 修改用户密码 |

嵌套子模块：roles, permissions

#### 1.2.13 设备模板 `/v1/device-templates`

#### 1.2.14 市场模块 `/v1/marketplace`

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/v1/marketplace/templates` | 市场模板列表 |
| GET | `/v1/marketplace/templates/:id` | 市场模板详情 |
| POST | `/v1/marketplace/templates/:id/install` | 安装市场模板 |
| GET | `/v1/marketplace/drivers` | 市场驱动列表 |
| GET | `/v1/marketplace/drivers/:id` | 市场驱动详情 |
| POST | `/v1/marketplace/drivers/:id/install` | 安装市场驱动 |

#### 1.2.15 通知模块 `/v1/notifications`

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/v1/notifications/rules` | 获取通知规则列表 |
| POST | `/v1/notifications/rules` | 创建通知规则 |
| GET | `/v1/notifications/rules/:rule_id` | 获取通知规则详情 |
| PUT | `/v1/notifications/rules/:rule_id` | 更新通知规则 |
| DELETE | `/v1/notifications/rules/:rule_id` | 删除通知规则 |
| GET | `/v1/notifications/history` | 通知历史 |
| POST | `/v1/notifications/test` | 发送测试通知 |

#### 1.2.16 通知渠道 `/v1/notification-channels`

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/v1/notification-channels/notification-channels` | 获取渠道列表 |
| POST | `/v1/notification-channels/notification-channels` | 创建渠道 |
| GET | `/v1/notification-channels/notification-channels/{id}` | 获取渠道详情 |
| PUT | `/v1/notification-channels/notification-channels/{id}` | 更新渠道 |
| DELETE | `/v1/notification-channels/notification-channels/{id}` | 删除渠道 |
| POST | `/v1/notification-channels/notification-channels/{id}/enable` | 启用渠道 |
| POST | `/v1/notification-channels/notification-channels/{id}/disable` | 禁用渠道 |
| POST | `/v1/notification-channels/notification-channels/{id}/test` | 测试渠道 |
| GET | `/v1/notification-channels/notification-channels/statistics` | 渠道统计 |

#### 1.2.17 租户管理 `/v1/tenants`（需认证）

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/v1/tenants/tenants` | 获取租户列表 |
| POST | `/v1/tenants/tenants` | 创建租户 |
| GET | `/v1/tenants/tenants/{id}` | 获取租户详情 |
| PUT | `/v1/tenants/tenants/{id}` | 更新租户 |
| POST | `/v1/tenants/tenants/{id}/suspend` | 暂停租户 |
| POST | `/v1/tenants/tenants/{id}/activate` | 激活租户 |
| POST | `/v1/tenants/tenants/{id}/change-plan` | 变更套餐 |
| GET | `/v1/tenants/tenants/{id}/usage` | 租户使用量 |
| GET | `/v1/tenants/tenants/{tenant_id}/api-keys` | 获取 API Key 列表 |
| POST | `/v1/tenants/tenants/{tenant_id}/api-keys` | 创建 API Key |
| POST | `/v1/tenants/api-keys/{id}/enable` | 启用 API Key |
| POST | `/v1/tenants/api-keys/{id}/disable` | 禁用 API Key |
| POST | `/v1/tenants/api-keys/{id}/revoke` | 撤销 API Key |
| GET | `/v1/tenants/tenants/{tenant_id}/usage-stats` | API 使用统计 |

#### 1.2.18 任务模块 `/v1/jobs`

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/v1/jobs/jobs` | 获取任务列表 |
| POST | `/v1/jobs/jobs` | 创建任务 |
| GET | `/v1/jobs/jobs/{id}` | 获取任务详情 |
| PUT | `/v1/jobs/jobs/{id}` | 更新任务 |
| DELETE | `/v1/jobs/jobs/{id}` | 删除任务 |
| POST | `/v1/jobs/jobs/{id}/enable` | 启用任务 |
| POST | `/v1/jobs/jobs/{id}/disable` | 禁用任务 |
| POST | `/v1/jobs/jobs/{id}/run` | 手动运行任务 |
| GET | `/v1/jobs/jobs/{id}/executions` | 任务执行记录 |
| GET | `/v1/jobs/jobs/statistics` | 任务统计 |
| GET | `/v1/jobs/executions` | 所有执行记录 |

#### 1.2.19 自动化模块 `/v1/automations`（存在于代码中，但未在 mod.rs 中注册）

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/v1/automations/automations` | 获取自动化列表 |
| POST | `/v1/automations/automations` | 创建自动化 |
| GET | `/v1/automations/automations/{id}` | 获取自动化详情 |
| PUT | `/v1/automations/automations/{id}` | 更新自动化 |
| DELETE | `/v1/automations/automations/{id}` | 删除自动化 |
| POST | `/v1/automations/automations/{id}/enable` | 启用自动化 |
| POST | `/v1/automations/automations/{id}/disable` | 禁用自动化 |
| POST | `/v1/automations/automations/{id}/run` | 手动运行自动化 |
| POST | `/v1/automations/automations/{id}/test` | 测试自动化 |
| GET | `/v1/automations/automations/statistics` | 自动化统计 |

#### 1.2.20 测试端点

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/v1/test-auth` | 测试认证（需认证） |

### 1.3 开放 API `/open`（API Key 认证）

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/open/health` | 开放 API 健康检查 |
| GET | `/open/devices` | 设备列表 |
| GET | `/open/devices/:id` | 设备详情 |
| GET | `/open/devices/:id/properties` | 设备属性 |
| GET | `/open/devices/:id/commands` | 设备命令列表 |
| POST | `/open/devices/:id/command` | 发送设备命令 |
| GET | `/open/devices/:id/events` | 设备事件 |
| GET | `/open/events` | 所有事件 |

---

## 二、不符合 RESTful 规范的问题列表

### 问题 1：路径中包含 HTTP 动词（动词用于 URL）⚠️ 严重

RESTful 规范要求：使用 HTTP 方法表达操作，资源名称应为名词。

| 当前路径 | HTTP 方法 | 问题 |
|---------|----------|------|
| `/devices/:id/enable` | POST | `enable` 是动词，应通过 PATCH 或 PUT 表示状态更新 |
| `/devices/:id/disable` | POST | 同上，`disable` 不应出现在路径中 |
| `/tenants/:id/suspend` | POST | `suspend` 是业务动作动词 |
| `/tenants/:id/activate` | POST | `activate` 是业务动作动词 |
| `/tenants/:id/change-plan` | POST | `change-plan` 是动词短语 |
| `/notification-channels/:id/enable` | POST | 同 enable/disable 问题 |
| `/notification-channels/:id/disable` | POST | 同上 |
| `/jobs/:id/enable` | POST | 同 enable/disable 问题 |
| `/jobs/:id/disable` | POST | 同上 |
| `/jobs/:id/run` | POST | `run` 是动词，应为 `/jobs/:id/executions` + POST |
| `/alarm-rules/:id/toggle` | POST | `toggle` 是动词 |
| `/automations/:id/run` | POST | 同 run 问题 |
| `/automations/:id/test` | POST | `test` 是动词，不应在路径中 |
| `/alarms/:id/acknowledge` | POST | `acknowledge` 是动词 |
| `/alarms/:id/resolve` | POST | `resolve` 是动词 |
| `/notification-channels/:id/test` | POST | `test` 是动词 |
| `/users/:id/password` | PUT | `password` 是资源，但路径缺少动词；实际是"修改密码"，应为 `PUT /users/:id/password` 尚可接受 |
| `/drivers/dynamic/load` | POST | `load` 是动词 |
| `/drivers/dynamic/reload` | POST | `reload` 是动词 |
| `/events/:id/acknowledge` | POST | `acknowledge` 是动词 |
| `/events/security/cleanup` | POST | `cleanup` 是动词 |

### 问题 2：批量操作路由使用 query string 而非路径参数 ⚠️ 中等

| 当前路径 | 问题 | 规范做法 |
|---------|------|---------|
| `POST /alarms/batch-acknowledge` | 批量操作应使用 `POST /alarms/batch` + 请求体指定操作类型 | 路径不表达具体动作 |
| `POST /alarms/batch-resolve` | 同上 | 同上 |
| `POST /devices/:id/data/batch` | `batch` 不应在路径中 | 应统一为 `POST /devices/:id/data` + `batch: true` |

### 问题 3：嵌套路由资源前缀重复 ⚠️ 中等

| 当前路径 | 问题 |
|---------|------|
| `/gateways/gateways` | 嵌套后再次出现 `gateways` |
| `/alarms` | 嵌套在 `/v1` 下后，又在子模块中出现 `/alarms/:id/acknowledge` |
| `/notification-channels/notification-channels` | 资源名前缀重复 |
| `/tenants/tenants` | 资源名前缀重复 |
| `/jobs/jobs` | 资源名前缀重复 |
| `/automations/automations` | 资源名前缀重复 |

**根因分析：** 这是因为 Axum 的 `nest()` 方法创建的路由已经是 `/gateways`，然后子模块 `management` 中又使用了 `Router::new().route("/gateways", ...)`，导致最终路径变成 `/gateways/gateways`。

### 问题 4：路径片段命名不一致 ⚠️ 轻微

| 问题类型 | 示例 |
|---------|------|
| 单复数混用 | `/auth/session/profile`（session 单数）vs `/tenants/plans`（plans 复数） |
| 同一概念不同命名 | `alarm-rules` vs `notification-channels` vs `device-templates`（均有复数-s/-es），但 `jobs` 无连字符，`automations` 无连字符 |
| 中英混用 | `/sse` 是英文缩写，其他中文注释混用 |

### 问题 5：POST 用于预览操作（语义不清）⚠️ 轻微

| 当前路径 | HTTP 方法 | 问题 |
|---------|----------|------|
| `/devices/from-template/:template_id/preview` | POST | "预览"是读操作，应使用 GET |

### 问题 6：Open API 路径前缀冗余 ⚠️ 轻微

| 当前路径 | 问题 |
|---------|------|
| `/open/devices` | `open` 本身已表明是公开 API，路径中包含 `/open/` 后又出现资源名，无冗余 |
| `/open/open/*` | 目前代码中 `/open` 是 nest 了一层 `open` router，`open` 路由下直接是 `open/devices`，没有双 `open`，但需要注意不要误写成 `/open/open/*` |

### 问题 7：events 模块职责混乱 ⚠️ 中等

`/events` 下混合了多个不同子域：
- `security/*` — 权限、审计日志（属于 access control）
- `performance/*` — 性能监控（属于 system monitoring）
- `real_time/*` — 实时状态
- `sse/*` — SSE 连接管理

这些应该拆分为独立模块或至少独立nest。

### 问题 8：系统追踪路径结构异常 ⚠️ 轻微

| 当前路径 | 问题 |
|---------|------|
| `/system/traces/overview` | `traces` 应属于 events 或独立的审计模块，`/system/traces/cleanup` 也是同样问题 |
| `/system/traces/cleanup` | `cleanup` 动词不应在路径中 |

### 问题 9：嵌套过深 ⚠️ 轻微

| 当前路径 | 嵌套层级 | 建议 |
|---------|---------|------|
| `/notification-channels/notification-channels/{id}/enable` | 4层 | 简化为 `/notification-channels/{id}/enable` |
| `/tenants/tenants/{tenant_id}/api-keys` | 4层 | 简化为 `/tenants/{tenant_id}/api-keys` |
| `/jobs/jobs/{id}/executions` | 4层 | 简化为 `/jobs/{id}/executions` |
| `/devices/from-template/:template_id/validate` | 4层 | 扁平化或合并资源 |

### 问题 10：自动化模块未注册 ⚠️ 需确认

`automations` 模块代码存在于 `api/src/api/automations/`，但在 `api/src/api/mod.rs` 的路由注册中并未包含此模块，属于"死代码"。

---

## 三、建议的规范化方案

### 3.1 动词路径 → HTTP 方法改造

**原则：** 状态变更操作用 `PATCH /resources/{id}` + 请求体中含 `enabled: true/false`；动作操作用 `POST /resources/{id}/actions` + 请求体指定具体 action。

| 当前路径 | 建议改为 | 说明 |
|---------|---------|------|
| `POST /devices/:id/enable` | `PATCH /devices/:id` `{ "enabled": true }` | 状态更新用 PATCH |
| `POST /devices/:id/disable` | `PATCH /devices/:id` `{ "enabled": false }` | 同上 |
| `POST /tenants/:id/suspend` | `PATCH /tenants/:id` `{ "status": "suspended" }` | 或 `POST /tenants/:id/actions` `{ "action": "suspend" }` |
| `POST /tenants/:id/activate` | `PATCH /tenants/:id` `{ "status": "active" }` | 同上 |
| `POST /tenants/:id/change-plan` | `PATCH /tenants/:id` `{ "plan_id": "xxx" }` | 资源属性更新 |
| `POST /notification-channels/:id/enable` | `PATCH /notification-channels/:id` `{ "is_enabled": true }` | 同 enable/disable |
| `POST /notification-channels/:id/disable` | `PATCH /notification-channels/:id` `{ "is_enabled": false }` | 同上 |
| `POST /jobs/:id/enable` | `PATCH /jobs/:id` `{ "enabled": true }` | 同上 |
| `POST /jobs/:id/disable` | `PATCH /jobs/:id` `{ "enabled": false }` | 同上 |
| `POST /jobs/:id/run` | `POST /jobs/:id/executions` | 手动执行是一次新的 execution |
| `POST /alarm-rules/:id/toggle` | `PATCH /alarm-rules/:id` `{ "enabled": <取反> }` | 状态切换 |
| `POST /automations/:id/run` | `POST /automations/:id/executions` | 同 jobs |
| `POST /automations/:id/test` | `POST /automations/:id/test-run` 或 `POST /automations/:id/test`（可接受，test-run 是 noun） | 测试执行 |
| `POST /alarms/:id/acknowledge` | `POST /alarms/:id/acknowledgments` | 确认记录是一种资源 |
| `POST /alarms/:id/resolve` | `POST /alarms/:id/resolutions` | 解决记录是一种资源 |
| `POST /notification-channels/:id/test` | `POST /notification-channels/:id/test-messages` | 测试消息是资源 |
| `POST /events/:id/acknowledge` | `POST /events/:id/acknowledgments` | 同 alarms |
| `POST /events/security/cleanup` | `POST /audit-logs/cleanup` + 请求体 `{ "retention_days": 90 }` | 清理是动作 |

### 3.2 批量操作规范化

| 当前路径 | 建议改为 |
|---------|---------|
| `POST /alarms/batch-acknowledge` | `POST /alarms/batch` `{ "action": "acknowledge", "ids": [...] }` |
| `POST /alarms/batch-resolve` | `POST /alarms/batch` `{ "action": "resolve", "ids": [...] }` |
| `POST /devices/:id/data/batch` | `POST /devices/:id/data` + 请求体添加 `batch: true` 或直接批量数组 |

### 3.3 路由前缀去重

**原则：** `nest("/gateways", ...)` 之后，子模块 `management` 中的路由应直接以 `/` 为基础，不要再加 `/gateways`。

修复示例：
```rust
// 当前（错误）
let gateways = Router::new()
    .route("/gateways", get(list_gateways))           // → /gateways/gateways
    .route("/gateways/:id", get(get_gateway))         // → /gateways/gateways/:id

// 建议（正确）
let gateways = Router::new()
    .route("/", get(list_gateways))                   // → /gateways
    .route("/:id", get(get_gateway))                 // → /gateways/:id
```

以下模块需要修复：
- `gateways/management.rs` — 当前生成 `/gateways/gateways/*`
- `notification_channels/mod.rs` — 当前生成 `/notification-channels/notification-channels/*`
- `tenants/mod.rs` — 当前生成 `/tenants/tenants/*`
- `jobs/mod.rs` — 当前生成 `/jobs/jobs/*`
- `automations/mod.rs`（如果启用）— 当前生成 `/automations/automations/*`

### 3.4 Open API 路径保持简洁

| 当前路径 | 建议改为 |
|---------|---------|
| `/open/devices` | `/open/devices`（当前正确，无需修改）|
| `/open/devices/:id/properties` | `/open/devices/:id/properties`（当前正确）|

注意：确保不要出现 `/open/open/*` 的双层前缀。

### 3.5 预览操作使用 GET

| 当前路径 | 建议改为 |
|---------|---------|
| `POST /devices/from-template/:template_id/preview` | `GET /devices/from-template/:template_id/preview` |

### 3.6 事件模块拆分建议

将 `/events` 下的混合子模块拆分为独立域：

| 当前路径 | 建议改为（独立 nest） |
|---------|---------------------|
| `/events/security/*` | `/security/*`（RBAC/审计） |
| `/events/performance/*` | `/system/performance/*` |
| `/events/real-time/*` | `/events/realtime/*`（扁平化） |
| `/events/sse/*` | `/events/sse/*`（可保留在 events 下） |

### 3.7 资源命名一致性

| 模块 | 当前路径 | 建议改为 |
|------|---------|---------|
| alarm-rules | `/alarm-rules` | `/alarm-rules`（保持）|
| notification-channels | `/notification-channels` | `/notification-channels`（保持）|
| device-templates | `/device-templates` | `/device-templates`（保持）|
| jobs | `/jobs` | `/jobs`（保持）|
| automations | `/automations` | `/automations`（保持）|

**原则：统一使用 kebab-case 复数名词**。

### 3.8 系统追踪重命名

| 当前路径 | 建议改为 |
|---------|---------|
| `/system/traces/overview` | `/audit/traces`（或 `/system/audit/traces`） |
| `/system/traces/cleanup` | `POST /audit/traces/cleanup` |

---

## 四、优先级排序

### 🔴 P0 - 必须修复（影响正确性）

1. **路由前缀重复导致路径冗余**  
   影响范围：`/gateways/gateways/*`、`/notification-channels/notification-channels/*`、`/tenants/tenants/*`、`/jobs/jobs/*`、`/automations/automations/*`  
   修复方式：调整子模块 `management` 中的路由定义，移除冗余前缀  
   影响：当前 API 路径比预期深一层，客户端调用会出错

2. **自动化模块未注册**  
   `automations` 模块存在于代码中但未在 `mod.rs` 注册，导致所有自动化 API 不可用  
   修复方式：在 `mod.rs` 的 `protected_routes` 中添加 `.nest("/automations", automations::create_router())`

### 🟠 P1 - 高优先级（不符合 REST 规范，影响可维护性）

3. **路径中包含动词（enable/disable/suspend/activate/run/acknowledge 等）**  
   影响范围：15+ 路由  
   修复方式：改用 `PATCH /resources/{id}` 或 `POST /resources/{id}/actions`（详见 3.1）  
   影响：API 语义不清，违反 RESTful 设计原则

4. **批量操作使用 query string**  
   影响范围：`/alarms/batch-acknowledge`、`/alarms/batch-resolve`  
   修复方式：合并为 `POST /alarms/batch` + 请求体

### 🟡 P2 - 中优先级（改进建议）

5. **events 模块职责混乱**  
   `/events/security/*`、`/events/performance/*`、`/events/sse/*` 应拆分为独立模块  
   建议：先拆 `/events/security` → `/security`，`/events/performance` → `/system/performance`

6. **POST 用于预览操作**  
   `POST /devices/from-template/:template_id/preview` 应改为 `GET`

7. **系统追踪路径结构异常**  
   `/system/traces/*` 应归属 audit 域

### 🟢 P3 - 低优先级（代码质量）

8. **Open API 路径注意不要出现双 open 前缀**（代码中目前正确，但需注意）

9. **资源命名一致性问题**  
   当前 `alarm-rules`（kebab-case）vs `jobs`（无连字符），建议统一使用 kebab-case 复数

---

## 五、总结

| 类别 | 数量 |
|------|------|
| 总路由数（估算） | ~120+ |
| P0 必须修复 | 2 项 |
| P1 高优先级 | 2 项（约 15+ 路由受影响）|
| P2 中优先级 | 3 项 |
| P3 低优先级 | 2 项 |

**最紧急的修复项：**
1. 修复 `/gateways/gateways/*` 等前缀重复路由（影响 API 路径正确性）
2. 注册未启用的 `automations` 模块
3. 将 `enable/disable/suspend/run` 等动词移出 URL 路径

修复 P0 和 P1 问题后，API 将符合基本 RESTful 规范，且路径不会再出现意外的深度嵌套。
