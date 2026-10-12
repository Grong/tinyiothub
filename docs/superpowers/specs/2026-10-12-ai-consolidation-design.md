# AI 子系统全链路梳理设计（概念统一 + 内核归一 + 前端单入口）

- 日期：2026-10-12
- 分支：另起 ai-consolidation 分支实施（CEO plan 头部统一为此）
- 状态：四节设计已与作者逐节确认（概念模型/内核归一/前端 AI 中心/迁移测试策略）；spec 评审第 1 轮 18 条发现已全部落回本文（4 条作者裁定 D8-D11 + 14 条直接修）

## 背景与问题

AI 相关功能是三次需求各自长出来的（心跳线 / ai-brain-p0 处置中心线 / chat 线），无人站在"用户去哪办这件事"统一过。实证（本会话盘点）：

**前端 4 个 AI 入口**：`#/dispositions`（处置中心）、`#/ai-ops`（AI 运维）、`#/agents`（工作区设定/权限/技能/心跳 4 tab）、`#/chat`（对话）。

**dispositions 与 ai-ops 的功能重合**（作者确认，梳理的直接证据）：

| 能力 | dispositions | ai-ops |
|---|---|---|
| 待审批 | 「需要你」tab 待审批判断的批准/拒绝 | 「待审批」区提案的批准/拒绝——同一动作两个入口 |
| 巡检结果 | 「巡检」tab 提案行+tick 聚合行 | 「执行历史」heartbeat 时间线——同一批 tick 两种形态 |
| 统计摘要 | 头部统计条（今日消化/需要你/延迟 p50/反馈） | 「今日摘要」卡——两个统计条 |
| 独有 | 判断反馈 ✓/✕、run 详情链接 | 信任配置、知识/记忆、心跳任务配置 |

**后端**：三条触发链 + 双 runner（调查走 thing_agent Runner 产 RunReport 落 agent_runs；心跳走独立 HeartbeatRunner 产 HeartbeatResult 落 agent_actions）；host/ 20+ 模块平铺；chat 在 domains/agent/ 与 host/ 各一处。

**已定方向（与作者确认）**：全链路一起理；驱动动机=用户侧清晰一致优先；允许破 URL 结构（数据不丢）；借鉴 Claude Code/Codex——以"任务/run"为组织单位而非功能页面、权限分级作为一等 UX、过程叙事嵌入主流（详见下文方案修正）。

## 目标

- 用户侧：运维向单一 AI 中心入口（chat 独立形态除外），概念一致，审批只有一个地方，巡检结果只有一个地方，统计只有一条；
- 内核侧：一种 Run、一种轨迹、一份报告——双 runner 归一；
- 可理解性：概念模型单一事实源文档，新人半天上手。

## 设计

### 1. 概念模型（词汇表 + 概念图）

核心概念链：

```
触发（Trigger）──► 运行（Run）──► 判断（Verdict）──► 处置（Disposition）
  告警/定时/指令      一次 AI 工作单元      noise/self_healable/     自行闭环/待审批/
                     （trace+summary+        needs_human            转工单
                      narrative）
```

词汇表（收编异名）：

| 概念 | 定义 | 收编的异名 |
|---|---|---|
| Run | 一次 AI 工作单元（trace+summary） | 调查 run / 心跳 tick / 指令触发的 run（UserDirective；chat 消息不落 agent_runs，chat 是独立形态） |
| Verdict | Run 的结构化结论 | verdict / outcome / status |
| Proposal | 需人批准的动作建议 | proposal / pending action / 待审批 |
| Disposition | 判断的处置结果 | disposition / judgment 状态机 |
| Trace | Run 的逐步工具轨迹 | trace / 证据 / evidence / 执行历史 |
| Trust Mode | 工作区自主级别（一等 UX，统一循环后门控全部 run——D9 裁定的行为变更） | trust_level（埋在配置页） |
| Narrative | Run 结论的大白话展示形态（D8 裁定：=summary 的展示形态，RunReport 不增字段） | 调查叙事 |

归一依据：T1/T9 已打通双 runner 的轨迹类型（ToolTraceEntry 共享），RunStarted/registry/trace 端点已按"run 是一等工作单元"落地——概念模型是对已有演化的命名收口，不是新发明。

产出物：`docs/ai-concept-model.md`——概念图 + 词汇表 + 概念→代码/表映射。

### 2. 内核归一（统一 Run 模型 + host 模块分组）

```
                 ┌─── WakeSignal（告警/指令）─────────┐
Trigger ────────►│                                    ├─► 统一 Run Loop ──► RunReport ──► agent_runs
                 └─── 定时（心跳=周期 WakeSignal）─────┘    （一种 run、一种轨迹、一份报告）
```

- HeartbeatRunner 归并进 thing_agent Run 循环：心跳 tick = `TriggerSource::Heartbeat` **新变体**驱动的 run（D11 裁定：专用变体+专用 dedup key `heartbeat:{ws}`——复用 TimerTrigger 会丢每工作区可配间隔且与 `timer:{ws}` dedup 碰撞吞 tick）。HeartbeatResult 退役为 RunReport 的读侧兼容视角（适配层=handler 侧纯函数 RunReport→ExecutionRecord；历史无 trace 行前端显示"无过程记录"不炸）；
- 不做（YAGNI）：不动自治策略门、调度器合并、判断流水线——已是统一通道。归一只合并"执行循环+结果形态"这一层；
- 落库归一：tick trace 从 agent_actions anchor content 迁至 agent_runs.report（迁移回填可回填的；历史无 trace 不补）。提案/审批仍留 agent_actions——它们是动作不是 run；**提案行 tick_id 迁移时一并改写为 run_id**（C3：联结不断链），读侧兼容期保留 tick_id→run_id 映射视图，切换完成后删。**耐久性对齐（D10）**：RunRecorded 补心跳同款重试+DLQ（persist.rs 现成模式）——归一不得静默降级可靠性；
- 告警触发链（HeartbeatSignal 带优先级+dedup）本期保留入口、内部转 TriggerSource::Heartbeat 变体（优先级映射：Critical→Critical，High/Normal→Normal）；

**行为保持清单**（C1，每项=行为/归一后归宿/验证）：

| 既有行为 | 归一后归宿 | 验证 |
|---|---|---|
| 连续失败 5 次自动暂停 loop（loop_.rs:81-105） | 统一循环同等语义：连败达阈暂停该工作区触发 | mock 池连败 5 次断言暂停 |
| last_tick D13 实时读（runner 内存态） | 统一循环内存态同字段透出 | handler 测试不变 |
| 任务/配置热更新（ReloadTasks/ReloadConfig 写穿） | 统一循环信号通道承接 | 热更新测试 |
| 每工作区可配间隔（interval_minutes） | Heartbeat 变体 per-workspace 间隔驱动 | 间隔配置测试 |

**信任门控作用域（D9 裁定的行为变更）**：统一循环后所有 run 过信任门（调查/指令也受约束）——一等 UX 言行一致。行为保持清单的显式例外（唯一故意行为变更），迁移期告警路径先行验证。

host/ 模块分组（20+ 平铺 → 按概念归位）：

```
host/
  judge/      ← judgment_subscriber + policy_engine
  notify/     ← ticket_subscriber + 通知装配
  persist/    ← persist.rs + dlq_repo + memory
  trigger/    ← directive_sink + thing_action_hooks + agent_hooks
  chat/       ← domains/agent/chat 收进来（目前两处，并掉一处）
  tools/      ← 不动（已是边界）
  host 壳     ← thing_agent_host + service + scaffold + pool_adapter
```

每组 mod.rs 写三行契约（做什么/谁调用/依赖谁）。

### 3. 前端 AI 中心（run 流为组织单位 + 信任分级前置）

单一入口 `#/ai`（旧 3 入口废弃+临时重定向一个版本；chat 为独立形态不在本期统一范围——K1 修正）：

```
#/ai  AI 中心
├─ 顶栏：信任模式选择器（需要你批准 / 只读自动 / 全自动）常驻一等 UX；右侧配置抽屉
├─ 「AI 今日自述」（E1）：Run 流顶部 LLM 每日一句——"我今天做了什么"。输入=当日 run 的 summary 集合（D8 裁定，RunReport 不增字段）；时机=页面加载时聚合一次、结果按日缓存（当日不变不重跑）；
├─ 主视图：Run 流（时间倒序统一流——调查/巡检/指令混排，每条=卡片+可展开时间线，
│  来源徽章区分三类 trigger；卡片+时间线组件直接复用现状）
│  · 筛选+搜索（E4）：按 trigger 类型/结果/关键词过滤
│  · SSE 实时插入新 run（E6：新事件类型 run_recorded 推内容而非只推刷新信号）
├─ 需要你（待审批提案+待确认判断——审批唯一入口）
└─ 配置（信任细节/工具权限/技能/心跳任务——agents 4 tab 归位）
```

信任模式切换（E2）：切换历史记录（谁/何时/从何改到何）+ 切换确认弹窗（提示影响面：当前在途 run 是否受影响）。存储：新表 `trust_config_history`（Q5 修正——不复用 brain_event；与 persist.rs 的 fencing upsert 语义对齐）。

详情页反馈入口（E5）：trace 步骤/结论的点踩直接进 judgment_feedback（复用既有反馈表，详情页挂入口——反馈闭环最后一厘米）。

关键转变：组织单位从功能页面换成 run；过程叙事嵌在流里，不做独立日志页；执行历史不再单独存在——它是 Run 流的巡检段。

归并映射：

| 旧入口 | 去向 |
|---|---|
| #/dispositions | → #/ai 主视图 Run 流 |
| #/ai-ops | → 摘要进顶栏统计条；审批进「需要你」；历史并入 Run 流；信任进顶栏+配置 |
| #/agents | → 配置抽屉/分区 |
| #/chat | 不动（对话是独立形态，本期边界） |
| #/ai-ops/runs/{id} | → #/ai/runs/{id}（路由改名，内容不动） |

不做：不改卡片/时间线组件；不动 SSE/轮询通道；不动 chat。

### 4. 迁移/兼容/测试策略

三步各自独立可交付、可回滚：

1. **Phase 1 概念文档**（纯 docs 零风险）：docs/ai-concept-model.md 先出——后两 phase 的评审校准尺。
2. **Phase 2 内核归一**（行为保持重构）：护栏=分类后的现有套件全绿（K3 修正：落库改向后直接断言 agent_actions 内容的测试——crates/db/src/heartbeat.rs 锚行/提案共存、agent_persist_tests、heartbeat_bridge_tests——属"实现细节重写"类，先分类再改）+ 每步独立 commit；迁移只做 trace 回填+读侧切换；心跳并入后补「等价对照测试」（Q1/F2 修正：**shadow 双跑**——心跳路径照常执行，统一 Run 路径在 mock 池接缝上只产报告不落库/不落提案，比对字段白名单=status/summary/proposals 集合/trace 工具名序列；比对期 N=7 天或 M=50 次 tick 后切换）。
   - **概念模型 lint（E3）**：CI 脚本扫描 `web/src` 文案 + `apps/cloud/src/domains/agent` 注释（Q4 修正——限定范围防误报），异名清单即词汇表右列；首版 warn-only 一个版本后转 fail。
3. **Phase 3 前端 AI 中心**（最后做）：#/ai 落地，旧 URL 保留一个版本的 hash 重定向（#/dispositions→#/ai 等）防书签/历史链接 404，下版本删。

兼容：数据零丢失；API 端点 phase 3 期间保留（AI 中心内部复用），切换完成后标 deprecated，再下版本删。

测试：Phase 2 行为保持=分类后套件即回归网；Phase 3 复用 dispositions/ai-ops 既有测试搬家（行为不变只搬家，含站内路由引用改写——F3：runs 详情链接从 #/ai-ops/runs 改 #/ai/runs）+ 重定向测试；每 phase 收尾跑全量门禁。

## 非目标（YAGNI）

- chat 链路重构（独立形态，本期不动）；
- OpenClaw 接入（后续扩展，本梳理的"内核归一"是它的前置地基）；
- 自治策略门/调度器/判断流水线重构（已是统一通道）；
- sms/email 通知通道（仍打桩）。
