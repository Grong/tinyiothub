---
description: Workspace page visualization — find scene + push A2UI (Stage + Insight). BLOCKING REQUIREMENT when user is on Workspace.
version: "2.1"
---

# Workspace Visualization — MANDATORY Workflow

**This skill is a BLOCKING REQUIREMENT on the Workspace page.**
You MUST complete ALL steps. The user can ONLY see A2UI components —
text-only responses are INVISIBLE and useless here.

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

1. `search_devices` → device count, status breakdown
2. `alarm_list` → alarm count, recent alarms
3. `list_schedules` → schedule count

| User is asking about | Required queries |
|---------------------|------------------|
| 综合情况 / 概览 | `search_devices` + `alarm_list` + `list_schedules` |
| 查看 XX 楼层 / 区域 | `search_devices` (filter by area) |
| 查看 XX 设备 | `get_device` |
| 告警 | `alarm_list` |
| 任务 / 调度 | `list_schedules` |
| 驱动 | `list_drivers` |

### 2b. Push A2UI — BOTH Stage AND Insight

Call `canvas(action="a2ui_push", jsonl="...")`. Structure:
- Line 1: `createSurface` for Stage
- Line 2: `updateComponents` for Stage
- Line 3: `createSurface` for Insight
- Line 4: `updateComponents` for Insight (can have MULTIPLE components)

Insight panels render components vertically from top to bottom.
Push StatRow first (key metrics), then lists/tables.

#### Scenario 1: Overview / 综合情况

Query `search_devices` + `alarm_list` + `list_schedules` first, then push:

```jsonl
{"createSurface":{"id":"scene","surfaceKind":"stage"}}
{"updateComponents":{"surfaceId":"scene","components":[{"id":"s1","componentKind":"Scene3D","dataModel":{"modelUrl":"<FILE_PATH>"}}]}}
{"createSurface":{"id":"data","surfaceKind":"insight"}}
{"updateComponents":{"surfaceId":"data","components":[{"id":"d1","componentKind":"StatRow","dataModel":{"items":[{"label":"设备总数","value":"<N>","unit":"台"},{"label":"在线设备","value":"<N>","unit":"台"},{"label":"活跃告警","value":"<N>","unit":"条"},{"label":"调度任务","value":"<N>","unit":"个"}]}},{"id":"d2","componentKind":"AlarmTable","dataModel":{"alarms":[{"alarmId":"<id>","severity":"<critical|warning|info>","title":"<title>","deviceName":"<name>","timestamp":"<time>"}]}},{"id":"d3","componentKind":"DeviceTable","dataModel":{"columns":["设备名称","状态","类型","最后上线"],"rows":[["<name>","<online|offline>","<type>","<time>"]]}}]}}
```

#### Scenario 2: View specific floor / area / 查看 XX 楼层/区域

```jsonl
{"createSurface":{"id":"scene","surfaceKind":"stage"}}
{"updateComponents":{"surfaceId":"scene","components":[{"id":"s1","componentKind":"Scene3D","dataModel":{"modelUrl":"<FILE_PATH>"}}]}}
{"createSurface":{"id":"data","surfaceKind":"insight"}}
{"updateComponents":{"surfaceId":"data","components":[{"id":"d1","componentKind":"StatRow","dataModel":{"items":[{"label":"区域设备","value":"<N>","unit":"台"},{"label":"在线","value":"<N>","unit":"台"},{"label":"离线","value":"<N>","unit":"台"}]}},{"id":"d2","componentKind":"DeviceTable","dataModel":{"columns":["名称","状态","类型"],"rows":[["<device_name>","<status>","<type>"]]}}]}}
```

#### Scenario 3: View specific device / 查看 XX 设备

Query `get_device` for device details + `read_properties` for live properties.

```jsonl
{"createSurface":{"id":"scene","surfaceKind":"stage"}}
{"updateComponents":{"surfaceId":"scene","components":[{"id":"s1","componentKind":"Scene3D","dataModel":{"modelUrl":"<FILE_PATH>"}}]}}
{"createSurface":{"id":"data","surfaceKind":"insight"}}
{"updateComponents":{"surfaceId":"data","components":[{"id":"d1","componentKind":"DeviceCard","dataModel":{"deviceId":"<device_id>","name":"<name>","status":"<status>","properties":[{"key":"温度","value":"25","unit":"°C"},{"key":"湿度","value":"60","unit":"%"}]}},{"id":"d2","componentKind":"DataChart","dataModel":{"type":"line","labels":["10:00","11:00","12:00"],"data":[25,26,24]}}]}}
```

#### Scenario 4: Alarms / 告警

Query `alarm_list`, then push stats + list:

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
{"updateComponents":{"surfaceId":"data","components":[{"id":"d1","componentKind":"StatRow","dataModel":{"items":[{"label":"设备总数","value":"<N>","unit":"台"},{"label":"在线设备","value":"<N>","unit":"台"},{"label":"活跃告警","value":"<N>","unit":"条"},{"label":"调度任务","value":"<N>","unit":"个"}]}},{"id":"d2","componentKind":"AlarmTable","dataModel":{"alarms":[...]}},{"id":"d3","componentKind":"DeviceTable","dataModel":{"columns":["设备名称","状态","类型"],"rows":[...]}}]}}
```

## CRITICAL RULES

- `<FILE_PATH>` MUST be the exact `file_path` from `search_knowledge` — do NOT construct or guess
- `<REAL_COUNT>` MUST be real numbers from data tools — NEVER invent
- Stage MUST always be pushed — if no scene model, use Text placeholder (see Scenario 5)
- jsonl format: each `createSurface` and `updateComponents` on its own line, valid JSON
- Push both Stage AND Insight in a SINGLE `canvas` call (all 4 lines together)

## Available A2UI Components

### Stage (left panel — "WHERE")

| Component | dataModel |
|-----------|----------|
| `Scene3D` | `{modelUrl: "<path>"}` |
| `Image` | `{src: "<url>", alt: "<text>"}` |
| `Text` | `{content: "<markdown>"}` — placeholder when no scene found |

### Insight (right panel — "HOW MANY / WHAT STATE")

| Component | dataModel |
|-----------|----------|
| `StatRow` | `{items: [{label, value, unit?}]}` — horizontal stat cards |
| `StatCard` | `{label, value, unit?, icon?}` — single stat |
| `DeviceCard` | `{deviceId, name, status, properties?: [{key, value, unit?}]}` |
| `DeviceTable` | `{columns: [string], rows: [[string]]}` |
| `AlarmCard` | `{alarmId, severity, title, message, deviceName, timestamp}` |
| `AlarmTable` | `{alarms: [{alarmId, severity, title, deviceName, timestamp}]}` |
| `DataChart` | `{type: "bar"|"line"|"pie", data: [number], labels?: [string]}` |
| `Text` | `{content: "<markdown>"}` |

## Quick Reference

| User request | Stage | Insight |
|-------------|-------|---------|
| 综合情况 / 概览 | 默认 3D 模型 | StatRow → AlarmTable → DeviceTable |
| 查看 XX 楼层 / 区域 | 该区域模型 | StatRow（区域统计）→ DeviceTable |
| 查看 XX 设备 | 设备所在场景 | DeviceCard → DataChart |
| 今天告警 / 最近告警 | 默认模型 | StatRow（告警统计）→ AlarmTable |
| 任务 / 调度 / 驱动状态 | 默认模型 | StatRow → 对应列表 |
| 无 3D 模型 | Text 占位 | 完整数据面板（同综合情况） |
