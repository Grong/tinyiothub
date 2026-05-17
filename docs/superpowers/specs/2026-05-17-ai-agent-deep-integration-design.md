# AI Agent 深度集成设计

> 2026-05-17 | 设计阶段 | TinyIoTHub + zeroclaw v0.7.5

## 动机

zeroclaw v0.7.5 已提供完整的 AI Agent 能力：会话管理、上下文压缩、Memory 命名空间隔离、SystemPromptBuilder、Heartbeat 引擎、SecurityPolicy、ResponseCache。但当前 TinyIoTHub 自行维护了大量重复实现：

- **两套会话管理**：TinyIoTHub SqliteSessionRepository + zeroclaw Agent 内部历史
- **两套压缩逻辑**：TinyIoTHub `compact_messages()` + zeroclaw `ContextCompressor`（前者实际未被调用）
- **手动 prompt 拼接**：8 层 `build_full_system_prompt()` vs zeroclaw 的 `SystemPromptBuilder` 9 个内置 section
- **无 namespace 隔离**：所有 workspace 共享一个 Memory 实例

## 目标

删除重复代码，深度集成 zeroclaw 成熟功能，同时保留 TinyIoTHub 在 IoT 领域的差异化部分。

## 原则

1. **zeroclaw 已有的，TinyIoTHub 不重复**
2. **TinyIoTHub 保留 IoT 独有的**：MCP 工具、设备记忆、session 索引（user 维度）
3. **尊重 API 边界**：不 patch zeroclaw 内部模块（已 patch 的 security/heartbeat 改为 pub 属于合理暴露）

---

## 架构对比

### 当前

```
ChatService
  ├── SessionService (自有 CRUD)
  ├── AgentMemoryService (设备记忆)
  ├── compaction 逻辑 (实际未调用)
  ├── system prompt 手动拼接
  └── AgentRuntimeImpl (单例)
       ├── Memory (无 namespace，全 workspace 共享)
       ├── ResponseCache (已启用)
       ├── SecurityPolicy (已启用，审批未接通)
       ├── chat_history() → 读 chat_messages 表
       └── zeroclaw Agent
```

### 新架构

```
ChatService (精简)
  ├── SessionIndex (轻量索引)
  ├── AgentMemoryService (保留)
  ├── [删除] compaction 逻辑
  └── AgentRuntimeImpl
       ├── Agent 池 (DashMap<workspace_id, Agent>)
       │    └── NamespacedMemory(namespace = workspace_id)
       ├── ResponseCache (已启用)
       ├── SecurityPolicy → 审批流接通
       ├── SystemPromptBuilder + workspace 注入
       ├── chat_history() → 读 zeroclaw Agent.history()
       └── 共享底层 SQLite Memory (WAL 模式)
```

---

## 模块设计

### 1. AgentRuntimeImpl 重写

**文件**：`cloud/src/shared/agent/runtime.rs`

#### 1.1 Agent 池 + Namespace 隔离

```rust
pub struct AgentRuntimeImpl {
    db_pool: sqlx::SqlitePool,
    model_name: String,
    /// workspace_id → Agent (懒加载，LRU 淘汰)
    agents: Arc<DashMap<String, Arc<tokio::sync::Mutex<Agent>>>>,
    /// 共享底层 Memory
    shared_memory: Arc<dyn Memory>,
    /// 共享 Observer
    observer: Arc<dyn Observer>,
    /// ResponseCache
    response_cache: Option<Arc<ResponseCache>>,
}

fn get_or_create_agent(&self, workspace_id: &str) -> Arc<Mutex<Agent>> {
    self.agents.entry(ws.to_string()).or_insert_with(|| {
        let namespaced = Arc::new(NamespacedMemory::new(
            Arc::clone(&self.shared_memory),
            workspace_id.to_string(),
        ));
        // builder 用 namespaced 创建 Agent
        // security_summary + autonomy_level + response_cache 同当前
        let agent = Agent::builder()
            .memory(namespaced)
            .observer(self.observer.clone())
            .security_summary(...)
            .autonomy_level(AutonomyLevel::Supervised)
            .response_cache(self.response_cache.clone())
            .build().unwrap();
        Arc::new(tokio::sync::Mutex::new(agent))
    }).clone()
}
```

**关键决策**：
- 共享底层 SQLite Memory（WAL 模式，并发读无瓶颈）
- `NamespacedMemory` 装饰器保证 namespace 隔离（跨 namespace 访问静默返回空）
- Agent 懒创建，闲置超过 30 分钟的实例可 LRU 淘汰
- IoT 场景 workspace 数 << 用户数，数百 workspace 级别 SQLite 完全够用；未来超 1000 可切换到 Postgres backend

#### 1.2 接通审批流

```rust
TurnEvent::ApprovalRequest { tool_name, tool_args, risk_level, .. } => {
    serde_json::json!({
        "runId": forward_run,
        "sessionKey": forward_session,
        "state": "approval_required",
        "toolName": tool_name,
        "toolArgs": serde_json::to_string(&tool_args).unwrap_or_default(),
        "riskLevel": risk_level,
    })
}
```

审批事件通过 SSE 推给前端。前端审批 UI 不在本次设计范围内，本次只保证事件正确推送。

#### 1.3 chat_history 切换

```rust
async fn chat_history(&self, agent_id: &str, session_key: &str, limit: u32) -> Result<Value> {
    let parsed = ParsedSessionKey::parse_str(session_key)?;
    let agent = self.get_or_create_agent(&parsed.workspace_id).await;
    let ag = agent.lock().await;
    let messages = ag.history();
    Ok(serde_json::json!({ "messages": &messages[..messages.len().min(limit)] }))
}
```

`chat_messages` 表不再写入，迁移文件保留不动。

#### 1.4 SystemPromptBuilder 集成

用 zeroclaw `SystemPromptBuilder::with_defaults()` 替代手动 8 层拼接：

| 默认 Section | 处理 |
|-------------|------|
| DateTimeSection | 直接用 |
| IdentitySection | 替换为 IDENTITY.md |
| ToolHonestySection | 直接用 |
| ToolsSection | 替换为 IoT 工具目录 |
| SafetySection | 直接用（SecurityPolicy 注入） |
| SkillsSection | 替换为 skills/*.md |
| WorkspaceSection | 直接用 |
| RuntimeSection | 直接用 |
| ChannelMediaSection | 移除 |

Workspace markdown 文件（MEMORY.md、USER.md）和用户 Persona 以自定义 section 注入。

### 2. ChatService 精简

**文件**：`cloud/src/modules/agent/chat_service.rs`

**删除**：
- `check_compaction_needed()` — zeroclaw ContextCompressor 接管
- `process_sse_response()` 中写 `chat_messages` 的逻辑
- `config.max_messages_before_compact`、`config.enable_compaction`

**简化**：`chat()` 方法去掉 compaction 检查分支，直接调用 AgentRuntime。

### 3. SessionService → SessionIndex

**文件**：`cloud/src/modules/agent/service.rs`

**删除的函数**：
- `should_compact()` / `compact_messages()` / `rebuild_messages()` / `generate_default_summary()` / `estimate_tokens()`
- `CompactedSession` 类型
- `SessionService.compact_session()` / `check_compaction_needed()` / `get_compacted()`

**新增**：轻量 `session_index` 表

```sql
CREATE TABLE session_index (
    session_key  TEXT PRIMARY KEY,
    user_id      TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    agent_id     TEXT NOT NULL,
    label        TEXT,
    created_at   TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at   TEXT NOT NULL DEFAULT (datetime('now'))
);
```

**保留的 API**：`GET /sessions`、`PATCH /sessions/:key/label`、`DELETE /sessions/:key`

### 4. Module 清理

**文件**：`cloud/src/shared/agent/mod.rs`

删除 `build_full_system_prompt()` 中的手动拼接逻辑，替换为 `SystemPromptBuilder` 调用。

保留 `build_tools_catalog_json()` 和 skills 加载逻辑（给 SystemPromptBuilder 的自定义 section 使用）。

### 5. HeartbeatService

保持现状。唯一调整：心跳任务执行时通过 `get_or_create_agent()` 获取 namespace 隔离的 Agent。

---

## 变更文件清单

| 文件 | 操作 | 说明 |
|------|------|------|
| `shared/agent/runtime.rs` | 重写 | Agent 池、namespace 隔离、审批流、SystemPromptBuilder、chat_history 切换 |
| `shared/agent/mod.rs` | 精简 | 删除手动 prompt 拼接，保留 tools catalog 和 skills 加载 |
| `modules/agent/chat_service.rs` | 精简 | 删除 compaction 检查、chat_messages 写入、无用字段 |
| `modules/agent/service.rs` | 精简 | 删除 compaction 函数和 CompactedSession 类型 |
| `modules/agent/types.rs` | 精简 | 删除 CompactedSession，新增 SessionIndex 相关类型 |
| `shared/persistence/repositories/` | 新增 | SessionIndex repository + 迁移 SQL |
| `shared/agent/heartbeat_service.rs` | 微调 | namespace Agent 获取 |
| `shared/app_state.rs` | 调整 | AgentRuntimeImpl 构造参数变更 |
| `shared/service_manager.rs` | 无变更 | HeartbeatConfig 已配置 |

---

## 不包含在本次设计内

- 前端审批 UI 组件
- Agent LRU 淘汰策略实现（先做懒加载 + 手动清理）
- `chat_messages` 表数据迁移（旧数据保留不删）
- Postgres backend 切换
- zeroclaw Gateway 集成

## 验证方式

1. `cargo build` + `cargo test` 通过
2. 两个不同 workspace 各发一条消息 → 各自只看到自己的历史
3. 危险工具调用 → SSE 收到 `approval_required` 事件
4. 心跳巡检 → 正常运行
5. 长时间对话 → zeroclaw ContextCompressor 自动触发（日志可见）
