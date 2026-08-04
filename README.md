# TinyIoTHub

轻量级工业边缘 IoT 平台。多协议设备接入、L0-L3 自愈引擎、自然语言运维 — 让边缘网络管理像聊天一样简单。

> **项目状态**：早期开发阶段（v0.x）。核心平台（物模型、告警、事件、AI Agent、MCP）可用；
> 部分协议驱动仍在开发中，API 在 1.0 前可能有破坏性变更。欢迎试用与反馈。

**官方网站**: https://tinyiothub.com  
**仓库地址**: https://github.com/Grong/tinyiothub  
**Docker Hub**: https://hub.docker.com/r/grong/tinyiothub

## 特性

**设备接入**
- 物本体模型：设备、空间、产线统一为层级化「物」，属性/事件/操作 + 知识文档
- 协议支持：MQTT 可用；Modbus RTU/TCP、ONVIF、SNMP 驱动开发中（驱动框架与 SDK 已就绪）
- AI 驱动匹配（实验性）：描述设备类型，辅助匹配或生成驱动代码
- 物模板：JSON 模板一键创建，支持 DTDL / WoT 模型导入与 DTDL 导出

**智能运维**
- L0-L3 分级自愈引擎：system/device/task 三级探针，自动故障检测与恢复
- 自治运维（Thing Agent Loop）：AI 被设备事件/定时巡检/用户指令唤醒，查本体、做决策、经三态策略门（off/diagnose/act）自主操作设备，行动后回读验证并留全量审计
- 规则引擎：阈值、范围、变化、持续时间、组合五种条件类型
- 心跳探针：定期自检网关与子设备，提前发现隐患；诊断结论自动转交自治 Loop 处置
- Cron 定时任务：Workspace 隔离，执行记录与统计

**AI 原生**
- 自然语言交互：用日常语言配置设备、查询状态、排查故障
- MCP Server：内嵌 Model Context Protocol 服务，Claude Desktop、Cursor 直接连接
- A2UI 聊天界面：SSE 流式对话，Agent 技能调用
- 设备自发现：AI 辅助识别设备类型并推荐驱动

**部署灵活**
- 单进程部署：~80MB 内存占用，无需外部数据库
- SQLite 存储：零依赖，自动迁移
- 开源 MIT 协议：可私有化部署，可商用
- Lit 3 前端：Web Components，轻量快速

## 项目结构（多 Crate 架构）

```
tinyiothub/
├── cloud/                   # SaaS 应用编排层（主二进制）
│   ├── src/                 # SaaS 领域逻辑（tenant, user, workspace, marketplace）
│   ├── migrations/          # 数据库迁移
│   ├── templates/           # 设备模板
│   └── Cargo.toml           # Rust 项目配置
├── crates/                  # 内部库 Crate
│   ├── tinyiothub-core/     # 契约层：traits + 领域模型 + repository 接口
│   ├── tinyiothub-runtime/  # 基础设施：EventBus, DataServer, drivers
│   ├── tinyiothub-storage/  # 数据层：SQLite 实现（re-export core traits）
│   ├── tinyiothub-web/      # HTTP 基础设施层（中间件、ApiResponseBuilder）
│   ├── tinyiothub-error/    # 错误类型（带 `thiserror` 派生）
│   └── ...（其他支持库）
├── web/                     # Lit 3 前端应用 (Web Components)
│   ├── src/                 # 源代码
│   │   ├── ui/             # Lit 组件、页面、聊天/A2UI
│   │   ├── api/            # API 客户端
│   │   ├── i18n/           # 国际化
│   │   ├── styles/         # CSS 样式
│   │   └── stores/         # nanostore 状态管理
│   ├── package.json         # Node.js 项目配置
│   └── vite.config.ts      # Vite 构建配置
├── sdks/                    # SDK 开发包
│   └── plugin-sdk/         # 驱动开发 SDK
├── examples/                # 示例项目
│   ├── example-plugin/     # 插件示例
│   └── bacnet-driver/      # BACnet 驱动示例
├── marketplace/            # 市场资源
│   ├── drivers/            # 驱动市场
│   └── templates/          # 模板市场
├── vendor/                  # 第三方依赖（本地 fork，如 onvif-rs）
├── scripts/                # 工具脚本
├── deploy/                 # Docker 部署（docker-compose、边缘镜像）
├── docs/                   # 项目文档
└── skills/                 # AI prompts / skills
```

**注意：本项目采用多 Crate 架构，依赖方向为单向不可逆：`cloud/edge → runtime → core ← storage`。详细架构见 [CLAUDE.md](CLAUDE.md)。**

## 快速开始

### 环境要求

**后端**:
- **Rust**: 1.85+ (2024 Edition)
- **操作系统**: Linux, Windows（HarmonyOS 支持实验性）
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
cd cloud
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
./scripts/build-static.sh
```

**运行**:
```bash
cd cloud
.\target\release\tinyiothub.exe  # Windows
./target/release/tinyiothub      # Linux/macOS
```

访问: http://localhost:3002

**优势**:
- ✅ 单进程部署，无需 Node.js
- ✅ 内存占用低（~80MB vs ~200MB）
- ✅ 启动快速（<2s vs ~5s）
- ✅ 支持动态路由

Docker 部署见 [deploy/docker/README.md](deploy/docker/README.md)。

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

后端配置文件位于仓库根目录 `app_settings.toml`：

```toml
# app_settings.toml 示例
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
use tinyiothub_web::response::ApiResponseBuilder;

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

详细的API开发规范请参考：[AGENTS.md](AGENTS.md)

## 项目架构

### 整体架构

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Lit 3 UI      │    │   REST API      │    │   MQTT Client   │
│   (web/)        │    │   (cloud/)      │    │   (rumqttc)     │
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

### 后端目录结构 (cloud/)

```
cloud/
├── src/
│   ├── api/                  # 路由挂载 + HTTP 中间件（WorkspaceScope, auth）
│   ├── modules/              # 业务模块（types → service → handler 三层结构）
│   │   ├── thing/            # 物本体管理（CRUD、层级树、本体、资源、LLM 摘要）
│   │   ├── device/           # 设备连接运行时（驱动、遥测、心跳）
│   │   ├── event/            # 事件管道（router、throttle、real-time、SSE、保留任务）
│   │   ├── alarm/            # 告警规则 + 通知（rule_type='device' | 'event'）
│   │   ├── agent/            # AI Agent（chat、config、tools、session、memory）
│   │   ├── template/         # 物模板（创建时蓝图）
│   │   ├── marketplace/      # 应用市场（驱动 / 物模板）
│   │   ├── workspace/        # 工作空间（含知识资源）
│   │   ├── mcp/              # 内嵌 MCP Server
│   │   └── ...               # auth, chat, cron, jobs, open, system 等
│   ├── shared/               # 跨模块组件（persistence, security, error_handling, utils）
│   ├── tests/                # 集成测试
│   ├── lib.rs                # 库入口
│   └── main.rs               # 程序入口
├── migrations/               # 数据库迁移文件
├── templates/                # 物模板
├── Cargo.toml                # Rust 项目配置
└── README.md                 # 后端说明
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

### 物管理（Thing）
- `GET /api/v1/things` - 获取物列表
- `POST /api/v1/things` - 创建物
- `GET /api/v1/things/{id}` - 获取物详情
- `PUT /api/v1/things/{id}` - 更新物
- `DELETE /api/v1/things/{id}` - 删除物
- `GET /api/v1/things/{id}/profile` - 获取物画像（属性/事件/操作 + LLM 本体摘要）
- `GET /api/v1/things/{id}/ontology` - 获取物本体定义
- `GET /api/v1/things/{id}/tree` - 获取物层级树
- `POST /api/v1/things/{id}/actions/{name}/invoke` - 调用物操作（需确认）
- `POST /api/v1/things/{id}/actions/{name}/confirm` - 确认并执行物操作
- `POST /api/v1/things/import/dtdl` / `POST /api/v1/things/import/wot` - DTDL / WoT 模型导入
- `GET /api/v1/things/templates/{id}/export/dtdl` - DTDL 模型导出
- `GET /api/v1/things/resources/unassigned` - 获取未关联的知识资源
- `POST /api/v1/things/{id}/resources` - 关联知识资源到物
- `DELETE /api/v1/things/{id}/resources/{rid}` - 解除物的资源关联

> **破坏性变更（v0.4.5.0）**：`/api/v1/devices` 管理端点已移除，请迁移至 `/api/v1/things`。设备连接运行时接口（dashboard、命令执行、trace）仍保留在 `/api/v1/devices` 下。

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
- `GET /api/v1/alarms/recent` - 获取最新告警
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

### 自愈引擎
- `GET /api/v1/self-healing/probes` - 获取探针列表
- `POST /api/v1/self-healing/probes` - 创建探针
- `GET /api/v1/self-healing/status` - 获取自愈状态

### 事件系统
- `GET /api/v1/events` - 获取事件列表
- `GET /api/v1/events/sse` - SSE 事件流订阅
- `GET /api/v1/events/real-time` - 实时事件查询

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
use tinyiothub_web::response::ApiResponseBuilder;

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

1. 在 `crates/tinyiothub-runtime/src/driver/drivers/` 创建驱动文件
2. 实现 `DeviceDriver` trait
3. 在 `mod.rs` 中注册驱动

```rust
// 示例：创建新驱动
use tinyiothub_core::driver::{DeviceDriver, DriverResult};

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
cd cloud
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

#### 测试与检查

```bash
cargo test           # 运行后端测试
just ci              # 完整 CI 检查（fmt + clippy + test）
```

## 鸿蒙系统部署（实验性）

HarmonyOS 支持处于实验阶段，部署脚本：`scripts/deploy-to-ohos.ps1`（Windows）。

## MQTT 主题

```
gateway/{sn}/heartbeat        # 心跳消息
gateway/{sn}/device_regist    # 设备注册
gateway/{sn}/command          # 命令下发
gateway/{sn}/device_command   # 设备命令
gateway/{sn}/data             # 数据上传
gateway/{sn}/alarm            # 告警消息
thing/{thing_id}/event/{event_name}   # 物事件上报（节流 60/分钟，error/critical 不节流）
```

物事件上报的完整协议契约见 [docs/device-event-contract.md](docs/device-event-contract.md)。

## 最新动态

- **物本体（Thing Ontology）**: 设备泛化为「物」— 层级树、物模板蓝图、属性/事件/操作、LLM 本体摘要，全新 `/api/things` API（v0.4.5.0）
- **沉浸式工作空间**: 3D 数字孪生场景 + AI 数据洞察面板，可折叠执行过程，玻璃拟态 UI
- **A2UI 协议**: 27 种动态 UI 组件（设备卡片、数据图表、Scene3D 等），AI 实时渲染仪表盘
- **工作空间资源管理**: 场景模型、设备模型、图片、文档的上传与管理，语义搜索
- **Cron 定时任务**: 统一调度服务，Workspace 隔离，执行记录与统计


## 许可证

MIT License - 详见 [LICENSE](LICENSE) 文件