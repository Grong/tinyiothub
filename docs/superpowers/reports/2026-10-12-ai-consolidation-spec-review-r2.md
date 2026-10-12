# Spec 复审报告（第 2 轮）：AI 子系统全链路梳理

- 复审日期：2026-10-12
- 输入 A（修订后 spec）：`docs/superpowers/specs/2026-10-12-ai-consolidation-design.md`
- 输入 B（CEO 摘要）：`~/.gstack/projects/Grong-tinyiothub/ceo-plans/2026-10-12-ai-consolidation.md`
- 输入 C（第 1 轮报告）：`docs/superpowers/reports/2026-10-12-ai-consolidation-spec-review.md`
- 复审方式：逐条验证 18 条发现 + 新矛盾/新歧义扫描；D8-D11 为作者裁定，仅验证 spec 是否忠实反映，不重开裁定本身

**总体质量分：8 / 10**
**总体结论：PASS**（五维度全部 PASS；16/18 条完全解决，K4 部分残留，Q1/Q3 各留一处小尾巴；新增 5 条发现，其中 1 条中等[N3]、4 条轻微）

修订质量高：18 条发现逐条落回 spec 且大多原文标注了来源编号（C1/C3/D8-D11/K1/K3/Q1/Q4/Q5/F2/F3），可追溯性好。行为保持清单（§2 表格）正是第 1 轮 F1 要求的形态。残留问题集中在两处：(a) D11 专用 dedup key 与告警路径的交互未定义（N3，唯一实质缺口）；(b) 若干措辞级不一致（N1/N2/N4/N5）。

---

## 1. 第 1 轮 18 条发现逐条验证

### Completeness（C1-C6）

| # | 结论 | 验证 |
|---|---|---|
| C1 心跳既有行为无着落 | **已解决** | §2 新增「行为保持清单（C1）」表（L74-81），四行逐一覆盖第 1 轮列的四项行为：连败 5 次暂停→统一循环同等语义+mock 池连败断言；last_tick 内存态→同字段透出+handler 测试不变；ReloadTasks/ReloadConfig 热更新→信号通道承接；interval_minutes→Heartbeat 变体 per-workspace 间隔驱动。每行均有归宿+验证方式，正是建议修法要求的形态。 |
| C2 落库耐久性降级 | **已解决（D10 裁定）** | §2 L71「耐久性对齐（D10）：RunRecorded 补心跳同款重试+DLQ（persist.rs 现成模式）——归一不得静默降级可靠性」。忠实反映 D10，且复用现成模式可行。 |
| C3 提案 tick_id 断链 | **已解决** | §2 L71「提案行 tick_id 迁移时一并改写为 run_id（C3：联结不断链），读侧兼容期保留 tick_id→run_id 映射视图，切换完成后删」。归属+联结+退役路径三段齐全，与建议修法一致。 |
| C4 narrative 无数据面 | **已解决（D8 裁定）** | 词汇表 L55 Narrative 行「=summary 的展示形态，RunReport 不增字段」；E1（L107）输入明示为「当日 run 的 summary 集合」，时机=页面加载聚合一次+按日缓存。采纳了建议修法的 (a) 分支。残留措辞问题见 N2。 |
| C5 Trust Mode 语义扩张 | **已解决（D9 裁定）** | 词汇表 L54 与 §2 L83 双重声明「统一循环后所有 run 过信任门（调查/指令也受约束）」，且显式标注为「行为保持清单的显式例外（唯一故意行为变更），迁移期告警路径先行验证」。语义扩张被承认并隔离，正是 C5 要求的「列入例外」。 |
| C6 读侧兼容视角形态 | **已解决** | §2 L69「适配层=handler 侧纯函数 RunReport→ExecutionRecord；历史无 trace 行前端显示"无过程记录"不炸」——位置、形态、历史行 UI 表现三要素齐全，与建议修法逐字对应。 |

### Consistency（K1-K4）

| # | 结论 | 验证 |
|---|---|---|
| K1 "旧 4 入口废弃"矛盾 | **已解决** | §3 L102 改为「旧 3 入口废弃+临时重定向一个版本；chat 为独立形态不在本期统一范围——K1 修正」；目标节 L28 同步改「运维向单一 AI 中心入口（chat 独立形态除外）」。三处矛盾点全部消除。 |
| K2 指令会话与 chat 边界 | **已解决** | 词汇表 Run 行 L49 改为「指令触发的 run（UserDirective；chat 消息不落 agent_runs，chat 是独立形态）」——删掉了"指令会话"表述并明示 chat 不落 agent_runs，与建议修法一致。 |
| K3 "套件全绿"不成立 | **已解决** | §4 L139 改为「分类后的现有套件全绿」，并点名三个必重写测试（crates/db/src/heartbeat.rs 锚行/提案共存、agent_persist_tests、heartbeat_bridge_tests）归为「实现细节重写」类。测试分类+清单+「全绿」定义三点齐全。 |
| K4 分支名不一致 | **部分残留** | spec L4 已改为「另起 ai-consolidation 分支实施（CEO plan 头部统一为此）」——spec 侧已修，但 **CEO plan 头部 L6 仍是 `Branch: ai-brain-p0`**，spec 声称的「CEO plan 头部统一为此」未兑现。琐碎但与第 1 轮同级，见 N1。 |

### Clarity（Q1-Q5）

| # | 结论 | 验证 |
|---|---|---|
| Q1 双跑语义危险 | **已解决（留一处小尾巴）** | §4 L139 改为「shadow 双跑——心跳路径照常执行，统一 Run 路径在 mock 池接缝上只产报告不落库/不落提案，比对字段白名单=status/summary/proposals 集合/trace 工具名序列；比对期 N=7 天或 M=50 次 tick 后切换」。shadow 语义/不落库不落提案/字段白名单/切换判据四要素齐全。残留：mock 池接缝作用域与比对基线未点明（见 N5）。 |
| Q2 Timer 映射二选一+dedup 碰撞 | **已解决（D11 裁定）** | §2 L69 显式选定「TriggerSource::Heartbeat 新变体+专用 dedup key `heartbeat:{ws}`」，并给出不选 (a) 的理由（丢 per-workspace 间隔+与 `timer:{ws}` 碰撞吞 tick）。忠实反映 D11。但 D11 与告警路径的交互引入新缺口，见 N3。 |
| Q3 告警外部信号路径去向 | **基本解决（答了半边）** | §2 L72「告警触发链（HeartbeatSignal 带优先级+dedup）本期保留入口、内部转 TriggerSource::Heartbeat 变体（优先级映射：Critical→Critical，High/Normal→Normal）」——入口保留+内部转换+优先级映射表三要素齐全。但原问题中的 dedup 半边（HeartbeatSignal 按 (thing_id, alarm_type) dedup）在转换后用什么 key 未答，与 D11 的 `heartbeat:{ws}` 存在吞并风险，见 N3。 |
| Q4 E3 lint 判定语义 | **已解决** | §4 L140 限定扫描范围（`web/src` 文案 + `apps/cloud/src/domains/agent` 注释）、异名清单=词汇表右列、首版 warn-only 一个版本后转 fail。与建议修法逐条对应。 |
| Q5 E2 切换历史存储 | **已解决** | §3 L116「存储：新表 `trust_config_history`（Q5 修正——不复用 brain_event；与 persist.rs 的 fencing upsert 语义对齐）」。表名+不复用决策+语义对齐三点齐全。 |

### Scope

第 1 轮已 PASS，本轮复核无变化：E1-E6 与 CEO Scope Decisions 仍逐项对应；D8-D11 四条裁定均为作者明示的行为/范围决策且已回填 CEO Reviewer Concerns（CEO plan L36），不构成夹带。

### Feasibility（F1-F3）

| # | 结论 | 验证 |
|---|---|---|
| F1 行为保持不可验证 | **已解决** | §2 行为保持清单表（行为/归一后归宿/验证 三列）正是建议修法要求的形态，C1 四项+C2/C3 答复均已并入该表或紧邻段落。实现者现在有了一张可核对清单。 |
| F2 等价对照断言维度 | **已解决** | 并入 Q1 修正：mock 池接缝+字段白名单（status/summary/proposals 集合/trace 工具名序列），与建议修法一致。 |
| F3 搬家与路由改名张力 | **已解决** | §4 L145 补「含站内路由引用改写——F3：runs 详情链接从 #/ai-ops/runs 改 #/ai/runs」。 |

**小结：完全解决 16 条；部分解决 2 条（K4=CEO plan 头部未同步；Q3=dedup 半边未答，已转入新发现 N3）；错误修复 0 条；需重开裁定 0 条。**

---

## 2. 修订引入的新发现

### N3（中，Clarity/Completeness）：D11 专用 dedup key 与告警路径的吞并风险未定义
D11 裁定心跳专用 dedup key `heartbeat:{ws}`（§2 L69），Q3 修复又让告警「内部转 TriggerSource::Heartbeat 变体」（§2 L72）。两处合流后产生一个 spec 未回答的问题：**告警驱动的 run 用什么 dedup key？** 原 HeartbeatSignal 按 `(thing_id, alarm_type)` dedup（第 1 轮 Q3 引 types.rs:31-49），正是为了让不同 thing/不同告警类型互不合并。若告警转换后复用 `heartbeat:{ws}`，merge window 内一条 Critical 告警会被同工作区一个在途定时 tick 合并吞掉——这比第 1 轮 Q2 的「tick 被 timer 吞」更糟（告警是用户可感知的）。若保留 `(thing_id, alarm_type)` 维度，spec 没说，且与「专用键」表述有张力。
**建议修法**：§2 L72 补半句——告警转换后 dedup key 形态（如 `heartbeat:{ws}:{thing_id}:{alarm_type}` 或保留原语义），并声明告警 run 与定时 tick 不互合并。

### N1（轻，Consistency）：K4 残留——CEO plan 头部未同步
spec L4 声称「CEO plan 头部统一为 ai-consolidation」，实测 CEO plan L6 仍是 `Branch: ai-brain-p0`。spec 侧声明与事实不符。
**建议修法**：改 CEO plan 头部一行，或 spec 删掉「CEO plan 头部统一为此」的断言。

### N2（轻，Consistency）：概念链仍把 narrative 列为 Run 组成，与 D8 不一致
§1 核心概念链 L40-42 仍写 Run =「（trace+summary+ narrative）」，把 narrative 列为 Run 的并列组成；而 D8 裁定（词汇表 L55、E1 L107）narrative 只是 summary 的展示形态、RunReport 不增字段，词汇表 Run 行 L49 也只写「trace+summary」。同节内两处对 Run 组成的表述不一致。
**建议修法**：概念链 Run 注脚删 narrative，或改注「narrative=summary 的展示形态」。

### N4（轻，Consistency）：「迁移只做 trace 回填+读侧切换」漏了 tick_id 改写
§4 L139「迁移只做 trace 回填+读侧切换」与 §2 L71「提案行 tick_id 迁移时一并改写为 run_id」有张力——tick_id 改写是迁移工作但不属「trace 回填」。「只做」的排他表述会让执行者疑惑 tick_id 改写属不属于 Phase 2。
**建议修法**：§4 改为「迁移只做 trace 回填+提案 tick_id 改写+读侧切换」。

### N5（轻，Clarity）：shadow 双跑的 mock 池作用域/比对基线未点明
§4 L139「心跳路径照常执行，统一 Run 路径在 mock 池接缝上只产报告」——若「照常执行」指真实 LLM 池而统一路径走 mock 池，两路径 LLM 输出天然不同，白名单比对无意义。charitable reading 是比对 harness 内两路径共享 mock 池接缝（「照常执行」指心跳逻辑不 stub），但字面可两读。
**建议修法**：半句消歧——「比对 harness 内两路径共用 mock 池（确定性接缝），心跳逻辑照常执行」。

---

## 3. 五维度评分

| 维度 | 结论 | 问题数 | 说明 |
|---|---|---|---|
| Completeness | **PASS** | 0（N3 计入 Clarity） | C1-C6 全部落实；行为保持清单+耐久性对齐+联结改写+兼容视角形态齐全 |
| Consistency | **PASS** | 3 轻（N1/N2/N4） | K1-K3 已修；K4 CEO 侧残留+2 处措辞级自相矛盾，均不阻塞 |
| Clarity | **PASS** | 1 中（N3）+1 轻（N5） | Q1-Q5 基本落实；告警 dedup key 交互是唯一需要作者再答一句的实质点 |
| Scope | **PASS** | 0 | E1-E6 与 CEO 对齐；D8-D11 为作者裁定且已回填 CEO Concerns，无夹带 |
| Feasibility | **PASS** | 0 | F1-F3 全部落实；第 1 轮核实的正向支撑（Timer 触发器/ToolTraceEntry 共享/chat 两处平铺/judgment_feedback/RunRecorded 存在）未被修订破坏 |

**总分 8/10（第 1 轮 6/10）**。扣分项：N3（-1，实质设计缺口），N1/N2/N4/N5 措辞级（合计 -1）。修掉 N3 一句话 + N1/N2/N4/N5 四处措辞，可达 9-9.5。

## 4. 给 CEO Reviewer Concerns 的回填建议

第 2 轮：18 条全部验证——16 完全解决，K4 CEO 侧残留（N1），Q3 dedup 半边转 N3。**建议放行条件**：仅需补 N3 一句（告警 dedup key 形态+不与 tick 互合并）；N1/N2/N4/N5 顺手修，不阻塞。
