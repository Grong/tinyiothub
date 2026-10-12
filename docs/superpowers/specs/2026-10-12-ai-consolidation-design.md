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
                     （trace+summary；         needs_human            转工单
                      narrative=summary 的
                      展示形态）
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
- 告警触发链（HeartbeatSignal 带优先级+dedup）本期保留入口、内部转 TriggerSource::Heartbeat 变体（优先级映射：Critical→Critical，High/Normal→Normal）；**告警 run 的 dedup key 保留原语义维度：`heartbeat:{ws}:{thing_id}:{alarm_type}`，与定时 tick（`heartbeat:{ws}`）不互合并**（N3——Critical 告警不得被在途 tick 吞）；

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

每组 mod.rs 写三行契约（做什么/谁调用/依赖谁）。**RunRegistry（记忆注入源）与 ActiveRunRegistry（实时 trace 源）是两个职责不同的注册表，归一时不合并、注明关系**（eng S1-P3）。

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
2. **Phase 2 内核归一**（行为保持重构）：护栏=分类后的现有套件全绿（K3 修正：落库改向后直接断言 agent_actions 内容的测试——crates/db/src/heartbeat.rs 锚行/提案共存、agent_persist_tests、heartbeat_bridge_tests——属"实现细节重写"类，先分类再改）+ 每步独立 commit；迁移只做 trace 回填+提案 tick_id 改写+读侧切换（N4）；心跳并入后补「等价对照测试」（Q1/F2 修正：**shadow 双跑**——比对 harness 内两路径共用 mock 池接缝（确定性接缝），心跳逻辑照常执行，统一 Run 路径只产报告不落库/不落提案，比对字段白名单=status/summary/proposals 集合/trace 工具名序列；比对期 N=7 天或 M=50 次 tick 后切换）。
   - **概念模型 lint（E3）**：CI 脚本扫描 `web/src` 文案 + `apps/cloud/src/domains/agent` 注释（Q4 修正——限定范围防误报），异名清单即词汇表右列；首版 warn-only 一个版本后转 fail。
3. **Phase 3 前端 AI 中心**（最后做）：#/ai 落地，旧 URL 保留一个版本的 hash 重定向（#/dispositions→#/ai 等）防书签/历史链接 404，下版本删。

兼容：数据零丢失；API 端点 phase 3 期间保留（AI 中心内部复用），切换完成后标 deprecated，再下版本删。

测试：Phase 2 行为保持=分类后套件即回归网；Phase 3 复用 dispositions/ai-ops 既有测试搬家（行为不变只搬家，含站内路由引用改写——F3：runs 详情链接从 #/ai-ops/runs 改 #/ai/runs）+ 重定向测试；每 phase 收尾跑全量门禁。

## 实现时序要点（CEO 评审 0I）

- **顺序**：Phase 1 概念文档（评审校准尺先行）→ Phase 2 内核归一（行为保持清单逐项先写失败测试）→ Phase 3 前端 AI 中心；
- **埋伏**：① shadow 双跑的 mock 池接缝在 AgentPoolLike 测试接缝处；② HeartbeatSignal→Heartbeat 变体转换点在入口适配层；③ 迁移顺序=先 trace 回填再 tick_id→run_id 改写（否则提案行找不到锚）；④ 测试分类（契约保/细节重写）先行，否则中途被"测试红了"卡住；
- **估计**：human ~1-2 周（三 phase）/ CC+gstack ~2-3 天。

## 非目标（YAGNI）
- chat 链路重构（独立形态，本期不动）；
- OpenClaw 接入（后续扩展，本梳理的"内核归一"是它的前置地基）；
- 自治策略门/调度器/判断流水线重构（已是统一通道）；
- sms/email 通知通道（仍打桩）。

---

# CEO 审查输出（/plan-ceo-review 2026-10-12，SCOPE EXPANSION）

## NOT in scope

延期：无（E1-E6 全收）。拒绝：chat 链路统一（独立形态本期边界）、OpenClaw 接入（后续扩展，内核归一为其地基）、复用 TimerTrigger（D11 否决——丢 per-workspace 间隔+dedup 碰撞）、narrative 增字段（D8 否决——复用 summary）、接受对账窗口不补重试（D10 否决——可靠性不降级）。

## 评审历程

- 第 1 轮对抗性评审（6/10 FAIL，18 条全部对代码核实）：C1 心跳 4 行为无归宿 / C2 落库耐久降级 / C3 tick_id 断链 / C4 narrative 无数据面 / C5 信任语义扩张 / C6 适配层未定义 / K1-K4 矛盾 / Q1-Q5 歧义 / F1-F3 可行性缺口——全部落回 spec；
- 作者裁定 D8-D11（复用 summary / 全部 run 过门 / 补重试+DLQ / 专用变体+键）；
- 第 2 轮复审（8/10 PASS，16/18 全解）：残留 N1-N5（CEO 头部/概念链措辞/迁移措辞/shadow 消歧/告警 dedup key 形态）全部修复。

## 11 节深审结论

架构/错误/安全/数据流/代码质量/测试/性能/可观测/部署/长期轨迹 10 节无新增阻断；第 11 节（设计）IA 级通过，建议实现前对 AI 中心跑一轮 /plan-design-review 专项。详见证录各节。

## 外部声音

codex broken_install（vendor 二进制 ENOENT）；native fallback 两次完成但报告内容丢失（subagent 最终消息丢内容的已知问题）——无可计覆盖，记录在案。实质审查由两轮对抗性 spec 评审+11 节深审承担。

## 实现任务（Phase 维度，实现前由 writing-plans 拆细）

- Phase 1：概念文档 docs/ai-concept-model.md
- Phase 2：内核归一（行为保持清单逐项 TDD + shadow 双跑 + 迁移三件套 + host 分组 + RunRecorded 重试+DLQ）
- Phase 3：前端 AI 中心（#/ai 单入口 + E1/E2/E4/E5/E6 + 归并映射 + 重定向）

## Eng Review — Scope Record（2026-10-12）

- **feature answers**：无新 cut 提案（范围已由 CEO 评审裁定）
- **structure**：**Original arrangement**（eng D1）——三 phase 各为最小可交付单元，不再拆小
- **accepted scope**：spec 全量（三 phase 主方案 + E1-E6），结构按 D1
- **pending remedies**：无
- **Reality rows**：premise-table/already-done/surface-check/binding-decisions 全 pass；numbers=工作量估计值 [estimated]；deferred-asks n/a；pr-count=三 phase 的 PR 划分待实施时定（建议 Phase 2/3 合一或按 ship-policy）

## Eng Review — 评审记录（2026-10-12）

- **Scope Challenge**：B 结构门禁触发（~40+ 文件估计），D1=Original arrangement（三 phase 不再拆小）；C=No issues found。
- **Reality rows**：premise-table/already-done/surface-check/binding-decisions pass；numbers=估计值标注；deferred-asks n/a；pr-count=PR 划分待实施时定（记此备查）。
- **Section 1 架构**：1 发现（P3 RunRegistry vs ActiveRunRegistry 关系注明——已落 spec §2 末行）。**Dispositions**：P3 → accepted（措辞级补充，不改行为）。
- **Section 2 代码质量 / Section 3 测试 / Section 4 性能**：No issues found（测试分类+行为保持清单+shadow 白名单已在前两轮夯实）。
- **测试计划 artifact**：~/.gstack/projects/Grong-tinyiothub/chenguorong-ai-brain-p0-eng-review-test-plan-20261012-130000.md
- **外部声音**：codex broken_install；native fallback 完成但报告内容丢失（已知问题）——无可计覆盖。
- **Approval readiness: PASS**（D1 结构裁定；CEO 轮 D8-D11 结转；无 pending 行）
- **Suppressed findings**：无。

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 3 | CLEAR | 6 proposals, 6 accepted, 0 deferred |
| Outside Review | codex (auto) | Independent 2nd opinion | 7 | unavailable | codex broken_install；native 内容丢失——无可计覆盖 |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 4 | CLEAR (PLAN) | 1 issue（P3 注册表关系注明——已落 spec），0 critical gaps |
| Design Review | `/plan-design-review` | UI/UX gaps | 1 | ISSUES OPEN | 上轮（透明化）5→7/10；本 spec 的 AI 中心建议实现前专项 |
| DX Review | `/plan-devex-review` | Developer experience gaps | 0 | — | — |

- **OUTSIDE COVERAGE:** codex, plan-review phase, **unavailable** — CLI broken_install（vendor binary ENOENT，修复：`npm install -g @openai/codex`）；native fallback 完成但报告内容丢失。无可计覆盖。
- **VERDICT:** CEO + ENG CLEARED——可实现；实现前建议对 AI 中心跑一轮 /plan-design-review 专项（非阻塞）。

NO UNRESOLVED DECISIONS
