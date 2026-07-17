# 定时任务管理 (Schedule Management)

管理定时任务和自动化规则，让设备操作按计划自动执行。

## 可用工具

| 工具 | 用途 | 风险 |
|---|---|---|
| `list_schedules` | 查看工作空间的所有定时任务 | 低 |
| `create_schedule` | 创建定时任务（Cron 表达式 + 设备操作） | 中 |
| `update_schedule` | 修改定时任务 | 中 |
| `delete_schedule` | 删除定时任务 | 中 |

## 典型场景
- "有哪些定时任务？" → `list_schedules`
- "每天早上 8 点读取所有传感器的温度" → `create_schedule`（cron: `0 8 * * *`）
- "把这个定时任务改成每小时执行一次" → `update_schedule`
- "删除这个不再需要的定时任务" → `delete_schedule`

## 注意事项
- 创建定时任务需要 Cron 表达式（分 时 日 月 周）
- 任务执行的操作通过 `send_command` 或 `read_properties` 等工具完成
- 修改或删除定时任务前确认用户意图
