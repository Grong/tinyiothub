# Chat & Agent + A2UI Feature Design

## Context

TinyIoTHub needs chat and agent management capabilities. The OpenClaw UI codebase (`/Users/chenguorong/code/github/openclaw/ui/`) has a mature implementation of both — we reference and adapt its patterns (not copy verbatim) for TinyIoTHub's Lit-based frontend. The existing web/ codebase already has chat CSS, OpenClaw-compatible types, and UI settings — but zero Lit component implementation.

## Architecture

```
TinyIoTHub 前端 (Lit 3, Light DOM)
        ↓ HTTP + SSE
TinyIoTHub 后端 (Rust/Axum) — 新增代理端点
        ↓ HTTP + SSE
OpenClaw Gateway — chat.* / agents.* / tools.* / config.* RPC
```

- 前端不直接连接 OpenClaw Gateway，所有请求经过 TinyIoTHub 后端代理
- 后端统一走 JWT 认证、`{ code, msg, result }` 响应格式
- SSE 流式响应用于 chat 消息和 A2UI 推送

## Routes

| 路由 | 组件 | 说明 |
|------|------|------|
| `/chat` | `<view-chat>` | 聊天页面：SSE 流式对话、会话管理、A2UI 交互组件 |
| `/agents` | `<view-agents>` | Agent 管理：6 面板（概览、文件、工具、技能、渠道、定时任务） |

## File Structure

```
web/src/
├── ui/
│   ├── views/
│   │   ├── chat.ts                  # 聊天主视图组件
│   │   └── agents.ts                # Agent 管理视图组件（6 面板）
│   ├── controllers/
│   │   ├── chat.ts                  # 聊天状态机：历史加载、发送、流式事件
│   │   └── agents.ts                # Agent CRUD、配置保存、工具/技能管理
│   ├── chat/
│   │   ├── grouped-render.ts        # 消息分组渲染
│   │   ├── message-normalizer.ts    # 原始消息标准化
│   │   └── a2ui/
│   │       ├── a2ui-renderer.ts     # A2UI JSON → Lit 模板调度器
│   │       └── catalog/
│   │           ├── text.ts
│   │           ├── button.ts
│   │           ├── card.ts
│   │           ├── column.ts
│   │           ├── row.ts
│   │           ├── divider.ts
│   │           ├── device-card.ts
│   │           ├── device-table.ts
│   │           ├── data-chart.ts
│   │           ├── control-panel.ts
│   │           ├── progress-indicator.ts
│   │           ├── confirmation-dialog.ts
│   │           └── index.ts         # 组件注册表 + re-exports
│   └── app.ts                       # 添加 /chat 和 /agents 路由
├── styles/                          # CSS 已存在，无需新建
│   ├── chat.css
│   ├── chat/layout.css
│   ├── chat/text.css
│   ├── chat/grouped.css
│   ├── chat/tool-cards.css
│   └── chat/sidebar.css
```

## Chat Page

### Component: `<view-chat>`

Lit 组件，管理本地 UI 状态（draft、scroll、slash menu），通过属性和回调与控制器通信。

**Key state:**
- `chatMessages: ChatMessage[]` — 消息列表
- `chatStream: string` — 当前流式文本缓冲
- `chatSending: boolean` — 是否正在发送
- `chatRunId: string | null` — 当前运行 ID
- `sessionKey: string` — 当前会话键
- `sessionsList: SessionRow[]` — 会话列表
- `draft: string` — 输入框草稿
- `attachments: ChatAttachment[]` — 附件

**Key methods:**
- `handleSend(text, attachments)` — 发送消息
- `handleAbort()` — 中止当前运行
- `handleNewSession()` — 新建会话
- `renderMessages()` — 渲染消息列表（使用 grouped-render）
- `renderInputBar()` — 渲染输入栏

### Controller: `controllers/chat.ts`

状态机模块，操作 `ChatState` 对象。

**State machine (delta → final/aborted/error):**
1. `delta` — 增量文本追加到 `chatStream` 缓冲区
2. `final` — 标准化消息追加到 `chatMessages`，清空流状态
3. `aborted` — 类似 final，优先用 payload message，回退到 stream buffer
4. `error` — 清空流状态，设置 `lastError`

**RPC calls (via 后端代理):**
- `chat.history` — 加载最近 200 条消息
- `chat.send` — 发送消息（带 idempotencyKey = runId）
- `chat.abort` — 中止运行

### Message Rendering: `grouped-render.ts`

- 同角色连续消息合并（Slack 风格）
- 头像、时间戳、消息气泡
- 工具执行卡片（实时更新）
- 思维链折叠/展开
- Markdown 渲染（marked + DOMPurify）
- A2UI 组件嵌入在消息气泡中

### SSE Streaming

后端 `POST /api/v1/chat/stream` 返回 SSE 流：
```
data: {"runId":"xxx","state":"delta","message":{"role":"assistant","content":[{"type":"text","text":"Hello"}]}}

data: {"runId":"xxx","state":"delta","message":{"role":"assistant","content":[{"type":"text","text":" world"}]}}

data: {"runId":"xxx","state":"final","message":{"role":"assistant","content":[{"type":"text","text":"Hello world"}]}}
```

带 A2UI 的事件：
```
data: {"runId":"xxx","state":"delta","message":{...},"a2ui":"{\"createSurface\":{\"id\":\"s1\",\"surfaceKind\":\"inline\"}}\n{\"updateComponents\":{\"components\":[{\"id\":\"c1\",\"componentKind\":\"DeviceCard\",\"dataModel\":{\"deviceId\":\"dev-001\"}}]}}"}
```

## A2UI Integration

### Data Flow

```
OpenClaw Agent → canvas tool (a2ui_push)
    ↓
SSE 事件携带 a2ui JSONL 字段
    ↓
控制器解析 canvas tool 调用 → 提取 a2ui_push jsonl
    ↓
a2ui-renderer.ts 解析 JSONL 消息序列:
  createSurface    → 创建 inline/overlay 容器
  updateComponents → 添加/更新组件实例
  updateDataModel  → 更新数据绑定
  callFunction     → 用户交互（按钮点击）→ 发回 action
    ↓
根据 componentKind 查找 catalog 注册表
    ↓
渲染为对应的 Lit 模板 → 嵌入消息气泡
```

### Standard Catalog Components (from OpenClaw)

Text, Image, Icon, Row, Column, List, Card, Tabs, Modal, Divider, Button, TextField, CheckBox, ChoicePicker, Slider, DateTimeInput

### IoT Extended Components

| 组件 | componentKind | 用途 |
|------|--------------|------|
| DeviceCard | `DeviceCard` | 设备状态卡片 |
| DeviceTable | `DeviceTable` | 设备列表表格 |
| DataChart | `DataChart` | 实时数据图表 |
| ControlPanel | `ControlPanel` | 设备控制面板 |
| ProgressIndicator | `ProgressIndicator` | 进度指示器 |
| ConfirmationDialog | `ConfirmationDialog` | 确认对话框 |

### A2UI Renderer: `a2ui-renderer.ts`

- 维护 `SurfaceMap`（surfaceId → 组件树）
- `handleA2uiMessage(jsonl: string)` — 解析 JSONL 消息序列
- `renderSurface(surfaceId)` — 渲染 surface 内所有组件
- 组件注册表：`componentKind → renderFunction` 映射
- `callFunction` 事件通过 SSE 回传给后端

## Agents Page

### Component: `<view-agents>`

Agent 选择器 + 6 标签页。

**Key state:**
- `agentsList: AgentsListResult` — agent 列表
- `selectedAgentId: string` — 当前选中 agent
- `activePanel: AgentsPanel` — 当前面板
- `config: ConfigSnapshot` — agent 配置
- `toolsCatalog: ToolsCatalogGroup[]` — 工具目录
- `toolsEffective: ToolsEffectiveState` — 有效工具配置
- `files: AgentFileEntry[]` — agent 文件
- `skills: SkillStatusEntry[]` — 技能列表
- `channelsStatus: ChannelsStatusSnapshot` — 渠道状态
- `cronJobs: CronJob[]` — 定时任务

### Controller: `controllers/agents.ts`

**RPC calls (via 后端代理):**
- `agents.list` — 获取 agent 列表
- `config.get` / `config.set` — 配置读写（optimistic concurrency via baseHash）
- `tools.catalog` — 工具目录
- `tools.effective` — 有效工具配置
- `tools.toggle` — 切换工具状态
- `agents.files.list` / `agents.files.get` / `agents.files.put` — 文件 CRUD
- `cron.list` / `cron.create` / `cron.delete` / `cron.run` — 定时任务管理

### 6 Panels

1. **概览 (Overview)**：主模型/备选模型选择 chips、workspace 显示、配置保存/重载按钮、dirty 状态跟踪
2. **文件 (Files)**：文件树 + 编辑器 + Markdown 预览对话框
3. **工具 (Tools)**：profile 预设按钮、按 section 分组的工具开关、allow/deny/alsoAllow 策略
4. **技能 (Skills)**：过滤输入框、可折叠分组、逐个技能开关
5. **渠道 (Channels)**：渠道状态快照、账号连接/错误状态
6. **定时任务 (Cron)**：任务列表、Run Now 按钮、创建/编辑表单

## Backend Proxy Endpoints

```
POST   /api/v1/chat/stream         → OpenClaw chat.send (SSE)
GET    /api/v1/chat/history        → OpenClaw chat.history
POST   /api/v1/chat/abort          → OpenClaw chat.abort
GET    /api/v1/sessions            → OpenClaw sessions.list
POST   /api/v1/sessions/reset      → OpenClaw sessions.reset

GET    /api/v1/agents              → OpenClaw agents.list
GET    /api/v1/agents/:id/config   → OpenClaw config.get
PUT    /api/v1/agents/:id/config   → OpenClaw config.set
GET    /api/v1/agents/:id/files    → OpenClaw agents.files.list
GET    /api/v1/agents/:id/files/:path → OpenClaw agents.files.get
PUT    /api/v1/agents/:id/files/:path → OpenClaw agents.files.put

GET    /api/v1/tools/catalog       → OpenClaw tools.catalog
GET    /api/v1/tools/effective     → OpenClaw tools.effective
POST   /api/v1/tools/toggle        → OpenClaw tools.toggle

GET    /api/v1/channels/status     → OpenClaw channels.status

GET    /api/v1/cron                → OpenClaw cron.list
POST   /api/v1/cron                → OpenClaw cron.create
DELETE /api/v1/cron/:id            → OpenClaw cron.delete
POST   /api/v1/cron/:id/run        → OpenClaw cron.run
```

## Implementation Phases

| Phase | Scope | 说明 |
|-------|-------|------|
| 1 | 后端代理端点 | Rust handlers → OpenClawAgentClient 扩展 |
| 2 | Chat 页面 | 视图 + 控制器 + 消息渲染 + SSE 流式 |
| 3 | A2UI 渲染层 | a2ui-renderer + 6 个 IoT catalog 组件 |
| 4 | Agents 页面 | 控制器 + 6 面板视图 |
| 5 | 路由集成 | app.ts 导航、路由、sidebar 更新 |

## Reuse from Existing Codebase

| 已有资源 | 位置 | 用途 |
|---------|------|------|
| Chat CSS (6 files) | `src/styles/chat*.css` | 完整聊天样式 |
| OpenClaw 类型 | `src/ui/types.ts` | GatewaySessionRow, AgentsListResult, ToolCatalogEntry 等 |
| UI 类型 | `src/ui/ui-types.ts` | ChatAttachment, ChatQueueItem, CronFormState |
| 聊天设置 | `src/ui/storage.ts` | chatFocusMode, chatShowThinking, gatewayUrl |
| API 客户端 | `src/api/client.ts` | REST fetch, Bearer token, snake_case 转换 |
| 格式化工具 | `src/ui/format.ts` | stripThinkingTags() |
| 后端代理客户端 | `api/src/infrastructure/openclaw_agent.rs` | OpenClaw HTTP client（需扩展） |

## Verification

1. `pnpm build` — 无 TypeScript 错误
2. `cargo build` — 无 Rust 编译错误
3. 浏览器测试：
   - `/chat` — 能发送消息、接收 SSE 流式响应、消息渲染正确
   - `/chat` — A2UI 组件在消息中正确渲染（DeviceCard、DataChart）
   - `/agents` — agent 列表加载、6 面板切换、配置保存
   - 导航栏显示 Chat 和 Agents 入口
