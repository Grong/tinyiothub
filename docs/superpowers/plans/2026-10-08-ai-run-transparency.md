# AI 运行透明化 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 打开 AI 处理过程黑盒——巡检/处置链路的逐步工具轨迹实时可见（卡片轮询）+ 事后完整可查（详情页）+ 终端用户大白话（narrative）。

**Architecture:** trace 随 RunReport/HeartbeatResult 落库（不加表）；进行中轨迹经进程内 ActiveRunRegistry + 只读端点 3s 轮询；run_id 经新 bus 事件 RunStarted 由 cloud 订阅者回填 judgment；evidence 存 trace_run_id 引用（单源）；前端卡片展开区统一为时间线组件，详情页时间线为主视觉。

**Tech Stack:** Rust (axum/tokio/sqlx/DashMap) + Vite/Lit/signals + SQLite 迁移。

**Spec:** `docs/superpowers/specs/2026-10-08-ai-run-transparency-design.md`（含三轮评审全部裁定）。

## Global Constraints

- **web 构建/测试需 Node 24**：先 `export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"`（vite/vitest 在 Node 18 必败）。
- **DTO 契约**：后端 `#[serde(rename_all = "camelCase")]`，前端类型直接写 camelCase（case-converter 运行时/类型级不对称先例）。
- **F11 证据契约**：trace 全量入库不截断（`agent_runs.report` 列=唯一全量副本）；evidence 只存 `trace_run_id` 引用。
- **本地门禁**：`cargo clippy --workspace --all-targets -- -D warnings`（`let _ = <future>` 会被拒，detached spawn 用 `drop(...)`）；web 改动跑 `npx tsc --noEmit`（vite build 不查类型）。
- **迁移**：走 DDL 守卫加白的重建表模式；视图重建必须配逆迁移。
- **提示词改动必须跑 eval 基线**（≥7a2b5f87 的 91.7%）。
- **narrative 协议**：协议层必填、解析层 `Option<String>`；≤200 字、只能在 verdict JSON 块内、禁块外散文。
- **推送**：git push 走 gh token extraheader + HTTP/1.1（SSH/代理截断先例）。
- 设计：暗色主题（--bg #12141a）；时间线用新建 `--timeline-*` 视图作用域令牌；禁 role=button 套行；trace 一律纯文本渲染（不走 md()）。

---

### Task 1: ToolTraceEntry 挪 core + RunReport.trace

**Files:**
- Modify: `crates/core/src/agent_runs.rs`（RunReport :104 区域）
- Modify: `crates/agent/src/runtime/thing_agent/runner.rs`（:57-64 删本地定义，:446 assemble_report 搬入）
- Test: `crates/agent/src/runtime/thing_agent/runner.rs` 既有测试模块

**Interfaces:**
- Produces: `tinyiothub_core::agent_runs::ToolTraceEntry { id: String, name: String, args: serde_json::Value, output: Option<String> }`（Serialize+Deserialize+Clone+Debug）；`RunReport.trace: Vec<ToolTraceEntry>`（`#[serde(default)]`）。

- [ ] **Step 1: 写失败测试**（runner.rs 测试模块加）

```rust
#[test]
fn report_carries_full_trace() {
    // 用既有 entry() helper（runner.rs:549）构造两条轨迹，assemble 后断言
    let mut ctx = RunContextInner::default();
    ctx.trace.push(entry("c1", "read_property", serde_json::json!({"p":"temperature"}), Some("26.5")));
    ctx.trace.push(entry("c2", "query_events", serde_json::json!({}), None));
    let report = assemble_report("r1", "ws1", &ctx, /* 其余参数按现有签名 */);
    assert_eq!(report.trace.len(), 2);
    assert_eq!(report.trace[1].output, None);
}
```

- [ ] **Step 2: 跑测试确认 FAIL**（`report` 无 `trace` 字段编译错）

Run: `cargo test -p tinyiothub-agent report_carries_full_trace`
Expected: FAIL（compile error: no field `trace`）

- [ ] **Step 3: 实现**——`agent_runs.rs` 加：

```rust
/// 一次工具调用轨迹（runner.rs 迁入；output=None = 配对 ToolResult 未到）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolTraceEntry {
    pub id: String,
    pub name: String,
    pub args: serde_json::Value,
    pub output: Option<String>,
}
```

`RunReport` 追加 `#[serde(default)] pub trace: Vec<ToolTraceEntry>`。runner.rs 删本地 `ToolTraceEntry` 定义改 `use tinyiothub_core::agent_runs::ToolTraceEntry;`，`assemble_report` 加 `trace: ctx.trace.clone()`。

- [ ] **Step 4: 跑测试 PASS + 向后兼容**（旧 report JSON 无 trace 字段能反序列化：`serde_json::from_str::<RunReport>(旧JSON)` 成功且 trace 为空）

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/agent_runs.rs crates/agent/src/runtime/thing_agent/runner.rs
git commit -m "feat(core): RunReport 携带完整工具轨迹——ToolTraceEntry 下放 core"
```

---

### Task 2: 迁移——judgments 加 narrative 列 + brain_events 视图重建

**Files:**
- Create: `crates/db/migrations/20261008000001_judgments_narrative.sql`
- Create: `crates/db/migrations/20261008000002_brain_events_tick_id.sql`（含逆迁移注释/配套）
- Test: `crates/db/src/judgment.rs` / `brain_event.rs` 测试模块

**Interfaces:**
- Produces: `judgments.narrative TEXT NULL`；brain_events 视图 patrol 提案行投影 `tick_id`（tick 聚合行保持 NULL）。

- [ ] **Step 1: 写失败测试**（brain_event.rs 测试模块）

```rust
#[tokio::test]
async fn proposal_row_projects_tick_id() {
    let pool = test_pool().await;
    // 插 tick anchor（tick_id=t1）+ 提案行（tick_id=t1，Task 9 才写——此处手工 INSERT 模拟）
    // 查 list_brain_events，断言提案行 tick_id == Some("t1")、tick 聚合行 tick_id 为 NULL
}
```

- [ ] **Step 2: 跑测试确认 FAIL**（视图无 tick_id 列）

- [ ] **Step 3: 实现迁移**——① `ALTER TABLE judgments ADD COLUMN narrative TEXT;` ② 视图重建走加白模式：`CREATE TABLE brain_events_new ... INSERT INTO ... SELECT ... DROP/ALTER` 或 `DROP VIEW + CREATE VIEW`（按 `20260919000001_brain_events_view_fix.sql` 先例），提案行加 `a.tick_id AS tick_id`，tick 聚合行 `NULL AS tick_id`；同文件注释给出逆迁移 SQL。

- [ ] **Step 4: 迁移测试 PASS** + judgments 旧行 narrative 为 NULL 断言

- [ ] **Step 5: Commit**

```bash
git add crates/db/migrations/ crates/db/src/
git commit -m "feat(db): judgments+narrative 列 + brain_events 视图提案行投影 tick_id（含逆迁移）"
```

---

### Task 3: ActiveRunRegistry

**Files:**
- Create: `crates/agent/src/runtime/thing_agent/active_runs.rs`
- Modify: `crates/agent/src/runtime/thing_agent/runner.rs`（execute 注册/注销）
- Modify: `crates/agent/src/runtime/thing_agent/mod.rs`（导出）
- Test: `active_runs.rs` 内测试模块

**Interfaces:**
- Produces: `ActiveRunRegistry::new(max: usize)`；`register(&self, ctx: RunContext) -> Option<ActiveRunGuard>`（超限返回 None 并 `tracing::warn!(metric="active_run_registry_full")`）；`get(&self, run_id: &str) -> Option<RunContext>`；`ActiveRunGuard: Drop`（注销）。上限常量与 scheduler 并发容量同源（复用/同值 `TRIGGER_CHANNEL_CAPACITY` 所在常量区）。
- Consumes: `RunContext`（runner.rs:91）。

- [ ] **Step 1: 写失败测试**

```rust
#[tokio::test]
async fn register_visible_then_guard_drop_removes() {
    let reg = ActiveRunRegistry::new(2);
    let ctx = RunContext::new("r1".into(), "ws1".into(), "t".into());
    {
        let _g = reg.register(ctx.clone()).expect("容量内");
        assert!(reg.get("r1").is_some());
    } // guard drop
    assert!(reg.get("r1").is_none());
}

#[tokio::test]
async fn over_capacity_refuses_with_warn() {
    let reg = ActiveRunRegistry::new(1);
    let _g1 = reg.register(RunContext::new("r1".into(), "ws".into(), "t".into()));
    assert!(reg.register(RunContext::new("r2".into(), "ws".into(), "t".into())).is_none());
}
```

- [ ] **Step 2: FAIL**（类型不存在）

- [ ] **Step 3: 实现**——DashMap 包 `RunContext`，guard 持 `Arc` 引用 + run_id，Drop 时 remove；**注册点统一在 `run_pipeline`（manager 侧）而非 execute 内部**——只有 manager 能保"先注册后发 RunStarted"的时序（T4）：run_pipeline 创建 ctx 后 `let _guard = ctx.active_runs.as_ref().and_then(|r| r.register(ctx.clone()));`，guard 生命周期横跨整个 execute 调用。**本计划选定：RunContext 加 `pub active_runs: Option<ActiveRunRegistry>`**（`RunContext::new` 加参，默认 None 不影响既有构造点）。

- [ ] **Step 4: PASS**（含 cancel 路径：既有 cancel 测试断言注销发生）

- [ ] **Step 5: Commit** `feat(agent): ActiveRunRegistry——进行中 run 的内存注册表（RAII+有界）`

---

### Task 4: RunStarted 事件 + cloud 订阅扇入回填

**Files:**
- Modify: `crates/agent/src/runtime/events.rs`（加变体）
- Modify: `crates/agent/src/runtime/thing_agent/manager.rs`（run_pipeline :278 发出）
- Modify: `apps/cloud/src/domains/agent/host/judgment_subscriber.rs`（订阅分支）
- Test: judgment_subscriber.rs 测试模块（既有 fixture 复用）

**Interfaces:**
- Produces: `AgentEventKind::RunStarted { run_id: String, workspace_id: String, problem_keys: Vec<String> }`；提取函数 `fn collect_problem_keys(source: &TriggerSource) -> Vec<String>`（递归 Merged.signals，收集 UserDirective.problem_key 的 Some，去重）。
- **时序约定**：**manager 先注册 registry 再发 RunStarted**（注册点已在 T3 定于 run_pipeline；execute 内部不注册）。

- [ ] **Step 1: 写失败测试**（judgment_subscriber.rs）

```rust
#[tokio::test]
async fn run_started_backfills_investigating_judgment() {
    // 既有 fixture：insert_judgment("ws1", Some("a1"), None, Some("t1"), "annotate")
    // project(&RunStarted{run_id:"r9", problem_keys:["alarm:t1:rule1"]}, ...) 后
    // 断言 judgment.run_id == Some("r9")
}

#[tokio::test]
async fn run_started_matches_at_most_one_per_problem_key() {
    // 同 problem_key 只可能一个 investigating（flapping 防抖不变量）——断言只回填它
}
```

manager 侧测试：Merged{signals:[ThingEvent, UserDirective{problem_key:Some("k1")}, UserDirective{problem_key:Some("k1")}]} → `collect_problem_keys` == `["k1"]`。

- [ ] **Step 2: FAIL**

- [ ] **Step 3: 实现**——events.rs 加变体；manager.rs `run_pipeline` 在 ctx 创建后：

```rust
if let Some(reg) = &ctx.inner_active_runs { reg.register(ctx.clone()); } // 先注册
deps.bus.emit(AgentEventKind::RunStarted {
    run_id: run_id.clone(),
    workspace_id: ws.clone(),
    problem_keys: collect_problem_keys(&signal.source),
}); // 后发事件
```

judgment_subscriber `project` 加 `AgentEventKind::RunStarted` 分支：对每个 problem_key 解析 thing+rule（problem_key 格式 `alarm:{thing}:{rule}`——按既有 `find_investigating_judgment_by_thing_rule` 的键构造），调 `db.set_judgment_run_id`（幂等 NULL 守卫，失败 warn 不传播）。

- [ ] **Step 4: PASS**（含"结束后既有回填幂等兜底"双时点断言：RunStarted 已填则 RunRecorded 路径 no-op）

- [ ] **Step 5: Commit** `feat(agent): RunStarted 事件——run 启动即回填 judgment.run_id（先注册后发）`

---

### Task 5: trace 端点 + JudgmentDto run_id + registry 接线

**Files:**
- Create: `apps/cloud/src/domains/agent/trace_handler.rs`（或在 agent 域 handler 新增子模块）
- Modify: `apps/cloud/src/domains/judgment/dto.rs`（加 `run_id: Option<String>`）
- Modify: `apps/cloud/src/domains/brain_event/handler/mod.rs`（挂路由）
- Modify: thing_agent_host / AppState（registry 与 bus 同路径注入）
- Modify: `crates/db/src/judgment.rs`（列表查询投影加 run_id）
- Test: trace_handler 测试 + dto 断言

**Interfaces:**
- Produces: `GET /api/v1/agent-runs/{run_id}/trace`（workspace 成员鉴权，同 /brain-events 模式）→ `{ live: bool, budgetMax: u32, entries: [{ id, name, args, output: string|null }] }`；404 未命中两源。`JudgmentDto.runId`。
- Consumes: `ActiveRunRegistry`（Task 3）、`agent_runs.report`（Task 1 的 trace 字段）、`MAX_TOOL_CALLS_PER_RUN`（runner.rs:27 pub）。

- [ ] **Step 1: 写失败测试**（handler 级三分支）

```rust
#[tokio::test]
async fn trace_live_from_registry() { /* registry 放一条 → 200 live:true */ }
#[tokio::test]
async fn trace_ended_from_report() { /* agent_runs 插 report 带 trace → 200 live:false */ }
#[tokio::test]
async fn trace_unknown_404() { /* 都不存在 → 404 */ }
```

- [ ] **Step 2: FAIL**（路由不存在）

- [ ] **Step 3: 实现**——handler：先 `registry.get(run_id)`（`read().await` 后 **立即 clone trace + drop guard，不持 guard 跨 await**），命中返回 live:true；未命中查 agent_runs.report（serde 反序列化整个 report 取 trace，**不写 SQL JSON 函数**）；都没有 404。`budgetMax` 两分支同值=常量。DTO 加字段（camelCase 自动）。接线：thing_agent_host 构造 registry 存入 AppState，同时传给每个 RunContext。

- [ ] **Step 4: PASS + dto 测试**（列表响应含 runId）

- [ ] **Step 5: Commit** `feat(cloud): trace 端点三分支 + JudgmentDto.runId 断面 + registry 接线`

---

### Task 6: narrative 解析落列 + evidence 引用式 + detail 透出

**Files:**
- Modify: `apps/cloud/src/domains/agent/host/judgment_subscriber.rs`（VerdictPayload :56、evidence 组装 :428）
- Modify: `crates/db/src/judgment.rs`（narrative 列读写）
- Modify: `crates/db/src/brain_event.rs`（detail 对 judgment 类从 report 取 trace 透出）
- Test: judgment_subscriber.rs / brain_event.rs 测试模块

**Interfaces:**
- Produces: `VerdictPayload.narrative: Option<String>`；`judgments.narrative` 列；evidence_json 增 `trace_run_id`；`BrainEventDetail`（judgment 类）透出 `trace: Vec<ToolTraceEntry>`（从 `agent_runs.report` 取）。
- Consumes: Task 1（trace 字段）、Task 2（narrative 列）。

- [ ] **Step 1: 写失败测试**

```rust
#[tokio::test]
async fn verdict_with_narrative_persisted() { /* verdict JSON 带 narrative → judgments.narrative 落列 */ }
#[tokio::test]
async fn verdict_without_narrative_fallback_ok() { /* 老协议无 narrative → parse 成功、列 NULL（F3 回归护栏）*/ }
#[tokio::test]
async fn evidence_stores_reference_not_copy() { /* evidence.trace_run_id == run_id；evidence 无 trace 键；扇入 2 judgment 时零副本 */ }
```

- [ ] **Step 2: FAIL**

- [ ] **Step 3: 实现**——VerdictPayload 加 `#[serde(default)] pub narrative: Option<String>`；`update_judgment_verdict`（judgment.rs:300 区域）写 narrative 列；evidence 组装点写 `"trace_run_id": report.run_id`（**不写 trace**）；brain_event detail：judgment 类事件按 evidence.trace_run_id（或 judgment.run_id）读 agent_runs.report 取 trace 放进 detail 响应。

- [ ] **Step 4: PASS**

- [ ] **Step 5: Commit** `feat(agent): narrative 落列（Option 容错）+ evidence 引用式 trace（消扇入副本）`

---

### Task 7: 提示词 narrative 协议 + eval 基线

**Files:**
- Modify: 提示词模块（`19a1ebac` 收敛后的唯一模块；verdict 协议段 + 宪法段）
- Modify: eval 夹具（verdict 协议断言处）
- Test: eval 套件

- [ ] **Step 1: eval 夹具加 narrative 断言**（带 narrative 的 verdict 样例 → 解析出；缺 narrative → 不炸且标 fallback）
- [ ] **Step 2: FAIL/红**
- [ ] **Step 3: 提示词加协议说明**：narrative 必填、≤200 字、第一人称大白话（"我检查了…"）、只能在 ```json verdict 块内、块外禁止散文
- [ ] **Step 4: 跑 eval 基线 ≥ 91.7%**（低于基线则调提示词，不动解析）
- [ ] **Step 5: Commit** `feat(prompt): verdict 协议加 narrative（必填+防爆约束），eval 基线守绿`

---

### Task 8: notify 双触发点带 narrative

**Files:**
- Modify: `apps/cloud/src/domains/agent/host/judgment_subscriber.rs`（`spawn_needs_you_notify` :565 与 `escalate_to_ticket` 通知路径）
- Test: 既有 X1 notify 测试扩展

**Interfaces:**
- Consumes: Task 6 的 narrative 列。

- [ ] **Step 1: 写失败测试**——两触发点（awaiting_approval / escalated）payload 均含 narrative；narrative NULL 时降级 reason
- [ ] **Step 2: FAIL**
- [ ] **Step 3: 实现**——notify payload 构造处取 `narrative.unwrap_or(reason)`
- [ ] **Step 4: PASS**
- [ ] **Step 5: Commit** `feat(agent): notify 双触发点携带 narrative（NULL 降级 reason）`

---

### Task 9: HeartbeatRunner trace 仪表化 + 提案行写 tick_id

**Files:**
- Modify: `crates/core/src/heartbeat.rs`（HeartbeatResult 加 trace）
- Modify: `crates/agent/src/runtime/heartbeat/runner.rs`（记录 trace）
- Modify: `crates/db/src/heartbeat.rs`（`insert_action_row` 加 tick_id 参数；tick anchor content 加 trace；`insert_heartbeat_result` 传 tick_id）
- Test: heartbeat runner 测试 + db 测试；**OV#4 手工镜像 fixture 同步加 trace 字段**

**Interfaces:**
- Produces: `HeartbeatResult.trace: Vec<ToolTraceEntry>`（`#[serde(default)]`）；agent_actions 提案/auto_executed 行的 `tick_id` 列有值；tick anchor content JSON 含 `trace`。
- Consumes: Task 1 的 ToolTraceEntry。

- [ ] **Step 1: 写失败测试**

```rust
#[tokio::test]
async fn proposal_row_carries_tick_id() { /* insert_heartbeat_result 后：提案行 tick_id == result.id；anchor content 含 trace */ }
```

- [ ] **Step 2: FAIL**
- [ ] **Step 3: 实现**——HeartbeatRunner 工具循环按 ToolTraceEntry 记录；HeartbeatResult 加字段；`insert_action_row(..., tick_id: Option<&str>)` 签名扩展（所有调用点传入 `result.id`）；anchor content json 加 `trace`。
- [ ] **Step 4: PASS + fixture 同步**
- [ ] **Step 5: Commit** `feat(agent): HeartbeatRunner trace 仪表化——提案行写 tick_id、anchor 带 trace`

---

### Task 10: brain-events detail 按 tick_id join anchor + patrol 提案卡片内嵌时间线

**Files:**
- Modify: `crates/db/src/brain_event.rs`（detail 查询扩展）
- Modify: `web/src/ui/views/dispositions.ts`（patrol 提案可展开=内嵌完成态时间线）
- Test: brain_event.rs 测试 + dispositions.test.ts

**Interfaces:**
- Produces: `BrainEventDetail`（patrol 提案类）透出 `trace`（按行 tick_id join 同 workspace tick anchor content.trace）。
- Consumes: Task 2（视图 tick_id）、Task 9（anchor trace）、Task 11（时间线组件）——**实现顺序：本任务 db 部分可在 Task 11 前做，前端部分在 Task 11 后**。

- [ ] **Step 1: 写失败测试**（detail 对提案行返回 anchor 的 trace；无 anchor/tick_id 时 trace 为空不炸）
- [ ] **Step 2: FAIL**
- [ ] **Step 3: db 实现**——detail 查询：提案行有 tick_id → 查同 workspace anchor 行 content 取 trace 透出
- [ ] **Step 4: db PASS**
- [ ] **Step 5: 前端**——patrol 提案行（有 tick_id）`clickable`，展开=时间线组件完成态模式（数据来自 detail.trace）；tick 聚合行保持跳 `#/ai-ops`（不链接，设计 D10 边界）
- [ ] **Step 6: vitest 两态 PASS**
- [ ] **Step 7: Commit** `feat(web,db): patrol 提案卡片内嵌 tick 轨迹时间线（detail 按 tick_id join anchor）`

---

### Task 11: trace API 客户端 + 时间线组件（含 --timeline-* 令牌层）

**Files:**
- Create: `web/src/api/agent-runs.ts`
- Create: `web/src/ui/shared/run-timeline.ts` + `run-timeline.css`
- Test: `web/src/api/agent-runs.test.ts`、`web/src/ui/shared/run-timeline.test.ts`

**Interfaces:**
- Produces: `fetchRunTrace(runId): Promise<{ live: boolean; budgetMax: number; entries: TraceEntry[] }>`（404 抛 `TraceNotFoundError`）；`renderRunTimeline({ entries, live, budgetMax, mode: 'live'|'full' })` Lit 组件——props 控制详略（live=卡片精简行/full=详情全量可展开 JSON）；`--timeline-done/--timeline-active/--timeline-connector/--timeline-gap` 令牌定义在组件根。
- Consumes: Task 5 端点。

- [ ] **Step 1: 写失败测试**——api 客户端（unwrap + camelCase 类型直接写）+ 组件渲染（done 步 ✓/进行中步 spinner/output null/预算头"n / budgetMax"/n 略超不截断）
- [ ] **Step 2: FAIL**
- [ ] **Step 3: 实现**——令牌层初始值取既有调色板；spinner 带动画但**无脉冲点**；trace 内容一律纯文本节点（不插 HTML）
- [ ] **Step 4: PASS（`npx tsc --noEmit` 也过）**
- [ ] **Step 5: Commit** `feat(web): trace API 客户端 + run-timeline 组件（--timeline-* 令牌层，双模式）`

---

### Task 12: 卡片层改造（统一时间线展开区）

**Files:**
- Modify: `web/src/ui/views/dispositions.ts` / `.css` / `.test.ts`
- Consumes: Task 5（runId 断面）、Task 11（组件）

**Interfaces:**
- Consumes: `BrainEvent.runId`（列表投影——**注意**：investigating 卡片数据源的列表投影需带 run_id；若 brain_events 列表行的 judgment 分支不带，需随 Task 2 视图一并投影，实现时先验证再补）。

- [ ] **Step 1: 写失败测试**——investigating 可展开/轮询启停/代际守卫（快速展开收起+刷新不串数据）/404 有限重试 3 次后"轨迹不可用"/runId NULL 显示"排队中"无展开/完成态变完成时间线+narrative+链接/Esc 收起/aria-live 只在步数变化播报
- [ ] **Step 2: FAIL**
- [ ] **Step 3: 实现**——展开区=run-timeline（live 模式，max-height≈300px 内滚）；轮询 3s + generation counter + AbortController；**证据面板从卡片移除**（ verdict/think 由详情页承接）；焦点不抢夺；预算头接入 budgetMax
- [ ] **Step 4: PASS + tsc**
- [ ] **Step 5: Commit** `feat(web): 处置中心卡片展开区统一为进度时间线——实时轮询+代际守卫+预算条`

---

### Task 13: run 详情页

**Files:**
- Create: `web/src/ui/views/run-detail.ts` + `.css` + `.test.ts`
- Modify: `web/src/ui/app.ts`（路由 `#/ai-ops/runs/{id}`）
- Consumes: Task 11（组件）、Task 6/10（detail 透出）

- [ ] **Step 1: 写失败测试**——层级（元信息条→时间线主视觉→verdict 折叠→think 折叠）/三态（骨架屏/404"该运行记录不存在或已过期"+返回/空 trace"本次运行未调用工具"）/导出按钮写剪贴板/纯文本渲染断言
- [ ] **Step 2: FAIL**
- [ ] **Step 3: 实现**——数据源：trace 端点 404 时退 `/brain-events/{id}` detail；进行中继续轮询（复用代际守卫模式）；导出=组装 {meta, entries} JSON `navigator.clipboard.writeText`
- [ ] **Step 4: PASS + tsc**
- [ ] **Step 5: Commit** `feat(web): run 详情页——时间线主视觉+三态+导出 JSON`

---

## 依赖与执行顺序

```
T1(core) ──┬─► T3(registry) ──► T5(端点+DTO) ─┐
           ├─► T4(RunStarted) ────────────────┤
T2(迁移) ──┴─► T6(narrative/evidence) ─► T7(prompt/eval) ─► T8(notify)
T9(heartbeat trace) ─► T10(patrol join+内嵌) ─┐
T11(web 组件) ─► T12(卡片层) ─► T13(详情页) ◄──┘
```

并行可行：T7/T8 与 T9 互不依赖；T11 只依赖 T5。验收：每个任务 commit 前 `cargo test -p <crate>` + 全量门禁在最后一个任务后跑（`cargo clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace` + `npx tsc --noEmit` + `npm test`）。
