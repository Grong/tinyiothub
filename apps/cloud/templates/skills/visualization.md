---
name: visualization
title: 可视化展示
description: 当任务需要以卡片、表格、图表或控制面板展示设备、告警和趋势时，使用 canvas 推送 A2UI。
version: "1"
inject: index
---

# 可视化

## 适用条件

- 用户要求展示设备列表、告警、趋势、统计或操作面板；
- 当前任务需要比纯文本更清楚的结构化视觉表达；
- 已具备真实工具结果，可视化只负责展示，不能编造数据或替代结论。

非可视化任务不加载本技能，普通查询不强制使用 canvas。

## 使用方式

调用 `canvas` 工具，`action` 固定为 `"a2ui_push"`。`jsonl` 至少包含 `createSurface` 和 `updateComponents`，组件内容必须来自真实结果。

### 示例：展示温湿度设备

```jsonl
{"createSurface":{"surfaceId":"device-view","surfaceKind":"inline","title":"设备列表"}}
{"updateComponents":{"surfaceId":"device-view","components":[{"id":"card-1","componentKind":"ThingCard","dataModel":{"thingId":"dev_temp_001","name":"温湿度传感器-01","status":"online","category":"temp_humidity","primaryMetric":{"key":"温度","value":"25.6","unit":"°C"},"properties":[{"name":"温度","value":"25.6","unit":"°C"},{"name":"湿度","value":"68","unit":"%"}],"signalStrength":85,"lastSeen":"2026-05-24T10:30:00Z","actions":[{"label":"查看详情","functionId":"viewDevice"},{"label":"控制","functionId":"controlDevice"}]}}]}}
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

| 组件 | 说明 | 参数 |
|------|------|------|
| Text | 文本展示 | content, style? |
| Image | 图片 | src, alt?, width?, height? |
| Icon | 图标 | name, size?, color? |
| Row | 水平布局 | children |
| Column | 垂直布局 | children |
| List | 列表 | items, itemTemplate? |
| Card | 卡片 | title, children |
| Tabs | 标签页 | tabs, activeTabId? |
| Modal | 模态框 | title, children, visible |
| Button | 按钮 | label, onClick?, variant? |
| TextField | 文本输入 | label, value, placeholder? |
| CheckBox | 复选框 | label, checked |
| ChoicePicker | 选择器 | options, selectedId? |
| Slider | 滑块 | min, max, value, step? |
| DateTimeInput | 日期时间输入 | value, format? |

## IoT 组件速查

| 组件 | 用途 | 关键字段 |
|------|------|---------|
| ThingCard | 单设备详情卡片 | thingId, name, status, icon?, category?, primaryMetric?{key,value,unit}, properties?[{name,value,unit}], telemetry?[{name,value,unit}], signalStrength?, lastSeen?, sparkline?, tags?[], actions?[{label,functionId}] |
| ThingTable | 设备列表表格 | columns[], rows[][] |
| AlarmCard | 告警卡片 | alarmId, severity, title, message, deviceName, timestamp |
| AlarmTable | 告警列表表格 | alarms[] |
| StatCard | 统计数值卡片 | label, value, unit?, description?, icon?, color?, trend? |
| StatRow | 横向统计条 | items[]（label, value, unit?, description?）, columns? |
| ProgressIndicator | 进度/百分比 | label, value, max, variant(linear/circular), color |
| ConfirmationDialog | 确认对话框 | title, message, confirmLabel, cancelLabel |
| ControlPanel | 设备控制面板 | thingId, controls[] |
| DataChart | 历史数据图表 | chartType, labels[], datasets[] |
| Scene3D | 3D 建筑场景展示 | modelUrl, resourceId?, activeFloorId?, selectedDeviceId?, deviceFilter?, interactions? |

可视化失败不影响真实事实和文字总结；不得为了使用组件而执行额外写操作。
