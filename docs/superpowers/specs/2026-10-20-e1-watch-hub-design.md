# E1 值守中枢子 spec（Watch 引擎 + LiveState，E1a+b）

- 日期：2026-10-20
- 分支：ai-consolidation
- 上位文档：`docs/superpowers/specs/2026-10-20-hub-runtime-architecture-design.md`（架构原则 P1-P6、E1-E5 阶段、CEO/eng 评审裁定）
- 范围：E1a（引擎内核）+ E1b（派生摘要）。E1c（GoalWatch）与 E1d（知觉权）各自另立 spec（2026-10-20 brainstorm 裁定：两者依赖未调研领域，不进本期）
- 状态：设计五节已与作者逐节确认（组件边界/数据流/scheduler 改造/持久化与错误处理/测试）

## 0. 承接收尾（eng 评审裁定落地清单）

| 裁定 | 内容 | 本 spec 落点 |
|---|---|---|
| R1（D3/D4） | 废除 30s 合并窗 + 同键速率限制替代 | §3 |
| R2（D5） | LiveState Lagged 检测 + device_cache 全量重同步 | §2.2 |
| F3 | runtime 持久化必须走 ports.rs port trait | §4.1 |
| F4 | 账本**先不限速**（用户裁定 2026-10-20）：状态变更全量落账，接受体积风险换简单；**观测兜底=账本行数/体积日增指标+阈值告警** | §4.2 |
| W1 | watch 唤醒不再需要特例通道（合并窗已废，即来即醒） | §3 |

## 1. 组件与边界

```
crates/runtime/src/
  watch/
    registry.rs   条件词表注册表（v1 三类比较器）
    engine.rs     WatchEngine：订阅 EventBus → LiveState 更新 → 命中检测 → 发 WatchHit
    live_state.rs LiveState + health/zone 派生摘要
    ledger.rs     账本写入（经 ports.rs 新 port trait）
crates/agent/src/runtime/thing_agent/trigger/
    watch.rs      WatchTrigger：WatchHit → WakeSignal{TriggerSource::Watch}
crates/db/
    watch_rules 表 + hub_ledger 表（migration）
apps/cloud/
    watch API（注册/列表/删除）+ 装配接线（ports 实现注入）
```

边界规则：

1. 评估器在 `crates/runtime`（纯逻辑、零 sqlx；持久化走 ports）；唤醒适配器在 `crates/agent`（Trigger 接口实现）。
2. LiveState 是 AI 侧唯一读面；device_cache 仍是驱动侧写面，`cache_and_publish`（cache-before-publish）不变量不动。
3. 持久源 = `watch_rules` 表；内存评估态 = 易失投影；启动时全量重织（幂等 upsert）。
4. `TriggerSource::Watch` 新变体携带命中上下文：规则 id、条件、当前值、health/zone 摘要。**调度语义上它是普通信号**（合并窗已废，无窗可绕——R1），变体存在的理由是上下文携带与 dedup key 语义（`watch:{rule_id}`），不是优先级特例。
5. 本期不做快照注入（E2）；被唤醒的 run 用现有工具读 LiveState 摘要 API。

## 2. 数据流

### 2.1 注册与重织

```
API 注册 → 校验（词表 schema + 引用存在性，fail-loud 400）
        → watch_rules 表 INSERT（持久源）
        → 内存注册表 upsert（幂等）
启动时：watch_rules 全量 → 重织内存注册表
删除：API → 表 DELETE → 注册表移除；设备删除 → 活跃 watch 失效事件落账 + 通知注册者
```

### 2.2 值守链路

```
DataServer cache_and_publish → EventBus(broadcast 1000)
  → LiveState 更新
      │ Lagged 检测（broadcast::RecvError::Lagged）
      │   → device_cache 全量重同步（去抖 1s）→ health 空洞期标 unknown，重同步后恢复
  → WatchEngine 求值（按字段倒排索引；O(命中字段的 watch 数)；纯函数比较器）
  → 命中判定 = only-new-data：注册时即成立的条件下，下一次满足条件的变化才命中
  → 命中后：① hub_ledger 落一行 ② WatchHit → WatchTrigger → WakeSignal{Watch}
  → scheduler（§3）→ run_pipeline → run 读 LiveState 摘要
```

one-shot：命中后 watch 从注册表移除（临时 watch）或保持（持久规则型 watch 由 `watch_rules.persistent` 标记——v1 两类：one-shot 临时 watch（API 注册默认）与 persistent 规则）。同一 watch 1s 内多次命中合并为一次唤醒。

### 2.3 watch 条件词表 v1（注册表可扩）

| 类型 | 条件 | 命中时机 |
|---|---|---|
| `property` | `{thing_id, property, op: lt/gt/eq/ne, value}`（派生属性同为 property） | 属性值变化且比较成立 |
| `health` | `{thing_id, level: attention/bad}` | LiveState 派生 health 级别变为该级 |
| `availability` | `{thing_id, online: true/false}` | 设备上下线切换 |

每工作区活跃 watch ≤20；超时默认 600s、上限 3600s，超时落 `matched:false` 事件。

## 3. scheduler 改造（R1 落地）

```
现状：WakeSignal → merger task(30s 窗, 同 dedup_key 聚合) → 配额(20/h) → ready 队列(50)
目标：WakeSignal → 限速器(同键 N/min) → 配额(20/h 不变) → ready 队列(50)
```

- merger task 整体退役；`MERGE_WINDOW` 常量删除；`TriggerSource::Merged` 变体不再产生（枚举保留，防存储/回放兼容破坏）。
- 限速器：每 workspace 每 dedup_key 每分钟最多 N 次唤醒；默认 N=3；`workspaces.heartbeat_config` JSON 加键 `rate_limit_per_key_per_min`（复用配置先例，热改生效）；超出丢弃 + metric `agent_wake_rate_limited` + hub_ledger 落一条限速事件（被限的 flap 必须可见，防静默）。
- 保持不变的既有语义（回归锁）：Critical 绕过限速与预算；用户指令永不节流（60s 同文本去重保留）；队列容量 50；每工作区串行执行。
- 显式行为变更（D3/D4 已裁定）：心跳/事件信号失去 30s 聚合，每个信号独立唤醒。
- 回滚：限速器开关 + 恢复 merger（git revert 粒度）；feature flag 复用 mode 门控。

## 4. 持久化与错误处理

### 4.1 ports（F3）

`crates/runtime/src/ports.rs` 新增两个 port trait（apps/cloud 注入 db 实现）：

- `WatchRuleStore`：`list_all() / insert(rule) / delete(id)`（重织与注册路径）
- `HubLedger`：`append(entry)`（账本写入；entry 含 kind 词表版本号——P5）

### 4.2 账本（F4：先不限速）

新表 `hub_ledger`：`id, workspace_id, ts, kind, thing_id, payload(json)`；`kind` 词表版本化。
落账内容：状态变更（全量）/ watch 注册·命中·超时·失效 / job 生命周期（accepted/progress/result/rejected）/ 限速事件。
观测兜底：`hub_ledger` 日增行数与字节数进指标，日增超阈值（默认 100k 行，可配）告警——先不限速不等于瞎。

### 4.3 错误处理矩阵

| 失败 | 处理 | 用户可见 |
|---|---|---|
| watch 注册 schema 非法 / 引用不存在 | API 400 + 原因 | 是 |
| 设备删除时有活跃 watch | 失效事件落账 + 通知注册者 | 是 |
| EventBus Lagged | LiveState 全量重同步（§2.2） | health=unknown 期间可见 |
| 唤醒投递失败 | 重试 + DLQ（复用 persist.rs 模式） | 指标 |
| 账本写失败 | 重试 3 次 → DLQ → metric；**值守链路不因账本故障中断**（账本可补，唤醒不能丢） | 指标 |
| 摘要装配异常 | 降级为空摘要，run 不炸 | 否 |
| watch 超 20 上限 | 注册拒绝 + 原因 | 是 |

## 5. 测试策略

- 比较器：纯函数表驱动单测（三类词表 × lt/gt/eq/ne × 边界：缺失字段/类型不符/NaN）
- LiveState：Lagged 注入→重同步后摘要一致（集成）；health 派生规则单测（worst-first）
- 限速器：同键计数边界（第 N 次过/第 N+1 次拒）、跨分钟重置、Critical 绕过
- 端到端：模拟驱动上报→watch 命中→run 触发（<2s）；重启→重织→值守继续
- 仲裁退化形态（每设备默认单一互斥资源）：两 run 并发同设备→后者 busy 带持有者
- 回归：scheduler 合并窗用例重写为限速语义；保留行为逐项有锁（见 §3 清单）
- eval：本期无 prompt 变更，evals 不动

## 6. 退出标准

一个真实场景端到端：注册 watch（温度>30）→ 模拟驱动越限 → 秒级唤醒 run → run 读到含 zone 归属的摘要 → 账本可回放该场景全程；且：两 run 并发操作同设备时后者收到带持有者的 busy 拒绝；重启后 watch 网由 watch_rules 重织、值守无需人工介入继续。

## 7. 非目标（YAGNI）

- GoalWatch 目标编译/目标表（E1c 另立 spec）
- 知觉权/遥测异常检测（E1d 另立 spec，依赖算法选型调研）
- 快照注入 prompt（E2）
- 账本限速/聚合行（F4 已裁定先不限，指标告警兜底）
- 表达式条件词表（留注册表第一次真实扩展）
- Devices 工作台前端（E3）
