# AI 与系统事件打通设计

> 状态: 设计完成 | 日期: 2026-06-15

## 概述

建立"系统事件 → AI Agent"的通用通道，使 AI 能够接收重要事件（报警、设备故障等）并在后台自主处理，通过 Tools 执行操作，所有决策记录审计日志。

## 需求

- 报警等关键事件触发后，AI Agent 在后台自主处理（不走聊天会话）
- 处理结果写审计日志，用户可事后查看
- AI 处理失败不影响系统正常运行
- 每个 Workspace 复用已有 Agent 配置（System Prompt、Tools、Memory）
- 具体 AI 能力通过 Tools 扩展，此版本先搭通道

## 架构

```
Device Event → EventBus
                 ├── AlarmEventHandler (p=50): 评估规则 → 创建 Alarm → 通知
                 │
                 └── AgentEventHandler (p=60): 查询新 Alarm/过滤重要事件 →
                                                 AgentPool → zeroclaw Agent →
                                                 LLM → Tools → agent_actions 表
```

核心思路：利用 EventBus 优先级顺序调度 —— `AgentEventHandler`(p=60) 在 `AlarmEventHandler`(p=50) 完成后执行。

## 事件重要性过滤

不是所有设备事件都发给 AI（量太大），只处理重要事件：

| 层级 | 触发条件 | 默认 |
|------|---------|------|
| 报警关联 | 事件触发了 Alarm（AlarmEventHandler 刚创建） | 开启 |
| 严重事件 | 事件级别 ≥ Error 且无关联 Alarm | 开启 |
| 普通事件 | PropertyChange 等周期性上报 | 跳过 |

```
Device Event → EventBus
                 ├── AlarmEventHandler (p=50)
                 │     ├── 规则触发 → 创建 Alarm
                 │     └── 未触发 → 跳过
                 │
                 └── AgentEventHandler (p=60)
                       ├── 有新 Alarm? → AI 处理 ✅
                       ├── 级别 ≥ Error? → AI 处理 ✅
                       └── 普通上报 → 跳过 ❌
```

后续可扩展 Workspace 级别的「事件订阅配置」，允许自定义哪些事件类型触发 AI。

## AI 处理管道

```
Alarm/重要事件 → AgentEventHandler (filter in handle)
                   ↓
              AutonomousAgentRunner
                   ↓
              查询设备信息 + 报警上下文 + 属性快照
                   ↓
              构建事件消息（不是替换 System Prompt，而是注入一条 user message）
                   ↓
              复用 Workspace Agent（System Prompt + Tools + Memory 与聊天一致）
                   ↓
              zeroclaw Agent.run() → LLM 推理 → Tool Calls
                   ↓
              agent_actions 表记录（每条 reasoning / tool_call / tool_result）
```

### System Prompt 复用

不单独构建 Prompt，直接复用 Workspace 已有 Agent。和用户聊天的区别仅在于输入不是用户消息，而是格式化的事件上下文：

```
[系统事件通知]

事件类型: PropertyChange
级别: Warning
设备: 温度传感器 A (dev-01)
属性: temperature = 95°C

触发的报警:
- [alarm-123] 高温报警 (Warning): 95°C > 阈值 80°C
  状态: active | 创建时间: 2026-06-15 14:30:00

设备当前状态:
温度: 95°C | 湿度: 45% | 在线: true

请分析当前情况并决定是否需要执行操作。
```

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
CREATE INDEX idx_agent_actions_created ON agent_actions(created_at);
```

`agent_actions` 是 AI 操作的完整审计日志。前端：
- 报警详情页展示关联的 AI 操作（按 `alarm_id` 查询）
- 独立的 AI 操作日志页（按时间线/设备/workspace 查询）

## 错误处理

| 场景 | 处理方式 |
|------|---------|
| AgentPool 中 Agent 未就绪 | 跳过，不处理 |
| LLM 调用超时（>60s） | 记录 error action |
| Tool 执行失败 | 记录失败 action，LLM 继续（可重试其他方案） |
| 同一 alarm 重复处理 | `find_recent_active` 窗口去重 |
| AgentEventHandler 崩溃 | 不影响 EventBus 其他 handler |
| tokio::spawn 内 panic | catch 并记录 error action |

## 文件变更

### 新增

| 文件 | 作用 |
|------|------|
| `cloud/src/modules/agent/event_handler.rs` | `AgentEventHandler` 实现 |
| `cloud/src/modules/agent/autonomous_runner.rs` | 构建上下文、调用 LLM、记录 action |
| `cloud/migrations/20260615120000_agent_actions.sql` | `agent_actions` 表 |

### 修改

| 文件 | 变更 |
|------|------|
| `cloud/src/shared/service_manager.rs` | 注册 `AgentEventHandler` 到 EventBus |
| `cloud/src/modules/agent/mod.rs` | 导出新模块 |
| `cloud/src/modules/alarm/repo.rs` | 新增 `find_recent_active` 查询方法 |

## AgentEventHandler 接口

```rust
pub struct AgentEventHandler {
    agent_pool: Arc<AgentPool>,
    alarm_repo: Arc<dyn AlarmRepository>,
    action_repo: Arc<dyn AgentActionRepository>,
    runner: Arc<AutonomousAgentRunner>,
}

#[async_trait]
impl EventHandler for AgentEventHandler {
    fn name(&self) -> &str { "AgentEventHandler" }

    fn should_handle(&self, event: &Event) -> bool {
        matches!(event.event_type(), EventType::Device(_))
    }

    fn priority(&self) -> u8 { 60 }

    async fn handle(&self, event: &Event) -> tinyiothub_core::error::Result<()> {
        // 1. 查询该设备最近创建的活跃报警
        let device_id = event.source().device_id()
            .unwrap_or_else(|| event.source().source_id());
        let alarms = self.alarm_repo.find_recent_active(device_id, ...).await?;

        // 2. 无报警且级别低 → 跳过
        if alarms.is_empty() && event.level() < EventLevel::Error {
            return Ok(());
        }

        // 3. fire-and-forget，不阻塞事件分发
        let runner = self.runner.clone();
        let event = event.clone();
        tokio::spawn(async move {
            if let Err(e) = runner.process(event, alarms).await {
                tracing::error!("AgentEventHandler processing failed: {}", e);
            }
        });

        Ok(())
    }
}
```

关键设计点：
- `should_handle` 只匹配设备事件，重要性过滤在 `handle` 内
- `handle` 不阻塞事件分发：所有 LLM 调用在 `tokio::spawn` 中执行
- p=60 确保在 AlarmEventHandler(p=50) 完成后执行

## AutonomousAgentRunner 接口

```rust
pub struct AutonomousAgentRunner {
    agent_pool: Arc<AgentPool>,
    action_repo: Arc<dyn AgentActionRepository>,
    cache: Arc<DeviceCache>,
}

impl AutonomousAgentRunner {
    pub async fn process(&self, event: Event, alarms: Vec<Alarm>) -> AgentResult<()> {
        // 1. 构建上下文（事件 + 报警 + 设备快照）
        // 2. 获取 Workspace Agent（复用 System Prompt + Tools + Memory）
        // 3. 注入事件消息，调用 agent.run()
        // 4. 记录 actions 到 agent_actions 表
    }
}
```

## 测试策略

- `AgentEventHandler` 单元测试：验证 `should_handle` + `handle` 过滤逻辑
- `AutonomousAgentRunner` 单元测试：验证上下文构建、action 记录
- 集成测试：完整流水线（Device Event → Alarm → AI 处理 → agent_actions 写入）
- Mock LLM：测试环境使用 mock LLM 响应，避免依赖外部服务
