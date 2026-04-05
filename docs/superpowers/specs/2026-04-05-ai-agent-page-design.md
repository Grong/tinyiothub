# AI Agent 交互页面设计

> web-lit 前端新增 AI 对话页面，参考 OpenClaw UI 复用核心渲染模块，IoT 专用增强。

## 需求总结

- **范围**：IoT 专用增强版对话界面（文本对话 + 流式响应 + Markdown + 4 种 IoT 卡片）
- **通信**：SSE (Server-Sent Events) 流式响应
- **会话**：单会话模式（刷新保留历史）
- **复用**：从 OpenClaw UI 复制 Markdown 渲染、消息分组渲染、自动滚动管理

## 架构设计

### 页面层级

```
web-lit/src/
├── pages/agent-page.ts          # 新页面，路由 /agent
├── components/agent/
│   ├── chat-thread.ts           # 消息列表（滚动容器）
│   ├── chat-input.ts            # 输入区域（textarea + 发送按钮）
│   ├── message-group.ts         # 消息分组渲染（基于 OpenClaw grouped-render）
│   ├── streaming-message.ts     # 流式消息实时渲染
│   ├── iot-card-base.ts         # IoT 卡片基类
│   ├── device-status-card.ts    # 设备状态卡片
│   ├── alarm-info-card.ts       # 告警信息卡片
│   ├── command-result-card.ts   # 命令执行结果卡片
│   └── data-trend-chart.ts      # 数据趋势图表
├── services/agent.ts            # SSE API 调用层
├── stores/agent-store.ts        # 对话状态（nanostore）
├── lib/
│   ├── markdown.ts              # 复制自 OpenClaw markdown.ts
│   ├── chat-scroll.ts           # 复制自 OpenClaw app-scroll.ts
│   └── message-normalizer.ts    # 消息标准化
└── types/agent-types.ts         # 对话相关类型定义
```

### 路由注册

在 `app.ts` 的路由 switch 中新增：

```ts
case 'agent':
  page = html`<agent-page></agent-page>`;
  break;
```

侧边栏新增「AI 助手」导航项（在「运维管理」和「应用中心」之间）。

### 通信层：SSE 流式

```
POST /api/v1/agent/chat
Body: { message: string, session_id: string }
Response: text/event-stream

event: message
data: {"type": "delta", "content": "部分文本"}

event: message
data: {"type": "iot_card", "card_type": "device_status", "data": {...}}

event: message
data: {"type": "final", "content": "完整回复"}

event: done
data: {}
```

**Service 层** (`services/agent.ts`)：

```ts
export async function sendAgentMessage(
  message: string,
  sessionId: string,
  onDelta: (content: string) => void,
  onCard: (card: IotCardData) => void,
  onFinal: (content: string) => void,
  signal?: AbortSignal
): Promise<void>
```

使用 `fetch` + `ReadableStream` 读取 SSE，逐行解析 `data:` 帧。复用 `ApiClient` 的 token/headers 获取逻辑。

### 状态管理：agent-store.ts

```ts
// nanostore atoms
export const $chatMessages = atom<ChatMessage[]>([])
export const $streamingContent = atom<string>('')
export const $isStreaming = atom<boolean>(false)
export const $sessionId = atom<string>(generateSessionId())

// actions
export function addMessage(msg: ChatMessage): void
export function appendStreamDelta(delta: string): void
export function finalizeStream(): void
export function clearChat(): void
```

`ChatMessage` 类型：

```ts
interface ChatMessage {
  id: string
  role: 'user' | 'assistant'
  content: string
  timestamp: number
  cards?: IotCardData[]       // IoT 卡片（仅 assistant）
  isStreaming?: boolean
}
```

### IoT 卡片数据类型

```ts
type IotCardData =
  | { type: 'device_status'; deviceId: string; name: string; status: string; properties: Record<string, unknown> }
  | { type: 'alarm_info'; alarms: Array<{ level: string; device: string; message: string; time: string }> }
  | { type: 'command_result'; command: string; device: string; success: boolean; result: string }
  | { type: 'data_trend'; title: string; series: Array<{ name: string; data: Array<{ time: string; value: number }> }> }
```

## 组件设计

### 1. `<agent-page>` — 页面入口

- Shadow DOM，占满内容区（`height: 100%`，flex column）
- 包含 `<chat-thread>` + `<chat-input>`
- 顶部可选的标题栏："AI 助手"
- `firstUpdated()` 时加载历史消息
- 订阅 `$chatMessages` 和 `$streamingContent`

### 2. `<chat-thread>` — 消息列表

- Shadow DOM，`overflow-y: auto`，flex: 1
- 使用 `repeat()` 指令渲染消息列表
- 复用 OpenClaw 的自动滚动逻辑（粘底 450px 阈值）
- 消息渲染委托给 `<message-group>`
- 流式消息用 `<streaming-message>` 占位

### 3. `<message-group>` — 消息分组渲染

- 基于 OpenClaw `grouped-render.ts` 的模式适配
- 用户消息：右侧气泡，accent 色背景
- 助手消息：左侧，包含 Markdown 渲染 + IoT 卡片
- Markdown 通过 `marked` + `DOMPurify` 渲染为 HTML，用 `innerHTML` 注入
- IoT 卡片在 Markdown 之后渲染

### 4. `<streaming-message>` — 流式消息

- 显示打字光标动画
- 实时 Markdown 渲染（每 100ms 节流）
- 底部显示 "AI 正在输入..." 指示器

### 5. `<chat-input>` — 输入区域

- `<textarea>` 自动高度（参考 OpenClaw 的 auto-resize）
- Enter 发送，Shift+Enter 换行
- 发送按钮（流式中变为停止按钮）
- placeholder："询问设备状态、告警、数据..."

### 6. IoT 卡片组件

所有卡片使用统一的 `<iot-card-base>` 包裹，提供标题栏、图标、展开/折叠。

**`<device-status-card>`**：
- 设备名称 + 状态指示灯（在线/离线/故障）
- 属性列表：key-value 表格
- 底部时间戳

**`<alarm-info-card>`**：
- 告警级别标签（紧急/重要/一般/提示）带颜色
- 列表展示：设备名 + 告警描述 + 时间
- 超过 3 条折叠

**`<command-result-card>`**：
- 命令名 + 目标设备
- 执行结果：成功（绿色）/ 失败（红色）
- 结果文本（可展开）

**`<data-trend-chart>`**：
- 标题 + 时间范围
- 简单 SVG 折线图（不引入 chart 库，用 SVG path 手绘）
- 多系列支持（不同颜色）
- hover 显示数值 tooltip

## 复用模块

### 从 OpenClaw 复制的文件

| 源文件 | 目标位置 | 改动 |
|--------|----------|------|
| `openclaw/ui/src/ui/markdown.ts` | `web-lit/src/lib/markdown.ts` | 移除 OpenClaw 特定导入，导出 `toSanitizedMarkdownHtml()` |
| `openclaw/ui/src/ui/app-scroll.ts` | `web-lit/src/lib/chat-scroll.ts` | 适配为独立类 `ChatScroller`，移除对 app state 的依赖 |
| `openclaw/ui/src/ui/chat/grouped-render.ts` | 参考模式（不直接复制） | 重写为 Lit 组件 `<message-group>`，因 OpenClaw 用 light DOM 纯函数渲染，web-lit 用 Shadow DOM 组件 |

### 依赖安装

```json
{
  "marked": "^17.0.5",
  "dompurify": "^3.3.3",
  "@types/dompurify": "^3.0.5"
}
```

### 复用现有 web-lit 基础设施

- `ApiClient` — 获取 token/headers
- `apiPost` — 发送消息
- `$token`, `$isAuthenticated` — 认证状态
- CSS 设计令牌 — `--bg`, `--text`, `--accent`, `--border` 等
- `navigate()` — 路由跳转

## CSS 设计

### 对话区布局

```css
.agent-page {
  display: flex;
  flex-direction: column;
  height: 100%;
  padding: 0;
}

.chat-thread {
  flex: 1;
  overflow-y: auto;
  padding: 16px 24px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.chat-input-area {
  flex-shrink: 0;
  padding: 12px 24px 16px;
  border-top: 1px solid var(--border);
  background: var(--bg);
}
```

### 消息气泡

- 用户消息：`background: var(--accent)`，白色文字，右对齐，圆角 12px
- 助手消息：`background: var(--bg-elevated)`，左对齐，圆角 12px
- 最大宽度 72%，超出换行

### IoT 卡片

- `background: var(--bg-card)`，`border: 1px solid var(--border)`，`border-radius: var(--radius)`
- 内部间距 12px
- 标题栏：图标 + 标题 + 折叠按钮
- 状态指示灯使用 `--ok`/`--warn`/`--danger` 颜色

## 数据流

```
用户输入
  ↓
chat-input dispatches 'message-send' event
  ↓
agent-page 收到 → addMessage(user) → sendAgentMessage()
  ↓
SSE fetch → ReadableStream
  ↓
onDelta → appendStreamDelta() → streaming-message 实时更新
onCard → addCardToLastMessage() → IoT 卡片渲染
onFinal → finalizeStream() → message-group 静态渲染
```

## 错误处理

- SSE 连接失败：显示错误气泡 "连接失败，请重试"+ 重试按钮
- 流式中断：保留已接收内容，显示 "响应中断"
- 认证过期：自动跳转 signin（复用 auth-store 的 auth-error 事件）
- Abort：用户点击停止按钮 → `AbortController.abort()` → 保留已接收内容

## 无障碍

- 消息区域 `role="log"` + `aria-live="polite"`（仅新消息时）
- 输入框 `aria-label="输入消息"`
- IoT 卡片 `role="article"` + `aria-label` 描述内容
- 键盘：Enter 发送，Escape 停止

## 实现优先级

1. **Phase 1**：路由 + 页面骨架 + 基础文本对话 + SSE 流式 + Markdown 渲染
2. **Phase 2**：消息分组（用户/助手区分）+ 自动滚动 + 输入历史
3. **Phase 3**：4 种 IoT 卡片（设备状态、告警、命令结果、数据趋势）
4. **Phase 4**：Polish — 动画、空状态、错误处理、无障碍
