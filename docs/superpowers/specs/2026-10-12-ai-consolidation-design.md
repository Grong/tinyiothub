# AI 子系统全链路梳理设计（概念统一 + 内核归一 + 前端单入口）

- 日期：2026-10-12
- 分支：另起 ai-consolidation 分支实施（CEO plan 头部统一为此）
- 状态：四节设计已与作者逐节确认（概念模型/内核归一/前端 AI 中心/迁移测试策略）；spec 评审第 1 轮 18 条发现已全部落回本文（4 条作者裁定 D8-D11 + 14 条直接修）；设计评审 18 条发现（F1-F18）已按推荐批量裁定落回 §2/§3

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
| 心跳失败不开票（周期 tick 失败只产 G3 system_error 事件——旧模型从不进工单系统） | ticket_for_run 漏斗守卫：trigger 前缀 heartbeat: 的失败 run 不开票；对账 SQL 同步排除 trigger_type='heartbeat'（2026-10-14 事故：T9 合成行被对账误捞补开 3 票） | ticket_subscriber_tests::heartbeat_run_failure_does_not_ticket + ticket.rs::unticketed_failed_runs_excludes_heartbeat_trigger |

**信任门控作用域（D9 裁定的行为变更）**：统一循环后所有 run 过信任门（调查/指令也受约束）——一等 UX 言行一致。行为保持清单的显式例外（唯一故意行为变更），迁移期告警路径先行验证。**读门时机（F2，eng 复审 R1 改裁）**：每次动作调用新鲜读档（保持 O7 kill-switch 立即生效 seam，autonomous_invoke.rs:278-280）——切换立即生效，在途 run 的后续动作也按新档执行。

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
├─ 顶栏：信任模式选择器（segmented control，常驻一等 UX——F3）+ 统计条（≤3 指标——F4）+ 配置入口
├─ 「需要你」（第一屏锚点——F1：有待办时置顶区呈现带计数；清零塌陷为「一切正常」一行+今日摘要入口）
├─ 「AI 今日自述」（E1）：一行正文灰字，非卡片（F6）
├─ 主体：Run 流（时间倒序统一流——调查/巡检/指令混排，每条=卡片+可展开时间线，
│  来源徽章=中性灰底文字区分三类 trigger——F5；卡片+时间线组件直接复用现状）
│  · 筛选+搜索（E4）：trigger 类型/结果/关键词（F14 维度全集）
│  · SSE 实时插入新 run（E6：新事件类型 run_recorded 推内容而非只推刷新信号）
└─ 配置 = 独立分区路由 #/ai/config（可深链可分享——F11；信任细节/工具权限/技能/心跳任务——agents 4 tab 归位）
```

**第一屏层级（F1）**：需要你 > Run 流 > E1 > 统计条。运维打开的第一问是"有没有事需要我"——有待办时置顶区是锚点；E1 是氛围不是锚点，不抢层级。

**信任选择器（F3）**：segmented control 三档，当前档下方配一句话行为说明；三档措辞=「需要你批准 / 自动巡检，动手先问 / 全自动」（原「只读自动」语义矛盾——只读为何叫自动，改名）；档位解释文案单一事实源=E2 确认弹窗与配置分区信任区共用。组件词汇现无 segmented control（仅 cfg-toggle），新建小组件并入 DESIGN_SYSTEM 令牌（eng P3 注记）。

**统计条（F4）**：今日消化 / 需要你 / 异常数 三指标；p50 延迟与反馈学习数移入配置分区诊断区，不堆顶栏。

**信任模式切换（E2）**：切换历史记录（谁/何时/从何改到何）+ 切换确认弹窗。**生效规则（F2，eng R1 改裁）**：切换立即生效——在途 run 的后续动作也按新档执行（保持 O7 kill-switch seam：每动作新鲜读档）；弹窗文案模板="立即生效：当前 N 个进行中的 run 的后续动作也按「全自动」执行"。存储：新表 `trust_config_history`（Q5 修正——不复用 brain_event；与 persist.rs 的 fencing upsert 语义对齐）。

**「AI 今日自述」状态矩阵（F6）**：LLM 挂/超时→静默隐藏不留残壳；零 run（新工作区首日）→引导文案"还没开始干活，去配置心跳任务"（链配置分区）；加载中→骨架一行占位不跳动；缓存=按本地日期 key+当日首次可见时刷新。输入=当日 run 的 summary 集合（D8 裁定，RunReport 不增字段）；时机=页面加载时聚合一次、结果按日缓存。可折叠且折叠态 localStorage 持久化（F13）。

**批准→执行回链（F7）**：批准动作产生的执行 run 与提案行互链——提案行显示"已批准 → 查看执行 run"，执行完成后回填执行结果状态；执行 run 卡片标注来源提案。审批闭环的最后一环（批准的后果可追溯）。

**详情页反馈入口（E5/F12）**：结论级常驻 ✓/✕ + 步骤级 hover 才显现的单个「这步有问题」入口（点击后该步骤 id 随反馈写入 judgment_feedback——反馈闭环最后一厘米）；同一 run 已反馈过的视觉回执防重复提交。步骤级不做常驻按钮（10+ 步的按钮丛林不可接受）。

**E6 实时（F10）**：沿用现状 sse-banner 断连细条（「实时更新已断开，点击重连」）；断连重连后整表补拉一次（推内容模式下没有整表刷新兜底，错过即缺失）；run_recorded 推内容与 60s 轮询整表刷新双通道按 run_id 幂等（已存在则更新，不重复插入）；SSE 新 run 插入=顶端淡入+短暂高亮（F18）。run_recorded payload=卡片字段子集（id/来源/标题/徽章/时间），不推整 RunReport（eng P3 注记）。

**空态矩阵（F9）**：全新工作区首日（零 run 零提案未配心跳）=引导去配置心跳任务——新用户对产品智能的第一印象；E4 筛选零结果="没有匹配的运行"+清除筛选按钮（与"还没有任何运行"严格区分）；「需要你」清零=「一切正常」+今日消化数（沿用现状产品故事时刻）。

**行为沿用清单（F14/F17——照 dispositions 现状搬家）**：PAGE_SIZE=20 游标分页+「加载更多」；展开卡 reconcile；防重入 pendingIds；键盘 a11y（Enter/Space 展开、Esc 收起+焦点交还）；时间戳三档格式；Run 流三态（骨架行 ×3 / 空态 / 错误条+重试保留旧数据）照 v3 状态表。E4 筛选态由 hash query 承载（可回退可分享）；从 `#/ai/runs/{id}` 返回时筛选态与滚动位置保留。路由器需扩展 query 解析（现状仅路径正则，app.ts:323）——`#/ai?trigger=heartbeat` 依赖之（eng P3 注记）。

**视觉基调（F8）**：延续 dispositions 单色纪律为本体——颜色只出现在需要行动处（琥珀●）；深空元素仅限 DESIGN_SYSTEM 已令牌化的微件（发光状态点、主按钮渐变）；不引入浮动卡/渐变边框；令牌纪律（--space-*/--radius-*/--duration-* 禁硬编码）；动效沿用 reveal + prefers-reduced-motion 纪律（DESIGN_SYSTEM §4）。来源徽章中性灰底文字，行动色只属于「需要你」（F5）。

**统一状态词表（F5）**：归一后"一种 Run"在统一流的徽章词=分析中 / 自行闭环 / 需要你 / 执行中 / 闭环 / 转工单 / 已拒绝 / 未受理 / 巡检正常（沿用 v3 十秒测试评分表，逐词不新增）。

**响应式与可达性（F16——v3 规格继承，本期落地还债）**：移动 <768px——事件单列全宽（徽章与标题同行）、统计行折行、筛选 chips 可横滑、信任选择器全宽分段；键盘 tab 序列=信任选择器→需要你卡→筛选→流卡（Enter 展开）；焦点环可见（--ring）；批准/拒绝/反馈按钮最小 44px 触摸目标；色盲安全=徽章不靠单色区分，「需要你」=琥珀点+文字标签同时存在；正文对比度 ≥4.5:1，弱化文字（时间/元信息）≥3:1 但不承载操作含义。**上轮 deferred 的移动规格（现 dispositions.css 零 media query）随 #/ai 一并落地，不再递延。**

**新/老用户梯度（F13）**：信任选择器旁一次性新手 tooltip（首次访问展示，看过即记录）；E1 可折叠（见上）。

**用户旅程故事板（设计评审 Pass 3 产物）**：

| 步骤 | 用户做 | 用户感到 | spec 靠什么支撑 |
|---|---|---|---|
| 1 | 打开 #/ai | 「有没有事需要我？」 | 「需要你」置顶区+计数，5 秒定位（F1） |
| 2 | 扫提案卡 | 「AI 已经查过了」 | 标题=结论、理由=人话、徽章=决定（沿用现状） |
| 3 | 批准/拒绝 | 「我说了算」 | 一键批准；切换确认弹窗讲清在途影响（F2） |
| 4 | 批准后追踪 | 「它干了没有？」 | 提案↔执行 run 互链+结果回填（F7） |
| 5 | 待办清零 | 「它在替我值班」 | 「一切正常」+今日消化数（F9） |
| 6 | 日后回翻 | 「它每天都干了这些」 | Run 流+E1 自述+筛选态保留（F14） |
| 7 | 首日（新工作区） | 「这东西会干活吗？」 | 引导空态→配置心跳任务（F9） |

关键转变：组织单位从功能页面换成 run；过程叙事嵌在流里，不做独立日志页；执行历史不再单独存在——它是 Run 流的巡检段。

归并映射：

| 旧入口 | 去向 |
|---|---|
| #/dispositions | → #/ai 主体（Run 流+需要你） |
| #/ai-ops | → 摘要进顶栏统计条（F4 收敛）；审批进「需要你」；历史并入 Run 流；信任进顶栏+配置 |
| #/agents | → #/ai/config 配置分区（F11） |
| #/chat | 不动（对话是独立形态，本期边界） |
| #/ai-ops/runs/{id} | → #/ai/runs/{id}（路由改名，内容不动） |
| patrol_tick 聚合行点击（原跳 #/ai-ops） | → #/ai?trigger=heartbeat（Run 流巡检筛选预设——F15） |
| 卡片「查看完整轨迹」 | → #/ai/runs/{id}（F15） |
| 卡片「打开工单」 | → #/tickets/{id}（不变） |

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

## Design Review — 评审记录（2026-10-12）

- **方式**：7 pass 全维度 + 外部声音（codex broken_install 不可用；Claude 独立 subagent 完成，15 条发现，报告 `docs/superpowers/reports/2026-10-12-ai-consolidation-design-outside-voice.md`）。mockup 未生成（designer 缺 OpenAI key，文本评审）。
- **评分轨迹**：Pass 1 IA 4→9；Pass 2 状态 2→9；Pass 3 旅程 3→8；Pass 4 AI-slop 5→8；Pass 5 设计系统 4→8；Pass 6 响应式/可达性 1→8；Pass 7 决策登记 18 条全裁。总分（六 pass 最低）1→8。
- **裁定**：18 条（F1-F18，外部声音 15 + 本会话 3）按推荐批量裁定全部落回 §2/§3，逐条可溯（F 编号标注在 spec 正文）。
- **三条 critical 的核心裁定**：F1「需要你」=第一屏锚点（置顶区制，清零塌陷）；F2 信任切换~~在途沿用旧档~~→**eng R1 改裁=立即生效**（保持 O7 每动作新鲜读 seam，弹窗文案改为「立即生效」）；F3 信任选择器=segmented control，「只读自动」改名「自动巡检，动手先问」。
- **litmus**：评审前 2/7（每节一职/卡片必要性）；裁定后品牌归属（F8 视觉基调）、锚点（F1）、动效归属（F18）等 5 项有 spec 依据，实现后 /design-review 复核。

### NOT in scope（设计决策已考虑、明确不做）

- 配置抽屉方案（F11 裁定独立分区路由 `#/ai/config`——抽屉不可深链且 a11y 成本高）
- 「需要你」tab 制（F1 裁定置顶区制——现状 v3 已证明行动优先的层级）
- 彩色来源徽章（F5 否决——违反单色纪律，流变彩条）
- 步骤级常驻反馈按钮（F12 否决——10+ 步按钮丛林；hover 单入口替代）
- p50 延迟/反馈学习数留顶栏（F4 裁进配置分区诊断区）
- Tuya 深空全套视觉（浮动卡/渐变边框）引入 #/ai（F8 裁定单色纪律为本体）
- 新手引导全流程（F13 轻量处理=一次性 tooltip+E1 可折叠）
- mockup 视觉稿（designer 缺 OpenAI key；实现后 /design-review 现场补）

### What already exists（复用清单）

- `web/DESIGN_SYSTEM.md`：--space-*/--radius-*/--duration-* 令牌、单色/深空两套纪律、§4 动效+reduced-motion、§7.4 overlay a11y 清单
- `web/src/ui/shared/run-timeline.ts`（--timeline-* 令牌层，双模式）+ dispositions 卡片/徽章/证据面板——卡片+时间线原样复用
- v3 设计稿（`~/.gstack/projects/Grong-tinyiothub/chenguorong-fix-dispositions-ui-design-20260917-103834.md`）：交互状态表/旅程故事板/响应式与可达性最小规格——§3 已声明继承
- `web/src/ui/views/confirm-modal.ts`（E2 弹窗落点）、sse-banner 断连细条模式（dispositions/tickets 两处现成）
- 现状行为资产：PAGE_SIZE=20 游标分页、reconcileExpanded、pendingIds 防重入、键盘 a11y、时间戳三档格式

### Implementation Tasks（Phase 3 维度，writing-plans 时再拆细）

- [ ] **T1 (P1, human: ~2d / CC: ~1h)** — web — #/ai 页面骨架：需要你置顶区（清零塌陷）+统计条 3 指标+E1 一行+Run 流+配置分区路由
  - Surfaced by: F1/F4/F6/F11 — 第一屏层级与五块形态
  - Files: `web/src/ui/app.ts`（路由）、新建 `web/src/ui/views/ai-center.ts`
  - Verify: `cd web && npx vitest run src/ui/views/ai-center.test.ts`
- [ ] **T2 (P1, human: ~1d / CC: ~30min)** — web+cloud — 信任选择器 segmented control+三档文案+E2 确认弹窗（在途影响文案模板）+trust_config_history
  - Surfaced by: F2/F3 — 控件形态/生效规则/措辞单一事实源
  - Files: `web/src/ui/views/agents-workspace-tab.ts`（信任区）、`apps/cloud/src/domains/agent/`（读门时机已在 §2）
  - Verify: 弹窗文案快照测试；切换对在途/新触发 run 的档位断言
- [ ] **T3 (P1, human: ~2d / CC: ~1h)** — web — Run 流混排：中性来源徽章+统一状态词表+E4 筛选（hash query 承载）+SSE run_recorded 幂等插入+插入入场动效
  - Surfaced by: F5/F10/F14/F18 — 徽章纪律/幂等/筛选态/动效
  - Files: `web/src/ui/views/dispositions.ts`（搬家）、`web/src/api/sse-client.ts`
  - Verify: run_id 幂等测试；hash query 回退/分享测试
- [ ] **T4 (P1, human: ~1d / CC: ~30min)** — web — 空态矩阵三行+Run 流三态沿用 v3 状态表
  - Surfaced by: F9/F17 — 首日引导/筛选零结果/需要你清零
  - Files: `web/src/ui/views/ai-center.ts`
  - Verify: 三空态组件测试（含清除筛选出口）
- [ ] **T5 (P2, human: ~1d / CC: ~30min)** — web+cloud — 提案↔执行 run 互链+执行结果回填
  - Surfaced by: F7 — 批准→执行回链
  - Files: `web/src/ui/views/ai-center.ts`、`apps/cloud/src/domains/agent/host/`
  - Verify: 提案行回填状态测试
- [ ] **T6 (P2, human: ~0.5d / CC: ~20min)** — web — E5 详情页反馈入口（结论级常驻+步骤级 hover+已反馈回执）
  - Surfaced by: F12 — 反馈粒度与控件形态
  - Files: `web/src/ui/views/run-detail.ts`
  - Verify: 步骤 id 随反馈写入断言；重复提交防重测试
- [ ] **T7 (P2, human: ~1d / CC: ~30min)** — web — 响应式/可达性规格落地（<768px 布局/键盘序列/44px/色盲/对比度/reduced-motion）
  - Surfaced by: F16 — v3 规格继承+上轮债本期还
  - Files: `web/src/ui/views/ai-center.css`（新建即带 media query，不复制零 media query 的旧债）
  - Verify: 键盘序列手动过一遍 + 对比度抽查
- [ ] **T8 (P2, human: ~0.5d / CC: ~20min)** — web — E1 自述组件（四状态+按日缓存+折叠持久化+新手 tooltip）
  - Surfaced by: F6/F13 — 状态矩阵/新老用户梯度
  - Files: `web/src/ui/views/ai-center.ts`
  - Verify: LLM 挂→隐藏、零 run→引导、跨日刷新三测试
- [ ] **T9 (P2, human: ~0.5d / CC: ~20min)** — web — 旧 URL 重定向+卡片级深链改写+重定向测试
  - Surfaced by: F15 — 深链映射三行（聚合行/轨迹/工单）
  - Files: `web/src/ui/app.ts`、`web/src/ui/views/dispositions.ts`（搬家期）
  - Verify: 重定向测试覆盖卡片级来源
- [ ] **T10 (P3, human: ~0.5d / CC: ~20min)** — web — 配置分区诊断区（p50 延迟+反馈学习数）
  - Surfaced by: F4 — 统计条收敛的迁出指标去向
  - Files: `web/src/ui/views/ai-config.ts`
  - Verify: 诊断区渲染测试

### TODOS.md updates

无新增 deferred 项——18 条全部落入本期 spec（F16 反而把上轮移动债收编进本期）。原有 TODOS 中「dispositions 响应式规格」条目在 #/ai 落地时由 T7 消化。

### 未决决策

无。18 条全部裁定落回 spec。

## Eng Review — 复审记录（2026-10-12，设计轮增量）

前轮（同日 07:19，CLEAR）覆盖 spec 主体；本轮聚焦设计轮 F1-F18 增量的工程影响。

- **Scope record**：feature answers=无新 cut 提案；structure=**Original arrangement**（精确前轮答案 eng D1 结转——设计增量不新增 phase/后端服务，2 个新前端视图在 CEO 批准的 Phase 3 范围内）；accepted scope=spec 全量+设计轮 18 裁定；pending remedies=R1。
- **Scope Challenge 结果**：scope accepted as-is（FULL_REVIEW）。
- **发现**：P1×1（R1，见台账）+ P3×3（路由器 query 解析扩展 app.ts:323；segmented control 为新组件需入令牌 config.css:2 仅有 cfg-toggle；E6 run_recorded payload 界形=卡片字段子集非整 RunReport）。P3 三条为已实现合同的必要机制注记，随 R1 裁定一并落 spec。
- **Section 1 架构**：R1（F2/O7 冲突）→ D1=A 改裁立即生效；新路径失败模式逐条有主（run_recorded 断连→F10 补拉；E1 LLM 挂→F6 隐藏；trust_config_history→fencing+DLQ 现成模式），无 critical gap。
- **Section 2 代码质量**：路由器 query 解析=app.ts 单点扩展（所有路由共用的根因点）；segmented control 无第二调用方不抽取。No further issues。
- **Section 3 测试**：新增唯一要求=**O7 立即生效回归测试**（切换档位后在途 run 下一动作按新档——锁定现 seam，防将来被「优化」成快照；D1 批准行为的必需证明，结转）；其余增量断言已在设计轮 T2/T3/T4/T8/T9。测试计划 artifact：`~/.gstack/projects/Grong-tinyiothub/chenguorong-ai-brain-p0-eng-review-test-plan-20261012-170349.md`。
- **Section 4 性能**：No issues found（run_recorded payload 已限卡片字段；统计条=现成 summary 端点；E1 按日缓存）。
- **外部声音**：codex CODEX_MODEL invalid（custom endpoint 未配 frontier plan-review 模型——修复：`gstack-config set model_frontier_openai <id>` 或 `plan_review_tier host`）；native fallback 完成但报告内容丢失（最终消息只剩 "Done."，已知问题，Plan 类 subagent 禁 Write 无法用报告文件契约）——无可计覆盖。
- **Implementation Tasks（增量，接设计轮 T1-T10）**：
  - [ ] **T11 (P1, human: ~2h / CC: ~15min)** — crates/agent+cloud — O7 立即生效回归测试：信任切换后在途 run 的下一动作按新档
    - Surfaced by: Section 3 — 锁 O7 seam，防快照化回退
    - Files: `apps/cloud/src/domains/agent/host/tools/autonomous_invoke.rs`（测试）、`apps/cloud/src/tests/`
    - Verify: `cargo test -p tinyiothub-cloud autonomy`
- **Approval readiness: PASS**（R1→D1=A 实际答案结转；P3×3 机制注记随 D1 accepted scope；测试要求为批准行为的必需证明；无 pending 行）

## Decision ledger

### R1: 信任切换对在途 run 的生效语义（F2 与 O7 冲突重开）
Finding: [P1] (confidence: 9/10) apps/cloud/src/domains/agent/host/tools/autonomous_invoke.rs:278-280 — 策略门每动作新鲜读库（注释原文 "fresh read on EVERY call (O7: kill switch takes effect immediately)"），与设计轮 F2 裁定「在途 run 沿用触发时档位跑完」矛盾；reviewer=eng 复审。
Plan baseline: F2（设计轮批量裁定，2026-10-12）——切换对切换后新触发的 run 生效，在途 run 沿用触发时档位；E2 弹窗文案模板="当前 N 个进行中的 run 将按原模式跑完"。
Runtime evidence: autonomous_invoke.rs:278-280 每个 invoke_action 新鲜 load_autonomy_policy（fail-closed）；manager.rs:356 唤醒时另有一次 prompt 边界读（fail-soft）。现 seam 下在途 run 的后续动作天然按新档执行。
Comparison grid:

| Choice | Current | A | B | C |
|---|---|---|---|---|
| 生效语义 | F2=在途沿用旧档（与 O7 seam 矛盾） | 立即生效（在途 run 后续动作按新档） | 快照语义（run 触发时快照档位，invoke 改用快照） | 方向不对称（收紧立即，放宽下一 run） |
| O7 kill-switch 立即生效 | 现网行为 | 保持 | 推翻（收紧也不立即） | 收紧方向保持 |
| E2 弹窗文案 | "在途 run 按原模式跑完" | 改="立即生效，在途 run 的后续动作也按新档执行" | 不变 | 文案需分方向两句 |
| 实现成本 | — | 零新机制（改 spec 文案） | 新 plumbing（快照随 RunContext 传入 invoke 路径） | 每次 invoke 比较新旧档，最复杂 |
| 安全风险 | — | 收紧立即生效=更安全 | 收紧被快照延迟=风险 | 收紧立即=安全 |

Question D1:
D1 — 信任切换对在途 run 的生效语义（F2 vs O7 冲突）
Project/branch/task: ai-brain-p0，AI 子系统梳理 spec 设计轮增量 eng 复审
ELI10: 现状策略门每个动作都重新读档（O7：kill switch 立即生效——你切档位，下一个动作就按新档算，包括正在跑的 run 的后续动作）。设计轮定的 F2 说"在途 run 按原模式跑完"，两者矛盾。若守 F2 就要给 run 做档位快照， kill switch 就不再立即生效——你切到"需要你批准"，正在跑的 run 还按旧档自动执行，安全方向反了。
Stakes if we pick wrong: 弹窗文案与实际行为不符=信任事故（用户以为批了的其实已自动执行，或以为收紧了其实还在自动跑）。
Recommendation: A because 保持 O7 安全性质+零新机制，只改一句弹窗文案；快照语义削弱 kill switch 且实现最重。
Note: options differ in kind, not coverage — no completeness score.
Pros / cons:
A) 立即生效，改 F2 文案 (recommended)
  ✅ 保持 O7 kill-switch 安全性质：收紧方向立即生效，正是用户切换时的直觉预期
  ✅ 零新机制零新测试面，spec 只改弹窗文案模板一句，统一循环不动 invoke seam
  ❌ 推翻两小时前的 F2 裁定（文案承诺方向反过来），需要改 §2/§3 两处
  ❌ 放宽方向也立即生效——弹窗必须把这点讲清楚，否则用户以为有缓冲
B) 快照语义，守 F2 原文
  ✅ 弹窗文案不变，"在途 run 可预测地跑完旧档"叙事完整
  ✅ run 级语义自洽（一个 run 一个档位，审计更干净）
  ❌ 推翻 O7：切到「需要你批准」后在途 run 仍自动执行，kill switch 失灵是安全事故
  ❌ 新 plumbing：快照随 RunContext 传入 invoke 路径替换新鲜读，改动面+测试面最大
C) 方向不对称（收紧立即，放宽下一 run）
  ✅ 安全方向最优：收紧永远立即，放宽有缓冲
  ✅ 两个方向的直觉预期都满足
  ❌ 每次 invoke 比较新旧档的实现+解释成本最高，弹窗文案要分方向两句
  ❌ 语义最难一句话讲清，"部分立即部分延后"本身就是理解负担
Net: 安全性质（O7）与文案承诺（F2）二选一；A 保安全改文案，B 保文案削弱安全，C 两者都保但复杂度最高。
Header: 在途 run 生效语义
Options:
A) 立即生效，改 F2 文案 (recommended)
保持 O7 seam 不动；E2 弹窗文案改为"立即生效：在途 run 的后续动作也按新档执行"；§2 读门时机改为「每次动作调用新鲜读档（O7 保持），切换立即生效」。
B) 快照语义，守 F2 原文
统一循环在 run 触发时快照档位并随 RunContext 下传 invoke 路径；接受 kill switch 不再立即生效的后果。
C) 方向不对称
收紧立即生效、放宽下一 run 生效；invoke 处比较快照档与当前档取更严者。

State: approved
Actual answer: A（立即生效，改 F2 文案）— AskUserQuestion D1，2026-10-12
Accepted scope: 保持 O7 seam（每动作新鲜读档）；§2 读门时机与 §3 E2 弹窗文案改为「立即生效」语义；信任选择器 segmented control 新建组件入令牌、路由器扩展 query 解析、run_recorded payload=卡片字段子集三条 P3 机制注记随本裁定落 §3 正文。
History: F2 初裁（设计轮批量裁定，2026-10-12）=在途沿用触发时档位；eng 复审发现与 O7（autonomous_invoke.rs:278-280 代码内裁定）矛盾，重开；D1 答 A 改裁为立即生效。

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 3 | CLEAR | 6 proposals, 6 accepted, 0 deferred |
| Outside Review | codex (auto) | Independent 2nd opinion | 8 | unavailable | codex broken_install→CODEX_MODEL invalid（custom endpoint 未配 frontier 模型）；native 两次完成均内容丢失——无可计覆盖 |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 5 | CLEAR (PLAN) | 复审（设计轮增量）：1 issue（R1 F2/O7 冲突→D1=A 立即生效）+3 P3 机制注记+1 测试结转，0 critical gaps |
| Design Review | `/plan-design-review` | UI/UX gaps | 2 | CLEAR | score: 1/10 → 8/10, 18 decisions（F1-F18 全落 §2/§3） |
| DX Review | `/plan-devex-review` | Developer experience gaps | 0 | — | — |

- **OUTSIDE COVERAGE:** codex, plan-review phase, **unavailable** — CLI 已修复但 CODEX_MODEL invalid（custom endpoint 未配 frontier plan-review 模型；修复：`gstack-config set model_frontier_openai <id>` 或 `plan_review_tier host`）；native fallback 完成但报告内容丢失（已知问题）。design phase 同样 unavailable（Claude subagent 完成 15 条发现，已并入 F1-F15）。无可计外部覆盖。
- **VERDICT:** CEO + ENG + DESIGN CLEARED——可实现。

NO UNRESOLVED DECISIONS
