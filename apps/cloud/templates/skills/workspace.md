---
name: workspace
title: 工作台大屏
description: 用户在工作台页面要求"综合情况 / 看楼层 / 看设备 / 看告警"时的固定剧本：找 3D 场景，推 Stage + Insight 双面板。组件字段见 visualization 技能。
version: "3"
trigger: /workspace
inject: trigger
builtin: "3"
---

# Workspace Visualization — MANDATORY Workflow

**本技能只在工作台页面（用户在看可视化大屏）时使用。**
在这里文本回复是看不见的，必须推送 A2UI 组件；但普通聊天里的图表需求用 `visualization` 技能，不要套用本剧本。

组件 `dataModel` 字段以 `visualization` 技能为准；本技能只规定页面必须出现的面板、组件组合和数据来源。

## Step 1: Find Scene Resources (MANDATORY)

Call `search_knowledge`:

| User says | Parameters |
|-----------|-----------|
| 综合情况 / 总览 / 概览（未指定场景） | `query="默认"`, `entity_type="space"` |
| 查看 XX 楼层 / XX 区域 / XX 车间 | 用用户原话作为 query |
| 查看 XX 设备 | 用设备名作为 query |
| No results | broaden: `query="默认 全局"` |

SAVE the returned `file_path` — you will use it in Step 2. If `search_knowledge` returns
multiple results, pick the one with the highest relevance or the first `.glb`/`.gltf` file.

## Step 2: Query Data + Push A2UI (MANDATORY)

### 2a. Query real data

NEVER use fake numbers. For 综合情况/overview you MUST query ALL of:

1. `list_devices` → device count, status breakdown
2. 如当前工具列表提供告警查询工具，查询告警数量
3. 如当前工具列表提供定时任务工具，查询任务数量

| User is asking about | Required queries |
|---------------------|------------------|
| 综合情况 / 概览 | `list_devices` + 当前可用的告警/任务工具 |
| 查看 XX 楼层 / 区域 | `list_devices` (filter by area) |
| 查看 XX 设备 | `read_device` |
| 告警 | 当前可用的告警查询工具 |
| 任务 / 调度 | 当前可用的任务查询工具 |
| 驱动 | 当前可用的驱动查询工具 |

### 2b. Push A2UI — BOTH Stage AND Insight

Call `canvas(action="a2ui_push", jsonl="...")`. Structure:
- Line 1: `createSurface` for Stage
- Line 2: `updateComponents` for Stage
- Line 3: `createSurface` for Insight
- Line 4: `updateComponents` for Insight (can have MULTIPLE components)

Insight panels render components vertically from top to bottom.
Push StatRow first (key metrics), then lists/tables.

#### Scenario 1: Overview / 综合情况

Query `list_devices` and the available alarm/task tools first, then push:

```jsonl
{"createSurface":{"id":"scene","surfaceKind":"stage"}}
{"updateComponents":{"surfaceId":"scene","components":[{"id":"s1","componentKind":"Scene3D","dataModel":{"modelUrl":"<FILE_PATH>"}}]}}
{"createSurface":{"id":"data","surfaceKind":"insight"}}
{"updateComponents":{"surfaceId":"data","components":[{"id":"d1","componentKind":"StatRow","dataModel":{"items":[{"label":"设备总数","value":"<N>","unit":"台"},{"label":"在线设备","value":"<N>","unit":"台"},{"label":"活跃告警","value":"<N>","unit":"条"},{"label":"调度任务","value":"<N>","unit":"个"}]}},{"id":"d2","componentKind":"AlarmTable","dataModel":{"alarms":[{"alarmId":"<id>","severity":"<critical|warning|info>","title":"<title>","deviceName":"<name>","timestamp":"<time>"}]}},{"id":"d3","componentKind":"ThingTable","dataModel":{"columns":["设备名称","状态","类型","最后上线"],"rows":[["<name>","<online|offline>","<type>","<time>"]]}}]}}
```

#### Scenario 2: View specific floor / area / 查看 XX 楼层/区域

```jsonl
{"createSurface":{"id":"scene","surfaceKind":"stage"}}
{"updateComponents":{"surfaceId":"scene","components":[{"id":"s1","componentKind":"Scene3D","dataModel":{"modelUrl":"<FILE_PATH>"}}]}}
{"createSurface":{"id":"data","surfaceKind":"insight"}}
{"updateComponents":{"surfaceId":"data","components":[{"id":"d1","componentKind":"StatRow","dataModel":{"items":[{"label":"区域设备","value":"<N>","unit":"台"},{"label":"在线","value":"<N>","unit":"台"},{"label":"离线","value":"<N>","unit":"台"}]}},{"id":"d2","componentKind":"ThingTable","dataModel":{"columns":["名称","状态","类型"],"rows":[["<device_name>","<status>","<type>"]]}}]}}
```

#### Scenario 3: View specific device / 查看 XX 设备

Query `read_device` for the device snapshot and live properties.

```jsonl
{"createSurface":{"id":"scene","surfaceKind":"stage"}}
{"updateComponents":{"surfaceId":"scene","components":[{"id":"s1","componentKind":"Scene3D","dataModel":{"modelUrl":"<FILE_PATH>"}}]}}
{"createSurface":{"id":"data","surfaceKind":"insight"}}
{"updateComponents":{"surfaceId":"data","components":[{"id":"d1","componentKind":"ThingCard","dataModel":{"thingId":"<thing_id>","name":"<name>","status":"<status>","properties":[{"key":"温度","value":"25","unit":"°C"},{"key":"湿度","value":"60","unit":"%"}]}},{"id":"d2","componentKind":"DataChart","dataModel":{"title":"温度趋势","unit":"°C","series":[{"name":"温度","color":"#3b82f6","data":[{"time":"10:00","value":25},{"time":"11:00","value":26},{"time":"12:00","value":24}]}]}}]}}
```

#### Scenario 4: Alarms / 告警

如当前工具列表提供告警查询工具，先查告警（未提供时跳过告警面板并说明），再推统计 + 列表：

```jsonl
{"createSurface":{"id":"scene","surfaceKind":"stage"}}
{"updateComponents":{"surfaceId":"scene","components":[{"id":"s1","componentKind":"Scene3D","dataModel":{"modelUrl":"<FILE_PATH>"}}]}}
{"createSurface":{"id":"data","surfaceKind":"insight"}}
{"updateComponents":{"surfaceId":"data","components":[{"id":"d1","componentKind":"StatRow","dataModel":{"items":[{"label":"严重","value":"<N>","unit":"条"},{"label":"警告","value":"<N>","unit":"条"},{"label":"信息","value":"<N>","unit":"条"}]}},{"id":"d2","componentKind":"AlarmTable","dataModel":{"alarms":[{"alarmId":"<id>","severity":"<critical|warning|info>","title":"<title>","deviceName":"<name>","timestamp":"<time>"}]}}]}}
```

#### Scenario 5: No 3D model found — placeholder

```jsonl
{"createSurface":{"id":"scene","surfaceKind":"stage"}}
{"updateComponents":{"surfaceId":"scene","components":[{"id":"s1","componentKind":"Text","dataModel":{"content":"## 场景预览\n\n未找到 3D 模型文件，请上传 .glb/.gltf 资源到知识库。"}}]}}
{"createSurface":{"id":"data","surfaceKind":"insight"}}
{"updateComponents":{"surfaceId":"data","components":[{"id":"d1","componentKind":"StatRow","dataModel":{"items":[{"label":"设备总数","value":"<N>","unit":"台"},{"label":"在线设备","value":"<N>","unit":"台"},{"label":"活跃告警","value":"<N>","unit":"条"},{"label":"调度任务","value":"<N>","unit":"个"}]}},{"id":"d2","componentKind":"AlarmTable","dataModel":{"alarms":[...]}},{"id":"d3","componentKind":"ThingTable","dataModel":{"columns":["设备名称","状态","类型"],"rows":[...]}}]}}
```

## CRITICAL RULES

- `<FILE_PATH>` MUST be the exact `file_path` from `search_knowledge` — do NOT construct or guess
- `<REAL_COUNT>` MUST be real numbers from data tools — NEVER invent
- Stage MUST always be pushed — if no scene model, use Text placeholder (see Scenario 5)
- jsonl format: each `createSurface` and `updateComponents` on its own line, valid JSON
- Push both Stage AND Insight in a SINGLE `canvas` call (all 4 lines together)

## Stage 组件（本页专用）

| Component | dataModel |
|-----------|----------|
| `Scene3D` | `{modelUrl: "<path>"}` |
| `Image` | `{src: "<url>", alt: "<text>"}` |
| `Text` | `{content: "<markdown>"}` — placeholder when no scene found |

Insight 面板用的 `StatRow` / `StatCard` / `ThingCard` / `ThingTable` / `AlarmCard` / `AlarmTable` / `DataChart` 字段以 `visualization` 技能为准，不要自创字段（例如 `DataChart` 用 `series[{name,color,data[{time,value}]}]`，不是 `type/labels/data`）。

## Quick Reference

| User request | Stage | Insight |
|-------------|-------|---------|
| 综合情况 / 概览 | 默认 3D 模型 | StatRow → AlarmTable → ThingTable |
| 查看 XX 楼层 / 区域 | 该区域模型 | StatRow（区域统计）→ ThingTable |
| 查看 XX 设备 | 设备所在场景 | ThingCard → DataChart |
| 今天告警 / 最近告警 | 默认模型 | StatRow（告警统计）→ AlarmTable |
| 任务 / 调度 / 驱动状态 | 默认模型 | StatRow → 对应列表 |
| 无 3D 模型 | Text 占位 | 完整数据面板（同综合情况） |
