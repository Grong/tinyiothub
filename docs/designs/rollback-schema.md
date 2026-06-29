# Rollback Snapshot Schema

> 状态: 设计完成 | 日期: 2026-06-25

## 概述

定义每个可变工具的 before-snapshot 格式和执行回滚的语义。核心理念：「回滚可靠才能信任 AI」。

## 原则

1. **只对明确可逆的工具做自动 snapshot**，不可逆标注「不可回滚」
2. **Snapshot 即时捕获**：在 write 类操作执行前读取当前值
3. **部分回滚明确**：多属性写入时支持按 key 回滚
4. **回滚也是操作**：rollback 记录为 `AgentAction`，关联原 action

## 工具可逆性分类

| 工具 | 可逆性 | Snapshot 内容 | 回滚机制 |
|------|--------|---------------|----------|
| `write_properties` | ✅ 可逆 | 每个被修改属性的旧值 `{prop_key: old_value}` | 反向写入 before_snapshot 值 |
| `alarm_acknowledge` | ⚠️ 部分可逆 | 告警当前状态 | 调用 unacknowledge API |
| `send_command` | ❌ 不可逆 | — | 命令有副作用，无法回滚 |
| `delete_device` | ❌ 不可逆 | — | 删除不可撤销 |
| `create_device` | ⚠️ 部分可逆 | 创建的设备 ID | 删除创建的设备 |
| `write_data` | ✅ 可逆 | 被修改数据点的旧值 | 反向写入 |
| 搜索/读取类 | N/A | — | 只读操作无需回滚 |

## Snapshot Schema

### write_properties 快照

```json
{
  "tool": "write_properties",
  "device_id": "dev-001",
  "before_snapshot": {
    "temperature": { "value": 25.5, "timestamp": "2026-06-25 10:00:00" },
    "humidity": { "value": 60, "timestamp": "2026-06-25 10:00:00" }
  },
  "changed_keys": ["temperature", "humidity"],
  "captured_at": "2026-06-25 10:00:01"
}
```

### alarm_acknowledge 快照

```json
{
  "tool": "alarm_acknowledge",
  "device_id": "dev-001",
  "alarm_id": "alarm-123",
  "before_status": "active",
  "captured_at": "2026-06-25 10:00:00"
}
```

### create_device 快照

```json
{
  "tool": "create_device",
  "device_id": "dev-new-001",
  "created_device_id": "dev-new-001",
  "captured_at": "2026-06-25 10:00:00"
}
```

## 回滚流程

```
用户点击「回滚」
      │
      ▼
查询原 action 的 before_snapshot
      │
      ▼
检查可逆性：不可逆 → 提示"此操作不可回滚"
      │ (可逆)
      ▼
确认对话框："将 dev-001 的 temperature 恢复为 25.5？"
      │ (确认)
      ▼
执行反向操作（write_properties 写入旧值）
      │
      ▼
记录 rollback action（关联原 action，action_type="rollback"）
      │
      ▼
SSE 推送回滚结果到 AI 运维中心
```

## 部分回滚

多属性写入时，用户可以选择部分回滚（只恢复其中一个属性）：

```
原操作：write_properties(dev-001, {temperature: 30, humidity: 80})
before_snapshot: {temperature: 25.5, humidity: 60}

用户只想回滚 temperature：
→ write_properties(dev-001, {temperature: 25.5})
→ humidity 保持 80 不变
→ 记录 partial_rollback action
```

## agent_actions 表扩展

在 `content` JSON 中新增字段：

```json
{
  "type": "auto_executed",
  "tool": "write_properties",
  "deviceId": "dev-001",
  "before_snapshot": { "temperature": 25.5 },
  "rollback_status": "rollbackable",  // rollbackable | rolled_back | irreversible
  "rollback_action_id": null          // 指向回滚操作的 action_id
}
```

## 不可回滚的工具处理

UI 标注：

```
执行历史：
  ✓ write_properties(dev-01) — 已执行 [回滚]
  ✓ send_command(dev-03, "reboot") — 已执行 [不可回滚]
```

hover 提示："命令操作有不可逆副作用，无法回滚。"

## NOT in scope

- 跨工具的多步事务回滚（v2）
- 自动回滚（无需用户确认）—— v1 始终需要确认
- 基于 LLM 推断的回滚策略
