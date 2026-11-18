# Design: Assistant Harness — Claude Code 式常驻智能体

- Date: 2026-11-07
- Status: Draft for user review
- Scope: backend harness first, frontend interaction second
- Relationship: 部分取代 `2026-11-02-agent-harness-core-design.md` 的 Workflow-first 方向；其安全、Effect/Decision、恢复与对账合同继续有效

## 1. Problem Statement

TinyIoTHub 已经投入大量时间建设 Harness Core，但实际效果与预期和主流 harness 差距明显。用户给出的三个具体失败面是：

1. **Agent 干不成真实任务**——多步任务中途走偏，无法真正把一件事做完；
2. **不知道如何开始**——用户需要大量配置，才能让系统开始工作；
3. **交互与可见性差**——看不到 Agent 在干什么、为什么做，无法有效干预。

用户对目标的心智模型：

> 我需要的是一个完整的大脑。大脑是一直存在的，我只需要告诉它需要注意什么、需要干什么活，它就会执行；如果没有明确指示，它按默认执行。

这不是 UI 修补，而是对现有 Harness Core 前提的修正。

## 2. Why the Existing Design Falls Short

现有 `2026-11-02-agent-harness-core-design.md` 以 Workflow 声明为中心：用户必须先定义 `objective / trigger / device_selector / source_selector / tool_ids / skill_refs / required_capabilities / approval_policy / evidence_policy`，系统才会调度执行。

这与用户的需求有三处结构性冲突：

- **入口冲突**：Workflow 要求"先声明再工作"；用户要的是"先有大脑，再补充关注点"。
- **执行冲突**：当前后端有两个执行器——交互式 chat 循环与自治 thing-agent 循环。`dispatch_thing_task` 把活异步甩给后者，然后回一条报告（`apps/cloud/src/domains/agent/host/tools/dispatch_task.rs:1-10`）。用户看到的是一段一段的日志，而不是一个连续工作的智能体。
- **过程冲突**：事件模型只有工具级 `TurnEvent`（`crates/agent/src/port/events.rs`），没有任务、技能、提问的 span；前端无法稳定回答"现在在干哪件事"。

Workflow 不是主流解决方案。主流 harness（Claude Code 等）的核心是：**一个常驻主循环 + 技能按需加载 + 模型自己维护待办 + 执行中提问 + 同步子代理**。

## 3. Premises

- 每个 workspace 有一个常驻 Assistant（产品文案称"大脑"）；Assistant 的身份、记忆、默认职责来自 md 主 prompt。
- Assistant 可以开多个 Session：对话、定时巡检、事件处置；Session 之间共享身份与记忆，但不共享上下文。
- 技能是工作手册（怎么干）；主 prompt 是值班表（什么时候、干什么）。
- 默认零配置可运行；所有默认值都可通过改 md 或直接对话覆盖。
- 物理设备安全边界不因本次改造而放松。

## 4. Target Model — Claude Code 的五个 primitive

| Primitive | 主流做法 | 本设计采纳 |
|---|---|---|
| **Skill** | `SKILL.md` + `name`/`description` frontmatter；渐进披露：上下文只放索引，调用时注入正文 | 采纳；复用现有 `get_skill` 与 `crates/skills/src/loader.rs` |
| **TodoWrite** | 工具参数为 `{ todos: [{ content, status, activeForm }] }`；整体替换；同一时间一个 `in_progress` | 采纳同款 schema，工具名 `todo_write` |
| **AskUserQuestion** | 1–4 个问题，每题 2–4 个选项，可多选；阻塞 turn | 采纳同款语义，工具名 `ask_user` |
| **Task** | 同步子代理：`description` / `prompt` / `subagent_type`；独立上下文，返回最终报告 | 采纳同款语义；为避免与持久化 `Task` 冲突，工具名定为 `spawn_subagent` |
| **Plan mode** | 只读探索 + 计划批准 | **本期不做** |

关键取舍：**任务清单采用主流软约束**。技能正文写编号步骤，模型加载后用 `todo_write` 生成待办；系统不解析任务结构。好处是与 Claude Code 行为一致；代价是不能硬保证每一项都执行。用户已明确选择完全对齐主流。

## 5. Architecture

```
                    ┌──────────── 唤醒源 ────────────┐
  用户消息 ─────────┤                                │
  定时 tick ────────┤   Trigger Router               │
  设备/告警事件 ────┤   (schedule / event / chat)    │
  用户显式指令 ─────┘                │               │
                                    ▼               │
                        ┌───────────────────────┐   │
                        │ Workspace Assistant   │   │
                        │  主 prompt + 记忆      │   │
                        └──────────┬────────────┘   │
                                   ▼                │
                        ┌───────────────────────┐   │
                        │  Session Loop (唯一)   │◄──┘
                        │  turn → tool → turn   │
                        └──────────┬────────────┘
                                   │
        ┌──────────────┬───────────┼────────────┬─────────────┐
        ▼              ▼           ▼            ▼             ▼
   Skill 正文      Todo 状态    AskUser      Subagent      工具层
   (md)           (事件)       (提问)       (同步)        (policy/trust/
                                                           Effect/对账)
```

### 5.1 三条边界

1. **触发与执行分离**：`ThingAgentManager` 的调度部分保留并改名为 `TriggerRouter`，只负责产生 wake；不再自己拼 prompt、不再自己跑 `run_pipeline`。
2. **Assistant 与 Session 分离**：一个 workspace 一个 Assistant；可以有多个 Session，Session 之间不共享对话上下文。
3. **安全层不动**：`policy` / `trust` / `Effect` / `Decision` / unknown 对账 / 审批，全部保留在工具层。

### 5.2 保留与替换

**保留**：
- rig loop（`crates/agent/src/adapters/rig/loop_.rs`）
- ToolRegistry / ToolDescriptor / 工具目录（`apps/cloud/src/domains/agent/tools/`）
- policy / trust / approval / Effect / 恢复与对账
- session_events、SSE、MCP
- skills md loader（`crates/skills/src/loader.rs`）

**替换**：
- `AutonomousAgentProvider` 的独立 prompt 拼装 → 统一 prompt builder（`apps/cloud/src/domains/agent/prompt/mod.rs`）
- `dispatch_thing_task` 的异步甩单 → `spawn_subagent` 同款同步语义
- `heartbeat_tasks` 数据库任务 → 技能正文步骤；新的 `Schedule` 只保留调度状态
- chat 循环与 thing-agent 循环 → 一个 Session Loop

**不替换**：物理设备安全边界。统一循环只统一"谁在什么上下文里调用工具"，不放松任何一道闸门。

### 5.3 组件落位

| 组件 | 位置 |
|---|---|
| Session Loop、Session 类型、Trigger Router | `apps/cloud/src/domains/agent/session/`（新增） |
| `todo_write` / `ask_user` / `spawn_subagent` 工具 | `apps/cloud/src/domains/agent/host/tools/` |
| 统一 prompt builder | `apps/cloud/src/domains/agent/prompt/mod.rs`（扩展） |
| 事件新增字段 | `crates/agent/src/port/events.rs` |
| 技能加载与索引 | `crates/skills/src/loader.rs`（复用，不加任务 DSL） |
| 前端会话时间线 / HUD / composer | `web/src/ui/views/`、`web/src/styles/` |

Assistant 的主 prompt 沿用现有 md 组合（IDENTITY / SOUL / AGENTS / TOOLS / MEMORY / USER）。**默认职责写在 `AGENTS.md`**，workspace 目录可覆盖共享模板；用户改这个文件就是"告诉大脑注意什么、干什么活"。

### 5.4 术语与命名规范

> **硬规则：一个概念只允许一个代码名。** 产品文案可以更亲切，但类型、表、事件、API、路由必须使用规范名。历史上 `Agent`、`Task`、`Run`、`Job`、`Harness`、`Heartbeat` 都出现过多义，本节一次性定死。

#### 5.4.1 规范术语

| 规范名 | 代码标识 | 中文文案 | 定义 | 明确不用 |
|---|---|---|---|---|
| Assistant | `Assistant`, `assistant_id` | 助手（产品文案可用"大脑"） | 每 workspace 一个常驻身份：主 prompt、记忆、技能、默认职责 | `Brain`（仅比喻）、`ThingAgent`、`AutonomousAgent` |
| Session | `Session`, `session_id` | 会话 | 一条独立的对话/执行线程；kind = `chat` / `schedule` / `event` | `Conversation`、`Thread`、`Chat`（作名词） |
| Message | `Message`, `message_id` | 消息 | Session 内的 user/assistant 内容 | `ChatMessage` |
| Turn | `Turn`, `turn_id` | 轮次（通常不展示） | 一次模型调用周期，可含多次工具调用 | `Loop`、`Iteration` |
| Task | `Task`, `task_id` | 任务 | 持久化工作单元，有 `objective`，跨 Run 存活 | `Job`、`WorkItem` |
| Run | `Run`, `run_id` | 运行（审计/调试用） | Task 的一次执行尝试；同一 Task 可多次 Run | `Attempt`、`Execution` |
| Todo | `Todo`, `todo_id` | 待办 / 任务清单项 | Session 内的计划项，模型用 `todo_write` 维护，不持久化为独立工作单元 | `PlanItem`、`Subtask` |
| Subagent | `Subagent`, `subagent_id` | 子代理 | 同步派生的隔离上下文执行单元，只做一层 | `Task`（工具名）、`ChildAgent` |
| Skill | `Skill`, `skill_id` | 技能 | `SKILL.md` 工作手册 | `Playbook`、`Workflow` |
| Tool | `Tool`, `tool_call_id` | 工具 | 可调用能力 | `Action`（仅设备语义） |
| Trigger | `Trigger`, `TriggerSource` | 触发 | 唤醒来源：`user` / `schedule` / `event` / `recovery` | `Heartbeat`（AI 巡检语义） |
| Wake | `Wake`, `wake_event_id` | 唤醒 | Trigger 投递到 Session 的内部信号 | `Tick`（仅调度节拍） |
| Effect | `Effect`, `effect_id` | 外部动作（通常不展示） | 对外部世界的预期副作用 | `Action`、`Job` |
| Decision | `Decision`, `decision_id` | 决策 | Effect 的单赢家裁定记录 | `Approval`（请求侧） |
| Approval | `Approval`, `approval_id` | 审批 | 需要用户确认的动作请求与结果 | `Decision`（裁定侧） |
| Policy | `Policy` | 策略 | 工具准入规则 | `Trust`（信任配置的一部分） |
| PermissionMode | `PermissionMode` | 权限模式 | `read_only` / `ask` / `full_access` | `AutonomyMode`（退役） |
| Job | `Job`, `job_id` | 设备作业 | 仅在设备侧执行的长操作（`device_jobs`） | 不得用于 Agent 工作 |
| Schedule | `Schedule`, `schedule_id` | 定时 | 周期性触发配置 | `CronJob`、`Heartbeat` |
| Workflow | — | — | 本设计退役，不再新增 | 任何新功能 |
| Heartbeat | — | 心跳 | 仅保留设备存活语义（如仍存在） | AI 巡检、Agent 调度 |
| Harness | `Harness` | — | 内部架构名：统一 Session Loop + primitives + 安全边界；不是用户实体，不是表名 | UI 导航、API 资源名 |
| Runtime | `Runtime` | — | 执行机制（循环、调度、安全层），不面向用户 | 业务实体 |
| Thing | `Thing`, `thing_id` | 物 | 本体实体 | `Device`（物理设备） |
| Device | `Device`, `device_id` | 设备 | 具体物理设备 | `Thing`（本体） |

#### 5.4.2 命名规则

1. **层级固定**：`Workspace → Assistant → Session → Turn`；`Task → Run`；Session 内可有 `Todo`；Turn 内可派生 `Subagent`。不允许把 Subagent 叫 Task，不允许把 Todo 叫 Task。
   - 纯对话 Turn 不强制创建 Task；需要跨 Turn、跨重启或等待审批/设备作业才存活的工作，才落 `Task`。
   - 定时/事件唤醒创建 `Task`，其 `session_key` 指向对应 Session；用户对话默认不创建 Task。
   - `Run` 只属于 `Task`；chat 由 `Session` / `Turn` 直接承载，不伪造 Run。
   - `Subagent` 记录 `subagent_id`，不单独建 Task；它是父 Turn 下的隔离子执行。
2. **Rust**：crate 用 `tinyiothub_<capability>`；模块用单数名词；新领域类型不加 `Agent` 前缀；禁止新增 `Manager` / `Service` / `Helper` / `Util`；HTTP 适配层统一 `handler`，领域编排用角色名（`SessionRunner`、`TriggerRouter`）。
3. **类型与字段**：`PascalCase` / `snake_case`；ID 一律 `<entity>_id`；布尔用 `is_` / `has_` / `should_`；时间用 `created_at` / `updated_at` / `ended_at`；状态枚举序列化为 `snake_case`。
4. **事件**：`<aggregate>.<past_tense>`，只用过去式；禁止 `done` / `handle` / `do`。例：`subagent.completed`，不是 `subagent.done`。
5. **数据库**：表名 `snake_case` 复数；沿用 `agent_tasks` / `agent_runs`；新增表用明确的域名词（`schedules`、`session_events`）；禁止一张表承载两个概念。
6. **API**：资源用复数 kebab-case（`/sessions`、`/tasks`、`/approvals`）；动作用子资源（`/sessions/{id}/questions/{qid}/answer`）；禁止动词式顶层路由（`/chat`、`/dispatch`）。
7. **前端**：视图统一 `assistant-*`；URL 状态只保留一个 `assistant-url-state.ts`；组件用 `session-timeline`、`todo-list`、`question-card`、`approval-card`、`subagent-card`、`assistant-hud`；不再新增 `harness-*` / `runtime-*`。
8. **中文文案**：一个概念一个词；"大脑"只作为 Assistant 的产品比喻，不进入 API、类型、表、事件。

#### 5.4.3 事件与 ID

Session 时间线事件（`session_events`，前端唯一时间线）：

```
session.started      { session_id, kind }
session.ended        { session_id, outcome }
turn.started         { turn_id }
turn.completed       { turn_id, usage }
message.created      { message_id, role }
skill.loaded         { skill_id, name, version }
todo.updated         { todos[] }
question.asked       { question_id, questions[] }
question.answered    { question_id, answers[] }
subagent.started     { subagent_id, description }
subagent.completed   { subagent_id, status, summary }
tool.called          { tool_call_id, tool, args }
tool.returned        { tool_call_id, result, evidence[] }
approval.requested   { approval_id, effect_id, action }
approval.resolved    { approval_id, resolution }
trigger.received     { trigger, source_id }
wake.dispatched      { wake_event_id }
```

安全/审计账本（`hub_ledger` kind `runtime.v1`，继续使用既有事件，不作为 UI 时间线）：

```
task.created / task.updated / task.completed / task.failed
run.started / run.completed / run.failed
effect.intended / effect.dispatched / effect.confirmed / effect.failed / effect.unknown
decision.recorded
capability.unavailable
```

ID 约定：`assistant_id`、`session_id`、`session_key`、`turn_id`、`message_id`、`task_id`、`run_id`、`todo_id`、`subagent_id`、`question_id`、`approval_id`、`effect_id`、`decision_id`、`tool_call_id`、`wake_event_id`、`event_id`。同一实体在所有层使用同一 ID 名；wire 为 camelCase，DB/Rust 为 snake_case，不做 `xxxId` / `xxx_id` 混用。

`session_events` 位于每个 workspace 独立的 Harness DB（`data/agents/<workspace>/harness/harness.db`），因此不需要冗余 `workspace_id` 列：全局 HUD 直接在该 workspace 的 DB 内按 `occurred_at`/`seq` 读取最近事件。为"最近事件"增加 `(occurred_at)` 索引；`session_id` 仍是会话内按序回放的主键。`HarnessEvent.workspace_id` 由调用方从 DB 归属补齐，不落列。

#### 5.4.4 重命名映射

| 现有 | 新名 | 处理 |
|---|---|---|
| `ThingAgentManager` | `TriggerRouter` + `SessionRunner` | 拆分：调度只产 wake，执行归 Session |
| `AutonomousAgentProvider` | 删除 | 统一 prompt builder |
| `AgentTask` / `agent_tasks` | `Task` / 保留表名 | 类型层去掉 `Agent` 前缀 |
| `AgentRun` / `agent_runs` | `Run` / 保留表名 | 同上 |
| `HarnessTaskRow` | `TaskRow` | 去掉 Harness 前缀 |
| `dispatch_thing_task` | `spawn_subagent` | 同步语义 |
| Claude Code `Task` 工具 | `spawn_subagent` | 避免与持久化 `Task` 冲突 |
| `subagent.done` | `subagent.completed` | 事件过去式 |
| `brain_events` 视图 / `brain_event` 域 | 收窄后退役 | `agent_actions` / `agent_runs` 分支由 `session_events` 取代；`judgments` 继续走 `/judgments`；全局最近事件在查询层合并，不恢复该视图 |
| `AutonomyMode` | `PermissionMode` | `diagnose`→`read_only`，`act`→`ask`；`off` 不属于权限模式，映射为 Assistant 状态 `paused` |
| `heartbeat_tasks` | `Schedule` | 旧表只读保留，新模型用 Schedule |
| `RuntimeEvent`（`hub_ledger`） | 保留 | 明确为安全/审计账本，不是 UI 时间线 |
| `SessionEvent`（`session_events`） | 新增类型 | 复用 2026-11-03 的 `session_events` 表结构，作为唯一 UI 时间线 |
| `host/` 模块 | 拆入 `session/`、`tool/`、`prompt/` | 不新增 `host` |
| `domains/agent/harness.rs`（Workflow API） | 删除 | Workflow 退役，Harness 名不再指向它 |
| `core::harness` | 保留 | 仅 Effect / Decision / Recovery 值类型 |

#### 5.4.5 禁用词

`ThingAgent`、`AutonomousAgent`、`Brain`（代码标识）、`Conversation`、`Thread`、`Chat`（作领域名词）、`Workflow`（新功能）、`Heartbeat`（AI 巡检）、`Manager` / `Service` / `Helper` / `Util`（新领域类型）、`Task`（作为子代理工具的 API 名）、`Job`（作为 Agent 工作单元）、`done`（事件名）。

#### 5.4.6 例外

- `SessionKey` 的外部格式保持 `agent:<workspace>:<agent>/<session>`，与 OpenClaw 兼容；格式里的 `agent` 是协议前缀，不代表领域实体 `Agent`。领域内统一读作 `assistant_id`。
- `crates/agent`、`tinyiothub_agent` 是既有 crate 名，表示"通用 Agent 运行时"，保留；不据此新增 `Agent` 业务实体。

## 6. Skills

### 6.1 格式

沿用现有 `SKILL.md` 形态与 frontmatter：

```markdown
---
name: inspection
description: 定时巡检工作空间设备健康状态，发现异常时诊断并报告
version: 1
---

# 巡检

## 执行步骤
1. 检查离线设备并分析原因
2. 扫描未处理的高优先级告警
3. 生成状态摘要，正常设备一行带过，异常设备给出诊断结论

## 注意事项
- 高风险操作必须获得用户确认
- 发现异常主动报告，不要等用户问
```

- **不引入自定义任务 DSL**；步骤是正文里的编号列表，由模型理解并转成 todo。
- 系统提示只放技能索引（name + description）；正文在 `get_skill` 调用后注入。
- 支持 `/skill-name` 直接触发。

### 6.2 内置技能集

| 技能 | 默认职责 |
|---|---|
| `inspection` 巡检 | 定时检查设备、告警、摘要 |
| `emergency-response` 应急预案 | 高优先级告警/设备事件时处置 |
| `energy-saving` 节能 | 低频能耗分析并给建议 |
| `video-analysis` 视频问题发现 | 视频源异常时分析 |
| `device-management` 设备管理 | 按需 |
| `alarm-management` 告警管理 | 按需 |
| `troubleshooting` 故障排查 | 按需 |
| `workspace` / `platform-overview` | 按需 |
| `job-management` / `driver-management` | 按需 |

现有 `apps/cloud/templates/skills/` 已包含大部分，需要改写为"步骤 + 注意事项"形态，并补 `emergency-response`、`energy-saving`。

### 6.3 什么时候用哪个技能

**由 Assistant 的主 prompt 决定，不由每个技能自带 schedule。** 主 prompt 示例：

```markdown
## 默认职责
- 每次唤醒：先检查高优先级告警；无异常则执行巡检
- 收到 Critical 告警：加载 emergency-response 技能
- 每天首次唤醒：执行 energy-saving 分析
- 没有值得做的事时保持安静，不产生无意义动作
```

调度器只提供 tick 和事件，不决定工作内容。

## 7. Runtime Primitives

### 7.1 todo_write

```json
{
  "todos": [
    { "content": "检查离线设备", "status": "in_progress", "activeForm": "正在检查离线设备" },
    { "content": "扫描高优先级告警", "status": "pending", "activeForm": "正在扫描高优先级告警" }
  ]
}
```

- 整体替换清单；同一时间只允许一个 `in_progress`。
- 系统提示规定：多步骤任务（≥3 步）先建 todo，完成一步立即更新。
- 加载技能后，按技能正文步骤建立 todo。
- todo 是会话状态，经事件持久化，前端实时渲染；不落成 md 文件。

### 7.2 ask_user

- 1–4 个问题，每题 2–4 个选项，可多选；用户可选 Other。
- 调用后 turn 阻塞，等待回答。
- 与工具审批区分：审批回答"这个动作能不能做"；`ask_user` 回答"我需要一个决定"。
- 自主会话无人在线时：暂停该任务并通知，不静默失败。

### 7.3 spawn_subagent（Claude Code 的 Task 工具）

- 同步派生子代理，父循环等待并拿到最终报告。
- 子代理上下文隔离，事件挂在父 Session 下。
- 最多一层，不做三层套娃。
- 输入：`description` / `prompt` / `subagent_type`；输出：`status` / `summary` / `evidence_refs` / `recommended_actions`。
- 命名说明：Claude Code 的工具名是 `Task`；本域的 `Task` 已是持久化工作单元，故工具名定为 `spawn_subagent`，语义完全一致。

## 8. Events and Visibility

在现有 `TurnEvent` 上补充 Session 级 span，写入 `session_events`。事件名一律 `<aggregate>.<past_tense>`，完整清单见 5.4.3；本节只列 UI 直接消费的部分：

```
session.started / session.ended
turn.started / turn.completed
message.created
skill.loaded
todo.updated
question.asked / question.answered
subagent.started / subagent.completed
tool.called / tool.returned
approval.requested / approval.resolved
trigger.received
wake.dispatched
```

`hub_ledger` 的 `runtime.v1` 安全/审计事件（Task / Run / Effect / Decision / unknown）继续独立记录，不混入 UI 时间线。现有 `Chunk / Thinking / ToolCall / ToolResult / ApprovalRequest / Usage` 全部保留。一个 Session 就是一条按序时间线；前端不需要猜测任务边界。

## 9. Default Behavior

- 新 workspace 创建 → Assistant 启动 → 内置技能可用 → 主 prompt 决定首个动作。
- 调度器默认 tick 间隔 5 分钟，仅作为唤醒节拍。
- 零配置可运行；用户改主 prompt 或直接对话即可覆盖默认。
- 没有匹配技能时走通用循环，模型自行规划。
- 无人在线时，自主会话按主 prompt 的低风险默认继续；涉及写设备始终需要审批。

## 10. Frontend Interaction

参考 Codex 版式，核心主张：**一个会话一条时间线，todo 内联，提问与审批就地**。

```
┌────────────────────────────────────────────────────────────┐
│ 巡检 · 10:05                                     ○  ⋯  ＋   │
│                   ┌──────────────────────────┐             │
│                   │ ● 大脑运行中  下次 10:10 │ 全局 HUD    │
│                   │ 设备 128 · 离线 10       │             │
│                   │ 告警 3 高 · 1 待你回答   │             │
│                   └──────────────────────────┘             │
│        你：看看 3 号楼                           （右侧胶囊）│
│        大脑                                      计划中 1/3 │
│        已加载「巡检」技能                                  │
│        ┌ 任务 ─────────────────────────────────┐           │
│        │ ✓ 检查离线设备                         │           │
│        │ ▣ 扫描高优先级告警                     │           │
│        │ ○ 生成状态摘要                         │           │
│        └────────────────────────────────────────┘           │
│        ▸ search_things(3号楼)  → 12 台设备                 │
│        ❓ 需要你决定                                       │
│           [确认重连]  [稍后处理]  [跳过]                    │
│        ▸ 子代理：分析离线原因  → 2 台 unknown              │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ 和大脑一起工作…                                    ↑ │  │
│  │ ＋    [需要审批 ▾]                  [模型 ▾]          │  │
│  └──────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────┘
```

### 10.1 版式规则

1. 单列居中，最大宽度约 760px；助手正文文档式，用户消息右侧胶囊。
2. Todo 内联在当轮顶部，紧凑清单；顶部显示 `计划中 1/3`。
3. 工具调用默认一行摘要（`工具名(对象) → 结果`），点击展开参数与输出。
4. `ask_user` 与审批都是内联卡片、阻塞式；两者样式区分。
5. 子代理是内联卡片，可展开看目标/状态/结果，可点进子会话。
6. 会话列表放在左侧抽屉（对话 / 巡检 / 事件分组），不是常驻栏。
7. 空态：欢迎块 + 建议芯片（2 列，移动端 1 列，玻璃风格、错开淡入）。
8. 深色为主、跟随系统；颜色与间距沿用现有双主题 tokens。
9. 不做 KPI 卡片墙、不做假进度。

### 10.2 全局 HUD

顶部浮动卡显示全局动态信息，跨会话保持不变：

- Assistant 状态（大脑）：`idle` 空闲 / `working` 忙碌 / `waiting_for_user` 等待你 / `paused` 暂停 / `error` 异常；
- 设备：总数、在线、离线；
- 告警：高优先级数量；
- 待办：等你回答的问题、等你审批的动作；
- 活动：正在跑的会话数；
- 调度：下次唤醒时间（只显示计划，不承诺准点）。

实时更新，状态变化轻微高亮，不用 toast。点击展开全局详情面板（Assistant 状态、调度健康、活动 Session、待处理项、最近事件），可进入 Assistant 概览。只在 Assistant/对话主区域显示，不污染无关页面。空态显示"大脑空闲，下次检查 10:10"。

页面上两个层次：

- **全局 HUD**：整个世界现在怎么样；
- **会话时间线**：这一件事现在做到哪。

### 10.3 Composer 与安全模式

- 底部圆角 composer：`＋`、权限模式、模型选择、发送。
- 权限三档：
  - `只读`（`read_only`）：只允许查询，适合旁观定时巡检；
  - `需要审批`（`ask`）：读取自动，写设备/下发命令必须确认——**默认**；
  - `完全访问`（`full_access`）：Session 内显式选择才生效，且永远不能绕过 e-stop、联锁和 Effect 边界。

## 11. Safety

统一循环不改变任何安全语义：

- 所有写设备动作仍走 policy / trust / approval；
- Effect/Decision 单赢家、unknown fence、对账、恢复合同继续有效；
- 子代理继承父会话的权限上限，不能自行提升；
- `完全访问` 只表示"会话内免逐次批准"，不表示"绕过联锁"；
- 无人在线时，写动作不得因"自主运行"而自动放行。

## 12. Migration and Legacy Cleanup

### 12.1 迁移映射

| 现有 | 处理 |
|---|---|
| chat 循环（`host/chat/service.rs`） | 并入 Session Loop |
| thing-agent 循环（`thing_agent/runner.rs`） | 并入 Session Loop；调度器改为 `TriggerRouter` |
| `AutonomousAgentProvider` | 删除独立 prompt 拼装，改用统一 prompt builder |
| `dispatch_thing_task` | 替换为 `spawn_subagent` 或同循环直接执行 |
| `heartbeat_tasks` DB 任务 | 迁移为技能正文步骤；新 `Schedule` 只保留调度状态 |
| `HEARTBEAT.md` 遗留 | 保留只读迁移说明，不再作为真源 |
| 内置技能模板 | 改写为"步骤 + 注意事项"形态 |
| 现有 chat 会话 | 映射为 `Session`（kind = `chat`），历史保留 |
| `brain_events` 视图 / `brain_event` 域 | `agent_actions` / `agent_runs` 分支由 `session_events` 取代；`judgments` 继续走 `/judgments` |
| `AutonomyMode` | 退役，改用 `PermissionMode` |
| `RuntimeEvent` / `SessionEvent` | `RuntimeEvent` 保留为安全/审计账本；`SessionEvent` 为唯一 UI 时间线 |
| `AgentTask` / `AgentRun` 类型名 | 收敛为 `Task` / `Run`（表名不变） |
| policy/trust/Effect/对账 | 不动 |

### 12.2 清理原则

- **只删被本次设计取代的代码**；与本次无关的死代码不动（遵守 CLAUDE.md 的 Surgical Changes）。
- **分阶段删除**：每批删除必须在新路径上线、对应测试通过之后进行；禁止大爆炸。
- **删除独立提交**：同一提交内同步删除对应测试，不留下测试孤儿。
- **数据库不随代码一起删**：表先保留只读，单独迁移/备份后再下掉。
- **安全与恢复内核一律保留**：Effect/Decision/unknown fence/对账、policy/trust、`hub_ledger` 的 `runtime.v1` 审计账本。

### 12.3 删除批次

**Batch 0 — 停止继续沉淀**
- 冻结 `2026-11-03-agent-harness-core.md` 计划，不再实现。
- 旧 spec 标记为"产品方向已被取代；安全、Effect/Decision、恢复合同继续有效"。
- 移除 `tools/types.rs`、`host/tools/handler.rs`、`host/config/handler.rs` 的 `#![allow(dead_code)]`，清掉未使用项。

**Batch 1 — 统一循环切换后**
- 删除 `AutonomousAgentProvider` 及其独立 prompt 拼装。
- 删除 `thing_agent/runner.rs`、`scheduler.rs`、`governor.rs`、`watch_loop.rs`、`pushback.rs` 中被 Session Loop 取代的部分；按文件逐一标注保留/删除。
- `ThingAgentManager` 的调度部分改名为 `TriggerRouter`，执行部分并入 `SessionRunner`。
- 保留 `approval_port.rs`、`registry.rs`、`active_runs.rs`、`traits.rs`、仲裁与审批相关代码。
- 删除 `dispatch_thing_task` 工具及 chat 侧的甩单分支，新增 `spawn_subagent`。
- 删除旧 chat 循环与对应测试。
- `AgentTask` / `AgentRun` 类型名收敛为 `Task` / `Run`，保留 `pub type` 兼容别名一个版本；表名不变。

**Batch 2 — 心跳退役**
- 删除 `host/heartbeat.rs` 的 md 解析与一次性迁移代码。
- 删除 `workspace_heartbeat.rs` 的 legacy 双读分支（`legacy_logs_from_agent_actions`，注释标注"切换完成后删除"）。
- `get_default_tasks()` 的硬编码默认任务改写为 `inspection` 技能正文。
- `heartbeat_tasks` 表保留只读；下一版本迁移为 `Schedule` 后下掉。
- `AutonomyMode` 删除，统一由 `PermissionMode` + Assistant 状态表达。
- 删除 `HEARTBEAT.md` 模板及 `.migrated` 相关代码路径。

**Batch 3 — Workflow 实验退役**
- 删除 `workflows/` 中的 Workflow 定义/修订/模板/dry-run/runner。
- 删除 `harness.rs` 中 Workflow CRUD、模板实例化、simulation/dry-run 端点。
- 先把 `workflows/health.rs`、`workflows/supervisor.rs` 中仍服务 HUD 的调度健康投影迁入 session 域，再删原文件；保留全局概览端点。
- `runtime/harness_stream` 保留并改名 `runtime/session_stream`，作为新 Session 时间线的 SSE；删除仅被 Workflow 使用的分支。
- 前端删除 `harness-workflow-*`、`harness-templates`、`harness-dry-run` 及其测试；`harness-compaction` 若仍服务新会话投影则保留改造，否则删除。
- `agent_workflows` / `workflow_revisions` 表保留只读，独立迁移后再下掉。

**Batch 4 — 命名收敛与兼容层清除**
- 启用 `SessionEvent`（复用 `session_events` 表）作为唯一 UI 时间线；`RuntimeEvent`（`hub_ledger runtime.v1`）保留为安全/审计账本，不合并。
- 删除 `host/skill.rs`、`host/reflect.rs` 的 backward-compat re-export。
- 删除 `host/types.rs` 的 deprecated `ChatTurnMessage`；`host/` 其余内容拆入 `session/`、`tool/`、`prompt/`，不再新增 `host`。
- `brain_events` 视图收窄：`agent_actions` / `agent_runs` 分支由 `session_events` 取代后删除；`judgments` 继续由 `/judgments` 提供。全局最近事件在查询层合并，端点保留一个版本重定向。
- 删除已无调用方的旧 handler 模块与测试。

**Batch 5 — 前端收敛**
- 新 Codex 式界面通过真实浏览器验收后，删除被取代的 `runtime-*` / `harness-*` 视图、旧 `chat.ts`、`agents-*` 中由技能/主 prompt 编辑取代的 Tab。
- 新视图统一 `assistant-*`；删除重复的样式与测试。
- 只保留一个 `assistant-url-state.ts`；旧 `runtime-url-state` / `harness-url-state` 仅作一个版本的书签重定向兼容层。

**Batch 6 — 文档收敛**
- 旧 spec 顶部加状态：产品方向被取代，安全合同仍为规范。
- 未跟踪的 2026-11-03 计划删除或移入 archive。
- 更新 AGENTS.md / CLAUDE.md 中指向旧路径的说明。

### 12.4 保留清单

- `runtime/`：dispatch、controls、tasks、Effect/Decision、恢复与 unknown fence、stream。
- `core::harness`：Effect/Decision/Recovery 类型。
- policy / trust / approval / arbitration。
- `session_events`（唯一 UI 时间线）/ `hub_ledger runtime.v1`（安全/审计账本）/ SSE。
- `Task` / `Run` 持久化记录与恢复语义。
- skills loader、ToolRegistry、MCP。
- 旧书签重定向薄兼容层（一个版本）。

### 12.5 清理验收

- 触达模块内无 `#![allow(dead_code)]`。
- `cargo clippy --workspace --all-targets -- -D warnings` 通过。
- `cargo test --workspace` 全绿，安全回归测试一条不少。
- 前端 `pnpm type-check` / `pnpm build` / `pnpm test` 通过，并完成真实浏览器验收。
- 路由审计：每条保留路由都有真实前端调用方，或被明确标注为兼容层。
- 全局只有一个循环、一个 prompt builder、一套界面。
- `git grep -nE 'ThingAgent|AutonomousAgent|dispatch_thing_task|subagent\.done'` 返回零命中（兼容层与迁移说明除外）。
- `git grep -nE 'runtime-url-state|harness-url-state'` 只命中重定向兼容层。
- 新增代码不出现 5.4.5 的禁用词；`Task` 只表示持久化工作单元，子代理只用 `Subagent` / `spawn_subagent`。

### 12.6 清理风险

- `thing_agent/` 内可能混有安全编排代码：删除前逐文件标注保留/删除，不按目录整删。
- 数据库表下掉需要独立迁移、备份与回滚方案。
- 前端删除可能破坏旧深链接：保留一个版本的重定向。
- 采用分批门禁，避免一次性大删除导致不可定位的回归。

## 13. Recovery

- 会话状态经事件持久化；重启后 `running` 续跑、`waiting_user` 保持等待。
- 执行中的设备动作仍先走 unknown fence 对账，再决定继续或转人工。
- todo 未完成 → 结果 `partial`，报告列出欠项，不伪装完成。
- `ask_user` 无人在线 → 暂停该任务 + 通知，不静默失败。
- 子代理中断 → 父会话收到失败结果，可选择重试或继续。

## 14. Verification

### 14.1 单元

- `todo_write` schema 与状态机（只允许一个 `in_progress`）；
- `ask_user` 1–4 问、2–4 选项、超时；
- skill 渐进披露：索引可见、正文按需注入；
- 子代理隔离与一层深度限制。

### 14.2 集成

- 技能加载 → 建 todo → 工具调用 → 提问 → 恢复 → 完成；
- 同一会话事件按序、可回放；
- 未完成 todo → `partial` 结果。

### 14.3 E2E

- 新 workspace 零配置启动，按主 prompt 产生首次巡检会话；
- 多步任务中途 kill/restart，会话续跑且不重复设备动作；
- 写设备仍触发审批；`只读` 模式拒绝写操作；
- Critical 告警触发应急预案技能；
- 无匹配技能时走通用循环。

### 14.4 回归与清理

- 现有 policy / trust / Effect / unknown / 对账测试全部保留并通过；
- 现有 chat 会话历史可读；
- 迁移期 `/chat/stream` 保持兼容；新路径为 `/sessions/{id}/events`（SSE）；trace 行为不回退；
- 每批删除后按 12.5 重新执行 clippy、测试、前端构建与路由审计。

## 15. Non-Goals

- Plan mode（只读规划 + 批准）——下一期。
- Workflow 声明式引擎退役（按 12.3 彻底清理），不作为产品路径；仅保留 Effect/Decision/恢复内核。
- 多层子代理——只做一层。
- 重写安全、Effect、恢复层。
- 通用文件系统/代码编辑工具——本期聚焦 IoT 域。
- 不追求一次性删完旧代码；按批次推进，每批独立验收。

## 16. Open Questions

1. `spawn_subagent` 同步阻塞对长设备操作是否可接受？是否需要"后台任务 + 完成通知"的混合模式？
2. 每 5 分钟一次唤醒的模型调用成本；是否需要在主 prompt 层做"无事可做则快速返回"的优化？
3. 现有用户配置（心跳间隔、信任配置）如何迁移到主 prompt / 技能，是否需要一次性转换器？
4. 会话列表的分组与保留策略（保留多久、如何归档）？
5. 前端真实浏览器验证（`/design-review`）在实现后进行。

## 17. Decision Log（本会话）

| # | 决策 | 选择 |
|---|---|---|
| 1 | 后端路线 | A：统一主循环 |
| 2 | 失败面 | Agent 干不成任务、不知道如何开始/配置重、交互可见性差 |
| 3 | 大脑默认行为 | 内置技能 + 主 prompt 决定 |
| 4 | 任务清单来源 | 技能正文里的步骤 |
| 5 | 执行语义 | 完全对齐 Claude Code：技能正文写步骤，模型用 `todo_write` 生成并可调整待办（软约束） |
| 6 | 大脑/会话 | 一个大脑，多个会话 |
| 7 | 技能格式 | 主流 SKILL.md，不引入自定义任务 DSL |
| 8 | 默认技能 | 巡检、应急预案、节能等，由主 prompt 决定何时运行 |
| 9 | 前端版式 | 参考 Codex：单列时间线、内联 todo、底部 composer、左侧会话抽屉 |
| 10 | 顶部浮动 | 全局动态 HUD，跨会话不变 |
| 11 | 权限 | 只读 / 需要审批（默认）/ 完全访问；不绕过联锁 |
| 12 | 旧代码清理 | 彻底清理：分 6 批删除双循环旧路径、旧心跳、Workflow 实验、重复前端视图；安全/恢复内核保留 |
| 13 | 命名基线 | 一个概念一个代码名：Assistant / Session / Turn / Task / Run / Todo / Subagent / Skill / Tool |
| 14 | 子代理工具名 | `spawn_subagent`（语义对齐 Claude Code `Task`，避免与持久化 `Task` 冲突） |
| 15 | 事件分层 | `session_events` = 唯一 UI 时间线；`hub_ledger runtime.v1` = 安全/审计账本；事件名一律过去式 |
