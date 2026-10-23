# Spec 对抗性评审（第 2 轮复审）：AI 运行透明化

- 日期：2026-10-08
- 评审对象：
  - spec（修订版）：`docs/superpowers/specs/2026-10-08-ai-run-transparency-design.md`
  - CEO 摘要：`~/.gstack/projects/Grong-tinyiothub/ceo-plans/2026-10-08-ai-run-transparency.md`
  - 第 1 轮报告：`docs/superpowers/reports/2026-10-08-ai-run-transparency-spec-review.md`
- 本轮核实代码：`runner.rs`（MAX_TOOL_CALLS_PER_RUN=25 pub，:27）、`manager.rs`（run_id 在 run_pipeline 入口生成，:280）、`judgment_subscriber.rs`（F11 注释 :424、evidence 组装 :428、双触发点 :463/:543）、`crates/db/src/judgment.rs`（set_judgment_run_id 幂等 `WHERE run_id IS NULL`，:400）、`crates/db/src/heartbeat.rs`（insert_action_row 无 run_id 绑定，:572）、`crates/db/migrations/20260819000001_baseline.sql`（agent_actions 无 run_id 列，:1077-1087）、`20260919000001_brain_events_view_fix.sql`（patrol 提案行 NULL :56 / directive r.id :94）、`dispositions.ts`（canEvidence 排除 investigating）

**总体质量分：7 / 10**

修订忠实度高：第 1 轮 9 条发现 + 1 条备注全部在正文得到响应，两项作者裁定（C2 #3 保留重估 M；N1/C4 不截断守 F11）均按裁定方向落地而非按评审建议方向，本复审不重开已裁定项。但修订引入 **1 个新的阻断级时序矛盾（R2-1：run_id 只在 run 结束后回填，实时轮询主目标在时序上断裂）**，以及两处旧文案残留与裁定直接打架（R2-2/R2-3）。修复量小（一句话 + 删残留行 + 恢复一条测试断言），但按现文实现者会把实时进度做成死功能。总体 **FAIL**（Completeness / Consistency 未通过）。

---

## A. 第 1 轮 9 条发现 + F-a 逐条验证

| # | 结论 | 验证 |
|---|------|------|
| C1（JudgmentDto 无 run_id） | **已解决** | §1② 第 3 条"API 断面：JudgmentDto 增 run_id 字段 + 列表查询投影"；改动清单（:71）列"JudgmentDto+列表查询（run_id 断面）"；测试（:108）"JudgmentDto 列表响应携带 run_id"。但见 R2-1——DTO 有字段不等于进行中字段有值。 |
| C2（#3 前提不成立） | **已按裁定解决** | 裁定为"保留重估 M"。§2 patrol 条（:80）如实写明评审核实结论（提案行/tick 行 run_id 均 NULL、唯一有 run_id 的是 directive 分支）+ 三件套（①agent_actions 提案行补 run 关联 ②视图迁移、tick 聚合行不链接并标注 ③有 run_id 的卡片加链接）；CEO 摘要 #3 改标 `~~S~~ M`（:23）；改动清单与测试（:107/:110）同步。代码复核确认 M 估价合理：agent_actions baseline（:1077-1087）无 run_id 列、insert_action_row（heartbeat.rs:572）无 run_id 绑定，确实需要 DDL+代码+视图三处改。忠实反映裁定。 |
| C3（404 异步窗口） | **已解决** | §3（:95）新增"404 异步窗口（评审 C3）"：registry 注销与 persist 落库间窗口返回 404，前端有限重试（3 次 × 3s ≈ 9s）才放弃；测试（:110）覆盖"404 有限重试（3 次）后放弃"。窗口量级判断（毫秒到秒级）与 persist.rs 异步投影结构吻合。 |
| C4（截断只覆盖 evidence） | **裁定方向已落地，但有残留矛盾** | 裁定为"不截断守 F11"。§1④（:69）已改写为不截断：F11 原文维持、output 全量入库（report 列与 evidence 同）、显式风险接受声明（作者裁定 D4）+ 未来存储护栏走独立决策。**但** §3（:98）仍写"output 截断 2KB/条"、§4 旧测试条（:105）仍写"evidence_json 含 trace（output 截断生效）"——两处旧文案未随裁定更新，见 R2-2/R2-3。 |
| N1（F11 契约矛盾） | **已按裁定解决（同样受残留削弱）** | §1④ 现在显式引用 F11 v2 原文并声明维持，矛盾本体消除；前端可折叠 JSON 被正确归为渲染选择而非数据截断。残留问题同 C4 行。 |
| N2（notify 双触发点） | **已解决** | §1③（:65）明确两个触发点并给行号（:463 self_healable→awaiting_approval、:543 needs_human→escalated 经 escalate_to_ticket），均携带 narrative（NULL 降级 reason）；代码复核两触发点行号准确；测试（:112）挂现有 X1 notify 测试断言两触发点。CEO 摘要 :34 仍只写 needs_human——minor 文档漂移，见 R2-4。 |
| Y1（run_id 机制写错） | **已解决（但引出 R2-1）** | §1②（:55-58）改为三处联动：生成上移（dispatch 生成 + WakeSignal 携带 + run_pipeline 不再自生成）、合并扇入（回填所有关联 judgment；set_judgment_run_id 幂等 `WHERE run_id IS NULL` 经核实 judgment.rs:400 成立；队列拒绝无需回填走既有卡死兜底）、API 断面。改动清单含 manager.rs/WakeSignal。机制描述现在与代码结构（manager.rs:280 生成点、TriggerSource::Merged）吻合。**但**回填只规定在"run 结束后"，与实时轮询目标冲突——见 R2-1。 |
| Y2（响应 schema 未定义） | **已解决** | §1②（:48-54）给出 JSON 示例 `{live, budget_max, entries:[{id,name,args,output}]}`、声明进行中/已结束两源同形、定义 `output:null` 语义（配对 ToolResult 未到）、entries 全量不截断并指向 ④ 的 F11 决策；测试（:108）加 schema 形状断言。残留 nit：live:false 分支 budget_max 取值来源未写明（RunReport 不持久化该值；可用 crates/agent 的 pub 常量，F-a 已证实 cloud 可达）——实现细节，不阻断。 |
| Y3（registry 接线缺失） | **已解决** | §1②（:47）"registry 实例由 thing_agent_host 持有（与 AgentEventBus 同路径注入），经 AppState 暴露给路由层"；改动清单（:71）含"registry→AppState 接线"。 |
| F-a（预算显示上限断言） | **已解决** | §2 #10（:78）"显示 n / 25 不设上限断言——出现 27 / 25 是正常态不是 bug"，与 runner.rs fire-and-forget 拦截语义吻合；测试（:110）覆盖"n 略超时不截断显示"。 |

**小结**：9+1 条全部得到忠实响应；C4/N1 因旧文案残留只算"方向落地、执行有残留"（计入 R2-2/R2-3，不重开裁定）。

---

## B. 修订引入的新问题

### R2-1【阻断级，Completeness/Consistency】run_id 只在 run 结束后回填——实时轮询在时序上断裂

§1② 第 2 条（:57）规定"run 结束后 run_id 回填所有关联 judgment"，全文再无更早的回填点。但 §2（:77）要求 investigating 卡片"展开后启动 3s 轮询 `/agent-runs/{run_id}/trace`"，§1② 第 3 条（:58）自述"investigating 卡片拿不到 run_id 就无从轮询，这是实时进度主目标的 API 出口"。按现文时序：run 进行中 judgment.run_id 为 NULL → JudgmentDto.run_id 为 NULL → 卡片无从轮询 → **实时进度主目标死在自己修复的链路上**。C1（DTO 加字段）修对了断面，Y1（生成上移）修对了机制，但回填时点停在"结束后"，前两步修了个寂寞。

佐证这是修订引入的倒退：第 1 轮 spec 旧测试条（修订前 §4）尚有"dispatch 时 run_id 提前回填、完成时幂等再回填"的断言——方向正确；修订版 §4 新 cloud handler 条（:108）把该断言删掉了，与正文"只结束后回填"一致地错了。

修复建议（小改）：§1② 第 2 条改为"信号准入（dispatch/合并落定）时即把 run_id 回填所有关联 judgment（进行中可轮询），run 结束后幂等再回填兜底"——`WHERE run_id IS NULL` 幂等语义（judgment.rs:400）天然支持双时点；并在 §4 恢复"dispatch 时提前回填、完成时幂等再回填"测试断言。附带一个次要边界：信号在队列中排队尚未准入时 judgment 仍无 run_id，前端对"run_id 为 NULL 的 investigating 卡片"的展示态（不显示展开入口或显示"排队中"）宜一句话说清。

### R2-2【中，Consistency】§3 残留"output 截断 2KB/条"与 F11 裁定正面打架

§3（:98）"trace 体积：条数受工具预算限界；output 截断 2KB/条。"——§1④ 已裁定不截断（report 列与 evidence 同，全量入库），:98 是第 1 轮旧文案未删。同一份文档第 69 行说全量、第 98 行说截断 2KB，实现者无法判断哪个有效。修复：:98 改为"条数受工具预算限界；单条 output 不截断（F11，见 §1④），体积风险已接受"。

### R2-3【中，Consistency/Clarity】§4 测试策略新旧两组 bullet 并存且互相矛盾

:102-105（旧四条）与 :106-109（新四条）是同一批测试项的两个版本，修订应为替换却变成追加：

- :102 vs :106（crates/agent）：:106 是 :102 的超集（多了 run_id 随 WakeSignal 传递 + 合并扇入回填）；
- :103 vs :107（crates/db）：:107 多了视图迁移断言；
- :104 vs :108（cloud handler）：:108 新增 schema/DTO 断言，但**删掉了** :104 的"dispatch 时 run_id 提前回填"（与 R2-1 同源）；
- :105 vs :109（judgment_subscriber）：直接互相矛盾——:105"output 截断生效" vs :109"全量 trace（F11：不截断）"。

修复：删除 :102-105 整组，把 :104 的 dispatch 回填断言并入 :108（按 R2-1 修正后的时序）。

### R2-4【minor，Consistency】CEO 摘要 #9 范围行未同步双触发点

CEO 摘要 Accepted Scope（:34）仍写"#9 notify X1 通道 **needs_human** 通知携带 narrative"，而 spec §1③ 已按 N2 改为双触发点，且摘要自己的 Reviewer Concerns（:45）也写了"N2 双触发点…已并入 spec 正文"——摘要内部两行自相矛盾。修复：:34 改为"两个'需要你'触发点（awaiting_approval / escalated）携带 narrative"。

### R2-5【nit，Clarity】budget_max 在 live:false 分支的取值来源未写明

RunReport 只加 trace 字段，不持久化 budget_max；schema 声明两源同形即已结束分支也返回 budget_max。实现上 cloud 可直接引用 crates/agent 的 pub 常量（MAX_TOOL_CALLS_PER_RUN，runner.rs:27，F-a 已证实可达），但 spec 未写明，留给实现者推断。一句话即可闭合。

### R2-6【nit，Completeness】三件套①的 db 层改动未进改动清单

agent_actions 无 run_id 列（baseline :1077-1087，后续 ALTER 仅加 tick_id），insert_action_row/insert_tick_anchor_row（heartbeat.rs:572 起）INSERT 列清单无 run_id。三件套①因此实际需要：agent_actions 加列 DDL + heartbeat.rs 两个 INSERT 函数带 run_id + HeartbeatResult 携 run_id。改动清单（:71）只有"agent_actions+brain_events 视图迁移（#3 run 关联投影）"——读不出加列迁移（全文唯一明确的加列迁移是 judgments 的 narrative 列），crates/db 代码改动也未列。M 估价仍然成立，但清单颗粒度与其他项（连"提示词模块"都单列）不对齐。修复：改动清单加"agent_actions 加 run_id 列迁移 + heartbeat.rs INSERT 带 run_id"。

---

## 五维度评分

### 1. Completeness — FAIL（1 个阻断 + 1 nit）

- **R2-1**：run_id 回填时序缺"准入时回填"这一环，实时进度主目标的完整链路（生成→回填→DTO→轮询）在"进行中"区段断裂。
- R2-6（nit）：三件套①的 db 层改动未入清单。

### 2. Consistency — FAIL（3 个）

- **R2-1**：§1②（结束后回填）与 §2（进行中轮询）+ §1② 第 3 条自述目标直接矛盾。
- **R2-2**：:98 截断 vs §1④ 不截断（F11 裁定）。
- **R2-3**：§4 :105 vs :109 截断断言互相矛盾；:104 vs :108 回填断言一存一亡。
- R2-4（minor）：CEO 摘要 :34 vs :45 及 spec §1③。

### 3. Clarity — PASS（2 nit，不阻断）

响应 schema、output:null 语义、registry 接线、run_id 三处联动机制、"27/25 正常态"均清晰可操作；C2 三件套把 tick 聚合行"不链接并标注"的边界也写明了。nits：R2-5（budget_max 来源）、§4 新旧并存造成的"以哪组为准"歧义（主账记在 Consistency）。

### 4. Scope — PASS

4 个已接受扩展仍与 CEO Scope Decisions 表一一对应，#3 重估 M 已回写摘要（:23），延期项（#1/#2/#4/#5/#7）在 spec 非目标节（:120-125）与 CEO Deferred（:37-42）一致。R2-1/R2-6 属既有范围内的实现缺口，非蔓延。

### 5. Feasibility — PASS

全部机制经代码核实可构建：run_id 上移 + WakeSignal 穿透是常规重构（manager.rs:280 现状确认）；幂等双时点回填有 `WHERE run_id IS NULL`（judgment.rs:400）天然支撑；registry（DashMap 先例）、`#[serde(default)]` 兼容（agent_runs.rs 先例）、budget 常量 pub 可达（runner.rs:27）、agent_actions 加列 + 视图重建均为既有迁移模式。#3 重估 M 后工作量判断与实际改动面（DDL+INSERT+视图+前端两态）匹配。

---

## 结论

**总体 FAIL**（Scope / Clarity / Feasibility 通过；Completeness / Consistency 各有 R2-1 等阻断与矛盾项）。修复路径短且全部定位到行：

1. **R2-1**：§1② 第 2 条加"准入时即回填、结束后幂等再回填"+ §4 恢复 dispatch 回填断言（+ 一句话说清排队中卡片的 run_id NULL 展示态）；
2. **R2-2**：:98 删掉"output 截断 2KB/条"，改为指向 §1④ 的不截断裁定；
3. **R2-3**：删除 §4 :102-105 旧四条；
4. **R2-4**（minor）：CEO 摘要 :34 同步双触发点；
5. R2-5/R2-6（nit）：各一句话。

修掉 1-3 即可转正，无需第 3 轮全量复审（diff 级核对足够）。值得肯定：修订对两项不利裁定（#3 变 M、放弃截断护栏）没有粉饰，风险接受声明（§1④ D4）和 tick 行"不链接并标注"的边界都写得诚实；评审-修订回路本身工作正常，本轮问题集中在编辑残留与一处时序推敲不足。
