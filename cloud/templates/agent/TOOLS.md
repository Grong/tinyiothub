# Available Tools

你可以使用以下工具类别的 MCP 工具：

## 设备管理
- 搜索、查看、创建、删除设备
- 读取和写入设备属性
- 向设备下发控制命令

## 告警管理
- 查询告警列表和历史
- 确认和关闭告警
- 创建告警规则

## 驱动管理
- 查询已注册的协议驱动
- 测试驱动连接状态

## 任务管理
- 查询、创建、更新、删除调度任务

## 工作空间资源查询

- `search_workspace_resources` — 搜索工作空间的多媒体资源（3D 场景、平面图、图片）
  - 参数：`workspace_id`（必填）、`query`（自然语言查询，必填）、`resource_type`（可选过滤）、`limit`（默认 10）
  - 使用时机：当用户请求查看场景、楼层、设备位置时，必须先调用此工具查询可用的 3D 模型和平面图资源
  - 示例：`search_workspace_resources({"workspace_id": "ws-001", "query": "大楼 3D 模型", "limit": 5})`

## A2UI 组件

Agent 可以通过 `canvas` 工具推送以下 UI 组件到前端：

### Basic 组件
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

### IoT 组件
| 组件 | 说明 | 参数 |
|------|------|------|
| DeviceCard | 设备卡片 | deviceId, name, status, properties[] |
| DeviceTable | 设备表格 | devices[], columns? |
| AlarmCard | 告警卡片 | alarmId, severity, title, message, deviceName, timestamp |
| AlarmTable | 告警表格 | alarms[] |
| DataChart | 数据图表 | type, data[], labels? |
| Scene3D | 3D 建筑场景展示 | modelUrl, resourceId?, activeFloorId?, selectedDeviceId?, deviceFilter?, interactions? |
| ControlPanel | 控制面板 | controls[], layout? |
| ProgressIndicator | 进度指示 | value, max, label? |
| StatCard | 统计数值卡片 | label, value, unit?, description?, icon?, color?, trend? |
| StatRow | 横向统计条（多个 StatCard 并排） | items[]（每个包含 label, value, unit?, description?）, columns? |

工具权限可由管理员在「工具权限」Tab 中单独开启或关闭。

## 可视化 A2UI 渲染（canvas 工具）

你可以使用 `canvas` 工具将数据以可视化组件的形式展现给用户，让信息更直观可读。

**核心原则：凡是查询/搜索类操作的结果，都应该用 canvas 渲染，而不是只输出纯文本。**

### 何时使用 canvas

- **设备列表/搜索结果** → 用 `DeviceCard` 或 `DeviceTable` 渲染每个设备
- **告警列表/查询结果** → 用 `AlarmCard` 或 `AlarmTable` 渲染告警
- **传感器数值/状态** → 用 `StatCard` 展示关键指标
- **设备当前读数** → 用 `ProgressIndicator` 展示温度、湿度等百分比指标
- **确认操作** → 用 `ConfirmationDialog` 让用户二次确认删除等危险操作

### 使用方法

调用 `canvas` 工具，`action` 固定为 `"a2ui_push"`，`jsonl` 包含两个消息：`createSurface` 创建 UI 容器，`updateComponents` 添加组件。

### 示例：展示温湿度设备

```jsonl
{"createSurface":{"surfaceId":"device-view","surfaceKind":"inline","title":"设备列表"}}
{"updateComponents":{"surfaceId":"device-view","components":[{"id":"card-1","componentKind":"DeviceCard","dataModel":{"deviceId":"dev_temp_001","name":"温湿度传感器-01","status":"online","deviceType":"temp_humidity","primaryMetric":{"key":"温度","value":"25.6","unit":"°C"},"properties":[{"name":"温度","value":"25.6","unit":"°C"},{"name":"湿度","value":"68","unit":"%"}],"signalStrength":85,"lastSeen":"2026-05-24T10:30:00Z","actions":[{"label":"查看详情","functionId":"viewDevice"},{"label":"控制","functionId":"controlDevice"}]}}]}}
```

### 组件速查

| 组件 | 用途 | 关键字段 |
|------|------|---------|
| DeviceCard | 单设备详情卡片 | deviceId, name, status, icon?, deviceType?, primaryMetric?{key,value,unit}, properties?[{name,value,unit}], telemetry?[{name,value,unit}], signalStrength?, lastSeen?, sparkline?, tags?[], actions?[{label,functionId}] |
| DeviceTable | 设备列表表格 | columns[], rows[][] |
| AlarmCard | 告警卡片 | alarmId, severity, title, message, deviceName, timestamp |
| AlarmTable | 告警列表表格 | alarms[] |
| StatCard | 统计数值卡片 | label, value, unit?, description?, icon?, color?, trend? |
| StatRow | 横向统计条 | items[]（label, value, unit?, description?）, columns? |
| ProgressIndicator | 进度/百分比 | label, value, max, variant(linear/circular), color |
| ConfirmationDialog | 确认对话框 | title, message, confirmLabel, cancelLabel |
| ControlPanel | 设备控制面板 | deviceId, controls[] |
| DataChart | 历史数据图表 | chartType, labels[], datasets[] |
