# AI Harness Loop Design

## 设计目标

将 AI 子系统的所有执行路径（Chat、Heartbeat、Workspace）统一到同一个 Harness Loop 中，实现对 AI 行为的完整运行时控制。

## 核心公式

```
Agent = Model + Harness
```

Harness 是模型决策和真实世界之间的控制层。同一个模型在不同 harness 下，性能差异大于不同模型在同一个 harness 下。

## 现状问题

当前三条执行路径互相独立，缺乏统一控制：

```
Chat:      用户消息 → zeroclaw Agent → 工具调用 → SSE 输出
Heartbeat: Timer → build_prompt → LLM → parse_report → publish_event
Workspace: 没有 AI 执行路径
```

具体问题：

1. **Chat 工具调用绕过 TrustEngine** — `chat/service.rs` 中 zeroclaw 的工具调用直接透传为 `ChatEvent::ToolCallStart`，没有任何信任检查
2. **LLM 执行无步骤约束** — Skill Markdown 有步骤描述，但 LLM 可以跳过、绕路、乱序执行
3. **无工具结果验证** — 工具返回成功就当作成功，不验证副作用是否真的生效
4. **无虚报检测** — LLM 可能在所有工具调用都失败的情况下依然报告成功
5. **Proposal 链路不通** — TrustEngine 输出了 Propose 决策，但没有后续的审批-执行闭环

## 架构概览

### 统一入口

```
Chat 消息 ──┐
Timer 到点 ──┼──→ Harness Loop ──→ LoopReport
Event 事件 ──┘
```

### 6 阶段流水线

```
Wake → Load Context → Plan → Execute → Verify → Report → Sleep
```

三个触发源的差异仅在 Wake 阶段（触发方式）和 Report 阶段（输出格式）。中间 4 个阶段完全相同。

---

## 阶段 1：Wake

接收触发信号，决定是否唤醒 Loop。

| 触发源 | 信号类型 | 优先级 | 可被拒? |
|--------|---------|--------|---------|
| Timer | `LoopSignal::Timer` | Normal | 可合并（多次 Timer 只唤醒一次） |
| Event | `LoopSignal::External(AlarmSignal)` | High/Critical | 不可拒绝 |
| Chat | `LoopSignal::Chat(user_message)` | High | 不可拒绝 |

```rust
struct LoopSignal {
    source: SignalSource,
    priority: SignalPriority,
    payload: SignalPayload,
}

enum SignalSource { Timer, Event, Chat }

enum SignalPayload {
    Timer,
    Alarm(AlarmEvent),
    Chat { message: String, session_key: String, user_id: String },
}
```

---

## 阶段 2：Load Context

根据触发源加载 Loop 执行所需的上下文。

```rust
struct LoopContext {
    workspace_id: String,
    agent_id: String,
    trust_config: TrustConfig,
    tasks: Vec<HeartbeatTask>,
    skill_index: SkillIndex,
    history: Vec<PreviousTickResult>,
    system_prompt: String,
}
```

Chat 场景额外注入：`session_key`、`user_id`、`message`。

---

## 阶段 3：Plan

将任务分解为结构化的执行步骤。

### 步骤数据结构

```rust
struct PlanStep {
    id: String,                // "1", "2", "3"
    title: String,             // "收集信息"
    required: bool,            // false → 失败可跳过
    max_retries: u32,          // 最大重试次数
    tool_hints: Vec<String>,   // 建议工具列表
    on_failure: FailureAction,
}

enum FailureAction {
    Retry { max: u32 },
    SkipAndContinue,
    Escalate { message: String },
}
```

### Prompt 注入格式

Plan 阶段将步骤列表注入 LLM prompt，使用强制约束语言：

```
╔══════════════════════════════════════════╗
║  EXECUTION PLAN — FOLLOW IN ORDER       ║
║  You MUST complete each step BEFORE     ║
║  moving to the next.                    ║
╠══════════════════════════════════════════╣
║  Step 1/3: 收集信息                    ║
║  Required: YES    Max Retries: 2        ║
║  Tools: alarm_list, get_device          ║
║  On Failure: escalate "无法获取设备信息" ║
║                                         ║
║  Step 2/3: 上下文分析                   ║
║  Required: NO                           ║
║  Tools: search_devices                  ║
║  On Failure: skip → Step 3              ║
╚══════════════════════════════════════════╝

After EACH step, output EXACTLY:
{"step_report": {"step_id":"1","status":"done|failed|skipped","output":"...","tool_calls":[{"name":"...","success":true|false}]}}

Continue to next step ONLY after outputting step_report.
```

### Skill 匹配

根据任务文本匹配 Skill：

```
任务 "排查传感器异常" → 匹配 troubleshooting skill
→ 提取 workflow 步骤 → PlanStep[]
任务 "统计本月用电量" → 匹配 workspace skill（数据分析子流程）
→ 提取 workflow 步骤 → PlanStep[]
```

---

## 阶段 4：Execute

每个 PlanStep 内的工具调用走三段式拦截。

### PreToolUse → Execute → PostToolUse

```
LLM 想调 write_properties(device_id="d1", temp=25)
  │
  ├─ PreToolUse ──────────────────────────
  │  1. TrustEngine.evaluate(tool_name)   ← 工具安全检查
  │  2. 参数范围校验                       ← 防止离谱参数
  │  3. 速率限制                           ← 每 tick 限制
  │  4. 决策：Allow / Propose / Block
  │
  ├─ Execute ─────────────────────────────
  │  实际调用工具，记录原始输入输出
  │
  └─ PostToolUse ─────────────────────────
     1. 返回值格式校验
     2. 副作用验证（write 类工具自动回读）
     3. 审计日志
```

### PreToolUse 决策

```rust
struct PreToolResult {
    decision: Decision,
    reason: String,
}

enum Decision {
    Allow,
    Propose { reason: String },   // 需人工批准
    Block { reason: String },     // 直接拒绝
}
```

### PostToolUse 副作用验证

最关键的一步：工具返回成功 ≠ 真的生效。

```
write_properties(d1, temp=25) → success
  ↓ PostToolUse
  ↓ 检测到这是 write 类工具，需要回读
  ↓ 自动调用 read_properties(d1)
  ↓ temp == 25 → 确认成功 ✅
  ↓ temp == 30 → 写入未生效 ❌ → 触发重试
```

工具元数据声明是否需要回读：

```rust
struct ToolMeta {
    name: String,
    safety: ToolSafety,
    needs_verification: bool,           // write 类工具 = true
    verification_tool: Option<String>,  // write_properties → read_properties
}
```

### Proposal 处理（不阻塞后续步骤）

当 PreToolUse 返回 Propose 时：

1. 创建 Proposal 写入 DB
2. 发布 ProposalCreated 事件 → 前端通知用户
3. 当前 step 标记为 `awaiting_approval`
4. Loop **继续执行其他 step**，不等待
5. 用户批准 → ProposalResolved 事件 → Loop 收到信号 → 重新执行被暂停的 step

---

## 阶段 5：Verify

两级验证：不信任 LLM 的自报告。

### 第一级：自报告 vs 实际执行比对

```rust
fn cross_check(report: &StepReport, calls: &[ToolCallRecord]) -> StepVerdict {
    // report 是 LLM 输出的 step_report
    // calls 是 Execute 阶段实际记录的工具调用

    if report.status == "done" {
        if calls.iter().all(|c| !c.success) {
            return StepVerdict::Lying {
                reason: "所有工具调用都失败了，但 LLM 报告成功"
            };
        }
        if calls.is_empty() && report.required {
            return StepVerdict::Incomplete {
                reason: "required step 没有执行任何工具调用"
            };
        }
    }
    StepVerdict::Consistent
}
```

### 虚报后果

- 首次虚报 → 注入警告到下一次 tick 的 Plan prompt
- 连续 3 次虚报 → Agent 标记为 `degraded`，只能查询不能写操作，需人工恢复

### 第二级：Tick 质量门禁

```rust
enum TickVerdict {
    Pass,
    Partial { escalated: Vec<String> },   // 部分失败但已生成 Proposal
    Fail { reason: String },              // 不可恢复
}
```

---

## 阶段 6：Report

根据 Verify 结果分发。

```
TickVerdict::Pass
  → 发布 HeartbeatCompleted（含 executed_actions、summary）
  → Chat 场景：通过 SSE 输出最终结果

TickVerdict::Partial { escalated }
  → 发布 HeartbeatCompleted + 为每个 escalation 发布 ProposalCreated
  → Chat 场景：输出部分结果 + "以下操作需要您批准"

TickVerdict::Fail
  → 发布 HeartbeatPersistFailed
  → 连续 N 次 Fail → 暂停 Loop → 通知用户
```

### 统一输出结构

```rust
struct LoopReport {
    workspace_id: String,
    trigger_source: SignalSource,
    verdict: TickVerdict,
    steps: Vec<StepResult>,
    executed_actions: Vec<ExecutedAction>,
    proposals: Vec<Proposal>,
    duration_ms: u64,
    tool_call_count: u32,
    lie_detected: bool,
}
```

---

## 实现范围

### 新增类型（`crates/tinyiothub-ai/src/heartbeat/types.rs`）

```
PlanStep, FailureAction, StepResult, StepStatus,
LoopSignal(增强), SignalSource, SignalPayload,
ToolMeta(增强), ToolCallRecord, 
PreToolResult, Decision, StepVerdict, TickVerdict, LoopReport
```

### 新增模块（`crates/tinyiothub-ai/src/heartbeat/`）

```
harness.rs      — 统一 Loop 入口，6 阶段调度
plan.rs         — Plan 阶段：Skill 匹配 + step 分解 + prompt 注入
execute.rs      — Execute 阶段：PreToolUse → Tool → PostToolUse
verify.rs       — Verify 阶段：cross_check + quality gate
```

### 修改文件

```
loop_.rs        — 重构为调用 harness，保留 sleep/wake 循环
report.rs       — 接入 step-level 验证
chat/service.rs — Chat 路径通过 harness 发送消息（而非直接调 zeroclaw）
```

### 不变文件

```
tool/trust.rs      — TrustEngine 保持不变，成为 PreToolUse 的检查项之一
policy/mod.rs      — PolicyEngine trait 保持不变，PreToolUse 调用它
orchestrator/      — Orchestrator 接口不变，消费 LoopReport
```

### 新增 AiEvent 变体

Harness 输出需要新事件类型让 Orchestrator 路由：

```
event/types.rs 新增:
  HarnessStepCompleted { workspace_id, step_id, verdict }
  HarnessTickCompleted { workspace_id, report: LoopReport }
  AgentDegraded { workspace_id, reason }
  ProposalCreated { workspace_id, proposal }     // 填补现有 no-op
  ProposalResolved { workspace_id, proposal_id } // 填补现有 no-op
```

---

## 不做的

- 不引入 DSL 或形式化执行引擎 — Skill 文件保持 Markdown，Plan 阶段负责解析和约束注入
- 不改变 AgentPool 接口 — Harness 在 AgentPool 之上，不在其内部
- 不重写现有工具 — 工具的签名和执行逻辑不变，控制层外挂
- 不引入 feature flag — Harness 直接上线，不保留旧路径

---

## CEO Review 修订 (2026-07-06)

审查模式: HOLD SCOPE

### 架构决策

1. **完全统一**: Chat/Heartbeat/Workspace 全部走 6 阶段 Loop，无快速通道
2. **Plan 智能短路**: 简单查询跳过 Skill 匹配，直接生成单步 Plan
3. **Heartbeat 流式化**: 改用 zeroclaw streaming/turn API 以获得工具级拦截能力
4. **PostToolUse 分类处理**: 回读超时/设备离线 → VerificationUnavailable（不重试）；回读值不匹配 → WriteFailed（触发重试）
5. **工具级 Proposal 恢复**: 批准后只重新执行被批准的单个工具，不重做整个 Step
6. **完整指标集**: 阶段耗时分布、lie_detected 率、Propose 率、tick 成功率、degraded agent 数量

### zeroclaw PreToolUse 集成（已验证并修订）

**集成策略（2026-07-07 修订）**：

zeroclaw 的 `ApprovalManager` 是内部审批机制，设计用于 CLI 交互式提示和 Channel 消息平台（Telegram/Slack 等）。TrustEngine 不走 zeroclaw 审批管线——而是在 harness 的 `execute.rs` 中，**工具调用之前**运行。

```
LLM 想调 write_properties(device_id="d1", temp=25)
  ↓
Harness execute.rs 拦截
  ↓
PreToolUse: TrustEngine.evaluate("write_properties", args)
  ├─ Allow    → 正常调用工具
  ├─ Block    → 不调用工具，记录拒绝
  └─ Propose  → 创建 Proposal，不调用工具
  ↓
（仅 Allow）调用工具执行
  ↓
PostToolUse: 验证返回值 + write 类工具回读
```

**zeroclaw 配置**：
- Agent 使用 `AutonomyLevel::Full`（不禁用任何工具，所有控制权在 harness）
- 不使用 `ApprovalManager.always_ask` 机制
- `AgentPoolLike::send_message_streamed()` 返回 `Receiver<TurnEvent>` 用于**观察**工具调用的实际执行情况（步骤追踪、撒谎检测），而非用于拦截
- 不需要修改 zeroclaw 源码，不需要实现 `Channel` trait

### Outside Voice 发现的有效问题

1. **Skill 工作流提取** 是独立大功能 → Phase 1 用硬编码步骤列表，Phase 2 加 Skill 解析
2. **撒谎检测** 改为 per-tool 对比（而非 all-or-nothing），覆盖部分撒谎场景
3. **IoT 读回竞态**: write 后设备可能尚未处理 → 回读时增加短暂延迟或检查设备 ACK

### 新增指标要求

`LoopReport` 需额外包含: stage_durations (p50/p99 per stage)、tick_success_rate、agent_degraded_count

---

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 1 | CLEAR | 3 architectural decisions, 0 critical gaps |
| Eng Review | `/plan-eng-review` | Architecture & implementation | 1 | ISSUES | 4 arch findings, 2 code quality, 2 perf, 0 critical gaps |
| Outside Voice | auto | Independent 2nd opinion | 2 | ISSUES | 4 valid (integration, events, skills, lifecycle), 4 invalid (channel adapter, calibration) |

### Engineering Review — Architecture Findings

**AF1 — AgentPoolLike 缺少流式 API** (RESOLVED)
- 当前 trait 只有 `send_message() → String`，harness 需要 `send_message_streamed() → Receiver<TurnEvent>` 获取工具级可见性
- 决策：添加 `send_message_streamed()` 方法到 `AgentPoolLike` trait
- 注意：TurnEvent 用于**观察**（步骤追踪、撒谎检测），工具**拦截**在 harness execute.rs 中独立完成

**AF2 — Chat ApproveRequest 绕过 TrustEngine** (RESOLVED)
- `chat/service.rs:66-74` 将 `TurnEvent::ApprovalRequest` 直接转为 `ChatEvent::ToolCallStart`，未经 TrustEngine
- 决策：Chat 路径走 harness execute.rs，由 PreToolUse 阶段的 TrustEngine 统一处理

**AF3 — TrustConfig 硬编码 default()** (RESOLVED)
- `runner.rs:241-243` 的 `load_trust_config()` 忽略 workspace_id 返回 `TrustConfig::default()`
- 决策：从 DB 加载，`SELECT config_json FROM workspace_ai_config WHERE workspace_id = $1`

**AF4 — PolicyEngine 无实际实现** (RESOLVED)
- `policy/mod.rs` 只有 `NoopPolicyEngine`（永远 Allow）
- 决策：构建 DB-backed PolicyEngine，支持速率限制和内容过滤

### Engineering Review — Code Quality

**CQ1 — JSON fence regex 重复** (RESOLVED)
- `report.rs` 和 `reflect.rs` 各自有 JSON 提取逻辑
- 决策：harness 重构自然消除重复（report.rs 被替换为结构化 LoopReport）

**CQ2 — 无统一错误类型** (RESOLVED)
- 心跳模块用 `Result<T, String>` 无上下文
- 决策：定义 `HarnessError` enum，含 stage/workspace_id/reason 字段

### Engineering Review — Performance

**PF1 — 每次 tick 双层 LLM 调用** (RESOLVED)
- Skill 匹配 + Plan + 执行 = 2次 LLM 调用 per tick
- 决策：Phase 1 硬编码步骤列表（无额外 LLM 调用），Phase 2 加 Skill 解析

**PF2 — 上下文无限增长** (RESOLVED)
- `history: Vec<PreviousTickResult>` 无上限
- 决策：滑动窗口 cap，保留最近 N 个 tick（默认 20）

### Outside Voice — 有效发现

**OV1 — TurnEvent 是只读流** (FIXED)
- 原方案暗示 TurnEvent 可用于拦截工具调用。实际上 TurnEvent 只能观察
- 修订：zeroclaw 配置 `AutonomyLevel::Full`，所有拦截在 harness execute.rs 完成。spec 已更新集成策略章节

**OV2 — 缺少 Harness AiEvent 变体** (FIXED)
- spec 说「AiEvent 类型不变」但 harness 输出 StepVerdict/TickVerdict/LoopReport
- spec 已添加 5 个新 AiEvent 变体

**OV3 — 现有 Skills 系统被忽略** (NOTED)
- 代码库已有 `build_skills_prompt()` 和 `get_skill` tool
- Phase 2 改为：扩展现有 Skills 系统（而非新建解析器）。Phase 1 不受影响（硬编码步骤）

**OV4 — 心跳信号生命周期未处理** (NOTED)
- 现有 heartbeat loop 支持 External/Pause/Resume/ReloadTasks/ReloadConfig 信号
- 实现时在 harness.rs 保留这些信号处理

### Outside Voice — 无效发现

- **「Channel trait 是集成点」** — Channel trait 用于消息平台，非 TrustEngine 集成点。正确方案是 harness 层拦截
- **「send_message_streamed() 设计有根本缺陷」** — 该方法用于观察，非拦截。两者职责不同
- **「Workspace AI execution 不存在」** — spec 指的是 Workspace 场景的**未来** AI 执行路径，非当前代码
- **「这是错误抽象层级」** — 架构一致性是设计目标，不是 bug

**VERDICT:** ENG REVIEW COMPLETE — 6 个架构/代码/性能问题已解决。Outside Voice 发现 4 个有效 gap 已修正入 spec。

**UNRESOLVED DECISIONS:** 无
