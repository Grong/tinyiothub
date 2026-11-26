---
name: device-management
title: 设备管理
description: 查看设备清单和状态，读写设备属性并执行设备动作。
version: "1"
trigger: /device-management
inject: index
---

# 设备管理 (Device Management)

管理和操作 IoT 设备：查看设备清单、读取状态、属性读写、执行设备动作。

## 可用工具

| 工具 | 用途 | 风险 |
|---|---|---|
| `list_devices` | 列出设备，支持按名称、类型、状态过滤 | 低 |
| `get_device_profile` | 一次读取设备/空间的完整档案：属性定义与当前值、动作、知识文档正文、最近事件 | 低 |
| `read_device` | 读取设备实时快照：状态、健康度、区域、属性值；可用 `properties` 过滤属性，用 `sources` 读取画面/检测结果 | 低 |
| `set_device` | 写入设备属性值 | **高** |
| `call_device` | 执行设备动作/命令（如重启、切换模式） | **高** |
| `get_thing_tree` | 查看空间、楼层、产线和设备的层级关系 | 低 |
| `query_events` | 回看设备事件和历史 | 低 |
| `list_composites` / `run_composite` | 查看或执行预定义动作链 | 低 / **高** |

## 典型场景

### 查询设备
- "有哪些设备在线？" → `list_devices` 列出设备并统计在线状态
- "传感器 A 的温度是多少？" → `list_devices` 找到设备 → `get_device_profile`（档案里已含当前值）或 `read_device(properties=["温度"])`
- "设备 B 的详细信息 / 手册 / 怎么巡检 / 告警怎么处理" → `get_device_profile` 一次取回属性、动作和知识文档，不要逐属性读取
- "3 号楼有哪些设备？" → `get_thing_tree` 或 `list_devices` 按区域过滤

### 控制设备
- "把空调调到 26 度" → `list_devices` 确认设备 → 向用户确认 → `set_device`
- "重启网关设备" → `list_devices` 确认设备 → 向用户确认 → `call_device`
- 动作链优先使用 `list_composites` 查看，再按需 `run_composite`

## 注意事项
- `set_device`、`call_device`、`run_composite` 必须服从当前权限、Policy、审批和 Effect 边界
- 写入前先 `read_device` 确认设备在线和当前值，写入后回读验证
- 属性值类型必须匹配（字符串、数字、布尔）
- 离线设备无法写入或执行动作，先检查设备状态
- 设备注册/删除不在当前 Agent 工具集中；如用户需要，说明应通过平台设备管理界面完成
