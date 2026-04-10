# TinyIoTHub Edge Intelligence Agent - Phase 2 Implementation Plan

> **Version**: 2.0.0
> **Date**: 2026-04-04
> **Status**: Eng Review Complete
> **Issue Resolutions**: 1A (OpenClaw API discovery) | 2B (SetProperty workaround) | 3A (trait+mock) | GAP1 (ParseError) | GAP2 (conflict check) | GAP3 (idempotency) | Fix1 (SQLx send_command) | Fix2 (degraded definition) | Fix3 (race condition)
> **Depends on**: Phase 1 (feature/edge-agent-phase1)
> **Target**: feature/edge-agent-phase2

## 1. Overview

### 1.1 Goal

Phase 2 introduces **Workspace** as the core organizational unit for AI-driven device management. Each Workspace maps to a physical/logical environment (factory, building, campus, home) and is associated with one OpenClaw Agent. The MCP tool surface is scoped to Workspace context.

### 1.2 Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        用户交互层                                 │
│  tinyiothub-web (React)  │  小程序  │  第三方应用               │
└─────────────────────────────┬───────────────────────────────────┘
                              │ HTTPS / WebSocket
┌─────────────────────────────▼───────────────────────────────────┐
│                      tinyiothub-api (Rust)                       │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐          │
│  │ 用户/Workspace│  │  设备管理     │  │  数据服务    │          │
│  │     管理      │  │  规则引擎     │  │  时序数据库  │          │
│  └──────────────┘  └──────────────┘  └──────────────┘          │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐          │
│  │  MCP Server  │  │  A2UI 渲染   │  │  驱动管理    │          │
│  │  (工具暴露)   │  │  适配器      │  │  沙箱        │          │
│  └──────────────┘  └──────────────┘  └──────────────┘          │
└─────────────┬─────────────┬─────────────────────┬───────────────┘
              │ MCP协议      │                     │
              │ (stdio/SSE) │                     │
┌─────────────▼─────────────┐                     │
│        OpenClaw 集群       │                     │
│  ┌─────────────────────┐  │                     │
│  │  Agent Orchestrator  │  │                     │
│  └──────────┬──────────┘  │                     │
│     ┌───────┴───────┐      │                     │
│     ▼               ▼      │                     │
│ ┌─────────┐   ┌─────────┐  │                     │
│ │Agent W1 │...│Agent Wn │  │                     │
│ └─────────┘   └─────────┘  │                     │
│  (每个workspace一个agent)    │                     │
└─────────────┬─────────────┘                     │
              │ (可选) 直接调用                     │
              └─────────────────────────────────────┘
```

### 1.3 Core Concepts

```
Tenant (租户)
 └── Workspace (工作空间) — 1:1 with OpenClaw Agent
       ├── agent_id → OpenClaw Agent
       └── Device[] (设备列表) — 1:many, 设备只能属于一个 Workspace
```

### 1.4 Phase 2 Deliverables

| Category | MCP Tools | REST APIs | Domain Modules |
|----------|-----------|-----------|----------------|
| **Workspace Management** | 5 | 6 (CRUD + assign device) | 1 new (`workspace/`) |
| **Automation** | 2 (stub → real) | — | 1 enhanced (`automation/`) |
| **Jobs** | 3 (stub → real) | — | 1 enhanced (`jobs/`) |
| **Batch Command** | 1 | 1 | 1 new (`infrastructure/batch_command`) |
| **OTA** | 3 | 4 | 1 new (`ota/`) |
| **Logs** | 2 | 1 | — |
| **Timeseries** | 3 | 2 | 1 new (`timeseries/`) + InfluxDB wiring |
| **Alarm MCP** | 4 | — | — |
| **Device Enhanced** | 3 | — | 1 new (`infrastructure/diagnostics`) |
| **Skills** | 4 new + 4 enhanced | — | — |
| **AI Chat UI** | — | — | 1 new page (`web/app/(commonLayout)/ai-chat/`) |
| **Total** | **27 new/enhanced** | **14** | **5** + 1 web page |

---

## 1.5 AI Chat UI — Page Specification (P0, NEW)

### 1.5.1 Page Overview

**Page**: `web/app/(commonLayout)/ai-chat/page.tsx`
**Layout**: 居中全宽对话 — 设备卡片以内联形式出现在对话流中
**Access**: Authenticated users, scoped to current workspace via JWT

### 1.5.2 Layout Structure

```
┌─────────────────────────────────────────────────────────────────┐
│  [Sidebar]  │  FULL-WIDTH CONVERSATION AREA (max-w-4xl centered) │
│              │                                                        │
│              │  ┌──────────────────────────────────────────────┐   │
│              │  │ Workspace: Building A  │ Agent: ● Online   │   │  ← Context bar (minimal)
│              │  └──────────────────────────────────────────────┘   │
│              │                                                        │
│              │  ┌─ AI Message ────────────────────────────────┐   │
│              │  │ 你好！我是 Building A 的 AI 助手。           │   │
│              │  │ 可以帮你查询设备状态、发送命令、诊断故障。     │   │
│              │  │                                              │   │
│              │  │ 示例命令：                                   │   │
│              │  │ • 查看 Floor 3 所有设备状态                   │   │
│              │  │ • 温度传感器 #7 最近有没有异常？               │   │
│              │  └──────────────────────────────────────────────┘   │
│              │                                                        │
│              │              ┌─ User Message ─────────────────┐   │
│              │              │ Floor 3 的空调设备状态如何？     │   │
│              │              └─────────────────────────────────┘   │
│              │                                                        │
│              │  ┌─ AI Message ────────────────────────────────┐   │
│              │  │ 正在查询 Floor 3 的空调设备...              │   │  ← Loading state
│              │  └──────────────────────────────────────────────┘   │
│              │                                                        │
│              │  ┌─ AI Device Card (inline) ──────────────────┐   │
│              │  │ 🏠 Floor 3 空调设备 (3 台)                 │   │
│              │  │ ─────────────────────────────────────────  │   │
│              │  │ AC-001  ● Online  温度: 24.5°C  运行中    │   │
│              │  │ AC-002  ● Online  温度: 25.1°C  运行中    │   │
│              │  │ AC-003  ⚠ Offline 温度: --      —        │   │
│              │  └──────────────────────────────────────────────┘   │
│              │                                                        │
│              │                              [Empty space / scroll]   │
│              │                                                        │
│              │  ┌──────────────────────────────────────────────┐   │
│              │  │ [🤖 AI typing...]                         │   │  ← Typing indicator
│              │  └──────────────────────────────────────────────┘   │
│              │                                                        │
│              │  ┌──────────────────────────────────────────────┐   │
│              │  │ 输入你的问题...                        [Send] │   │  ← Input area (fixed bottom)
│              │  └──────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

### 1.5.3 信息层级

**第一眼可见** (above the fold):
1. Workspace 上下文条（workspace 名 + agent 状态指示）
2. 最新对话消息（或空状态的欢迎语）
3. 输入框

**用户第二眼**:
4. 历史对话线程（滚动可见）
5. 设备状态卡片（A2UI 渲染）

**第三眼**:
6. Agent 降级警告（如果 OpenClaw 不可用）

### 1.5.4 Message Types

| 类型 | 样式 | 说明 |
|------|------|------|
| `user_message` | 右对齐，蓝色背景 (#155aef)，白色文字 | 用户输入 |
| `ai_text` | 左对齐，深色背景 (var(--color-bg-secondary)) | AI 纯文本回复 |
| `ai_device_card` | 左对齐，带边框的卡片，内含设备表格/状态 | 设备状态结构化展示 |
| `ai_action_confirm` | 左对齐，成功(绿色)/失败(红色)指示器 | 命令执行结果 |
| `ai_error` | 左对齐，红色边框警告样式 | 错误提示 |
| `system_warning` | 左对齐，黄色背景横幅 | Agent 降级/不可用警告 |
| `typing_indicator` | 左对齐，三个跳动的点 | AI 思考中 |

### 1.5.5 Interaction States

| 状态 | 用户看到什么 |
|------|------------|
| **Empty (first time)** | 欢迎语 + 3 个示例命令按钮 |
| **Loading (AI thinking)** | 消息气泡显示 "..." 动画 + typing indicator |
| **Success** | AI 回复 + 内联设备卡片 |
| **Partial success** | AI 回复部分成功（如 3 台设备中 2 台在线）|
| **Error (network)** | 红色警告 + "重试" 按钮 |
| **Error (agent unavailable)** | 黄色系统横幅 "OpenClaw Agent 不可用，部分功能受限" + 可用功能继续 |
| **Empty results** | AI 回复 "没有找到匹配的设备" |

### 1.5.6 Empty State (首次访问)

```
┌──────────────────────────────────────────────┐
│                                              │
│          🤖                                  │
│                                              │
│    你好！我是 Building A 的 AI 助手           │
│    可以帮你：                                │
│                                              │
│    ┌──────────────────────────────────────┐  │
│    │ 📊 查看所有设备状态                   │  │  ← 可点击的示例命令
│    └──────────────────────────────────────┘  │
│    ┌──────────────────────────────────────┐  │
│    │ 🔧 诊断设备问题                       │  │
│    └──────────────────────────────────────┘  │
│    ┌──────────────────────────────────────┐  │
│    │ ⚡ 执行批量命令                       │  │
│    └──────────────────────────────────────┘  │
│                                              │
│    [输入你的问题...                    Send] │  ← 输入框始终可见
└──────────────────────────────────────────────┘
```

### 1.5.7 Key Component Inventory

| 组件 | 文件 | 说明 |
|------|------|------|
| `ai-chat-page` | `web/app/(commonLayout)/ai-chat/page.tsx` | 页面入口 |
| `conversation-thread` | `web/app/components/ai-chat/conversation-thread.tsx` | 消息列表 |
| `message-bubble` | `web/app/components/ai-chat/message-bubble.tsx` | 单条消息 |
| `device-status-card` | `web/app/components/ai-chat/device-status-card.tsx` | A2UI 设备卡片 |
| `chat-input` | `web/app/components/ai-chat/chat-input.tsx` | 输入框 |
| `agent-status-bar` | `web/app/components/ai-chat/agent-status-bar.tsx` | 顶部状态条 |
| `typing-indicator` | `web/app/components/ai-chat/typing-indicator.tsx` | 思考动画 |

### 1.5.8 API Integration

- `POST /api/v1/ai-chat/send` — 发送消息，返回 AI 回复流（SSE）
- `GET /api/v1/ai-chat/history?workspace_id=` — 获取对话历史
- `GET /api/v1/agents/:agentId/sessions` — 获取会话列表

### 1.5.9 技术约束

- 使用现有 `web/service/` API 封装
- 使用现有 `web/hooks/` 中的 `useQuery` / `useMutation`
- 遵循 `web/themes/light.css` / `dark.css` CSS 变量系统
- 消息历史存储在 localStorage 或后端（Phase 2 存后端）
- 移动端：输入框 fixed 底部，对话区域可滚动

### 1.5.10 A2UI Rendering

**流式推送**: SSE (Server-Sent Events) — `POST /api/v1/ai-chat/send` 返回 SSE 流

**A2UI 渲染**: Web Components (Lit) — OpenClaw Canvas Host 使用 Lit 构建 A2UI 组件。Phase 2 实现前需确认 OpenClaw 是否提供可复用的 A2UI 组件库（`<device-card>`, `<action-result>` 等）。如不可用，降级为 React 组件方案。

渲染组件：
- `device_card`: 设备状态表格
- `action_result`: 操作结果（成功/失败）
- `diagnostic_report`: 诊断报告
- `timeseries_chart`: 时序数据图表

### 1.5.11 Interaction State Table

| FEATURE | LOADING | EMPTY | ERROR | SUCCESS | PARTIAL |
|---------|---------|-------|-------|---------|---------|
| AI 对话发送 | 气泡显示 "..." + typing indicator | — | 红色警告 + 重试按钮 | AI 回复 + 设备卡片 | AI 回复 "部分成功" |
| 设备卡片 | 骨架屏加载 | "没有找到设备" | "加载失败" + 重试 | 设备列表 + 状态 | 部分设备离线 |
| 输入框 | Send 按钮禁用 + spinner | placeholder 提示 | 网络错误提示 | 清空输入框 | — |
| Agent 状态 | 顶部黄点 "connecting..." | 绿色 "● Online" | 红色 "● Agent 不可用" | — | 黄色 "● 部分功能受限" |
| 对话历史 | 骨架屏 | 欢迎语 + 示例命令 | 错误横幅 | 消息列表 | — |

**Empty State 设计**:
- 温暖友好的欢迎语，不用 "No items found"
- 3 个可点击的示例命令按钮作为引导
- 输入框始终可见，降低用户认知负担

**Error State 设计**:
- 网络错误：红色边框 + 明确的重试操作
- Agent 不可用：黄色系统横幅（不阻断用户操作），说明哪些功能仍然可用

### 1.5.12 User Journey Storyboard

时间维度设计：5秒感官 → 5分钟行为 → 长期关系

```
STEP | USER DOES              | USER FEELS          | PLAN SPECIFIES?
-----|------------------------|---------------------|------------------------------------------
1    | 打开 AI Chat 页面      | 期待："这个能帮我   | 欢迎语立即显示，不需加载
     |                        |  管设备吗？"          | 输入框自动聚焦
-----|------------------------|---------------------|------------------------------------------
2    | 输入第一个命令          | 好奇，测试："它能   | 输入框有 placeholder 示例问题
     | "Floor 3空调状态"      | 听懂吗？"            | 发送按钮有键盘快捷键 (Cmd/Ctrl+Enter)
-----|------------------------|---------------------|------------------------------------------
3    | 看到 AI "正在查询"     | 等待中有信心："它在  | Typing indicator 显示
     |                        | 工作了"              | 不丢话，不闪烁
-----|------------------------|---------------------|------------------------------------------
4    | 看到设备卡片结果        | 哇，效率！"这么快   | 设备卡片在 1-2 秒内出现
     | AC-001, AC-002, AC-003 | 就查完了！"        | 内联在对话流中，不需要切换页面
-----|------------------------|---------------------|------------------------------------------
5    | 点击设备卡片展开详情    | 掌控感："深入看     | 点击展开更多属性（历史数据、告警）
     |                        | 看这个设备"          | 非模态，在原位展开
-----|------------------------|---------------------|------------------------------------------
6    | 发送 follow-up 问题    | 自然对话："像在问   | 对话上下文保持（不丢上一轮状态）
     | "为什么AC-003离线了？"  | 一个值班人员"       | AI 理解上下文，不需要重新描述
-----|------------------------|---------------------|------------------------------------------
7    | 命令执行成功确认        | 安心："动作已执行"  | 绿色确认消息 + 设备状态自动更新
     | "关闭AC-001"           |                    | 不需要手动刷新页面
-----|------------------------|---------------------|------------------------------------------
8    | Agent 不可用（降级）   | 略微失望但理解：    | 黄色横幅："部分功能受限"
     |                        | "只是不完美，但不   | 仍可查看历史数据，只是不有新命令
     |                        | 担心"              |
```

**情感弧线**：好奇 → 测试 → 信任 → 掌控 → 依赖

### 1.5.13 Responsive & Accessibility

**移动端适配 (375px - 768px)**:
- Sidebar 自动折叠（使用 hamburger menu）
- 对话区域全宽
- 输入框 fixed 底部（始终可见）
- 设备卡片：横向滚动替代固定列宽
- 字体大小：最小 14px（触摸目标 44px）

**无障碍设计**:
- 键盘导航：Tab 在输入框和示例按钮之间切换，Enter 发送
- 屏幕阅读器：`role="log"` 用于消息列表，`aria-live="polite"` 用于动态内容
- 颜色对比度：文字与背景对比度 ≥ 4.5:1（WCAG AA）
- Focus 状态：所有可交互元素有明确的 focus ring（2px solid #155aef）
- 触摸目标：所有按钮最小 44x44px
- Agent 状态指示器：有颜色之外的颜色-independent 标识（文字标签 "Online/Offline"）

---

## 2. Architecture

### 2.1 Phase 2 Architecture

```
OpenClaw (AI orchestrator, skill-driven)
    │
    │ MCP over HTTP (Authorization: Bearer <jwt>)
    │     ┌──────────────────────────────────────┐
    │     │ JWT payload contains:               │
    │     │   user_id, tenant_id, workspace_id  │
    │     │   Agent can only access its workspace│
    │     └──────────────────────────────────────┘
    │
    ▼
TinyIoTHub API :3002
    ├── /mcp ─────────────────────────────────────────────────────┐
    │                                                              │
    │   Workspace-Scoped Tools (workspace_id from JWT)             │
    │   ──────────────────────────────────────────────────────────│
    │   device_* (12)        ← 自动过滤到当前 workspace           │
    │   driver_* (7)        ← 过滤到当前 workspace 的驱动         │
    │   heartbeat_* (3)     ← 过滤到当前 workspace               │
    │   self_heal_* (3)    ← 过滤到当前 workspace               │
    │   alarm_* (4)        ← NEW: 封装已有 alarm REST API       │
    │   batch_command (1)   ← NEW: Workspace 级别批量命令        │
    │   diagnose_device (1)  ← NEW: Workspace 内设备诊断         │
    │   timeseries_* (3)    ← NEW: 时序数据查询                │
    │   job_* (3)           ← NEW: 定时任务 MCP 封装            │
    │   ota_* (3)           ← NEW: OTA 升级管理                  │
    │   log_* (2)           ← NEW: 日志查询                    │
    │                                                              │
    │   Workspace Management Tools (workspace_id from JWT)        │
    │   ──────────────────────────────────────────────────────────│
    │   list_workspaces (1)   ← NEW: 当前租户下所有 workspace    │
    │   create_workspace (1)  ← NEW: 创建时同步创建 OpenClaw Agent│
    │   update_workspace (1)  ← NEW: 更新 workspace + Agent 配置 │
    │   delete_workspace (1)  ← NEW: 删除时同步删除 OpenClaw Agent│
    │   assign_device (1)     ← NEW: 设备归属 workspace           │
    │                                                              │
    │   /api/v1/* ── REST APIs                                   │
    │   ──────────────────────────────────────────────────────────│
    │   workspaces (6 endpoints)                                  │
    │   devices (已有, 增加 workspace_id 过滤)                     │
    │   batch (1 endpoint)                                        │
    │   ota (4 endpoints)                                         │
    │   logs (1 endpoint)                                         │
    │   timeseries (2 endpoints)                                  │
    │   alarms (已有)                                               │
    └──────────────────────────────────────────────────────────────┘
          │
          ▼
    Rust Backend
    ├── domain/workspace/       [NEW] workspace CRUD + OpenClaw Agent 联动
    ├── domain/automation/       [ENHANCED] ControlDevice wired to device service
    ├── domain/ota/              [NEW] firmware management
    ├── domain/timeseries/       [NEW] timeseries query service
    └── infrastructure/
          ├── batch_command.rs    [NEW] multi-device command executor
          ├── diagnostics.rs      [NEW] device fault analysis
          └── openclaw_agent.rs  [NEW] OpenClaw Agent API client
```

### 2.2 JWT Context for Workspace-Scoped Access

MCP 工具调用时，JWT payload 包含 `workspace_id`，所有工具自动过滤到该 workspace：

```rust
// JWT payload
struct Claims {
    sub: String,        // user_id
    tenant_id: String,
    workspace_id: String, // 新增：当前 workspace
    exp: i64,
}
```

### 2.3 OpenClaw Agent Lifecycle

```
create_workspace(name, tenant_id)
    │
    ├─→ TinyIoTHub: 创建 workspace 记录 (id, name, tenant_id, agent_id=NULL)
    │
    ├─→ OpenClaw API: POST /agents { name: "workspace-{id}", workspace_id: "..." }
    │       返回 { agent_id: "agent-xxx" }
    │
    └─→ TinyIoTHub: 更新 workspace.agent_id = "agent-xxx"
            └── 保存 agent_id

delete_workspace(id)
    │
    ├─→ TinyIoTHub: 查询 workspace.agent_id
    │
    ├─→ OpenClaw API: DELETE /agents/{agent_id}
    │
    └─→ TinyIoTHub: 删除 workspace 记录
            └── 设备回归 tenant 全局池 (workspace_id=NULL)
```

---

## 3. Database Schema

### 3.1 workspaces 表 (NEW)

```sql
CREATE TABLE workspaces (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    tenant_id TEXT NOT NULL,
    agent_id TEXT,                    -- 关联的 OpenClaw Agent ID
    agent_config TEXT,                 -- Agent 配置 JSON
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,

    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE INDEX idx_workspaces_tenant ON workspaces(tenant_id);
CREATE INDEX idx_workspaces_agent ON workspaces(agent_id);
```

### 3.2 devices 表变更

```sql
-- 新增 workspace_id 字段 (nullable, 设备可属于 workspace)
ALTER TABLE devices ADD COLUMN workspace_id TEXT REFERENCES workspaces(id) ON DELETE SET NULL;

-- 设备归属查询
CREATE INDEX idx_devices_workspace ON devices(workspace_id);
```

---

## 4. Task Breakdown

### Task 11: Workspace Management System (P0)

**Status**: NEW

**Files**:
- Create: `api/src/domain/workspace/mod.rs`
- Create: `api/src/domain/workspace/workspace.rs`
- Create: `api/src/domain/workspace/service.rs`
- Create: `api/src/domain/infrastructure/openclaw_agent.rs` — OpenClaw Agent API client
- Create: `api/src/api/workspaces/mod.rs`
- Create: `api/src/api/workspaces/handlers.rs`
- Create: `migrations/YYYYMMDDHHMMSS_create_workspaces_table.sql`
- Create: MCP tools in `api/src/api/mcp/tools/workspace.rs`

**Migration**:
```sql
CREATE TABLE workspaces (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    tenant_id TEXT NOT NULL,
    agent_id TEXT,
    agent_config TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);
CREATE INDEX idx_workspaces_tenant ON workspaces(tenant_id);
CREATE INDEX idx_workspaces_agent ON workspaces(agent_id);
```

**REST APIs** (6):
| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/workspaces` | List workspaces for current tenant |
| GET | `/api/v1/workspaces/{id}` | Get workspace detail |
| POST | `/api/v1/workspaces` | Create workspace (同步创建 OpenClaw Agent) |
| PUT | `/api/v1/workspaces/{id}` | Update workspace |
| DELETE | `/api/v1/workspaces/{id}` | Delete workspace (同步删除 OpenClaw Agent) |
| POST | `/api/v1/workspaces/{id}/devices` | Assign device to workspace |

**MCP Tools** (5):
| Tool | Description |
|------|-------------|
| `list_workspaces` | List workspaces for current tenant |
| `create_workspace` | Create workspace (同步创建 OpenClaw Agent) |
| `update_workspace` | Update workspace name/agent_config |
| `delete_workspace` | Delete workspace (同步删除 OpenClaw Agent) |
| `assign_device` | Assign a device to workspace |

**Input Schema** (`create_workspace`):
```json
{
  "name": "工业园区A",
  "description": "3号厂房生产线"
}
```

**Output Schema** (`create_workspace`):
```json
{
  "workspace_id": "ws-xxx",
  "name": "工业园区A",
  "agent_id": "agent-xxx",
  "device_count": 0,
  "created_at": "2026-04-04T10:00:00Z"
}
```

**Input Schema** (`assign_device`):
```json
{
  "device_id": "dev-001",
  "workspace_id": "ws-xxx"
}
```

**Validation** (GAP 2 + Fix 3: must verify device归属 + race condition):
- Device must belong to same `tenant_id` as the target workspace
- Device must NOT already belong to another workspace (return error, not silent overwrite)
- If device already in another workspace → return `WorkspaceConflict` error: "device already assigned to workspace X"
- If device is free (workspace_id=NULL) → allow assignment
- **Race condition fix (Fix 3):** Use `SELECT ... FOR UPDATE` within the same transaction when checking assignment, OR add unique constraint `UNIQUE(device_id)` if devices can only belong to one workspace at a time. Two concurrent assign requests to the same free device must not both succeed.

**Step 1**: Create domain module `api/src/domain/workspace/`
- `workspace.rs` — Workspace entity
- `service.rs` — WorkspaceService with OpenClaw Agent 联动

**Step 2**: Create `api/src/infrastructure/openclaw_agent.rs`
- **Trait + Mock** (Issue 3A resolution): Define `OpenClawAgentClient` trait with `create_agent()`, `delete_agent()`, `get_agent()`, `update_agent()`, plus `MockOpenClawAgentClient` for tests
- **Resilient HTTP Client** (Approach B): Reqwest + tower Retry middleware + timeout, graceful degradation if OpenClaw unavailable
- **API Discovery** (Issue 1A resolution): On startup, probe OpenClaw `/agents` endpoint to confirm API shape. If probe fails, log warning and continue — workspace operations degrade gracefully (agent created but OpenClaw unreachable = warning in response, not hard failure)
- **Degraded Definition** (Fix 2): When OpenClaw unavailable:
  - `create_workspace`: workspace created with `agent_id = NULL`, response includes `"warning": "OpenClaw unavailable, agent pending"`
  - All other agent operations: return error with `agent_id: NULL`
  - No local queuing or sync — agent operations fail until OpenClaw recovers
- Methods: `create_agent()`, `delete_agent()`, `get_agent()`, `update_agent()`

**Step 3**: Create migration `migrations/YYYYMMDDHHMMSS_create_workspaces_table.sql`

**Step 4**: Alter devices table: `ALTER TABLE devices ADD COLUMN workspace_id TEXT`

**Step 5**: Create REST API `api/src/api/workspaces/`

**Step 6**: Create MCP tools in `api/src/api/mcp/tools/workspace.rs`

**Step 7**: Create prompt `skills/tinyiothub/prompts/workspace-management.md`

**Step 8**: Commit

---

### Task 12: Automation → ControlDevice & Notification Wiring (P0)

**Status**: PARTIAL — stub returns mock result, needs real execution

**Files**:
- Modify: `api/src/domain/device/service.rs` — add `send_command(device_id, command_name, command_type, params)` method using SQLx parameterized query
- Modify: `api/src/domain/automation/executor.rs` — wire `ControlDevice`, `SetProperty`, `PowerOn`, `PowerOff`
- Modify: `api/src/domain/automation/executor.rs` — wire `Notify`, `SendEmail`
- Add: MCP tools in `api/src/api/mcp/tools/automation_mcp.rs`

**What to wire** (Finding 1 update — outside voice: must use SQLx parameterized query, NOT raw SQL):
- `ControlDevice` → `DeviceService::send_command(device_id, command, "custom", params_json)`
- `SetProperty` → `DeviceService::send_command(device_id, "set_property", "property_set", params_json)` (Phase 2 workaround; proper `write_property()` deferred to Task 17)
- `PowerOn` / `PowerOff` → `DeviceService::send_command(device_id, "power_on"/"power_off", "custom", None)`
- `Notify` → call notification service

**DeviceService::send_command signature:**
```rust
pub async fn send_command(
    &self,
    device_id: &str,
    command_name: &str,
    command_type: &str,
    params: Option<String>,  // JSON string
) -> Result<String, Error>  // returns command_id
```

Uses SQLx parameterized query — NO raw `format!` SQL. Violating this = PR rejection.

**MCP Tools** (2):
| Tool | Description |
|------|-------------|
| `list_automations` | List automation rules (workspace-scoped) |
| `create_automation` | Create automation rule with conditions + actions |

**Step 1**: Confirm `device_service.send_command()` signature before wiring — grep `api/src/domain/device/` for the actual function signature
**Step 2**: Implement real `execute_control_device` in `executor.rs` (call `send_command` with confirmed signature)
**Step 3**: Implement real `execute_notify` / `execute_send_email`
**Step 4**: Create MCP tools for automation CRUD
**Step 5**: Commit

---

### Task 13: Jobs → device_command Wiring (P0)

**Status**: PARTIAL — `execute_device_command_job` returns mock result

**Files**:
- Modify: `api/src/api/jobs/mod.rs` — wire `execute_device_command_job` to real device service
- Add: MCP tools in `api/src/api/mcp/tools/job_mcp.rs`

**What to wire** (GAP 1 Fix + Fix 1: use DeviceService::send_command with SQLx):
```rust
async fn execute_device_command_job(job: &Job) -> Result<String, String> {
    // Parse job.config for device_id, command, parameters
    let config: JobConfig = serde_json::from_str(&job.config)
        .map_err(|e| format!("job.config parse error: {}", e))?;  // ← GAP 1
    // Call device_service.send_command(device_id, command_name, "custom", params_json)
    // Return result (command_id)
}
```

**MCP Tools** (3):
| Tool | Description |
|------|-------------|
| `list_schedules` | List scheduled jobs (workspace-scoped) |
| `create_schedule` | Create one-time or cron job |
| `delete_schedule` | Delete a scheduled job |

**Step 1**: Wire `execute_device_command_job` in `jobs/mod.rs`
**Step 2**: Create MCP tools for job management
**Step 3**: Commit

---

### Task 14: Batch Command (P0)

**Status**: MISSING

**Files**:
- Create: `api/src/infrastructure/batch_command.rs`
- Create: `api/src/api/batch/mod.rs`
- Create: `api/src/api/batch/handlers.rs`
- Create: `migrations/YYYYMMDDHHMMSS_create_batch_command_tables.sql`
- Create: MCP tools in `api/src/api/mcp/tools/batch.rs`

**Migration**:
```sql
CREATE TABLE batch_commands (
    id TEXT PRIMARY KEY,
    idempotency_key TEXT UNIQUE,                    -- GAP 3 fix: prevent duplicate batch from double-submit
    workspace_id TEXT NOT NULL,
    name TEXT NOT NULL,
    status TEXT NOT NULL,
    total_count INTEGER NOT NULL,
    success_count INTEGER DEFAULT 0,
    failed_count INTEGER DEFAULT 0,
    created_by TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE
);

CREATE TABLE batch_command_items (
    id TEXT PRIMARY KEY,
    batch_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    command_name TEXT NOT NULL,
    parameters TEXT,
    status TEXT NOT NULL,
    result TEXT,
    executed_at TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY (batch_id) REFERENCES batch_commands(id) ON DELETE CASCADE,
    FOREIGN KEY (device_id) REFERENCES devices(id) ON DELETE CASCADE
);
```

**MCP Tool** (1):
| Tool | Description |
|------|-------------|
| `batch_command` | Send command to multiple devices in workspace, return per-device results |

**Input Schema**:
```json
{
  "idempotency_key": "uuid-v4",      -- GAP 3 fix: client-provided key, if same key resubmitted return existing batch_id
  "command": "power_off",
  "device_ids": ["dev-001", "dev-002"],
  "parameters": {}
}
```

**Idempotency** (GAP 3 fix): If `idempotency_key` matches an existing batch_command with same `workspace_id`, return the existing `batch_id` with current status (do NOT re-execute). This prevents double-submit from UI retry or MCP timeout retry.

**Output Schema**:
```json
{
  "batch_id": "batch-xxx",
  "total": 3,
  "success": 2,
  "failed": 1,
  "results": [
    { "device_id": "dev-001", "success": true },
    { "device_id": "dev-002", "success": false, "error": "device offline" }
  ]
}
```

**Step 1**: Create `batch_command.rs` infrastructure
**Step 2**: Create REST API
**Step 3**: Create MCP tool
**Step 4**: Commit

---

### Task 15: OTA Firmware Upgrade (P1)

**Status**: STUB — needs full scoping. Plan has no detail on OTA protocol, device firmware schema, or upgrade workflow. Do NOT treat as ready to implement. Full spec required before implementation.

**Files**:
- Create: `api/src/domain/ota/mod.rs`
- Create: `api/src/domain/ota/package.rs`
- Create: `api/src/domain/ota/task.rs`
- Create: `api/src/domain/ota/service.rs`
- Create: `api/src/api/ota/mod.rs`
- Create: `api/src/api/ota/handlers.rs`
- Create: `migrations/YYYYMMDDHHMMSS_create_ota_tables.sql`
- Create: MCP tools in `api/src/api/mcp/tools/ota.rs`

**Migration**:
```sql
CREATE TABLE ota_packages (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    name TEXT NOT NULL,
    version TEXT NOT NULL,
    device_type TEXT NOT NULL,
    file_path TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    checksum TEXT NOT NULL,
    description TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE
);

CREATE TABLE ota_tasks (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    package_id TEXT NOT NULL,
    target_device_ids TEXT NOT NULL,
    status TEXT NOT NULL,
    progress INTEGER DEFAULT 0,
    started_at TEXT,
    completed_at TEXT,
    created_by TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE,
    FOREIGN KEY (package_id) REFERENCES ota_packages(id) ON DELETE CASCADE
);
```

**REST APIs** (4):
| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/ota/packages` | List firmware packages (workspace-scoped) |
| POST | `/api/v1/ota/packages` | Upload firmware package |
| POST | `/api/v1/ota/tasks` | Create OTA task |
| GET | `/api/v1/ota/tasks/{id}` | Get OTA task status |

**MCP Tools** (3):
| Tool | Description |
|------|-------------|
| `ota_check` | Check for available firmware updates |
| `ota_upgrade` | Trigger firmware upgrade for device(s) |
| `list_ota_tasks` | List OTA upgrade tasks |

**Step 1**: Create domain module
**Step 2**: Create migration
**Step 3**: Create REST API
**Step 4**: Create MCP tools
**Step 5**: Create prompt `skills/tinyiothub/prompts/ota-management.md`
**Step 6**: Commit

---

### Task 16: Log Query API (P1)

**Status**: STUB — needs full scoping. `audit_log` and `device_traces` tables exist but query API is minimal. Need to define filter schema, pagination, and log level handling before implementation.

**Files**:
- Create: `api/src/api/logs/mod.rs`
- Create: `api/src/api/logs/handlers.rs`
- Create: MCP tools in `api/src/api/mcp/tools/log.rs`

**REST API** (1):
| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/logs` | Query logs with filters (workspace-scoped) |

**MCP Tools** (2):
| Tool | Description |
|------|-------------|
| `fetch_logs` | Query system/audit logs with filters |
| `get_device_traces` | Get device trace history (workspace-scoped) |

**Step 1**: Create log query API using existing tables (filter by workspace via device_ids)
**Step 2**: Create MCP tools
**Step 3**: Create prompt `skills/tinyiothub/prompts/log-analysis.md`
**Step 4**: Commit

---

### Task 17: Timeseries & InfluxDB Integration (P1)

**Status**: STUB — needs full scoping. InfluxDB plugin exists (`plugin/storage/handlers/influxdb.rs`) but is not wired. Need to define: which device properties get written to InfluxDB, measurement naming schema, retention policy, and query API response format.

**Files**:
- Create: `api/src/domain/timeseries/mod.rs`
- Create: `api/src/domain/timeseries/query.rs`
- Create: `api/src/api/timeseries/mod.rs`
- Create: `api/src/api/timeseries/handlers.rs`
- Modify: `api/src/domain/plugin/storage/handlers/influxdb.rs` — wire into property write pipeline
- Create: MCP tools in `api/src/api/mcp/tools/timeseries.rs`

**REST APIs** (2):
| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/timeseries/query` | Query timeseries data (workspace-scoped) |
| GET | `/api/v1/timeseries/aggregate` | Aggregate timeseries (workspace-scoped) |

**MCP Tools** (3):
| Tool | Description |
|------|-------------|
| `query_timeseries` | Query historical device property data |
| `aggregate_timeseries` | Aggregate data (sum, avg, min, max) over time range |
| `forecast_timeseries` | Simple forecast based on recent trend |

**Step 1**: Wire InfluxDB write into device property write path (write_property calls InfluxDB)
**Step 2**: Create timeseries query API
**Step 3**: Create MCP tools
**Step 4**: Commit

---

### Task 18: Alarm MCP Tools (P0)

**Status**: REST API exists, needs MCP wrapper

**Files**:
- Create: `api/src/api/mcp/tools/alarm_mcp.rs`
- Modify: `skills/tinyiothub/prompts/alarm-management.md` (enhance existing)

**MCP Tools** (4):
| Tool | Description |
|------|-------------|
| `alarm_list` | List alarms (workspace-scoped via device_ids) |
| `alarm_statistics` | Get alarm statistics for time range |
| `alarm_acknowledge` | Acknowledge an alarm |
| `alarm_rule_add` | Create alarm rule |

**Step 1**: Create `alarm_mcp.rs` wrapping existing REST endpoints (add workspace filter)
**Step 2**: Enhance alarm-management prompt
**Step 3**: Commit

---

### Task 19: Device Enhanced — Compare & Diagnosis (P0)

**Status**: MISSING

**Files**:
- Create: `api/src/infrastructure/diagnostics.rs`
- Create: `api/src/api/mcp/tools/device_enhanced.rs`

**MCP Tools** (3):
| Tool | Description |
|------|-------------|
| `compare_devices` | Compare current property values across multiple devices in workspace |
| `diagnose_device` | Analyze device for common fault patterns (offline count, reconnect history) |
| `scan_serial` | Scan serial port for connected devices (workspace-scoped) |

**Input Schema** (`compare_devices`):
```json
{
  "device_ids": ["dev-001", "dev-002"],
  "property": "temperature"
}
```

**Output Schema**:
```json
{
  "property": "temperature",
  "values": [
    { "device_id": "dev-001", "name": "1号风机", "value": "25.6", "unit": "C" },
    { "device_id": "dev-002", "name": "2号风机", "value": "26.1", "unit": "C" }
  ],
  "comparison": { "max_diff": 0.5, "average": 25.85 }
}
```

**Note**: `diagnose_device` returns basic analysis from `device_traces`. Extended signal strength (RSSI) tracking is Phase 3.

**Step 1**: Create `diagnostics.rs` for fault analysis
**Step 2**: Create `device_enhanced.rs` for compare and scan
**Step 3**: Create prompt `skills/tinyiothub/prompts/diagnostics.md`
**Step 4**: Commit

---

### Task 20: OpenClaw Skills Enhancement (P0)

**Status**: Partial — 4 skills exist (Phase 1), need 4 new + 4 enhanced

**Files**:
- Modify: `skills/tinyiothub/skill.yaml` (add 4 new skills)
- Create: `skills/tinyiothub/prompts/workspace-management.md` (NEW)
- Create: `skills/tinyiothub/prompts/scene-control.md` (DELETED — not in scope)
- Create: `skills/tinyiothub/prompts/automation-rules.md` (NEW)
- Create: `skills/tinyiothub/prompts/schedule-tasks.md` (NEW)
- Create: `skills/tinyiothub/prompts/diagnostics.md` (NEW)
- Create: `skills/tinyiothub/prompts/ota-management.md` (NEW)
- Create: `skills/tinyiothub/prompts/log-analysis.md` (NEW)
- Modify: Enhance existing 4 prompts (device-onboarding, heartbeat-query, device-status, alarm-management) with workspace context

**New Skills** (4):
| Skill | Tools | Purpose |
|-------|-------|---------|
| `workspace-management` | `list_workspaces`, `create_workspace`, `update_workspace`, `delete_workspace`, `assign_device` | Workspace CRUD + OpenClaw Agent lifecycle |
| `automation-rules` | `list_automations`, `create_automation` | Rule-based automation |
| `schedule-tasks` | `list_schedules`, `create_schedule`, `delete_schedule` | Scheduled jobs |
| `diagnostics` | `diagnose_device`, `scan_serial` | Device troubleshooting |

**Enhanced Existing Skills**:
- `device-onboarding.md` — Add workspace context, `assign_device`
- `device-status.md` — Add `compare_devices`
- `alarm-management.md` — Add `alarm_statistics`, `alarm_acknowledge`
- `heartbeat-query.md` — Already covers health queries

**Skills NOT in Phase 2 scope** (removed):
- ~~scene-control~~ — Scene联动 removed from scope
- `ota-management` — P1, included
- `log-analysis` — P1, included

**Step 1**: Update `skill.yaml` with new skills
**Step 2**: Write 7 new/enhanced prompt files
**Step 3**: Commit

---

### Task 21: MCP Tool Tests Phase 2 (P0)

**Files**:
- Create: `api/src/api/mcp/tests/phase2_tools_tests.rs`

**Tests**:
```rust
// Workspace tools
#[tokio::test]
async fn test_create_workspace_creates_openclaw_agent() { ... }
#[tokio::test]
async fn test_delete_workspace_removes_openclaw_agent() { ... }
#[tokio::test]
async fn test_assign_device_to_workspace() { ... }

// Batch command
#[tokio::test]
async fn test_batch_command_partial_failure() { ... }
#[tokio::test]
async fn test_batch_command_all_devices() { ... }

// Timeseries
#[tokio::test]
async fn test_aggregate_timeseries_avg() { ... }

// Diagnostics
#[tokio::test]
async fn test_diagnose_device_offline_analysis() { ... }
#[tokio::test]
async fn test_compare_devices_multiple() { ... }
```

**Step 1**: Write tests for all Phase 2 tool categories
**Step 2**: Run `cd api && cargo test --lib mcp::tests`
**Step 3**: Commit

---

### Task 22: E2E Verification Phase 2 (P0)

**Files**:
- Create: `docs/superpowers/plans/2026-04-04-e2e-verification-phase2.md`

**Verification Scripts**:
```bash
# 1. Create workspace
curl -X POST http://localhost:3002/api/v1/workspaces \
  -H "Authorization: Bearer <jwt>" \
  -d '{"name":"工业园区A","description":"3号厂房"}' | jq '.result'

# 2. Assign device to workspace
curl -X POST http://localhost:3002/api/v1/workspaces/{id}/devices \
  -H "Authorization: Bearer <jwt>" \
  -d '{"device_id":"dev-001"}' | jq '.result'

# 3. Batch command via MCP
curl -X POST http://localhost:3002/mcp/tools/call \
  -H "Authorization: Bearer <jwt>" \
  -d '{"name":"batch_command","arguments":{"command":"power_off","device_ids":["dev-001","dev-002"]}}' \
  | jq '.result'

# 4. Device compare
curl -X POST http://localhost:3002/mcp/tools/call \
  -H "Authorization: Bearer <jwt>" \
  -d '{"name":"compare_devices","arguments":{"device_ids":["dev-001","dev-002"],"property":"temperature"}}' \
  | jq '.result'

# 5. Alarm statistics via MCP
curl -X POST http://localhost:3002/mcp/tools/call \
  -H "Authorization: Bearer <jwt>" \
  -d '{"name":"alarm_statistics","arguments":{"time_range":"24h"}}' \
  | jq '.result'
```

**Step 1**: Write verification script
**Step 2**: Execute and document results
**Step 3**: Commit

---

## 5. File Structure (Phase 2)

```
api/src/
├── domain/
│   ├── workspace/                 [NEW]
│   │   ├── mod.rs
│   │   ├── workspace.rs
│   │   └── service.rs
│   ├── ota/                      [NEW]
│   │   ├── mod.rs
│   │   ├── package.rs
│   │   ├── task.rs
│   │   └── service.rs
│   ├── timeseries/               [NEW]
│   │   ├── mod.rs
│   │   └── query.rs
│   ├── automation/               [ENHANCED]
│   │   └── executor.rs            (wire ControlDevice, Notify)
│   └── plugin/storage/handlers/
│       └── influxdb.rs            [ENHANCED] (wire into property write)
├── infrastructure/
│   ├── batch_command.rs           [NEW]
│   ├── diagnostics.rs             [NEW]
│   └── openclaw_agent.rs         [NEW] OpenClaw Agent API client
├── api/
│   ├── workspaces/                [NEW]
│   │   ├── mod.rs
│   │   └── handlers.rs
│   ├── batch/                    [NEW]
│   │   ├── mod.rs
│   │   └── handlers.rs
│   ├── ota/                      [NEW]
│   │   ├── mod.rs
│   │   └── handlers.rs
│   ├── logs/                     [NEW]
│   │   ├── mod.rs
│   │   └── handlers.rs
│   ├── timeseries/               [NEW]
│   │   ├── mod.rs
│   │   └── handlers.rs
│   ├── mcp/tools/
│   │   ├── workspace.rs          [NEW]
│   │   ├── alarm_mcp.rs          [NEW]
│   │   ├── job_mcp.rs            [NEW]
│   │   ├── batch.rs              [NEW]
│   │   ├── ota.rs                [NEW]
│   │   ├── log.rs               [NEW]
│   │   ├── timeseries.rs         [NEW]
│   │   ├── device_enhanced.rs    [NEW]
│   │   └── automation_mcp.rs      [NEW]
│   └── jobs/
│       └── mod.rs               [MODIFIED] (wire device_command)
├── dto/
│   └── entity/
│       ├── workspace.rs           [NEW]
│       ├── batch_command.rs        [NEW]
│       ├── ota_package.rs        [NEW]
│       └── ota_task.rs           [NEW]
migrations/
├── YYYYMMDDHHMMSS_create_workspaces_table.sql
├── YYYYMMDDHHMMSS_create_batch_command_tables.sql
└── YYYYMMDDHHMMSS_create_ota_tables.sql
skills/tinyiothub/
├── skill.yaml                     [MODIFIED] (add 4 new skills)
└── prompts/
    ├── workspace-management.md      [NEW]
    ├── automation-rules.md          [NEW]
    ├── schedule-tasks.md           [NEW]
    ├── diagnostics.md              [NEW]
    ├── ota-management.md           [NEW]
    ├── log-analysis.md             [NEW]
    ├── device-onboarding.md       [MODIFIED] (add assign_device)
    ├── device-status.md           [MODIFIED] (add compare_devices)
    └── alarm-management.md        [MODIFIED] (add statistics, ack)
api/src/api/mcp/tests/
├── phase2_tools_tests.rs            [NEW]
└── integration_tests.rs           [MODIFIED]
```

---

## 6. Task Dependency Graph

```
Task 11 (Workspace) ──┬──→ Task 21 (Skills: workspace-management)
                     │
Task 12 (Automation) ──→ Task 21 (Skills: automation-rules)
Task 13 (Jobs) ──────────→ Task 21 (Skills: schedule-tasks)
Task 14 (Batch) ─────────→ Task 22 (Tests)
Task 15 (OTA) ────────────→ Task 21 (Skills: ota)
Task 16 (Logs) ───────────→ Task 21 (Skills: log-analysis)
Task 17 (Timeseries) ────→ Task 22 (Tests)
Task 18 (Alarm MCP) ───────→ Task 21 (Skills: alarm-management)
Task 19 (Device Enhanced) ───→ Task 21 (Skills: diagnostics)

Task 21 (All Skills) ──┐
Task 22 (All Tests) ──┼── Task 23 (E2E Verification)
                       │
                       └── Phase 2 Complete ✓
```

---

## 7. Phase 2 Delivery Checklist

| Deliverable | Task | Priority |
|-------------|------|----------|
| Workspace CRUD + OpenClaw Agent 联动 | Task 11 | P0 |
| Automation ControlDevice wiring | Task 12 | P0 |
| Jobs device_command wiring | Task 13 | P0 |
| Batch command | Task 14 | P0 |
| Alarm MCP tools | Task 18 | P0 |
| Device compare + diagnose | Task 19 | P0 |
| OpenClaw Skills (4 new + 4 enhanced) | Task 21 | P0 |
| MCP tool tests Phase 2 | Task 22 | P0 |
| E2E verification | Task 23 | P0 |
| OTA firmware upgrade | Task 15 | P1 |
| Log query API + MCP | Task 16 | P1 |
| Timeseries + InfluxDB wiring | Task 17 | P1 |

---

## 8. NOT in Scope

- Scene 联动系统 (场景联动功能已移除)
- Device 属于多个 Workspace (一个设备只能属于一个 Workspace)
- RSSI/信号强度追踪 (`diagnose_device` 返回基本分析，详细信号分析 Phase 3)
- 云端 LLM 驱动生成 (Phase 3)
- 知识库云端同步 (Phase 4)

---

## 9. Future Phases

### Phase 3: Cloud LLM Driver Generation + Advanced Diagnostics
- `generate_driver` full implementation via cloud LLM
- RSSI signal strength tracking in `device_traces`
- Advanced `diagnose_device` with signal analysis

### Phase 4: Knowledge Cloud Sync
- `contribute_knowledge` → cloud knowledge base
- `sync_knowledge` bi-directional sync
- `get_device_manual` full implementation

### Phase 5: Advanced Analytics
- `forecast_timeseries` real ML-based prediction
- Anomaly detection
- Cross-device correlation analysis

---

## 10. Testing Strategy

### Unit Tests
- Each new MCP tool tested individually
- OpenClaw Agent lifecycle (create/delete) mocked
- Batch command failure handling

### Integration Tests
- Workspace → OpenClaw Agent creation flow
- Automation → device command wiring
- Jobs → device command execution

### E2E Tests
- OpenClaw skill → MCP → API → Backend → DB full chain
- Natural language → workspace creation scenario
- Natural language → batch command scenario

---

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 1 | issues_open | 3 critical gaps fixed (GAP1 job ParseError, GAP2 assign_device conflict, GAP3 batch idempotency) |
| Codex Review | `/codex review` | Independent 2nd opinion | 0 | — | — |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | issues_open | 3 findings fixed (send_command doesn't exist → SQLx method, raw SQL → SQLx, P1 tasks stubbed) |
| Design Review | `/plan-design-review` | UI/UX gaps | 1 | issues_open | 1 new page added (AI Chat UI), 3 design decisions made (居中全宽对话, SSE, Web Components) |

**VERDICT:** CEO + ENG + DESIGN REVIEW DONE — Phase 2 plan now includes full AI Chat UI spec. Ready to implement pending Phase 1 completion.

### CEO Review Key Findings

**Critical Gaps Fixed:**
1. GAP1: `execute_device_command_job` ParseError → Task 13 adds error handling
2. GAP2: `assign_device` conflict → Task 11 adds WorkspaceConflict校验
3. GAP3: `batch_command` idempotency → Task 14 adds idempotency_key

**Issue Resolutions:**
- 1A: OpenClaw API discovery (probe on startup)
- 2B: SetProperty workaround (send_command)
- 3A: OpenClawAgentClient trait + mock

### Eng Review Key Findings

**Findings Fixed (outside voice accepted):**
1. **Fix 1:** `device_service.send_command()` does NOT exist → added as new DeviceService method with SQLx parameterized query (NOT raw SQL)
2. **Fix 2:** "Graceful degradation" undefined → clarified: `agent_id=NULL` + `"warning": "OpenClaw unavailable"` in response JSON
3. **Fix 3:** assign_device race condition → `SELECT ... FOR UPDATE` or unique constraint required
4. **Fix 4:** Tasks 15/16/17 (P1) were empty stubs → marked explicitly as STUB requiring full scoping before implementation

**NOT in Scope (confirmed):**
- Scene 联动系统
- Device 多 Workspace 支持
- RSSI 信号强度追踪
- 云端 LLM 驱动生成

**Deferred to Phase 3:**
- `write_property()` proper implementation
- RSSI signal strength in `device_traces`
- Cloud LLM driver generation

**Observability Suggestions (non-blocking):**
- S1: `create_workspace` 成功时应 structured log `{workspace_id, agent_id}`
- S2: `batch_command` 每设备执行结果应 log

### Design Review Key Findings

**Initial score: 2/10** — Phase 2 plan described complete backend architecture but zero frontend UI spec. No React page specifications, no interaction states, no visual hierarchy defined.

**Issues Found & Fixed:**
1. **Issue 1:** AI Chat UI page missing from plan → Added Section 1.5 with full page spec (layout, components, message types, API integration)
2. **Issue 2:** No interaction state coverage → Added Interaction State Table (1.5.11)
3. **Issue 3:** No user emotional journey defined → Added User Journey Storyboard (1.5.12)
4. **Issue 4:** No responsive/accessibility spec → Added Responsive & Accessibility spec (1.5.13)

**Design Decisions Made (confirmed by user):**
1. **Layout:** 居中全宽对话 (centered full-width conversation) — device cards inline in chat flow
2. **Streaming:** SSE (Server-Sent Events) for AI response streaming
3. **A2UI Rendering:** Web Components (Lit) — requires confirmation of OpenClaw A2UI component availability before Phase 2 implementation

**What Already Exists (reuse these):**
- CSS variable system: `web/themes/light.css` / `dark.css`
- Primary accent: `#155aef`
- Glass morphism patterns (already in codebase)
- Sidebar navigation: `web/app/components/app-sidebar/index.tsx`
- API service layer: `web/service/`
- React Query hooks: `web/hooks/`
- No existing AI chat UI (confirmed) — new page must be built from scratch
