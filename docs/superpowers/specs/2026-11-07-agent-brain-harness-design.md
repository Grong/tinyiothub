# Design: Agent Brain Harness — Claude Code 式常驻智能体

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

- 每个 workspace 有一个常驻 Brain；Brain 的身份、记忆、默认职责来自 md 主 prompt。
- Brain 可以开多个会话：对话、定时巡检、事件处置；会话之间共享身份与记忆，但不共享上下文。
- 技能是工作手册（怎么干）；主 prompt 是值班表（什么时候、干什么）。
- 默认零配置可运行；所有默认值都可通过改 md 或直接对话覆盖。
- 物理设备安全边界不因本次改造而放松。

## 4. Target Model — Claude Code 的五个 primitive

| Primitive | 主流做法 | 本设计采纳 |
|---|---|---|
| **Skill** | `SKILL.md` + `name`/`description` frontmatter；渐进披露：上下文只放索引，调用时注入正文 | 采纳；复用现有 `get_skill` 与 `crates/skills/src/loader.rs` |
| **TodoWrite** | 工具参数为 `{ todos: [{ content, status, activeForm }] }`；整体替换；同一时间一个 `in_progress` | 采纳同款 schema，工具名 `todo_write` |
| **AskUserQuestion** | 1–4 个问题，每题 2–4 个选项，可多选；阻塞 turn | 采纳同款语义，工具名 `ask_user` |
| **Task** | 同步子代理：`description` / `prompt` / `subagent_type`；独立上下文，返回最终报告 | 采纳同款语义，工具名 `task` |
| **Plan mode** | 只读探索 + 计划批准 | **本期不做** |

关键取舍：**任务清单采用主流软约束**。技能正文写编号步骤，模型加载后用 `todo_write` 生成待办；系统不解析任务结构。好处是与 Claude Code 行为一致；代价是不能硬保证每一项都执行。用户已明确选择完全对齐主流。

## 5. Architecture

```
                    ┌──────────── 唤醒源 ────────────┐
  用户消息 ─────────┤                                │
  定时 tick ────────┤   Trigger Adapter              │
  设备/告警事件 ────┤   (heartbeat / event / chat)   │
  用户显式指令 ─────┘                │               │
                                    ▼               │
                        ┌───────────────────────┐   │
                        │  Workspace Brain      │   │
                        │  主 prompt + 记忆       │  │
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

1. **触发与执行分离**：`ThingAgentManager` 的调度器保留，但只负责产生 wake；不再自己拼 prompt、不再自己跑 `run_pipeline`。
2. **大脑与会话分离**：一个 workspace 一个 Brain；可以有多个会话，会话之间不共享对话上下文。
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
- `dispatch_thing_task` 的异步甩单 → `task` 工具同款同步语义
- `heartbeat_tasks` 数据库任务 → 技能正文步骤（数据库只保留调度状态）
- chat 循环与 thing-agent 循环 → 一个 Session Loop

**不替换**：物理设备安全边界。统一循环只统一"谁在什么上下文里调用工具"，不放松任何一道闸门。

### 5.3 组件落位

| 组件 | 位置 |
|---|---|
| Session Loop、会话类型、Trigger Adapter | `apps/cloud/src/domains/agent/session/`（新增） |
| `todo_write` / `ask_user` / `task` 工具 | `apps/cloud/src/domains/agent/host/tools/` |
| 统一 prompt builder | `apps/cloud/src/domains/agent/prompt/mod.rs`（扩展） |
| 事件新增字段 | `crates/agent/src/port/events.rs` |
| 技能加载与索引 | `crates/skills/src/loader.rs`（复用，不加任务 DSL） |
| 前端会话时间线 / HUD / composer | `web/src/ui/views/`、`web/src/styles/` |

Brain 的主 prompt 沿用现有 md 组合（IDENTITY / SOUL / AGENTS / TOOLS / MEMORY / USER）。**默认职责写在 `AGENTS.md`**，workspace 目录可覆盖共享模板；用户改这个文件就是"告诉大脑注意什么、干什么活"。

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

**由 Brain 的主 prompt 决定，不由每个技能自带 schedule。** 主 prompt 示例：

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

### 7.3 task

- 同步派生子代理，父循环等待并拿到最终报告。
- 子代理上下文隔离，事件挂在父会话下。
- 最多一层，不做三层套娃。
- 输入：`description` / `prompt` / `subagent_type`；输出：`status` / `summary` / `evidence_refs` / `recommended_actions`。

## 8. Events and Visibility

在现有 `TurnEvent` 上补充会话级 span：

```
skill.loaded      { name, version }
todo.updated      { todos[] }
question.asked    { id, questions[] }
question.answered { id, answers[] }
subagent.started  { task_id, description }
subagent.done     { task_id, status, summary }
session.started   { session_id, kind, skill? }
session.ended     { session_id, outcome }
```

现有 `Chunk / Thinking / ToolCall / ToolResult / ApprovalRequest / Usage` 全部保留。一个会话就是一条按序时间线；前端不需要猜测任务边界。

## 9. Default Behavior

- 新 workspace 创建 → Brain 启动 → 内置技能可用 → 主 prompt 决定首个动作。
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

- 大脑状态：运行中 / 忙碌 / 等待你 / 暂停 / 异常；
- 设备：总数、在线、离线；
- 告警：高优先级数量；
- 待办：等你回答的问题、等你审批的动作；
- 活动：正在跑的会话数；
- 调度：下次唤醒时间（只显示计划，不承诺准点）。

实时更新，状态变化轻微高亮，不用 toast。点击展开全局详情面板（大脑状态、调度健康、活动会话、待处理项、最近事件），可进入完整 Harness 概览。只在 Harness/对话主区域显示，不污染无关页面。空态显示"大脑空闲，下次检查 10:10"。

页面上两个层次：

- **全局 HUD**：整个世界现在怎么样；
- **会话时间线**：这一件事现在做到哪。

### 10.3 Composer 与安全模式

- 底部圆角 composer：`＋`、权限模式、模型选择、发送。
- 权限三档：
  - `只读`：只允许查询，适合旁观定时巡检；
  - `需要审批`：读取自动，写设备/下发命令必须确认——**默认**；
  - `完全访问`：会话内显式选择才生效，且永远不能绕过 e-stop、联锁和 Effect 边界。

## 11. Safety

统一循环不改变任何安全语义：

- 所有写设备动作仍走 policy / trust / approval；
- Effect/Decision 单赢家、unknown fence、对账、恢复合同继续有效；
- 子代理继承父会话的权限上限，不能自行提升；
- `完全访问` 只表示"会话内免逐次批准"，不表示"绕过联锁"；
- 无人在线时，写动作不得因"自主运行"而自动放行。

## 12. Migration

| 现有 | 处理 |
|---|---|
| chat 循环（`host/chat/service.rs`） | 并入 Session Loop |
| thing-agent 循环（`thing_agent/runner.rs`） | 并入 Session Loop；调度器改为 Trigger Adapter |
| `AutonomousAgentProvider` | 删除独立 prompt 拼装，改用统一 prompt builder |
| `dispatch_thing_task` | 替换为 `task` 工具或同循环直接执行 |
| `heartbeat_tasks` DB 任务 | 迁移为技能正文步骤；表只保留调度状态 |
| `HEARTBEAT.md` 遗留 | 保留只读迁移说明，不再作为真源 |
| 内置技能模板 | 改写为"步骤 + 注意事项"形态 |
| 现有 chat 会话 | 映射为 `chat` 类型会话，历史保留 |
| policy/trust/Effect/对账 | 不动 |

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

### 14.4 回归

- 现有 policy / trust / Effect / unknown / 对账测试全部保留并通过；
- 现有 chat 会话历史可读；
- `/chat/stream`、SSE、trace 行为不回退。

## 15. Non-Goals

- Plan mode（只读规划 + 批准）——下一期。
- Workflow 声明式引擎作为主路径——降为高级自动化，本期不推进。
- 多层子代理——只做一层。
- 重写安全、Effect、恢复层。
- 通用文件系统/代码编辑工具——本期聚焦 IoT 域。

## 16. Open Questions

1. `task` 同步阻塞对长设备操作是否可接受？是否需要"后台任务 + 完成通知"的混合模式？
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
