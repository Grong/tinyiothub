# AI 中心探索报告（分支 ai-brain-p0，2026-10-08）

注：仓库中并没有 `cloud/src` 目录——后端实际位于 `apps/cloud/src`（Rust workspace），共用 crate 在 `crates/{agent,db,core,...}`。

---

## 1. 前端 AI 中心视图

**核心视图：`web/src/ui/views/dispositions.ts`（处置中心，AI 大脑事件 feed）**
- 数据来自 `/brain-events` API（`web/src/api/brain-events.ts`），是 judgments ∪ agent_actions ∪ agent_runs 的只读投影。
- 展示：5 个 tab（需要你/全部/巡检/报警/指令，`dispositions.ts:35-41`）；状态徽章覆盖 11 个状态词（`:44-56`）；卡片两行结构=标题+理由+建议动作（`:511-526`）；头部统计=今日消化数、需要你数、判断延迟 p50、反馈学习计数（`:382-399`）；待审批卡片有批准/拒绝按钮+24h 自动转工单倒计时（`:528-537`）；alarm 源有 ✓/✕ 判断反馈（`:515-520`）。
- **处理过程的唯一可见窗口是"证据"懒加载**：展开卡片调 `/brain-events/{id}` 拿 `evidence`，`segmentEvidence()`（`:98-125`）把原始输出切成 think 块（折叠为"思考过程"）、正文、尾部 verdict JSON 协议块（折叠为"结构化结论"），外加动作清单（✓/✗/– 标记）和元信息（工具调用次数、耗时、tokens，`:436-470`）。

**不可见的处理过程**：
- LLM 的逐步工具调用轨迹（ToolTraceEntry）不进 evidence，只有聚合计数；
- 实时流式过程完全没有——证据是 run 结束后一次性落库的成品；
- `investigating`（分析中）和 `patrol_ok`（巡检正常聚合行）不可展开证据（`dispositions.ts:274-276`）；patrol_tick 行点击只跳 `#/ai-ops`（`:279-283`）。
- AgentEvent 总线内部事件（RunRecorded、HeartbeatResultReady 等）不推送到前端。

**其他 AI 相关视图**：
- `web/src/ui/views/ai-ops.ts`（AI 运维中心，488 行）：今日摘要、待审批 proposals、执行历史（`/workspaces/{id}/heartbeat/logs` 的 ExecutionRecord）、信任配置、知识/记忆，巡检间隔/任务配置。
- `web/src/ui/views/agents.ts`（243 行）：4 个 tab——工作区设定/工具权限/技能/心跳（`agents-heartbeat-tab.ts`，400 行，含心跳日志+审批），另有"反思引擎"开关。
- `web/src/ui/views/tickets.ts`：工单页，同样有 SSE 断连细条模式。

**未提交改动**（`git diff --stat`：4 文件，+12/-10）：
- `dispositions.ts`：X7 键盘可达的 WCAG 修正——行元素去掉 `role=button`（因为行内嵌套真实按钮/输入框，button 角色会把后代强制 presentational，屏幕阅读器丢语义），保留 Enter/Space 展开（`:497-509`）。
- `dispositions.test.ts:281-288`：对应断言改为 `role` 为 null。
- `dispositions.css` / `tickets.css`：设计令牌化清理——硬编码 `#fef3c7/#92400e` 改 `var(--warn-subtle)/var(--warn)`，像素 padding/margin 改 `var(--space-*)`，一处字号 11.5px→12px。纯样式债偿还，无行为变化。

---

## 2. 后端 AI 处理链路

**事件总线（进程内）**：`crates/agent/src/runtime/events.rs:26-44`，`AgentEventKind` 仅 5 种：`RunRecorded`（含完整 RunReport）、`HeartbeatResultReady`、`TrustConfigChanged`、`HeartbeatTasksChanged`、`DlqEntryAdded`。tokio broadcast，进程内单调 seq。

**Thing agent loop**：`crates/agent/src/runtime/thing_agent/runner.rs` —— 一次 Wake 产生 `RunReport`（`assemble_report` `:446`）：summary（LLM 最终输出，含 think 块+尾部 verdict JSON 协议块）、actions 数组（每个动作带验证标记，`judge_verified` `:344`——客观验证：invoke 后必须跟 read/query_events 才算 verified，"不信任 LLM 自报"）、tool_calls/tokens/duration_ms、outcome（acted/no_action/failed/budget_exceeded/rejected）。**中间轮次的 turn 事件流（`consume_events` `:271`）只用于聚合，不逐条持久化**。

**心跳/巡检**：`crates/agent/src/runtime/heartbeat/runner.rs`（HeartbeatRunner，每 workspace 一个 loop），结果走 `HeartbeatResultReady` → 持久化。

**持久化订阅者**：`apps/cloud/src/domains/agent/host/persist.rs`
- `RunRecorded` → `agent_runs` 表（insert-once 幂等，失败进 DLQ，`:266-411`）；
- `HeartbeatResultReady` → `agent_actions` 表（`insert_heartbeat_result`，失败 spawn 独立重试任务，`:282-318`）。

**判断流水线**：`apps/cloud/src/domains/agent/host/judgment_subscriber.rs`（1073 行）
- run 完成 → 解析 summary 尾部 ```json verdict 块（`parse_verdict` `:72`，宽松 fallback 会打标记进 evidence）；
- verdict 三出口（`crates/db/src/judgment.rs:30-34`）：noise（自行闭环）/ self_healable（待审批→批准执行→resolved）/ needs_human（escalated 转工单 `escalate_to_ticket`）；
- **evidence_json 组装点**（`judgment_subscriber.rs:428-433`）：`{run_id, summary, actions, tool_calls, duration_ms, tokens...}` 写 `judgments.evidence_json`——这就是前端"证据"面板的全部内容；
- 状态机（`crates/db/src/judgment.rs:57-69`）：investigating → noise_archived/awaiting_approval/executing/resolved/escalated/investigation_failed/budget_skipped/dispatch_suppressed/dismissed（X5 拒绝不开票，拒绝原因 json_set 进 evidence.dismiss_reason，原子 UPDATE `dismiss_judgment`）。

**推送**：`broadcast_judgment`（`judgment_subscriber.rs:552-561`）发 SSE `judgment_judged` / `judgment_updated`（payload 只有 workspace_id + judgment_id，无内容）；X1 起 `spawn_needs_you_notify`（`:565`）把"需要你"事件送 notify 域（SSE/in-app 通道，sms/email 仍是打桩）。`ticket_subscriber.rs`（425 行）管工单侧。

**中间数据去向汇总**：思考/verdict 原文 → judgments.evidence_json（仅终态成品）；工具调用轨迹 → 聚合成计数+actions 数组，逐步轨迹丢弃；agent_runs.report 存完整 RunReport JSON；patrol 提案 → agent_actions(action_type='proposal'/'auto_executed')；内部 bus 事件 → 无持久化无推送（DLQ 除外）。

---

## 3. 数据持久化与推送通道

**brain_events 是只读 SQL 视图**（非实体表），投影查询在 `crates/db/src/brain_event.rs:123-258`：
- `list_brain_events`：48h 窗（needs_you tab 无窗——陈旧未认领最该被看见）、alarm 源按 thing+rule 折叠（window function）、需要你置顶且 escalated 排 awaiting_approval 前（X6）、元组游标分页；
- 列表 19 列**不载 evidence_json**（性能评审裁决），detail 才载；
- summary：今日消化/需要你/alarm 判定延迟 p50/反馈对错（judgment_feedback 表）。

**HTTP 端点**：`apps/cloud/src/domains/brain_event/handler/mod.rs:22-26` —— `GET /brain-events`、`/brain-events/summary`、`/brain-events/{id}`。变更操作不改端点：alarm 源走 `/judgments/{id}/approve|reject|feedback`，patrol 源走 `/workspaces/{id}/heartbeat/approvals/{pid}/approve|reject`。

**推送通道 = SSE（非 WebSocket）**：
- 前端 `web/src/api/sse-client.ts`：fetch + ReadableStream 自实现（为了带 Authorization header，不用 EventSource），3 秒自动重连；
- 端点 `/api/v1/workspaces/notifications/stream`，后端 `apps/cloud/src/domains/event/sse_manager.rs` + `domains/notify/channels/sse_channel.rs`；
- **前端消费的事件类型只有 3 个**：`judgment_judged`、`judgment_updated`、`ticket_created`（`dispositions.ts:199-201`），且收到后只是整表重新拉取，payload 本身不带内容；
- 兜底：dispositions 还有 60s 轮询（`:207`）；SSE 断连显示"实时更新已断开"细条。

**`RealTimeEvent`**（`crates/db/src/event.rs:324-353`）是另一套通用实时事件模型（id/type/level/source/title/content_preview/acknowledged），经 `SseConnectionManager.broadcast_event`（`sse_manager.rs:89`）广播，与 AI 判断链路是平行通道。

---

## 4. 近期提交脉络（ai-brain / agent / disposition 相关）

按时间倒序：
- `0b3df3b5` **外部声音 follow-up**：纯注释+测试批——judgment.rs 状态清单与 `judgments_active_alarm` 唯一索引的三处迁移互指注释（OV#1）、resurface 竞态残窗降级为可接受 error log（OV#2）、markdown.ts 单源姿态说明（OV#3）、heartbeat_approvals fixture 手工镜像取舍（OV#4）、SLA notify 测试断言投递目标 ws1（OV#5）。
- `0fc54ad4` **eng 评审修复批**：R-F1 resurface 前查同报警未终态判断（消除唯一索引每周期的 error log）；R-F2 `dismiss_judgment` 单条条件 UPDATE 原子化（状态翻转+dismiss_reason 写入不再两步 await）；R-F4b 前端两个文件重复的 md() 收敛到共享 `ui/shared/markdown.ts`；R-G1/G4 新增 heartbeat approvals handler 级测试（409/403/400）；R-G2 SLA 升级 notify 断言。TODOS 登记 F-3 notify 扇出聚合/限流（P1）。
- 之前的功能线：`X5 dismissed 状态机 + X1 notify 例外到人`（1002d9c4）、`F9 批准角色闸 + X2 dismiss_reason`（af49d210）、`F7 PendingActionStore 周期清扫`、`F4 批准审计竞态`、X6/X7/F8（web 侧排序/可达性/markdown）、`ad271e06 提案 problem_key 折叠`、`19a1ebac 提示词收敛——唯一模块管全部+宪法段单源`、`15abb58e 巡检 tick 提示词全中文化`（AI 运维页不再英文）、`8a32af7c 幻觉工具名提案自动拒绝`、`e45787f3 提案参数蛇形/驼峰并容`。

整体脉络：P0 主干（brain_events 投影 + 处置中心 UI）已落地，近期全部是评审驱动的健壮性/可达性/一致性修复。

---

## 5. 相关 spec 要点（`docs/superpowers/specs/`）

**`2026-07-29-thing-agent-loop-design.md`**（最直接相关，v2=CEO 审查+三轮对抗评审+外部声音，v3=eng 轮外部声音——即上述提交的设计源头）：
- 触发器三源：用户指令/物事件/定时（`trigger/` 下 thing_event.rs、timer.rs）；
- Run 生命周期 + system prompt 四段式（prompt 纪律替代刚性流水线）；
- 自治策略门：`workspace_autonomy_policy` + `policy_rules` 表，每次 invoke_action 同步毫秒级裁决；
- **数据模型**：`agent_runs` 表（outcome/summary/report JSON/verified/tool_rounds/tokens/problem_key/dedup_key）；动作级审计复用 invoke_action 既有通道（actor=agent, run_id）；
- **记忆连续性**：每次 Run 结束写 trigger+outcome+一句话 summary 进 memory service，下次唤醒注入最近 5 条 Run 摘要；
- **报告回推分路**：用户指令 → chat 会话 SSE 回推 assistant 消息；物事件/定时 → 静默落库，failed/rejected 才走告警通道；
- 前端范围"本期最小"：只做 chat 回推可见，Runs 列表 UI 留后续——这正解释了当前"处理过程不可见"是**有意的设计裁决**。

**`2026-06-17-heartbeat-as-ai-life-source-design.md`**：心跳作为 AI 生命线——HeartbeatManager/心跳循环、静态巡检任务（HEARTBEAT.md）、上次执行记录注入 prompt（执行连续性）、AgentActionRepository 扩展、告警风暴保护、format_context。

**其他相关**：`2026-06-15-ai-event-integration-design.md`（事件集成）、`2026-06-29-ai-subsystem-refactor-design.md`、`2026-07-06-ai-harness-loop-design.md`。

---

## 结论（回答核心问题）

**当前 AI 中心给用户看的**：处置中心卡片流（状态徽章+标题+理由+建议动作）、统计摘要、审批按钮、判断反馈按钮、以及懒加载的"证据"面板——证据里有折叠的思考过程、结构化结论原文、动作清单和元信息（工具调用次数/耗时/tokens）。AI 运维中心另有巡检待审批+执行历史+信任配置。

**处理过程数据在哪产生/是否持久化**：产生于 `crates/agent` 的 thing_agent runner 和 heartbeat runner，经进程内 AgentEventBus → `persist.rs` 落 `agent_runs`/`agent_actions`，经 `judgment_subscriber.rs` 落 `judgments`（含 evidence_json）。**持久化的是终态成品**（RunReport + verdict + 聚合计数）；**逐步 LLM 轨迹、流式中间输出、bus 内部事件均不持久化也不推送**。推送仅 SSE 三个信号类事件（judgment_judged/judgment_updated/ticket_created），且只是"刷新提示"不含内容——这是 thing-agent-loop spec 中"物事件/定时触发静默落库、前端本期最小"的既定设计。
