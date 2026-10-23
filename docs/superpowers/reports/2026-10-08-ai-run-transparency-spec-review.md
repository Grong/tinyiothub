# Spec 对抗性评审：AI 运行透明化

- 日期：2026-10-08
- 评审对象：
  - spec：`docs/superpowers/specs/2026-10-08-ai-run-transparency-design.md`
  - CEO 摘要：`~/.gstack/projects/Grong-tinyiothub/ceo-plans/2026-10-08-ai-run-transparency.md`
- 核实代码：`crates/agent/src/runtime/thing_agent/runner.rs`、`manager.rs`、`crates/core/src/agent_runs.rs`、`apps/cloud/src/domains/agent/host/judgment_subscriber.rs`、`apps/cloud/src/domains/judgment/dto.rs`、`crates/db/migrations/20260919000001_brain_events_view_fix.sql`、`web/src/ui/views/dispositions.ts`

**总体质量分：6 / 10**

骨架扎实：现状诊断准确（trace 在 `runner.rs:226` 逐步记录、`RunReport` 无 trace 字段 `agent_runs.rs:104-122`、run_id 完成后才回填 `judgment_subscriber.rs:162`——全部与代码吻合），架构简化决策合理，测试策略具体。但有两处与代码事实直接冲突的硬伤（patrol 卡片 run_id 不存在、dispatch 时 run_id 尚未生成）、一个 API 断面遗漏（JudgmentDto 无 run_id）、一个与既有证据契约矛盾的截断决策。总体 **FAIL**（五维度未全 PASS）。

---

## 1. Completeness — FAIL（4 个问题）

**C1. investigating 卡片拿不到 run_id——实时进度目标在 API 断面断裂。**
spec §1②（:52）修复了 DB 层 run_id 回填时序，§2（:71）让 investigating 卡片"展开后启动 3s 轮询 `/agent-runs/{run_id}/trace`"。但 investigating 卡片数据源是 `/judgments`（`dispositions.ts:9` "alarm 源走 /judgments"），而 `JudgmentDto`（`apps/cloud/src/domains/judgment/dto.rs:7-23`）**没有 run_id 字段**。即使 DB 提前回填，列表响应也不带 run_id，前端无从轮询。spec 改动清单（:65）未列 judgment DTO/handler 改动。两份文档均缺失。
建议：spec 改动清单加 `JudgmentDto` 增 `run_id` 字段 + 列表查询投影。

**C2. 已接受扩展 #3 的前提不成立——patrol 卡片根本没有 run_id。**
spec §2（:74）与 CEO 摘要（:23, :32）均称"patrol_ok/提案卡片（有 run_id 的）加链接"。但 brain_events 视图（`20260919000001_brain_events_view_fix.sql`）中：patrol 提案行 `NULL AS run_id`（:56）；patrol_tick 聚合行列清单第 12 位 run_id 同为 NULL（:107-130，且该行是按小时 GROUP BY 的聚合，天然不对应单一 run）。视图中唯一有 `r.id AS run_id` 的是 **directive** 分支（:94）——而对话链路恰是本期非目标。即扩展 #3 按现状无卡可链；实现它需要给 `agent_actions` 提案行/tick 行补 run 关联并改视图——这超出"只加链接"（CEO :23 "S" 工作量）的估价，且两份文档都只字未提。
建议：要么把 #3 降级为"仅对有 run_id 的卡片（现状=无）"并如实标注需要视图/agent_actions 改造、工作量重估为 M；要么延期 #3。

**C3. "run 刚结束、report 尚未持久化"窗口的 404 未覆盖。**
spec §3（:88）的轮询竞态只处理"注册表未命中 → 落到 report 分支"。但 report 持久化走事件总线异步投影（`persist.rs:382` 起），注册表在 execute 出口即注销——两事件之间存在窗口：run 已结束、registry 已注销、report 尚未落库，端点返回 404。而 §3（:89）规定前端对 404"显示轨迹不可用并停轮询"——永久性误报"轨迹不可用"。
建议：404 分支前端做有限次重试（如 3 次/9s）再放弃；或端点对"近期 run_id"返回 202；或 registry 注销延迟到 persist 确认后。spec §3 需补此 case 及测试。

**C4. output 截断只覆盖 evidence，未覆盖 report 落库。**
spec §1④（:63）规定 `output` 进 evidence 前截断 2KB/条，但 ①（:43）中 `RunReport.trace` 随 `agent_runs.report` 落库时**未提截断**——LLM 读了大遥测 blob 会原样撑爆 report JSON 列。trace 端点 live 分支返回内存 trace 是否截断也未说明（见 Clarity Y2）。
建议：在 `assemble_report` 搬入 trace 时统一截断 2KB/条（单点处理，evidence 与端点自然继承），并在 §1① 写明。

## 2. Consistency — FAIL（2 个问题）

**N1. 与既有证据契约 F11 直接矛盾。**
`judgment_subscriber.rs:424-427` 注释载明"F11 证据契约 v2（2026-09-16）：证据 = 完整审计记录，**不截断不剥离**"。spec §1④（:63）要求 trace 进 evidence 且 output 截断 2KB/条——与契约条文正面冲突。spec 未声明修订 F11。
建议：spec 显式写明"修订 F11 契约：trace 条目 output 截断为例外，理由=遥测 blob 体积"，或截断只落在渲染/端点层、evidence 存全量。

**N2. notify 携带 narrative 的范围自相矛盾（minor）。**
spec §1③（:59）说"X1 通道投递 needs_human'需要你'通知时携带 narrative"。但代码中"需要你"通知有两个触发点：`self_healable → awaiting_approval`（`judgment_subscriber.rs:463`）和 `needs_human → escalated`（:543，注释 :461 明确"needs_human 的通知在 escalate_to_ticket 统一发出"）。spec 只提 needs_human，self_healable 路径带不带 narrative 未言明；CEO 摘要（:34）同样只写 needs_human。
建议：spec 明确两路径都携带（同为"需要你"触点），或说明仅 needs_human 的理由。

## 3. Clarity — FAIL（3 个问题）

**Y1. run_id 时序修复的机制写错了对象，实现者无法照做。**
spec §1②（:52）："改为 dispatch 调查 run 时（dispatch 方知道 judgment_id 的时刻）即回填"。但 run_id 在 `run_pipeline` 入口才生成（`manager.rs:280` `format!("run_{}", Uuid::new_v4())`）——dispatch 方（发 WakeSignal 处）**根本不知道 run_id**；且信号经 scheduler 排队/合并（`TriggerSource::Merged`），一个 run 可服务多个 judgment，"dispatch 时一对一回填"的模型不成立。修复实际要求：run_id 生成上移到 dispatch 点、给 WakeSignal 加字段、穿透 scheduler 队列、处理合并扇入与队列满丢弃时的孤儿 run_id——spec 均未提。
建议：§1② 改写为"run_id 生成上移至 dispatch 点并随 WakeSignal 传递；合并信号时 run_id 回填所有关联 judgment；信号被队列拒绝时无需回填"，并列入改动清单（manager.rs/WakeSignal 类型）。

**Y2. trace 端点响应形状未定义。**
live 分支返回"当前内存 trace + live: true"——output 是否截断？`MAX_TOOL_CALLS_PER_RUN` 预算常量"随端点响应下发"（:72）只在扩展 #10 顺带一提，响应 schema（字段名、截断策略、budget 字段）无定义。详情层已结束走 `/brain-events/{id}`、进行中走 trace 端点，两源形状是否对齐也未说。
建议：§1② 补一个响应 JSON 示例（`{live, budget_max, entries: [{id,name,args,output}]}` + 截断规则）。

**Y3. registry 到 HTTP 层的接线缺失（minor）。**
`ActiveRunRegistry` 在 `crates/agent`，HTTP handler 在 `apps/cloud`——registry 实例如何经 thing_agent_host/service 暴露给路由层，改动清单（:65）只有"1 个 HTTP handler"，未提 wiring。实现者需自己摸索。
建议：改动清单加一行 wiring 说明。

## 4. Scope — PASS

4 个已接受扩展（#3/#8/#9/#10）与 CEO Scope Decisions 表逐一对应，均已并入 spec 正文且未超表；延期项（#1/#2/#4/#5/#7）在 spec 非目标节（:107-114）与 CEO Deferred 清单（:37-42）一致。未发现未批准蔓延。（#3 的实现成本被低估——见 C2——但那是完整性/可行性问题，不是蔓延。）

## 5. Feasibility — FAIL（1 个问题 + 2 个备注）

**F1. spec 断言的修复点在代码中不成立（与 Y1 同源）。**
"dispatch 方知道 judgment_id 的时刻即回填 run_id"——dispatch 点无 run_id 可填（`manager.rs:280`）。方案本身可构建（run_id 上移 + 信号穿透是常规重构），但按 spec 现文实现者第一步就会撞墙。判 FAIL 是因为 spec 把非平凡重构描述成一行时序调整，且未触及合并信号扇入这一真实结构。
建议：同 Y1；另需确认 `set_judgment_run_id` 的 `WHERE run_id IS NULL`（`crates/db/src/judgment.rs:400`）幂等语义在"一 run 多 judgment"下仍正确（现状已幂等，风险低）。

**备注 F-a（不阻断）**：`MAX_TOOL_CALLS_PER_RUN=25`（`runner.rs:27`）在 `crates/agent` 且 pub，cloud 可直接引用；"trace 条数被预算天然限界"（:63）近似成立——预算拦截的是下一次调用（`runner.rs:232-235` 注释自认 fire-and-forget），在途调用可使条数略超 MAX，扩展 #10 显示 "27/25" 会是怪相，前端展示宜写 "n / 25" 不设上限断言。

**备注 F-b（不阻断）**：RAII guard 注销、DashMap 注册表（manager 已有 DashMap 先例）、`#[serde(default)]` 向后兼容（`end_reason`/`thing_id` 已有同法先例 `agent_runs.rs:117-121`）、judgments 加列迁移、parse_verdict 加必填字段+宽松 fallback——均有既有模式支撑，可构建。

---

## 结论

**总体 FAIL**（Completeness / Consistency / Clarity / Feasibility 各有问题，Scope 通过）。阻断项按优先级：

1. **C2**（扩展 #3 前提不成立——需重估或延期，涉及 CEO 已批准范围，必须回 CEO 确认）；
2. **C1 + Y1/F1**（run_id 从 DB 到 DTO 到 dispatch 机制的链路断裂——实时进度主目标依赖于此）；
3. **N1**（F11 契约矛盾——需显式修订）；
4. **C3**（404 竞态窗口——错误处理节补一条即可）。

修复 1-4 后可复审转正。CEO 摘要的 Reviewer Concerns 节（:45"待 Spec Review Loop 结果填入"）应填入本报告要点，尤其 C2 的范围重估。
