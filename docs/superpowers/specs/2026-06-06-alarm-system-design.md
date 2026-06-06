# 报警系统全功能设计

> 状态: 设计完成 | 分支: `feature/alarm` | 日期: 2026-06-06

## 概述

实现 TinyIoTHub 报警系统的全部后端功能。前端 API 客户端和视图已完备，后端需实现数据库表、存储层、规则评估引擎、API 处理器、通知分发及高级功能（批量操作、告警抑制、告警升级、自动恢复）。

## 架构

```
设备数据写入 → 触发告警规则评估 → 生成告警 → 通知分发 → 告警生命周期管理
                                                                   ├── 确认 (Acknowledge)
                                                                   ├── 解决 (Resolve)
                                                                   ├── 抑制 (Suppression)
                                                                   ├── 升级 (Escalation)
                                                                   └── 自动恢复 (AutoResolve)
```

### 模块依赖顺序

1. 数据库表 (SQL 迁移)
2. 存储层 (SQL 函数)
3. 规则评估引擎 (核心逻辑)
4. API 处理器 (HTTP 端点)
5. 通知分发 (多渠道)
6. 高级功能 (抑制/升级/自动恢复)

---

## 模块 1: 数据库表

### alarm_rules

```sql
CREATE TABLE IF NOT EXISTS alarm_rules (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    description     TEXT DEFAULT '',
    device_id       TEXT DEFAULT NULL,
    property_id     TEXT DEFAULT NULL,
    rule_type       TEXT NOT NULL,          -- threshold/range/change/duration/composite
    condition       TEXT NOT NULL,           -- JSON: AlarmCondition
    alarm_level     TEXT NOT NULL DEFAULT 'Warning',
    is_enabled      INTEGER DEFAULT 1,
    notification_config TEXT DEFAULT '{}',   -- JSON: NotificationConfig
    suppress_duration_secs INTEGER DEFAULT 0,
    escalation_delay_secs INTEGER DEFAULT 0,
    escalation_level TEXT DEFAULT NULL,
    workspace_id    TEXT DEFAULT NULL,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_alarm_rules_device_id ON alarm_rules(device_id);
CREATE INDEX IF NOT EXISTS idx_alarm_rules_enabled ON alarm_rules(is_enabled);
```

### alarms

```sql
CREATE TABLE IF NOT EXISTS alarms (
    id              TEXT PRIMARY KEY,
    device_id       TEXT NOT NULL,
    device_name     TEXT DEFAULT NULL,
    property_id     TEXT DEFAULT NULL,
    property_name   TEXT DEFAULT NULL,
    rule_id         TEXT DEFAULT NULL,
    rule_name       TEXT DEFAULT NULL,
    alarm_type      TEXT NOT NULL,          -- device_alarm / property_alarm
    alarm_level     TEXT NOT NULL,          -- Info / Warning / Error / Critical
    message         TEXT NOT NULL,
    alarm_value     TEXT DEFAULT NULL,
    threshold_value TEXT DEFAULT NULL,
    alarm_time      TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'Active',
    is_acknowledged INTEGER DEFAULT 0,
    acknowledged_by TEXT DEFAULT NULL,
    acknowledged_at TEXT DEFAULT NULL,
    acknowledged_note TEXT DEFAULT NULL,
    is_resolved     INTEGER DEFAULT 0,
    resolved_by     TEXT DEFAULT NULL,
    resolved_at     TEXT DEFAULT NULL,
    resolved_note   TEXT DEFAULT NULL,
    resolution_type TEXT DEFAULT NULL,      -- Fixed / FalseAlarm / Ignored / AutoResolved
    suppress_until  TEXT DEFAULT NULL,
    escalation_count INTEGER DEFAULT 0,
    workspace_id    TEXT DEFAULT NULL,
    created_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_alarms_device_id ON alarms(device_id);
CREATE INDEX IF NOT EXISTS idx_alarms_status ON alarms(status);
CREATE INDEX IF NOT EXISTS idx_alarms_level ON alarms(alarm_level);
CREATE INDEX IF NOT EXISTS idx_alarms_time ON alarms(alarm_time DESC);
```

### 关键设计决策

- `condition` 和 `notification_config` 存 JSON 字符串，直接对应前端 TypeScript 类型，避免过度规范化
- `alarm_time`（告警发生的业务时间）与 `created_at`（记录创建时间）分离
- `suppress_until` 记录抑制截止时间，`escalation_count` 限制升级次数（最多 3 次）
- `workspace_id` 可选，支持多工作空间隔离

---

## 模块 2: 存储层

位于 `crates/tinyiothub-storage/src/sqlite/`，沿用 `notification_channel.rs` 的 `QueryBuilder` 动态查询模式。

### alarm_rule.rs

| 函数 | 说明 |
|---|---|
| `find_alarm_rule_by_id(db, id, ws_id)` | 根据 ID 查询，可选 workspace 过滤 |
| `find_all_alarm_rules(db, params)` | 分页+筛选查询（device_id, is_enabled, rule_type） |
| `count_alarm_rules(db, params)` | 统计总数 |
| `create_alarm_rule(db, req, ws_id)` | 创建规则，自动生成 ID 和时间戳 |
| `update_alarm_rule(db, id, req, ws_id)` | 部分更新（仅更新传入字段） |
| `delete_alarm_rule(db, id, ws_id)` | 物理删除 |
| `set_alarm_rule_enabled(db, id, enabled)` | 启用/禁用 |
| `find_enabled_rules_for_device(db, device_id)` | 查询设备关联的所有启用规则（评估用） |

### alarm.rs

| 函数 | 说明 |
|---|---|
| `find_alarm_by_id(db, id, ws_id)` | 单个告警查询 |
| `find_alarms(db, params)` | 分页+筛选（statuses, levels, device_ids, 时间范围） |
| `count_alarms(db, params)` | 统计总数 |
| `create_alarm(db, alarm)` | 插入告警记录 |
| `acknowledge_alarm(db, id, by, note)` | 确认告警，更新状态为 Acknowledged |
| `resolve_alarm(db, id, by, type, note)` | 解决告警，更新状态为 Resolved |
| `batch_acknowledge(db, ids, by)` | 批量确认 |
| `batch_resolve(db, ids, by, type)` | 批量解决 |
| `get_alarm_statistics(db, ws_id)` | 统计：total/active/acknowledged/resolved |
| `find_active_alarm_for_property(db, device_id, property_id)` | 查找活跃告警（抑制逻辑用） |
| `find_escalation_candidates(db, delay_secs)` | 查找待升级告警 |

### Repository Trait

在 `crates/tinyiothub-core/src/repository/` 中新增 `AlarmRepository` 和 `AlarmRuleRepository` async trait，遵循 `DeviceRepository` 的模式。

---

## 模块 3: 规则评估引擎

### 触发机制（经 eng review 纠正）

~~设备属性写入成功后~~ **纠正：** 属性值通过 `data_server.rs` 的轮询循环流入，不经过 SQLite 存储层写入。触发点位于 `crates/tinyiothub-runtime/src/data_server.rs:136` 的 `collect_property_change_events` 之后。

`AlarmRuleEngine` 作为 EventBus 订阅者，监听 `PropertyChange` 事件。收到事件后 `tokio::spawn` 异步评估，不阻塞 DataServer 轮询循环。EventBus 基础设施已存在于 `crates/tinyiothub-runtime/src/event_bus.rs`。

### 新增文件: `crates/tinyiothub-core/src/rule/alarm.rs`

```rust
pub struct AlarmRuleEngine {
    evaluator: RuleEvaluator,
}
```

### 核心方法

- `evaluate_rule(rule, data) -> Result<bool>` — 评估单条规则
- `build_conditions(property_id, condition_json) -> Vec<CompositeCondition>` — 将前端 AlarmCondition JSON 转为可评估条件树
- `build_alarm_message(rule, current_value) -> String` — 生成中文告警消息

### 条件类型评估

| 类型 | 评估逻辑 |
|---|---|
| threshold | 单字段比较：`property_id operator value`（直接调用 existing RuleEvaluator） |
| range | min ≤ value AND value ≤ max（拆为两个 RuleCondition，AND 组合） |
| change | 获取上一次属性值，计算变化量，判断变化方向和阈值 |
| duration | 条件持续满足超过指定秒数（需查历史数据） |
| composite | 递归评估子条件，按 operator（and/or/not）组合结果 |

### 告警生成流程

```
evaluate_rule 返回 true
    ↓
find_active_alarm_for_property(device_id, property_id)
    ↓
存在活跃告警 且 在 suppress_duration_secs 内？
    ├── 是 → 抑制，不创建
    └── 否 → 构建 AlarmDto → create_alarm() → dispatch_notifications()
```

### 自动恢复

规则评估返回 false 后，查找该 device_id + rule_id 的活跃告警，若存在则自动解决（`resolution_type = "AutoResolved"`）。

---

## 模块 4: API 处理器

位于 `crates/tinyiothub-web/src/handlers/`。

### alarm.rs — 告警端点

| 方法 | 路径 | 功能 |
|---|---|---|
| GET | `/api/alarms` | 分页查询（Query: page, pageSize, statuses, levels, deviceIds, startTime, endTime） |
| GET | `/api/alarms/statistics` | 告警统计 |
| GET | `/api/alarms/:id` | 单个告警详情 |
| PUT | `/api/alarms/:id/acknowledge` | 确认告警（Body: `{ note? }`），当前用户从 auth 上下文提取 |
| PUT | `/api/alarms/:id/resolve` | 解决告警（Body: `{ resolutionType, note? }`） |
| POST | `/api/alarms/batch/acknowledge` | 批量确认（Body: `{ alarmIds[] }`） |
| POST | `/api/alarms/batch/resolve` | 批量解决（Body: `{ alarmIds[], resolutionType }`） |

### alarm_rule.rs — 告警规则端点

| 方法 | 路径 | 功能 |
|---|---|---|
| GET | `/api/alarm-rules` | 分页查询（Query: page, pageSize） |
| GET | `/api/alarm-rules/:id` | 单个规则详情 |
| POST | `/api/alarm-rules` | 创建规则（Body: `CreateAlarmRuleRequest`） |
| PUT | `/api/alarm-rules/:id` | 更新规则（Body: `UpdateAlarmRuleRequest`） |
| DELETE | `/api/alarm-rules/:id` | 删除规则 |
| PUT | `/api/alarm-rules/:id/toggle` | 切换启用/禁用 |

### 返回格式

沿用项目统一 `ApiResponse<T>` 格式。分页响应包含 `pagination: { page, pageSize, totalPages, totalCount }`。

### 鉴权

所有端点通过 auth middleware 校验，从 JWT/Token 提取当前用户信息。workspace_id 从请求上下文注入。

---

## 模块 5: 通知分发

### 新增文件: `crates/tinyiothub-core/src/notification/mod.rs`

```rust
pub struct NotificationDispatcher;

impl NotificationDispatcher {
    pub async fn dispatch(
        db: &Database,
        alarm: &AlarmDto,
        rule: &AlarmRuleDto,
    ) -> Result<Vec<NotificationResult>, String>;
}
```

### 分发流程

```
解析 rule.notification_config (JSON → NotificationConfig)
    ↓
enabled == false？ → 跳过
    ↓
遍历 config.channels (Email/Sms/Webhook/Sse)
    ↓
查询 notification_channels 表中对应类型的启用渠道
    ↓
限流检查（repeat_interval / suppress_duration）
    ↓
按渠道类型发送
```

### 通知内容

- 标题: `[{alarm_level}] {alarm_message}`
- 正文: 设备名、属性名、当前值、阈值、时间
- 渠道特定格式化

### 渠道实现

| 渠道 | 实现方式 |
|---|---|
| Email | 解析 channel.config 获取 SMTP 配置，发送邮件 |
| Sms | 解析 channel.config 调用短信网关 API |
| Webhook | HTTP POST JSON 到配置的 URL |
| Sse | 推送到 SSE 流供前端消费 |

### 错误处理

单个渠道发送失败不影响其他渠道，记录错误日志，返回每个渠道的发送结果。

---

## 模块 6: 高级功能

### 6.1 告警抑制

- 规则触发时，若同一 device_id + rule_id 已有活跃告警且在 `suppress_duration_secs` 内，不创建新告警
- `suppress_until` 字段记录抑制截止时间

### 6.2 告警升级

- 作为可选的定时扫描任务（可集成到现有 cron 框架）
- 查找 `status = 'Acknowledged'` 且 `acknowledged_at + escalation_delay_secs < now` 的告警
- 升级 `alarm_level → escalation_level`，重新触发通知，`escalation_count += 1`
- 最多升级 3 次

### 6.3 批量操作

- 批量确认/解决：一次操作处理多个告警 ID
- 返回 `BatchOperationResult { successCount, totalCount }`
- 部分失败不影响整体

### 6.4 自动恢复

- 属性值恢复正常（规则条件不再满足）时自动解决对应告警
- `resolution_type = "AutoResolved"`，`resolved_by = "system"`

---

## 数据流总览

```
┌──────────────┐     ┌─────────────────┐     ┌──────────────┐
│ Device Data  │────▶│ AlarmRuleEngine │────▶│ Alarm Store  │
│ (property)   │     │ (evaluate)      │     │ (SQLite)     │
└──────────────┘     └───────┬─────────┘     └──────┬───────┘
                             │                      │
                             │ condition matched     │ alarm created
                             ▼                      ▼
                    ┌────────────────┐    ┌──────────────────┐
                    │ AutoResolve?   │    │ Notification     │
                    │ (recovery)     │    │ Dispatcher       │
                    └────────────────┘    └──────┬───────┘
                                                 │
                                    ┌────────────┼────────────┐
                                    ▼            ▼            ▼
                                  Email        Sms        Webhook/SSE
```

## 文件变更清单

| Crate | 文件 | 操作 |
|---|---|---|
| `cloud/migrations` | `20260606000001_create_alarm_tables.sql` | 新增 |
| `tinyiothub-storage/src/sqlite/` | `alarm_rule.rs` | 新增 |
| `tinyiothub-storage/src/sqlite/` | `alarm.rs` | 新增 |
| `tinyiothub-storage/src/sqlite/` | `mod.rs` | 修改 (+2 mod) |
| `tinyiothub-core/src/rule/` | `alarm.rs` | 新增 |
| `tinyiothub-core/src/rule/` | `mod.rs` | 修改 (+1 mod) |
| `tinyiothub-core/src/notification/` | `mod.rs` | 新增 |
| `tinyiothub-core/src/repository/` | `alarm.rs` | 新增 |
| `tinyiothub-core/src/repository/` | `mod.rs` | 修改 (+2 mod) |
| `tinyiothub-web/src/handlers/` | `alarm.rs` | 新增 |
| `tinyiothub-web/src/handlers/` | `alarm_rule.rs` | 新增 |
| `tinyiothub-web/src/handlers/` | `mod.rs` | 修改 (+2 mod) |

**前端无需改动**，API 契约与已有前端代码完全匹配。

---

## 审查补充（来自 /plan-ceo-review，2026-06-06）

以下设计决策在 HOLD SCOPE 审查中确认，需纳入实现：

### D3: tokio::spawn panic 保护
规则评估的 `tokio::spawn` 需包裹 `std::panic::catch_unwind` + `AssertUnwindSafe`。Panic 时记录完整 backtrace + device_id/property_id 上下文。

### D4: 振荡传感器告警风暴防护
`AlarmRuleEngine` 增加内存限流器：`HashMap<(device_id, rule_id), Instant>`。同规则触发后 min(suppress_duration_secs, 60) 秒内不再评估。防止传感器在阈值附近振荡导致告警/恢复循环。

### D5: acknowledge/resolve 前置校验
acknowledge 和 resolve 端点先 SELECT 检查告警存在性 + 当前状态：
- 不存在 → 404 "告警不存在"
- 已确认 → 409 "告警已确认，无需重复操作"
- 已解决 → 409 "已解决的告警无法确认"

### D6: 批量操作大小限制
`batch_acknowledge` / `batch_resolve` 限制 `alarmIds` 最多 100 条。空数组返回 400。超过上限返回 400 "单次批量操作最多 100 条"。

### D7: 结构化日志
关键路径增加 `tracing::info!` 结构化字段：`alarm_evaluation`（rule_id, device_id, result）、`alarm_created`（alarm_id, device_id, level）、`notification_sent`（channel, success）。

### 安全补充
acknowledge/resolve 的存储函数需传入 `workspace_id` 参数，WHERE 子句包含 workspace 过滤，防止跨工作空间操作。

### 错误与救援注册表

| 方法 | 失败模式 | 异常类型 | 已救援？ | 用户看到 |
|---|---|---|---|---|
| find_enabled_rules_for_device | DB 连接池耗尽 | PoolExhausted | N → 需加 | 500 + 日志 |
| evaluate_rule | condition JSON 解析失败 | serde_json::Error | Y (跳过) | N/A |
| evaluate_rule | 非数值比较 | String error | Y (跳过) | N/A |
| create_alarm | DB 主键冲突 | ConstraintViolation | N → 需加 | 500 + 日志 |
| acknowledge_alarm | 告警不存在 | RowNotFound | Y (前置检查) | 404 |
| acknowledge_alarm | 告警已确认 | AlreadyAcknowledged | Y (前置检查) | 409 |
| resolve_alarm | 告警不存在 | RowNotFound | Y (前置检查) | 404 |
| resolve_alarm | 告警已解决 | AlreadyResolved | Y (前置检查) | 409 |
| batch_acknowledge | 部分 ID 无效 | PartialFailure | Y (计数) | successCount/totalCount |
| dispatch (webhook) | HTTP 超时 | TimeoutError | Y (per-channel) | 该渠道记录失败 |
| dispatch (email) | SMTP 认证失败 | AuthError | Y (per-channel) | 该渠道记录失败 |
| dispatch (sse) | 连接断开 | ConnectionError | Y (per-channel) | 该渠道记录失败 |

### 故障模式注册表

| 代码路径 | 故障模式 | 已救援？ | 已测试？ | 用户感知？ | 已记录？ |
|---|---|---|---|---|---|
| RuleEngine::evaluate | Panic in evaluation | Y (catch_unwind) | 待测 | N (日志) | Y |
| RuleEngine::evaluate | Oscillation storm | Y (throttle) | 待测 | N (抑制) | Y |
| API: acknowledge | 重复确认 | Y (409) | 待测 | Y (错误消息) | N/A |
| API: batch | 空 ID 列表 | Y (400) | 待测 | Y (错误消息) | N/A |
| API: batch | 超过 100 条 | Y (400) | 待测 | Y (错误消息) | N/A |
| Notification: dispatch | 部分渠道失败 | Y (per-channel) | 待测 | N (日志) | Y |
| Storage: create_alarm | DB 锁竞争 | N (SQLite 内部) | 待测 | N (静默) | N — GAP |

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 1 | CLEAR | 7 issues, 6 fixed, 1 accepted risk |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | CLEAR | 3 issues: EventBus integration (fixed), test strategy (core paths), retention TODO |
| Codex Review | — | — | 0 | — | — |
| Design Review | — | — | 0 | — | — |
| DX Review | — | — | 0 | — | — |

### Eng Review Amendments
- **D1:** 触发点从 SQLite 存储层纠正为 `data_server.rs` → EventBus 订阅模式
- **D2:** 测试策略：核心路径（~25-30 测试），跳过 duration/change 条件
- **D3:** 告警保留策略添加到 TODOS.md（90 天自动清理）
- **架构图已纠正:** DataServer → EventBus → AlarmRuleEngine → Storage → Notification

**VERDICT: CEO + ENG CLEARED — ready to implement.**
