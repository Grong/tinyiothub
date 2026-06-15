# AI 与系统事件打通设计

> 状态: 设计完成（已通过 Engineering Review）| 日期: 2026-06-15

## 概述

建立"告警 → AI Agent"的通用通道，使 AI 能够接收告警事件并在后台自主处理，通过 Tools 执行操作，所有决策记录审计日志。

## 需求

- 告警触发后，AI Agent 在后台自主处理（不走聊天会话）
- 处理结果写审计日志，用户可事后查看
- AI 处理失败不影响系统正常运行
- 每个 Workspace 复用已有 Agent 配置（System Prompt、Tools、Memory）
- 具体 AI 能力通过 Tools 扩展，此版本先搭通道

## 架构

```
Device Event → EventBus → AlarmEventHandler (p=50)
                               ├── 规则触发 → AlarmService::create_alarm()
                               │                  └── AutonomousAgentRunner.process(alarm, event)
                               │                        ├── 去重检查 (5s per-workspace)
                               │                        ├── AgentPool.get_or_create("default", ws_id)
                               │                        ├── 构建上下文 → agent.run()
                               │                        └── agent_actions 记录
                               └── 未触发 → 跳过
```

核心思路：在 `AlarmService::create_alarm()` 成功后直接触发 AI 处理，不再通过 EventBus handler。告警自带 `workspace_id`，无需从 Event 查询。

### 为什么不用 EventBus Handler

EventBus handler 设计为无状态、同步过滤、单事件分发。AI 自治调用的特点（有状态合批、长时间 LLM 调用）不适合这个模式。直接 Hook AlarmService 更简单：

| 问题 | EventBus Handler | AlarmService Hook |
|------|-----------------|-------------------|
| workspace_id 来源 | 需从 Device/Event 查询 | Alarm 自带 |
| should_handle 过滤 | 同步方法无法查 DB | 不需要过滤（只处理告警） |
| 合批/去重 | 需共享状态 | 简单 DashMap 去重 |
| 调度保证 | 依赖优先级排序 | 创建告警后立即触发 |

### 去重策略

同一 Workspace 5 秒内只触发一次 AI 处理。用 `DashMap<String, Instant>` 记录每个 Workspace 上次处理时间，避免短时间大量告警重复调用 LLM。

## AI 处理管道

```
AlarmService::create_alarm() 成功
        ↓
AutonomousAgentRunner.process(alarm, event)
        ↓
去重检查: DashMap<workspace_id, last_processed_at>
        ↓ (5s 内已处理则跳过)
查询设备信息 + 告警上下文 + 属性快照
        ↓
构建事件消息（注入一条 user message）
        ↓
AgentPool.get_or_create("default", workspace_id)
        ↓
agent.run() → LLM 推理 → Tool Calls
        ↓
agent_actions 表记录（每条 reasoning / tool_call / tool_result）
```

### System Prompt 复用

不单独构建 Prompt，直接复用 Workspace 已有 Agent。和用户聊天的区别仅在于输入不是用户消息，而是格式化的事件上下文：

```
[系统事件通知]

触发告警:
- [alarm-123] 高温报警 (Warning): 95°C > 阈值 80°C
  设备: 温度传感器 A (dev-01)
  状态: active | 创建时间: 2026-06-15 14:30:00

设备当前状态:
温度: 95°C | 湿度: 45% | 在线: true

请分析当前情况并决定是否需要执行操作。
```

### AutonomyLevel

Agent 需要在自治通道中自主执行操作（包括破坏性操作如重启设备），因此 `AgentRuntimeConfig` 新增 `autonomy_level` 字段。默认保持 `Supervised`（安全），自治通道的 "default" Agent 配置为 `Autonomous`。

## 数据库：agent_actions

```sql
CREATE TABLE IF NOT EXISTS agent_actions (
    id              TEXT PRIMARY KEY,
    workspace_id    TEXT NOT NULL,
    agent_id        TEXT NOT NULL,
    alarm_id        TEXT,
    device_id       TEXT,
    event_type      TEXT NOT NULL,
    action_type     TEXT NOT NULL,   -- reasoning / tool_call / tool_result / summary / error
    content         TEXT NOT NULL,   -- JSON
    created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_agent_actions_workspace ON agent_actions(workspace_id);
CREATE INDEX idx_agent_actions_alarm ON agent_actions(alarm_id);
CREATE INDEX idx_agent_actions_agent ON agent_actions(agent_id);
CREATE INDEX idx_agent_actions_created ON agent_actions(created_at);
```

`agent_actions` 是 AI 操作的完整审计日志。v1 提供 API 查询，前端在后续版本实现：
- 告警详情页展示关联的 AI 操作（按 `alarm_id` 查询）
- 独立的 AI 操作日志页（按时间线/设备/workspace 查询）

## 错误处理

| 场景 | 处理方式 |
|------|---------|
| AgentPool 中 Agent 未就绪 | 跳过，记录 warning 日志 |
| LLM 调用超时（>60s） | 记录 error action |
| Tool 执行失败 | 记录失败 action，LLM 继续（可重试其他方案） |
| 同一 Workspace 重复触发（5s 内） | 去重跳过 |
| AutonomousAgentRunner 内部 panic | catch 并记录 error action |
| 告警没有 workspace_id | 跳过（不应发生，但防御性检查） |

## 文件变更

### 新增

| 文件 | 作用 |
|------|------|
| `cloud/src/modules/agent/autonomous_runner.rs` | 构建上下文、调用 LLM、记录 action |
| `cloud/migrations/20260615120000_agent_actions.sql` | `agent_actions` 表 |

### 修改

| 文件 | 变更 |
|------|------|
| `cloud/src/modules/alarm/service.rs` | `create_alarm()` 成功后触发 AutonomousAgentRunner |
| `cloud/src/modules/agent/mod.rs` | 导出 autonomous_runner 模块 |
| `cloud/src/modules/agent/agent.rs` | `build_agent()` 接受 `AutonomyLevel` 参数 |
| `cloud/src/shared/agent/config.rs` | `AgentRuntimeConfig` 增加 `autonomy_level` 字段 |

## AutonomousAgentRunner 接口

```rust
pub struct AutonomousAgentRunner {
    agent_pool: Arc<AgentPool>,
    action_repo: Arc<dyn AgentActionRepository>,
    cache: Arc<DeviceCache>,
    /// Per-workspace dedup: last processed timestamp
    dedup: DashMap<String, Instant>,
}

impl AutonomousAgentRunner {
    /// 在 AlarmService::create_alarm() 成功后调用。
    /// 5 秒内同一 Workspace 的后续调用会被跳过。
    pub async fn process(&self, alarm: Alarm, event: Event) -> AgentResult<()> {
        // 1. 去重检查 (5s per-workspace)
        // 2. 构建上下文（告警 + 设备快照）
        // 3. AgentPool.get_or_create("default", workspace_id)
        // 4. agent.run() → 记录 actions 到 agent_actions 表
    }
}
```

关键设计点：
- 在 `create_alarm()` 成功后同步调用，fire-and-forget via `tokio::spawn`
- Per-workspace 5 秒去重，避免短时间大量告警重复调用
- 复用 AgentPool 的 Agent（含 NamespacedMemory、Tools、Observer）
- AutonomyLevel 由 AgentRuntimeConfig 控制，自治通道设为 Autonomous

## AlarmService 集成

```rust
// alarm/service.rs — create_alarm() 方法末尾添加:
if let Some(runner) = &self.autonomous_runner {
    let runner = Arc::clone(runner);
    let alarm = alarm.clone();
    let event = event.clone();  // event 需传入 create_alarm
    tokio::spawn(async move {
        if let Err(e) = runner.process(alarm, event).await {
            tracing::error!("AutonomousAgentRunner failed: {}", e);
        }
    });
}
```

## 测试策略

- `AutonomousAgentRunner` 单元测试：验证去重逻辑、上下文构建、action 记录
- 集成测试：AlarmService 创建告警 → AutonomousAgentRunner 触发 → agent_actions 写入
- LLM 调用通过集成测试手动验证（不 mock LLM，跳过实际 LLM 调用测试）

## NOT in scope

| Item | Rationale |
|------|-----------|
| Error+ 级别无告警事件处理 | 简化为 AlarmService Hook，v2 扩展 |
| AI 操作结果通知用户（推送/邮件） | v1 只记录 agent_actions |
| 前端 AI 操作日志页 | v1 只提供 API 查询 |
| Event 模型增加 workspace_id | AlarmService Hook 不需要 |
| AgentEventHandler EventBus 注册 | 架构改为直接 Hook |
| 多 Agent 事件路由 | 始终使用 default Agent |

## What already exists

| Existing | How plan uses it |
|----------|-----------------|
| `AgentPool::get_or_create("default", workspace_id)` | AutonomousAgentRunner 直接复用 |
| `AlarmService::create_alarm()` | Hook 点 |
| `Alarm.workspace_id` | 直接使用 |
| `tool_service::resolve_tools_for_agent()` | 由 AgentPool 内部调用 |
| `NamespacedMemory` | 由 AgentPool 自动创建 |
| `DeviceCache` | 构建设备快照上下文 |
| 现有 alarm 测试基础设施 (sqlx::test) | 复用相同模式 |
