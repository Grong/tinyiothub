# Crates 重组（P0–P6 全量）Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 85k 行 cloud god crate 与 crates/* 双轨架构重组为 21 个单一职责 crate + apps/ + drivers/ 布局，行为零变化。

**Architecture:** 设计文档 `docs/superpowers/specs/2026-08-03-crates-reorg-design.md`（CEO+ENG 双 CLEAR）为唯一事实源。分 8 个阶段执行：P0 基线 → P1 改名合并 → P2 数据/运行时层 → P3 web 层 → P3.5 AI 层 → P4.0 斩环 → P4 渐进领域抽取（thing 试点定型）→ P5 apps/ 归位+CI/CD → P6 文档。

**Tech Stack:** Rust 2024 edition / Cargo workspace / Axum 0.8 / SQLx 0.9.0-alpha.1 (SQLite) / tokio。

**上游输入（已完成的 P0 产出）：**
- 环扫描（2026-08-03，python 脚本扫 `modules::` 引用）实测 **5 组环**：
  `agent↔chat`（chat 规划在 agent crate 内，无害）、`alarm↔event`、`event↔notification`、
  `agent↔thing`（agent/tools/thing.rs）、`agent↔workspace`（agent/agent.rs ↔ workspace/{service,handler/mod,handler/heartbeat}.rs）
- 单向边记录：agent→{event,mcp}, auth→{system,user}, batch→device, device→{event,monitoring,template},
  drivers→device, marketplace→{device,template}, mcp→{alarm,heartbeat}, open→{mcp,tenant},
  plugin→{agent,device}, system→{agent,user}, workspace→mcp

## Global Constraints

1. **Lib 命名规则（用户裁决 2026-08-03）**：目录与 package 用短名（`crates/core`、package `core`），
   但每个 crate 的 `[lib] name = "tinyiothub_<短名>"` 显式钉住。现有 `use tinyiothub_core::` 等
   **零重写**；新 crate（llm/scheduler/各领域）从创建起即设 `[lib] name = "tinyiothub_llm"` 等。
   ⚠️ package `core` 与 Rust 内置 core 不冲突（lib 名不同），但 `use core::` 永远指向内置库，禁止。
2. **行为零变化**：不改业务逻辑、API 行为、DB schema。每个 Task 结束 `cargo check --workspace` 必须绿。
3. **测试门禁**：每个 Task 结束运行受影响的 `cargo test -p <crate>`；全量 `cargo test --workspace`
   在每个 Phase 末运行。E2E（T19 脚本）只在 P5 运行（用户裁决）。
4. **每个 Task 一个 commit**，信息格式 `<type>(<scope>): <desc> (PX-TaskN)`。禁止 `git add -A`，逐文件 add。
5. **执行分支**：`refactor/crates-reorg`，从 `chore/open-source-prep` 切出（含设计文档与追踪 checkbox）。
6. **循环依赖禁令**：新依赖边只允许设计文档 §依赖方向 中的方向。允许的领域间边：
   driver→thing、notify→event、alarm→event、agent→{event,thing,tenant,policy,memory,skills,llm}、
   mcp→{alarm,agent}、user→tenant、auth→{user,tenant}（实现中确认，不成环即可）。
7. **core 守门条款**：core 只许 trait + 值类型（DTO/error/config），禁止逻辑函数与 I/O。
8. **db 用 buzz 模式**：具体 struct、无 trait 倒置（AI 层内部的 trait 注入先例除外）、按领域平铺
   单文件模块（<2k 行/文件，超出再拆子模块）、测试用真实 SQLite。
9. 文档同步义务：AGENTS.md 在 P1 更新（Task 5）；`docs/superpowers/specs/2026-08-03-crates-reorg-design.md`
   §9 checkbox 每周五勾选。

---

### Task 1: P0 收尾 — 基线确认 + 执行分支 + 设计文档环数据订正

**Files:**
- Modify: `docs/superpowers/specs/2026-08-03-crates-reorg-design.md`（§1.2 第 3 条）
- Create: branch `refactor/crates-reorg`

**Interfaces:**
- Consumes: 已运行的 `just ci`（后台任务 banw86in1）与环扫描结果
- Produces: 绿基线 + 执行分支 + 订正后的环清单（5 组），供 Task 15-18（P4.0）使用

- [ ] **Step 1: 确认 CI 基线绿**

Run: `just ci`（若后台任务已完成则查看其输出尾部）
Expected: fmt/clippy/test 全绿。若不绿，先修复再开工（基线不绿禁止进入 P1）。

- [ ] **Step 2: 订正设计文档环清单**

将 §1.2 第 3 条"已核实的循环引用 3 组"改为：

```markdown
3. **已核实的循环引用 5 组**（P0 环扫描 2026-08-03 实测）：
   alarm↔event（event/router.rs 反向引用）、event↔notification（event/mod.rs:94 + service.rs:354）、
   thing→agent/mcp（thing/handler/actions.rs）、agent→thing（agent/tools/thing.rs，与上一项成环）、
   agent↔workspace（agent/agent.rs ↔ workspace/{service,handler/mod,handler/heartbeat}.rs）、
   agent↔chat（chat 规划并入 agent crate，内部消解）。
   另：mcp→alarm、mcp→heartbeat 依赖边（mcp/tools/alarm_mcp.rs）。
```

- [ ] **Step 3: 创建执行分支**

```bash
git checkout -b refactor/crates-reorg chore/open-source-prep
```

- [ ] **Step 4: Commit**

```bash
git add docs/superpowers/specs/2026-08-03-crates-reorg-design.md
git commit -m "docs: correct cycle inventory to 5 groups from P0 scan (P0-Task1)"
```

---

### Task 2: P1.1 — tinyiothub-error 并入 core（error 子模块）

**Files:**
- Create: `crates/tinyiothub-core/src/error.rs`（内容来自 error crate）
- Modify: `crates/tinyiothub-core/src/lib.rs`（加 `pub mod error;` + re-export）
- Modify: 所有引用 `tinyiothub-error` 的 Cargo.toml（改用 core）
- Delete: `crates/tinyiothub-error/`

**Interfaces:**
- Consumes: `tinyiothub_error::Error` 现有用法（`grep -rn "tinyiothub_error" --include="*.rs" -l`）
- Produces: `tinyiothub_core::error::{Error, Result}` re-export，外部路径变为
  `tinyiothub_core::error::Error`；`tinyiothub_core::Error` 也 re-export 保持兼容

- [ ] **Step 1: 迁移 error 类型**

```bash
git mv crates/tinyiothub-error/src/lib.rs crates/tinyiothub-core/src/error.rs
# 编辑 error.rs：移除 crate 级 attribute（如 #![...]），保留所有 pub 类型
```

在 `crates/tinyiothub-core/src/lib.rs` 顶部加入：

```rust
pub mod error;
pub use error::{Error, Result};
```

- [ ] **Step 2: 处理 sqlx feature 合并**

error crate 有 `sqlx-dep` feature（`sqlx = { optional = true }`）。core 已有 `sqlx` feature。
合并：core 的 `sqlx` feature 需同时覆盖原 error 的 sqlx 错误变体。检查
`grep -n "sqlx" crates/tinyiothub-core/src/error.rs`，把 `#[cfg(feature = "sqlx-dep")]`
改为 `#[cfg(feature = "sqlx")]`。

- [ ] **Step 3: 重写引用与依赖**

```bash
grep -rln "tinyiothub_error" --include="*.rs" . | xargs sed -i '' \
  's/tinyiothub_error::/tinyiothub_core::error::/g'
grep -rln "tinyiothub-error" --include="Cargo.toml" . | xargs sed -i '' \
  '/^tinyiothub-error = /d'
# 对原来依赖 tinyiothub-error 的 crate，在其 Cargo.toml [dependencies] 确认已有
# tinyiothub-core = { workspace = true }（没有则补上）
```

- [ ] **Step 4: 从 workspace 移除并验证**

```bash
# 根 Cargo.toml：members 不变（crates/* glob 自动排除已删目录）；
# [workspace.dependencies] 删除 tinyiothub-error 行
git rm -r crates/tinyiothub-error
cargo check --workspace
cargo test -p tinyiothub-core
```

Expected: 全绿。

- [ ] **Step 5: Commit**

```bash
git add crates/ Cargo.toml Cargo.lock
git commit -m "refactor(core): merge tinyiothub-error into core::error (P1-Task2)"
```

---

### Task 3: P1.2 — tinyiothub-config 并入 core（config 子模块）

**Files:**
- Create: `crates/tinyiothub-core/src/config.rs`（git mv 自 config crate）
- Modify: `crates/tinyiothub-core/src/lib.rs`（`pub mod config;`）
- Modify: `cloud/Cargo.toml`（删 tinyiothub-config 依赖）
- Delete: `crates/tinyiothub-config/`

**Interfaces:**
- Consumes: `grep -rn "tinyiothub_config" --include="*.rs" -l` 的使用点
- Produces: `tinyiothub_core::config::*`

- [ ] **Step 1: 迁移**

```bash
git mv crates/tinyiothub-config/src/lib.rs crates/tinyiothub-core/src/config.rs
# lib.rs 加：pub mod config;
```

- [ ] **Step 2: 重写引用**

```bash
grep -rln "tinyiothub_config" --include="*.rs" . | xargs sed -i '' \
  's/tinyiothub_config::/tinyiothub_core::config::/g'
grep -rln "tinyiothub-config" --include="Cargo.toml" . | xargs sed -i '' \
  '/^tinyiothub-config = /d'
# 根 Cargo.toml [workspace.dependencies] 删 tinyiothub-config 行
```

- [ ] **Step 3: 验证 + Commit**

```bash
git rm -r crates/tinyiothub-config
cargo check --workspace && cargo test -p tinyiothub-core
git add crates/ cloud/Cargo.toml Cargo.toml Cargo.lock
git commit -m "refactor(core): merge tinyiothub-config into core::config (P1-Task3)"
```

---

### Task 4: P1.3 — 目录/package 短名化（lib 名钉住不动）

**Files:**
- Modify: 全部 `crates/*/Cargo.toml`（package.name 改短名 + `[lib] name` 显式钉住）
- Modify: 根 `Cargo.toml`（`[workspace.dependencies]` 键名）
- Rename: `crates/tinyiothub-core` → `crates/core` 等 8 个目录；`sdks/plugin-sdk` → `crates/plugin-sdk`

**Interfaces:**
- Consumes: Task 2/3 的合并结果
- Produces: 短名映射表（下游所有 Task 的 Cargo.toml 编辑基准）：

| 目录 | package | [lib] name（不变） |
|---|---|---|
| crates/core | core | tinyiothub_core |
| crates/db | db | tinyiothub_storage |
| crates/runtime | runtime | tinyiothub_runtime |
| crates/web | web | tinyiothub_web |
| crates/macros | macros | tinyiothub_macros |
| crates/ai | ai | tinyiothub_ai（P3.5 再拆） |
| crates/memory | memory | tinyiothub_memory（P3.5 并入） |
| crates/plugin | plugin | tinyiothub_plugin（P2 并入 runtime） |
| crates/plugin-sdk | plugin-sdk | tinyiothub_plugin_sdk |

- [ ] **Step 1: 逐目录 git mv + Cargo.toml 编辑**

对每个目录（以 core 为例，其余照表）：

```bash
git mv crates/tinyiothub-core crates/core
# crates/core/Cargo.toml：
#   [package] name = "core"
#   新增 [lib] name = "tinyiothub_core"
```

- [ ] **Step 2: 根 Cargo.toml 与依赖键重写**

```bash
# 根 Cargo.toml [workspace.dependencies]：
#   tinyiothub-core = { path = "crates/tinyiothub-core" }  →  core = { path = "crates/core" }
#   （全表照映射改；删除 tinyiothub-plugin-sdk 的旧 sdks 路径，改为 crates/plugin-sdk）
# members 中 "sdks/*" 若为空则移除
grep -rln "tinyiothub-core = { workspace = true }" --include="Cargo.toml" . \
  | xargs sed -i '' 's/tinyiothub-core = { workspace = true }/core = { workspace = true }/'
# 对其余 8 个 crate 重复等价 sed（db/runtime/web/macros/ai/memory/plugin/plugin-sdk）
# path 依赖写法同步改：tinyiothub-core = { path = "../tinyiothub-core" } → core = { path = "../core" }
grep -rln 'path = "\.\./tinyiothub-' --include="Cargo.toml" . | xargs sed -i '' \
  -e 's#\.\./tinyiothub-core#../core#g' -e 's#\.\./tinyiothub-storage#../db#g' \
  -e 's#\.\./tinyiothub-runtime#../runtime#g' -e 's#\.\./tinyiothub-web#../web#g' \
  -e 's#\.\./tinyiothub-macros#../macros#g' -e 's#\.\./tinyiothub-ai#../ai#g' \
  -e 's#\.\./tinyiothub-memory#../memory#g' -e 's#\.\./tinyiothub-plugin#../plugin#g'
```

- [ ] **Step 3: 验证**

```bash
cargo metadata --no-deps --quiet && cargo check --workspace && cargo test --workspace
```

Expected: 全绿。⚠️ 若某 crate 的 `[lib]` 漏钉，use 语句会全部报错 —— 逐个补齐。

- [ ] **Step 4: Commit**

```bash
git add crates/ sdks/ Cargo.toml Cargo.lock cloud/Cargo.toml edge/Cargo.toml marketplace/Cargo.toml cli/Cargo.toml
git commit -m "refactor(workspace): short package names with pinned tinyiothub_* lib names (P1-Task4)"
```

---

### Task 5: P1.4 — AGENTS.md 同步（T9，防文档撒谎期）

**Files:**
- Modify: `AGENTS.md`（Dependency Direction 表、Stability Tiers 表、目录约定段）

**Interfaces:**
- Consumes: Task 4 映射表
- Produces: 与代码一致的行为准则文档（后续所有 Task 的守门依据）

- [ ] **Step 1: 更新依赖方向表**

```markdown
| Crate | Role | Forbidden |
|-------|------|-----------|
| `core` (lib tinyiothub_core) | 值类型、错误、配置。【守门】只许 trait+值类型，禁止逻辑与 I/O | 逻辑函数、I/O、DB |
| `runtime` | EventBus、DataServer、驱动框架、plugin loader、DLQ trait | 依赖 web/领域 crate |
| `db` | SQLite 具体实现（buzz 模式：平铺、无 trait 倒置） | 依赖除 core 外任何 crate |
| `web` | HTTP middleware、ApiResponseBuilder | 业务逻辑 |
```

- [ ] **Step 2: 更新 Stability Tiers 与目录约定**（crates/ 短名 + apps/ 规划说明 + core 守门条款）

- [ ] **Step 3: Commit**

```bash
git add AGENTS.md
git commit -m "docs(agents): sync crate names, core guardrail, dependency rules (P1-Task5)"
```

---

### Task 6: P2.1 — scheduler crate 成立

**Files:**
- Create: `crates/scheduler/Cargo.toml`、`crates/scheduler/src/lib.rs`
- Move: `crates/runtime/src/cron.rs` → `crates/scheduler/src/engine.rs`
- Move: `cloud/src/shared/cron_scheduler.rs` → `crates/scheduler/src/scheduler.rs`
- Modify: `cloud/src/shared/{service_manager,mod}.rs`、`cloud/src/modules/jobs/handler.rs`、`cloud/src/modules/cron/mod.rs`、`cloud/src/modules/plugin/scheduler/handlers/cron.rs`

**Interfaces:**
- Consumes: `tinyiothub_runtime::cron::*`（现引用点：上述 6 个文件）
- Produces: `tinyiothub_scheduler::{CronEngine, CronScheduler}`；repo trait 暂留 db
  （cron_job/cron_run 实现不动），具体 struct 在 Task 19（admin 抽取）定型

- [ ] **Step 1: 建 crate**

```toml
# crates/scheduler/Cargo.toml
[package]
name = "scheduler"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[lib]
name = "tinyiothub_scheduler"

[dependencies]
core = { workspace = true }
tokio = { workspace = true }
tokio-cron-scheduler = { workspace = true }
cron = { workspace = true }
tracing = { workspace = true }
async-trait = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
uuid = { workspace = true }
chrono = { workspace = true }
```

- [ ] **Step 2: 移动代码**

```bash
git mv crates/runtime/src/cron.rs crates/scheduler/src/engine.rs
git mv cloud/src/shared/cron_scheduler.rs crates/scheduler/src/scheduler.rs
# lib.rs：pub mod engine; pub mod scheduler; + 常用 re-export
# 修正两个文件内的 use 路径（crate:: → tinyiothub_scheduler::；db 类型经 core trait 或暂由参数注入）
```

- [ ] **Step 3: 重写引用**

```bash
grep -rln "tinyiothub_runtime::cron" --include="*.rs" . | xargs sed -i '' \
  's/tinyiothub_runtime::cron/tinyiothub_scheduler::engine/g'
grep -rln "cron_scheduler" --include="*.rs" cloud/src | xargs sed -i '' \
  's/crate::shared::cron_scheduler/tinyiothub_scheduler::scheduler/g'
# cloud/Cargo.toml 加 scheduler = { workspace = true }；runtime/Cargo.toml 移除不再需要时保留无妨
```

- [ ] **Step 4: 验证 + Commit**

```bash
cargo check --workspace && cargo test -p scheduler
git add crates/scheduler crates/runtime cloud/ Cargo.toml Cargo.lock
git commit -m "refactor(scheduler): extract cron engine+scheduler into scheduler crate (P2-Task6)"
```

---

### Task 7: P2.2 — plugin loader 并入 runtime::plugin + FFI 去重

**Files:**
- Move: `crates/plugin/src/{ffi,loader,registry,sandbox}.rs` → `crates/runtime/src/plugin/`
- Modify: `crates/runtime/src/lib.rs`（`pub mod plugin;`）
- Modify: `crates/plugin-sdk/src/ffi.rs`（确认为 ABI 单一事实源）
- Modify: `plugins/*/Cargo.toml`（7 个 stub，改依赖 runtime 或保持 plugin-sdk）
- Delete: `crates/plugin/`

**Interfaces:**
- Consumes: `tinyiothub_plugin::{PluginLoader, PluginRegistry, PluginFfi,...}`
- Produces: `tinyiothub_runtime::plugin::{PluginLoader, PluginRegistry, ...}`；
  FFI 类型以 `tinyiothub_plugin_sdk::ffi` 为准，runtime::plugin::ffi 仅保留 host 侧 glue

- [ ] **Step 1: 移动 + 模块组装**

```bash
mkdir -p crates/runtime/src/plugin
git mv crates/plugin/src/ffi.rs crates/runtime/src/plugin/ffi.rs
git mv crates/plugin/src/loader.rs crates/runtime/src/plugin/loader.rs
git mv crates/plugin/src/registry.rs crates/runtime/src/plugin/registry.rs
git mv crates/plugin/src/sandbox.rs crates/runtime/src/plugin/sandbox.rs
# 写 crates/runtime/src/plugin/mod.rs：pub mod ffi/loader/registry/sandbox + re-export
# runtime/Cargo.toml 加 libloading = { workspace = true }
```

- [ ] **Step 2: FFI 去重**

对照 `crates/plugin-sdk/src/ffi.rs` 与 `crates/runtime/src/plugin/ffi.rs`：
重复定义的 ABI 类型（PluginFfi/PluginInfo/PluginVersion 等）从 runtime 侧删除，
改为 `pub use tinyiothub_plugin_sdk::ffi::*;`。runtime/Cargo.toml 加
`plugin-sdk = { workspace = true }`（如尚未有）。

- [ ] **Step 3: 重写引用**

```bash
grep -rln "tinyiothub_plugin::" --include="*.rs" . | xargs sed -i '' \
  's/tinyiothub_plugin::/tinyiothub_runtime::plugin::/g'
grep -rln "^tinyiothub-plugin = \|^plugin = { workspace" --include="Cargo.toml" . \
  | xargs sed -i '' 's/^tinyiothub-plugin = .*/runtime = { workspace = true }/'
git rm -r crates/plugin
# 根 Cargo.toml [workspace.dependencies] 删 plugin 行
```

- [ ] **Step 4: 验证 + Commit**

```bash
cargo check --workspace && cargo test -p runtime
git add crates/ Cargo.toml Cargo.lock plugins/
git commit -m "refactor(runtime): merge plugin loader into runtime::plugin, dedup FFI (P2-Task7)"
```

---

### Task 8: P2.3 — shared/persistence → db（buzz 平铺）+ Dockerfile/TODOS 同步（T10）

**Files:**
- Move: `cloud/src/shared/persistence/repositories/*.rs` → `crates/db/src/`（按领域平铺：event.rs、session.rs、real_time_event.rs、driver_installation.rs、device_query.rs、notification_*.rs、device_trace.rs）
- Move: `cloud/src/shared/persistence/{database,pool,factory,config}.rs` → `crates/db/src/`
- Move: `cloud/migrations/` → `crates/db/migrations/`
- Modify: `crates/db/src/lib.rs`（pub mod 平铺声明）
- Modify: `Dockerfile:79`（cloud/migrations → crates/db/migrations）
- Modify: `TODOS.md`（#40/#41/#44 锚点 → runtime::plugin 新路径）
- Modify: `cloud/src/shared/{mod.rs,app_state.rs}`（re-export 更新）

**Interfaces:**
- Consumes: 现有 `crate::shared::persistence::*` 全部引用
- Produces: `tinyiothub_db::{Database, Pool, EventRepository, SessionRepository, ...}`
  具体 struct（无 trait 倒置；现有 core 中 repository trait 保留至 P4 逐领域评估削除）

- [ ] **Step 1: 迁移 migrations 与 sqlx 路径**

```bash
git mv cloud/migrations crates/db/migrations
grep -rn 'migrate!' crates/db/src cloud/src --include="*.rs"
# 将 migrate!("./migrations") 类宏路径改为相对 crates/db 的正确路径（migrate! 相对 CARGO_MANIFEST_DIR）
sed -i '' 's#cloud/migrations#crates/db/migrations#g' Dockerfile
docker build -t tinyiothub-reorg-check . # 验证（失败则修 COPY 源路径）
```

- [ ] **Step 2: 平铺迁移 repositories**

```bash
for f in cloud/src/shared/persistence/repositories/*.rs; do git mv "$f" crates/db/src/; done
git mv cloud/src/shared/persistence/database.rs crates/db/src/database.rs
git mv cloud/src/shared/persistence/pool.rs crates/db/src/pool.rs
git mv cloud/src/shared/persistence/factory.rs crates/db/src/factory.rs
git mv cloud/src/shared/persistence/config.rs crates/db/src/db_config.rs  # 避免与 core::config 撞名
# 合并 adapters/ 内容到对应平铺文件；删除空的 persistence/ 目录
# crates/db/src/lib.rs 平铺 pub mod + 每文件顶部 doc 注释（一行职责，参照 buzz-db 风格）
# db/Cargo.toml 补 tokio-postgres/influxdb2（若 repositories 用到）
```

- [ ] **Step 3: 重写引用**

```bash
grep -rln "shared::persistence" --include="*.rs" cloud/src | xargs sed -i '' \
  's/crate::shared::persistence/tinyiothub_db/g'
# cloud/Cargo.toml 加 db = { workspace = true }（若已有则跳过）
```

- [ ] **Step 4: TODOS 锚点更新**

`TODOS.md` 中 #40（registry.rs:48-50 → `crates/runtime/src/plugin/registry.rs`）、
#41（exporter 路径按现状核实后更新）、#44（DriverRegistry → runtime::plugin）逐条改。

- [ ] **Step 5: 验证 + Commit**

```bash
cargo check --workspace && cargo test -p db && cargo test -p tinyiothub-cloud
git add crates/db cloud/ Dockerfile TODOS.md Cargo.toml Cargo.lock
git commit -m "refactor(db): flatten persistence into db crate (buzz pattern), sync Docker/TODOS (P2-Task8)"
```

---

### Task 9: P3 — web crate 充实 + llm crate 成立

**Files:**
- Move: `cloud/src/shared/middleware/*` → `crates/web/src/middleware/`
- Move: `cloud/src/shared/{api_response.rs,error_handling.rs}` → `crates/web/src/`
- Move: `cloud/src/shared/{llm_provider.rs,ai_adapter.rs}` → `crates/llm/src/`（新建 crate）
- Move: `crates/ai/src/{prompt,session}/` → `crates/llm/src/`（类型归位）
- Modify: `crates/web/src/lib.rs`、新建 `crates/llm/{Cargo.toml,src/lib.rs}`（lib tinyiothub_llm）

**Interfaces:**
- Produces: `tinyiothub_web::{middleware, ApiResponseBuilder, ErrorHandling}`；
  `tinyiothub_llm::{LlmProvider, LlmResponse, LlmCallMetadata, prompt, session}`

- [ ] **Step 1: web 迁移 + 引用重写**

```bash
git mv cloud/src/shared/middleware crates/web/src/middleware
git mv cloud/src/shared/api_response.rs crates/web/src/api_response.rs
git mv cloud/src/shared/error_handling.rs crates/web/src/error_handling.rs
grep -rln "shared::middleware\|shared::api_response\|shared::error_handling" --include="*.rs" cloud/src \
  | xargs sed -i '' -e 's/crate::shared::middleware/tinyiothub_web::middleware/g' \
  -e 's/crate::shared::api_response/tinyiothub_web::api_response/g' \
  -e 's/crate::shared::error_handling/tinyiothub_web::error_handling/g'
```

- [ ] **Step 2: llm crate**

```bash
mkdir -p crates/llm/src
git mv cloud/src/shared/llm_provider.rs crates/llm/src/provider.rs
git mv cloud/src/shared/ai_adapter.rs crates/llm/src/adapter.rs
git mv crates/ai/src/prompt crates/llm/src/prompt
git mv crates/ai/src/session.rs crates/llm/src/session.rs 2>/dev/null || git mv crates/ai/src/session crates/llm/src/session
# Cargo.toml 按 Task 6 模板（deps: core, serde, async-trait, tracing, reqwest? 按实际 import 定）
# crates/ai 引用改为 tinyiothub_llm::（sed）；ai/Cargo.toml 加 llm 依赖
```

- [ ] **Step 3: 验证 + Commit**

```bash
cargo check --workspace && cargo test -p web -p llm
git add crates/web crates/llm crates/ai cloud/ Cargo.toml Cargo.lock
git commit -m "refactor(web,llm): consolidate HTTP infra and LLM contract crates (P3-Task9)"
```

---

### Task 10: P3.5 — memory / policy / skills 归位

**Files:**
- Move: `crates/memory/src/*` + `crates/ai/src/memory/` + `crates/ai/src/knowledge/` + `cloud/src/shared/workspace_memory.rs` → 新 `crates/memory/`（lib tinyiothub_memory；先 git mv crates/memory 暂存）
- Move: `crates/ai/src/policy/` + `crates/ai/src/proposal/` → `crates/policy/`（新建，lib tinyiothub_policy）
- Move: `crates/ai/src/{skills,tool}/` → `crates/skills/`（新建，lib tinyiothub_skills）

**Interfaces:**
- Produces: `tinyiothub_memory::{MemoryFact, KnowledgeGraph, SqliteAgentMemoryRepository, reflect}`；
  `tinyiothub_policy::{PolicyEngine, Proposal, ...}`；`tinyiothub_skills::{SkillRegistry, ToolRegistry, ...}`；
  `tinyiothub_ai` 保留 thing_agent/orchestrator/heartbeat/event/alarm(types 待 Task 18 归 alarm)/agent(types)

- [ ] **Step 1: memory 合并**

```bash
mkdir -p /tmp/mem-stage && git mv crates/memory/src /tmp/mem-stage/src
mkdir -p crates/memory/src
git mv /tmp/mem-stage/src/* crates/memory/src/
git mv crates/ai/src/memory crates/memory/src/agent_memory
git mv crates/ai/src/knowledge crates/memory/src/knowledge
git mv cloud/src/shared/workspace_memory.rs crates/memory/src/workspace_memory.rs
# lib.rs 组装 + 内部 use 修正；原 crates/ai 引用 sed 为 tinyiothub_memory::
```

- [ ] **Step 2: policy / skills 同法迁移 + 引用 sed**

- [ ] **Step 3: 验证 + Commit**

```bash
cargo check --workspace && cargo test -p memory -p policy -p skills -p ai
git add crates/ cloud/ Cargo.toml Cargo.lock
git commit -m "refactor(ai): split memory/policy/skills crates from ai (P3.5-Task10)"
```

---

### Task 11: P4.0a — 消灭 mcp AppState 单例

**Files:**
- Modify: `cloud/src/modules/mcp/mod.rs`（删 `static APP_STATE` + `get_app_state`/`init_app_state`）
- Modify: `cloud/src/modules/thing/handler/actions.rs:118,285`（改 State 萃取）
- Modify: `cloud/src/modules/mcp/` 内所有 `get_app_state()` 调用点
- Modify: `cloud/src/shared/service_manager.rs`（registry 构造注入 state）

**Interfaces:**
- Consumes: axum `State<Arc<AppState>>`（handlers 已有）
- Produces: `HandlerRegistry::new(state: Arc<AppState>)`；工具 handler 签名改为
  接收 `&AppState`；`grep get_app_state` 计数 = 0

- [ ] **Step 1: registry 持有 state**

```rust
// mcp/mod.rs — 删除 static APP_STATE / init_app_state / get_app_state
// HandlerRegistry 增加字段：
pub struct HandlerRegistry { state: Arc<crate::shared::app_state::AppState>, /* ... */ }
impl HandlerRegistry {
    pub fn new(state: Arc<crate::shared::app_state::AppState>) -> Self { /* ... */ }
    pub fn state(&self) -> &Arc<crate::shared::app_state::AppState> { &self.state }
}
```

- [ ] **Step 2: 调用点改造**

`thing/handler/actions.rs` 两处 `crate::modules::mcp::get_app_state()` 改为 handler
已有的 `State(state): State<Arc<AppState>>` 萃取值。mcp 工具内部调用改走
`registry.state()`。service_manager 中 `init_app_state(...)` 改为
`HandlerRegistry::new(app_state.clone())`。

- [ ] **Step 3: 守门验证 + Commit**

```bash
! grep -rn "get_app_state\|init_app_state" cloud/src --include="*.rs" | grep -v "registry.state()"
cargo check --workspace && cargo test -p tinyiothub-cloud
git add cloud/
git commit -m "refactor(mcp): eliminate global AppState singleton, inject via registry (P4.0-Task11)"
```

---

### Task 12: P4.0b — 斩 thing→agent/mcp 边

**Files:**
- Modify: `cloud/src/modules/thing/handler/actions.rs`（:18 import、:240、:264 调用）
- Modify: `cloud/src/modules/agent/tools/mod.rs`（agent 侧反向提供 API）

**Interfaces:**
- Consumes: Task 11 的 registry state 注入
- Produces: thing 侧只依赖 core 类型；`SqlitePolicyEngine`/`take_pending_action`/
  `validate_action_params`/`store_pending_action` 的调用改由 agent 在组合层
  （cloud/src/api 或 server.rs 路由组装处）注入为 `Arc<dyn ThingActionHooks>`
  （trait 定义在 core，属守门允许范围）

- [ ] **Step 1: core 定义 hooks trait**

```rust
// crates/core/src/thing_hooks.rs
#[async_trait::async_trait]
pub trait ThingActionHooks: Send + Sync {
    async fn validate_params(&self, schema: &serde_json::Value, params: Option<&serde_json::Value>) -> Result<(), String>;
    async fn store_pending(&self, req: &serde_json::Value) -> Result<String, String>;
    async fn take_pending(&self, token: &str) -> Result<Option<serde_json::Value>, String>;
}
```

- [ ] **Step 2: actions.rs 改注入 + agent 实现该 trait 并在路由组装处接线**

- [ ] **Step 3: 守门验证 + Commit**

```bash
! grep -rn "modules::agent\|modules::mcp" cloud/src/modules/thing/ --include="*.rs"
cargo check --workspace && cargo test -p tinyiothub-cloud
git add crates/core cloud/
git commit -m "refactor(thing): cut thing->agent/mcp edges via core hooks trait (P4.0-Task12)"
```

---

### Task 13: P4.0c — 斩 event→notification 边

**Files:**
- Modify: `cloud/src/modules/event/mod.rs:94`（删 notification re-export）
- Modify: `cloud/src/modules/event/service.rs:354`（参数类型）
- Modify: `crates/core/src/`（NotificationChannelType/NotificationAggregate 下沉或 event 侧自定义）

**Interfaces:**
- Produces: `core::notification_types::{NotificationChannelType, NotificationAggregateRef}`
  （若下沉）；单向 `notify→event` 保持

- [ ] **Step 1: 类型下沉 core（纯值类型，守门允许）**

```bash
# 将 notification/types.rs 中 NotificationChannelType（及被 event 引用的聚合类型）
# git mv 到 crates/core/src/notification_types.rs；notification 侧 re-export 兼容
```

- [ ] **Step 2: event 侧改引用 core + 验证 + Commit**

```bash
grep -rln "modules::notification" cloud/src/modules/event --include="*.rs" \
  | xargs sed -i '' 's#crate::modules::notification::types::NotificationChannelType#tinyiothub_core::notification_types::NotificationChannelType#g'
# service.rs:354 NotificationAggregate 同法处理（或改为 event 侧自有的规则视图类型）
! grep -rn "modules::notification" cloud/src/modules/event --include="*.rs"
cargo check --workspace && cargo test -p tinyiothub-cloud
git add crates/core cloud/
git commit -m "refactor(event): cut event->notification edge, sink types to core (P4.0-Task13)"
```

---

### Task 14: P4.0d — 斩 workspace→agent 边（保留 agent→tenant 单向）

**Files:**
- Modify: `cloud/src/modules/workspace/{service.rs,handler/mod.rs,handler/heartbeat.rs}`

**Interfaces:**
- Consumes: Task 12 的 hooks 模式
- Produces: `core::agent_hooks::AgentHooks`（trait）→ agent 实现，组合层注入 workspace；
  `grep "modules::agent" cloud/src/modules/workspace` = 0；agent→workspace 单向保留
  （P4 时 agent→tenant crate）

- [ ] **Step 1: 确认 workspace 对 agent 的 3 处调用语义**（service/handler/heartbeat 各为何调用 agent——
  启动 agent？查询状态？），定义最小 trait 面

- [ ] **Step 2: 同 Task 12 模式改造 + 守门验证 + Commit**

```bash
! grep -rn "modules::agent" cloud/src/modules/workspace --include="*.rs"
cargo check --workspace && cargo test -p tinyiothub-cloud
git add crates/core cloud/
git commit -m "refactor(workspace): cut workspace->agent edges via hooks trait (P4.0-Task14)"
```

---

### Task 15: P4 试点 — thing crate 抽取（定型标准抽取程序 SEP）

**Files:**
- Create: `crates/thing/{Cargo.toml,src/lib.rs,src/types.rs,src/service.rs,src/handler/,src/repo.rs}`
- Move: `cloud/src/modules/thing/*` → `crates/thing/src/`
- Move: `cloud/src/modules/{template,tag}/` → `crates/thing/src/{template,tag}/`
- Move: 旧 device 管理面 `cloud/src/modules/device/{diagnostics,trace,monitoring,performance,query,types}.rs` → `crates/thing/src/legacy/`
- Move: `crates/db/src/` 中 thing 相关 repository → 定型 `crates/db/src/thing.rs`（buzz 平铺）
- Move: `cloud/templates/` → `crates/thing/templates/`（若 fs 运行时读取；同步 Dockerfile:80）
- Modify: `.github/workflows/ci.yml`（守卫路径 cloud/src/modules → crates 适配）
- Modify: `apps 组合层`（暂为 cloud/src/server.rs + api/）挂载 `thing::router()`

**Interfaces:**
- Consumes: Task 4 映射、Task 12 斩边结果、`tinyiothub_db::Database`
- Produces（后续所有领域 crate 的 SEP 模板）:
  - `thing::router() -> Router<ThingState>`
  - `ThingState { db: Arc<tinyiothub_db::Database>, /* 其他切片 */ }`，
    `impl FromRef<AppState> for ThingState`（组合层总 AppState）
  - `crates/db/src/thing.rs` 具体 `ThingRepository`

- [ ] **Step 1: 建 crate（Cargo.toml 模板）**

```toml
[package]
name = "thing"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[lib]
name = "tinyiothub_thing"

[dependencies]
core = { workspace = true }
db = { workspace = true }
web = { workspace = true }
llm = { workspace = true }          # 物画像 LLM 摘要
axum = { workspace = true }
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
async-trait = { workspace = true }
tracing = { workspace = true }
uuid = { workspace = true }
chrono = { workspace = true }
thiserror = { workspace = true }
validator = { workspace = true }
```

- [ ] **Step 2: 迁移代码**

```bash
mkdir -p crates/thing/src/legacy
git mv cloud/src/modules/thing/types.rs crates/thing/src/types.rs
git mv cloud/src/modules/thing/repo.rs crates/thing/src/repo.rs
git mv cloud/src/modules/thing/service crates/thing/src/service
git mv cloud/src/modules/thing/handler crates/thing/src/handler
git mv cloud/src/modules/thing/errors.rs crates/thing/src/errors.rs
git mv cloud/src/modules/thing/summary.rs crates/thing/src/summary.rs
git mv cloud/src/modules/template crates/thing/src/template
git mv cloud/src/modules/tag crates/thing/src/tag 2>/dev/null || true
git mv cloud/src/modules/device/diagnostics.rs crates/thing/src/legacy/  # 及其余 legacy 面文件
# 每个文件内 use crate::modules:: 路径改为目标 crate 路径（sed 逐模式）
```

- [ ] **Step 3: ThingState + FromRef + router()**

```rust
// crates/thing/src/lib.rs
pub mod types; pub mod service; pub mod handler; pub mod repo; pub mod errors;
pub mod template; pub mod legacy;

#[derive(Clone)]
pub struct ThingState { pub db: std::sync::Arc<tinyiothub_db::Database> }

pub fn router() -> axum::Router<ThingState> { handler::router() }
```

```rust
// cloud/src/shared/app_state.rs — 组合层
impl axum::extract::FromRef<AppState> for tinyiothub_thing::ThingState {
    fn from_ref(state: &AppState) -> Self {
        tinyiothub_thing::ThingState { db: state.db.clone() }
    }
}
```

- [ ] **Step 4: db 平铺定型 + Dockerfile/ci.yml 同步**

```bash
# crates/db/src/thing.rs：合并 thing 相关 repository 为具体 struct（buzz 风格 doc 头）
git mv cloud/templates crates/thing/templates 2>/dev/null || true
sed -i '' 's#cloud/templates#crates/thing/templates#g' Dockerfile
# ci.yml: cloud/src/modules 守卫 grep 改为兼容 crates/ 新布局（规则同义改写）
```

- [ ] **Step 5: 试点判据全量验证**

```bash
cargo test -p thing                                          # 判据1 独立测试绿
! grep -rn "get_app_state" crates/thing/src                  # 判据2 无单例
! grep -rn "tinyiothub_agent\|tinyiothub_mcp" crates/thing/src  # 判据3 无反向 import
cargo check -p tinyiothub-cloud                              # 判据4 组合层 FromRef 编译
grep -c "" crates/db/src/thing.rs                            # 判据5 db 平铺成型（<2k 行）
cargo test --workspace                                       # 全量绿
docker build -t tinyiothub-reorg-check .                     # Dockerfile 同步有效
```

- [ ] **Step 6: Commit**

```bash
git add crates/thing crates/db cloud/ Dockerfile .github/
git commit -m "refactor(thing): extract thing domain crate (pilot, SEP established) (P4-Task15)"
```

---

### Task 16: P4 — auth crate 抽取（SEP 应用 #1）

**Files:**
- Move: `cloud/src/modules/auth/` → `crates/auth/src/`
- Move: `cloud/src/shared/security/`（auth 相关 extractors/JWT）→ `crates/auth/src/security/`
- Create: `crates/auth/Cargo.toml`（lib tinyiothub_auth；deps: core, db, web, axum, jwt-simple, bcrypt, ...）

**Interfaces:**
- Consumes: Task 15 SEP
- Produces: `AuthState` + `FromRef<AppState>` + `auth::router()`；
  `tinyiothub_auth::{JwtService, AuthExtractor}` 供其他领域 crate 复用（web middleware 引用其类型）

- [ ] **Step 1-4: 按 SEP 执行**（git mv 上述清单 → use sed → AuthState/FromRef/router → db 平铺 auth.rs）
  注意偏差：shared/security 只迁 auth 相关部分，workspace 级 scope 校验留 cloud（Task 25 tenant 再迁）。

- [ ] **Step 5: 验证（判据同 Task 15 Step 5，crate 名替换）+ ci.yml 守卫同步**

- [ ] **Step 6: Commit**

```bash
git add crates/auth crates/db cloud/ .github/
git commit -m "refactor(auth): extract auth domain crate (P4-Task16)"
```

---

### Task 17: P4 — user + tenant 抽取（SEP 应用 #2/#3，同 PR 或顺序两 PR）

**Files:**
- Move: `cloud/src/modules/{user,role,permission}/` → `crates/user/src/`
- Move: `cloud/src/modules/{tenant,workspace}/` → `crates/tenant/src/`
- Create: 两个 Cargo.toml（lib tinyiothub_user / tinyiothub_tenant）

**Interfaces:**
- Produces: `UserState`/`TenantState` + router()；user→tenant 单向边
  （`grep "modules::tenant\|modules::workspace" crates/user` 只允许指向 tinyiothub_tenant；
  若成环按预案合并为一个 identity crate —— 先尝试拆分，环出现在编译期立即可见）

- [ ] **Step 1-6: 按 SEP 执行**（user 先行，tenant 次之；workspace 的 knowledge 资源相关
  repo 归 thing 还是 tenant 按 import 实测归属，记入 commit message）
  验证同 Task 15 判据。Commit 分两个：`refactor(user): ... (P4-Task17a)` / `refactor(tenant): ... (P4-Task17b)`

---

### Task 18: P4 — event crate 抽取（SEP 应用 #4）

**Files:**
- Move: `cloud/src/modules/event/` → `crates/event/src/`
- Move: `crates/ai/src/alarm/`（26 行类型）→ `crates/alarm/src/types_ai.rs` 暂存（Task 19 用）
- Modify: 传输层接 runtime EventBus（确认 `modules/event/bus.rs` 与 `runtime/event_bus.rs`
  关系：保留领域 bus 语义、传输委托 runtime，去重写进 commit message）

**Interfaces:**
- Produces: `EventState` + router()；`tinyiothub_event::{EventService, EventRepository(from db)}`；
  单向 alarm→event、notify→event、agent→event 的承载者

- [ ] **Step 1-6: 按 SEP 执行 + Commit** `refactor(event): extract event domain crate (P4-Task18)`

---

### Task 19: P4 — alarm crate 抽取（SEP 应用 #5）

**Files:**
- Move: `cloud/src/modules/alarm/` → `crates/alarm/src/`
- Move: Task 18 暂存的 `crates/alarm/src/types_ai.rs` 归位
- Move: `cloud/src/modules/event/router.rs` 中 alarm 相关路由 → `crates/alarm/src/handler/`

**Interfaces:**
- Consumes: `tinyiothub_event::EventService`（依赖 event，单向）
- Produces: `AlarmState` + router()；alarm→event 单向实现

- [ ] **Step 1-6: 按 SEP 执行 + Commit** `refactor(alarm): extract alarm domain crate (P4-Task19)`

---

### Task 20: P4 — driver crate 抽取（SEP 应用 #6）

**Files:**
- Move: `cloud/src/modules/{drivers,driver_health,gateway,plugin,heartbeat}/` → `crates/driver/src/`
- Move: `cloud/src/modules/device/driver.rs`（re-export 壳）→ 删除，直接用 runtime

**Interfaces:**
- Consumes: `tinyiothub_runtime::{driver, plugin}`、`tinyiothub_thing`（写数据，单向 driver→thing）
- Produces: `DriverState` + router()；设备 heartbeat 服务

- [ ] **Step 1-6: 按 SEP 执行 + Commit** `refactor(driver): extract driver/access domain crate (P4-Task20)`

---

### Task 21: P4 — notify crate 抽取（SEP 应用 #7）

**Files:**
- Move: `cloud/src/modules/notification/` → `crates/notify/src/`

**Interfaces:**
- Consumes: `tinyiothub_event`（单向 notify→event）、`core::notification_types`（Task 13 下沉）
- Produces: `NotifyState` + router()

- [ ] **Step 1-6: 按 SEP 执行 + Commit** `refactor(notify): extract notify domain crate (P4-Task21)`

---

### Task 22: P4 — agent crate 抽取（三合一大成，SEP 应用 #8）

**Files:**
- Move: `cloud/src/modules/agent/` → `crates/agent/src/host/`
- Move: `cloud/src/modules/chat/` → `crates/agent/src/chat/`
- Move: `cloud/src/shared/agent/` → `crates/agent/src/host/shared/`
- Move: `crates/ai/src/{thing_agent,orchestrator,heartbeat,event,agent}/` → `crates/agent/src/loop/`
- Delete: `crates/ai/`（清空后移除；workspace glob 自动处理）

**Interfaces:**
- Consumes: `tinyiothub_{llm,memory,policy,skills,event,thing,tenant,db,runtime}`
- Produces: `AgentState` + router()；`tinyiothub_agent::loop_::{ThingAgentRunner, Orchestrator}`；
  内部模块 loop/host/chat 三层隔离（host→loop 单向，loop 不依赖 web）

- [ ] **Step 1-6: 按 SEP 执行**，附加判据：`cargo tree -p agent | grep axum` 只出现在 host 路径。
  Commit `refactor(agent): unify agent loop+host+chat into agent crate (P4-Task22)`

---

### Task 23: P4 — mcp crate 抽取（SEP 应用 #9）

**Files:**
- Move: `cloud/src/modules/mcp/` → `crates/mcp/src/`

**Interfaces:**
- Consumes: Task 11 的 HandlerRegistry(state) 形态；mcp→{alarm,agent} 单向
- Produces: `McpState` + router()（含 /mcp 端点挂载函数）

- [ ] **Step 1-6: 按 SEP 执行 + Commit** `refactor(mcp): extract mcp domain crate (P4-Task23)`

---

### Task 24: P4 — admin crate 抽取（SEP 应用 #10，最后一块）

**Files:**
- Move: `cloud/src/modules/{system,monitoring,batch,jobs,open,cron}/` → `crates/admin/src/`
- Modify: jobs/调度 API 接 `tinyiothub_scheduler`（admin→scheduler）

**Interfaces:**
- Produces: `AdminState` + router()；cloud/src/modules/ 清空（仅剩 mod.rs 壳后删除）

- [ ] **Step 1-6: 按 SEP 执行 + Commit** `refactor(admin): extract admin domain crate, modules/ emptied (P4-Task24)`

---

### Task 25: P5.1 — apps/ 归位 + cloud 薄壳化

**Files:**
- Move: `cloud/` → `apps/cloud/`（git mv 整个目录）
- Move: `edge/` → `apps/edge/`、`marketplace/` → `apps/marketplace/`、`cli/` → `apps/cli/`
- Modify: 根 `Cargo.toml` members（"cloud","edge","marketplace","cli" → "apps/*"）
- Modify: `apps/cloud/src/main.rs`（<200 行：读配置 → 建 AppState → 组装各 router → serve）
- Modify: `apps/cloud/Cargo.toml`（解除 cloud→marketplace 依赖；marketplace 逻辑若被引用，
  下沉为 crates 内 client 模块或经 HTTP 调用，按 import 实测定）
- Modify: `Dockerfile`、`Dockerfile.dev`、`.github/workflows/*.yml`、`deploy/docker/*`、
  `scripts/build-static.sh`（cloud/ → apps/cloud/ 路径全量替换）

**Interfaces:**
- Produces: 根 members = `["crates/*", "apps/*", "tests/integration"]`；`cargo build --bin tinyiothub-cloud` 不变

- [ ] **Step 1: git mv + members 更新 + cargo check**
- [ ] **Step 2: main.rs 薄壳化**（server.rs 逻辑拆为 `apps/cloud/src/bootstrap.rs`，main 只做组装）
- [ ] **Step 3: CI/CD 全量路径替换 + 验证**

```bash
grep -rln "cloud/" Dockerfile Dockerfile.dev .github/ deploy/ scripts/build-static.sh \
  | xargs sed -i '' 's#cloud/#apps/cloud/#g'
# 例外：crates/ 内引用不受影响；web/ 前端代理路径不受影响
docker build -t tinyiothub-reorg-check .
```

- [ ] **Step 4: Commit** `refactor(apps): relocate deployables to apps/, thin cloud main (P5-Task25)`

---

### Task 26: P5.2 — plugins/cli 移出 workspace + E2E 验收门

**Files:**
- Modify: 根 `Cargo.toml`（members 删 `plugins/*`；`cli` 已从 apps/* glob 覆盖则无需处理，
  否则显式排除 apps/cli）
- Move: `plugins/` → `drivers/`（git mv；不再列入 members）
- Run: T19 E2E 验收脚本（唯一 E2E 门）

**Interfaces:**
- Produces: `cargo metadata` 不再含 7 个空壳；E2E 全绿记录

- [ ] **Step 1: 移出 + metadata 验证**

```bash
git mv plugins drivers
# 根 Cargo.toml members 删 "plugins/*"；[workspace.dependencies] 无 plugins 残留
cargo metadata --no-deps --quiet | python3 -c "import json,sys; d=json.load(sys.stdin); print(len(d['packages']))"
```

- [ ] **Step 2: E2E 验收**

Run: T19 E2E 脚本（路径见 cloud/src/tests 或 scripts/ 中 T19 提交 12681a45 引入的验收脚本；
若路径随 apps/ 移动变更，先按新位置定位再执行）
Expected: 全绿。**红则阻断**：按 phase 二分定位（git bisect P4 各 PR），修复后重跑。

- [ ] **Step 3: Commit** `chore(workspace): drop stub members, drivers/ layout, E2E gate passed (P5-Task26)`

---

### Task 27: P6 — 文档收官

**Files:**
- Modify: `AGENTS.md`（最终校对）、`CLAUDE.md`（hot paths 更新）、`README.md`（项目结构树）
- Modify: `docs/superpowers/specs/2026-08-03-crates-reorg-design.md`（§9 checkbox 全勾）

**Interfaces:**
- Produces: 文档与代码最终一致

- [ ] **Step 1: README 结构树按最终 crates/ + apps/ + drivers/ 重写**
- [ ] **Step 2: AGENTS.md 校对（目录约定、依赖方向、稳定性层级与现状一致）**
- [ ] **Step 3: Commit** `docs: finalize reorg documentation (P6-Task27)`

---

## Self-Review 记录

- **Spec 覆盖**：设计文档 P0-P6 全部 phase 映射到 Task 1-27；§9 T1-T11 全部映射
  （T1→Task1, T2→Task11, T3→Task12, T4→Task13, T5→Task5, T6→Task8/25, T7→Task15,
  T8→Task14 关联, T9→Task5, T10→Task8, T11→Task15-24 Step4）。
- **Placeholder 扫描**：P4 各领域的具体 use-sed 模式留给 executor 按实际 grep 结果生成
  （属可执行程序而非占位）；无 TBD/TODO 项。
- **类型一致性**：lib 名全局统一 tinyiothub_*；State 命名 `<Domain>State` 全局统一；
  hooks trait 命名 `core::{thing_hooks,agent_hooks}` 全局统一。
