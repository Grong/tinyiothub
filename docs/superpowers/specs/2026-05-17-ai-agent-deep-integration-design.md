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

## 实现前置验证

在写任何实现代码前，先验证 zeroclaw v0.7.5 的以下 API 签名和语义：

```bash
# 确保以下 API 可访问且签名匹配
cargo doc -p zeroclaw --open --no-deps
```

| API | 检查点 | 验证方式 |
|-----|-------|---------|
| `NamespacedMemory::new(memory, namespace)` | 构造函数签名、是否为 `Arc<dyn Memory>` | 编译检查 |
| `Agent::builder().memory().build()` | builder 方法是否存在、返回值类型 | 编译检查 |
| `Agent::history()` | 返回类型是否为可 JSON 序列化的消息列表 | 集成测试 |
| `SystemPromptBuilder::with_defaults()` | 是否存在、section 列表 | 编译检查 |
| `TurnEvent::ApprovalRequest` | 变体字段名与设计文档一致 | 编译检查 |

**模块设计**

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

**并发约束**：`Arc<Mutex<Agent>>` 意味着同一 workspace 的并发对话被串行化处理。zeroclaw Agent 本质是单线程事件循环，这个锁是必要的。不同 workspace 之间完全并行。

**关键决策**：
- 共享底层 SQLite Memory（WAL 模式，并发读无瓶颈）
- `NamespacedMemory` 装饰器保证 namespace 隔离（跨 namespace 访问静默返回空）
- Agent 懒创建。为防止长时间运行内存无限增长，加入简单的定时清理：Agent 闲置超过 30 分钟后从 DashMap 中移除（下次访问时重新懒创建，历史由 zeroclaw SQLite Memory 持久化不受影响）
- IoT 场景 workspace 数 << 用户数，数百 workspace 级别 SQLite 完全够用；未来超 1000 可切换到 Postgres backend
- Agent 构建失败不使用 `unwrap()`，改为 `build().map_err(...)` 优雅降级

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

审批事件通过 SSE 推给前端。前端审批 UI 不在本次设计范围内。

**过渡期行为**：在审批 UI 就绪前，危险工具直接拒绝（返回「此操作需要审批，审批 UI 开发中」），而非推送 `approval_required` 事件后无限等待。待审批 UI 就绪后切换为完整的审批流。

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

`chat_messages` 表不再写入，迁移文件保留不动。`chat_history` API 直接读取 zeroclaw `Agent.history()`。若 Agent 刚创建无历史，返回空数组——前端需兼容此情况。

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

**简化**：`chat()` 方法去掉 compaction 检查分支，直接调用 AgentRuntime。`chat_send()` 中 workspace_id 提取统一使用 `ParsedSessionKey::parse_str()`，不再手动字符串分割。

**新架构 chat() 端到端流程**：

```
用户消息 → POST /api/v1/agent/:id/chat
  → ChatService.chat()
    → 解析 agent_id / session_key / message
    → 构建 system prompt（SystemPromptBuilder + workspace 文件）
    → AgentRuntimeImpl.chat_send()
      → get_or_create_agent(workspace_id)
      → agent.lock().await  // 同一 workspace 串行
      → zeroclaw Agent 事件循环
        → LLM 调用（MiniMax API）
        → 工具调用 → MCP handlers
        → 审批请求（危险工具）→ 过渡期直接拒绝
        → TurnEvent → JSON → SSE 字节
      → 返回 reqwest::Response（SSE stream）
  → 转发 SSE stream 到前端
    → 前端逐事件渲染（text / tool_call / approval_required / error）
```

错误传播路径：
- Agent 构建失败 → `AgentError::BuildError` → 500 + 错误消息
- LLM API 超时 → zeroclaw 内部重试 → 最终 `AgentError::Timeout`
- 工具调用失败 → `TurnEvent::ToolError` + JSON → SSE 推送前端
- 审批流拒绝 → `TurnEvent` + `state: "rejected"` → SSE 推送前端

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
- `chat_messages` 表数据迁移（旧数据保留不删）
- Postgres backend 切换
- zeroclaw Gateway 集成

## 可观测性

### 关键日志点

| 事件 | 级别 | 内容 |
|------|------|------|
| Agent 懒创建 | INFO | `workspace_id`、耗时 |
| Agent 构建失败 | ERROR | `workspace_id`、错误详情 |
| 审批请求（过渡期拒绝） | WARN | `tool_name`、`risk_level` |
| chat_history 调用 | DEBUG | `workspace_id`、返回消息数 |

### 关键 Metrics

- Agent 池大小（活跃 workspace 数）
- Agent 创建/销毁计数
- 审批触发次数（按 risk_level 分组）
- chat_history API 延迟（p50/p99）

## 验证方式

1. `cargo build` + `cargo test` 通过
2. 两个不同 workspace 各发一条消息 → 各自只看到自己的历史
3. 危险工具调用 → SSE 收到 `approval_required` 事件
4. 心跳巡检 → 正常运行
5. 长时间对话 → zeroclaw ContextCompressor 自动触发（日志可见）

## 测试要求

> `/plan-eng-review` | 目标：完整覆盖（15 个测试）

### runtime.rs 测试

| # | 测试 | 类型 | 覆盖路径 |
|---|------|------|---------|
| 1 | `test_get_or_create_agent_first_time` | 集成 | 首次创建 Agent（新 workspace）→ Agent 创建成功 |
| 2 | `test_get_or_create_agent_reuse` | 集成 | 复用已有 Agent → 返回同一实例 |
| 3 | `test_get_or_create_agent_build_error` | 单元 | Agent 构建失败 → 错误传播（非 panic） |
| 4 | `test_get_or_create_agent_after_cleanup` | 集成 | Agent 清理后重新创建 → 懒加载恢复 |
| 5 | `test_chat_history_with_messages` | 集成 | Agent 有历史 → 返回 JSON 数组 |
| 6 | `test_chat_history_empty` | 集成 | Agent 刚创建无历史 → 返回空数组 |
| 7 | `test_chat_history_invalid_session_key` | 单元 | session_key 解析失败 → 错误响应 |
| 8 | `test_chat_send_normal_flow` | 集成 | 正常对话 → SSE stream 产生 delta/final 事件 |
| 9 | `test_two_workspaces_isolation` | 集成 | 两个不同 workspace 各发消息 → 各自只看到自己的历史 |
| 10 | `test_dangerous_tool_rejected` | 集成 | 危险工具调用 → 直接拒绝（过渡期行为） |
| 11 | `test_llm_timeout_error` | 集成 | Agent 执行超时 → 返回 timeout 错误事件 |
| 12 | `test_chat_abort` | 集成 | 调用 chat_abort() → Agent 任务中止 |
| 13 | `test_concurrent_same_workspace_serialized` | 集成 | 同一 workspace 并发请求 → 串行处理 |
| 14 | `test_agent_idle_cleanup` | 集成 | Agent 闲置 30min → DashMap 移除 |
| 15 | `test_heartbeat_triggers_agent_creation` | 集成 | 心跳任务触发 → 懒创建 Agent |

### mod.rs 测试（保留 + 新增）

| # | 测试 | 类型 |
|---|------|------|
| 16 | `test_tools_catalog_structure` | 单元（保留） |
| 17 | `test_tools_catalog_dangerous_tools_are_disabled_by_default` | 单元（保留） |
| 18 | `test_system_prompt_builder_integration` | 集成 |

### SessionIndex 测试

| # | 测试 | 类型 |
|---|------|------|
| 19 | `test_session_index_crud` | 集成 |
| 20 | `test_session_index_list_by_user` | 集成 |

## 审查决策记录

> 2026-05-17 | `/plan-ceo-review` | HOLD SCOPE 模式

| 决策 | 结论 | 原因 |
|------|------|------|
| 实现方案 | Approach B（深度集成） | 删除所有重复代码，namespace 隔离保证多 workspace 安全 |
| 并发约束 | 补充到文档 | 同一 workspace 串行处理的约束需明确 |
| Agent 构建失败处理 | `build().map_err(...)` | 生产环境不可 `unwrap()` |
| 审批流过渡期 | 直接拒绝危险工具 | 前端审批 UI 未就绪，无限等待比直接拒绝更差 |
| chat_history 切换 | 直接切换 | 读 zeroclaw Agent.history()，前端兼容空数组 |
| 可观测性 | 补充到文档 | 关键日志点和 metrics 需在设计阶段明确 |
| LRU 淘汰 | 加入（定时清理） | 闲置 30 分钟后从 DashMap 移除，防止内存无限增长 |
| 双写过渡 | 不做 | chat_messages 表保留不删，数据可查 |
| Agent 定时清理 | 加入 | 闲置 30 分钟后从 DashMap 移除，防止内存无限增长 |
| zeroclaw API 烟雾测试 | 加入 | 实现前编译验证 NamespacedMemory、Agent::history()、SystemPromptBuilder 签名 |
| chat() 端到端流程 | 加入设计文档 | 补充用户消息→Agent→SSE→前端的完整路径和错误传播 |

### Eng Review 决策

> 2026-05-17 | `/plan-eng-review` | 完整覆盖

| 决策 | 结论 | 原因 |
|------|------|------|
| workspace_id 提取 | 统一使用 ParsedSessionKey::parse_str() | 消除脆弱的字符串分割 |
| Provider 重建 | 提取共享 builder 辅助方法 | 消除 `new()` 和 `refresh_tools_impl()` 的重复 |
| 测试覆盖 | 完整覆盖（20 个测试） | runtime.rs 当前零测试，必须补齐 |
| JSONL 任务 | 8 个实现任务 | 覆盖 Agent 池、审批流、SystemPromptBuilder、SessionIndex、清理、测试、API 验证 |

## Implementation Tasks

Synthesized from this review's findings.

- [ ] **T1 (P1, human: ~4h / CC: ~30min)** — runtime.rs — Rewrite AgentRuntimeImpl with Agent pool (DashMap) and NamespacedMemory per workspace
  - Surfaced by: Architecture review — Single Agent instance cannot provide per-workspace memory isolation
  - Files: cloud/src/shared/agent/runtime.rs, cloud/src/shared/app_state.rs
  - Verify: cargo test + two-workspace isolation test

- [ ] **T2 (P1, human: ~2h / CC: ~15min)** — runtime.rs — Implement approval flow: SSE push ApprovalRequest events, transition-period direct rejection for dangerous tools
  - Surfaced by: Security review — ApprovalRequest events return empty bytes
  - Files: cloud/src/shared/agent/runtime.rs
  - Verify: cargo test test_dangerous_tool_rejected

- [ ] **T3 (P1, human: ~3h / CC: ~20min)** — mod.rs — Replace manual 8-layer prompt building with zeroclaw SystemPromptBuilder, inject workspace files as custom sections
  - Surfaced by: Code quality review — build_full_system_prompt() is manual concatenation
  - Files: cloud/src/shared/agent/mod.rs
  - Verify: cargo test test_system_prompt_builder_integration

- [ ] **T4 (P1, human: ~1h / CC: ~10min)** — chat_service.rs — Simplify ChatService: remove compaction check, chat_messages writes, compaction config fields; unify workspace_id extraction via ParsedSessionKey
  - Surfaced by: Architecture review — session_key string splitting is fragile
  - Files: cloud/src/modules/agent/chat_service.rs
  - Verify: cargo test + manual SSE stream test

- [ ] **T5 (P1, human: ~2h / CC: ~15min)** — service.rs + types.rs — Replace SessionService with lightweight SessionIndex (6-column table + migration + repository)
  - Surfaced by: Architecture review — Full SessionService (541 lines) is overkill
  - Files: cloud/src/modules/agent/service.rs, cloud/src/modules/agent/types.rs, cloud/src/shared/persistence/repositories/
  - Verify: cargo test test_session_index_crud + test_session_index_list_by_user

- [ ] **T6 (P1, human: ~1h / CC: ~10min)** — runtime.rs — Add Agent idle cleanup: remove Agents idle >30min from DashMap
  - Surfaced by: Performance/Memory review — unbounded Agent accumulation is a production outage risk
  - Files: cloud/src/shared/agent/runtime.rs
  - Verify: cargo test test_agent_idle_cleanup

- [ ] **T7 (P1, human: ~6h / CC: ~45min)** — tests — Write 20 tests: Agent pool lifecycle, chat history, SSE streaming, isolation, cleanup, SessionIndex CRUD
  - Surfaced by: Test review — runtime.rs has zero test coverage (0/1072 lines)
  - Files: cloud/src/shared/agent/runtime.rs, cloud/src/shared/agent/mod.rs, cloud/src/shared/persistence/repositories/
  - Verify: cargo test — 20 agent tests pass

- [ ] **T8 (P1, human: ~30min / CC: ~5min)** — pre-impl — Verify zeroclaw v0.7.5 API signatures: NamespacedMemory, Agent::history(), SystemPromptBuilder, TurnEvent::ApprovalRequest
  - Surfaced by: Outside voice review — entire plan rests on unverified zeroclaw API assumptions
  - Files: cloud/src/shared/agent/runtime.rs
  - Verify: cargo doc -p zeroclaw + compile check

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 1 | CLEAR | 8 decisions, 0 critical gaps |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | CLEAR | 4 issues found in Architecture, 4 in Code Quality, 20 test gaps identified, 0 performance issues |
| Design Review | — | UI/UX gaps | 0 | — | N/A (backend-only) |
| Outside Voice | `/plan-ceo-review` + `/plan-eng-review` | Independent 2nd opinion | 1 | ISSUES_FOUND | 7 issues found, 3 incorporated |

- **UNRESOLVED:** 0
- **VERDICT:** CEO + ENG CLEARED — ready to implement
