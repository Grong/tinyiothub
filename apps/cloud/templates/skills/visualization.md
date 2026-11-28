---
name: visualization
title: 可视化展示
description: 通用 A2UI 组件契约与 canvas 调用方式：把已有工具结果渲染成卡片、表格、图表、控制面板。不含工作台大屏剧本。
version: "2"
trigger: /visualization
inject: trigger
builtin: "3"
---

# 可视化

## 适用条件

- 用户要求展示设备列表、告警、趋势、统计或操作面板；
- 当前任务需要比纯文本更清楚的结构化视觉表达；
- 已具备真实工具结果，可视化只负责展示，不能编造数据或替代结论。

非可视化任务不加载本技能，普通查询不强制使用 canvas。

用户在工作台页面要求"综合情况 / 看楼层 / 看设备"的大屏时，改用 `workspace` 技能；本技能不负责那套 Stage + Insight 剧本。

## 调用方式

调用 `canvas` 工具，`action` 固定为 `"a2ui_push"`。`jsonl` 每行一条消息，至少包含一条 `createSurface` 和一条 `updateComponents`。

**字段名硬约束**（写错会导致面板空白）：

- `createSurface` 用 `id`，不是 `surfaceId`：`{"createSurface":{"id":"device-view","surfaceKind":"inline","title":"设备列表"}}`
- `updateComponents` 用 `surfaceId`，值是上面那个 `id`。
- `componentKind` 必须是下方表格里的组件名；`dataModel` 的字段按表格写，不要自创字段。

### 示例：展示温湿度设备

```jsonl
{"createSurface":{"id":"device-view","surfaceKind":"inline","title":"设备列表"}}
{"updateComponents":{"surfaceId":"device-view","components":[{"id":"card-1","componentKind":"ThingCard","dataModel":{"thingId":"dev_temp_001","name":"温湿度传感器-01","status":"online","category":"temp_humidity","primaryMetric":{"key":"温度","value":"25.6","unit":"°C"},"properties":[{"name":"温度","value":"25.6","unit":"°C"},{"name":"湿度","value":"68","unit":"%"}],"signalStrength":85,"lastSeen":"2026-05-24T10:30:00Z","actions":[{"label":"查看详情","functionId":"viewDevice"}]}}]}}
```

## 组件选择

| 内容 | 建议组件 |
|---|---|
| 单设备详情 | ThingCard |
| 多设备列表或搜索结果 | ThingTable |
| 单条告警 | AlarmCard |
| 告警列表 | AlarmTable |
| 关键指标 | StatCard / StatRow |
| 历史数据与趋势 | DataChart |
| 设备控制 | ControlPanel |
| 危险操作确认 | ConfirmationDialog |

## Basic 组件

| 组件 | 参数 |
|------|------|
| Text | content, style? |
| Image | src, alt?, width?, height? |
| Icon | name, size?, color? |
| Row / Column | children |
| List | items, itemTemplate? |
| Card | title, children |
| Tabs | tabs, activeTabId? |
| Modal | title, children, visible |
| Button | label, onClick?, variant? |
| TextField | label, value, placeholder? |
| CheckBox | label, checked |
| ChoicePicker | options, selectedId? |
| Slider | min, max, value, step? |
| DateTimeInput | value, format? |
| Divider | — |

## IoT 组件

| 组件 | dataModel 字段 |
|------|------|
| ThingCard | thingId, name, status, icon?, category?, primaryMetric?{key,value,unit}, properties?[{name,value,unit}], signalStrength?, lastSeen?, sparkline?[], tags?[], actions?[{label,functionId}] |
| ThingTable | columns?[], rows?: string[][]（或传 things?[]） |
| AlarmCard | alarmId, severity, title, message, deviceName, timestamp |
| AlarmTable | alarms[]，每项 {alarmId, severity, title, deviceName, timestamp} |
| StatCard | label, value, unit?, description?, icon?, trend? |
| StatRow | items[]（label, value, unit?, description?, color?）, columns? |
| ProgressIndicator | label, value, max, variant(linear/circular), color |
| ConfirmationDialog | title, message, confirmLabel, cancelLabel |
| ControlPanel | thingId, controls[] |
| DataChart | title?, unit?, timeRange?, series[{name,color,data[{time,value}]}], thresholds?[{label,value,color}] |
| Scene3D | modelUrl 或 resourceId, metadata?, devices?[], activeFloorId? |

可视化失败不影响真实事实和文字总结；不得为了使用组件而执行额外写操作。
