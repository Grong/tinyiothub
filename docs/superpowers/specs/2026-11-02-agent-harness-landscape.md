# Agent Harness 主流架构调研（2026-11）

> 目的：在重新设计 TinyIoTHub Harness 之前，先弄清主流 harness 的共同契约、分歧点与取舍。本文只做调研与分析，不给最终结论。

## 1. 调研范围与方法

对象：AGH（agnes-harness as-built 技术报告）、OpenHands、LangGraph、Claude Agent SDK、OpenAI Agents SDK、Microsoft Agent Framework Harness / Foundry Copilot harness、Amazon Bedrock AgentCore Harness、Hermes（Arize 开源分析）、Letta（MemGPT）、AgentScope Harness。

方法：公开文档、技术报告与架构分析交叉对照；只提取可验证的公开事实，不把厂商营销当设计依据。TinyIoTHub 侧的证据来自本仓代码。

## 2. 主流 harness 的最小共识

九个架构关注点在所有实现里反复出现。

### 2.1 Loop 契约

- 统一形态：`model call → tool dispatch → append result → repeat`，直到最终答复、预算耗尽或中断。
- provider 适配器与 loop 解耦：Hermes 用 transport adapter 统一 chat-completions / Anthropic Messages / Responses / Bedrock；OpenHands 用 LiteLLM。
- 循环边界显式：`max_turns`（OpenAI Agents SDK）、预算/停止原因（Claude Agent SDK 的 `ResultMessage` 含 `error_max_turns`、`error_max_budget_usd`、`num_turns`、`permission_denials`）。
- 推荐把 loop 当作可替换插件（AGH 的 `@agnes/loop-default`；AgentScope 的 `ReActAgent` 与 `HarnessAgent` 分层）。

### 2.2 事件/会话平面是主干

- OpenHands：`EventStream` 是"所有通信的骨架"；Action / Observation 事件带单调 id、时间戳、source、`_cause` 因果关系；**先持久化再分发**。
- AGH：session ledger 是唯一权威，hash-chained、可校验；一切投影由 ledger 折叠而来；"Models propose; the ledger records."
- Claude Agent SDK：会话是 JSONL 转录，可 resume / fork；但文档明确"会话是连续性，不是真相"，关键状态要落到类型化记录并对账。
- Microsoft harness：每次模型调用后持久化，用于崩溃恢复与运行中检查。
- LangGraph：按 super-step 写 checkpoint，`thread_id` 是主键，支持 state history 与 time travel。

共同点：**执行事实必须持久化在会话/事件层，UI 与历史都是投影**。分歧在"事件流"还是"步骤快照"。

### 2.3 可恢复性语义

- 事件溯源（OpenHands/AGH）：事件先落盘再分发，崩溃后重放；副作用要求幂等。
- Checkpoint（LangGraph）：按步骤快照；恢复是从最近 checkpoint 重放，`sync/async/exit` 三档持久化模式；HITL 的 `interrupt()` 是持久状态。
- 厂商托管（Bedrock AgentCore / Claude Managed Agents）：把 loop 与沙箱托管，自动 tracing；代价是控制面在厂商。
- 共识：恢复不是"从断点继续执行代码"，而是"从持久状态重新进入"；副作用必须可对账，未知结果不能自动重放。

### 2.4 工具运行时

- **注册与暴露分离**（Hermes）：工具库可以很宽，但每次 Run 只暴露经 toolset 过滤后的子集，控制 token 与安全面。
- **权限求值顺序**（Claude Agent SDK）：PreToolUse hook → deny 规则 → ask 规则 → permission mode → allow 规则 → canUseTool；hook 是确定性执行点。
- **审批策略**（Microsoft、AWS、AGH）：按工具/参数/策略审批，unattended 模式有 pending gate；审批与执行分开记录。
- **沙箱**：OpenHands 的 Runtime/Sandbox 有多级实现（Local/Docker/Remote/K8s/Modal）；AGH 的 Effect Sandwich 要求副作用走受控路径。
- **MCP**：作为工具/资源接入标准，带 allow/deny 与会话级过滤（AgentScope、Claude、AGH）。

### 2.5 上下文管线

- 上下文是被管理的资源，不是无界追加：裁剪、去重、按时间压缩、摘要（Harness Stack 提到的 40–60% 水位主动压缩）。
- 压缩是**会话生命周期事件**，产生子会话与血缘，而不是改写历史（Hermes、OpenHands 的 Condenser）。
- 记忆分层（Letta/MemGPT）：core memory（常驻、可编辑）、recall（可检索的消息）、archival（长期向量库）；OpenHands 用 EventStream + View + MicroAgents 三层。
- Skills 渐进加载（Claude Agent SDK、AGH）：领域流程放在 Skill，而不是把全部内容塞进 system prompt。

### 2.6 控制面

- steering / interrupt / pause / cancel / queue 是一等公民（AGH 的 durable controls；Claude 的排队与 Interrupt now；OpenAI 的 `RunState` 可恢复）。
- HITL 不只是弹窗：审批问题、答案、消费状态要持久化；未知效果需要人工对账（AGH 的 decision register）。

### 2.7 多代理

- 三种主流形态：handoff（OpenAI：agent-as-tool `transfer_to_*`）、subagent-as-tool（OpenHands V1、Claude Agent 的 `Agent` 工具）、图/工作流（LangGraph、AgentScope）。
- 复杂拓扑的常见风险：handoff 漂移与环（需要 hop 计数）；子代理预算与 lineage 必须显式。
- 简单可组合的默认：**subagent-as-tool**，共享父级事件流或使用隔离子会话，结果作为类型化记录返回，不用子代理的自由文本当状态。

### 2.8 扩展模型

- 插件清单 + 能力声明：AGH 的插件（tools/loops/adapters/panels/skills/connections/services）、Claude 的 plugins（skills/agents/hooks/MCP）、OpenHands 的 SDK 包 DAG（sdk / tools / workspace / agent-server）。
- 版本与固定：AGH 按会话固定插件代码代际，支持热升级；扩展不能重定义核心语义。
- 扩展的边界必须可测试：manifest 校验、能力清单、权限面、integrity digest。

### 2.9 可观测性

- Tracing 默认开启：OpenAI Agents SDK 的 span 树（LLM/tool/handoff/guardrail），Claude 的 lifecycle 事件（工具提议/允许/拒绝、token、延迟、停止原因），Bedrock 自动 CloudWatch 追踪。
- Trace 是产品能力，不只是调试：回放、会话搜索、eval、审计。
- 关键点：事件必须结构化、可关联（session/run/tool_call/approval/effect）。

## 3. 主流方案的分歧与取舍

| 维度 | 事件溯源（OpenHands/AGH） | Checkpoint（LangGraph） | SDK 组合（Claude/OpenAI SDK） | 托管 Harness（Bedrock/Foundry） |
|---|---|---|---|---|
| 事实主干 | append-only 事件流 | 步骤快照 | 会话消息 + traces | 厂商托管 |
| 透明度 | 最强（全事件可回放） | 中（按步骤快照） | 中（tracing 默认开） | 强（厂商 tracing） |
| 恢复语义 | 重放事件 + 幂等副作用 | 从 checkpoint 重放 | 会话 resume/fork | 托管恢复 |
| 复杂度 | 高（schema、事件量） | 中 | 低 | 低（控制面外置） |
| 扩展性 | 强（插件订阅事件） | 中 | 强（hook/plugin） | 受厂商边界限制 |
| 适合场景 | 长跑、审计、IoT | 图/工作流、HITL | 快速产品化 | 不想自建 runtime |

两个关键判断：

1. **Checkpoint 不等于可观测的 harness**。LangGraph 的 checkpoint 解决"恢复到哪一步"，但不自动提供完整的执行叙述；Temporal 也指出 checkpoint 不是 durable execution。TinyIoTHub 要"用户透明看到过程"，事件溯源更贴近目标。
2. **托管 harness 会拿走控制面**。对 IoT 平台而言，设备协议、策略、审批、证据链是核心资产，不宜把 loop 与沙箱整体托管给外部。

## 4. 对照 TinyIoTHub

| 能力 | 本仓现状 | 主流目标 | 结论 |
|---|---|---|---|
| 会话事实 | `chat_sessions/messages`；Task 只有 `agent_runs` 结束态 | 统一会话事件账本，step 级、可回放 | 缺主干 |
| 执行过程 | `TurnEvent` 只进内存 trace | 思考/文本/工具/结果落事件流并投影 | 缺持久化 |
| 控制 | `SteeringBuffer`、Task pause/cancel | steer/edit/withdraw/interrupt/queue + 持久化 | 部分 |
| 审批 | Task approvals、heartbeat approvals 两套 | 统一 decision register + 效果对账 | 分裂 |
| 工具 | 固定清单，硬编码 | 声明/插件注册 + 按 Run 暴露 + 策略/沙箱 | 缺运行时 |
| 扩展 | 驱动 manifest（设备域） | Harness 级插件清单与版本固定 | 缺 |
| 上下文 | prompt sections、chat compacted_sessions | Context Pipeline：记忆/skills/压缩/血缘 | 缺 |
| 多代理 | 基本没有 | subagent-as-tool / child sessions | 缺 |
| 观测 | RunReport.trace 结束态 | 实时 trace、回放、搜索、eval | 缺 |
| IoT | 声明、Hub、Job/Watch、证据链 | 作为应用插件接入 | 可继承 |

## 5. 三种候选架构方向（待讨论，不是结论）

### 方向 A：事件溯源 Harness Core（OpenHands/AGH 形态）
- 以 append-only 会话事件为唯一事实主干；Loop、工具、审批、投影、UI 全部订阅事件。
- 优点：透明度、回放、恢复、审计最强；天然支持"过程可见"。
- 代价：事件 schema 演进、事件量、幂等副作用纪律、迁移成本最高。
- 适合：IoT 长跑任务、需要证据链与人工审批。

### 方向 B：Checkpoint/工作流 Harness（LangGraph 形态）
- 以步骤快照为持久化单元，配合显式状态机与 HITL interrupt。
- 优点：实现简单、状态可检查、适合固定流程（巡检、联动）。
- 代价：完整执行叙述仍需额外事件；长跑常驻 loop 与动态流程适配较弱。
- 适合：流程编排为主、Agent 自由度较低的场景。

### 方向 C：SDK 组合式 Harness（Claude/OpenAI SDK 形态）
- 最小原语（Agent/Runner/Session/Tool/Hook/Guardrail）+ 插件；快速获得主流行为。
- 优点：上手快、生态与模式成熟、测试面清晰。
- 代价：durable 会话与回放要自建；多租户/边缘部署的控制面仍需自己做。
- 适合：先追平主流能力，再逐步补 durable 与审计。

### 可能的混合
事件主干 + checkpoint 式恢复 + SDK 式 loop/tool 原语。对 TinyIoTHub 最可能的方向，但需要先确认事件量与多租户边界。

## 6. 待确认的问题

1. 事件主干的边界：只记 Agent 执行事实，还是把设备遥测也纳入同一账本？（当前 `hub_ledger` 已承载 100 万行/天的 state_change）
2. 恢复语义：崩溃后自动重放，还是只恢复到安全停顿点等人工确认？
3. 副作用对账：设备动作的"未知结果"是否必须人工对账，还是允许自动重试？
4. 扩展边界：插件能否注册工具之外的 Loop、上下文段、UI 面板？版本固定策略？
5. 多租户/边缘：Harness 与设备域是否部署在同一进程？事件账本是否按租户隔离？
6. 与现有 IoT 能力的关系：设备声明 → 工具/上下文的生成链，是否作为 Harness 的第一个应用插件？

## 7. 事实主干：三种设计（本轮主轴）

事实主干必须同时承担四件事：**执行事实**（发生了什么）、**恢复**（从哪里重新进入）、**副作用安全**（外部动作不重复）、**投影**（UI/历史/审计如何读）。主流方案在这四件事上的取舍不同。

### 7.1 数据边界：不要复用设备遥测账本

本仓 `hub_ledger` 已经承载约 100 万行/天的 `state_change`（313 万行、1GB）。把 Agent 的 step 事件混进同一张表会带来三个问题：写入争用、索引膨胀、回放时混入遥测。建议：

- `hub_ledger`：继续只存设备遥测与设备事件（state_change/watch/job）。
- 新增 `session_events`（或 `agent_events`）：只存 Agent 执行事实，按 `workspace_id + session_id + seq` 索引。
- 两边通过 `task_id / run_id / tool_call_id / job_id / evidence_id` 关联，不复制 payload。

### 7.2 方案 1：纯事件溯源（AGH/OpenHands 形态）

```text
session_events(seq PK, session_id, workspace_id, run_id, task_id,
               type, occurred_at, cause_id, schema_v, payload_json)
```

- 一切执行（turn start、thinking/text 聚合、tool call/result、approval、effect intent/result、run end）都是事件。
- 投影：Task/Chat/Workbench 由事件折叠；SSE 按 `seq` 尾随。
- 恢复：重放事件重建状态；副作用通过 effect 状态机与幂等键保护，未知结果人工/设备对账，不盲目重放。
- 优点：透明度、审计、回放最强；统一 Chat 与 Task 两套事实。
- 代价：事件 schema 演进、事件量（thinking/text 必须聚合）、从 `chat_messages`/`agent_runs` 迁移。

### 7.3 方案 2：Checkpoint 为主（LangGraph 形态）

```text
run_state(run_id, step_no, state_json)          -- 每步快照
run_events(run_id, seq, type, payload)          -- 仅供 UI/调试
```

- 恢复：加载最近 `run_state`，从下一步继续；副作用仍要幂等。
- 优点：实现简单、恢复快、状态可检查。
- 代价：执行叙述是"附带产物"，回放/审计不完整；两个存储需要一致；常驻/动态流程不如事件模型自然。

### 7.4 方案 3：混合主干（推荐候选）

```text
session_events      -- 唯一事实主干（append-only）
projection_snapshots -- 周期性投影快照，仅用于加速启动，可从事件重建
effect_intents      -- 外部副作用的持久状态机
```

- 事实以事件为准；快照只是缓存，丢了可以从事件重建。
- 副作用状态机：`Prepared → Dispatching → Completed | Failed | Unknown`；`Dispatching/Unknown` 是失败关闭状态；重试前必须先对账（读设备/Job 状态），不得自动重放。
- 稳定效果身份：`effect_id = workspace + task + run + tool_call + intent_digest`，不是每次尝试的 request id；同一身份不同意图是完整性冲突。
- 优点：同时拿到透明回放、快速启动、副作用安全；与 AGH 的 ledger + effect 语义、OpenHands 的 EventStream + 幂等副作用一致。
- 代价：三套存储的归属与一致性要写清；事件量需要聚合策略。

## 8. 副作用安全（跨方案共同要求）

公开的 durable-execution 资料给出的收敛规则：

1. **先持久化意图，再执行副作用**；`Dispatching` 状态是唯一的物理发送许可。
2. **稳定业务身份**，不是 per-attempt request id；同一身份不同参数是冲突。
3. **outbox + 幂等键**：原子写意图与待发送事件，由 dispatcher 幂等投递；崩溃后另一个 worker 只能按状态机接管。
4. **超时是 UNKNOWN，不是 FAILED**；未知结果隔离，先对账（设备/Job 状态、provider 查询）再决定重试或人工处理。
5. **只保护有后果的工具**：只读/生成类直接执行，有副作用的走保护边界，语义不确定的失败关闭。
6. **不声称通用 exactly-once**：只声明在明确的 provider/对账条件下成立。

本仓已有可继承的事实：DeviceJob 的 `cancelled_unknown` 语义、`task_wake_outbox` 的幂等唤醒、`task_approvals` 的持久决策。新主干应把它们收编为统一的 effect/decision 记录，而不是再造一套。

## 9. 来源

- AGH as-built 技术报告：https://github.com/AgnesAI-Labs/agnes-harness-tech-report
- AGH 仓库与架构文档：https://github.com/AgnesAI-Labs/agnes-harness
- OpenHands 架构：https://docs.openhands.dev/enterprise/architecture
- LangGraph durable execution / checkpointers：https://docs.langchain.com/oss/python/langgraph/checkpointers
- Claude Agent SDK overview：https://code.claude.com/docs/en/agent-sdk/overview
- OpenAI Agents SDK：https://openai.github.io/openai-agents-python/
- Microsoft Agent Framework Harness：https://devblogs.microsoft.com/agent-framework/the-microsoft-agent-framework-harness-is-now-released/
- Amazon Bedrock AgentCore Harness：https://builder.aws.com/content/3KD10JLQ7SyuHMG1jI1JuUdleFm/amazon-bedrock-agent-core-harness-from-idea-to-production-grade-agent
- Hermes harness 分析（Arize）：https://arize.com/blog/how-hermes-implements-open-source-agent-harness-architecture/
- Letta / MemGPT：https://docs.letta.com/concepts/memgpt/
- Harness Stack 章节：https://raw.githubusercontent.com/jayminwest/agentic-engineering-book/98606f12b694194c1092dac19c5756f7604ac927/chapters/6-harnesses/2-harness-stack.md
- 副作用安全（效果身份/状态机）：https://github.com/mmarano2k14/deterministic-ai-runtime/blob/feature/sdk-interactive-agent-demo/docs/ai/durable-mcp-effect-evidence.md
- Once Reliable Agent Execution（UNKNOWN/对账/选择性保护）：https://zenodo.org/records/22969881
- agent-effects-store（Rust，committed/failed/unknown 分类）：https://crates.io/crates/agent-effects-store
