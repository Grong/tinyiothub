# Crates 重组 G 系列（G3–G9）Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 relay 范式（buzz-relay, 2026-08）已定型的前提下，把与参照架构 buzz 的执行层差距全部闭合：authn 纯化、反向边类型级切断、全局状态收官、AppState 治理、comment-as-policy 补齐。行为零变化。

**Architecture:** 接续 `docs/superpowers/plans/2026-08-03-crates-reorg.md`（P0–P6 已完成至 F7/G2）。范式不变：`apps/cloud` 拥有全部行为，`crates/*` 是能力库（不编排、不 import apps），`core` 只放值类型。本计划只消除"运行时切断了但类型层/依赖层没切断"的半成品。

**参照架构（buzz）：**
- `/Users/chenguorong/code/github/buzz/ARCHITECTURE.md` — 四层依赖层级
- `crates/buzz-core/Cargo.toml` — "NO tokio, NO sqlx, NO redis, NO axum" 注释即政策
- `crates/buzz-media/` — transport-free 库（无 Axum，handler 住 relay）
- `crates/buzz-auth/` + `crates/buzz-pubsub/` — trait 住拥有方 crate（auth 定义 `RateLimiter`，pubsub 提供 `RedisRateLimiter`，relay 接线）
- `crates/buzz-relay/src/state.rs` — 组合根 AppState
- `crates/buzz-conformance/Cargo.toml` — 依赖限制即设计的注释范例

**差距清单（2026-08-14 实测，审查记录见本文件 §差距对照）：**
1. `crates/authn` 自称"纯机制"但 jwt.rs 含 axum `FromRequestParts` extractor 且依赖 `web`；Cargo.toml 残留 8 个未使用依赖（db/sqlx/hex/async-trait/serde_json/subtle/tokio/tracing）。
2. 反向边 `thing→agent`（thing/handler/actions.rs:3 import `agent::host::thing_action_hooks::ThingConfirmVerdict`）与 `tenant→agent` 只靠 AppState 注入在运行时切断；`ThingActionHooks`/`AgentHooks` trait 尚不存在，AppState 字段 naming 具体类型 `AgentThingActionHooks`/`AgentHooksImpl`（app_state.rs:158,163）。
3. 残留全局：`CONFIG` OnceLock（`config::get()` 共 9 文件 28 处调用）；`MCP_REGISTRY`（已注释 sanction，保留）。
4. AppState 45 字段 / 1107 行；`FromRef` 切片只覆盖 driver/alarm/tenant 三域，admin/mcp/agent handler 仍直接吃 `State<AppState>`。
5. AGENTS.md repo map（`state.rs`/`router.rs`）超前于代码（`shared/app_state.rs`/`server.rs`/`api/mod.rs`）。
6. 根 `Cargo.toml` workspace.dependencies 有悬挂 `tenant = { path = "crates/tenant" }`（目录不存在）。
7. comment-as-policy 不系统：仅 authn doc header、MCP_REGISTRY 注释两例。

## Global Constraints

1. **继承 P 计划全部约束**：行为零变化；每 Task 一个 commit（格式 `<type>(<scope>): <desc> (GN)`）；禁止 `git add -A`；每 Task 结束 `cargo check --workspace` 绿 + 受影响 `cargo test -p` 绿；执行分支 `refactor/crates-reorg`。
2. **依赖方向不变**：`apps/* → crates/* → core`。G 系列不新增跨 crate 边，只删边或移动类型归属。web→authn（G4 新增）合法：web 是 HTTP infra 层，authn 是纯机制层。
3. **core 守门条款不变**：core 只许 trait + 值类型。G5 若把 hooks trait 放 core，trait 定义合法，impl 必须在 agent。
4. **trait 位置规则（对齐 buzz）**：trait 定义住消费抽象的域（或 core），impl 住提供实现的域，AppState 只持 `Arc<dyn Trait>`。AppState 字段禁止 naming 任何 domains 下的具体类型。
5. **comment-as-policy**：每个 G task 落地时，把该 task 守护的规则写成注释放在执行点（Cargo.toml 依赖块或 lib.rs `//!` 头部），不只写进文档。
6. **测试**：`apps/cloud/src/tests/`（39 文件）+ `tests/integration` 受影响必跑；G3 提交前全量 `cargo test --workspace`。

---

### Task 1: G3 收尾 — WIP 验证 + 提交

**Files:**
- Modify: 当前未提交的 20 个文件（`shared/app_state.rs`、`shared/service_manager.rs`、`main.rs`、`server.rs`、`domains/driver/heartbeat/*`、`domains/admin/monitoring/handler/{health,metrics}.rs`、`domains/mcp/*`、`domains/agent/host/*`、test 更新）

**Interfaces:**
- Consumes: 现有 WIP（AppState 已增 `started_at`/`pending_actions`/`driver_heartbeat_status`/`driver_heartbeat_config` 字段，标注"G3，替代 …全局静态"）
- Produces: 心跳状态/pending actions/started_at 全部经 AppState 注入，对应全局静态删除

- [ ] **Step 1: 验证 WIP 完整性** — 确认被删全局无残留引用：`grep -rn "static ref\|lazy_static\|OnceLock" apps/cloud/src/domains/driver apps/cloud/src/domains/admin/monitoring apps/cloud/src/domains/agent/host`
- [ ] **Step 2: 门禁** — `cargo check --workspace && cargo test --workspace` 全绿
- [ ] **Step 3: Commit** `refactor(cloud): sink heartbeat/pending-action globals into AppState (G3)`

---

### Task 2: G4 — authn 纯化（兑现"纯机制"承诺）

**Files:**
- Move: `crates/authn/src/jwt.rs` 中 `FromRequestParts` extractor impl 与 `AuthBody`/`WebClaims` 桥接 → `crates/web/src/security/`（web 已拥有 `AuthBody`/`Claims`，extractor 是传输层胶水，buzz 范式下属 relay/web 侧）
- Modify: `crates/authn/Cargo.toml`（删依赖 + 加政策注释）、`crates/web/Cargo.toml`（加 authn 依赖）、extractor 的调用方 import（cloud 侧约 33 文件，多为 `Claims` 提取，路径 sed）

**Interfaces:**
- Consumes: G2 的 `JwtService::new(JwtSettings)` 构造注入（不变）
- Produces: authn 依赖收敛为纯机制集（jwt-simple/bcrypt/hmac/sha2/base64/dashmap/uuid/chrono/serde）；web 新增 `web → authn` 边；cloud handler 的 `Claims` extractor 来源从 authn 改为 web

- [ ] **Step 1: 移动 extractor** — jwt.rs 中 `impl<S> FromRequestParts<S> for Claims`（含 `Arc<JwtService>: FromRef<S>` 约束与 Bearer 解析）git mv 到 `crates/web/src/security/jwt_extractor.rs`；web 侧 `use tinyiothub_authn::JwtService`；web/Cargo.toml 加 `authn = { workspace = true }`
- [ ] **Step 2: authn 脱钩** — authn/Cargo.toml 删 `axum, headers, web, db, sqlx, hex, async-trait, serde_json, subtle, tokio, tracing` 及实测未用的 `core`；保留 jwt-simple/bcrypt/hmac/sha2/base64/dashmap/uuid/chrono/serde
- [ ] **Step 3: 调用方 sed** — cloud 内 `use tinyiothub_authn::Claims` → `use tinyiothub_web::security::Claims`（按 grep 实际清单逐个改）；`cargo check --workspace` 绿
- [ ] **Step 4: 政策注释** — authn/Cargo.toml 依赖块上方加：
  ```toml
  # 纯机制 crate：签发/校验/哈希。禁止 axum/sqlx/db/tokio 依赖（G4 裁决）。
  # extractor 与 HTTP 胶水乡 crates/web；业务查询住 apps/cloud。
  ```
  authn/src/lib.rs `//!` 头部同步此 invariant
- [ ] **Step 5: 门禁 + Commit** `refactor(authn): pure mechanism crate — extractor to web, drop 11 unused deps (G4)`

---

### Task 3: G5a — 斩 thing→agent 反向边（类型级）

**Files:**
- Create: `apps/cloud/src/domains/thing/hooks.rs` — `trait ThingActionHooks`（`take_pending`/`validate_params`/`store_pending` 等，签名从 thing/handler/actions.rs 的实际调用归纳）+ `ThingConfirmVerdict` 等关联类型
- Modify: `domains/agent/host/thing_action_hooks.rs`（`AgentThingActionHooks impl ThingActionHooks`，doc 注释指向真实 trait 路径——当前 lib doc 引用的 trait 不存在）、`shared/app_state.rs:158`（字段改 `Arc<dyn ThingActionHooks>`）、`domains/thing/handler/actions.rs`（import 改向 `crate::domains::thing::hooks`）

**Interfaces:**
- Produces: thing 只依赖自有 trait；agent→thing 单向边（impl 依赖 trait 定义）；AppState 不再 naming `AgentThingActionHooks`

- [ ] **Step 1: 归纳 trait** — 从 actions.rs 的 5 处调用提取方法集 + 关联类型（`ThingConfirmVerdict`、`PendingThingAction` 视图），定义 `domains/thing/hooks.rs`。值类型若被 thing/agent 共用，按 core 守门条款放 `crates/core`
- [ ] **Step 2: impl + 接线** — agent 侧 `impl ThingActionHooks for AgentThingActionHooks`；AppState 字段与构造（app_state.rs:477-484）改 `Arc<dyn>`
- [ ] **Step 3: 门禁 + Commit** `refactor(thing): cut thing→agent edge at type level via hooks trait (G5a)`

---

### Task 4: G5b — 斩 tenant→agent 反向边（类型级）

**Files:**
- Create: `apps/cloud/src/domains/tenant/hooks.rs` — `trait AgentHooks`（从 tenant 对 `agent_hooks` 的 2 处实际调用归纳）
- Modify: `domains/agent/host/agent_hooks.rs`（`AgentHooksImpl impl AgentHooks`）、`shared/app_state.rs:163`（改 `Arc<dyn AgentHooks>`）、tenant 侧调用 import

**Interfaces:**
- Produces: tenant 只依赖自有 trait；AppState 不再 naming `AgentHooksImpl`

- [ ] **Step 1-3:** 同 G5a 三步程序（trait 归纳 → impl + 接线 → 门禁）
- [ ] **Step 4: Commit** `refactor(tenant): cut tenant→agent edge at type level via hooks trait (G5b)`
- [ ] **Step 5: 守卫 grep 入 CI** — `! grep -rn "domains::agent" apps/cloud/src/domains/thing apps/cloud/src/domains/tenant` 加入 CI 守卫脚本（与 P 计划既有守卫同机制）

---

### Task 5: G6 — CONFIG 全局收官（9 文件 28 处）

**Files:**
- Modify: `shared/config/mod.rs`（保留加载与类型，删 `config::get()` 全局访问器或标注 `#[deprecated]` 过渡）、9 个 `config::get()` 调用文件（清单：`grep -rln "config::get()" apps/cloud/src`）

**Interfaces:**
- Consumes: AppState 已有的 config slices 字段
- Produces: 配置经 AppState/构造参数注入；`CONFIG` OnceLock 仅在 `main.rs` 启动期写入一次后不再被读取（或彻底改为局部变量传递）

- [ ] **Step 1: 逐文件替换** — 每文件一个小步：`config::get()` → `state.config()`（或对应 slice）。调用点在 handler 的从 `State` 取；在 service 的改构造参数
- [ ] **Step 2: 删访问器** — 全量 grep 归零后删 `get()`；`shared/config/mod.rs:11` 的 OnceLock 若只剩启动写入，改为 `main.rs` 局部 `ApplicationSettings` 传入 `AppState::new`
- [ ] **Step 3: 政策注释** — config/mod.rs 头部加"禁止新增进程级配置全局（G6 裁决）；配置经 AppState 注入"
- [ ] **Step 4: 门禁 + Commit**（可按域拆 2-3 个 commit，均带 `(G6)` 后缀）

---

### Task 6: G7 — AppState 治理 + 文件命名对齐 AGENTS.md

**Files:**
- Move: `shared/app_state.rs` → `src/state.rs`；`server.rs` → `src/router.rs`（git mv；`api/mod.rs` 路由聚合并入 router.rs 或保持嵌套，执行时按 diff 最小裁决）
- Modify: admin/mcp/agent 域 handler（`State<AppState>` → `State<S> where <Domain>State: FromRef<S>`，按 driver/alarm/tenant 已成例的程序）

**Interfaces:**
- Consumes: driver/alarm/tenant 的 `<Domain>State + FromRef` 成例
- Produces: 代码与 AGENTS.md repo map 一致；admin/mcp/agent 不再直接依赖 45 字段全集

- [ ] **Step 1: 重命名先行** — git mv + use 路径修正，独立 commit：`refactor(cloud): rename app_state.rs→state.rs, server.rs→router.rs per AGENTS.md (G7)`。机械变更与行为变更分离
- [ ] **Step 2: AdminState/TenantState 式切片推广** — 每域一个 commit：定义 `<Domain>State` 结构 + `FromRef<AppState>` impl + handler 泛型化。顺序：admin（16 处 app_state import，最大消费者）→ mcp（7）→ agent（7）
- [ ] **Step 3: 政策注释** — state.rs 头部加"新增域必须走 `<Domain>State + FromRef` 切片；禁止 handler 直接吃 `State<AppState>`（G7 裁决）"
- [ ] **Step 4: 门禁 + 各域 Commit**（`(G7)` 后缀）

---

### Task 7: G8 — comment-as-policy + workspace 卫生

**Files:**
- Modify: 根 `Cargo.toml`（删悬挂 `tenant = { path = "crates/tenant" }`）
- Modify: 12 个 crates/*/Cargo.toml 依赖块 + src/lib.rs 头部（政策注释，模板见下）

**Interfaces:**
- Produces: 每条架构规则在执行点可见；buzz 同款 comment-as-policy

- [ ] **Step 1: 删悬挂 tenant dep**
- [ ] **Step 2: 逐 crate 注释**，模板（按 crate 角色填）：
  ```toml
  # 角色:<能力库|纯机制|值类型>;禁止依赖:<清单>;规则出处:G 系列计划 Task N
  ```
  重点 crate：core（禁 I/O，已有口头规则落成文字）、db（buzz 模式：具体 struct 无 trait 倒置）、runtime（EventBus/driver 框架，不编排）、authn（G4 注释已在 Task 2 落地）
- [ ] **Step 3: lib.rs `//!` invariant 块** — 参照 buzz-auth/buzz-pubsub 的头部风格，每 crate 3-8 行：职责、invariant、依赖规则
- [ ] **Step 4: [可选] deny.toml** — 引入 cargo-deny 做依赖审计（独立 commit，不阻塞本 task 收尾）
- [ ] **Step 5: Commit** `docs(crates): comment-as-policy at enforcement points + workspace hygiene (G8)`

---

### Task 8: G9 — agent/ 域内部瘦身（评估后执行）

**前置:** G5a/G5b 完成后评估。本 task 先出评估报告再动手，不预设拆分方案。

**评估对象（2026-08-14 实测）：**
- `domains/agent/` 20,840 行（占 cloud 25%）；`loop_/orchestrator/callbacks.rs` 1,379 行、`host/tools/thing.rs` 1,250 行、`loop_/thing_agent/scheduler.rs` 1,017 行

- [ ] **Step 1: 评估报告** — loop_/host/chat 三子域的实际耦合边、大文件的职责清单；判断按文件拆分（轻）还是按子域拆模块边界（重）
- [ ] **Step 2: 用户裁决后执行** — 拆分方案经确认再实施，遵循"一文件一职责 <2k 行"的 db 平铺先例
- [ ] **Step 3: Commit**（`(G9)` 后缀）

---

## 范围外但需确认（另立 task，不进本计划）

- **apps/marketplace（999 行独立服务）与 cloud `domains/marketplace` 两套实现并存** — 需确认是历史遗留还是有意为之（客户端/服务端分工）。若是遗留，另开 cleanup task。
- buzz 的 release profile 实践（`[profile.ci]`/`[profile.sprig]`、shippable 产物独立版本）— 等 cloud 有独立部署物需求时再引入，当前不花创新 token。

## 差距对照（审查原始记录，2026-08-14）

| 维度 | buzz | tinyiothub 现状 | 闭合 task |
|---|---|---|---|
| 库 crate 无传输层 | buzz-media 不带 Axum | authn 含 axum extractor + 11 个冗余依赖 | G4 |
| 反向边 | 类型层不存在 | thing→agent、tenant→agent 运行时切断但类型层命名具体类型 | G5a/G5b |
| 全局状态 | 全部构造注入 | CONFIG 9 文件 28 处；MCP_REGISTRY（保留） | G6 |
| 组合根 | state.rs ~8 服务 | AppState 45 字段；FromRef 仅 3 域 | G7 |
| 文档同步 | ARCHITECTURE.md 与代码同步 | AGENTS.md 超前（state.rs/router.rs 不存在） | G7 Step 1 |
| workspace 卫生 | 无悬挂项 | 悬挂 tenant dep | G8 |
| comment-as-policy | 系统性 | 2 例 | G8（+各 task Step 内嵌） |
| core 纯度 / 测试 / 错误处理 | — | 已相当 | 无需动作 |

## Self-Review 记录

- **决策来源**：2026-08-14 plan-eng-review，用户裁决采纳完整路线 G3–G9（选项 A）。参照 buzz 全量 crate 审查（26 crates 清单与四层规则已核对原文）。
- **Placeholder 扫描**：G5a/G5b 的 trait 方法集留给 executor 按实际调用点归纳（属可执行程序而非占位）；G9 明确"先评估后执行"，不设预设方案。无 TBD。
- **风险**：G4 的 extractor 迁移触及 33 文件的 `Claims` import，sed 必须按 grep 清单逐个核对（`Claims` 在 web 与 authn 中同名，防止误替换）；G7 Step 2 的 handler 泛型化需逐域跑对应 src/tests 集成测试。
- **独立性**：Task 1→2→{3,4}→5→6→7→8 可按序执行；G5a/G5b 互不依赖可并行；G6/G7 无依赖但都做 AppState，建议串行避免 rebase 冲突。
