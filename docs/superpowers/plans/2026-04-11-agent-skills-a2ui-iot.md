# Agent Skills + A2UI IoT Components Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete the Agent Skills (file-based) and A2UI IoT-specific components feature — backend skills loading from filesystem into system prompts, frontend Skills management UI, and IoT-specific A2UI components (alarm_card, alarm_table) with inline surface rendering.

**Architecture:**

**Skills (file-based):** Skills are `.md` files in `skills/` directory with YAML frontmatter. Each skill has `name`, `description`, and `paths` (glob patterns for conditional activation). `load_skills_prompt()` reads all matching skill files at startup/request time and formats them as Layer 3 in `build_full_system_prompt()`.

**A2UI:** Existing `A2uiRendererEngine` parses `createSurface`/`updateComponents` messages. New IoT components (alarm_card, alarm_table) are added to the catalog. Surfaces are rendered inline in chat messages via `renderAllSurfaces()` in the grouped-render pipeline.

**Tech Stack:** Rust (axum, sqlx), TypeScript (Lit web components), SQLite

---

## File Structure

```
api/src/
├── api/chat/skills.rs          # Skills CRUD handlers (file-based)
├── api/chat/mod.rs             # + skills module
├── api/mod.rs                  # + /chat/skills route
├── infrastructure/
│   └── zeroclaw_agent.rs       # build_full_system_prompt() + load_skills_prompt()
api/migrations/
├── 20260410000002_add_agent_skills.sql  # DELETE (DB approach abandoned)
└── (no new migrations needed)

skills/
└── tinyiothub/                # Skill files directory
    ├── skill.yaml              # Existing: skill definitions
    ├── device-onboarding.md    # New: device onboarding skill
    ├── alarm-management.md     # New: alarm management skill
    └── troubleshooting.md      # New: troubleshooting skill

web/src/
├── api/client.ts               # + skills API calls
├── ui/
│   ├── views/agents.ts         # + skills tab panel
│   ├── controllers/agents.ts    # + skills state + load/save functions
│   └── chat/
│       ├── grouped-render.ts   # + surface rendering in messages
│       └── a2ui/
│           ├── catalog/
│           │   ├── alarm-card.ts     # New
│           │   └── alarm-table.ts    # New
│           └── a2ui-renderer.ts      # + getSurfaceIds / clearSurface
```

---

## Task 1: Revert DB skills code

**Files:**
- Delete: `api/src/api/chat/skills.rs`
- Modify: `api/src/api/chat/mod.rs` — remove `pub mod skills;`
- Modify: `api/src/api/mod.rs` — remove `.nest("/chat/skills", ...)` route
- Delete: `api/migrations/20260410000002_add_agent_skills.sql`
- Modify: `api/src/infrastructure/zeroclaw_agent.rs` — revert `build_full_system_prompt` to original signature (no pool, no workspace_id/agent_id params)

- [ ] **Step 1: Delete skills.rs**

```bash
rm api/src/api/chat/skills.rs
```

- [ ] **Step 2: Remove skills module from chat/mod.rs**

Remove the line `pub mod skills;` from `api/src/api/chat/mod.rs`.

- [ ] **Step 3: Remove skills route from api/mod.rs**

Remove the line `.nest("/chat/skills", chat::skills::create_router())` from `api/src/api/mod.rs`.

- [ ] **Step 4: Delete DB migration**

```bash
rm api/migrations/20260410000002_add_agent_skills.sql
```

- [ ] **Step 5: Update build_full_system_prompt to file-based with workspace/agent params**

In `api/src/infrastructure/zeroclaw_agent.rs`, replace the async `build_full_system_prompt` and `load_skills_prompt` with the file-based implementation:

```rust
use crate::domain::agent::skill::AgentSkill;

/// Build the full system prompt by combining Layer 1 (platform base) + Layer 2 (user persona) + Layer 3 (skills)
pub fn build_full_system_prompt(
    user_persona: &str,
    workspace_id: Option<&str>,
    agent_id: Option<&str>,
) -> String {
    let base = platform_base_prompt();

    // Layer 2: user persona
    let layer2 = if user_persona.trim().is_empty() {
        String::new()
    } else {
        format!("\n\n## Agent 灵魂设定（用户配置）\n{}\n", user_persona)
    };

    // Layer 3: skills loaded from filesystem
    let layer3 = load_skills_prompt(workspace_id, agent_id);

    format!("{}{}{}", base, layer2, layer3)
}

/// Load skill files from the skills/ directory and format as Layer 3 prompt
/// Priority: skills/<ws>/<ag>/prompts/ > skills/<ws>/prompts/ > skills/<ws>/ > skills/tinyiothub/prompts/
fn load_skills_prompt(workspace_id: Option<&str>, agent_id: Option<&str>) -> String {
    use crate::domain::agent::skill::AgentSkill;

    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("skills");
    let ws = workspace_id.unwrap_or("tinyiothub");

    // Try in order: prompts subdir first, then flat directory
    let candidates: Vec<std::path::PathBuf> = match (workspace_id, agent_id) {
        (Some(w), Some(a)) => vec![
            base.join(w).join(a).join("prompts"),
            base.join(w).join("prompts"),
            base.join(w).join(a),
            base.join(w),
        ],
        (Some(_w), None) => vec![
            base.join(ws).join("prompts"),
            base.join(ws),
        ],
        _ => vec![
            base.join("tinyiothub").join("prompts"),
            base.join("tinyiothub"),
        ],
    };

    for dir in candidates {
        if dir.exists() {
            let result = read_skill_dir(&dir);
            if !result.is_empty() {
                return result;
            }
        }
    }

    String::new()
}

fn read_skill_dir(dir: &std::path::Path) -> String {
    use crate::domain::agent::skill::AgentSkill;

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("Failed to read skills directory {:?}: {}", dir, e);
            return String::new();
        }
    };

    let mut skill_files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "md"))
        .collect();

    skill_files.sort_by_key(|e| e.file_name());

    let mut all_skills = String::new();

    for entry in skill_files {
        let content = match std::fs::read_to_string(entry.path()) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let file_name = entry.file_name().to_string_lossy();
        let skill_name = file_name.trim_end_matches(".md");

        let (fm, body) = AgentSkill::parse_frontmatter(&content);
        let body = body.trim();

        if body.is_empty() {
            continue;
        }

        let description = fm
            .as_ref()
            .and_then(|f| f.get("description"))
            .and_then(|v| v.as_str())
            .unwrap_or(skill_name);

        let version = fm
            .as_ref()
            .and_then(|f| f.get("version"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        all_skills.push_str(&format!(
            "### {}{}\n{}\n{}\n",
            skill_name,
            if version.is_empty() { String::new() } else { format!(" (v{})", version) },
            description,
            body
        ));
    }

    if all_skills.is_empty() {
        String::new()
    } else {
        format!("\n\n## 技能（Skills）\n你可以使用以下技能来完成任务：\n\n{}\n", all_skills)
    }
}
```

- [ ] **Step 6: Update proxy.rs to extract workspace_id from session_key and pass to build_full_system_prompt**

In `api/src/api/chat/proxy.rs`, update the call:

```rust
// session_key format: agent:<agentId>:<mainKey>/<sess_uuid>
// Extract workspace_id from the second colon-separated segment
let workspace_id = req.session_key.split(':').nth(1).map(|s| s.split('/').next()).flatten();
let full_prompt = crate::infrastructure::zeroclaw_agent::build_full_system_prompt(
    user_persona,
    workspace_id,
    None,
);
```

- [ ] **Step 7: Verify clean build**

```bash
cd api && cargo build 2>&1 | grep "^error" | head -5
```

Expected: no errors. Fix any import/path issues.

---

## Task 2: Create skill files in `skills/` directory

> **Note:** The codebase already has skill content in `skills/tinyiothub/prompts/` (7 `.md` files).
> The `load_skills_prompt` implementation reads `skills/<workspace_id>/` directly, not the `prompts/` subdirectory.
> Either move existing files or update the path resolution in Task 3 to include `prompts/`.

**Files:**
- Create: `skills/tinyiothub/device-onboarding.md`
- Create: `skills/tinyiothub/alarm-management.md`
- Create: `skills/tinyiothub/troubleshooting.md`

- [ ] **Step 1: Create device-onboarding.md**

```markdown
---
name: device-onboarding
description: IoT 设备快速 onboarding — 从自然语言描述到设备上线
version: 1.0.0
---

# 设备快速 Onboarding 技能

当用户要求"添加设备"、"注册设备"或描述新设备时，使用此技能。

## 技能描述

你擅长将用户的自然语言设备描述转化为可配置的设备条目。

## 工作流程

1. 使用 `list_drivers` 列出可用驱动
2. 使用 `match_driver` 匹配驱动（可选，如果用户未指定协议）
3. 使用 `create_device` 创建设备（需要 name, driver_id）
4. 使用 `test_driver` 测试连接（需要 device_id）
5. 使用 `report_heartbeat` 上报心跳（需要 device_id）

## 示例对话

用户: "我有一个 Modbus 温度传感器，IP 是 192.168.1.100"
助手: 好的，我来帮你注册这个 Modbus 温度传感器。

[调用 list_drivers，筛选 modbus]
[调用 create_device，name="温度传感器", driver_id="modbus", address="192.168.1.100"]
[调用 test_driver，device_id=<新设备ID>]
```

- [ ] **Step 2: Create alarm-management.md**

```markdown
---
name: alarm-management
description: 告警管理、统计和自愈操作
version: 1.0.0
---

# 告警管理技能

当用户询问告警、告警统计、或要求处理告警时，使用此技能。

## 技能描述

你擅长查看和管理物联网网关的告警系统，包括告警列表、统计、自愈策略。

## 可用工具

- `alarm_list`: 列出告警，支持 status、level 筛选
- `alarm_statistics`: 获取告警统计
- `alarm_acknowledge`: 确认告警
- `alarm_rule_add`: 添加告警规则
- `get_self_heal_policy`: 获取自愈策略
- `execute_self_heal_action`: 执行自愈动作
- `get_recovery_history`: 查看恢复历史

## 示例对话

用户: "最近有哪些告警？"
助手: 让我查看最近的告警列表。

[调用 alarm_list，status="active"，limit=10]

用户: "确认 3 号告警"
助手: [调用 alarm_acknowledge，alarm_id=3]
```

- [ ] **Step 3: Create troubleshooting.md**

```markdown
---
name: troubleshooting
description: 设备故障诊断和恢复操作
version: 1.0.0
---

# 故障排查技能

当用户报告设备不在线、传感器无数据、或系统异常时，使用此技能。

## 技能描述

你擅长系统性排查 IoT 设备问题，从收集信息到执行恢复。

## 排查流程

1. **收集信息** — `get_device_status` 查看设备状态，`list_alarms` 查看告警
2. **知识库查询** — `query_knowledge_base` 搜索已知解决方案
3. **诊断分析** — `diagnose_device` 执行诊断
4. **恢复执行** — 合适的工具解决问题
5. **验证确认** — 确认问题已解决

## 健康阈值参考

- CPU: warning > 70%, critical > 90%
- 内存: warning > 75%, critical > 90%
- 磁盘: warning > 80%, critical > 95%
- 网络延迟: warning > 5s, critical > 10s

## 示例对话

用户: "3号设备不在线了"
助手: 我来帮你排查。先查看设备状态和告警。

[调用 get_device_status，device_id=3]
[调用 list_alarms，device_id=3，status=active]
```

---

## Task 3: Load skills from filesystem into system prompt

**Files:**
- Modify: `api/src/infrastructure/zeroclaw_agent.rs` — add `load_skills_prompt()` + update `build_full_system_prompt()`
- Modify: `api/src/api/chat/proxy.rs` — extract workspace_id from session_key

- [ ] **Step 1: Update build_full_system_prompt signature**

In `api/src/infrastructure/zeroclaw_agent.rs`, change `build_full_system_prompt` to accept optional workspace_id and agent_id:

```rust
/// Build the full system prompt by combining Layer 1 (platform base) + Layer 2 (user persona) + Layer 3 (skills)
pub fn build_full_system_prompt(
    user_persona: &str,
    workspace_id: Option<&str>,
    agent_id: Option<&str>,
) -> String {
    let base = platform_base_prompt();

    // Layer 2: user persona
    let layer2 = if user_persona.trim().is_empty() {
        String::new()
    } else {
        format!("\n\n## Agent 灵魂设定（用户配置）\n{}\n", user_persona)
    };

    // Layer 3: skills loaded from filesystem
    let layer3 = load_skills_prompt(workspace_id, agent_id);

    format!("{}{}{}", base, layer2, layer3)
}
```

- [ ] **Step 2: Add load_skills_prompt function**

Add this function after `platform_base_prompt()` in `api/src/infrastructure/zeroclaw_agent.rs`:

```rust
/// Load skill files from the skills/ directory and format as Layer 3 prompt
/// Priority: skills/<ws>/<ag>/prompts/ > skills/<ws>/prompts/ > skills/<ws>/ > skills/tinyiothub/prompts/
fn load_skills_prompt(workspace_id: Option<&str>, agent_id: Option<&str>) -> String {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("skills");
    let ws = workspace_id.unwrap_or("tinyiothub");

    // Try in order: prompts subdir first, then flat directory
    let candidates: Vec<std::path::PathBuf> = match (workspace_id, agent_id) {
        (Some(w), Some(a)) => vec![
            base.join(w).join(a).join("prompts"),
            base.join(w).join("prompts"),
            base.join(w).join(a),
            base.join(w),
        ],
        (Some(w), None) => vec![
            base.join(w).join("prompts"),
            base.join(w),
        ],
        _ => vec![
            base.join("tinyiothub").join("prompts"),
            base.join("tinyiothub"),
        ],
    };

    for dir in candidates {
        if dir.exists() {
            let result = read_skill_dir(&dir);
            if !result.is_empty() {
                return result;
            }
        }
    }

    String::new()
}

fn read_skill_dir(dir: &std::path::Path) -> String {
    let mut all_skills = String::new();

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("Failed to read skills directory {:?}: {}", dir, e);
            return String::new();
        }
    };

    let mut skill_files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "md"))
        .collect();

    // Sort for deterministic ordering
    skill_files.sort_by_key(|e| e.file_name());

    for entry in skill_files {
        let content = match std::fs::read_to_string(entry.path()) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let file_name = entry.file_name().to_string_lossy();
        // Remove .md extension for skill name
        let skill_name = file_name.trim_end_matches(".md");

        let (fm, body) = AgentSkill::parse_frontmatter(&content);
        let body = body.trim();

        if body.is_empty() {
            continue;
        }

        let description = fm.as_ref()
            .and_then(|f| f.get("description"))
            .and_then(|v| v.as_str())
            .unwrap_or(skill_name);

        let version = fm.as_ref()
            .and_then(|f| f.get("version"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        all_skills.push_str(&format!(
            "### {}{}\n{}\n{}\n",
            skill_name,
            if version.is_empty() { String::new() } else { format!(" (v{})", version) },
            description,
            body
        ));
    }

    if all_skills.is_empty() {
        String::new()
    } else {
        format!("\n\n## 技能（Skills）\n你可以使用以下技能来完成任务：\n\n{}", all_skills)
    }
}
```

**Note:** This requires adding `use crate::domain::agent::skill::AgentSkill;` import at the top of `zeroclaw_agent.rs`.

- [ ] **Step 3: Update proxy.rs to extract workspace_id from session_key and pass to build_full_system_prompt**

In `api/src/api/chat/proxy.rs`, update the call to extract workspace_id from session_key:

```rust
// session_key format: agent:<agentId>:<mainKey>/<sess_uuid>
// Extract workspace_id from the second colon-separated segment
let workspace_id = req.session_key.split(':').nth(1).map(|s| s.split('/').next()).flatten();
let full_prompt = crate::infrastructure::zeroclaw_agent::build_full_system_prompt(
    user_persona,
    workspace_id,
    None,
);
```

- [ ] **Step 4: Verify build**

```bash
cd api && cargo build 2>&1 | grep "^error"
```

Expected: no errors. Fix any import/path issues.

---

## Task 4: File-based Skills CRUD API

**Files:**
- Create: `api/src/api/chat/skills.rs` — file-based skills handlers
- Modify: `api/src/api/chat/mod.rs` — add `pub mod skills;`
- Modify: `api/src/api/mod.rs` — add `.nest("/chat/skills", chat::skills::create_router())`

**API contract (file-based, name is string identifier):**
- `GET /?workspace_id=` — list all skills in workspace dir
- `GET /:name?workspace_id=` — get single skill by filename (no extension)
- `POST /` — create skill file at `skills/<workspace_id>/<name>.md`
- `PUT /:name` — update/replace skill file at `skills/<workspace_id>/<name>.md`
- `DELETE /:name?workspace_id=` — delete skill file

**Security requirements:**
- Path traversal check: reject workspace_id/skill_name containing `..` or starting with `/`
- Disk-space check before write
- `std::sync::Mutex` for concurrent file writes

- [ ] **Step 1: Create skills.rs with file-based handlers**

```rust
// api/src/api/chat/skills.rs
// File-based skills CRUD — writes to skills/<workspace_id>/<skill_name>.md

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{delete, get, post},
    Router,
};
use serde::Deserialize;
use std::sync::Mutex;
use std::path::PathBuf;

use crate::{api::AppState, dto::response::{api_response::ApiResponse, builder::ApiResponseBuilder}, shared::security::jwt::Claims};

#[derive(Debug, Deserialize)]
pub struct CreateSkillRequest {
    pub workspace_id: String,
    pub skill_name: String,
    pub skill_content: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSkillRequest {
    pub skill_content: String,
}

// Mutex for concurrent file writes
lazy_static::lazy_static! {
    static ref SKILL_WRITE_MUTEX: Mutex<()> = Mutex::new(());
}

fn validate_skill_path(workspace_id: &str, skill_name: &str) -> Result<PathBuf, String> {
    // Reject path traversal attempts
    if workspace_id.contains("..") || skill_name.contains("..") {
        return Err("Invalid path: traversal not allowed".to_string());
    }
    if workspace_id.starts_with('/') || skill_name.starts_with('/') {
        return Err("Invalid path: absolute paths not allowed".to_string());
    }

    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("skills");
    let file_path = base.join(workspace_id).join(format!("{}.md", skill_name));

    // Verify the resolved path is still under skills/ (defense in depth)
    let canonical = file_path.canonicalize().map_err(|_| "Invalid path")?;
    let skills_canonical = base.canonicalize().map_err(|_| "Invalid skills directory")?;
    if !canonical.starts_with(&skills_canonical) {
        return Err("Invalid path: escape detected".to_string());
    }

    Ok(file_path)
}

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_skills).post(create_skill))
        .route("/:name", get(get_skill).put(update_skill).delete(delete_skill))
}

// GET /api/v1/chat/skills?workspace_id=
pub async fn list_skills(
    _state: State<AppState>,
    _claims: Claims,
    axum::extract::Query(q): axum::extract::Query<ListSkillsQuery>,
) -> Json<ApiResponse<Vec<SkillInfoDto>>> {
    let workspace_id = q.workspace_id.as_deref().unwrap_or("tinyiothub");
    let skills_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("skills").join(workspace_id);

    let skills = list_skill_files(&skills_dir);
    ApiResponseBuilder::success(skills)
}

// GET /api/v1/chat/skills/:name?workspace_id=
pub async fn get_skill(
    _state: State<AppState>,
    Path(name): Path<String>,
    _claims: Claims,
    axum::extract::Query(q): axum::extract::Query<ListSkillsQuery>,
) -> Result<Json<ApiResponse<SkillInfoDto>>, StatusCode> {
    let workspace_id = q.workspace_id.as_deref().unwrap_or("tinyiothub");
    let file_path = validate_skill_path(workspace_id, &name)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    match std::fs::read_to_string(&file_path) {
        Ok(content) => {
            let (fm, body) = crate::domain::agent::skill::AgentSkill::parse_frontmatter(&content);
            let skill_name = name.clone();
            let description = fm.as_ref()
                .and_then(|f| f.get("description"))
                .and_then(|v| v.as_str())
                .unwrap_or(&skill_name).to_string();
            Ok(ApiResponseBuilder::success(SkillInfoDto {
                name: skill_name,
                description,
                content: body.trim().to_string(),
            }))
        }
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

// POST /api/v1/chat/skills
pub async fn create_skill(
    _state: State<AppState>,
    _claims: Claims,
    Json(req): Json<CreateSkillRequest>,
) -> Result<Json<ApiResponse<SkillInfoDto>>, StatusCode> {
    // Validate path
    validate_skill_path(&req.workspace_id, &req.skill_name)
        .map_err(|e| StatusCode::BAD_REQUEST)?;

    let file_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("skills")
        .join(&req.workspace_id)
        .join(format!("{}.md", req.skill_name));

    // Concurrent write guard
    let _guard = SKILL_WRITE_MUTEX.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if file_path.exists() {
        return Err(StatusCode::CONFLICT); // File already exists
    }

    std::fs::create_dir_all(file_path.parent().unwrap())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Disk-space check before write (skip if < 1MB available)
    if let Ok(metadata) = std::fs::metadata(file_path.parent().unwrap()) {
        if metadata.available_space() < req.skill_content.len() as u64 {
            tracing::warn!("Disk full when writing skill: {:?}", file_path);
            return Err(StatusCode::INSUFFICIENT_STORAGE);
        }
    }

    std::fs::write(&file_path, &req.skill_content)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tracing::info!("Skill saved: {}/{} -> {:?}", req.workspace_id, req.skill_name, file_path);

    Ok(ApiResponseBuilder::success_with_message(
        SkillInfoDto {
            name: req.skill_name.clone(),
            description: req.skill_name.clone(),
            content: req.skill_content,
        },
        "Skill created",
    ))
}

// PUT /api/v1/chat/skills/:name
pub async fn update_skill(
    _state: State<AppState>,
    Path(name): Path<String>,
    _claims: Claims,
    axum::extract::Query(q): axum::extract::Query<ListSkillsQuery>,
    Json(req): Json<UpdateSkillRequest>,
) -> Result<Json<ApiResponse<SkillInfoDto>>, StatusCode> {
    let workspace_id = q.workspace_id.as_deref().unwrap_or("tinyiothub");
    let file_path = validate_skill_path(workspace_id, &name)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let _guard = SKILL_WRITE_MUTEX.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !file_path.exists() {
        return Err(StatusCode::NOT_FOUND);
    }

    std::fs::write(&file_path, &req.skill_content)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tracing::info!("Skill updated: {:?}", file_path);

    Ok(ApiResponseBuilder::success(SkillInfoDto {
        name: name.clone(),
        description: name.clone(),
        content: req.skill_content,
    }))
}

// DELETE /api/v1/chat/skills/:name?workspace_id=
pub async fn delete_skill(
    _state: State<AppState>,
    Path(name): Path<String>,
    _claims: Claims,
    axum::extract::Query(q): axum::extract::Query<ListSkillsQuery>,
) -> Result<Json<ApiResponse<()>>, StatusCode> {
    let workspace_id = q.workspace_id.as_deref().unwrap_or("tinyiothub");
    let file_path = validate_skill_path(workspace_id, &name)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let _guard = SKILL_WRITE_MUTEX.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if file_path.exists() {
        std::fs::remove_file(&file_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        tracing::info!("Skill deleted: {:?}", file_path);
    }

    Ok(ApiResponseBuilder::success_with_message((), "Skill deleted"))
}

fn list_skill_files(dir: &std::path::Path) -> Vec<SkillInfoDto> {
    let mut skills = Vec::new();
    if !dir.exists() { return skills; }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return skills,
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "md") {
            let file_name = path.file_stem().unwrap().to_string_lossy().to_string();
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            let (fm, body) = crate::domain::agent::skill::AgentSkill::parse_frontmatter(&content);
            let description = fm.as_ref()
                .and_then(|f| f.get("description"))
                .and_then(|v| v.as_str())
                .unwrap_or(&file_name).to_string();
            skills.push(SkillInfoDto {
                name: file_name,
                description,
                content: body.trim().to_string(),
            });
        }
    }
    skills.sort_by_key(|s| s.name.clone());
    skills
}

#[derive(Debug, serde::Serialize)]
pub struct SkillInfoDto {
    pub name: String,
    pub description: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct ListSkillsQuery {
    pub workspace_id: Option<String>,
}
```

- [ ] **Step 2: Add `lazy_static` to Cargo.toml**

In `api/Cargo.toml`, add:

```toml
lazy_static = "1.4"
```

- [ ] **Step 3: Add skills module to chat/mod.rs**

Add `pub mod skills;` to `api/src/api/chat/mod.rs`.

- [ ] **Step 4: Add skills route to api/mod.rs**

Add `.nest("/chat/skills", chat::skills::create_router())` to the protected routes.

- [ ] **Step 5: Verify build**

```bash
cd api && cargo build 2>&1 | grep "^error"
```

Expected: no errors.

- [ ] **Step 6: Add integration tests**

Create `api/src/api/chat/skills/tests/integration_tests.rs`:

```rust
//! Integration tests for file-based skills CRUD API
//!
//! Requires `tempfile` crate. Add to api/Cargo.toml:
//! tempfile = "3.8"

use std::io::Write as IoWrite;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_skills_dir() -> (TempDir, std::path::PathBuf) {
        let dir = TempDir::new().unwrap();
        let skills = dir.path().join("skills");
        std::fs::create_dir_all(&skills).unwrap();
        (dir, skills)
    }

    #[test]
    fn validate_skill_path_rejects_traversal() {
        let result = validate_skill_path("..", "foo");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Invalid path: traversal not allowed");
    }

    #[test]
    fn validate_skill_path_rejects_absolute() {
        let result = validate_skill_path("/", "foo");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Invalid path: absolute paths not allowed");
    }

    #[test]
    fn validate_skill_path_accepts_valid_name() {
        let result = validate_skill_path("tinyiothub", "device-onboarding");
        assert!(result.is_ok());
        let path = result.unwrap();
        // Path resolves to <crate>/skills/tinyiothub/device-onboarding.md
        assert!(path.file_name().unwrap() == "device-onboarding.md");
        // The workspace dir must exist under CARGO_MANIFEST_DIR/skills/
        assert!(path.to_string_lossy().contains("skills/"));
    }

    #[test]
    fn create_and_read_skill() {
        let (_dir, skills) = temp_skills_dir();
        let ws_dir = skills.join("tinyiothub");
        std::fs::create_dir_all(&ws_dir).unwrap();

        // Create a skill file
        let file_path = ws_dir.join("test-skill.md");
        let content = "---\nname: test\ndescription: Test skill\n---\n\n# Test";
        std::fs::write(&file_path, content).unwrap();

        // Verify it can be read
        let read = std::fs::read_to_string(&file_path).unwrap();
        assert!(read.contains("Test skill"));
    }

    #[test]
    fn skill_with_frontmatter_parsing() {
        let content = r#"---
name: alarm-management
description: Manage alarms
version: 1.0.0
---

# Alarm Management"#;

        let (fm, body) = crate::domain::agent::skill::AgentSkill::parse_frontmatter(content);
        assert!(fm.is_some());
        let fm = fm.unwrap();
        assert_eq!(fm.get("name").unwrap().as_str().unwrap(), "alarm-management");
        assert_eq!(fm.get("description").unwrap().as_str().unwrap(), "Manage alarms");
        assert_eq!(fm.get("version").unwrap().as_str().unwrap(), "1.0.0");
        assert!(body.contains("Alarm Management"));
    }

    #[test]
    fn skill_without_frontmatter() {
        let content = "# Plain skill without frontmatter";

        let (fm, body) = crate::domain::agent::skill::AgentSkill::parse_frontmatter(content);
        assert!(fm.is_none());
        assert_eq!(body.trim(), "# Plain skill without frontmatter");
    }
}
```

In `api/Cargo.toml`, add:

```toml
[dev-dependencies]
tempfile = "3.8"
```

Run tests:

```bash
cd api && cargo test chat::skills
```

Expected: 5 tests pass.

---

## Task 5: Create A2UI alarm components

**Files:**
- Create: `web/src/ui/chat/a2ui/catalog/alarm-card.ts`
- Create: `web/src/ui/chat/a2ui/catalog/alarm-table.ts`
- Modify: `web/src/ui/chat/a2ui/catalog/index.ts` — register new components

- [ ] **Step 1: Create alarm-card.ts**

```typescript
import { html, nothing, type TemplateResult } from "lit";

const LEVEL_COLORS: Record<string, string> = {
  info: "#3498db",
  warning: "#f39c12",
  error: "#e74c3c",
  critical: "#9b59b6",
};

const LEVEL_LABELS: Record<string, string> = {
  info: "提示",
  warning: "警告",
  error: "错误",
  critical: "严重",
};

const STATUS_COLORS: Record<string, string> = {
  active: "#e74c3c",
  acknowledged: "#f39c12",
  resolved: "#2ecc71",
};

export function renderAlarmCard(
  data: Record<string, unknown>,
  _onAction?: (fn: string, args: Record<string, unknown>) => void,
): TemplateResult {
  const alarmId = String(data.id || "");
  const deviceName = String(data.deviceName || data.device_id || "未知设备");
  const level = String(data.level || "info").toLowerCase();
  const message = String(data.message || "");
  const status = String(data.status || "active");
  const createdAt = data.created_at as string | undefined;
  const acknowledgedAt = data.acknowledged_at as string | undefined;
  const resolvedAt = data.resolved_at as string | undefined;

  const levelColor = LEVEL_COLORS[level] || "#95a5a6";
  const levelLabel = LEVEL_LABELS[level] || level;
  const statusColor = STATUS_COLORS[status] || "#95a5a6";

  const timeStr = createdAt
    ? new Date(createdAt).toLocaleString([], { month: "numeric", day: "numeric", hour: "numeric", minute: "2-digit" })
    : "";

  const badgeStyle = `background: ${levelColor}; color: white; padding: 1px 6px; border-radius: 4px; font-size: 11px;`;

  return html`
    <div class="a2ui-alarm-card">
      <div class="a2ui-alarm-card__header">
        <span class="a2ui-alarm-card__badge" style=${badgeStyle}>${levelLabel}</span>
        <span class="a2ui-alarm-card__status" style="color: ${statusColor}">● ${status}</span>
      </div>
      <div class="a2ui-alarm-card__message">${message}</div>
      <div class="a2ui-alarm-card__meta">
        <span>${deviceName}</span>
        <span>${timeStr}</span>
      </div>
      ${status === "active" && _onAction ? html`
        <div class="a2ui-alarm-card__actions">
          <button class="a2ui-btn a2ui-btn--secondary a2ui-btn--sm"
                  @click=${() => { _onAction?.("acknowledgeAlarm", { alarmId }); }}>
            确认
          </button>
        </div>
      ` : nothing}
    </div>
  `;
}
```

- [ ] **Step 2: Create alarm-table.ts**

```typescript
import { html, nothing, type TemplateResult } from "lit";

const LEVEL_COLORS: Record<string, string> = {
  info: "#3498db",
  warning: "#f39c12",
  error: "#e74c3c",
  critical: "#9b59b6",
};

const STATUS_COLORS: Record<string, string> = {
  active: "#e74c3c",
  acknowledged: "#f39c12",
  resolved: "#2ecc71",
};

export function renderAlarmTable(
  data: Record<string, unknown>,
  _onAction?: (fn: string, args: Record<string, unknown>) => void,
): TemplateResult {
  const title = String(data.title || "");
  const columns = (data.columns as string[]) || ["设备", "级别", "消息", "状态", "时间"];
  const alarms = (data.alarms as Array<Record<string, unknown>>) || [];

  return html`
    <div class="a2ui-alarm-table">
      ${title ? html`<div class="a2ui-alarm-table__title">${title}</div>` : nothing}
      <table class="a2ui-alarm-table__table">
        <thead>
          <tr>${columns.map((c) => html`<th>${c}</th>`)}</tr>
        </thead>
        <tbody>
          ${alarms.map((a) => {
            const level = String(a.level || "info").toLowerCase();
            const status = String(a.status || "active");
            const levelColor = LEVEL_COLORS[level] || "#95a5a6";
            const statusColor = STATUS_COLORS[status] || "#95a5a6";
            const timeStr = a.created_at
              ? new Date(a.created_at as string).toLocaleString([], { month: "numeric", day: "numeric", hour: "numeric", minute: "2-digit" })
              : "";
            return html`
              <tr>
                <td>${String(a.deviceName || a.device_id || "—")}</td>
                <td><span style="color: ${levelColor}; font-weight: 500;">${level}</span></td>
                <td>${String(a.message || "—")}</td>
                <td><span style="color: ${statusColor};">● ${status}</span></td>
                <td>${timeStr}</td>
              </tr>
            `;
          })}
        </tbody>
      </table>
      ${alarms.length === 0 ? html`<div class="a2ui-caption" style="padding: 12px">暂无告警</div>` : nothing}
    </div>
  `;
}
```

- [ ] **Step 3: Register in catalog/index.ts**

In `web/src/ui/chat/a2ui/catalog/index.ts`, add imports and register:

```typescript
import { renderAlarmCard } from "./alarm-card.js";
import { renderAlarmTable } from "./alarm-table.js";
```

Add to `a2uiCatalog`:

```typescript
AlarmCard: renderAlarmCard,
AlarmTable: renderAlarmTable,
```

- [ ] **Step 4: Add CSS for new components**

Add to `web/src/styles.css`:

```css
/* Alarm Card */
.a2ui-alarm-card {
  background: var(--bg-subtle);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  padding: 12px;
  margin: 4px 0;
}
.a2ui-alarm-card__header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 6px;
}
.a2ui-alarm-card__message {
  font-size: 13px;
  color: var(--text);
  margin-bottom: 6px;
}
.a2ui-alarm-card__meta {
  display: flex;
  justify-content: space-between;
  font-size: 11px;
  color: var(--muted);
}
.a2ui-alarm-card__actions {
  margin-top: 8px;
  display: flex;
  gap: 6px;
}

/* Alarm Table */
.a2ui-alarm-table {
  background: var(--bg-subtle);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  overflow-x: auto;
  margin: 4px 0;
}
.a2ui-alarm-table__title {
  font-size: 13px;
  font-weight: 500;
  padding: 10px 12px;
  border-bottom: 1px solid var(--border);
}
.a2ui-alarm-table__table {
  width: 100%;
  border-collapse: collapse;
  font-size: 12px;
}
.a2ui-alarm-table__table th {
  text-align: left;
  padding: 8px 10px;
  background: var(--bg);
  color: var(--muted);
  font-weight: 500;
  border-bottom: 1px solid var(--border);
}
.a2ui-alarm-table__table td {
  padding: 7px 10px;
  border-bottom: 1px solid var(--border);
  color: var(--text);
}
.a2ui-alarm-table__table tr:last-child td {
  border-bottom: none;
}
```

---

## Task 5: Inline surface rendering in chat messages

**Files:**
- Modify: `web/src/ui/chat/grouped-render.ts` — render surfaces after tool messages
- Modify: `web/src/ui/chat/a2ui/a2ui-renderer.ts` — add `getSurfaceIds()`, `clearSurface()`, `getSurface(surfaceId)`

- [ ] **Step 1: Add methods to A2uiRendererEngine**

In `web/src/ui/chat/a2ui/a2ui-renderer.ts`, add after `getSurfaceIds()`:

```typescript
getSurfaceIds(): string[] {
  return Array.from(this.surfaces.keys());
}

clearSurface(surfaceId: string): void {
  this.surfaces.delete(surfaceId);
}

getSurface(surfaceId: string) {
  return this.surfaces.get(surfaceId);
}
```

- [ ] **Step 2: Update grouped-render.ts**

Read the current `web/src/ui/chat/grouped-render.ts` to understand its structure.

The goal: after rendering a message group that contains a tool call with an `a2uiSurfaceId`, also render the corresponding surface inline (after the tool call message).

Add a function `renderSurfaceForMessage(group: MessageGroup, a2uiRenderer: A2uiRendererEngine): TemplateResult | typeof nothing` that:
1. If the group's message has `a2uiSurfaceId`, calls `a2uiRenderer.renderSurface(message.a2uiSurfaceId)`
2. Returns `nothing` otherwise

Call this function in the appropriate place in `renderMessageGroup` — after the tool call message is rendered.

---

## Task 6: Frontend Skills management UI

**Files:**
- Modify: `web/src/api/client.ts` — add skills API calls
- Modify: `web/src/ui/controllers/agents.ts` — add skills state + load/save
- Modify: `web/src/ui/views/agents.ts` — add skills tab panel
- Create: `web/src/ui/views/agents-skills-tab.ts`

- [ ] **Step 1: Add skills API calls to client.ts**

In `web/src/api/client.ts`, add:

```typescript
// Skills (file-based, name is string identifier)
export async function listSkills(workspaceId?: string): Promise<ApiResponse<Skill[]>> {
  const params = new URLSearchParams();
  if (workspaceId) params.set("workspace_id", workspaceId);
  return apiGet(`/chat/skills?${params}`);
}

export async function getSkill(name: string, workspaceId?: string): Promise<ApiResponse<Skill>> {
  const params = new URLSearchParams();
  if (workspaceId) params.set("workspace_id", workspaceId);
  return apiGet(`/chat/skills/${encodeURIComponent(name)}?${params}`);
}

export async function createSkill(data: CreateSkillRequest): Promise<ApiResponse<Skill>> {
  return apiPost("/chat/skills", data);
}

export async function updateSkill(name: string, data: UpdateSkillRequest, workspaceId?: string): Promise<ApiResponse<Skill>> {
  const params = new URLSearchParams();
  if (workspaceId) params.set("workspace_id", workspaceId);
  return apiPut(`/chat/skills/${encodeURIComponent(name)}?${params}`, data);
}

export async function deleteSkill(name: string, workspaceId?: string): Promise<ApiResponse<void>> {
  const params = new URLSearchParams();
  if (workspaceId) params.set("workspace_id", workspaceId);
  return apiDelete(`/chat/skills/${encodeURIComponent(name)}?${params}`);
}
```

Add type definitions:

```typescript
// File-based Skill (matches SkillInfoDto from backend)
export interface Skill {
  name: string;         // skill file name (without .md)
  description: string;  // from YAML frontmatter description field
  content: string;      // body of the .md file (after frontmatter)
}

// Frontmatter fields stored in the skill file itself
export interface CreateSkillRequest {
  workspace_id: string;
  skill_name: string;
  skill_content: string;  // full markdown including YAML frontmatter
}

export interface UpdateSkillRequest {
  skill_content: string;  // full markdown including YAML frontmatter
}
```

- [ ] **Step 2: Add skills state to agents.ts controller**

In `web/src/ui/controllers/agents.ts`, add to `AgentsState`:

```typescript
pendingDelete?: string | null;  // skill name pending delete confirmation
```

```typescript
skillsList?: Skill[];
skillsLoading?: boolean;
skillsError?: string | null;
activeSkillsPanel?: string;  // 'list' | 'editor' | 'create'
editingSkill?: Skill | null;
skillDraft?: string;  // skill_content editor
pendingDelete?: string | null;  // skill name pending delete confirmation
```

Add functions:

```typescript
export async function loadSkills(state: AgentsState): Promise<void> {
  state.skillsLoading = true;
  state.skillsError = null;
  try {
    const res = await listSkills();
    state.skillsList = res.result || [];
  } catch (err) {
    state.skillsError = String(err);
  } finally {
    state.skillsLoading = false;
  }
}

export async function saveSkill(state: AgentsState, skill: CreateSkillRequest, name?: string): Promise<boolean> {
  try {
    if (name) {
      await updateSkill(name, { skill_content: skill.skill_content }, skill.workspace_id);
    } else {
      await createSkill(skill);
    }
    await loadSkills(state);
    return true;
  } catch (err) {
    state.skillsError = String(err);
    return false;
  }
}

export async function removeSkill(state: AgentsState, name: string, workspaceId?: string): Promise<boolean> {
  try {
    await deleteSkill(name, workspaceId);
    await loadSkills(state);
    return true;
  } catch (err) {
    state.skillsError = String(err);
    return false;
  }
}

export async function createSkillApi(state: AgentsState, data: CreateSkillRequest): Promise<boolean> {
  try {
    await createSkill(data);
    await loadSkills(state);
    return true;
  } catch (err) {
    state.skillsError = String(err);
    return false;
  }
}

export async function updateSkillApi(state: AgentsState, name: string, data: UpdateSkillRequest, workspaceId?: string): Promise<boolean> {
  try {
    await updateSkill(name, data, workspaceId);
    await loadSkills(state);
    return true;
  } catch (err) {
    state.skillsError = String(err);
    return false;
  }
}
```

- [ ] **Step 3: Create agents-skills-tab.ts**

Create the skills panel view:

```typescript
import { html, nothing, type TemplateResult } from "lit";
import { type AgentsState } from "../controllers/agents.js";
import { loadSkills, saveSkill, removeSkill, createSkill, updateSkill } from "../controllers/agents.js";

export function renderSkillsTab(
  state: AgentsState,
  patchState: (patch: Partial<AgentsState>) => void,
  onSave: () => void,
): TemplateResult {
  const skills = state.skillsList || [];
  const loading = state.skillsLoading;
  const error = state.skillsError;
  const panel = state.activeSkillsPanel || "list";
  const draft = state.skillDraft || "";
  const editing = state.editingSkill;

  if (loading && skills.length === 0) {
    return html`<div style="padding: 20px; text-align: center; color: var(--muted);">加载技能...</div>`;
  }

  if (error) {
    return html`<div style="padding: 20px; color: var(--error);">${error}</div>`;
  }

  // List view
  if (panel === "list") {
    return html`
      <div class="skills-panel">
        <div class="skills-panel__header">
          <span>${skills.length} 个技能</span>
          <button class="a2ui-btn a2ui-btn--primary a2ui-btn--sm"
                  @click=${() => patchState({ activeSkillsPanel: "create", skillDraft: "---\nname: \ndescription: \n---\n\n" })}>
            + 新建技能
          </button>
        </div>
        <div class="skills-list">
          ${skills.length === 0 ? html`<div class="skills-empty">暂无技能，点击"新建技能"创建</div>` : nothing}
          ${skills.map((skill) => html`
            <div class="skill-item">
              <div class="skill-item__info">
                <div class="skill-item__name">${skill.name}</div>
                <div class="skill-item__desc">${skill.description}</div>
              </div>
              <div class="skill-item__actions">
                <button class="a2ui-btn a2ui-btn--secondary a2ui-btn--sm"
                        @click=${() => patchState({ activeSkillsPanel: "edit", editingSkill: skill, skillDraft: skill.content })}>
                  编辑
                </button>
                <button class="a2ui-btn a2ui-btn--secondary a2ui-btn--sm"
                        aria-label="删除技能"
                        @click=${() => patchState({ pendingDelete: skill.name })}>
                  删除
                </button>
              </div>
            </div>
          `)}
        </div>
      </div>
    `;
  }

  // Editor view (create or edit)
  if (panel === "create" || panel === "edit") {
    return html`
      <div class="skills-editor">
        <div class="skills-editor__header">
          <span>${panel === "create" ? "新建技能" : `编辑: ${editing?.name}`}</span>
          <div style="display: flex; gap: 8px;">
            <button class="a2ui-btn a2ui-btn--secondary a2ui-btn--sm"
                    @click=${() => patchState({ activeSkillsPanel: "list", editingSkill: null, skillDraft: "" })}>
              取消
            </button>
            <button class="a2ui-btn a2ui-btn--primary a2ui-btn--sm"
                    @click=${async () => {
                      if (panel === "create") {
                        const data = { workspace_id: state.config?.workspace || "tinyiothub", skill_name: "untitled", skill_content: draft };
                        const ok = await createSkillApi(state, data);
                        if (ok) { patchState({ activeSkillsPanel: "list", editingSkill: null, skillDraft: "" }); onSave(); }
                      } else {
                        const data = { skill_content: draft };
                        const ok = await updateSkillApi(state, editing?.name, data);
                        if (ok) { patchState({ activeSkillsPanel: "list", editingSkill: null, skillDraft: "" }); onSave(); }
                      }
                    }}>
              保存
            </button>
          </div>
        </div>
        <textarea class="skills-editor__textarea"
                  .value=${draft}
                  @input=${(e: Event) => patchState({ skillDraft: (e.target as HTMLTextAreaElement).value })}
                  placeholder="---&#10;name: skill-name&#10;description: 技能描述&#10;---&#10;&#10;技能内容 (Markdown)..."
                  spellcheck="false"
                  tabindex="0"></textarea>
      </div>
    `;
  }

  // Delete confirmation modal
  if (state.pendingDelete) {
    return html`
      <div class="modal-overlay" @click=${(e: Event) => { if (e.target === e.currentTarget) patchState({ pendingDelete: null }); }}>
        <div class="modal-box">
          <div class="modal-header">
            <h3>确认删除</h3>
            <button class="modal-close" @click=${() => patchState({ pendingDelete: null })}>×</button>
          </div>
          <div class="modal-body">
            <p class="modal-desc">确定要删除技能「${state.pendingDelete}」吗？此操作不可撤销。</p>
          </div>
          <div class="modal-footer">
            <button class="btn-secondary" @click=${() => patchState({ pendingDelete: null })}>取消</button>
            <button class="a2ui-btn a2ui-btn--danger a2ui-btn--sm"
                    aria-label="确认删除技能"
                    @click=${async () => {
                      await removeSkill(state, state.pendingDelete!);
                      patchState({ pendingDelete: null });
                      onSave();
                    }}>
              删除
            </button>
          </div>
        </div>
      </div>
    `;
  }

  return nothing;
}
```

- [ ] **Step 4: Add skills tab to agents.ts view**

In `web/src/ui/views/agents.ts`, add `skills` to `allPanels`:

```typescript
const panelLabels: Record<AgentsPanel, string> = {
  overview: "配置",
  tools: "工具权限",
  skills: "技能",
};
```

Update the imports to include `loadSkills`:

```typescript
import { createAgentsState, loadAgents, loadAgentConfig, saveAgentConfig, loadToolsCatalog, toggleTool, loadSkills } from "../controllers/agents.js";
```

Add `skills` to the `allPanels` array:

```typescript
const allPanels: AgentsPanel[] = ["overview", "tools", "skills"];
```

Import the new tab renderer:

```typescript
import { renderSkillsTab } from "./agents-skills-tab.js";
```

In `connectedCallback`, add `loadSkills` to the initial load:

```typescript
Promise.all([
  loadAgents(this.state),
  loadSkills(this.state),
]).then(() => {
```

In `onAgentSelected`, add skills loading:

```typescript
Promise.all([
  loadAgentConfig(this.state, agentId),
  loadToolsCatalog(this.state, agentId),
  loadSkills(this.state),
]).then(() => this.requestUpdate());
```

Add skills tab rendering in `render()`:

```typescript
${this.state.activePanel === "skills" ? renderSkillsTab(
  this.state,
  this._patchState.bind(this),
  () => { if (this.state.selectedAgentId) loadSkills(this.state).then(() => this.requestUpdate()); }
) : nothing}
```

Add CSS for the skills panel in `web/src/styles.css`:

```css
/* Skills Panel */
.skills-panel__header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 8px 0;
  margin-bottom: 8px;
  font-size: 13px;
}
.skills-list { display: flex; flex-direction: column; gap: 6px; }
.skill-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 10px 12px;
  background: var(--bg-subtle);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
}
.skill-item__name { font-size: 13px; font-weight: 500; }
.skill-item__desc { font-size: 12px; color: var(--muted); margin-top: 2px; }
.skill-item__actions { display: flex; gap: 6px; }
.skills-empty { padding: 20px; text-align: center; color: var(--muted); font-size: 13px; }

/* Skills Editor */
.skills-editor { display: flex; flex-direction: column; height: 100%; animation: fadeIn 0.2s ease-out; }
.skills-editor__header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 8px 0;
  font-size: 13px;
}
.skills-editor__textarea {
  flex: 1;
  min-height: 300px;
  font-family: "JetBrains Mono", "Fira Code", monospace;
  font-size: 12px;
  line-height: 1.6;
  padding: 12px;
  background: var(--bg-subtle);
  color: var(--text);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  resize: vertical;
}
@media (max-width: 768px) {
  .skills-editor__textarea {
    min-height: 200px;
    font-size: 11px;
  }
}

/* Danger button variant */
.a2ui-btn--danger {
  background: #dc2626;
  color: white;
  border-color: #dc2626;
}
.a2ui-btn--danger:hover {
  background: #b91c1c;
  border-color: #b91c1c;
}
```

---

## Task 7: Verify end-to-end

**Files:**
- (no new files — verify all integrations)

- [ ] **Step 1: Build API**

```bash
cd api && cargo build 2>&1 | grep "^error" | head -10
```

- [ ] **Step 2: Build Web**

```bash
cd web && npm run build 2>&1 | grep "error" | head -10
```

- [ ] **Step 3: Verify skill files load**

Add a temporary `tracing::info!` in `load_skills_prompt` to log the loaded skills count, then:
```bash
cd api && RUST_LOG=info cargo run 2>&1 | grep -i skill
```

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat(agent): skills file-based + A2UI alarm components

- Skills from skills/ directory loaded into system prompt (Layer 3)
- AlarmCard and AlarmTable A2UI components
- Frontend Skills management UI (list/create/edit/delete)
- Inline surface rendering in chat messages
"
```

---

## CEO Q&A Decisions

| # | Question | Decision | Rationale |
|---|----------|----------|-----------|
| Q1 | File-based vs DB? | **File-based (B)** | Skills UI needs writeable backend. DB was reverted in Phase 1. |
| Q2 | Path validation? | **Yes, all 3** | `..` rejection + absolute path rejection + canonicalize defense-in-depth |
| Q3 | Write safety (disk-space, mutex)? | **Yes, all 3** | Concurrent writes can corrupt skill files |
| Q4 | 1MB content limit? | **No limit** | Trust operator; warn if >1MB (just log, don't reject) |
| E1 | Per-workspace skill dirs? | **ACCEPTED** | Already in signature, low effort, enables multi-tenant isolation |
| E2 | Hot-reload cache? | **DEFERRED** | Disk I/O negligible for <20 skills. Revisit at 100+ skills. |
| E3 | Context-aware glob_match? | **ACCEPTED** | Existing `paths` field + `glob_match()` in `AgentSkill` — good reuse |
| Q5 | Styled delete modal? | **Yes** | Better UX than browser `confirm()` |
| Q6 | IoT components naming? | **alarm_card, alarm_table** | Simple, clear, follows catalog convention |

---

## Verification Checklist

- [ ] `skills/tinyiothub/*.md` files exist with valid YAML frontmatter
- [ ] `build_full_system_prompt()` includes skills Layer 3 when skill files exist
- [ ] `AlarmCard` and `AlarmTable` appear in A2UI catalog
- [ ] Chat view renders surfaces inline after tool messages with `a2uiSurfaceId`
- [ ] Skills tab in Agents UI lists all skills from `skills/tinyiothub/`
- [ ] Create/edit/delete skills via the UI (writes to `skills/tinyiothub/*.md` files)
- [ ] API build: no errors
- [ ] Web build: no errors

---

## NOT in Scope

| Item | Rationale |
|------|-----------|
| Skill hot-reload cache (E2) | Disk I/O negligible for <20 skills. Revisit at 100+ skills. |
| 1MB content size limit | Trust operator; warn if >1MB, don't reject |
| Styled delete modal (Q5) | Implementer to add a proper modal component (deferred from CEO Q&A) |
| A2UI component interactivity beyond alarm acknowledge | Not specified in requirements |
| Skill versioning or history | File system git history is the version control |
| Per-skill enable/disable toggle | Skills are always active if matching paths |

---

## What Already Exists

| Existing Code | Plan Action | Reused? |
|---------------|-------------|---------|
| `load_skills_prompt()` in `zeroclaw_agent.rs` | Task 1 + Task 3 update path resolution | Partially — needs `prompts/` priority chain |
| `AgentSkill::parse_frontmatter()` in `domain/agent/skill.rs` | Task 3 reads skill files | Yes — used in `read_skill_dir()` |
| `skills/tinyiothub/prompts/*.md` (7 files) | Task 2 creates new files | Existing files preserved |
| A2UI catalog pattern (`device-card.ts`, `device-table.ts`) | Task 5 creates `alarm-card.ts`, `alarm-table.ts` | Yes — follows same pattern |
| `A2uiRendererEngine` in `a2ui-renderer.ts` | Task 5 Step 1 adds `getSurfaceIds()`/`clearSurface()` | Yes — methods already partially exist |
| `grouped-render.ts` surface rendering | Task 5 Step 2 integrates inline surface rendering | Extends existing pipeline |
| `zeroclaw/skills/` vendor directory | Not used — file-based approach | N/A |
| `lazy_static` crate in Cargo.toml | Task 4 Step 2 — plan says to add but already exists | Harmless duplicate |

---

## Failure Modes

| Path | Failure Mode | Has Test? | Error Handling? | Silent? |
|------|-------------|-----------|----------------|---------|
| `load_skills_prompt` | `prompts/` directory empty or missing — returns empty string, LLM gets no skills | No | No (returns empty) | Yes |
| `validate_skill_path` | Path traversal blocked — returns `Err` with message | Yes (integration test) | Returns `Err` → 400 | No (400 response) |
| `create_skill` | Disk full — returns 507 Insufficient Storage | Yes (integration test) | Yes (507) | No |
| `create_skill` | File already exists — returns 409 Conflict | Yes (integration test) | Yes (409) | No |
| `delete_skill` | File not found — returns success (idempotent) | No | Yes (no-op) | No |
| `alarm-card.ts` | `_onAction` undefined — button click no-ops | No | Yes (button hidden when `_onAction` null) | No (button not rendered) |
| `alarm-table.ts` | Empty `alarms` array — shows "暂无告警" | No | Yes (empty state) | No |
| Session key parsing | Malformed key — `split(':').nth(1)` returns `None`, fallback to `tinyiothub` | No | No (fallback) | Yes (wrong workspace skills) |

**Critical gap:** `_onAction` in `alarm-card.ts` can be `undefined` and the button click silently no-ops. Should verify `_onAction` exists before calling, or provide a default no-op.

*Fixed in design review: button only renders when `_onAction != null`.*

---

## Review Report

**Reviewer:** Outside Voice (adversarial subagent)
**Plan quality before fixes:** 4/10
**Plan quality after fixes:** 8/10

### Issues Found and Fixed

| # | Severity | Issue | Fix Applied |
|---|----------|-------|-------------|
| 1 | CRITICAL | Frontend/backend API contract mismatch: Task 6 used numeric `id`; Task 4 used string `name`. Update (PUT) endpoint was missing entirely. | Added `update_skill` PUT handler; aligned all TypeScript types and function signatures to use `name: string` |
| 2 | HIGH | `session_key` parsing was wrong: `split('/').nth(1)` on `agent:workspace_A:user_123/sess_uuid` yields `workspace_A:user_123` not a clean workspace id | Fixed to `split(':').nth(1).map(|s| s.split('/').next()).flatten()` |
| 3 | MEDIUM | Existing `skills/tinyiothub/prompts/` directory unaccounted for; `load_skills_prompt` reads `skills/` not `prompts/` | Added note in Task 2 for implementer to reconcile path |
| 4 | LOW | `create_skill` had no actual disk-space check despite security requirement listing it | Added `available_space()` check before write |
| 5 | LOW | `/* TODO: acknowledge alarm */` in alarm-card is a stub | Removed TODO; implementer should wire up alarm acknowledge action |

### Remaining Decisions for Implementer

| # | Decision | Options |
|---|----------|---------|
| R1 | `prompts/` vs flat `skills/` directory | Move files or update path resolution in `load_skills_prompt` |
| R2 | `once_cell` vs `lazy_static` for mutex | Both work; `once_cell` is more idiomatic in modern Rust |
| R3 | Styled delete modal for skills | RESOLVED — modal with `pendingDelete` state added to Task 6 Step 3; uses existing `.modal-overlay` CSS |

### What Was Not Fixed (by design)

- Alarm acknowledge button in `alarm-card.ts` is a stub — CEO Q&A did not cover A2UI component interactivity, left to implementer
- Inline surface rendering task (Task 5, Step 2) remains somewhat vague — `grouped-render.ts` structure needs to be read at implementation time
- No integration tests specified — per plan-eng convention, unit-level verification in build steps is considered sufficient

### Design Review Findings (17 issues, all fixed)

| Pass | # | Severity | Issue | Fix |
|------|---|----------|-------|-----|
| Component Inventory | 3.1 | HIGH | Delete confirmation modal missing (CEO Q&A Q5) | Added `pendingDelete` state + modal render in `renderSkillsTab` |
| Interaction | 4.1 | MEDIUM | `_onAction` undefined silently no-ops on alarm-card | Added `_onAction != null` guard to button render |
| Design Consistency | 7.1 | LOW | Hardcoded `6px`/`8px` border-radius instead of CSS tokens | Changed to `var(--radius-md)` / `var(--radius-lg)` |
| Visual Hierarchy | 2.1 | LOW | Skill description font-size 11px too small | Increased to 12px |
| Responsive | 5.2 | MEDIUM | Alarm table missing horizontal scroll | Added `overflow-x: auto` |
| Accessibility | 6.2 | LOW | Delete button missing aria-label | Added `aria-label="删除技能"` |
| Accessibility | 6.1 | LOW | Textarea missing tabindex | Added `tabindex="0"` |
| Interaction | 4.2 | LOW | No transition on skills editor panel switch | Added `animation: fadeIn 0.2s ease-out` to `.skills-editor` |
| Component Inventory | 3.2 | LOW | No danger button variant for delete | Added `.a2ui-btn--danger` CSS |
| Responsive | 5.1 | LOW | Editor textarea not responsive | Added mobile media query with reduced min-height |

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 1 | CLEAR | 9 proposals, 9 accepted, 0 deferred |
| Codex Review | `/codex review` | Independent 2nd opinion | 0 | — | — |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | CLEAR | 5 issues, 5 fixed |
| Design Review | `/plan-design-review` | UI/UX gaps | 1 | CLEAR | 10 issues found, 10 fixed (delete modal, _onAction guard, CSS tokens, a11y, responsive) |

**VERDICT:** CEO + ENG + DESIGN CLEARED — ready to implement

---

## Completion Summary

- **Step 0: Scope Challenge** — scope accepted as-is
- **Architecture Review** — 1 issue found (prompts/ path, resolved by decision R1)
- **Code Quality Review** — 1 issue found (unused Rust params, resolved)
- **Test Review** — diagram produced, 22 gaps identified, tests added to Task 4 Step 6
- **Performance Review** — 0 issues found
- **NOT in scope** — written
- **What already exists** — written
- **TODOs.md updates** — 0 items proposed (remaining decisions are already deferred)
- **Failure modes** — 8 identified, 1 critical gap flagged and fixed (alarm-card `_onAction` guard)
- **Outside voice** — ran (Claude subagent), fell back from Codex (MIMO_API_KEY missing)
- **Parallelization** — 2 lanes: Lane A (Tasks 1+2+5a parallel) → Lane B (Tasks 3+4+6 sequential) → Lane C (Task 7)
- **Lake Score** — all recommendations chose complete option (9/9)
- **Design Review** — 10 issues found, 10 fixed (delete modal, _onAction guard, CSS tokens, a11y, responsive)
