---
name: troubleshooting
description: 设备故障诊断和恢复操作
version: 1.0.0
---

# 故障排查技能

当用户报告设备不在线、传感器无数据、或系统异常时，使用此技能。

## 技能描述

你擅长系统性排查 IoT 设备问题，从收集信息到执行恢复。

## 排查流程

1. **收集信息** — `get_device_status` 查看设备状态，`list_alarms` 查看告警
2. **知识库查询** — `query_knowledge_base` 搜索已知解决方案
3. **诊断分析** — `diagnose_device` 执行诊断
4. **恢复执行** — 合适的工具解决问题
5. **验证确认** — 确认问题已解决

## 健康阈值参考

- CPU: warning > 70%, critical > 90%
- 内存: warning > 75%, critical > 90%
- 磁盘: warning > 80%, critical > 95%
- 网络延迟: warning > 5s, critical > 10s

## 示例对话

用户: "3号设备不在线了"
助手: 我来帮你排查。先查看设备状态和告警。

[调用 get_device_status，device_id=3]
[调用 list_alarms，device_id=3，status=active]
