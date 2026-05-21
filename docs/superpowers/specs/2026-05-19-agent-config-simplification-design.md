# Agent 配置简化设计

> 2026-05-19 | 设计阶段 | 前端 + 后端

## 动机

当前 Agent 配置页面暴露了 5 个 Tab（配置、工具权限、技能、心跳、工作区文件），其中「配置」Tab 包含 4 张人设卡片和 System Prompt 文本框，「工作区文件」Tab 展示了 8 个 Markdown 文件的编辑器。这对非技术用户来说过于复杂——他们不想了解 IDENTITY.md、SOUL.md 这些概念，只需要告诉 AI"我管理的是什么园区/工作区"。

## 目标

将 Agent 配置简化为一个自然语言文本框，用户只需描述工作区背景。角色推断、文件管理全部内部化，用户不可见。

## 原则

1. **用户只描述「是什么」**，不关心「怎么做」
2. **工作区文件是基础设施**，不是用户界面
3. **系统自动推断和管理**角色、工具、记忆
4. **不新建独立 System Prompt**，所有内容落工作区文件

---

## 用户视角

### 简化前后对比

| 当前 5 Tab | 简化后 3 Tab | 说明 |
|------------|-------------|------|
| 配置 (overview) | **工作区设定** | 单一文本框 |
| 工具权限 (tools) | 工具权限 | 保持不变 |
| 技能 (skills) | 技能 | 保持不变 |
| 心跳 (heartbeat) | 心跳 | 保持不变 |
| 工作区文件 (files) | — | **删除** |

### 「工作区设定」Tab

```
┌─────────────────────────────────────────────┐
│  工作区设定                                  │
│  描述这个 AI 助手所管理的工作区背景            │
│                                             │
│  ┌─────────────────────────────────────────┐│
│  │ 这是深圳龙华智能园区，总面积约 12 万平方   ││
│  │ 米，包含 3 栋办公楼和 1 个数据中心。主要   ││
│  │ 管理暖通空调系统、智能照明、门禁系统和消防  ││
│  │ 设备，共计约 200 台 IoT 设备...           ││
│  └─────────────────────────────────────────┘│
│                                             │
│  [保存]                                      │
└─────────────────────────────────────────────┘
```

- 删除人设卡片（运维助手/监控助手/客服助手/自定义）
- 删除 System Prompt 文本框
- 保存时写入 `USER.md`

---

## 数据流设计

```
用户输入（自然语言）           系统自动管理（用户不可见）
───────────────────          ──────────────────────────
    USER.md                  IDENTITY.md  ← 对话总结后自动更新
        │                    SOUL.md      ← 统一内置，物联网智能管家
        │                    TOOLS.md     ← 工具权限变更时生成
        │                    MEMORY.md    ← 对话结束后追加关键信息
        ▼                    ──────────────────────────
  build_full_system_prompt()
  每次对话前加载全部文件
```

### 各文件职责

| 文件 | 谁管理 | 内容 |
|------|--------|------|
| `USER.md` | **用户手动编辑**（前端文本框） | 工作区自然语言描述 |
| `IDENTITY.md` | **对话自动更新** | Agent 当前身份（哪个园区、管什么设备） |
| `SOUL.md` | **系统内置** | 物联网智能管家行为准则（统一模板，不拆分角色） |
| `TOOLS.md` | **权限变更时生成** | 当前可用工具列表 |
| `MEMORY.md` | **对话自动追加** | 关键决策和事实，带时间戳 |

---

## 对话后处理管线（新增）

```
对话结束
    │
    ▼
┌─────────────────────────────┐
│ Agent 自我总结               │
│ 分析本次对话是否产生了新的     │
│ 工作区信息（新增设备、园区     │
│ 变更、用户偏好等）            │
└──────────────┬──────────────┘
               │
       ┌───────┼───────┐
       ▼       ▼       ▼
  USER.md  IDENTITY.md  MEMORY.md
  (不动)   (可能更新)   (追加摘要)
```

### 触发时机

- 每次对话结束后的 background 异步任务
- 不阻塞用户，用户无感知

### IDENTITY.md 更新逻辑

- Agent 分析对话内容，判断工作区核心信息是否变化
- 变化示例：新增设备类型、园区扩展、管理范围调整
- 合并更新（非覆盖），保留历史身份信息

### MEMORY.md 追加逻辑

- 提取关键决策和事实
- 追加时间戳条目，不覆盖已有内容
- 格式：`## YYYY-MM-DD\n- 关键决策/事实`
- 保留最近 N 条，旧条目自动清理

### SOUL.md

- 统一模板：「物联网智能管家」
- 涵盖：设备管理、告警监控、自动化运维、数据分析
- 核心原则：安全第一、读取优先于写入、批量操作前确认
- 用户不可见、不可编辑

### TOOLS.md

- 每次工具权限变更时重新生成
- 内容：当前启用的工具列表及简要说明
- 危险工具标记

---

## 前端变更

### 删除

- `agents-model-tab.ts` 中的人设卡片 UI (`PERSONA_PRESETS`)
- `agents-model-tab.ts` 中的 System Prompt textarea
- `agents-files-tab.ts` 整个文件
- `agents.ts` 中 `files` panel 和相关加载逻辑
- `agents.ts` 中 `panelLabels.files`
- 预设模板按钮点击后设置 `personaPreset` 和 `systemPrompt` 的逻辑

### 新增/修改

- 新建 `agents-workspace-tab.ts`：「工作区设定」Tab
  - 单一 textarea，placeholder：「描述这个 AI 助手所管理的工作区背景...」
  - 加载时从 `GET /agents/{id}/files/USER.md` 读取
  - 保存时 `PUT /agents/{id}/files/USER.md`
- `agents.ts`：将 `overview` panel 改为渲染新 Tab，删除 `files` panel

### AgentsState 清理

- 删除 `personaPreset`、`systemPrompt` 字段（不再暴露给前端，后端负责管理）

---

## 后端变更

### 新增

1. **对话后处理管线**
   - `PostConversationProcessor`：异步任务，总结对话并更新 IDENTITY.md / MEMORY.md
   - 触发点：`ChatService::send_message()` 中 `tokio::spawn` 完成后调用

2. **IDENTITY.md 自动更新**
   - `IdentityUpdater`：分析对话摘要，合并更新 workspace 的 IDENTITY.md

3. **MEMORY.md 追加**
   - `MemoryAppender`：提取关键信息，追加时间戳条目到 MEMORY.md

4. **TOOLS.md 生成**
   - `ToolsDocGenerator`：根据工具权限配置生成 TOOLS.md

### 修改

1. **SOUL.md 模板**：重写为统一的「物联网智能管家」定义
2. **`build_full_system_prompt()`**：移除 `persona_layer` 注入（SOUL.md 已在文件加载中覆盖）
3. **`AgentRuntimeConfig`**：标记 `persona_preset`、`system_prompt` 字段为 `#[deprecated]` 或删除

### SOUL.md 新模板

```markdown
# Agent Soul

你是 TinyIoTHub 的物联网智能管家，负责全面的 IoT 系统管理。

## 核心能力
- 设备管理：搜索、监控、配置、控制所有已连接设备
- 告警管理：实时跟踪告警、分析根因、协助处理
- 自动化运维：调度巡检任务、健康检查、批量操作
- 数据分析：设备数据趋势、性能优化建议
- 驱动管理：协议适配、连接测试

## 行为准则
- 安全第一：危险操作需确认，不可逆操作需特别提醒
- 读取优先：先了解现状，再执行变更
- 批量谨慎：批量操作前确认影响范围
- 简洁准确：回复简洁明了，关键信息不遗漏
- 主动发现：持续监控异常，主动提醒风险
```

---

## 实现任务

### T1: 重写 SOUL.md 模板
- 文件：`cloud/templates/agent/SOUL.md`
- 内容：统一的物联网智能管家定义

### T2: 新增对话后处理管线
- 文件：`cloud/src/modules/agent/post_conversation.rs`
- 实现 `PostConversationProcessor`，异步分析对话并更新 IDENTITY.md / MEMORY.md

### T3: 新增 TOOLS.md 自动生成
- 在 `cloud/src/modules/agent/tools/service.rs` 中添加 `generate_tools_doc()`
- 每次工具权限变更时写 TOOLS.md

### T4: 修改 build_full_system_prompt
- 删除 `persona_layer` 参数和注入逻辑
- SOUL.md 已通过 `load_workspace_prompt()` 加载

### T5: 清理 AgentRuntimeConfig
- 删除或标记 deprecated：`persona_preset`、`system_prompt` 字段
- 保持 DB schema 兼容

### T6: 前端「工作区设定」Tab
- 新建 `agents-workspace-tab.ts`
- 删除人设卡片和 System Prompt textarea
- 删除 `files` Tab

### T7: 删除前端工作区文件 Tab
- 删除 `agents-files-tab.ts`
- 清理 `agents.ts` 中的 files 相关逻辑

### T8: 验证
- `cargo build` + `cargo test` + `cargo clippy`
- 前端功能验证

---

## 不变的部分

- 技能 Tab（skills）— 保持不变
- 工具权限 Tab（tools）— 开关列表形式不变
- 心跳 Tab（heartbeat）— 保持不变
- `build_full_system_prompt()` 整体框架 — 只删除 persona_layer
- 工作区文件基础设施 — scaffold、读写 API 保留（后端内部使用）

---

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 1 | CLEAR | 4 scope proposals, 1 accepted, 2 deferred, 1 skipped. 4 issues found, 0 critical gaps |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | CLEAR | 3 issues found, 0 critical gaps. Scope reduced: T2+T3 deferred, 5 core tasks retained |

**ACCEPTED EXPANSIONS:**
- 工作区描述模板（填空式模板，降低写作门槛）

**DEFERRED TO TODOS.md:**
- 零配置 Agent：首次对话自动询问工作区背景
- 预览角色：保存后模拟对话确认
- 对话后处理管线（PostConversationProcessor / IdentityUpdater / MemoryAppender）
- TOOLS.md 自动生成
- 工作区描述模板（CEO 扩展项）

**SCOPE REDUCTION (Eng Review):**
- 砍掉 T2（对话后处理管线 + 3 个新服务 -> TODOS.md）
- 砍掉 T3（TOOLS.md 自动生成 -> TODOS.md）
- 保留核心 5 个任务：SOUL.md 重写 + persona_layer 清理 + 配置字段弃用 + 工作区 Tab + 文件 Tab 删除

**CODEX:** not run (outside voice skipped)

**CROSS-MODEL:** N/A

**UNRESOLVED:** 0

**VERDICT:** CEO + ENG CLEARED — ready to implement
