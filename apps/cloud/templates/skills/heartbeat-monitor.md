# 心跳巡检 (Heartbeat Monitor)

作为 AI Agent 的核心自主行为，定时巡检工作空间内的设备健康状态，发现异常时主动诊断并报告。

## 巡检内容（按 HEARTBEAT.md 定义）

1. **设备在线检查** — 检查是否有离线设备，分析离线原因
2. **告警扫描** — 扫描未处理的高优先级告警
3. **状态日报** — 生成设备运行状态摘要
4. **系统资源** — 检查系统磁盘、内存使用率（如可访问）
5. **数据连续性** — 检查是否有数据断档或采集异常

## 执行流程

每次心跳巡检按以下步骤执行：

1. **扫描设备** → `search_devices` 获取所有设备列表
2. **读取状态** → 对关键设备调用 `read_properties` 获取实时数据
3. **检查告警** → `alarm_list` 查看是否有未处理告警
4. **分析诊断** → 发现异常时，用 `get_device` + `search_knowledge` 深入了解
5. **生成报告** → 汇总巡检结果，正常设备简要列出，异常设备重点说明
6. **主动告警** → 发现严重问题时，通过 `canvas` 展示诊断卡片

## 可用工具（作为巡检 agent 的工作箱）

- `search_devices` / `get_device` / `read_properties` — 设备状态检查
- `alarm_list` / `alarm_acknowledge` / `alarm_rule_add` — 告警管理
- `search_knowledge` — 查阅设备手册和维护记录
- `send_command` — 执行修复命令（需用户确认）
- `canvas` — 可视化展示巡检结果

## 注意事项
- 巡检是自主行为，看到异常主动报告，不要等用户问
- 给出诊断结论和修复建议，标注风险等级
- 低风险修复（如 read_properties 确认异常）可自行执行
- 高风险操作（send_command、write_properties）必须获得用户确认
