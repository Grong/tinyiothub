# Agent 管理 & Chat 页面重设计

> 日期: 2026-04-08
> 状态: 设计完成，待实施

## 1. 背景

当前 Agent 管理页面有 6 个 Tab，其中 4 个（文件/技能/渠道/定时任务）是占位符。模型选择硬编码 4 个选项，工具面板功能基本可用但 UI 粗糙。

Chat 页面能收发消息，但缺乏 IoT 场景的深度集成——A2UI 的 4 个 IoT 组件（DeviceCard/DeviceTable/DataChart/ControlPanel）都是 stub，`onAction` 回调从未连接，消息中无法触发设备操作。

目标：将两个页面提升到可交付产品质量。

## 2. Agent 管理页面设计

### 2.1 布局：Tab 式管理面板

保持现有 6-Tab 结构，聚焦前 2 个 Tab 做到 production quality，其余 4 个放优雅占位。

```
┌─────────────────────────────────────────────────────┐
│  < Agent 选择栏 >                                    │
│  [工作空间A的助手 ▾]                                 │
├───────┬───────┬───────┬───────┬───────┬─────────────┤
│  模型 │  工具 │  技能 │  渠道 │  定时 │  文件        │
├───────┴───────┴───────┴───────┴───────┴─────────────┤
│                                                      │
│  [当前 Tab 内容]                                      │
│                                                      │
└─────────────────────────────────────────────────────┘
```

### 2.2 模型配置 Tab

**功能区域：**

| 区域 | 控件 | 说明 |
|------|------|------|
| 基础模型 | 下拉选择器 | 从 ZeroClaw 获取可用模型列表，非硬编码 |
| Temperature | 滑块 (0.0-2.0) | 带数值显示 |
| Max Tokens | 数字输入 | 带范围校验 |
| System Prompt | 多行文本框 | 支持语法高亮或至少 monospace 字体 |
| Top P | 滑块 (0.0-1.0) | 可选，默认折叠 |

**交互流程：**
1. 进入 Tab → GET `/agents/:id/config` 加载当前配置
2. 用户修改任意字段 → 显示 "未保存" 状态指示
3. 点击保存 → PUT `/agents/:id/config`，带 `baseHash` 做乐观锁
4. 保存成功 → 显示成功 toast，重置 dirty 状态
5. 保存冲突（baseHash 不匹配）→ 提示"配置已被修改，请刷新后重试"

**模型列表来源：**
- 新增 GET `/agents/models` 端点（或复用 ZeroClaw 的模型列表接口）
- 回退：如果接口不可用，使用本地默认列表

### 2.3 工具权限 Tab

保持当前功能，UI 改进：

| 改进 | 说明 |
|------|------|
| 分组折叠 | 每组（device/monitor/alarm/...）可展开/折叠 |
| 批量操作 | "全部启用"/"全部禁用" 按钮 |
| 搜索过滤 | 按工具名搜索 |
| 权限说明 | 每个工具显示一行描述（从 tools catalog 获取） |
| 危险标记 | 高风险工具（如 device_delete）标红色警告 |

### 2.4 占位 Tab（技能/渠道/定时/文件）

不再显示 "coming soon"，改为统一的优雅占位：

```
┌─────────────────────────────────────────────┐
│                                              │
│        [图标]                                │
│                                              │
│        技能管理                               │
│        Agent 可以扩展自定义技能来处理          │
│        特定任务。此功能即将推出。              │
│                                              │
│        [了解更多信息 →]                       │
│                                              │
└─────────────────────────────────────────────┘
```

每个占位 Tab 使用不同图标和描述文案，保持专业感。

### 2.5 Agent 选择栏改进

当前：一排 pill 按钮横向排列，空间不足时溢出。

改为：
- 下拉选择器（`<select>` 样式自定义），显示 agent 名称 + workspace
- 下拉旁显示 agent 状态指示（在线/离线/配置中）

## 3. Chat 页面设计

### 3.1 布局

保持现有两栏布局（会话列表 + 聊天主区域），改进细节：

```
┌──────────┬─────────────────────────────────────────┐
│ 新建会话  │  工作空间A的助手                    ⚙️  │
├──────────┤                                         │
│ 今日      │  用户: 查看车间A所有设备状态            │
│ ├ 会话1   │                                         │
│ ├ 会话2   │  助手: 车间A共3台设备，状态概览如下：    │
│ 昨天      │  ┌─────────────────────────────────┐    │
│ ├ 会话3   │  │  [DeviceCard] [DeviceCard]       │    │
│          │  │  [DeviceCard]                     │    │
│          │  └─────────────────────────────────┘    │
│          │  [查看详情] [批量控制] [导出报告]         │
│          │                                         │
│          │  ─── 流式输出中 ●●● ───                  │
│          ├─────────────────────────────────────────┤
│          │  [/ 上传  🎤 语音]  [输入消息...]    [▶]  │
└──────────┴─────────────────────────────────────────┘
```

### 3.2 A2UI IoT 组件完善

#### DeviceCard（完善）

```
┌─────────────────────────────┐
│  🌡️ 温控器-01          ✅   │
│  ─────────────────────────  │
│  温度        26.5°C         │
│  目标温度     25.0°C         │
│  ┌──┐                       │
│  │  │ ← 迷你 sparkline      │
│  └──┘                       │
│  [查看详细] [控制]           │
└─────────────────────────────┘
```

**dataModel 字段：**
```typescript
{
  deviceId: string;
  name: string;
  status: "online" | "offline" | "warning" | "error";
  telemetry: { key: string; value: string; unit: string }[];
  sparkline?: number[];        // 最近 N 个数据点
  lastSeen?: string;           // ISO 时间戳
  actions?: { label: string; functionId: string }[];
}
```

#### DeviceTable（完善）

```
┌──────────────────────────────────────────────────┐
│  车间A 设备状态                          [刷新]   │
├────────────┬────────┬─────────┬──────────────────┤
│ 设备名称    │ 状态   │ 最新数据 │ 操作             │
├────────────┼────────┼─────────┼──────────────────┤
│ 温控器-01  │ ✅ 在线 │ 26.5°C  │ [详情] [控制]    │
│ 压力传感器  │ ⚠️ 告警 │ 2.8MPa  │ [详情] [控制]    │
│ 流量计-03  │ ✅ 在线 │ 15.2L/m │ [详情] [控制]    │
└────────────┴────────┴─────────┴──────────────────┘
```

- 支持列排序
- 状态列彩色标记（绿/红/黄/灰）
- 操作列带按钮，触发 `onAction`

#### DataChart（新建）

使用轻量 SVG 折线图，不引入第三方图表库：

```
┌──────────────────────────────────────────┐
│  温度趋势 (最近1小时)              [1h][6h][24h] │
│  30°C ┤      ╱╲                          │
│  28°C ┤   ╱╲╱  ╲    ╱╲                   │
│  26°C ┤──╱──────╲──╱──╲──────────────    │
│  24°C ┤╱        ╲╱    ╲╱                 │
│       └──────────────────────────         │
│       14:00  14:15  14:30  14:45          │
└──────────────────────────────────────────┘
```

**dataModel 字段：**
```typescript
{
  title: string;
  timeRange: "1h" | "6h" | "24h";
  series: {
    name: string;
    color: string;
    data: { time: string; value: number }[];
  }[];
  unit: string;
  thresholds?: { label: string; value: number; color: string }[];
}
```

实现方式：纯 SVG `<polyline>` + 渐变填充，无需 canvas 或第三方库。控制在 ~150 行 TS。

#### ControlPanel（新建）

```
┌──────────────────────────────────────────┐
│  ⚙️ 温控器-01 控制面板                    │
│                                          │
│  目标温度                                │
│  ├─ [25.0°C] ────────●────── 30°C       │
│  │  20°C                             35°C│
│  └─ [应用]                               │
│                                          │
│  工作模式                                │
│  (●) 自动  ( ) 手动  ( ) 定时            │
│                                          │
│  电源                                    │
│  ┌────────┐                              │
│  │ 关闭    │  ← 危险操作，带确认          │
│  └────────┘                              │
└──────────────────────────────────────────┘
```

**dataModel 字段：**
```typescript
{
  deviceId: string;
  deviceName: string;
  controls: {
    id: string;
    label: string;
    type: "slider" | "toggle" | "choice" | "button";
    // slider
    min?: number; max?: number; step?: number; value?: number; unit?: string;
    // toggle
    on?: boolean;
    // choice
    options?: { label: string; value: string }[];
    selected?: string;
    // button
    variant?: "primary" | "danger";
    confirmMessage?: string;
  }[];
}
```

控件交互 → 调用 `onAction(functionId, payload)` → A2UI renderer → chat view → POST 到后端执行设备命令。

### 3.3 onAction 回调连接

当前问题：`A2uiRendererEngine` 接受 `onAction` 参数，但 `ChatView` 从未传入。

**设计：**
```typescript
// chat.ts — connectedCallback 中
this.a2uiEngine = new A2uiRendererEngine((functionId: string, data: unknown) => {
  this._handleA2uiAction(functionId, data);
});

// 新方法
_handleA2uiAction(functionId: string, data: unknown) {
  // 将 action 作为新的用户消息发送给 agent
  // 格式: "执行操作: {functionId} 参数: {JSON data}"
  // 这样 agent 可以处理操作并返回结果
  sendChatMessage(this.chatState, `[操作] ${functionId}: ${JSON.stringify(data)}`);
}
```

### 3.4 消息中内联 A2UI 组件

当前：A2UI 渲染在消息列表下方单独区域。

改进：将 A2UI surface 渲染到对应的 assistant 消息气泡内。

**实现方式：**
- `handleChatEvent` 收到含 `a2ui` 字段的事件时，将 surface ID 关联到当前 streaming 消息
- `grouped-render.ts` 的 `renderSingleMessage` 检查消息是否有关联的 A2UI surface
- 有 → 在 markdown 内容下方渲染 A2UI 组件
- 无 → 正常 markdown 渲染

### 3.5 会话侧栏改进

| 改进项 | 说明 |
|--------|------|
| 自动标题 | 首条用户消息截取前 20 字作为会话标题 |
| 时间分组 | 今天/昨天/更早 |
| 折叠按钮 | 侧栏可收起，释放聊天区域宽度 |
| 活跃指示 | 当前会话高亮 |

### 3.6 输入栏改进

```
┌─────────────────────────────────────────────────┐
│  [+ 附件]  [🎤 语音]                             │
│  ┌─────────────────────────────────────────┐    │
│  │ 输入消息...                               │    │
│  │                                          │    │
│  └─────────────────────────────────────────┘    │
│  Shift+Enter 换行  ·  Enter 发送          [▶]   │
└─────────────────────────────────────────────────┘
```

- 多行输入框（自动高度，最大 120px）
- Enter 发送，Shift+Enter 换行
- 发送中显示 abort 按钮替代 send 按钮

## 4. 数据流

### Agent 管理

```
ViewAgents
  └─ connectedCallback() → loadAgents()
  └─ onAgentSelected(id) → loadAgentConfig(id) + loadToolsCatalog(id)
  └─ onSaveConfig() → saveAgentConfig(id)
  └─ onToggleTool(name, enabled) → toggleTool(id, name)
```

### Chat + A2UI

```
ChatView
  └─ handleSend() → sendChatMessage()
       └─ SSE stream → handleChatEvent()
            ├─ delta → update chatStream (流式文本)
            ├─ a2ui  → a2uiEngine.handleA2uiMessage(jsonl)
            │          └─ createSurface/updateComponents/updateDataModel
            │             └─ surface 关联到当前消息
            └─ final → append chatMessages
                       └─ renderSingleMessage 内联 A2UI surface

用户点击 ControlPanel 按钮
  └─ onAction(functionId, data)
     └─ _handleA2uiAction()
        └─ sendChatMessage("[操作] ...")
           └─ agent 处理并返回结果
```

## 5. 文件变更清单

### 修改文件

| 文件 | 变更 |
|------|------|
| `web/src/ui/views/agents.ts` | 重写 renderOverview，改进 renderTools，统一 renderPlaceholder |
| `web/src/ui/views/chat.ts` | 连接 onAction，改进侧栏，内联 A2UI 到消息 |
| `web/src/ui/controllers/agents.ts` | 新增 loadModelsList 函数 |
| `web/src/ui/controllers/chat.ts` | 会话自动标题，改进 SSE 解析 |
| `web/src/ui/chat/grouped-render.ts` | renderSingleMessage 支持内联 A2UI surface |
| `web/src/ui/chat/a2ui/catalog/device-card.ts` | 完善：telemetry、sparkline、actions |
| `web/src/ui/chat/a2ui/catalog/device-table.ts` | 完善：排序、状态颜色、操作列 |
| `web/src/ui/chat/a2ui/catalog/data-chart.ts` | 重写：SVG 折线图 |
| `web/src/ui/chat/a2ui/catalog/control-panel.ts` | 重写：slider/toggle/choice/button |
| `web/src/styles/components.css` | Agent 样式改进 |
| `web/src/styles/chat/` | 会话侧栏折叠、输入栏改进 |

### 可能新增文件

| 文件 | 说明 |
|------|------|
| `web/src/ui/chat/a2ui/catalog/sparkline.ts` | 迷你折线图 SVG 工具函数（被 DeviceCard/DataChart 复用） |

## 6. 分阶段实施

### Phase 1: Agent 管理页面（模型 + 工具）
1. Agent 选择栏改为下拉选择器
2. 模型配置 Tab：下拉选择 + Temperature 滑块 + System Prompt 文本框 + 保存
3. 工具权限 Tab：搜索过滤 + 批量操作 + 危险标记
4. 占位 Tab：统一优雅占位
5. 新增 controller 函数：loadModelsList

### Phase 2: A2UI IoT 组件
1. 完善 DeviceCard（telemetry + sparkline + actions）
2. 完善 DeviceTable（排序 + 状态颜色 + 操作）
3. 新建 DataChart（SVG 折线图）
4. 新建 ControlPanel（slider/toggle/choice/button）
5. 连接 onAction 回调

### Phase 3: Chat 页面集成
1. A2UI surface 内联到消息气泡
2. 会话侧栏改进（自动标题 + 时间分组 + 折叠）
3. 输入栏改进（多行 + 自动高度）

## 7. 约束

- 不引入第三方图表库，DataChart 用纯 SVG
- 不新增后端 API（Phase 1 复用已有 `/agents/:id/config`、`/tools/catalog`）
- 保持 Light DOM 渲染（`createRenderRoot() { return this; }`）
- 响应式：980px 以下侧栏自动折叠为单列
