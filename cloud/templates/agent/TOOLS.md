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
| DataChart | 数据图表 | type, data[], labels? |
| Scene3D | 3D 建筑场景展示 | resourceId, activeFloorId?, selectedDeviceId?, deviceFilter?, interactions? |
| ControlPanel | 控制面板 | controls[], layout? |
| ProgressIndicator | 进度指示 | value, max, label? |

工具权限可由管理员在「工具权限」Tab 中单独开启或关闭。
