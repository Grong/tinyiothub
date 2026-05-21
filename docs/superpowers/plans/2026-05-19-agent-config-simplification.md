# Agent Config Simplification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Simplify Agent configuration from 5 tabs to 4 tabs — replace persona presets + system prompt textarea with a single workspace description textarea, delete files tab.

**Architecture:** Backend: remove `persona_layer` injection from `build_full_system_prompt()`, deprecate `persona_preset`/`system_prompt` fields. Frontend: create `agents-workspace-tab.ts` (single textarea → USER.md), delete `agents-files-tab.ts`, clean `systemPrompt` dead code from chat controllers.

**Tech Stack:** Rust (axum, serde, tokio), TypeScript (Lit 3, nanostore)

---

## File Structure

| Action | File | Role |
|--------|------|------|
| MODIFY | `cloud/templates/agent/SOUL.md` | Unified 物联网智能管家 template |
| MODIFY | `cloud/src/shared/agent/mod.rs` | Remove `persona_layer` + `user_persona` param |
| MODIFY | `cloud/src/modules/chat/handler/proxy.rs` | Remove agent config read for systemPrompt |
| MODIFY | `cloud/src/shared/agent/config.rs` | Deprecate `persona_preset`/`system_prompt`, update default JSON |
| CREATE | `web/src/ui/views/agents-workspace-tab.ts` | Workspace description textarea |
| MODIFY | `web/src/ui/views/agents.ts` | Use workspace tab, remove files tab |
| DELETE | `web/src/ui/views/agents-files-tab.ts` | No longer needed |
| MODIFY | `web/src/ui/views/chat.ts` | Remove systemPrompt fetch |
| MODIFY | `web/src/ui/controllers/chat.ts` | Remove systemPrompt from types/state |
| MODIFY | `web/src/ui/controllers/agents.ts` | Remove systemPrompt/personaPreset, index signature, loadWorkspaceFiles |

---

### Task 1: Rewrite SOUL.md Template

**Files:**
- Modify: `cloud/templates/agent/SOUL.md`

- [ ] **Step 1: Replace SOUL.md content**

Replace the current 3-line content with the unified 物联网智能管家 template:

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

- [ ] **Step 2: Verify with cargo build**

```bash
cargo build
```
Expected: compiles successfully (SOUL.md is embedded via `include_str!`).

- [ ] **Step 3: Commit**

```bash
git add cloud/templates/agent/SOUL.md
git commit -m "feat(agent): rewrite SOUL.md as unified 物联网智能管家 template"
```

---

### Task 2: Remove persona_layer from build_full_system_prompt

**Files:**
- Modify: `cloud/src/shared/agent/mod.rs:76-109`
- Modify: `cloud/src/modules/chat/handler/proxy.rs:26-62`

- [ ] **Step 1: Update build_full_system_prompt signature**

In `cloud/src/shared/agent/mod.rs`, change the function signature at line 76-81 from:

```rust
pub async fn build_full_system_prompt(
    system_prompts: &crate::shared::config::SystemPromptsConfig,
    user_persona: &str,
    workspace_id: Option<&str>,
    agent_id: Option<&str>,
) -> String {
```

To:

```rust
pub async fn build_full_system_prompt(
    system_prompts: &crate::shared::config::SystemPromptsConfig,
    workspace_id: Option<&str>,
    agent_id: Option<&str>,
) -> String {
```

- [ ] **Step 2: Remove persona_layer construction**

Delete lines 88-93:

```rust
    // Layer 7: Persona override (user can override the default persona from config)
    // Only add persona layer if user provided one explicitly (via systemPrompt field)
    let persona_layer = if !user_persona.trim().is_empty() {
        format!("\n\n## Agent Persona（用户配置）\n{}\n", user_persona)
    } else {
        String::new()
    };
```

- [ ] **Step 3: Update format! call**

Change line 105-106 from:

```rust
    let full_prompt =
        format!("{}{}{}{}", workspace_prompt, persona_layer, skills_layer, context_layer);
```

To:

```rust
    let full_prompt =
        format!("{}{}{}", workspace_prompt, skills_layer, context_layer);
```

- [ ] **Step 4: Update proxy.rs caller — remove config read**

In `cloud/src/modules/chat/handler/proxy.rs`, delete lines 27-34:

```rust
    // Backend reads agent config for system_prompt
    let agent_config = state
        .agent_pool
        .get_agent_config(&req.agent_id, &claims.workspace_id)
        .await
        .map(|v| v.get("config").cloned().unwrap_or_default())
        .unwrap_or_default();
    let user_persona =
        agent_config.get("systemPrompt").and_then(|v| v.as_str()).unwrap_or("");
```

- [ ] **Step 5: Update proxy.rs build_full_system_prompt call**

Change lines 56-62 from:

```rust
    let full_prompt = crate::shared::agent::build_full_system_prompt(
        system_prompts,
        user_persona,
        Some(&workspace_id),
        None,
    )
    .await;
```

To:

```rust
    let full_prompt = crate::shared::agent::build_full_system_prompt(
        system_prompts,
        Some(&workspace_id),
        None,
    )
    .await;
```

- [ ] **Step 6: Check for other callers**

```bash
grep -rn "build_full_system_prompt" cloud/src/
```
Expected: only the definition in `mod.rs` and the one call in `proxy.rs`. If there are other callers, update them too.

- [ ] **Step 7: Verify with cargo build + test**

```bash
cargo build 2>&1
cargo test 2>&1
```
Expected: compiles, all existing tests pass.

- [ ] **Step 8: Commit**

```bash
git add cloud/src/shared/agent/mod.rs cloud/src/modules/chat/handler/proxy.rs
git commit -m "feat(agent): remove persona_layer from build_full_system_prompt"
```

---

### Task 3: Deprecate persona_preset/system_prompt in AgentRuntimeConfig

**Files:**
- Modify: `cloud/src/shared/agent/config.rs:57-128`

- [ ] **Step 1: Add #[deprecated] attributes**

In `cloud/src/shared/agent/config.rs`, change lines 71-75 from:

```rust
    /// System prompt / agent persona instructions
    #[serde(default)]
    pub system_prompt: String,
    /// Preset persona id: "ops" | "monitor" | "support" | "custom"
    #[serde(default)]
    pub persona_preset: String,
```

To:

```rust
    /// System prompt — deprecated: use USER.md workspace file instead
    #[deprecated(note = "Use USER.md workspace file instead")]
    #[serde(default)]
    pub system_prompt: String,
    /// Preset persona id — deprecated: persona is now inferred from workspace context
    #[deprecated(note = "Persona is now inferred from workspace context")]
    #[serde(default)]
    pub persona_preset: String,
```

- [ ] **Step 2: Update default_agent_config() JSON**

Change the `default_agent_config()` function (lines 116-128) to remove system_prompt and persona_preset from the fallback JSON:

```rust
pub fn default_agent_config() -> serde_json::Value {
    serde_json::to_value(AgentRuntimeConfig::default()).unwrap_or_else(|_| {
        serde_json::json!({
            "model": "minimax-m2",
            "temperature": 0.7,
            "max_tokens": 4096,
            "top_p": 1.0,
            "tool_denylist": ["delete_device", "delete_schedule"]
        })
    })
}
```

- [ ] **Step 3: Allow deprecated fields (suppress warnings)**

The `Default` impl and struct construction use these fields. Add `#[allow(deprecated)]` on the `Default` impl block at line 101:

```rust
#[allow(deprecated)]
impl Default for AgentRuntimeConfig {
```

- [ ] **Step 4: Verify with cargo build + clippy**

```bash
cargo build 2>&1
cargo clippy 2>&1
```
Expected: compiles with deprecation warnings on usages (expected — the fields still exist for DB compatibility), no new errors.

- [ ] **Step 5: Commit**

```bash
git add cloud/src/shared/agent/config.rs
git commit -m "feat(agent): deprecate persona_preset and system_prompt fields"
```

---

### Task 4: Create Workspace Settings Tab + Update agents.ts

**Files:**
- Create: `web/src/ui/views/agents-workspace-tab.ts`
- Modify: `web/src/ui/views/agents.ts:1-189`

- [ ] **Step 1: Create agents-workspace-tab.ts**

```typescript
import { html } from "lit";
import { apiGet, apiPut } from "../../api/client.js";

interface WorkspaceFileContent {
  name: string;
  content: string;
}

async function loadWorkspaceDescription(agentId: string): Promise<string> {
  try {
    const res = await apiGet<WorkspaceFileContent>(`/agents/${agentId}/files/USER.md`);
    return res.result?.content || "";
  } catch {
    return "";
  }
}

async function saveWorkspaceDescription(
  agentId: string,
  content: string,
): Promise<void> {
  await apiPut(`/agents/${agentId}/files/USER.md`, { content });
}

export function renderWorkspaceTab(
  agentId: string,
  onUpdate: () => void,
): ReturnType<typeof html> {
  // Module-level mutable state for the textarea
  const state = {
    content: "",
    loading: true,
    saving: false,
    dirty: false,
    error: "",
  };

  // Start loading — caller must re-render after promise resolves
  loadWorkspaceDescription(agentId).then((content) => {
    state.content = content;
    state.loading = false;
    onUpdate();
  });

  if (state.loading) {
    return html`<div class="callout info">加载中...</div>`;
  }

  if (state.error) {
    return html`<div class="callout error">${state.error}</div>`;
  }

  return html`
    <section class="card">
      <div class="card-title">工作区设定</div>
      <div class="card-sub">描述这个 AI 助手所管理的工作区背景</div>

      <div class="field" style="margin-top: 16px;">
        <textarea
          class="textarea"
          rows="8"
          placeholder="描述这个 AI 助手所管理的工作区背景，例如：所管理的园区、建筑、设备类型和数量..."
          .value=${state.content}
          @input=${(e: InputEvent) => {
            state.content = (e.target as HTMLTextAreaElement).value;
            state.dirty = true;
          }}
        ></textarea>
      </div>

      <div style="display: flex; gap: 8px; margin-top: 16px;">
        <button
          type="button"
          class="btn btn--sm primary"
          ?disabled=${!state.dirty || state.saving}
          @click=${async () => {
            state.saving = true;
            state.error = "";
            onUpdate();
            try {
              await saveWorkspaceDescription(agentId, state.content);
              state.dirty = false;
            } catch (err) {
              state.error = String(err);
            } finally {
              state.saving = false;
              onUpdate();
            }
          }}
        >
          ${state.saving ? "保存中..." : "保存"}
        </button>
      </div>

      ${state.error
        ? html`<div class="callout warn" style="margin-top: 8px;">保存失败: ${state.error}</div>`
        : ""}
    </section>
  `;
}
```

- [ ] **Step 2: Update agents.ts imports**

In `web/src/ui/views/agents.ts`, change the imports (lines 1-11):

Remove these imports:
```typescript
import { renderModelTab } from "./agents-model-tab.js";
import { renderFilesTab } from "./agents-files-tab.js";
import { loadWorkspaceFiles } from "../controllers/agents.js";
```

Also remove `saveAgentConfig` from the line 5 import — it was only used by `onSaveConfig` (which will be removed):
```typescript
// Before:
import { createAgentsState, loadAgents, loadAgentConfig, saveAgentConfig, loadToolsCatalog, toggleTool, loadSkills, loadHeartbeatConfig, loadHeartbeatLogs, updateHeartbeatConfig, updateHeartbeatTasks } from "../controllers/agents.js";
// After:
import { createAgentsState, loadAgents, loadAgentConfig, loadToolsCatalog, toggleTool, loadSkills, loadHeartbeatConfig, loadHeartbeatLogs, updateHeartbeatConfig, updateHeartbeatTasks } from "../controllers/agents.js";
```

Add:
```typescript
import { renderWorkspaceTab } from "./agents-workspace-tab.js";
```

- [ ] **Step 3: Update panelLabels**

Change lines 13-19 from:

```typescript
const panelLabels: Record<AgentsPanel, string> = {
  overview: "配置",
  tools: "工具权限",
  skills: "技能",
  heartbeat: "心跳",
  files: "工作区文件",
};
```

To:

```typescript
const panelLabels: Record<AgentsPanel, string> = {
  overview: "工作区设定",
  tools: "工具权限",
  skills: "技能",
  heartbeat: "心跳",
};
```

- [ ] **Step 4: Remove loadWorkspaceFiles call and onSaveConfig method**

In the `onAgentSelected` method (lines 42-52), remove the `loadWorkspaceFiles(this.state, agentId)` line (line 50). The Promise.all should contain 5 calls instead of 6.

Delete the `onSaveConfig` method (lines 54-60) — it was only used by the now-removed model tab:

- [ ] **Step 5: Update allPanels array**

Change line 133 from:
```typescript
    const allPanels: AgentsPanel[] = ["overview", "tools", "skills", "heartbeat", "files"];
```
To:
```typescript
    const allPanels: AgentsPanel[] = ["overview", "tools", "skills", "heartbeat"];
```

- [ ] **Step 6: Update overview panel render**

Change lines 164 from:
```typescript
          ${this.state.activePanel === "overview" ? renderModelTab(this.state, this._patchState.bind(this), this.onSaveConfig.bind(this), () => { if (this.state.selectedAgentId) loadAgentConfig(this.state, this.state.selectedAgentId).then(() => this.requestUpdate()); }) : nothing}
```

To:
```typescript
          ${this.state.activePanel === "overview" && this.state.selectedAgentId ? renderWorkspaceTab(this.state.selectedAgentId, this.requestUpdate.bind(this)) : nothing}
```

- [ ] **Step 7: Remove files panel render**

Delete lines 180-184:
```typescript
          ${this.state.activePanel === "files" && this.state.selectedAgentId ? renderFilesTab(
            this.state,
            this.state.selectedAgentId,
            this.requestUpdate.bind(this)
          ) : nothing}
```

- [ ] **Step 8: Verify frontend compiles**

```bash
cd web && npx tsc --noEmit 2>&1
```
Expected: no TypeScript errors.

- [ ] **Step 9: Commit**

```bash
git add web/src/ui/views/agents-workspace-tab.ts web/src/ui/views/agents.ts
git commit -m "feat(agent): add workspace settings tab, remove files tab from agents view"
```

---

### Task 5: Delete agents-files-tab.ts + Clean chat.ts/controllers

**Files:**
- Delete: `web/src/ui/views/agents-files-tab.ts`
- Modify: `web/src/ui/views/chat.ts:64-72`
- Modify: `web/src/ui/controllers/chat.ts:38-80`
- Modify: `web/src/ui/controllers/agents.ts:7-28,258-294`

- [ ] **Step 1: Delete agents-files-tab.ts**

```bash
git rm web/src/ui/views/agents-files-tab.ts
```

- [ ] **Step 2: Clean chat.ts — remove systemPrompt fetch**

In `web/src/ui/views/chat.ts`, replace lines 64-72:

```typescript
    // Load agent config to get systemPrompt, then create chat state
    try {
      const res = await apiGet<{ config: { systemPrompt?: string } }>(`/agents/${this.agentId}/config`);
      const systemPrompt = res.result?.config?.systemPrompt;
      this.chatState = createChatState(sessionKey || "", this.agentId, systemPrompt);
    } catch {
      // ZeroClaw not connected or config unavailable — still allow chat
      this.chatState = createChatState(sessionKey || "", this.agentId);
    }
```

With:

```typescript
    this.chatState = createChatState(sessionKey || "", this.agentId);
```

- [ ] **Step 3: Clean chat.ts — remove unused apiGet import**

`apiGet` imported on line 10 is now unused. Remove it:

```typescript
// Remove this line:
import { apiGet } from "../../api/client.js";
```

- [ ] **Step 4: Clean controllers/chat.ts — remove systemPrompt from createChatState**

In `web/src/ui/controllers/chat.ts`, change the `createChatState` function signature at line 64:

```typescript
export function createChatState(sessionKey: string, agentId: string, systemPrompt?: string): ChatState {
```

To:

```typescript
export function createChatState(sessionKey: string, agentId: string): ChatState {
```

- [ ] **Step 5: Remove systemPrompt from ChatState type**

In `web/src/ui/controllers/chat.ts`, remove line 57:

```typescript
  systemPrompt?: string;
```

- [ ] **Step 6: Remove systemPrompt from state initialization**

In `web/src/ui/controllers/chat.ts`, remove line 78:

```typescript
    systemPrompt,
```

The return in `createChatState` (lines 65-80) should end at `lastError: null` with no trailing comma issue.

- [ ] **Step 7: Clean controllers/agents.ts — remove systemPrompt/personaPreset from AgentConfig**

In `web/src/ui/controllers/agents.ts`, change the `AgentConfig` type (lines 9-28) to remove lines 15-16 and 27:

```typescript
export type AgentConfig = {
  model?: string;
  alternativeModels?: string[];
  workspace?: string;
  skills?: string[];
  // ZeroClaw 层
  temperature?: number;
  maxTokens?: number;
  topP?: number;
  tools?: {
    profile?: string;
    allow?: string[];
    alsoAllow?: string[];
    deny?: string[];
  };
};
```

(Removed: `systemPrompt?`, `personaPreset?`, and `[key: string]: unknown`)

- [ ] **Step 8: Remove loadWorkspaceFiles function + types**

Delete lines 257-294 from `web/src/ui/controllers/agents.ts`:

```typescript
// Workspace Files API
export interface WorkspaceFile {
  name: string;
  content: string;
}

export interface WorkspaceFilesListResponse {
  files: { name: string }[];
}

export async function loadWorkspaceFiles(state: AgentsState, agentId: string): Promise<void> {
  // ... entire function body
}
```

- [ ] **Step 9: Update AgentsPanel type**

Change line 7 from:

```typescript
export type AgentsPanel = "overview" | "tools" | "skills" | "heartbeat" | "files";
```

To:

```typescript
export type AgentsPanel = "overview" | "tools" | "skills" | "heartbeat";
```

- [ ] **Step 10: Verify frontend compiles**

```bash
cd web && npx tsc --noEmit 2>&1
```
Expected: no TypeScript errors.

- [ ] **Step 11: Commit**

```bash
git add web/src/ui/views/agents-files-tab.ts web/src/ui/views/chat.ts web/src/ui/controllers/chat.ts web/src/ui/controllers/agents.ts
git commit -m "feat(agent): remove files tab, clean systemPrompt dead code from chat and controllers"
```

---

### Task 6: Backend Tests + Verification

**Files:**
- Modify: `cloud/src/shared/agent/mod.rs` (tests module)
- Modify: `cloud/src/shared/agent/config.rs` (tests module)

- [ ] **Step 1: Add test for build_full_system_prompt without user_persona**

In `cloud/src/shared/agent/mod.rs`, add a new test after the existing tests (before the closing `}`):

```rust
    #[tokio::test]
    async fn test_build_full_system_prompt_no_persona() {
        // Verify that persona_layer is no longer injected
        let system_prompts = crate::shared::config::SystemPromptsConfig {
            context: String::new(),
            workspace_dir: String::new(),
        };
        let result = build_full_system_prompt(&system_prompts, None, None).await;
        // Should NOT contain the old persona header
        assert!(!result.contains("## Agent Persona（用户配置）"));
    }
```

- [ ] **Step 2: Run the new test**

```bash
cargo test test_build_full_system_prompt_no_persona 2>&1
```
Expected: PASS.

- [ ] **Step 3: Full verification**

```bash
cargo build 2>&1
cargo test 2>&1
cargo clippy 2>&1
```
Expected: compiles, all tests pass, no new clippy warnings.

- [ ] **Step 4: Commit**

```bash
git add cloud/src/shared/agent/mod.rs cloud/src/shared/agent/config.rs
git commit -m "test(agent): add tests for persona_layer removal and config cleanup"
```

---

## Verification Checklist

- [ ] `cargo build` — compiles
- [ ] `cargo test` — all tests pass
- [ ] `cargo clippy` — no new warnings
- [ ] `cd web && npx tsc --noEmit` — no TypeScript errors
- [ ] SOUL.md embedded content is the new unified template
- [ ] `build_full_system_prompt()` no longer accepts `user_persona`
- [ ] `proxy.rs` no longer reads agent config for systemPrompt
- [ ] `AgentRuntimeConfig.system_prompt` and `persona_preset` are `#[deprecated]`
- [ ] `agents.ts` has 4 tabs: 工作区设定, 工具权限, 技能, 心跳
- [ ] Workspace tab loads/saves USER.md
- [ ] `agents-files-tab.ts` is deleted
- [ ] `chat.ts` no longer fetches agent config for systemPrompt
- [ ] `ChatState` no longer has `systemPrompt` field
- [ ] `AgentConfig` type no longer has `systemPrompt`/`personaPreset`/index signature

## Edge Cases

1. **USER.md is empty**: Workspace tab shows placeholder text, saves empty content normally
2. **USER.md has special characters**: UTF-8 encoding handled by API layer
3. **Rapid save clicks**: Button disabled while `saving === true`
4. **Agent switch**: Switching agents re-renders workspace tab (new `agentId` prop)
5. **Old agents with system_prompt in DB**: `#[deprecated]` fields still deserialize for DB compatibility; just not exposed in UI
6. **Old agents with persona_preset in DB**: Same — field kept but ignored at prompt-build time
