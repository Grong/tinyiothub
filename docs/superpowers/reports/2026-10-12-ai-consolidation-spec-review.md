# Spec 评审报告：AI 子系统全链路梳理（概念统一+内核归一+前端单入口）

- 评审日期：2026-10-12
- 输入 A（spec）：`docs/superpowers/specs/2026-10-12-ai-consolidation-design.md`
- 输入 B（CEO 范围摘要）：`~/.gstack/projects/Grong-tinyiothub/ceo-plans/2026-10-12-ai-consolidation.md`
- 评审方式：对抗性评审，五维度；代码核实基于 `crates/agent/src/runtime/`、`crates/db/src/`、`apps/cloud/src/domains/agent/host/`、`web/src/ui/views/`

**总体质量分：6 / 10**
**总体结论：不 PASS**（Completeness / Consistency / Clarity / Feasibility 有编号问题；Scope PASS）

Spec 的方向正确且有代码支撑（TriggerSource::Timer 已存在、ToolTraceEntry 双 runner 共享、chat 两处平铺属实、judgment_feedback 表已存在），CEO 摘要与 spec 范围严格对齐。但"内核归一"是行为保持重构，spec 对三类行为保持风险（读侧兼容、提案↔run 联结、调度器合并碰撞）基本未正面处理，且"现有套件全绿即回归网"的护栏声明部分不成立。以下逐维度展开。

---

## 1. Completeness — 不 PASS（6 项）

对照 spec 自声明目标（概念统一、内核归一、前端单入口、E1-E6）逐项核查。

### C1. 心跳循环的既有行为在归一后无着落（内核归一）
spec §2（L60-70）与 §4（L126）均未提以下 `crates/agent/src/runtime/heartbeat/loop_.rs` 的既有行为：
- **连续失败自动暂停**：`MAX_CONSECUTIVE_FAILURES=5` 后暂停 loop 并发布一条合成 Error HeartbeatResult 事件（loop_.rs:81-105）。统一 Run 循环无对应语义，spec 未说保留/退役/映射。
- **`last_tick` D13 实时读**：`GET /workspaces/{id}/heartbeat/config` 的 `last_tick` 字段读 runner 内存态（workspace_heartbeat.rs:151-155，loop_.rs:75）。HeartbeatRunner 退役后该字段数据源未指定。
- **任务/配置热更新**：`LoopSignal::ReloadTasks / ReloadConfig`（types.rs:67-74）+ `update_trust_config` 写穿运行中 loop（runner.rs:153-169）。统一循环的触发模型如何承接"改任务列表立即生效"未说明。
- **每工作区可配间隔**：`HeartbeatConfig.interval_minutes` + `MIN_HEARTBEAT_INTERVAL_MINUTES`（types.rs:51-63）。映射到全局 `config.timer_interval` 的 TimerTrigger（manager.rs:191-195）会丢失每工作区可配性——行为变更，spec 未承认。

**建议修法**：spec §2 增列"行为保持清单"，逐条声明上述行为在统一 Run 循环中的归宿（保留=映射到哪、退役=读侧如何兜底）。

### C2. 落库耐久性不对称，归一会静默降级（内核归一）
`persist.rs` 模块文档明示（L4-12）：`HeartbeatResultReady` 落 agent_actions 有专属重试（2s base、2^attempt、5 次）+ DLQ + `HeartbeatPersistFailed` 事件（persist.rs:46-49, 555-630）；而 `RunRecorded` 落 agent_runs 是 insert-once 裸 INSERT，仅靠 5 分钟周期对账兜底（persist.rs:4-5, 44-53）。心跳并入 Run 后，tick 结果落库从"重试+DLQ"降级为"对账兜底"，最长 5 分钟丢失窗口。**两份文档均未提及**。
**建议修法**：spec §2 落库归一节声明耐久性对齐方案（RunRecorded 补重试/DLQ，或论证对账窗口可接受并量化）。

### C3. 提案↔run 的 tick_id 联结在迁移后断链（内核归一 / agent_actions 边界）
当前提案行与 tick anchor 行同表同 tick_id 共存（`crates/db/src/heartbeat.rs:591, 866-887` 测试明示"提案/auto_executed 行携带 tick_id（== result.id），tick anchor"）。spec §2（L70）说"提案/审批仍留 agent_actions——它们是动作不是 run"，方向对，但**未说 anchor 迁至 agent_runs 后提案行的 tick_id 是否改写为 run_id、读侧（get_logs 按 tick 聚合提案、审批定位来源 tick）如何 join**。这是 spec 自称已处理的"提案/审批与 run 的边界"实际上只处理了归属、没处理联结。
**建议修法**：明确 tick_id → run_id 的改写/桥接策略（迁移回填时一并改写，或保留映射表），并列入读侧兼容视角的职责。

### C4. E1 叙事（narrative）无数据面支撑（E1 / 概念模型）
概念模型定义 Run = "trace+summary+narrative"（§1 L41-42），E1 要"narrative 聚合"出每日一句。但 `RunReport`（`crates/core/src/agent_runs.rs:114-135`）**没有 narrative 字段**，只有 summary。spec 未说 narrative 是新增字段、派生计算还是词汇表的前瞻表述；E1 的 LLM 聚合何时跑（页面加载？每日定时？）、结果存哪、成本如何控制，全部未定义。
**建议修法**：二选一——(a) 词汇表 narrative 降级为"summary 的展示形态"，E1 输入明示为当日 run 的 summary 集合；(b) RunReport 增字段并写入 spec §2。同时补 E1 的运行时机与缓存策略一句话。

### C5. Trust Mode 语义扩张未定义（E2 / 前端单入口）
顶栏信任模式选择器是"工作区自主级别"一等 UX（§1 L54, §3 L93），E2 已批准。但 TrustConfig 今天**只门控心跳**：`pool.set_trust_config` 的全部生产调用点在 heartbeat runner（runner.rs:169, 390；`crates/agent/src/pool/pool.rs:160`），thing_agent 的调查/指令 run 不经信任门控。把选择器提升为工作区级一等 UX 而未定义"调查 run 是否从此被信任门控"，要么语义误导（UI 声称的管控范围大于实际），要么是未声明的行为变更。
**建议修法**：spec §2 或 §3 明示信任门控的作用域决策——统一循环后所有 run 过信任门（行为变更，需列入行为保持清单的例外），还是门控仍只作用于 Timer 源 run（则顶栏文案需限定）。

### C6. 读侧兼容视角的归属与形态未定义（内核归一）
"HeartbeatResult 退役为 RunReport 的读侧兼容视角"（§2 L68）是方向而非设计：`GET /heartbeat/logs` 当前从 agent_actions anchor content 解析 trace（workspace_heartbeat.rs:92-93），兼容视角意味着一个 RunReport→HeartbeatResult 形态的适配层，但其位置（db crate？handler？）、形态（SQL view？Rust 转换函数？）、对历史无 trace 行的行为（"不补"之后前端看到什么）均未指定。
**建议修法**：一句话指定适配层归属（建议 handler 侧纯函数 RunReport→ExecutionRecord，复用 report.rs 既有解析的逆向），并声明历史无 trace 行的 UI 表现。

---

## 2. Consistency — 不 PASS（4 项）

### K1. "旧 4 入口废弃"与"#/chat 不动"直接矛盾（前端单入口）
§3 L89："单一入口 `#/ai`（**旧 4 入口废弃**+临时重定向一个版本）"；同节映射表 L116："`#/chat` 不动（对话是独立形态，本期边界）"；L119"不做：……不动 chat"。三处自相矛盾——实际是 3 入口废弃+chat 保留。且目标节 L28"单一 AI 中心入口"在 chat 独立存在的前提下被夸大（实为"运维向单一入口 + 对话独立形态"）。
**建议修法**：L89 改为"旧 3 入口废弃"，目标节补半句"chat 为独立形态，不在本期统一范围"。

### K2. "指令会话"既被 Run 收编又与 chat 边界不清（概念统一）
词汇表 L49 把"指令会话"列为 Run 的收编异名；§3 L96 Run 流"调查/巡检/**指令**混排"。但 chat 是对话形态且不动。UserDirective run（directive_sink 链路）与 chat 会话是什么关系？若 chat 消息产生指令 run，则同一用户动作出现在 chat 和 Run 流两处——本梳理要消灭的"同一动作两个入口"在新结构里可能原地复活。两份文档都未定义这条边界。
**建议修法**：词汇表 Run 行删除"指令会话"或改为"指令触发的 run（UserDirective）"，并明示 chat 消息是否落 agent_runs。

### K3. "现有测试套件全绿即回归网"部分不成立（内核归一 / 迁移策略）
§4 L126 声称 Phase 2 护栏 = "现有测试套件（1836+92）全绿"。但落库目标从 agent_actions 改为 agent_runs 后，**编码旧行为的测试必然失败而非回归**——例如 `crates/db/src/heartbeat.rs` 的 tick anchor/提案共存测试（L784-887）、`agent_persist_tests.rs`、`heartbeat_bridge_tests.rs`（均直接断言 agent_actions 内容）。"全绿"需要先把这批测试判定为"行为契约（保）"还是"实现细节（重写）"，spec 未做此区分。
**建议修法**：§4 补一句测试分类——列出已知会失败并重写的测试文件清单（至少上述三个），明确"全绿"指分类后的套件。

### K4. 分支名不一致（琐碎）
spec L4"建议另起 ai-consolidation 分支实施" vs CEO plan 头部"Branch: ai-brain-p0"。
**建议修法**：统一为 ai-consolidation。

---

## 3. Clarity — 不 PASS（5 项）

### Q1. "切换期双跑"字面语义危险（§4 L126）
"同输入两路径同输出，切换期双跑"——心跳 tick 有副作用（提案写入、自动执行动作）。若指生产双 runner 并行，提案/动作会双份；若指 shadow 比对（心跳路径照常执行、Run 路径只算不写），需要说明比对什么字段、差异如何告警。实现者无法不追问。
**建议修法**：改为"切换期 shadow 双跑：统一 Run 路径只产报告不落库/不落提案，与心跳路径输出比对 summary+proposals 集合；比对期 N 天/ M 次 tick 后切换"。

### Q2. 心跳→Timer 的映射方式二选一未定，且触发调度器合并碰撞（§2 L68）
"心跳 tick = `TriggerSource::Timer` 驱动的 run（timer 触发器已存在，心跳与 timer 同类）"有两种实现：(a) 复用现有 TimerTrigger 唤醒——丢失每工作区可配间隔（见 C1）；(b) 另起心跳定时器——但现有 timer 的 dedup key 是 `timer:{ws}`（manager.rs:284，registry.rs:328 的 `trigger == dedup_key` 契约），两个同源触发器共享 dedup key，在 merge window 内心跳 tick 会被普通 timer 唤醒**合并吞掉**。spec 的"调度器合并不动（已是统一通道）"（L69）是对现状的判断，不覆盖归一后**新产生**的碰撞。这是 spec 必须正面回答的设计点。
**建议修法**：spec §2 显式选定 (a) 或 (b)；若 (b)，定义心跳专用 dedup key（如 `heartbeat:{ws}`）并声明 TriggerSource 是否加变体。

### Q3. 告警→心跳的外部信号路径归一后去向未定义（§2 图）
心跳 loop 接受 `LoopSignal::External(HeartbeatSignal)`，带优先级（Normal/High/Critical）与 (thing_id, alarm_type) dedup（types.rs:31-49）。§2 的图把"告警"画进 WakeSignal，但 WakeSignal 的调度合并语义与 HeartbeatSignal 的 dedup_key 语义不同源，优先级映射未提。若告警路径本期不动（HeartbeatSignal 保留），则"一种 run"目标打折扣且图有误；若动，属未声明的行为变更。
**建议修法**：明示告警触发链本期归宿（建议：保留 HeartbeatSignal 入口、内部转 TriggerSource 新变体，优先级映射表一句话）。

### Q4. E3 概念 lint 的判定语义缺失（§4 L127）
"CI 脚本检查代码注释/UI 文案出现异名报警"——扫描哪些路径？中文异名（证据/执行历史）在散文注释里的误报如何处理？"报警"是 warn 还是 fail CI？实现者可写出一个永远报警或永远不报的脚本都符合字面。
**建议修法**：限定扫描范围（如 `web/src` 文案 + `apps/cloud/src/domains/agent` 注释）、异名清单即词汇表 L47-54 右列、首版 warn-only 一个版本后转 fail。

### Q5. E2 切换历史的存储未指定（§3 L103）
"切换历史记录（谁/何时/从何改到何）"——存哪张表？复用 judgment/brain_event 还是新表？CEO 已批准 E2，属必要行为缺失。
**建议修法**：一句话指定存储（建议新表 `trust_config_history` 或复用 brain_event 加事件类型），与 persist.rs 的 fencing upsert 语义对齐说明。

---

## 4. Scope — PASS

- E1-E6 与 CEO Scope Decisions 表逐项对应，无未批准扩展（E1 §3 L94 ↔ E1 ACCEPTED；E2 L103 ↔ E2；E3 §4 L127 ↔ E3；E4 L97 ↔ E4；E5 L105 ↔ E5；E6 L98 ↔ E6）。
- 非目标清单（§ 非目标 L134-139：chat/OpenClaw/策略门调度器判断流水线/sms-email）与 CEO "Deferred: 无"不冲突（非目标 ≠ 延期项）。
- 未发现夹带：host 分组、URL 重定向、API deprecate 均属三 phase 主方案（CEO Proposal 1 ACCEPTED）的合理内涵。
- 唯一边缘项：E2 顶栏选择器暗含信任语义扩张（见 C5）——这是定义不清而非蓄意蔓延，不计 Scope 违例。

---

## 5. Feasibility — 不 PASS（声明方法大体可行，但 3 个支撑缺口）

已核实的正向支撑：
- **Timer 触发器确实存在**：`TriggerSource::Timer`、`TimerTrigger`（crates/agent/src/runtime/thing_agent/trigger/timer.rs）——spec L68 声明为真。
- **双 runner 轨迹类型已打通**：RunReport.trace 为 `Vec<ToolTraceEntry>`（agent_runs.rs:134）且 heartbeat report.rs 解析同一格式——spec L56"T1/T9 已打通"为真。
- **chat 两处平铺属实**：`domains/agent/chat/` 与 `host/chat/` 并存（ls 核实）——spec L22 为真。
- **E5 数据面就绪**：`judgment_feedback` 在 crates/db/src/judgment.rs + domains/judgment/handler 存在。
- **E6 是真实新工作而非虚构**：内部 `AgentEvent::RunRecorded` 存在（persist.rs:4）但不推 SSE，web/src 无 `run_recorded` 消费——spec L98"新事件类型"表述准确。
- **前端映射可落地**：ai-ops.ts 读 `/heartbeat/logs`+`/heartbeat/approvals`（ai-ops.ts:151-152）、dispositions.ts 读 /judgments + SSE + 链接 `#/ai-ops/runs/{id}`（dispositions.ts:552）——与 §3 映射表吻合。

缺口：

### F1. "行为保持"在 spec 颗粒度下不可验证
C1/C2/C3 三项都是行为保持重构的实质风险点，spec 均未列。Phase 2 的验收标准（"现有套件全绿"）又部分失效（见 K3），导致实现者没有一张可核对的行为清单。重构类 spec 的最低要求是"行为保持清单 + 每项的验证方式"，本 spec 缺前者。
**建议修法**：合并不另起炉灶——把 C1/C2/C3/Q1/Q2 的答复直接写成 §2 末尾的"行为保持清单"表（行为 / 归一后归宿 / 验证测试）。

### F2. 等价对照测试的断言维度未定义
§4 L126"同输入两路径同输出"——"输出"指 HeartbeatResult JSON 全字段？提案集合？落库行？trace 条目序？LLM 非确定性下"同输出"不可字面达成（两次 LLM 调用不同结果），需要定义可比对的确定性子集或 mock LLM 接缝。
**建议修法**：指明比对在 mock 池（AgentPoolLike 已有测试接缝，loop_.rs:248 等）上进行，断言字段白名单（status/summary/proposals 集合/trace 工具名序列）。

### F3. 前端"行为不变只搬家"与路由改名存在小张力
§4 L132 称 dispositions/ai-ops 测试"行为不变只搬家"，但 dispositions.ts:552 硬编码 `#/ai-ops/runs/${traceHref}` 链接，§3 L117 路由改名 `#/ai/runs/{id}`——搬家必然含链接改写，测试也需改。琐碎但会让执行者犹豫是否漏改。
**建议修法**：§4 测试一句补"含站内路由引用改写（runs 详情链接）"。

---

## 附：两份文档间的一致性总评

CEO 摘要是对 spec 的忠实裁剪：Scope Decisions 表与 spec 四节+E1-E6 严格对应，无 CEO 已拒而 spec 保留、或 spec 有而 CEO 未批的条目。"Reviewer Concerns: 待 spec 评审结果填入"——本报告的 C1-C6 / K1-K4 / Q1-Q5 / F1-F3 即应回填内容。建议回填优先级：**C1/C2/C3 + Q2 为阻塞项**（内核归一的行为保持核心），K1/Q1 为必修措辞，其余为补强。
