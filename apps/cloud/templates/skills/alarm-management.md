---
name: alarm-management
title: 告警管理
description: 查询、确认告警并创建告警规则。确认只表示已知晓，不等于修复；根因诊断交给 troubleshooting。
version: "2"
trigger: /alarm-management
inject: index
builtin: "3"
---

# 告警管理 (Alarm Management)

查看和管理设备告警：告警查询、确认处理、规则创建。

## 工具边界

告警工具属于平台扩展平面，仅在当前工具列表提供时可用；未提供时说明无法查询或确认，不要假设工具名，也不要用其它工具代替。

| 工具 | 用途 | 风险 |
|---|---|---|
| `alarm_list` | 查询告警列表，可按设备、等级、状态、时间筛选 | 低 |
| `alarm_acknowledge` | 确认告警（表示已知晓，正在处理） | 低 |
| `alarm_rule_add` | 创建告警规则（阈值、持续时间、通知方式） | 中 |

## 告警等级
- **critical** — 严重，需立即处理（设备离线、关键指标严重超标）
- **error** — 错误，需尽快处理
- **warning** — 警告，常规关注
- **info** — 信息通知

## 查询纪律

- **告警只查一次**：一次 `alarm_list` 就同时返回当页 `data`、`pagination.total_count` 和 `summary`（`summary.total` 与 `summary.by_level` 各等级数量，忽略 levels/statuses 过滤，沿用设备与时间范围）。
- 不要按等级拆成多次查询，也不要用 `pageSize=1` 去试探数量；需要明细就调大 `pageSize` 或翻页。
- 同一轮内不要重复查询未变化的告警；上一次结果已够回答就直接给结论。

## 典型场景
- "有什么告警？" → `alarm_list`（默认展示未恢复的告警）
- "最近有什么严重告警？" → `alarm_list` 一次调用后按 `summary.by_level.critical` 报数，并列出当页 critical 明细
- "这个告警我知道了" → `alarm_acknowledge`
- "设置一个高温告警：温度 > 80°C 持续 5 分钟就告警" → `alarm_rule_add`

## 与其它技能的分工

- 本技能只负责告警的查询 / 确认 / 规则，不做多步根因分析。
- 告警需要定位原因、评估影响或给出分级修复方案时，交给 `troubleshooting` 技能。
- 确认告警不会自动修复设备；需要改配置或下发命令时，按 `device-management` 技能走审批流程。

## 注意事项
- 创建告警规则时需要指定：触发条件（如 "temperature > 80"）、持续时间、告警等级
- 已恢复的告警默认不展示，需要时指定状态过滤
- 告警确认只是标记已知晓，不会自动修复，需要进一步诊断和处理
