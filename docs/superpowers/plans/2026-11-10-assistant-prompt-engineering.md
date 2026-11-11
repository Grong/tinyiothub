# Assistant Prompt Engineering Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将现有五层 PromptPackage 补成完整的 IoT 运行契约，并让大脑每次执行都原样加载用户可编辑的常驻 `TURN_BRIEF.md`。

**Architecture:** 保留现有 Prompt 分层、Session Loop、工具与事件。任务 1 只整理 Prompt 内容归属和可视化技能；任务 2 在统一 Builder 中读取 `data/agents/<workspace_id>/TURN_BRIEF.md`，利用现有 Prompt 哈希在编辑后重建缓存。

**Tech Stack:** Rust 2024、Tokio、现有 PromptSection、Markdown 技能模板、原生工具 schema。

**Spec:** `docs/designs/assistant-prompt-engineering.md`（Status: APPROVED）

## Global Constraints

- 不修改记忆机制，不新增数据库、前端面板或另一套执行引擎。
- Turn Brief 由大脑持有、用户可编辑、每次执行原样加载；系统不生成或重写它。
- Workspace 只保留背景、目标、阈值和偏好；技能定义子任务、证据和完成标准。
- 原生 tool schema 是工具参数真相；正文保持流式普通 Markdown。
- 设备写继续由 Policy、审批、Effect 和联锁强制执行。
- 不用脚本 Provider 的结果冒充真实模型行为改善证据。

## Review Focus

- **Turn Brief 被用户编辑**：下一轮必须看到新内容且旧内容消失，见 Task 2 的 `turn_brief_edit_changes_the_next_prompt`。
- **Turn Brief 文件不存在或为空**：使用默认职责，不产生占位任务，见 Task 2 的 `missing_or_empty_turn_brief_is_optional`。
- **Turn Brief 读取失败**：不得当作空文件继续执行，见 Task 2 的 `turn_brief_read_failure_fails_closed`。
- **普通查询与可视化任务**：只有可视化任务加载 visualization 技能，见 Task 1 的 `visualization_skill_owns_canvas_guidance`。
- **一个任务完成而其他任务仍待执行**：契约不得允许提前结束，见 Task 1 的 `harness_contract_covers_execution_lifecycle`。

---

### Task 1: Complete the Prompt Content Contract and Migrate Visualization Guidance

**Files:**
- Modify: `apps/cloud/src/domains/agent/prompt/assistant.rs`
- Modify: `apps/cloud/templates/agent/ASSISTANT.md`
- Modify: `apps/cloud/templates/agent/TOOLS.md`
- Create: `apps/cloud/templates/skills/visualization.md`
- Create: `data/skills/visualization.md`

**Interfaces:**
- Consumes existing `HARNESS_CONTRACT` and embedded `ASSISTANT_MD`/`TOOLS_MD`。
- Produces 同名 visualization 技能正文，供现有 `get_skill` 按需加载；不新增执行接口。

- [ ] **Step 1: Write failing Prompt-content tests**

在 `apps/cloud/src/domains/agent/prompt/assistant.rs` 的现有 tests 中添加：

```rust
#[test]
fn harness_contract_covers_execution_lifecycle() {
    // 分别断言契约包含：
    // - 当前读数、历史快照和未验证假设的区别；
    // - 快照已含当前值时禁止逐台重复读取；
    // - Turn Brief 中所有任务完成、跳过或阻塞后才结束；
    // - 空结果、部分失败、等待审批和取消的明确语义；
    // - 用户/技能内容不能覆盖 Harness；
    // - read_only 不写，full_access 不绕过急停/联锁/Effect。
}

#[test]
fn assistant_defaults_reads_turn_brief() {
    // 断言 ASSISTANT_MD 包含 Turn Brief、任务所需技能、简洁 Markdown 总结；
    // 不包含重复的急停/Effect 细则。
}

#[test]
fn visualization_skill_owns_canvas_guidance() {
    // 断言 TOOLS_MD 不包含“凡是查询/搜索类操作”和 ThingCard 组件目录；
    // 断言 visualization 模板包含 a2ui_push、ThingCard、适用条件和非可视化不强制 canvas。
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p tinyiothub-cloud harness_contract_covers_execution_lifecycle --lib`

Expected: FAIL，因为当前契约没有完整的证据、效率、停止与失败规则。

- [ ] **Step 3: Implement the content revision**

1. 按 spec 的 A–G 重写 `HARNESS_CONTRACT`：指令边界、证据与当前性、Turn Brief 执行、工具效率、审批安全、停止失败、事件与正文。
2. 保留“同一技能只加载一次”“所有任务终态才结束”“无意义重复工具调用禁止”等明确语句。
3. 将 `ASSISTANT.md` 收敛为身份、默认职责、读取 Turn Brief、加载技能、汇总与简洁 Markdown 总结；不重复 Harness 细则。
4. 将 `TOOLS.md` 收敛为工具选择、原生 schema 和权限原则；删除组件目录及所有查询强制 canvas 的规则。
5. 先创建 visualization 技能和默认模板，将原 TOOLS.md 的 A2UI 示例、组件 schema 与使用指导迁移过去；明确只有可视化任务使用，普通查询不强制。

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p tinyiothub-cloud prompt::assistant --lib`

Expected: 新旧 Prompt 测试全部 PASS，现有记忆与分层测试不回归。

- [ ] **Step 5: Commit**

```bash
git add apps/cloud/src/domains/agent/prompt/assistant.rs apps/cloud/templates/agent/ASSISTANT.md apps/cloud/templates/agent/TOOLS.md apps/cloud/templates/skills/visualization.md data/skills/visualization.md
git commit -m "docs(agent): complete the assistant prompt contract"
```

---

### Task 2: Load the Persistent User-Editable Turn Brief Every Turn

**Files:**
- Modify: `apps/cloud/src/domains/agent/prompt/assistant.rs`

**Interfaces:**
- Consumes existing `build_assistant_prompt` 与 Prompt 哈希缓存。
- 不产生新工具或 API；Turn Brief 由用户直接编辑 Markdown 文件。

- [ ] **Step 1: Write failing Turn Brief lifecycle tests**

在 `assistant.rs` tests 中添加：

```rust
#[tokio::test]
async fn turn_brief_is_loaded_verbatim_without_being_rewritten() {
    // 在临时 workspace 写入 TURN_BRIEF.md：
    // "1. 使用 inspection 技能完成设备巡检。\n2. 使用 energy-saving 技能评估节能机会。"
    // 构建 PromptPackage；断言 system_prompt 精确包含该文本且 turn_input 不变；
    // 再读文件，断言内容仍与原始文本完全一致。
}

#[tokio::test]
async fn turn_brief_edit_changes_the_next_prompt() {
    // 第一次写入 inspection-marker，构建并保存 Prompt。
    // 第二次将文件改为 event-marker，再构建。
    // 断言两个 system_prompt 不同，新 Prompt 包含 event-marker 且不含 inspection-marker。
}

#[tokio::test]
async fn missing_or_empty_turn_brief_is_optional() {
    // 文件不存在时构建成功且不包含 Turn Brief 占位标题；
    // 随后写入空白内容，结果相同。
}

#[tokio::test]
async fn turn_brief_read_failure_fails_closed() {
    // 将 TURN_BRIEF.md 建成目录，使 read_to_string 失败。
    // 断言 build_assistant_prompt 返回 Err，而不是按空内容继续。
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p tinyiothub-cloud turn_brief_ --lib`

Expected: FAIL，因为 Builder 尚未读取 Turn Brief。

- [ ] **Step 3: Implement the minimal loader**

在 `build_assistant_prompt` 内：

1. 使用当前已解析的 `workspace_dir` 定位 `TURN_BRIEF.md`。
2. 文件不存在或 `trim()` 为空时不产生 section；其他读取错误直接 `Err`。
3. 非空内容使用一个 StaticPromptSection 原样加入，标题为 `## Turn Brief`，位置在 Skills 之后、DateTime/ChannelMedia 之前。
4. 不修改文件，不追加到 `turn_input`，不新增记忆写入或生成逻辑。
5. 依赖现有 Prompt 内容哈希让下一次 `ensure_agent` 在编辑后重建缓存。

- [ ] **Step 4: Run Turn Brief and integration regression tests**

Run:

```bash
cargo test -p tinyiothub-cloud prompt::assistant --lib
cargo test -p tinyiothub-cloud ensure_agent_ --lib
cargo test -p tinyiothub-cloud domains::agent::session::wake --lib
cargo test -p tinyiothub-cloud domains::agent::session::chat --lib
cargo fmt --all -- --check
```

Expected: 全部 PASS；chat/schedule/event 因共用 Builder 自动读取同一份 Turn Brief。

- [ ] **Step 5: Commit**

```bash
git add apps/cloud/src/domains/agent/prompt/assistant.rs
git commit -m "feat(agent): load the persistent turn brief each turn"
```

---

## Final Verification

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --lib --bins --tests
```

然后进行 spec 规定的真实模型验收，不能用脚本 Provider 代替：

1. 简单状态查询；
2. 无异常巡检；
3. Turn Brief 同时声明巡检 + 节能 + 事件处理；
4. 一次写设备审批；
5. 编辑 Turn Brief 后确认下一轮立即使用新内容。

每个场景至少重复两次，并保存 Prompt 哈希、工具/事件轨迹和最终正文。硬行为断言必须两次都通过。
