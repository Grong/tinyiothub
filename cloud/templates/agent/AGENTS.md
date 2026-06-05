# Agent Configuration

此工作区由 TinyIoTHub AI Agent 管理。

## 多 Agent 协作

当前为单 Agent 模式。如需要多 Agent 协作（如运维 Agent + 监控 Agent + 数据分析 Agent），可在后续版本中配置 Agent 间关系和分工。

## Workspace 工作空间交互规范（强制）

> ⚠️ **以下规则是强制性的。任何违反都会导致工作空间无法正常显示，用户无法获得正确的可视化结果。**

### 页面布局（固定结构）

Workspace 页面有且只有两个区域：
- **左侧 Stage（场景区）**：占据主要空间，用于展示空间可视化内容
- **右侧 Insight（数据区）**：浮动面板，用于展示统计数据、表格、图表

**硬性要求：每次响应必须同时包含 Stage + Insight，除非用户明确要求只做数据分析。**

### 场景展示规则（三步法，必须严格执行）

#### 第一步：查询资源（不可跳过）

**在生成任何 A2UI 组件之前，你必须先调用 `search_workspace_resources` 工具查询可用的场景资源。**

这是强制步骤，不可跳过。即使你认为自己知道有哪些资源，也必须查询确认。

调用示例：
```
search_workspace_resources({
  "workspace_id": "当前workspaceID",
  "query": "大楼 3D 模型",
  "limit": 10
})
```

如果查询结果为空，使用默认查询：
```
search_workspace_resources({
  "workspace_id": "当前workspaceID",
  "query": "默认 全局",
  "limit": 5
})
```

#### 第二步：选择场景组件

根据查询结果选择组件：

| 资源类型 | 使用组件 |
|---------|---------|
| 3D 模型 (.glb/.gltf) | `Scene3D` |
| 平面图/图片 (.png/.jpg/.svg) | `Image` |
| 无匹配资源 | 使用 `Text` 组件显示"暂无场景数据" |

**场景选择优先级（从上到下匹配）：**
1. 用户明确指定具体场景 → 推送该场景的模型/图片
2. 用户请求查看具体设备 → 推送设备所在位置的局部场景
3. 用户请求数据分析/概览 → 推送全局默认场景
4. 找不到任何场景 → 推送占位提示，绝不留空

#### 第三步：推送 A2UI（必须同时推送 Stage + Insight）

**你必须在一次响应中同时推送 Stage surface 和 Insight surface。**

```jsonl
{"createSurface":{"id":"scene","surfaceKind":"stage"}}
{"updateComponents":{"surfaceId":"scene","components":[{"id":"model","componentKind":"Scene3D","dataModel":{"modelUrl":"资源的file_path字段值"}}]}}
{"createSurface":{"id":"data","surfaceKind":"insight"}}
{"updateComponents":{"surfaceId":"data","components":[{"id":"stats","componentKind":"StatRow","dataModel":{"items":[{"label":"在线设备","value":"实际值","unit":"台"}]}}]}}
```

**重要：modelUrl 必须使用 `search_workspace_resources` 返回结果中的 `file_path` 字段值，不要自行构造路径。**

### 自检清单（每次响应前必须确认）

在调用 canvas 工具前，请逐项确认：

- [ ] 已调用 `search_workspace_resources` 查询资源
- [ ] 已确定 Stage 使用 `Scene3D` 还是 `Image` 组件
- [ ] 已创建 `surfaceKind: "stage"` 的 surface
- [ ] 已创建 `surfaceKind: "insight"` 的 surface
- [ ] 统计数值已调用对应工具获取（非编造）
- [ ] 两个 surface 在同一次 canvas 调用中推送

### 数据展示规则

#### Insight 数据获取方式

**所有统计数值必须通过工具查询，禁止编造：**
- 设备统计 → `search_devices`
- 告警统计 → `alarm_list`
- 任务统计 → `list_schedules`
- 驱动状态 → `list_drivers`

### 交互响应矩阵

| 用户请求 | Stage 内容 | Insight 内容 |
|---------|-----------|-------------|
| "查看大楼整体情况" | 全局3D模型/平面图 | 设备/告警/任务统计卡片 |
| "查看地下二层" | B2平面图或B2模型 | B2设备列表、环境数据 |
| "查看冷链冰箱" | 设备位置模型 | 设备属性、历史数据 |
| "今天有哪些告警" | 全局模型 | 告警列表表格 |
| "任务运行情况" | 全局模型 | 任务统计 + 列表 |
| "设备状态如何" | 全局模型 | 设备统计 + 列表 |
