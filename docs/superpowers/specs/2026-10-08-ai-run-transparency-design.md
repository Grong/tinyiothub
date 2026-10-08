# AI 运行透明化设计（处置中心处理过程可见）

- 日期：2026-10-08
- 分支：ai-brain-p0
- 状态：已与作者分节确认（目的/范围/实时性/载体/用户文案/简化版架构/三节设计均逐项批准）

## 背景与问题

AI 中心（处置中心 `web/src/ui/views/dispositions.ts` + AI 运维中心 `ai-ops.ts`）当前只展示 AI 处理的**终态成品**：卡片流（状态徽章+标题+理由+建议动作）、统计摘要、审批按钮，以及懒加载的"证据"面板（折叠的 think 块、verdict 原文、聚合动作清单、工具调用计数/耗时/tokens）。

**处理过程是黑盒**：

- runner 每收到一个 ToolCall/ToolResult 事件就往 `RunContextInner.trace` push 一条 `ToolTraceEntry`（`crates/agent/src/runtime/thing_agent/runner.rs:226`）——逐步记录全程都在做；
- 但 `assemble_report` 只把聚合后的 `actions` 装进 `RunReport`，原始 trace 随 run 结束在内存中销毁（`crates/core/src/agent_runs.rs:104` RunReport 无 trace 字段）；
- 进行中的 trace 锁在 runner 任务内存里，无任何对外出口；`investigating` 卡片不可展开（`dispositions.ts:274-276`）。

这是 thing-agent-loop spec（`2026-07-29-thing-agent-loop-design.md`）中"物事件/定时触发静默落库、前端本期最小"的既定裁决，现在到期偿还。

## 目标（与作者确认）

- **双受众**：运营排障（全量轨迹）+ 终端用户信任（大白话叙述），同一套数据两层呈现。
- **范围**：AI 自治巡检/处置链路（告警调查 run + 心跳巡检 run）。用户指令对话链路不在本期。
- **实时性**：investigating 进行中可见进度 + 事后完整逐步轨迹。
- **载体**：卡片摘要 + 详情页全量。
- **用户文案**：LLM 生成大白话叙述（verdict 协议块加字段，零额外 LLM 调用）。

## 架构决策：活动 run 注册表 + 轮询（而非 bus 事件 + SSE 推送）

初版方案是 AgentEventBus 加 `RunTraceAppended` 变体、runner 注入 bus、SSE 加事件类型、cloud 加订阅者四道工序。作者认为复杂度过高，改走简化版：

- 进程内 `ActiveRunRegistry`（`DashMap<run_id, RunContext>`）暴露进行中 trace，只读端点查询，前端 3s 轮询；
- 实时性从秒级推送降为 3s 轮询——对几十秒到几分钟的 AI 分析过程人眼无感；
- 已知约束：内存注册表只能查本实例的 run。当前 AgentEventBus 本身就是进程内单实例架构，不引入新约束。

## 设计

### 1. 后端

**① RunReport 加 trace（事后轨迹持久化）**

- `ToolTraceEntry`（id / name / args / output:Option<String>）从 runner 挪到 `crates/core`，runner 引用 core 类型；
- `RunReport` 追加 `#[serde(default)] pub trace: Vec<ToolTraceEntry>`（向后兼容已落库旧 JSON）；
- `assemble_report` 从 `RunContextInner.trace` 搬入。随 `agent_runs.report` 落库——不加表、无迁移。

**② 活动 run 注册表 + 轨迹端点（进行中轨迹出口）**

- `crates/agent` 新文件 `active_runs.rs`：`ActiveRunRegistry`（`DashMap<run_id, RunContext>`），runner `execute` 入口注册、RAII guard 出口注销（panic/cancel 路径不泄漏）；注册表有界（上限 = 最大并发 run 数，超出拒绝注册并 warn，纯防御）。registry 实例由 thing_agent_host 持有（与 AgentEventBus 同路径注入），经 AppState 暴露给路由层。
- cloud 新增只读端点 `GET /agent-runs/{run_id}/trace`，响应 schema（进行中/已结束两源同形）：
  ```json
  { "live": true, "budget_max": 25, "entries": [{ "id": "...", "name": "read_property", "args": {...}, "output": null }] }
  ```
  - 注册表命中（进行中）→ 内存 trace + `live: true`；
  - 未命中 → 查 `agent_runs.report`（已结束）→ 持久化 trace + `live: false`；
  - 都没有 → 404。`output: null` = 配对的 ToolResult 未到（在途/取消）；entries 全量不截断（见 ④ 的 F11 决策）。`budget_max` 两分支同值，取自 crates/agent 的 pub 常量 `MAX_TOOL_CALLS_PER_RUN`（cloud 可直接引用，runner.rs:27）。
- **run_id 链路修复（评审 C1+Y1/F1，D6 裁定：回填路线 + bus 写入者）**：
  1. **写入者 = 新 bus 事件**：`AgentEventKind::RunStarted { run_id, workspace_id, problem_keys: Vec<String> }`——run_pipeline 启动时（合并落定后）经 AgentEventBus 发出；cloud 订阅者（judgment_subscriber 同域）按每个 problem_key 匹配 investigating judgment **逐个回填 run_id**（合并扇入：一 run 服务多 judgment 全覆盖）。run_id 生成点与 WakeSignal **不动**（事件自带 run_id，生成上移冗余——D6 裁定的最小实现）；
  2. **双时点回填**：RunStarted 回填 = 准入时点（进行中即可轮询，实时进度的时序闭环）；run 结束后既有回填（judgment_subscriber.rs:162）保留作幂等兜底（`set_judgment_run_id` 的 `WHERE run_id IS NULL` 天然支持双时点）。信号排队未启动时 judgment.run_id 为 NULL——前端对 run_id 为 NULL 的 investigating 卡片显示"排队中"，不提供展开入口；信号被队列拒绝/丢弃时无 RunStarted、无需回填（走既有卡死兜底/重发）；
  3. **API 断面**：`JudgmentDto` 增 `run_id` 字段 + 列表查询投影——investigating 卡片拿不到 run_id 就无从轮询，这是实时进度主目标的 API 出口。

**③ narrative（终端用户大白话）**

- verdict JSON 协议块加 `narrative` 字段（"我检查了温度曲线，发现 2 点后持续超阈，建议……"）——**协议层要求必填（提示词+宪法段约束），解析层 `Option<String>` 容错**（评审 F3：`parse_verdict` 两路径都是 serde 解析，必填 String 会让缺字段的 verdict 两路径全败 → investigation_failed 生产回归；必须与已批准的 fallback 行为一致）；
- `parse_verdict` 解析；`judgments` 表加列 `narrative TEXT`（一个小迁移）；
- 宽松 fallback / 老数据 → NULL，前端降级显示现有 reason。零额外 LLM 调用。
- **进通知（扩展 #9）**：notify 域 X1 通道的两个"需要你"触发点——`self_healable → awaiting_approval`（judgment_subscriber.rs:463）与 `needs_human → escalated`（:543，经 escalate_to_ticket 统一发出）——投递时都携带 narrative（NULL 降级 reason）。大白话出现在用户最痛的触点。仅 payload 加字段，sms/email 打桩通道不动。

**④ evidence_json 带 trace（全量，守 F11）**

`judgment_subscriber.rs:428` 组装点加 `trace: report.trace`——brain-event detail 端点直接出事后轨迹，前端不用 join agent_runs。**不截断**：F11 证据契约 v2（2026-09-16，`judgment_subscriber.rs:424`）"证据 = 完整审计记录，不截断不剥离"原文维持，trace 条目 output 全量入库（report 列与 evidence 同）。前端展示层的可折叠 JSON 是渲染选择，不是数据截断。已接受风险（作者裁定 D4）：LLM 读了大遥测 blob 会原样落 report 列——实践中 trace 条数被 `MAX_TOOL_CALLS_PER_RUN` 限界；若体积真成问题，未来以独立决策加存储护栏，不在本期。

**⑤ 心跳巡检 trace（D7 裁定：HeartbeatRunner 也装仪表）**

外部声音核实：心跳走**独立 HeartbeatRunner**（非 thing_agent），`HeartbeatResult`（core/heartbeat.rs:77）只有 summary/task_count/executed_actions/proposals——无 trace、无 run_id（有 tick_id，agent_actions.tick_id 唯一索引）。装仪表四件套：
- HeartbeatRunner 的工具调用循环按 `ToolTraceEntry` 同样式记录 trace（复用 ① 挪到 core 的类型）；
- `HeartbeatResult` 加 `#[serde(default)] pub trace: Vec<ToolTraceEntry>`；
- 落库：tick anchor 行（insert_tick_anchor_row）的 payload 加 trace（全量，守 F11 同一裁定）；
- 心跳 trace 本期**只做事后**（落库+详情透出），实时轮询不做（进行中 tick 的实时进度属 ai-ops 页，留后续）。

**改动清单**：`crates/core/src/agent_runs.rs`（挪类型+加字段）、`crates/core/src/heartbeat.rs`（HeartbeatResult 加 trace）、`crates/agent/src/runtime/thing_agent/runner.rs`（注册/注销+搬 trace）、`crates/agent` 新文件 `active_runs.rs`、`crates/agent/src/runtime/heartbeat/runner.rs`（trace 仪表化）、`events.rs`（RunStarted 变体）+ `manager.rs`（run 启动时发出，带合并后 problem_keys）、`judgment_subscriber.rs`（narrative+trace 进 evidence、RunStarted 订阅扇入回填）、`JudgmentDto`+列表查询（run_id 断面）、`heartbeat.rs`(db) tick anchor payload 带 trace + brain_events 视图迁移（#3 tick_id 投影）、1 个 judgments 加列迁移、提示词模块、1 个 HTTP handler + registry→AppState 接线、notify X1 两触发点 payload。

### 2. 前端两层展示

**卡片层（dispositions.ts 改造）**

- `investigating` 卡片变为可展开：展开后启动 3s 轮询 `/agent-runs/{run_id}/trace`，渲染进度时间线——每个工具调用一行（图标 + 工具名中文映射 + 参数摘要），`output` 未到的显示进行中 spinner；收起或卡片变终态时停轮询。
- **预算进度（扩展 #10）**：investigating 卡片时间线头部显示"工具调用 n / budget_max"（n = trace 长度，budget_max 随端点响应下发）。预算拦截是 fire-and-forget（在途调用可使条数略超 MAX），显示"n / 25"不设上限断言——出现"27 / 25"是正常态不是 bug。
- 终态卡片摘要区显示 `narrative`（NULL 降级为现有 reason）。
- **patrol 卡片轨迹链接（扩展 #3，评审 C2 重估为 M；D7 后改 tick_id 关联）**：评审核实 brain_events 视图现状——patrol 提案行/tick 聚合行 `run_id` 均为 NULL；外部声音进一步核实心跳**没有 run_id**（只有 tick_id）。因此 #3 落为三件套：① §1⑤ 的 HeartbeatRunner trace 装仪表（trace 落 tick anchor 行）；② brain_events 视图迁移，patrol 提案行投影 **tick_id**（tick 聚合行按小时 GROUP BY、天然不对应单一 tick，**不链接**并标注）；③ 有 tick_id 的 patrol 提案卡片加"查看完整轨迹"链接——详情数据源走 brain-events detail（patrol 证据透出 tick trace），不需要 run_id。
- SSE 通道不动（仍靠 judgment_judged/judgment_updated 刷新信号 + 现有轮询/断连细条模式）。

**详情层（新视图）**

- 展开面板底部加"查看完整轨迹"→ 新路由 `#/ai-ops/runs/{run_id}`，全宽时间线：think 块（折叠）、每次工具调用的完整参数+输出（可展开 JSON）、✓/✗ 验证标记、尾部 verdict 原文、元信息（耗时/tokens/工具数）。
- **trace 导出（扩展 #8）**：详情页加"导出 JSON"按钮——完整轨迹+元信息复制到剪贴板，排障时可贴给同事或喂给 LLM。
- 数据源：进行中走 trace 端点轮询；已结束走 `/brain-events/{id}` detail（evidence 已带 trace）。

**前端新建**：trace API 客户端、进度时间线组件（卡片层/详情层共用，**单一组件 props 控制详略密度**，不做两份实现）、run 详情视图。设计约束（S11）：走 web/DESIGN_SYSTEM.md 令牌（禁硬编码色值/像素）；时间线行不套 role=button（X7 先例）、spinner 加 aria-live；桌面优先，移动端沿用 X3 延期。

### 3. 错误处理

- **注册表泄漏**：RAII guard（Drop 注销）；cancel/panic 路径覆盖。
- **轮询竞态**：run 在两次轮询间完成 → 端点自动落到 report 分支返回 `live:false` + 完整 trace，前端收到 `live:false` 即停轮询并刷新卡片。无状态缝隙。
- **轮询生命周期竞态（S2/S4）**：快速展开/收起、SSE 整表刷新重渲染会启停多个轮询实例——前端轮询必须带**代际守卫**（generation counter，收起即 abort，过期实例的响应直接丢弃），防串数据。
- **准入回填失败（S2）**：准入时 `set_judgment_run_id` 失败 → warn log（带 judgment_id/run_id）并继续——结束后的幂等兜底回填会补上，用户只看到短暂"排队中"后自愈。
- **404 异步窗口（评审 C3）**：registry 在 execute 出口注销，report 经总线异步落库（persist.rs）——两者间存在"run 已结束、registry 已注销、report 未落库"的窗口，端点返回 404。前端对 404 做**有限重试**（3 次，间隔 3s，共约 9s）才放弃并显示"轨迹不可用"——窗口是毫秒到秒级，有限重试足以闭合，误报不会永久化。
- **trace 端点 404（真性）**：run_id 错误或实例重启后查进行中的 run → 重试耗尽后前端显示"轨迹不可用"并停轮询，不 crash。
- **narrative 缺失**：老数据/fallback → 降级 reason。
- **trace 渲染安全（S3）**：trace 的 args/output 含 LLM 生成与设备数据——前端**一律纯文本/转义渲染，禁止走 md()**（F8 markdown 收口只覆盖 reason，trace 不继承），防注入型 XSS。
- **trace 端点鉴权（S3）**：与 `/brain-events` 同模式强制 workspace 成员鉴权——run_id 虽是 UUID 不可猜，鉴权不依赖不可猜性。
- **trace 体积**：条数受工具预算限界；单条 output 不截断（F11 裁定，见 §1④）——大遥测 blob 体积风险已由作者裁定接受（D4），若成问题未来走独立决策加存储护栏。

### 4. 测试策略（先写失败测试）

- **crates/agent**：execute 期间注册表可查、trace 逐步增长；结束后注销且 report.trace 完整；cancel 路径也注销；run 启动时发出 RunStarted（合并信号携带全部 problem_keys）；HeartbeatRunner trace 记录进 HeartbeatResult。
- **crates/db**：迁移测试——judgments 加列后旧行 narrative 为 NULL；brain_events 视图迁移后 patrol 提案行投影 tick_id、tick 聚合行仍为 NULL；tick anchor payload 含 trace。
- **cloud handler 测试**：trace 端点三分支（live / 持久化 / 404）+ 响应 schema 断言（live/budget_max/entries 形状）；JudgmentDto 列表响应携带 run_id；RunStarted 扇入回填（一 run 多 judgment 全覆盖）+ 结束后既有回填幂等兜底（双时点）。
- **judgment_subscriber 测试**：verdict 带 narrative → 落列；evidence_json 含 **全量** trace（F11：不截断）；fallback 无 narrative 不炸。
- **前端 vitest**：investigating 卡片可展开 + 轮询启动/停止；`live:false` 停轮询；404 有限重试（3 次）后放弃；轮询代际守卫（快速展开/收起 + SSE 刷新不串数据）；run_id 为 NULL 的 investigating 卡片显示"排队中"且无展开入口；时间线组件两模式渲染；trace 纯文本渲染（不走 md()）；narrative 缺失降级 reason；patrol 提案卡片轨迹链接（有/无 run_id 两态，tick 行无链接）；预算进度"n / budget_max"（含 n 略超时不截断显示）；导出按钮写剪贴板。
- **提示词**：现有 verdict 协议测试夹具加 narrative 必填断言；**跑 eval 基线**（7a2b5f87 转绿的 91.7% 那条）并加 narrative 用例——CLAUDE.md prompt/LLM 约定，防协议回归。
- **notify**：两个"需要你"触发点（awaiting_approval / escalated）payload 均携带 narrative（NULL 降级 reason）的断言，挂在现有 X1 notify 测试上。

## CEO 评审增补（2026-10-08，SELECTIVE EXPANSION）

0G cherry-pick 仪式中作者接受全部 4 个扩展（#3/#8/#9/#10）。Spec Review 第 1 轮（对抗性评审，6/10 FAIL，9 条发现全部对代码核实）后作者裁定：**#3 保留但重估为 M**（评审 C2：patrol 行 run_id 为 NULL，需 agent_actions 关联+视图改造三件套，见 §2）；**不截断守 F11 原文**（评审 N1/C4，见 §1④，已接受存储体积风险）。其余 7 条发现（C1/Y1 run_id 链路、C3 404 窗口、Y2 响应 schema、Y3 wiring、N2 双触发点、F-a 预算显示）已并入正文对应节。第 2 轮复审（7/10）6 条新发现全部为编辑残留与时序推敲：**R2-1 run_id 改为准入时即回填+结束后幂等兜底**（§1② 第 2 条，否则实时轮询断在"进行中"区段）、R2-2/R2-3 截断旧文案与重复测试条清除、R2-4 CEO 摘要 #9 同步双触发点、R2-5/R2-6 各一句话补全——均已修复并经 diff 级核对。外部声音（native fallback 超时未完成，按其可核实线索由主审查自行验证）再出 3 条 Critical/High：**F1 准入回填无写入者**（agent crate 调度路径无 DB 访问）→ 作者裁定 D6=回填路线+新 bus 事件 RunStarted 写入者（§1②，生成点/WakeSignal 不动）；**F2 心跳走独立 HeartbeatRunner 无 trace** → 作者裁定 D7=心跳也装仪表（§1⑤，#3 改 tick_id 关联）；**F3 narrative 必填与 fallback 矛盾** → 解析层 Option 容错（§1③）。延期项（进 TODOS.md）：#1 trace→eval 反馈闭环、#2 对话链路接入同一 trace 端点、#4 SSE 推送替代轮询、#5 AI 运维页 trace 统计、#7 run 重放调试、**心跳 tick 实时进度**（D7 只做事后）。

## 实现时序要点（CEO 评审 0I）

- **顺序**：ToolTraceEntry 挪 core 第一刀 → 三个迁移（judgments 加 narrative 列、agent_actions 加 run_id 列、brain_events 视图重建——走 DDL 守卫加白的重建表模式，**视图重建必须配逆迁移**）→ runner/manager/registry → 端点+DTO → 前端。
- **埋伏**：① 前端 case-converter 运行时/类型级不对称——`budget_max`/`run_id` 等字段走惯例解法：后端 DTO `#[serde(rename_all="camelCase")]` + 前端类型直接写 camelCase；② live:false 分支从 report JSON 列提取 trace 用 serde 反序列化整个 report，别写 SQL JSON 函数；③ web 构建/测试需 Node 24；④ 合并信号扇入名单看 scheduler 的 Merged 实现。
- **门禁**：pre-push 全量 fmt+clippy+测试约 10min；本地先 `cargo clippy --workspace` 与 `tsc --noEmit`（vite build 不查类型，CI 前端守卫才查）。
- **估计**：human ~3-4 天 / CC+gstack ~1 天。

## 非目标（YAGNI）

- 用户指令对话链路的过程展示（**延期 #2**，进 TODOS）；
- SSE 实时推送轨迹（**延期 #4**，轮询已够）；
- 独立轨迹表（随 report 落库已够）；
- 轨迹的跨实例查询、长期归档策略（沿用 agent_runs 现有生命周期）；
- sms/email 通知通道（仍是打桩；#9 只改 X1 已有通道的 payload）；
- trace→eval 反馈闭环（**延期 #1**）、run 重放调试（**延期 #7**，涉及二次触发自治动作，需单独威胁建模）；
- 心跳 tick 的**实时**进度轮询（D7 裁定心跳 trace 本期只做事后；实时属 ai-ops 页后续项）。

---

# CEO 审查输出（/plan-ceo-review 2026-10-08，SELECTIVE EXPANSION）

## NOT in scope

**延期（已入 TODOS.md）**：#1 trace→eval 反馈闭环（依赖 trace 数据积累）；#2 对话链路接入 trace 端点；#4 SSE 推送替代轮询（D2 未选）；#5 AI 运维页 trace 统计面板（D2 未选）；#7 run 重放调试（需威胁建模）；心跳 tick 实时进度轮询（D7 裁定本期只做事后）；trace 存储护栏（D4 风险接受的触发式后续）。
**拒绝**：active-runs problem_key 查找端点替代 run_id 回填（D6 未选）；SSE/bus 全量推送方案（架构决策节否决）；独立轨迹表；截断入库（D4 否决）。

## What already exists

- `ToolTraceEntry`/`RunContextInner.trace` 逐步记录（runner.rs:58/226）——**复用**，类型下放 core；
- `agent_runs.report` 持久化通道——复用加字段；`persist.rs` insert-once 幂等先例；
- `set_judgment_run_id` 幂等回填（judgment.rs:400）+ judgment_subscriber 结束后回填（:162）——复用作第二时点兜底；
- SSE 刷新信号 + 前端轮询/断连细条模式——不动；
- evidence 懒加载面板 + segmentEvidence（think 块切分）——详情层复用；
- verdict 协议 parse_verdict 双路径 + eval 夹具——扩展 narrative；
- agent crate 无 DB、cloud persist.rs 订阅 bus 的既有分层——RunStarted 事件沿用此分层；
- heartbeat tick_id 唯一索引（insert-once 幂等）——#3 关联复用。

## Dream state delta

```
CURRENT: 终态成品卡片       THIS PLAN:                     12-MONTH IDEAL 余量:
证据=聚合成品       --->    实时进度+逐步轨迹+大白话  --->   #2 对话链路接入（端点已就绪）
investigating 黑盒          （巡检+处置+心跳装仪表）         #1 trace→eval 闭环（数据已积累）
                            trace 端点=平台地基             #4 SSE 推送（轮询已可用）
```

## Error & Rescue Registry

```
CODEPATH                        | FAILURE MODE                  | RESCUED? | RESCUE ACTION                    | USER SEES
GET /agent-runs/{id}/trace live  | run_id 不存在/已注销           | Y        | 404 → 前端 3 次重试               | 重试后"轨迹不可用"
GET /agent-runs/{id}/trace ended | report JSON 反序列化失败         | Y        | 500 + error log(带 run_id)        | "轨迹不可用"
                                | DB 查询失败                     | Y        | 500 + error log                  | "轨迹不可用"
RunStarted 订阅扇入回填           | DB 写失败（某 judgment）        | Y        | warn log；结束后既有回填补上       | 短暂"排队中"后自愈
结束后回填（既有）               | DB 写失败                       | Y        | 既有 warn log（judgment_subscriber:163）| 详情页轨迹仍可走 evidence
parse_verdict narrative          | 缺失/畸形/LLM 拒绝              | Y        | Option → NULL 降级 reason         | reason
notify payload narrative         | narrative NULL                 | Y        | 降级 reason                      | reason
前端轮询                          | 网络错误/SSE 断连               | Y        | 既有断连细条 + 自动重连           | "实时更新已断开"
前端轮询                          | 展开/收起/SSE 刷新竞态          | Y        | 代际守卫（generation counter+abort）| 无串数据
registry 注册                    | 超并发上限                      | Y        | 拒绝注册 + warn（防御，正常不触发） | 无（防御路径）
```

## Failure Modes Registry

```
CODEPATH                    | FAILURE MODE        | RESCUED? | TEST? | USER SEES?      | LOGGED?
trace endpoint live          | 404 异步窗口        | Y(重试)  | Y     | 轨迹不可用(重试后) | Y(debug)
trace endpoint ended         | 反序列化失败        | Y(500)   | Y     | 轨迹不可用        | Y(error)
RunStarted 回填              | DB 写失败           | Y(兜底)  | Y     | 排队中→自愈       | Y(warn)
parse_verdict                | narrative 缺失      | Y(Option)| Y     | 降级 reason       | 既有 fallback 标记
前端轮询                     | 代际竞态            | Y(守卫)  | Y     | 无感              | —
HeartbeatRunner trace        | tick anchor 落库失败 | Y(既有重试)| Y    | 提案无轨迹链接     | Y(既有)
```
**CRITICAL GAPS: 0**

## Scope Expansion Decisions（引用 CEO 计划 0G 记录）

- **Accepted**: #3 patrol 轨迹链接（M，D3 重估+D7 改 tick_id）、#8 trace 导出 JSON、#9 narrative 进 notify（双触发点）、#10 预算进度条
- **Deferred**: #1、#2、#4、#5、#7 + 心跳 tick 实时进度（见 TODOS.md）
- **Skipped**: 无（D2 全选）

## Diagrams

**1. 系统架构**（见审查聊天记录；要点：manager→scheduler→runner→registry/bus→cloud subscriber→DB→HTTP→前端轮询，单进程内）

**2. 数据流（trace 端点，含 shadow paths）**
```
run_id ──► registry 查 ──命中──► 内存 trace + live:true ──► 前端渲染
              │未命中
              ▼
        agent_runs.report 查 ──命中──► 反序列化取 trace + live:false
              │未命中/反序列化失败                                     │失败
              ▼                                                     ▼
            404 ──► 前端 3 次重试 ──仍 404──► "轨迹不可用"      500 + error log
nil: run_id NULL（排队中）→ 卡片显示"排队中"不展开 │ empty: entries:[] → "启动中"
```

**3. 状态机（judgment.run_id 生命周期）**
```
NULL(排队中,不可展开) ──RunStarted 回填──► run_id(可轮询 live)
      │                                       │ run 结束
      │(信号丢弃: 保持 NULL, 既有卡死兜底)      ▼
      └──────────────────────────────► 结束后回填(幂等 no-op, WHERE run_id IS NULL)
非法跃迁「覆盖既有 run_id」由 NULL 守卫阻止
```

**4. 错误流**：见 Error & Rescue Registry 的 RESCUE ACTION 列；所有失败路径均有 log + 用户可见终态，无静默。

**5. 部署顺序**
```
迁移①judgments+narrative ──► 迁移②tick anchor payload(如需DDL) ──► 迁移③视图重建(+逆迁移)
        ──► 部署代码(core→agent→cloud→web) ──► 冒烟: 调查 run 触发→卡片排队中→进度→完成
混跑窗口: 旧代码+新视图=无害(只加列)；新代码+旧视图=#3 链接不渲染(前端判空)
```

**6. 回滚流程**
```
代码回退(新列/新字段闲置无害) ──► 如需: 视图逆迁移 ──► narrative/trace 数据保留(可加列不回滚)
提示词回退独立可控(narrative 说明不影响旧解析——Option 容错)
```

## Stale Diagram Audit

本计划触及的文件中无既有 ASCII 图；spec 自身新增 6 幅（本文件），judgment.rs:10 的状态机注释未受影响（本计划不改 judgment 状态机）。0 stale。

## Implementation Tasks

- [ ] **T1 (P1, human: ~2h / CC: ~15min)** — core — ToolTraceEntry 挪 core + RunReport 加 trace 字段 + assemble_report 搬入
  - Surfaced by: spec §1①；S5 代码组织
  - Files: crates/core/src/agent_runs.rs, crates/agent/src/runtime/thing_agent/runner.rs
  - Verify: `cargo test -p tinyiothub-core`；runner 测试 report.trace 完整
- [ ] **T2 (P1, human: ~2h / CC: ~20min)** — db — 迁移：judgments 加 narrative 列 + brain_events 视图重建（tick_id 投影，含逆迁移，走 DDL 守卫加白模式）
  - Surfaced by: spec §1③/§2 #3；S1 WARNING（逆迁移）
  - Files: crates/db/migrations/
  - Verify: 迁移测试（旧行 NULL、提案行 tick_id、tick 行 NULL）
- [ ] **T3 (P1, human: ~2h / CC: ~15min)** — agent — ActiveRunRegistry（DashMap + RAII guard + 有界）
  - Surfaced by: spec §1②；S2 registry 行
  - Files: crates/agent/src/runtime/thing_agent/active_runs.rs, runner.rs
  - Verify: 注册/注销/cancel/泄漏测试
- [ ] **T4 (P1, human: ~3h / CC: ~25min)** — agent+cloud — RunStarted 事件（events.rs 变体 + manager.rs run 启动发出带 problem_keys）+ cloud 订阅扇入回填
  - Surfaced by: spec §1②（D6）；外部声音 F1
  - Files: crates/agent/src/runtime/events.rs, thing_agent/manager.rs, apps/cloud/src/domains/agent/host/judgment_subscriber.rs
  - Verify: 一 run 多 judgment 全回填；结束后回填幂等兜底
- [ ] **T5 (P1, human: ~3h / CC: ~25min)** — cloud — trace 端点（live/ended/404 三分支 + schema）+ JudgmentDto run_id 断面 + registry→AppState 接线 + workspace 鉴权
  - Surfaced by: spec §1②；评审 C1/Y2/Y3；S3 鉴权行
  - Files: apps/cloud/src/domains/（新 handler）, domains/judgment/dto.rs
  - Verify: handler 三分支 + schema + DTO 断言测试
- [ ] **T6 (P1, human: ~2h / CC: ~15min)** — cloud — narrative Option 解析落列 + evidence 全量 trace（守 F11）
  - Surfaced by: spec §1③④；外部声音 F3
  - Files: apps/cloud/src/domains/agent/host/judgment_subscriber.rs
  - Verify: verdict 夹具 + fallback + 全量不断言测试
- [ ] **T7 (P1, human: ~4h / CC: ~40min)** — web — 卡片层：investigating 展开 + 3s 轮询 + 代际守卫 + 预算条 + 排队中态 + narrative 降级 + 404 有限重试
  - Surfaced by: spec §2 卡片层；S2/S4 轮询竞态；S11 设计约束
  - Files: web/src/ui/views/dispositions.ts/.css/.test.ts, web/src/api/（新 trace 客户端）
  - Verify: vitest（含代际守卫/重试/排队中/降级）
- [ ] **T8 (P1, human: ~4h / CC: ~40min)** — web — run 详情页：路由 + 全宽时间线（单组件 props 密度）+ 导出 JSON + trace 纯文本渲染
  - Surfaced by: spec §2 详情层；S3 XSS 行；S11
  - Files: web/src/ui/views/（新 run-detail）, web/src/ui/app.ts
  - Verify: vitest（导出剪贴板/纯文本断言）
- [ ] **T9 (P2, human: ~4h / CC: ~35min)** — agent — HeartbeatRunner trace 仪表化 + HeartbeatResult.trace + tick anchor payload 落库
  - Surfaced by: spec §1⑤（D7）；外部声音 F2
  - Files: crates/agent/src/runtime/heartbeat/runner.rs, crates/core/src/heartbeat.rs, crates/db/src/heartbeat.rs
  - Verify: trace 记录 + 落库测试
- [ ] **T10 (P2, human: ~1h / CC: ~10min)** — web+db — #3：patrol 提案卡片"查看完整轨迹"链接（有 tick_id 两态，tick 行无链接）
  - Surfaced by: spec §2 #3
  - Files: web/src/ui/views/dispositions.ts
  - Verify: vitest 两态
- [ ] **T11 (P2, human: ~1h / CC: ~10min)** — cloud — notify X1 两触发点 payload 带 narrative（NULL 降级）
  - Surfaced by: spec §1③ #9；评审 N2
  - Files: apps/cloud/src/domains/agent/host/judgment_subscriber.rs（spawn_needs_you_notify/escalate_to_ticket 路径）
  - Verify: 挂现有 X1 notify 测试双触发点断言
- [ ] **T12 (P1, human: ~1h / CC: ~15min)** — prompt — verdict 协议 narrative 说明（协议必填/解析容错）+ eval 基线跑通 + narrative 用例
  - Surfaced by: spec §1③；S6 eval GAP
  - Files: 提示词模块 + eval 夹具
  - Verify: eval 基线 ≥ 7a2b5f87 的 91.7%

## Completion Summary（prepared）

  +====================================================================+
  |            MEGA PLAN REVIEW — COMPLETION SUMMARY                   |
  +====================================================================+
  | Mode selected        | SELECTIVE_EXPANSION                         |
  | System Audit         | 分支 90 文件 diff 为历史线;审查对象=新 spec  |
  | Step 0               | SEL 模式(D1);0A/0B/0C 通过;4 扩展接受(D2)    |
  | Section 1  (Arch)    | 0 issues（1 WARNING: 视图逆迁移→已修）       |
  | Section 2  (Errors)  | 9 error paths mapped, 1 GAP→已修(轮询代际)   |
  | Section 3  (Security)| 3 issues found, 1 High(XSS 渲染)→已修        |
  | Section 4  (Data/UX) | 5 edge cases mapped, 1 unhandled→已修        |
  | Section 5  (Quality) | 0 issues found                              |
  | Section 6  (Tests)   | Diagram produced, 2 gaps→已修(代际测试/eval) |
  | Section 7  (Perf)    | 0 issues found                              |
  | Section 8  (Observ)  | 0 gaps found                                |
  | Section 9  (Deploy)  | 1 risk(迁移顺序/混跑)→已修                   |
  | Section 10 (Future)  | Reversibility: 4/5, debt items: 0(新)        |
  | Section 11 (Design)  | 3 issues(设计令牌/可达性/响应式)→已约束      |
  +--------------------------------------------------------------------+
  | NOT in scope         | written (11 items: 7 延期 TODOS + 4 拒绝)    |
  | What already exists  | written (8 项复用)                           |
  | Dream state delta    | written                                     |
  | Error/rescue registry| 9 rows, 0 CRITICAL GAPS                     |
  | Failure modes        | 6 total, 0 CRITICAL GAPS                    |
  | TODOS.md updates     | 7 items written                             |
  | Scope proposals      | 9 proposed, 4 accepted (SEL)                |
  | CEO plan             | written                                     |
  | Outside voice        | codex unavailable(broken_install);           |
  |                      | native fallback timeout(无可计覆盖)           |
  | Lake Score           | N/A（全部决策为 kind 类,无 coverage 评分题）  |
  | Diagrams produced    | 6 (架构/数据流/状态机/错误流/部署/回滚)       |
  | Stale diagrams found | 0                                           |
  | Unresolved decisions | 0                                           |
  +====================================================================+


## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 2 | CLEAR | 9 proposals, 4 accepted, 5 deferred |
| Outside Review | codex (auto) | Independent 2nd opinion | 4 | unavailable | codex CLI broken (ENOENT); native fallback 5min timeout — no completed external review |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 2 | STALE | last clean 2026-09-29 @0b3df3b5 — 9 days old, 2 commits since, predates this review |
| Design Review | `/plan-design-review` | UI/UX gaps | 0 | — | finding count not recorded |
| DX Review | `/plan-devex-review` | Developer experience gaps | 0 | — | finding count not recorded |

- **OUTSIDE COVERAGE:** codex, plan-review phase, **unavailable** — CLI broken_install (vendor binary ENOENT, reinstall: `npm install -g @openai/codex`); native Plan-subagent fallback exceeded the 5-minute bounded wait (stopped, no credit). Its in-flight leads (F1 回填无写入者 / F2 心跳独立 runner 无 trace / F3 narrative 必填矛盾) were independently verified against repo code by the native review and resolved via D6/D7 and spec fixes — but no completed external review exists, and no outside coverage is claimed.
- **VERDICT:** CEO CLEARED — eng review required (stale, and this review expanded scope + changed architecture: RunStarted event, registry, HeartbeatRunner instrumentation).

NO UNRESOLVED DECISIONS
