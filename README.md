# TinyIoTHub

**给设备写 harness。** 设备接入是底座，真正的东西是中间那层 agent 运行环境。

> Claude Code 给代码写 harness——skills、todo、子代理、审批。TinyIoTHub 把同一套东西写给物理世界：
> 一个常驻 Assistant 被设备事件和定时 tick 唤醒，查本体、做决策、按权限模式操作设备；
> 动手前过审批，动手后回读属性验证，全程进审计账本。

> **项目状态**：v0.x，正在从 Workflow-first 迁移到统一 Session Loop。Harness 层与设备接入均可用；
> Modbus/ONVIF/SNMP 驱动仍在开发；旧执行路径尚未删净，API 在 1.0 前可能有破坏性变更。欢迎试用与反馈。

**官方网站**: https://tinyiothub.com  
**仓库地址**: https://github.com/Grong/tinyiothub  
**Docker Hub**: https://hub.docker.com/r/grong/tinyiothub

## 这是什么

三层结构，价值在中间那层：

| 层 | 内容 | 竞争状态 |
|---|---|---|
| 应用 | 巡检、故障诊断、心跳监控、你自己写的 `SKILL.md`；节能 / 视频分析在路线图上 | 场景可复制 |
| **Harness** | Session Loop · Skill · Todo · Ask · Subagent · 安全层 | **差异化所在** |
| 设备 | MQTT / Modbus / ONVIF / SNMP · 物本体与层级树 | 底座，必须够用 |

设备接入是前提，应用是产物，**harness 是产品本体**。场景差异写进 `SKILL.md`，不写进循环——新增一个「空压机巡检」不需要改 Rust。

## 特性

**Harness**
- 统一 Session Loop：`SessionRunner` 是一次 Assistant Turn 的唯一执行入口，对话、定时巡检、事件处置共用同一条循环
- 触发与执行分离：`TriggerRouter` 只产生 wake 不执行 Turn——30 秒扫描到期 Schedule 与设备事件，每 workspace 至多一个在途 wake，事件按 workspace+trigger 做 3/min 去重
- Skill 渐进披露：`SKILL.md` 只把 name + description 放进上下文，正文调用时才注入；支持 `/skill-name` 直接触发
- `todo_write`：模型自维护待办，整体替换、同一时间只允许一个 `in_progress`，前端实时渲染
- `ask_user`：1–4 个问题、每题 2–4 个选项、可多选，阻塞当前 turn 等你决定；无人在线则暂停该任务并通知，不静默失败
- `spawn_subagent`：同步派生子代理，上下文隔离、只做一层、返回 `status / summary / evidence_refs / recommended_actions`
- 安全层：policy / trust / approval，Effect 与 Decision 单赢家，unknown 状态停下对账，动作可恢复且不重复
- 权限三档：`read_only` 只看 / `ask` 写入需批准（默认）/ `full_access` 会话内免逐次批准——**都不绕过联锁与 e-stop**
- 可见性：`session_events` 是唯一 UI 时间线（session / turn / skill / todo / question / subagent / tool / approval 全量事件），全局 HUD 显示 Assistant 状态、设备在线离线、告警、待你回答、待你审批、下次唤醒
- 零配置可运行：新 workspace 建好即有内置技能与默认职责；改 `AGENTS.md` 就是「告诉大脑注意什么、干什么活」

**设备与本体**
- 物本体模型：设备、空间、产线统一为层级化「物」，属性/事件/操作 + 知识文档
- 协议支持：MQTT 可用；Modbus RTU/TCP、ONVIF、SNMP 驱动开发中（驱动框架与 SDK 已就绪）
- AI 驱动匹配（实验性）：描述设备类型，辅助匹配或生成驱动代码
- 物模板：JSON 模板一键创建，支持 DTDL / WoT 模型导入与 DTDL 导出
- L0-L3 分级自愈引擎：system/device/task 三级探针，自动故障检测与恢复
- 规则引擎：阈值、范围、变化、持续时间、组合五种条件类型
- 心跳探针：定期自检网关与子设备存活，诊断结论转交 Assistant 处置
- Cron 定时任务：Workspace 隔离，执行记录与统计

**接入与部署**
- 自然语言交互：用日常语言配置设备、查询状态、排查故障
- MCP Server：内嵌 Model Context Protocol 服务，Claude Desktop、Cursor 直接连接
- A2UI 聊天界面：SSE 流式对话，Agent 技能调用
- 单进程部署：~80MB 内存占用，无需外部数据库；SQLite 零依赖，自动迁移
- 开源 MIT 协议：可私有化部署，可商用
- Lit 3 前端：Web Components，轻量快速

## 项目结构（多 Crate 架构）

```
tinyiothub/
├── apps/                    # 可部署二进制
│   ├── cloud/               # relay：全部行为（domains/ 按域分模块）+ 组合根
│   │   ├── src/
│   │   │   └── domains/     # thing/auth/user/tenant/event/alarm/driver/notify/agent/mcp/admin
│   │   └── templates/       # 技能模板
│   ├── edge/                # 边缘网关
│   ├── marketplace/         # 市场服务（驱动/物模板市场）
│   └── cli/                 # 命令行工具
├── crates/                  # 能力库 Crate（relay 范式：只提供能力，不做编排）
│   ├── core/                # 纯值类型（零 I/O、零 API 语义）
│   ├── db/                  # 全部 SQL：Db 门面 + 领域平铺文件（行类型+查询函数+impl Db 委托）+ migrations/（baseline + DDL-only）
│   ├── runtime/             # EventBus, DataServer, 驱动框架, plugin loader
│   ├── web/                 # HTTP 基础设施（中间件、ApiResponseBuilder）
│   ├── scheduler/           # Cron 引擎
│   ├── llm/                 # LLM provider 契约
│   ├── memory/              # Agent 记忆引擎（纯逻辑）
│   ├── policy/              # 策略门评估（纯逻辑）
│   ├── skills/              # 技能/信任引擎
│   ├── agent/               # Agent 共性能力运行时（loop/pool/tools/session/prompt，零 axum/零 sqlx）
│   ├── authn/               # 认证机制（JWT/SSE token/密码哈希，纯机制）
│   ├── plugin-sdk/          # 驱动开发 SDK（ABI 契约）
│   └── macros/              # 过程宏
├── drivers/                 # 动态驱动 stub（cdylib，不在 workspace 内）
├── web/                     # Lit 3 前端应用 (Web Components)
│   ├── src/                 # 源代码
│   │   ├── ui/             # Lit 组件、页面、聊天/A2UI
│   │   ├── api/            # API 客户端
│   │   ├── i18n/           # 国际化
│   │   ├── styles/         # CSS 样式
│   │   └── stores/         # nanostore 状态管理
│   ├── package.json         # Node.js 项目配置
│   └── vite.config.ts      # Vite 构建配置
├── examples/                # 示例项目
│   ├── example-plugin/     # 插件示例
│   └── bacnet-driver/      # BACnet 驱动示例
├── vendor/                  # 第三方依赖（本地 fork，如 onvif-rs）
├── scripts/                # 工具脚本
├── deploy/                 # Docker 部署（docker-compose、边缘镜像）
├── docs/                   # 项目文档
└── skills/                 # AI prompts / skills
```

**注意：本项目采用 relay 范式（buzz-relay 模型）：apps/cloud 拥有全部行为，crates/ 为互不惊扰的能力库，依赖方向 `apps/* → crates/* → core` 单向不可逆。详细架构见 [AGENTS.md](AGENTS.md)。**

Harness 的落位：能力层在 `crates/agent`（port / adapters / harness / memory，零 axum、零 sqlx），编排层在 `apps/cloud/src/domains/agent/session/`——`runner.rs` 是 Turn 的执行入口，`trigger.rs` 只产 wake，`tools/` 放 `todo_write` / `ask_user` / `spawn_subagent`，`todos.rs` / `questions.rs` / `approvals.rs` 管会话内状态与阻塞，`hud.rs` 提供全局投影。安全与效果内核在 `crates/policy`、`crates/agent/src/port`（Effect / Decision / 恢复）。设计规范见 [docs/superpowers/specs/2026-11-07-assistant-harness-design.md](docs/superpowers/specs/2026-11-07-assistant-harness-design.md)。

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
cd apps/cloud
cargo run
```

**前端**:
```bash
cd web
pnpm install
pnpm dev
```

访问: http://localhost:5173

#### 生产模式（单进程部署）

**构建**:
```bash
# 前端静态资源 + 后端发布构建（仓库根目录）
cd web && pnpm build && cd ..
cargo build --release -p tinyiothub-cloud
# 或：just build（后端 release）；容器构建见 Dockerfile
```

**运行**:
```bash
cd apps/cloud
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
  port: 5173,
  proxy: {
    '/api': 'http://localhost:3002'
  }
}
```

### 访问服务

启动后访问以下地址：

- **Web 管理界面**: http://localhost:5173/ (前端开发服务器)
- **后端 API**: http://localhost:3002/api/v1/
- **健康检查**: http://localhost:3002/health（返回纯文本 OK）

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
        Lit 3 UI  ── REST /api/v1 ── MCP /mcp ── MQTT Client
                          │
                          ▼
        apps/cloud   (the relay — 全部行为与编排)
          domains/: thing · auth · user · tenant · event · alarm
                    driver · notify · agent/session · mcp · admin
                          │ 单向依赖
                          ▼
        crates/*     (capability libs — 只提供能力，不编排)
          agent · runtime · db · web · policy · skills · memory
          scheduler · llm · authn · plugin-sdk
                          │
                          ▼
        core         (纯值类型 — 零 I/O、零 API 语义)
```

一次 Assistant Turn 在 `apps/cloud/src/domains/agent/session/` 内的流转：

```
Trigger(chat / schedule / event) → TriggerRouter → Wake → SessionRunner
                                                          │
                                      ┌───────────────────┼───────────────────┐
                                      ▼                   ▼                   ▼
                                 Skill 正文            todo_write          ask_user
                                 (按需注入)           (模型自维护)        (阻塞等回答)
                                      │                   │                   │
                                      └───────────────────┼───────────────────┘
                                                          ▼
                                                 工具层（安全边界）
                                    policy · trust · approval · Effect/Decision
                                          unknown 对账 · 回读验证
                                                          │
                                                          ▼
                                                 session_events（唯一时间线）
```

### 后端目录结构 (apps/cloud/)

```
apps/cloud/
├── src/
│   ├── main.rs               # 薄入口（<200 行）：读配置 → 建 AppState → 组装 router → serve
│   ├── lib.rs                # 模块注册（deny(unsafe_code)）
│   ├── bootstrap.rs          # 启动逻辑（日志、驱动重载、设备缓存预热）
│   ├── state.rs              # AppState — 唯一共享状态
│   ├── router.rs             # build_router：全部路由挂载在一处
│   ├── api/                  # HTTP 中间件与抽取器（WorkspaceScope / auth）
│   ├── domains/              # 全部业务行为，一域一目录
│   │   ├── thing/  auth/  user/  tenant/  event/  alarm/
│   │   ├── driver/  notify/  mcp/  admin/
│   │   └── agent/            # harness 编排层
│   │       ├── session/      # SessionRunner · TriggerRouter · tools · todo · ask · subagent · HUD
│   │       └── prompt/       # 统一 prompt builder
│   ├── shared/               # 组合层胶水（service_manager / paths / initialization）
│   └── tests/                # 集成测试
├── templates/                # 技能模板（SKILL.md）
├── Cargo.toml
└── README.md
```

业务行为代码位于 `apps/cloud/src/domains/<domain>/`（thing, auth, user, tenant, event,
alarm, driver, notify, agent, mcp, admin），每域含 handler/service/dto；路由在
`apps/cloud/src/api/mod.rs` 集中挂载；全部 SQL 与迁移位于 `crates/db/`。

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

设计系统与样式规范见 [web/DESIGN_SYSTEM.md](web/DESIGN_SYSTEM.md)，前端分层规范见 [FRONTEND_LAYERING_GUIDE.md](FRONTEND_LAYERING_GUIDE.md)。

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
- `GET /api/v1/workspaces/{workspace_id}/agent/hud` - Assistant 全局 HUD（状态/设备/告警/待答/待批/调度）
- `GET /api/v1/workspaces/{workspace_id}/agent/schedule` - 获取 Assistant 调度
- `PUT /api/v1/workspaces/{workspace_id}/agent/schedule` - 更新 Assistant 调度（启用状态/间隔/提示词；权限模式走 `/agent/policy`）
- `POST /api/v1/judgments/{id}/approve` - 已退役（410）：报警处置由 Assistant 会话承接，请在 Assistant 中确认设备写操作
- `GET /api/v1/workspaces/{workspace_id}/agent/tasks` - 获取 Task 列表
- `POST /api/v1/workspaces/{workspace_id}/agent/tasks` - 创建 Task
- `GET /api/v1/workspaces/{workspace_id}/agent/tasks/{task_id}/stream` - Task SSE 流
- `GET /api/v1/chat/sessions` - 获取 Assistant 会话列表
- `POST /api/v1/chat/sessions/{session_key}/label` - 更新会话标签
- `DELETE /api/v1/chat/sessions/{session_key}` - 删除会话
- `GET /api/v1/chat/sessions/{session_key}/events` - 获取会话事件
- `GET /api/v1/chat/sessions/{session_key}/events/stream` - 会话事件 SSE
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

贡献流程、分支策略与提交规范见 [CONTRIBUTING.md](CONTRIBUTING.md)。

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

1. 在 `crates/runtime/src/driver/drivers/` 创建驱动文件
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
# 后端（仓库根目录对整个 workspace 生效）
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

- **Harness 统一循环**：对话、定时巡检、事件处置合并为唯一 Session Loop；`TriggerRouter` 只产 wake 不执行；Workflow 声明式执行退役（[设计](docs/superpowers/specs/2026-11-07-assistant-harness-design.md)）
- **主流 harness primitive 落地**：`SKILL.md` 渐进披露、`todo_write`、`ask_user`、`spawn_subagent`；Plan mode 待做
- **会话时间线 + 全局 HUD**：`session_events` 作为唯一 UI 时间线；HUD 常驻显示 Assistant 状态、设备、告警、待答、待批、下次唤醒
- **物本体（Thing Ontology）**: 设备泛化为「物」— 层级树、物模板蓝图、属性/事件/操作、LLM 本体摘要，全新 `/api/things` API（v0.4.5.0）
- **A2UI 协议**: 27 种动态 UI 组件（设备卡片、数据图表、Scene3D 等），AI 实时渲染仪表盘
- **Cron 定时任务**: 统一调度服务，Workspace 隔离，执行记录与统计


## 许可证

MIT License - 详见 [LICENSE](LICENSE) 文件