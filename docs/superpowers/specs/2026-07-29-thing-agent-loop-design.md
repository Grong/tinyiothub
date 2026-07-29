# Thing Agent Loop — AI 自治驱动物本体设计

> 日期：2026-07-29
> 状态：已确认（brainstorming 逐节批准 + /plan-ceo-review SELECTIVE EXPANSION 裁决）
> 背景：物本体（Thing Ontology）集成改造已完成（物/模板/事件管线/9 个本体工具/invoke_action 确认门）。本期实现"本体智能"的下一个阶段：**AI 自治驱动物**——感知→决策→行动→验证的无人闭环。这是"AI 驱动本体"四个方向中的第一个子项目（其余三个：AI 构建与进化本体、A2UI 本体渲染、智能问答入口，各自独立 spec 周期）。

## 修订记录 v2（2026-07-29，CEO 审查 + 三轮对抗评审 + 外部声音）

/plan-ceo-review（SELECTIVE EXPANSION）接受 6 项扩展（X1-X6，见 CEO Plan `~/.gstack/projects/Grong-tinyiothub/ceo-plans/2026-07-29-thing-agent-loop.md`），三轮对抗评审（27 项）与外部声音（14 项）全部裁决。**以下裁决与本文其余章节冲突时，以本节为准。**

| # | 裁决 |
|---|------|
| O1 | **run_single → 流式 turn_streamed + TurnEvent 拦截**（决策 #3、架构图、Run 生命周期三处）。核实 zeroclaw：ToolCall 事件 fire-and-forget 不暂停，因此硬性上限只在 tool 内策略门强制；流式侧负责轨迹捕获（R1 verified 客观判定）、预算监控、best-effort abort（CancellationToken）。abort 返回 `Err(ToolLoopCancelled)` 无 LLM 收尾，RunReport/清单由框架从轨迹合成 |
| O2 | **决策 #9/R4 修订**：心跳 runner 不动，仅加"结论投递出口"（X6 桥接：心跳诊断结论作为 UserDirective 投递，带 `problem_key` 与 `source=heartbeat:{tick_id}`） |
| O3 | **§八 thing_agent/policy.rs 作废**：策略逻辑收进 `crates/tinyiothub-ai/src/policy`——PolicyEngine trait 上新建 SQLite 持久化实现（现状仅 NoopPolicyEngine），扩展 `PolicyDecision::RequireApproval` 与计频规则；自有新表 `policy_rules`。**X3 全有或全无**：若砍 X3，连 policy_rules 表/策略端点都不建 |
| O4 | **agent_runs 加列**：`problem_key TEXT NULL`、`dedup_key TEXT`、`acked_at/acked_by`；API 加 `POST /agent/runs/{id}/ack`；索引 `(workspace_id, problem_key, created_at)` 与 `(workspace_id, dedup_key, created_at)` |
| O5 | **§二 UserDirectiveTrigger**：`TriggerSource::UserDirective` 加 `source` 字段；心跳来源 directive 降为 Normal、参与合并、不享"排队不丢"；用户指令队列上限 50/工作区，超出拒绝并告知；同工作区同文本指令 60s 去重（双击/重试） |
| O6 | **自治策略三态**：`mode: off | diagnose | act`（替代 enabled 布尔）——off=不起 Run 零 LLM 成本；diagnose=原"只诊断不行动"；act=完全自治。默认 off |
| O7 | **策略逐次现读**（废快照）：每次 invoke_action 现查策略（SQLite 点查），kill switch 即时生效；DB 读失败 fail-closed（V10 先例复述） |
| O8 | **RunContext = `Arc<RwLock<RunContext>>`**：per-workspace 一个自治 Agent 实例（串行调度保证无竞争），每 Run 换内容；与 chat 的 Agent 实例隔离 |
| O9 | **预算口径**：工具调用 25 次（zeroclaw 并行派发，"轮"不是可靠单位）+ 时长 5min，流式侧硬截断；每 Run/每物动作硬上限 tool 内强制；token 本期仅监控记录不设硬上限 |
| O10 | **Critical 事件绕过 30s 合并窗口**直接入队（与"数秒内唤醒"验收对齐） |
| O11 | **X6 dedup 规则（6h 窗口、窗口内计数、全 outcome 覆盖）**：failed/rejected/budget_exceeded → 跳过；acted+verified / no_action_needed → 跳过；acted+未 verified → 窗口内已放行过一次则跳；超 6h 不抑制（复发可再处置）；已 ack 的 problem_key 抑制 7 天 |
| O12 | **chat 回推 = 复用 `history::append_message`**（chat/service.rs:271 同款，SQLite 直写 assistant 消息，零 LLM 成本）；live SSE 推送留 TODOS |
| O13 | **安全节补强**：事件 payload 以 `<event_data>` 围栏、用户指令以 `<user_directive>` 围栏进 prompt（沿用 `<user_document>` 先例）；新端点（tasks/runs/ack/策略）全部工作区隔离 + admin 角色（V5 复述）；注入防护=围栏+动作名单+计频熔断三层 |
| O14 | **可观测性补充**：广播 channel 满丢弃记 `agent_wake_dropped`；`agent_action_denied` 激增告警（策略过紧或行为漂移信号）；kill switch runbook（off→全部 Run 停止） |
| O15 | **测试节补充**：X1-X6 用例行（见 CEO Plan）、LLM 无响应 5min 时长截断、事件 payload 注入文本下 denylist 仍生效 |
| O16 | **R6（新）**：zeroclaw abort 验证为 go/no-go 检查——若 abort 不停 LLM 循环，B 方案=工具内计数拒绝（RunContext 计数器超限即拒绝后续调用） |

**接受的扩展（叠加本期范围）**：X1 历史处置注入（同 dedup_key 历史 Runs ≤3 条 ×≤200 字进 prompt）、X2 失败人工清单（无 LLM 时框架从轨迹合成；无会话 Run 推最近活跃会话）、X3 统一策略面（三接入面：chat 确认/心跳 trust 适配/thing_agent 门）、X4 token 日聚合（普通视图 + Run 落库时发 `agent_tokens_daily` 指标）、X5 放宽 hint-only（`policy_relax_hint` 字段，UI 留 A2UI）、X6 心跳桥接（O2/O11）。

**新延后项**（入 TODOS）：live SSE 回推、TrendAnomalyTrigger、GoalTrigger、Runs/策略 UI 面板、heartbeat_trust_config 旧表下线、心跳迁入 Trigger 框架、POST tasks 前端面板。

## 核心决策汇总

| # | 决策点 | 结论 |
|---|--------|------|
| 1 | 总体路线 | 方案 B：新建独立 Thing Agent Loop 子系统（`crates/tinyiothub-ai/src/thing_agent`），不在心跳循环上演进，不做 per-thing agent |
| 2 | Loop 粒度 | per-workspace 一个自治 Loop，串行执行（复用 HeartbeatManager/AgentPool 模式）；物通过本体工具发现与操作 |
| 3 | Loop 内核 | **复用 zeroclaw agentic loop（run_single + 工具），不写刚性阶段编排**。Sense/Plan/Act/Verify 全部由 system prompt 纪律 + 工具设计约束（对标 Claude Code：harness 只做触发/权限/上下文/中断，循环由模型自主驱动）。吸取 2026-07-06 harness loop 刚性流水线被 revert（e38e404e）的教训 |
| 4 | 自治级别 | L4 完全自治：动作不需逐次人工确认，约束由预声明策略门承担；安全栏杆 = kill switch + 名单 + 熔断 + 共振防护 + 全量审计 |
| 5 | 触发源 | 本期三个：物事件实时唤醒、定时巡检、用户指令（chat 工具投递）；趋势异常、持续目标只留 Trigger 接口不实现 |
| 6 | 确认机制分工 | chat 链路的 `require_action_confirm` 确认令牌**不动**；自治 Run 内 invoke_action 走自治策略门。两条场景线互不干扰 |
| 7 | 共振防护 | AI 动作产生的事件（`actor=agent`）不唤醒 ThingEventTrigger，从根上断掉"动作→事件→唤醒→再动作"死循环 |
| 8 | 前端范围 | 本期只做 chat 回推可见；Runs 列表/策略配置保证 API 完整，UI 面板留后续迭代（A2UI 子项目是其天然展示层） |
| 9 | 既有心跳 | 本期完全不动；后续作为 TimerTrigger 迁入时自然统一 |

## 一、架构总览

```
触发器（可插拔）                    per-workspace 调度                 闭环执行
┌──────────────────┐   WakeSignal   ┌────────────────────┐          ┌─────────────────────────┐
│ ThingEventTrigger │ ────────────→ │ mpsc 队列 + 串行消费者 │ ──────→ │ RunContext 构建          │
│ TimerTrigger      │   30s 合并窗口  │ 同 workspace 同时只跑 │          │ system prompt 四段式      │
│ UserDirective     │   优先级排序    │ 一轮；每小时唤醒熔断  │          │ zeroclaw run_single+工具  │
│ (TrendAnomaly 预留)│               └────────────────────┘          │   └ invoke_action→策略门  │
│ (Goal 预留)       │                                               │ RunReport 落库/记忆/回推  │
└──────────────────┘                                               └─────────────────────────┘
```

## 二、触发器与调度

### Trigger 抽象

```rust
trait Trigger: Send + Sync {
    fn name(&self) -> &'static str;
    async fn run(&self, tx: mpsc::Sender<WakeSignal>) -> Result<()>;  // 长期运行，往队列发信号
}

struct WakeSignal {
    workspace_id: String,
    priority: Priority,          // Critical > High > Normal > Low
    source: TriggerSource,
    dedup_key: String,           // 合并窗口内同 key 信号合并
}

enum TriggerSource {
    ThingEvent { thing_id: String, event_name: String, event_id: i64, level: EventLevel, data: Value },
    Timer,
    UserDirective { user_id: String, text: String, session_key: Option<String> },
    TrendAnomaly { /* 预留，本期不实现 */ },
    Goal { /* 预留，本期不实现 */ },
}
```

### 本期三个触发器

| 触发器 | 机制 | 关键行为 |
|---|---|---|
| `ThingEventTrigger` | 订阅既有事件管线：物事件路由函数写入 events 表后同步进程内广播，**不轮询 DB** | 工作区级配置 `min_wake_level`（默认 warning）：info 不唤醒；`unknown_event=true` 不唤醒；状态类/发生类事件均可触发，携带事件上下文 |
| `TimerTrigger` | per-workspace 间隔巡检（默认 15min，可配） | 到点发 Normal 优先级信号，无具体事件上下文，AI 自主用本体工具巡检 |
| `UserDirectiveTrigger` | 用户行动指令异步闭环执行 | 优先级 High，**不合并**（每条用户指令独立执行）；计入每小时唤醒上限，但超限时不丢弃而是排队延迟 |

用户指令的两个入口：

1. **chat 工具 `dispatch_thing_task`**：对话 Agent 判断用户意图是"去执行某件事"而非"回答我"时，投递指令进 Loop，立即回复"已受理"；Loop 执行完结果回推该 chat 会话
2. **管理 API** `POST /api/workspaces/{id}/agent/tasks`（前端自治任务面板的后备，本期保证 API 可用）

**与 chat 链路的分工**：对话问答（"这台设备多少度？"）走现有 chat 链路同步响应，不进 Loop；行动指令（"去把温度调下来"）经 dispatch 进 Loop 异步闭环。

### 调度规则

- 每工作区一个 mpsc 队列 + 一个串行消费者：同一时刻一个工作区只跑一轮闭环，避免 AI 并发操作同一批物自相矛盾
- **合并窗口**：同 `dedup_key`（如 `thing:{id}:event:{name}`）的信号在 30s 窗口内合并为一次唤醒，聚合多条事件上下文一次交给 AI（告警风暴时 AI 看全貌而非被刷屏）
- **成本熔断**：每工作区每小时唤醒上限（默认 20 次，可配）；超限后低优先级信号丢弃，Critical 放行，记录 `agent_wake_throttled` 指标；用户指令超限排队不丢
- 工作区创建/删除时 Loop 自动启停（复用 Orchestrator 的 WorkspaceCreated/Deleted 事件接线模式）

## 三、闭环执行（一次 Wake → 一次自治行动）

### Run 生命周期

```
WakeSignal 出队
  → 构建 RunContext（触发源上下文 + 工作区自治配置 + 相关物本体信息 + 最近 5 条 Run 摘要）
  → 拼装 system prompt（四段式）
  → zeroclaw run_single(ws_id, prompt, tools)   ← 模型自主驱动，无刚性编排
  → 结束 → RunReport 落库 + 记忆更新 + （用户指令）回推 chat
```

### System prompt 四段式（prompt 纪律替代刚性流水线）

| 段 | 内容 | 作用 |
|---|---|---|
| 角色段 | "你是 {workspace} 的自治运维 Agent，被 {触发源} 唤醒" | 定身份 |
| 触发段 | 事件：事件名/级别/物 id/payload/面包屑；定时：无事件自主巡检；用户指令：原文 + 用户 id；**记忆：最近 5 条 Run 摘要** | 定任务 + 连续性 |
| 纪律段 | 行动纪律三条：①行动前先感知（get_thing_profile 了解现状再动手）②**行动后必须验证**（invoke_action 后 read_property/query_events 回读确认生效，未验证不得宣称完成）③做不到就说不做（工具全失败时禁止虚报成功） | 定规矩（Verify 在此，不是代码阶段） |
| 边界段 | 本次可用动作范围（策略门决议）+ 预算（最大工具轮次） | 定边界 |

### 工具与预算

- 工具集：自治 Run 内复用现有 9 个本体工具，其中 `invoke_action` 走策略门（第四节）而非确认令牌；`dispatch_thing_task` 仅注册在 chat 链路（用户指令入口），自治 Run 内不注册
- **防失控预算**：单次 Run 硬上限——最大工具轮次 15、单物最大动作 3 次、总时长 5min；任一超限强制收尾，Report 标记 `budget_exceeded`

### RunReport

```rust
struct RunReport {
    run_id: String,
    workspace_id: String,
    trigger: TriggerSource,
    outcome: Outcome,  // Acted | NoActionNeeded | Failed | BudgetExceeded | Rejected
    summary: String,              // LLM 自述
    actions: Vec<ActionRecord>,   // 每次 invoke_action：目标物/动作/参数/结果/验证结果
    verified: bool,               // 所有动作是否都完成回读验证
    duration_ms: u64,
    tool_rounds: u32,
    tokens: u64,
}
```

## 四、自治策略门（L4 的安全栏杆）

约束从"每次问人"换成"预先声明的规则"。拦截点只有一个：**自治 Run 内的 invoke_action 调用**。

### 配置模型（新表 `workspace_autonomy_policy`，每工作区一行）

| 字段 | 默认 | 说明 |
|---|---|---|
| `enabled` | **OFF** | 总开关（kill switch）。新工作区默认关闭自治；关闭时触发器照常发信号但 Run 内所有动作被拒，AI 退化为"只诊断不行动" |
| `allowed_actions` | `["*"]` | 动作白名单（模板动作名，支持 `*`） |
| `denied_actions` | `[]` | 黑名单，优先级高于白名单（如 `firmware_update`、`factory_reset` 永拒） |
| `max_actions_per_run` | 3 | 单次 Run 动作上限 |
| `max_actions_per_hour` | 30 | 工作区级动作频率熔断（防 AI-设备共振） |
| `constraints` | NULL | 预留 JSON 列（参数值域/时间窗口等，本期不实现逻辑） |
| `updated_by / updated_at` | — | 配置变更可审计 |

### 裁决流（每次 invoke_action 同步评估，毫秒级）

```
enabled? ──否──→ 拒绝(自治未开启)
  │是
黑名单? ──是──→ 拒绝(动作被禁止)
  │否
白名单? ──否──→ 拒绝(动作未授权)
  │是
频率上限? ──超──→ 拒绝(熔断)
  │否
  放行 → 走既有命令下发通道，决策落审计
```

### 被拒的后续

- 拒绝以结构化工具结果返回 LLM（`{denied: true, reason}`），模型可当次调整策略或在 Report 说明"建议人工执行 X"
- 同一 Critical 事件连续 3 次因策略被拒 → 经既有告警通道升级通知人（"AI 需要权限"）

### 共振循环防护

AI 动作产生的事件在事件管线标记 `actor=agent`，ThingEventTrigger 跳过——只有人工/设备自主产生的事件才唤醒 AI。

### 预留不做（YAGNI）

参数值域约束（setpoint 限 16-30°C）、时间窗口（仅夜间允许）、多级审批降级。

## 五、数据模型、审计、记忆与报告回推

### 新增两张表（一个小迁移，遵循既有迁移模式）

```sql
workspace_autonomy_policy   -- 第四节策略配置，workspace_id 主键
agent_runs                  -- 每次闭环运行一条
  ├── id, workspace_id, trigger_type, trigger_context(JSON)
  ├── outcome TEXT,            -- acted/no_action/failed/budget_exceeded/rejected
  ├── summary TEXT, report JSON,  -- 完整 RunReport（含 actions 数组与验证结果）
  ├── verified INTEGER, tool_rounds INTEGER, tokens INTEGER, duration_ms INTEGER
  └── created_at  -- 索引: (workspace_id, created_at)
```

**不新建的**：动作级审计复用既有——invoke_action 已有审计日志（操作者/时间/目标物），自治动作以 `actor=agent, run_id` 落入同一通道。

### 记忆连续性

- 每次 Run 结束把 `trigger + outcome + 一句话 summary` 写入既有 memory service（per-workspace）
- 下次唤醒时最近 5 条 Run 摘要注入 prompt 触发段——避免重复劳动和重复动作

### 报告回推（按触发源分路）

| 触发源 | 回推路径 |
|---|---|
| 用户指令 | 写 agent_runs 后向该 chat 会话回推一条 assistant 消息（结果摘要 + 动作清单 + 验证状态），走既有 SSE 通道；会话不在线则下次加载可见 |
| 物事件 / 定时 | 静默落库；`outcome=failed` 或 `rejected`（需人介入）时经既有告警通道发通知；全部 Runs 可经 `GET /api/workspaces/{id}/agent/runs` 分页查询 |

### 可观测性指标

结构化日志 metric 字段（沿用事件管线先例）：`agent_wake{source}` / `agent_wake_throttled` / `agent_run_completed{outcome}` / `agent_action_allowed` / `agent_action_denied{reason}` / `agent_run_duration` / `agent_budget_exceeded`。

### 前端范围（本期最小）

只做 chat 回推可见；Runs 列表/策略配置面板只保证 API 完整可用，UI 留后续迭代（A2UI 子项目天然是其展示层）。

## 六、错误处理

- LLM 调用失败/超时：Run 以 `outcome=failed` 落库，不静默；用户指令场景回推失败说明
- 动作下发失败（设备离线等）：复用既有命令通道行为，LLM 在 Report 中如实报告（纪律段第三条）
- 策略门全部拒绝：Run 降级为只诊断，`outcome=rejected`，按需升级通知
- 预算超限：强制收尾，`outcome=budget_exceeded`，已执行动作如实记录
- 唤醒队列积压：合并窗口 + 熔断兜底；队列满时低优先级丢弃记指标，Critical 与用户指令不丢

## 七、测试策略与验收

沿用项目铁律：**集成测试用 sqlx 真实 DB，禁 mock-only；唯一可 mock 的是 LLM**（StubLlm 按剧本应答）。

### 单元测试

合并窗口去重（同 dedup_key 30s 聚合）、优先级队列排序、每小时唤醒熔断计数、策略门裁决矩阵（关/黑名单/白名单/频率全组合）、预算强制（15 轮/3 动作/5min 截断）、`actor=agent` 事件过滤。

### 集成测试（真实 DB + 真实事件管线，仅 mock LLM）

| 用例 | 断言 |
|---|---|
| 事件全链路 | 真实 MQTT 上报 warning 事件 → events 落库 → WakeSignal → Run 执行 → invoke_action 经策略门放行 → 命令真实下发到模拟驱动 → RunReport 落库 + verified=true |
| 共振防护 | AI 动作产生的事件（actor=agent）→ 断言无新 WakeSignal |
| 策略门 | enabled=false 时动作全拒且 Run 降级只诊断；黑名单动作被拒；超 max_actions_per_hour 熔断拒绝 |
| 合并窗口 | 同物同名事件 30s 内 5 条 → 仅 1 次唤醒且上下文聚合 5 条 |
| 用户指令闭环 | chat 工具 dispatch → Run 执行 → 该会话收到回推 assistant 消息（结果+动作清单） |
| 级别过滤 | info 事件不唤醒；unknown_event 不唤醒 |
| 熔断豁免 | 超上限普通唤醒被 throttle，Critical 事件仍放行；用户指令排队不丢 |

### 端到端验收场景（真实模拟设备 + 真实 LLM，手动演示脚本）

> 模拟温控设备上报"温度超限"事件 → AI 数秒内唤醒 → 查询该物本体与近期事件 → 决策"调低设定温度" → 自主下发动作 → 回读属性确认设定值变更、后续遥测温度回落 → chat/Runs API 可见完整报告（含验证证据）→ 再次同事件唤醒时 AI 记忆显示上次处置，不重复动作。

**验收红线**（吸取 dead event path 前科）：不接受 mock 事件源、不接受 mock 命令下发通道——事件从 MQTT topic 进、动作从真实驱动通道出。

**性能基线**：事件落库到 WakeSignal 入队 < 100ms（进程内广播，不轮询 DB）。

## 八、代码归属与实施边界

- 新模块 `crates/tinyiothub-ai/src/thing_agent/`：`trigger/`（三个触发器 + 抽象）、`scheduler.rs`（队列/合并/熔断）、`runner.rs`（Run 生命周期/prompt 拼装/预算）、`policy.rs`（策略门）、`report.rs`（RunReport/落库/回推）
- 改动点：事件路由函数加进程内广播 + `actor` 标记；agent 工具注册处加 `dispatch_thing_task`；invoke_action 工具加自治上下文分支（走策略门）；Orchestrator 接线 Loop 启停；chat service 接收回推消息
- 不改动：chat 同步问答链路、心跳循环、require_action_confirm 确认令牌流、既有 9 个本体工具的语义

## 九、风险登记

- R1：prompt 纪律不如代码强制可靠——模型可能跳过验证。缓解：Report.verified 字段由框架根据工具调用轨迹**客观判定**（invoke_action 后是否有回读调用），不采信 LLM 自述；verified=false 的 Run 在指标与 UI 可见
- R2：L4 自治误操作物理设备。缓解：默认 OFF、名单、熔断、全量审计、共振防护；验收场景先跑模拟设备
- R3：LLM 成本失控。缓解：唤醒熔断 + Run 预算（轮次/时长/token 记录）
- R4：与心跳循环长期两套巡检语义并存。缓解：本期明确不动，后续 TimerTrigger 统一
- R5：zeroclaw run_single 的循环行为不可控（轮次/停止条件）。缓解：预算硬截断在框架侧实现，不依赖 zeroclaw 自觉
