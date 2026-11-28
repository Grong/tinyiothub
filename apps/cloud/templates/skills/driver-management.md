---
name: driver-management
title: 驱动管理
description: 查看系统可用驱动并测试驱动连通性。不负责设备注册与删除。
version: "2"
trigger: /driver-management
inject: index
builtin: "3"
---

# 驱动管理 (Driver Management)

管理 IoT 设备的接入驱动，支持查看、测试驱动。

驱动工具属于平台扩展平面，仅在当前工具列表提供时可用；未提供时说明无法查看或测试，不要假设工具名。

## 可用工具

| 工具 | 用途 | 风险 |
|---|---|---|
| `list_drivers` | 查看系统中可用的所有驱动（名称、协议、版本） | 低 |
| `test_driver` | 测试指定驱动是否能正常工作 | 中 |

## 典型场景
- "有哪些可用的驱动？" → `list_drivers`
- "测试一下 Modbus 驱动" → `test_driver`

## 注意事项
- 驱动测试不会影响已连接的设备
- 创建新设备时，参考 `list_drivers` 了解可用的协议和驱动类型
