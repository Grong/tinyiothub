# TinyIoTHub - Rust Implementation

**版本**: v1.1.0  
**官方网站**: https://tinyiothub.com  
**仓库地址**: https://github.com/Grong/tinyiothub  
**Docker Hub**: https://hub.docker.com/r/grong/tinyiothub  
**发布日期**: 2026-01-19

基于 Rust 的云端 SaaS 物联网平台，支持配置和管理边缘网关设备，兼容多协议设备接入。

## 版本说明

本项目基于 Rust 2024 Edition，针对鸿蒙系统进行了优化。

## 特性

- 🚀 **高性能异步架构**（基于 Tokio）
- 🔌 **多协议支持**（Modbus RTU/TCP、ONVIF、SNMP、Ping）
- 📊 **实时数据采集和处理**
- 🌐 **现代化 REST API**（基于 Axum + 统一响应格式）
- 📱 **MQTT 消息推送**（支持主备双通道）
- 🔐 **JWT 身份认证**（支持会话管理）
- 📈 **设备监控和告警**（实时状态监控 + 规则引擎）
- 🎯 **事件驱动架构**（设备联动、SSE 流式推送）
- ⏰ **定时任务调度**（Cron 表达式、任务执行记录、Workspace 隔离）
- 🏢 **多租户 SaaS 架构**（租户隔离、订阅管理）
- 🏗️ **工作空间管理**（物理环境分组、AI Agent 绑定）
- ⚡ **自动化规则**（触发器-动作引擎）
- 🔧 **自愈引擎**（探测调度器 + 自动故障恢复，支持 system/device/task 探针）
- 📢 **通知系统**（多渠道通知：Email、SMS、SSE、Webhook）
- 🤖 **AI Agent 集成**（内嵌 MCP Server + A2UI 聊天，Claude Desktop/Cursor 支持）
- 🛒 **应用市场**（驱动市场、模板市场）
- 💾 **SQLite 数据存储**（支持自动迁移）
- 🔄 **自动重连和故障恢复**
- 🤖 **鸿蒙系统原生支持**
- ⚙️ **专业配置系统**（层次化配置，环境感知）
- 🔒 **安全加固**（配置验证，权限控制）
- 🎨 **现代化前端界面**（Lit 3 + TypeScript + Vite + nanostore）

## 项目结构

```
tinyiothub/
├── api/                      # Rust 后端服务
│   ├── src/                  # 源代码
│   ├── migrations/           # 数据库迁移
│   ├── drivers/              # 驱动实现
│   ├── templates/            # 设备模板
│   ├── vendor/               # 第三方依赖
│   ├── Cargo.toml            # Rust 项目配置
│   └── tinyiothub.db         # SQLite 数据库
├── web/                      # Lit 3 前端应用 (Web Components)
│   ├── src/                  # 源代码
│   │   ├── ui/              # Lit 组件、页面、聊天/A2UI
│   │   ├── api/             # API 客户端
│   │   ├── i18n/            # 国际化
│   │   ├── styles/          # CSS 样式
│   │   └── stores/          # nanostore 状态管理
│   ├── package.json          # Node.js 项目配置
│   └── vite.config.ts       # Vite 构建配置
├── sdks/                     # SDK 开发包
│   └── driver-sdk/           # 驱动开发 SDK
├── examples/                 # 示例项目
│   ├── example-plugin/       # 插件示例
│   └── bacnet-driver/        # BACnet 驱动示例
├── marketplace/              # 市场资源
│   ├── drivers/              # 驱动市场
│   └── templates/            # 模板市场
├── scripts/                  # 工具脚本
├── docs/                     # 项目文档
├── .kiro/                    # 开发规范
└── skills/                   # AI prompts / skills
```

## 快速开始

### 环境要求

**后端**:
- **Rust**: 1.85+ (2024 Edition)
- **操作系统**: Linux, Windows, HarmonyOS
- **数据库**: SQLite (内置)
- **网络**: MQTT Broker (可选)

**前端**:
- **Node.js**: 18+
- **pnpm**: 8+ (推荐包管理器)
- **浏览器**: Chrome, Firefox, Safari, Edge

### 安装和运行

#### 开发模式（分离部署）

**后端**:
```bash
cd api
cargo run
```

**前端**:
```bash
cd web
pnpm install
pnpm dev
```

访问: http://localhost:3001

#### 生产模式（单进程部署）

**构建**:
```bash
# Windows
.\scripts\build-single-binary.ps1 -Release

# Linux/macOS
./scripts/build-single-binary.sh --release
```

**运行**:
```bash
cd api
.\target\release\tinyiothub.exe  # Windows
./target/release/tinyiothub      # Linux/macOS
```

访问: http://localhost:3002

**优势**:
- ✅ 单进程部署，无需 Node.js
- ✅ 内存占用低（~80MB vs ~200MB）
- ✅ 启动快速（<2s vs ~5s）
- ✅ 支持动态路由

详见: [单进程部署方案](docs/deployment/single-process-deployment.md)

#### 前端独立运行（开发调试）

```bash
cd web

# 安装依赖
pnpm install

# 开发运行
pnpm dev

# 构建生产版本
pnpm build
```

### 配置文件

后端配置文件位于 `api/app_settings.toml`：

```toml
# api/app_settings.toml 示例
[server]
host = "0.0.0.0"
port = 3002

[database]
url = "tinyiothub.db"
auto_migrate = true

[mqtt.primary]
host = "192.168.1.124"
port = 1883
username = "admin"
password = "password"

[security.jwt]
secret = "your-secret-key-must-be-at-least-32-characters-long"
expiration_secs = 10800  # 3 hours
```

前端开发服务器代理配置位于 `web/vite.config.ts`：

```typescript
server: {
  port: 3001,
  proxy: {
    '/api': 'http://localhost:3002'
  }
}
```

### 访问服务

启动后访问以下地址：

- **Web 管理界面**: http://localhost:3001/ (前端开发服务器)
- **后端 API**: http://localhost:3002/api/v1/
- **健康检查**: http://localhost:3002/api/v1/system/health

## API 开发规范

本项目严格遵循统一的API开发规范，确保前后端数据对接的一致性。

### 统一响应格式

所有API端点必须返回以下格式：

```json
{
    "code": 0,           // 0表示成功，非0表示错误
    "msg": "",           // 错误信息，成功时为空字符串
    "result": T | null   // 实际数据，错误时为null
}
```

### 后端API规范

```rust
// ✅ 正确的API函数签名
async fn list_devices(
    Query(params): Query<DeviceQuery>,
    State(state): State<AppState>,
) -> Json<ApiResponse<Vec<Device>>> {
    // 业务逻辑
    let devices = get_devices(&params).await?;
    ApiResponseBuilder::success(devices)
}

// 使用统一的响应构建器
use crate::dto::response::builder::ApiResponseBuilder;

// 成功响应
ApiResponseBuilder::success(data)

// 错误响应
ApiResponseBuilder::error("错误信息")
```

### 前端API调用规范

```typescript
// ✅ 正确：使用统一API客户端
import { apiGet, apiPost, apiPut, apiDelete } from './client'

// GET请求
const response = await apiGet<UserList>('users', { page: 1, pageSize: 20 })

// POST请求
const response = await apiPost<User>('users', userData)
```

### Service层结构

```typescript
// web/service/users.ts
export const userApi = {
  getUsers: (params?: { page?: number; pageSize?: number }) => 
    apiGet<User[]>('users', params),
  createUser: (data: CreateUserRequest) => 
    apiPost<User>('users', data),
}

// nanostore 状态管理
import { atom, task } from 'nanostores'

export const $users = atom<User[]>([])

export const loadUsers = task(async (params?: { page?: number; pageSize?: number }) => {
  const response = await userApi.getUsers(params)
  $users.set(response.result || [])
})
```

详细的API开发规范请参考：[API开发规范](.kiro/steering/api-standards.md)

## 项目架构

### 整体架构

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Lit 3 UI      │    │   REST API      │    │   MQTT Client   │
│   (web/)        │    │   (api/)        │    │   (rumqttc)     │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                       │                       │
         └───────────────────────┼───────────────────────┘
                                 │
         ┌─────────────────────────────────────────────────────┐
         │              Application Layer                      │
         │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐ │
         │  │ Data Server │  │Message Server│  │  Scheduler  │ │
         │  └─────────────┘  └─────────────┘  └─────────────┘ │
         └─────────────────────────────────────────────────────┘
                                 │
         ┌─────────────────────────────────────────────────────┐
         │               Domain Layer                          │
         │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐ │
         │  │   Device    │  │    Alarm    │  │    Event    │ │
         │  │   Domain    │  │   Domain    │  │   Domain    │ │
         │  └─────────────┘  └─────────────┘  └─────────────┘ │
         └─────────────────────────────────────────────────────┘
                                 │
         ┌─────────────────────────────────────────────────────┐
         │            Infrastructure Layer                     │
         │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐ │
         │  │   Config    │  │  Hardware   │  │ Persistence │ │
         │  │  System     │  │ Abstraction │  │   (SQLite)  │ │
         │  └─────────────┘  └─────────────┘  └─────────────┘ │
         └─────────────────────────────────────────────────────┘
```

### 后端目录结构 (api/)

```
api/
├── src/
│   ├── api/                  # REST API 层
│   │   ├── auth/             # 认证相关 API
│   │   ├── devices/          # 设备管理 API
│   │   ├── drivers/          # 驱动管理 API
│   │   ├── alarms/           # 告警管理 API
│   │   ├── alarm_rules/      # 告警规则 API
│   │   ├── agents/           # AI Agent 管理 API
│   │   ├── automations/      # 自动化规则 API
│   │   ├── chat/             # AI Agent 聊天 API
│   │   ├── events/           # 事件管理 API
│   │   ├── jobs/             # 定时任务 API
│   │   ├── marketplace/      # 应用市场 API
│   │   ├── mcp/              # 内嵌 MCP Server
│   │   ├── notifications/    # 通知管理 API
│   │   ├── notification_channels/ # 通知渠道 API
│   │   ├── self_healing/     # 自愈引擎 API
│   │   ├── system/           # 系统管理 API
│   │   ├── monitoring/       # 监控 API
│   │   ├── templates/        # 设备模板 API
│   │   ├── tenants/          # 租户管理 API
│   │   ├── users/            # 用户管理 API
│   │   ├── workspaces/       # 工作空间 API
│   │   ├── batch/            # 批量操作 API
│   │   ├── open/             # 开放接口 API
│   │   ├── heartbeat/        # 心跳 API
│   │   └── middleware/       # 中间件
│   ├── application/          # 应用服务层
│   │   ├── agent/            # Agent 会话、聊天、记忆服务
│   │   ├── cron_scheduler.rs # 定时任务调度（CronSchedulerService）
│   │   ├── data_context.rs   # 数据上下文
│   │   ├── data_server.rs    # 数据服务
│   │   ├── message_server.rs # 消息服务
│   │   └── service_manager.rs # 服务管理器
│   ├── domain/               # 领域层
│   │   ├── agent/            # Agent 领域
│   │   ├── alarm/            # 告警领域
│   │   ├── automation/       # 自动化领域
│   │   ├── cron/             # 定时任务领域
│   │   ├── device/           # 设备领域（含 driver/registry）
│   │   ├── event/            # 事件领域
│   │   ├── job/              # 任务领域
│   │   ├── marketplace/      # 市场领域
│   │   ├── organization/     # 组织领域
│   │   ├── permission/       # 权限领域
│   │   ├── plugin/           # 插件领域
│   │   ├── product/          # 产品领域
│   │   ├── role/             # 角色领域
│   │   ├── self_healing/     # 自愈引擎领域
│   │   ├── tag/              # 标签领域
│   │   ├── template/         # 模板领域
│   │   ├── tenant/           # 租户领域
│   │   ├── user/             # 用户领域
│   │   └── workspace/        # 工作空间领域
│   │       ├── repository.rs # Repository trait（接口）
│   │       └── service.rs    # 领域服务
│   ├── dto/                  # 数据传输对象（纯结构体，无 SQL）
│   ├── infrastructure/       # 基础设施层
│   │   └── persistence/
│   │       └── repositories/ # Repository 实现（SQLite）
│   ├── shared/               # 共享组件
│   ├── lib.rs                # 库入口
│   └── main.rs               # 程序入口
├── derive/                   # 自定义宏
├── migrations/               # 数据库迁移文件
├── drivers/                  # 驱动实现
├── templates/                # 设备模板
├── vendor/                   # 第三方依赖（本地 fork）
├── Cargo.toml                # Rust 项目配置
├── Dockerfile                # Docker 构建文件
├── app_settings.toml         # 应用配置
└── tinyiothub.db             # SQLite 数据库
```

### 前端目录结构 (web/)

```
web/
├── src/
│   ├── ui/                  # Lit Web Components
│   │   ├── components/      # 通用组件
│   │   ├── views/           # 页面视图
│   │   ├── controllers/     # 状态控制器
│   │   └── chat/            # AI 聊天 / A2UI 组件
│   ├── api/                 # API 客户端
│   ├── i18n/                # 国际化
│   ├── styles/              # CSS 样式
│   ├── stores/              # nanostore 状态管理
│   └── types/               # TypeScript 类型定义
├── package.json
└── vite.config.ts
```

## API 接口

### 认证接口
- `POST /api/v1/auth/login` - 用户登录
- `POST /api/v1/auth/logout` - 用户登出
- `GET /api/v1/auth/session` - 获取会话信息

### 设备管理
- `GET /api/v1/devices` - 获取设备列表
- `POST /api/v1/devices` - 创建设备
- `GET /api/v1/devices/{id}` - 获取设备详情
- `PUT /api/v1/devices/{id}` - 更新设备
- `DELETE /api/v1/devices/{id}` - 删除设备
- `GET /api/v1/devices/{id}/profile` - 获取设备配置文件

### 驱动管理
- `GET /api/v1/drivers` - 获取驱动列表
- `GET /api/v1/drivers/{name}` - 获取驱动详情
- `GET /api/v1/drivers/{name}/config` - 获取驱动配置参数
- `GET /api/v1/drivers/names` - 获取支持的驱动名称

### 设备模板
- `GET /api/v1/device-templates` - 获取模板列表
- `GET /api/v1/device-templates/{id}` - 获取模板详情
- `GET /api/v1/device-templates/categories` - 获取模板分类
- `POST /api/v1/device-templates/{id}/validate` - 验证模板输入
- `POST /api/v1/device-templates/{id}/preview` - 预览设备创建

### 告警管理
- `GET /api/v1/alarms` - 获取告警列表
- `GET /api/v1/alarms/{id}` - 获取告警详情
- `POST /api/v1/alarms/{id}/acknowledge` - 确认告警
- `POST /api/v1/alarms/{id}/resolve` - 解决告警
- `POST /api/v1/alarms/batch-acknowledge` - 批量确认告警
- `GET /api/v1/alarms/statistics` - 告警统计

### 告警规则
- `GET /api/v1/alarm-rules` - 获取告警规则列表
- `POST /api/v1/alarm-rules` - 创建告警规则
- `GET /api/v1/alarm-rules/{id}` - 获取告警规则详情
- `PUT /api/v1/alarm-rules/{id}` - 更新告警规则
- `DELETE /api/v1/alarm-rules/{id}` - 删除告警规则
- `POST /api/v1/alarm-rules/{id}/toggle` - 启用/禁用规则

### 工作空间
- `GET /api/v1/workspaces` - 获取工作空间列表
- `POST /api/v1/workspaces` - 创建工作空间
- `GET /api/v1/workspaces/{id}` - 获取工作空间详情
- `PUT /api/v1/workspaces/{id}` - 更新工作空间
- `DELETE /api/v1/workspaces/{id}` - 删除工作空间
- `POST /api/v1/workspaces/{id}/devices` - 分配设备到工作空间

### 定时任务
- `GET /api/v1/jobs` - 获取定时任务列表
- `POST /api/v1/jobs` - 创建定时任务
- `GET /api/v1/jobs/{id}` - 获取任务详情
- `PUT /api/v1/jobs/{id}` - 更新任务
- `DELETE /api/v1/jobs/{id}` - 删除任务
- `POST /api/v1/jobs/{id}/toggle` - 启用/禁用任务
- `GET /api/v1/jobs/{id}/runs` - 获取任务执行记录

### 自动化规则
- `GET /api/v1/automations` - 获取自动化规则列表
- `POST /api/v1/automations` - 创建自动化规则
- `PUT /api/v1/automations/{id}` - 更新自动化规则
- `DELETE /api/v1/automations/{id}` - 删除自动化规则

### 自愈引擎
- `GET /api/v1/self-healing/probes` - 获取探针列表
- `POST /api/v1/self-healing/probes` - 创建探针
- `GET /api/v1/self-healing/status` - 获取自愈状态

### 事件系统
- `GET /api/v1/events` - 获取事件列表
- `GET /api/v1/events/stream` - SSE 事件流订阅

### 通知管理
- `GET /api/v1/notifications` - 获取通知列表
- `POST /api/v1/notifications/{id}/read` - 标记已读
- `GET /api/v1/notification-channels` - 获取通知渠道
- `POST /api/v1/notification-channels` - 创建通知渠道

### 用户与租户
- `GET /api/v1/users` - 获取用户列表
- `POST /api/v1/users` - 创建用户
- `GET /api/v1/users/roles` - 获取角色列表
- `GET /api/v1/tenants` - 获取租户列表

### 系统管理
- `GET /api/v1/system/health` - 健康检查
- `GET /api/v1/system/features` - 获取系统特性
- `GET /api/v1/system/config` - 获取系统配置
- `GET /api/v1/system/initialization` - 系统初始化状态

### 监控接口
- `GET /api/v1/monitoring/health` - 健康检查
- `GET /api/v1/monitoring/metrics` - 系统指标
- `GET /api/v1/monitoring/dashboard/stats` - 仪表板统计

### AI Agent
- `GET /api/v1/agents` - 获取 Agent 列表
- `GET /api/v1/agents/{id}/config` - 获取 Agent 配置
- `PUT /api/v1/agents/{id}/config` - 更新 Agent 配置
- `GET /api/v1/agents/{id}/heartbeat/config` - 获取心跳配置
- `PUT /api/v1/agents/{id}/heartbeat/config` - 更新心跳配置
- `GET /api/v1/agents/{id}/heartbeat/logs` - 获取心跳执行日志
- `GET /api/v1/agents/{id}/heartbeat/tasks` - 获取心跳任务列表
- `PUT /api/v1/agents/{id}/heartbeat/tasks` - 更新心跳任务
- `GET /api/v1/agents/{id}/files` - 列出工作空间文件
- `GET /api/v1/agents/{id}/files/{name}` - 读取工作空间文件
- `PUT /api/v1/agents/{id}/files/{name}` - 写入工作空间文件
- `POST /api/v1/agents/skills` - 创建/更新技能
- `GET /api/v1/agents/skills` - 获取技能列表
- `GET /api/v1/agents/skills/{name}` - 获取技能内容
- `DELETE /api/v1/agents/skills/{name}` - 删除技能
- `POST /api/v1/chat/stream` - SSE 流式聊天
- `GET /api/v1/chat/history` - 获取聊天历史

### MCP Server（内嵌）
- `POST /mcp` - MCP JSON-RPC 统一端点（tools/list、tools/call）
- `POST /mcp/tools/list` - 列出可用工具
- `POST /mcp/tools/call` - 调用指定工具
- `POST /mcp/sse` - MCP SSE 流式端点

## 开发指南

### 后端开发

#### 添加新API端点

1. 在相应的API模块中创建处理函数
2. 使用统一的响应构建器
3. 遵循命名规范

```rust
// 示例：添加新API
use crate::dto::response::builder::ApiResponseBuilder;

async fn list_items(
    Query(params): Query<ItemQuery>,
    State(state): State<AppState>,
) -> Json<ApiResponse<Vec<Item>>> {
    // 业务逻辑
    let items = get_items(&params).await?;
    ApiResponseBuilder::success(items)
}
```

#### 添加新设备驱动

1. 在 `src/domain/device/driver/drivers/` 创建驱动文件
2. 实现 `DeviceDriver` trait
3. 在 `mod.rs` 中注册驱动

```rust
// 示例：创建新驱动
use crate::domain::device::driver::{DeviceDriver, DriverResult};

pub struct MyCustomDriver {
    // 驱动配置
}

#[async_trait::async_trait]
impl DeviceDriver for MyCustomDriver {
    async fn connect(&mut self) -> DriverResult<()> {
        // 连接逻辑
    }
    
    async fn read_data(&mut self) -> DriverResult<Vec<u8>> {
        // 数据读取逻辑
    }
}
```

### 前端开发

#### API 客户端

1. 在 `web/src/api/` 目录创建 API 封装
2. 使用统一的 API 客户端

```typescript
// web/src/api/items.ts
import { apiGet, apiPost } from './client'

export interface Item {
  id: string
  name: string
  createdAt: string
}

export const itemApi = {
  getItems: (params?: { page?: number }) => 
    apiGet<Item[]>('items', params),
  createItem: (data: CreateItemRequest) => 
    apiPost<Item>('items', data),
}
```

#### 创建新组件

1. 在 `web/src/ui/views/` 或 `web/src/ui/components/` 创建组件
2. 使用 `api/` 层提供的 API 客户端
3. 遵循组件命名规范

```typescript
// web/src/ui/views/item-list.ts
import { LitElement, html, css } from 'lit'
import { customElement, state } from 'lit/decorators.js'
import { itemApi } from '../../api/items'

@customElement('item-list')
export class ItemList extends LitElement {
  @state() private items: Item[] = []
  
  async firstUpdated() {
    const response = await itemApi.getItems()
    this.items = response.result || []
  }
  
  render() {
    return html`
      <div>
        ${this.items.map(item => html`<div>${item.name}</div>`)}
      </div>
    `
  }
}
```

### 开发工具

#### 代码格式化和检查

```bash
# 后端
cd api
cargo fmt          # 格式化代码
cargo check        # 检查代码
cargo clippy       # 代码检查

# 前端
cd web
pnpm dev           # 开发服务器
pnpm build         # 生产构建
pnpm test          # 运行测试
pnpm preview       # 预览生产构建
```

#### API测试

```bash
# 验证驱动API
./scripts/verify-driver-api.sh

# API格式检查
python3 scripts/test-api-format.py
```

## 鸿蒙系统部署

### 构建和部署

详细部署指南请参考：
- [鸿蒙部署指南](HARMONYOS_DEPLOYMENT_GUIDE.md)
- [快速开始](QUICK_START_HARMONYOS.md)
- [构建说明](build-harmonyos.md)

使用部署脚本：
```bash
# Linux/macOS
./deploy-to-harmonyos.sh

# Windows
.\build-harmonyos.bat

# 或使用构建脚本
./build-harmonyos.sh
```

## MQTT 主题

```
gateway/{sn}/heartbeat        # 心跳消息
gateway/{sn}/device_regist    # 设备注册
gateway/{sn}/command          # 命令下发
gateway/{sn}/device_command   # 设备命令
gateway/{sn}/data             # 数据上传
gateway/{sn}/alarm            # 告警消息
```

## 项目状态

✅ **最新完成的工作**:
- **Cron 定时任务重构**: 从旧双调度器迁移到统一的 `CronSchedulerService`，基于 `cron_jobs`/`cron_runs` 表，支持 Workspace 隔离
- **AI Agent 架构重构**: 内嵌 MCP Server + A2UI 聊天界面，支持 SSE 流式对话和 Agent 技能调用
- **前端架构迁移**: 从 Next.js 迁移到 Lit 3 + Vite，采用 Web Components 和 nanostore 状态管理
- **告警规则引擎**: 支持阈值、范围、变化、持续时间、组合五种条件类型
- **工作空间管理**: 物理环境分组，每个 Workspace 绑定一个 AI Agent
- **自愈引擎**: system/device/task 三级探针，自动故障检测与恢复
- **API 规范统一**: 建立完整的前后端API开发规范，确保数据对接一致性
- **统一响应格式**: 所有API使用 `ApiResponse<T>` 包装格式
- **设备创建向导**: 完整的模板选择和设备配置流程
- **驱动管理系统**: 动态驱动加载，配置参数管理
- **多语言支持**: 模板和界面的国际化处理

✅ **核心功能**:
- **REST API 系统**: 基于 Axum 的现代化 API，统一响应格式
- **前端界面**: Lit 3 + TypeScript + Vite 现代化界面
- **设备驱动系统**: 支持重试机制和状态管理的驱动框架
- **设备模板系统**: 模板化设备创建，支持验证和预览
- **配置管理**: 多源配置加载，环境变量覆盖，配置验证
- **认证授权**: JWT 会话管理，角色权限控制
- **监控告警**: 健康检查，指标收集，告警规则引擎
- **定时任务**: Cron 表达式调度，任务执行记录，Workspace 隔离
- **事件系统**: SSE 流式推送，设备联动
- **鸿蒙系统适配**: 硬件抽象层，资源优化配置

🔧 **技术栈**:
- **后端**: Rust 2024 + Axum + SQLite + Tokio
- **前端**: Lit 3 + TypeScript + Vite + nanostore
- **数据库**: SQLite + SQLx (自动迁移)
- **认证**: JWT + 会话管理
- **通信协议**: MQTT, HTTP, Modbus RTU/TCP, ONVIF, SNMP
- **AI 集成**: 内嵌 MCP Server + A2UI (SSE 流式)
- **包管理**: pnpm (前端)，Cargo (后端)


## 许可证

MIT License - 详见 [license](license) 文件