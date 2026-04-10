# IoT Agent 增强：从 Claude Code 架构中汲取经验

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 增强 TinyIoTHub 内置 Agent 的工具协议、内存管理和 UI 渲染能力，借鉴 Claude Code 的设计

**Architecture:** 三层增强：
1. **工具协议层**：为 IoTToolAdapter 增加并发安全和读写属性
2. **内存管理层**：设备状态快照 + 对话历史压缩
3. **UI 协议层**：完整的 canvas 工具 A2UI 渲染

**Tech Stack:** Rust (axum, zeroclaw), TypeScript (Lit Web Components), SQLite

---

## 1. IoTToolAdapter 工具协议增强

### 1.1 定义 IoTToolMetadata trait

**Files:**
- Create: `api/src/api/mcp/tool_metadata.rs`

- [ ] **Step 1: 创建 tool_metadata.rs**

```rust
use serde_json::Value;

/// IoT 工具元数据 trait
/// 参考 Claude Code 的 Tool 接口中的并发安全和读写属性
pub trait IoTToolMetadata: Send + Sync {
    /// 工具名称
    fn name(&self) -> &str;

    /// 工具描述
    fn description(&self) -> &str;

    /// 输入 JSON Schema
    fn input_schema(&self) -> Value;

    /// 是否并发安全（可并行执行）
    /// 例如：list_devices 读操作可以并发，control_device 写操作不行
    fn is_concurrency_safe(&self, _input: &Value) -> bool { false }

    /// 是否只读操作
    /// 例如：list_devices 是只读，control_device 不是
    fn is_read_only(&self, _input: &Value) -> bool { false }

    /// 是否危险操作（删除、固件更新等）
    fn is_destructive(&self, _input: &Value) -> bool { false }

    /// 权限级别：ask（始终询问）、allow（自动允许）、deny（需确认）
    fn permission_level(&self, _input: &Value) -> PermissionLevel {
        PermissionLevel::Ask
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PermissionLevel {
    Allow,  // 可信操作，自动放行
    Ask,    // 需要用户确认
    Deny,   // 危险操作，需额外确认
}
```

- [ ] **Step 2: 在 tool_registry.rs 中为 ToolHandler 添加默认实现**

修改 `api/src/api/mcp/tool_registry.rs`，在 `ToolHandler` trait 中添加：

```rust
/// 为 ToolHandler 提供 IoTToolMetadata 的默认实现
impl<T: ToolHandler> IoTToolMetadata for T {
    fn is_concurrency_safe(&self, input: &Value) -> bool {
        let name = self.name();
        // 默认规则：list/get/read 类操作为并发安全
        name.starts_with("list_") || name.starts_with("get_") || name.ends_with("_read")
    }

    fn is_read_only(&self, input: &Value) -> bool {
        let name = self.name();
        name.starts_with("list_") || name.starts_with("get_") || name.ends_with("_query")
    }

    fn is_destructive(&self, input: &Value) -> bool {
        let name = self.name();
        name.starts_with("delete_") || name.contains("firmware") || name.contains("reset")
    }

    fn permission_level(&self, _input: &Value) -> PermissionLevel {
        let name = self.name();
        // 危险操作需要询问
        if name.starts_with("delete_") || name.contains("firmware") || name.contains("reset") {
            PermissionLevel::Ask
        // 读操作默认允许
        } else if self.is_read_only(_input) {
            PermissionLevel::Allow
        // 其他操作询问
        } else {
            PermissionLevel::Ask
        }
    }
}
```

- [ ] **Step 3: 在 zeroclaw_runtime.rs 中使用元数据**

修改 `api/src/infrastructure/zeroclaw_runtime.rs` 的 `IoTToolAdapter`：

```rust
// 在 impl IoTToolAdapter 后添加

impl IoTToolAdapter {
    pub fn name(&self) -> &str { &self.name }
    pub fn description(&self) -> &str { &self.description }
    pub fn input_schema(&self) -> serde_json::Value { self.input_schema.clone() }
    pub fn is_concurrency_safe(&self, input: &serde_json::Value) -> bool {
        self.handler.is_concurrency_safe(input)
    }
    pub fn is_read_only(&self, input: &serde_json::Value) -> bool {
        self.handler.is_read_only(input)
    }
}
```

- [ ] **Step 4: Commit**

```bash
git add api/src/api/mcp/tool_metadata.rs api/src/api/mcp/tool_registry.rs api/src/infrastructure/zeroclaw_runtime.rs
git commit -m "feat(agent): add IoTToolMetadata trait with concurrency and permission attributes"
```

### 1.2 工具并发调度验证

**Files:**
- Modify: `api/src/infrastructure/zeroclaw_runtime.rs` (turn_streamed 部分)

- [ ] **Step 1: 在日志中打印工具并发属性**

在 `TurnEvent::ToolCall` 处理处添加日志：

```rust
TurnEvent::ToolCall { name, args } => {
    let args_str = serde_json::to_string(&args).unwrap_or_default();
    let is_safe = /* 从 tool spec 中查询 */ false; // TODO: 下一步实现
    tracing::info!("Tool call: {} (concurrency_safe: {})", name, is_safe);
    // ... 其余代码
}
```

- [ ] **Step 2: Commit**

```bash
git add api/src/infrastructure/zeroclaw_runtime.rs
git commit -m "feat(agent): log tool concurrency safety on call"
```

---

## 2. 设备状态 Memory 系统

### 2.1 设备状态快照表

**Files:**
- Create: `api/migrations/20260410000001_add_device_memory.sql`

- [ ] **Step 1: 创建设备状态快照表**

```sql
-- 设备状态快照表
CREATE TABLE IF NOT EXISTS device_memory (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id TEXT NOT NULL,
    agent_id TEXT NOT NULL DEFAULT 'default',
    device_id TEXT NOT NULL,
    snapshot_data TEXT NOT NULL,  -- JSON 格式的设备状态快照
    snapshot_time INTEGER NOT NULL,  -- Unix timestamp
    created_at TEXT DEFAULT (datetime('now')),
    UNIQUE(workspace_id, agent_id, device_id)
);

-- 创建索引
CREATE INDEX IF NOT EXISTS idx_device_memory_lookup
ON device_memory(workspace_id, agent_id, device_id, snapshot_time DESC);

-- 保留最近 100 条快照
CREATE TRIGGER IF NOT EXISTS keep_device_memory_limit
AFTER INSERT ON device_memory
BEGIN
    DELETE FROM device_memory
    WHERE workspace_id = NEW.workspace_id
      AND agent_id = NEW.agent_id
      AND id NOT IN (
          SELECT id FROM device_memory
          WHERE workspace_id = NEW.workspace_id AND agent_id = NEW.agent_id
          ORDER BY snapshot_time DESC
          LIMIT 100
      );
END;
```

- [ ] **Step 2: 添加 Rust 模型**

**Files:**
- Create: `api/src/domain/agent/models/device_memory.rs`

```rust
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceMemory {
    pub id: Option<i64>,
    pub workspace_id: String,
    pub agent_id: String,
    pub device_id: String,
    pub snapshot_data: String,  -- JSON
    pub snapshot_time: i64,
    pub created_at: Option<String>,
}

impl DeviceMemory {
    pub fn new(workspace_id: String, agent_id: String, device_id: String, snapshot_data: serde_json::Value) -> Self {
        Self {
            id: None,
            workspace_id,
            agent_id,
            device_id,
            snapshot_data: serde_json::to_string(&snapshot_data).unwrap_or_default(),
            snapshot_time: Utc::now().timestamp_millis(),
            created_at: None,
        }
    }
}
```

- [ ] **Step 3: 添加 Repository**

**Files:**
- Create: `api/src/infrastructure/persistence/repositories/device_memory_repository_impl.rs`

```rust
use crate::domain::agent::models::DeviceMemory;

pub trait DeviceMemoryRepository: Send + Sync {
    async fn save(&self, memory: &DeviceMemory) -> Result<(), DbErr>;
    async fn get_latest(&self, workspace_id: &str, agent_id: &str, device_id: &str) -> Result<Option<DeviceMemory>, DbErr>;
    async fn get_all_for_agent(&self, workspace_id: &str, agent_id: &str) -> Result<Vec<DeviceMemory>, DbErr>;
    async fn delete_old(&self, workspace_id: &str, agent_id: &str, keep_count: i64) -> Result<u64, DbErr>;
}
```

- [ ] **Step 4: Commit**

```bash
git add api/migrations/20260410000001_add_device_memory.sql api/src/domain/agent/models/device_memory.rs
git add api/src/infrastructure/persistence/repositories/device_memory_repository_impl.rs
git commit -m "feat(agent): add device memory snapshot system"
```

### 2.2 Memory Service 实现

**Files:**
- Create: `api/src/domain/agent/services/memory_service.rs`

- [ ] **Step 1: 创建 MemoryService**

```rust
use crate::domain::agent::models::DeviceMemory;
use crate::infrastructure::persistence::repositories::device_memory_repository_impl::DeviceMemoryRepository;
use std::sync::Arc;

pub struct MemoryService {
    repo: Arc<dyn DeviceMemoryRepository>,
}

impl MemoryService {
    pub fn new(repo: Arc<dyn DeviceMemoryRepository>) -> Self {
        Self { repo }
    }

    /// 保存设备状态快照
    pub async fn save_device_snapshot(
        &self,
        workspace_id: &str,
        agent_id: &str,
        device_id: &str,
        snapshot_data: serde_json::Value,
    ) -> Result<(), String> {
        let memory = DeviceMemory::new(
            workspace_id.to_string(),
            agent_id.to_string(),
            device_id.to_string(),
            snapshot_data,
        );
        self.repo.save(&memory)
            .await
            .map_err(|e| e.to_string())
    }

    /// 获取设备的最新快照
    pub async fn get_latest_device(
        &self,
        workspace_id: &str,
        agent_id: &str,
        device_id: &str,
    ) -> Result<Option<serde_json::Value>, String> {
        let memory = self.repo
            .get_latest(workspace_id, agent_id, device_id)
            .await
            .map_err(|e| e.to_string())?;
        Ok(memory.and_then(|m| serde_json::from_str(&m.snapshot_data).ok()))
    }

    /// 构建 Memory Prompt 片段（用于注入 system prompt）
    pub async fn build_memory_prompt(
        &self,
        workspace_id: &str,
        agent_id: &str,
    ) -> Result<String, String> {
        let memories = self.repo
            .get_all_for_agent(workspace_id, agent_id)
            .await
            .map_err(|e| e.to_string())?;

        if memories.is_empty() {
            return Ok(String::new());
        }

        let mut prompt = String::from("\n\n## 设备状态记忆\n");
        for mem in memories {
            if let Ok(data) = serde_json::from_str::<serde_json::Value>(&mem.snapshot_data) {
                let time = chrono::DateTime::from_timestamp_millis(mem.snapshot_time)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_default();
                prompt.push_str(&format!("\n[{}] 设备 {}: {}\n",
                    time, mem.device_id, data));
            }
        }
        Ok(prompt)
    }

    /// 清理旧快照（保留最近 20 条）
    pub async fn prune_old_snapshots(
        &self,
        workspace_id: &str,
        agent_id: &str,
    ) -> Result<u64, String> {
        self.repo.delete_old(workspace_id, agent_id, 20)
            .await
            .map_err(|e| e.to_string())
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add api/src/domain/agent/services/memory_service.rs
git commit -m "feat(agent): add MemoryService for device state snapshots"
```

---

## 3. Chat Auto-Compact 压缩

### 3.1 压缩服务

**Files:**
- Create: `api/src/domain/agent/services/compact_service.rs`

- [ ] **Step 1: 创建 CompactService**

```rust
/// 对话历史压缩服务
/// 参考 Claude Code 的 Auto-Compact 机制

const MAX_MESSAGES_IN_MEMORY: usize = 50;
const COMPACT_THRESHOLD_TOKENS: usize = 8000;

pub struct CompactService;

impl CompactService {
    /// 检查是否需要压缩
    pub fn should_compact(messages: &[ChatMessage]) -> bool {
        if messages.len() <= MAX_MESSAGES_IN_MEMORY {
            return false;
        }
        // 简单估算：平均每条消息 200 tokens
        let estimated_tokens = messages.len() * 200;
        estimated_tokens > COMPACT_THRESHOLD_TOKENS
    }

    /// 压缩对话：保留系统消息 + 最近 N 条 + 摘要
    pub fn compact(messages: &[ChatMessage], summary: &str) -> Vec<ChatMessage> {
        // 保留前两条（通常是 system prompt）
        let system_messages: Vec<_> = messages.iter()
            .filter(|m| m.role == "system")
            .cloned()
            .collect();

        // 保留最近 20 条用户/助手对话
        let recent: Vec<_> = messages.iter()
            .filter(|m| m.role == "user" || m.role == "assistant")
            .rev()
            .take(20)
            .cloned()
            .collect();

        // 插入摘要消息
        let summary_msg = ChatMessage {
            role: "system".to_string(),
            content: vec![serde_json::json!({
                "type": "text",
                "text": format!("[对话历史摘要]\n{}", summary)
            })],
            timestamp: Some(chrono::Utc::now().timestamp_millis()),
            ..Default::default()
        };

        let mut result = system_messages;
        result.push(summary_msg);
        result.extend(recent.into_iter().rev());
        result
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add api/src/domain/agent/services/compact_service.rs
git commit -m "feat(agent): add CompactService for chat history compression"
```

---

## 4. A2UI Canvas 工具完整实现

### 4.1 前端 canvas 工具渲染修复

**Files:**
- Modify: `web/src/ui/controllers/chat.ts` (handleChatEvent)
- Modify: `web/src/ui/chat/grouped-render.ts` (renderToolCallCard)

- [ ] **Step 1: 验证 tool_result 事件到达前端**

在 `chat.ts` 的 `handleChatEvent` 中添加调试日志：

```typescript
case "tool_result": {
  console.log("[chat] tool_result received:", payload.toolName, payload.result);
  // ... 现有逻辑
}
```

- [ ] **Step 2: 确认 tool card 正确显示结果**

检查 `grouped-render.ts` 中的 `renderToolCallCard` 函数，确认 result 显示逻辑正确：

```typescript
${result
  ? html`<div class="chat-tool-card__result">${unsafeHTML(toMarkdownHtml(resultDisplay))}</div>`
  : html`<div class="chat-tool-card__result chat-tool-card__result--loading">等待结果...</div>`}
```

- [ ] **Step 3: Commit**

```bash
git add web/src/ui/controllers/chat.ts
git commit -m "debug(agent): add tool_result logging to verify events"
```

### 4.2 A2UI 渲染验证

**Files:**
- Modify: `web/src/ui/chat/a2ui/a2ui-renderer.ts` (添加日志)

- [ ] **Step 1: 验证 A2UI JSONL 解析**

在 `handleA2uiMessage` 中添加日志：

```typescript
handleA2uiMessage(jsonl: string): void {
  console.log("[a2ui] received jsonl:", jsonl.substring(0, 200));
  // ... 其余代码
}
```

- [ ] **Step 2: 验证 surface 渲染**

在 `renderSurface` 中添加：

```typescript
renderSurface(surfaceId: string): TemplateResult | typeof nothing {
  const surface = this.surfaces.get(surfaceId);
  console.log("[a2ui] renderSurface:", surfaceId, surface ? `${surface.components.length} components` : "not found");
  // ... 其余代码
}
```

- [ ] **Step 3: Commit**

```bash
git add web/src/ui/chat/a2ui/a2ui-renderer.ts
git commit -m "debug(agent): add A2UI rendering logs for verification"
```

---

## 5. Skills 机制（IoT 技能模板）

### 5.1 Skills 数据模型

**Files:**
- Create: `api/migrations/20260410000002_add_agent_skills.sql`
- Create: `api/src/domain/agent/models/skill.rs`

- [ ] **Step 1: 创建 skills 表**

```sql
CREATE TABLE IF NOT EXISTS agent_skills (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id TEXT NOT NULL,
    agent_id TEXT NOT NULL DEFAULT 'default',
    skill_name TEXT NOT NULL,
    skill_content TEXT NOT NULL,  -- Markdown 格式
    skill_type TEXT NOT NULL DEFAULT 'file',  -- 'file' | 'bundled' | 'mcp'
    paths TEXT,  -- JSON array of glob patterns for conditional triggers
    is_hidden BOOLEAN DEFAULT FALSE,  -- 是否在 UI 隐藏
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now')),
    UNIQUE(workspace_id, agent_id, skill_name)
);
```

- [ ] **Step 2: Rust Skill 模型**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSkill {
    pub id: Option<i64>,
    pub workspace_id: String,
    pub agent_id: String,
    pub skill_name: String,
    pub skill_content: String,
    pub skill_type: String,
    pub paths: Option<Vec<String>>,  -- glob patterns
    pub is_hidden: bool,
}

impl AgentSkill {
    /// 从 Markdown 内容中提取 YAML frontmatter
    pub fn parse_frontmatter(content: &str) -> (serde_json::Value, &str) {
        // 简单的 YAML frontmatter 解析
        // ...
    }
}
```

- [ ] **Step 3: Commit**

```bash
git add api/migrations/20260410000002_add_agent_skills.sql api/src/domain/agent/models/skill.rs
git commit -m "feat(agent): add AgentSkill data model"
```

### 5.2 Skills Service

**Files:**
- Create: `api/src/domain/agent/services/skills_service.rs`

- [ ] **Step 1: 创建 SkillsService**

```rust
/// Skills 服务 - 管理和执行 Agent 技能
/// 类似 Claude Code 的 SKILL.md 机制

pub struct SkillsService;

impl SkillsService {
    /// 执行 skill 模板，替换变量
    pub fn execute_skill(skill: &AgentSkill, params: &serde_json::Value) -> String {
        let mut content = skill.skill_content.clone();

        // 替换 ${param_name} 变量
        if let Some(obj) = params.as_object() {
            for (key, value) in obj {
                let placeholder = format!("${{{}}}", key);
                let replacement = match value {
                    serde_json::Value::String(s) => s.clone(),
                    _ => value.to_string(),
                };
                content = content.replace(&placeholder, &replacement);
            }
        }

        content
    }

    /// 检查文件路径是否匹配 skill 的触发条件
    pub fn matches_path(skill: &AgentSkill, file_path: &str) -> bool {
        let paths = match &skill.paths {
            Some(p) => p,
            None => return false,
        };

        for pattern in paths {
            if glob_match(pattern, file_path) {
                return true;
            }
        }
        false
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add api/src/domain/agent/services/skills_service.rs
git commit -m "feat(agent): add SkillsService for IoT skill templates"
```

---

## 6. 端到端验证计划

### 6.1 验证步骤

1. **启动 API 服务**
```bash
cd /Users/chenguorong/code/my/tinyiothub
cargo run --manifest-path api/Cargo.toml
```

2. **启动前端**
```bash
cd web && npm run dev
```

3. **测试工具调用**
   - 在 Chat 页面发送"帮我列出所有设备"
   - 验证工具调用卡片显示"运行中..."然后显示结果
   - 检查控制台日志中 tool_call_start 和 tool_result 事件

4. **测试 A2UI 渲染**
   - 在 Agent 配置中添加 canvas 工具到 system prompt
   - 发送"显示设备表格"触发 A2UI 推送
   - 验证 UI 组件正确渲染

5. **测试 Memory**
   - 查询设备状态多次
   - 检查 device_memory 表中是否正确存储快照
   - 验证 agent 上下文包含历史设备状态

---

## 文件结构汇总

```
api/src/
├── domain/agent/
│   ├── models/
│   │   ├── device_memory.rs    [新建]
│   │   └── skill.rs            [新建]
│   └── services/
│       ├── memory_service.rs   [新建]
│       ├── compact_service.rs   [新建]
│       └── skills_service.rs   [新建]
├── infrastructure/
│   └── zeroclaw_runtime.rs      [修改]
└── api/mcp/
    ├── tool_metadata.rs         [新建]
    └── tool_registry.rs         [修改]

web/src/ui/
├── chat/
│   ├── a2ui/
│   │   └── a2ui-renderer.ts    [修改 - 添加日志]
│   └── grouped-render.ts        [修改]
└── controllers/
    └── chat.ts                  [修改 - 添加日志]
```

---

## 优先级排序

| 优先级 | 任务 | 价值 |
|--------|------|------|
| P0 | IoTToolAdapter 元数据增强 | 工具安全基础 |
| P0 | 端到端验证（修复死锁） | 确保功能可用 |
| P1 | Device Memory 系统 | 长期上下文 |
| P1 | Auto-Compact | 长会话性能 |
| P2 | Skills 机制 | 扩展性 |
