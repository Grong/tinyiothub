# AI 大脑 P0「让大脑可见」Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** brain_events 只读投影（SQL view UNION 三源）+ /brain-events 端点 + 处置中心进化 AI 工作台，用户第一次看到 AI 的全部工作。

**Architecture:** P0 不建物理表不双写——SQL view 查询时 UNION judgments ∪ agent_actions ∪ agent_runs，状态改名映射在视图层。零漂移、零镜像事务。前端处置中心改读新端点，变更操作仍走既有端点（alarm→/judgments，patrol→/heartbeat/approvals）。

**Tech Stack:** Rust + axum + sqlx(sqlite) / Vite + Lit + 自有设计系统 token（web/DESIGN_SYSTEM.md）

**Spec:** `~/.gstack/projects/Grong-tinyiothub/chenguorong-fix-dispositions-ui-design-20260917-103834.md`（v3，eng+design 评审双 CLEAR）

## Global Constraints

- 迁移落地后必须 `touch crates/db/src/lib.rs` 强制 sqlx 重嵌（learning: sqlx-migrate-stale-embed）
- 投影 id 前缀分域：`alarm:{judgment.id}` / `patrol:{proposalId}` / `directive:{run_id}` / `tick:{ws}:{hour}`
- 状态改名只发生在视图层：`noise_archived`→`self_closed`；patrol `rejected`→`dismissed`；其余原名
- 列表查询**不选 evidence_json**；证据由 detail 端点按 id 单独取（性能评审已定）
- 「需要你」SQL：`status='awaiting_approval' OR (status='escalated' AND (ticket_id IS NULL OR tickets.state='open'))`
- directive 事件分类口径：`problem_key IS NULL AND trigger_type='user'`（实测：现有 run 全部带 problem_key；chat 指令 run 无——Task 2 有验证步骤，若翻车改为排除 `alarm:`/`exec:`/`dispatch_`/`invoke_` 等已知前缀）
- risk 一律过 normalize（小写匹配 low/medium/high，否则 NULL）
- 前端沿用设计系统 token，单色纪律：颜色只出现在「需要你」

## File Structure

- Create: `crates/db/migrations/20260917000001_brain_events_view.sql` — brain_events 视图
- Create: `crates/db/src/brain_event.rs` — 投影查询（feed/summary/detail）+ 类型
- Modify: `crates/db/src/lib.rs` — 注册模块（顺带完成 touch 强制重嵌）
- Create: `apps/cloud/src/domains/brain_event/mod.rs` + `handler/mod.rs` + `dto.rs` — /brain-events 域（与 judgment 域同构）
- Modify: `apps/cloud/src/api/mod.rs` — 路由注册
- Create: `web/src/api/brain-events.ts` — 前端 API client（仿 judgments.ts）
- Modify: `web/src/ui/views/dispositions.ts` — 进化工作台（tab 计数/徽章/来源过滤/点行展开/状态表）
- Modify: `web/src/ui/views/dispositions.css` — 工作台样式（复用现有 + 徽章/聚合行）
- Modify: `web/src/ui/app.ts` — 导航与标题改「AI 工作台」
- Test: `crates/db/src/brain_event.rs` 内 #[cfg(test)] + `apps/cloud/src/tests/brain_event_tests.rs` + `web/src/ui/views/dispositions.test.ts`

---

### Task 1: brain_events SQL 视图（迁移）

**Files:**
- Create: `crates/db/migrations/20260917000001_brain_events_view.sql`
- Modify: `crates/db/src/lib.rs`（touch 强制 sqlx 重嵌——只加一行注释即可）
- Test: `crates/db/tests/migration_replay_test.rs`（既有重放测试自动覆盖；新增断言见下）

**Interfaces:**
- Produces: 视图 `brain_events`，列：id, workspace_id, source, alarm_id, thing_id, title, verdict, reason, suggested_action, action_category, action_params, risk, run_id, ticket_id, status, triage_mode, created_at, judged_at, state_entered_at, resolved_at, evidence_json

- [ ] **Step 1: 写失败测试——视图存在且三源各出一行**

在 `crates/db/tests/` 新建 `brain_events_view_test.rs`：

```rust
#[sqlx::test]
async fn brain_events_view_unions_three_sources(pool: sqlx::SqlitePool) {
    tinyiothub_storage::test_helpers::run_all_migrations(&pool).await.unwrap();
    // 各插一行：judgment / agent_actions(proposal) / agent_runs(指令 run)
    // （用最小 NOT NULL 列插行；judgment 列参照 judgments 表 schema）
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM brain_events")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(n, 3);
    // 状态改名映射在视图层生效
    let (status,): (String,) = sqlx::query_as(
        "SELECT status FROM brain_events WHERE source='alarm'")
        .fetch_one(&pool).await.unwrap();
    // judgments.status='noise_archived' 的行应映射为 self_closed
    assert_eq!(status, "self_closed");
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p db brain_events_view` （注意先 `touch crates/db/src/lib.rs`）
Expected: FAIL——`no such table: brain_events`

- [ ] **Step 3: 写迁移**

`crates/db/migrations/20260917000001_brain_events_view.sql`：

```sql
-- brain_events 只读投影（P0 不建物理表不双写）：
-- UNION judgments ∪ agent_actions(提案) ∪ agent_runs(指令) ∪ agent_actions(tick 聚合)。
-- 状态改名只在本层：noise_archived→self_closed；patrol rejected→dismissed。
CREATE VIEW brain_events AS
SELECT
  'alarm:' || j.id AS id,
  j.workspace_id,
  'alarm' AS source,
  j.alarm_id,
  j.thing_id,
  COALESCE((SELECT a.alarm_message FROM thing_alarms a WHERE a.id = j.alarm_id), '') AS title,
  j.verdict,
  j.reason,
  j.suggested_action,
  j.action_category,
  NULL AS action_params,
  (SELECT CASE a.alarm_level WHEN 'critical' THEN 'high' WHEN 'error' THEN 'high'
          WHEN 'warning' THEN 'medium' ELSE 'low' END
   FROM thing_alarms a WHERE a.id = j.alarm_id) AS risk,
  j.run_id,
  j.ticket_id,
  CASE j.status WHEN 'noise_archived' THEN 'self_closed' ELSE j.status END AS status,
  j.triage_mode,
  j.created_at,
  j.judged_at,
  j.state_entered_at,
  j.resolved_at,
  j.evidence_json
FROM judgments j

UNION ALL

-- patrol 提案（agent_actions JSON 行；每提案一行，不是一个 tick 一行）
SELECT
  'patrol:' || json_extract(a.content, '$.proposalId') AS id,
  a.workspace_id,
  'patrol' AS source,
  NULL AS alarm_id,
  json_extract(a.content, '$.thingId') AS thing_id,
  COALESCE(json_extract(a.content, '$.summary'), '') AS title,
  NULL AS verdict,
  COALESCE(json_extract(a.content, '$.reason'), '') AS reason,
  json_extract(a.content, '$.toolName') AS suggested_action,
  NULL AS action_category,
  json_extract(a.content, '$.parameters') AS action_params,
  CASE lower(COALESCE(json_extract(a.content, '$.risk'), ''))
       WHEN 'low' THEN 'low' WHEN 'medium' THEN 'medium' WHEN 'high' THEN 'high'
       ELSE NULL END AS risk,
  NULL AS run_id,
  NULL AS ticket_id,
  CASE json_extract(a.content, '$.status')
       WHEN 'pending' THEN 'awaiting_approval'
       WHEN 'approved' THEN 'resolved'
       WHEN 'rejected' THEN 'dismissed'
       ELSE 'awaiting_approval' END AS status,
  'annotate' AS triage_mode,
  a.created_at,
  NULL AS judged_at,          -- patrol 无 24h SLA（judged_at=NULL 不被清扫命中）
  a.created_at AS state_entered_at,
  NULL AS resolved_at,
  -- 证据：同 tick 锚行（action_type='summary'）内容 + 本提案 JSON
  COALESCE((SELECT s.content FROM agent_actions s
            WHERE s.workspace_id = a.workspace_id AND s.tick_id = a.tick_id
              AND s.action_type = 'summary' LIMIT 1), '{}') AS evidence_json
FROM agent_actions a
WHERE a.action_type = 'proposal'

UNION ALL

-- directive：chat 用户指令 run（problem_key IS NULL 且 trigger_type='user'）
SELECT
  'directive:' || r.id AS id,
  r.workspace_id,
  'directive' AS source,
  NULL AS alarm_id,
  json_extract(r.report, '$.thing_id') AS thing_id,
  COALESCE(NULLIF(json_extract(r.report, '$.trigger'), ''), substr(r.summary, 1, 80)) AS title,
  NULL AS verdict,
  COALESCE(substr(r.summary, 1, 200), '') AS reason,
  NULL AS suggested_action,
  NULL AS action_category,
  NULL AS action_params,
  NULL AS risk,
  r.id AS run_id,
  NULL AS ticket_id,
  CASE r.outcome WHEN 'failed' THEN 'investigation_failed' ELSE 'resolved' END AS status,
  'annotate' AS triage_mode,
  r.created_at,
  r.created_at AS judged_at,
  r.created_at AS state_entered_at,
  r.created_at AS resolved_at,
  r.report AS evidence_json      -- RunReport 全文即完整审计
FROM agent_runs r
WHERE r.problem_key IS NULL AND r.trigger_type = 'user'

UNION ALL

-- tick 聚合行（每小时一条，「AI 活着且查了没事」的负证据）
SELECT
  'tick:' || a.workspace_id || ':' || strftime('%Y-%m-%dT%H', a.created_at) AS id,
  a.workspace_id,
  'patrol_tick' AS source,
  NULL, NULL,
  '巡检正常 · 本小时 ' || COUNT(DISTINCT a.tick_id) || ' 次巡检无发现' AS title,
  NULL, '', NULL, NULL, NULL, NULL, NULL,
  'patrol_ok' AS status,
  'annotate',
  strftime('%Y-%m-%dT%H:00:00', a.created_at) AS created_at,
  NULL, strftime('%Y-%m-%dT%H:00:00', a.created_at), NULL,
  '{}' AS evidence_json
FROM agent_actions a
WHERE a.action_type = 'summary'
GROUP BY a.workspace_id, strftime('%Y-%m-%dT%H', a.created_at);
```

- [ ] **Step 4: 跑测试确认通过**

Run: `touch crates/db/src/lib.rs && cargo test -p db brain_events_view`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/db/migrations/20260917000001_brain_events_view.sql crates/db/src/lib.rs crates/db/tests/brain_events_view_test.rs
git commit -m "feat(db): brain_events 只读投影视图——UNION judgments/proposals/runs/tick 聚合"
```

---

### Task 2: 投影查询模块（feed/summary/detail）

**Files:**
- Create: `crates/db/src/brain_event.rs`
- Modify: `crates/db/src/lib.rs`（`pub mod brain_event;` + `pub use`）
- Test: 同文件 #[cfg(test)]

**Interfaces:**
- Consumes: Task 1 的 brain_events 视图；`tickets.state`（需要你 join）
- Produces:
  - `pub struct BrainEvent { id, workspace_id, source, alarm_id: Option<String>, thing_id: Option<String>, title, verdict: Option<String>, reason, suggested_action: Option<String>, action_category: Option<String>, risk: Option<String>, run_id: Option<String>, ticket_id: Option<i64>, status: String, triage_mode: String, created_at: String, judged_at: Option<String>, state_entered_at: Option<String>, resolved_at: Option<String> }`（注意：无 evidence_json——列表不载）
  - `Db::list_brain_events(workspace_id, tab: BrainEventTab, before: Option<&str>, limit: i64) -> Result<Vec<BrainEvent>>`
  - `Db::brain_event_detail(workspace_id, id) -> Result<Option<BrainEventDetail>>`（Detail = BrainEvent + evidence_json）
  - `Db::brain_events_summary(workspace_id) -> Result<BrainEventsSummary>`（digested_today / needs_you / latency_p50_secs / feedback 计数读 judgment_feedback 现状表）
  - `pub enum BrainEventTab { NeedsYou, All, Patrol, Alarm, Directive }`

- [ ] **Step 1: 写失败测试——需要你口径、折叠、游标**

```rust
#[tokio::test]
async fn needs_you_counts_approval_and_unclaimed_escalated() {
    // 插：1 条 awaiting_approval judgment + 1 条 escalated 且 ticket open + 1 条 escalated 且 ticket claimed
    // 断言 needs_you = 2（claimed 的不算）
}

#[tokio::test]
async fn alarm_rows_fold_by_thing_rule_others_dont() {
    // 同 thing+rule 两条 alarm 事件 → feed 首页 1 行；两条不同 proposalId 的 patrol 事件 → 2 行
}

#[tokio::test]
async fn tuple_cursor_no_loss_same_second() {
    // 同 created_at 三行，limit 2 翻页能拿到第三行（昨日时间炸弹同款教训：
    // 用 Utc::now() 锚定，不硬编码日期）
}

#[tokio::test]
async fn list_does_not_carry_evidence_detail_does() {
    // list 行无证据字段；detail 返回完整 evidence_json
}

#[tokio::test]
async fn directive_classification_rule() {
    // problem_key=NULL & trigger_type='user' 的 run 出现在 directive tab；
    // problem_key='alarm:...' 的不出现
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `touch crates/db/src/lib.rs && cargo test -p db brain_event`
Expected: FAIL——模块不存在

- [ ] **Step 3: 实现 brain_event.rs**

feed SQL 骨架（48h 窗 + 需要你口径 + alarm 折叠 + 置顶 + 元组游标）：

```rust
let since = (chrono::Utc::now() - chrono::Duration::hours(48)).to_rfc3339();
// tab → WHERE 片段：
//   NeedsYou  => status='awaiting_approval' OR (status='escalated' AND
//                (ticket_id IS NULL OR (SELECT t.state FROM tickets t WHERE t.id=ticket_id)='open'))
//   Patrol    => source IN ('patrol','patrol_tick')
//   Alarm     => source='alarm'
//   Directive => source='directive'
//   All       => 1=1
// alarm 折叠（仅首页）：ROW_NUMBER() OVER (PARTITION BY thing_id,
//   (SELECT a.rule_id FROM thing_alarms a WHERE a.id=brain_events.alarm_id)
//   ORDER BY created_at DESC) = 1——仅 source='alarm' 参与折叠，patrol/directive 不折叠
// 置顶：需要你（awaiting_approval/escalated-未认领）在前，其余按 created_at DESC, id DESC
// 游标：created_at < 锚点 OR (created_at = 锚点 AND id < 锚点 id)
```

摘要 SQL：今日消化（created_at 今日且 status IN 终态）、需要你计数、延迟 p50（julianday(judged_at)-julianday(created_at)，仅 alarm 源、judged_at 非空）、反馈对错（读 judgment_feedback——P0 不迁）。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p db brain_event`
Expected: PASS（含需要你口径/折叠/游标/证据分离/分类五条）

- [ ] **Step 5: Commit**

```bash
git add crates/db/src/brain_event.rs crates/db/src/lib.rs
git commit -m "feat(db): brain_events 投影查询——feed 折叠/置顶/游标 + summary + detail"
```

---

### Task 3: /brain-events 端点（domain + 路由）

**Files:**
- Create: `apps/cloud/src/domains/brain_event/mod.rs`、`handler/mod.rs`、`dto.rs`
- Modify: `apps/cloud/src/domains/mod.rs`（注册域）、`apps/cloud/src/api/mod.rs`（路由）
- Test: `apps/cloud/src/tests/brain_event_tests.rs` + `apps/cloud/src/tests/mod.rs` 注册

**Interfaces:**
- Consumes: `Db::list_brain_events / brain_event_detail / brain_events_summary`（Task 2）
- Produces:
  - `GET /brain-events?tab=needs_you|all|patrol|alarm|directive&before=<id>&limit=N`
  - `GET /brain-events/summary`
  - `GET /brain-events/{id}`（detail 含 evidence_json）
  - 路由挂在与 /judgments 同一嵌套层（看 judgment/mod.rs 的挂法逐字仿）

- [ ] **Step 1: 写失败测试——三端点 + tab 参数 + detail 证据**

仿 `apps/cloud/src/tests/judgment_handler_tests.rs` 的 harness（内存库 + 迁移 + seed）：

```rust
#[tokio::test]
async fn brain_events_list_needs_you_tab() { /* 插数 → GET ?tab=needs_you → 只含待审批+未认领升级 */ }
#[tokio::test]
async fn brain_events_detail_carries_evidence() { /* detail 有 evidence_json，list 行没有 */ }
#[tokio::test]
async fn brain_events_summary_counts() { /* digested_today/needs_you/latency/feedback */ }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p tinyiothub-cloud brain_event`
Expected: FAIL——路由不存在（404）

- [ ] **Step 3: 实现域**

`dto.rs`：`BrainEventDto`（CamelCase JSON，字段同 Task 2 的 BrainEvent，无 evidence_json）+ `BrainEventDetailDto`（+ evidence）+ `BrainEventsSummaryDto`。
`handler/mod.rs`：三个 handler 逐字仿 judgment handler（auth claims → workspace_id → db 调用 → ApiResponseBuilder）。
`mod.rs`：`create_brain_event_router()`：`.route("/", get(...)).route("/summary", get(...)).route("/{id}", get(...))`。
`api/mod.rs`：找 /judgments 的 nest 行，旁边加 /brain-events。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p tinyiothub-cloud brain_event`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add apps/cloud/src/domains/brain_event apps/cloud/src/domains/mod.rs apps/cloud/src/api/mod.rs apps/cloud/src/tests/brain_event_tests.rs apps/cloud/src/tests/mod.rs
git commit -m "feat(cloud): /brain-events 只读端点（list/summary/detail）"
```

---

### Task 4: 前端 API client + 工作台读切换

**Files:**
- Create: `web/src/api/brain-events.ts`
- Modify: `web/src/ui/views/dispositions.ts`
- Test: `web/src/ui/views/dispositions.test.ts`

**Interfaces:**
- Consumes: `/brain-events` 三端点（Task 3）
- Produces: `brainEventApi.list(tab, before?, limit)` / `.summary()` / `.detail(id)`；类型 `BrainEvent`（source/status/徽章字段）供 Task 5/6

- [ ] **Step 1: 写失败测试——tab 定义与徽章映射**

在 dispositions.test.ts 加（或新文件 brain-events 相关纯函数）：

```ts
it("badge map covers all 10 states + patrol_tick", () => {
  // BADGE_MAP: investigating→分析中 / self_closed→自行闭环 / awaiting_approval→需要你
  // executing→执行中 / resolved→闭环 / escalated|investigation_failed→转工单
  // dismissed→已拒绝 / budget_skipped|dispatch_suppressed→未受理 / patrol_ok→巡检正常
  // 断言每个状态都有映射且无 undefined
});
it("tab counts from summary", () => { /* needsYou 计数驱动 tab 角标 */ });
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd web && ./node_modules/.bin/vitest run src/ui/views/dispositions.test.ts`
Expected: FAIL——BADGE_MAP 未导出

- [ ] **Step 3: 实现 client + 视图读切换**

`web/src/api/brain-events.ts` 逐字仿 judgments.ts（apiGet 同款 unwrap）。`dispositions.ts`：`judgmentApi.list/summary` 换成 `brainEventApi.list/summary`；`Judgment` 类型换成 `BrainEvent`；tab 从五个旧 tab 换成 `needs_you/all/patrol/alarm/directive`；徽章渲染用 `BADGE_MAP[ev.status]`；详情展开调 `brainEventApi.detail(id)`（证据懒加载——列表行无 evidence）。

注意：judgmentApi 的 approve/reject/feedback 调用**不动**（alarm 源变更仍走 /judgments；patrol 源走 /heartbeat/approvals——前端按 ev.source 分支调用，P0 就这么定）。

- [ ] **Step 4: 跑测试确认通过**

Run: `cd web && ./node_modules/.bin/vitest run && ./node_modules/.bin/tsc --noEmit`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add web/src/api/brain-events.ts web/src/ui/views/dispositions.ts web/src/ui/views/dispositions.test.ts
git commit -m "feat(web): 处置中心改读 /brain-events——badge 映射 + tab 来源过滤 + 证据懒加载"
```

---

### Task 5: 工作台交互与状态（行展开/五格状态/点跳）

**Files:**
- Modify: `web/src/ui/views/dispositions.ts`、`web/src/ui/views/dispositions.css`
- Test: `web/src/ui/views/dispositions.test.ts`

**Interfaces:**
- Consumes: Task 4 的 BrainEvent 类型与 detail 端点
- Produces: 行点击展开/收起行为；空态文案常量；SSE 断连细条

- [ ] **Step 1: 写失败测试——行展开切换 + 空态文案**

```ts
it("row click toggles evidence expand", () => { /* expandedId 切换逻辑抽纯函数或组件级断言 */ });
it("empty states carry product copy", () => {
  // needs_you 空：含「一切正常」与「今日 N 条已消化」
  // all 空：含「还没有事件——大脑还没开始干活」
});
```

- [ ] **Step 2: 跑测试确认失败** → Expected: FAIL

- [ ] **Step 3: 实现**

- 行点击：`@click` 在行容器上切换 expandedId；批准/拒绝/✓✕ 按钮 `@click.stop` 防冒泡
- 展开时 detail 未加载显「取证中…」骨架；失败内联「证据加载失败，重试」
- 空态两案（需要你/全部）按设计稿文案；tick 聚合行与工单号可点跳（tick 行跳 AI 运维巡检历史，工单号跳 #/tickets/{id}）
- SSE 断连细条（仿 tickets.ts 的 sse-banner）

- [ ] **Step 4: 跑测试确认通过** → `vitest run` PASS

- [ ] **Step 5: Commit**

```bash
git add web/src/ui/views/dispositions.ts web/src/ui/views/dispositions.css web/src/ui/views/dispositions.test.ts
git commit -m "feat(web): 工作台行交互与五格状态——点行展开、空态产品时刻、SSE 断连条"
```

---

### Task 6: 页面改名「AI 工作台」

**Files:**
- Modify: `web/src/ui/app.ts`（nav label、getPageTitle、getPageSubtitle）

- [ ] **Step 1: 改三处文案**

- nav label：处置中心 → AI 工作台
- getPageTitle dispositions → 'AI 工作台'
- getPageSubtitle dispositions → '大脑的完整工作日志——巡检、报警理解、指令执行，你只处理例外'

- [ ] **Step 2: 手测**（`npm run dev` 打开 #/dispositions 确认标题/导航/角标）

- [ ] **Step 3: Commit**

```bash
git add web/src/ui/app.ts
git commit -m "feat(web): 处置中心更名 AI 工作台"
```

---

### Task 7: 投影一致性证明（P0 的「零漂移」替代门禁）

**Files:**
- Test: `crates/db/tests/brain_events_view_test.rs`（扩展 Task 1 的文件）

- [ ] **Step 1: 写一致性测试**

```rust
#[sqlx::test]
async fn projection_matches_sources_exactly(pool: sqlx::SqlitePool) {
    // 种三类源数据后断言：
    // 视图 alarm 行数 == judgments 行数（同状态改名映射后逐条对得上）
    // 视图 patrol 行数 == agent_actions 提案行数
    // 视图 directive 行数 == agent_runs 中 problem_key IS NULL 且 trigger_type='user' 的行数
    // tick 聚合行数 == agent_actions summary 行的 (workspace, 小时) 分组数
}
```

- [ ] **Step 2: 跑测试确认通过**（视图是投影，天然一致——这条测试是 P1 落表前的对照基线）

Run: `touch crates/db/src/lib.rs && cargo test -p db projection_matches`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/db/tests/brain_events_view_test.rs
git commit -m "test(db): 投影一致性基线——视图与三源逐类相等（P1 双写的对照组）"
```

---

## Self-Review 记录

- **Spec 覆盖**：设计稿 P0 四件套（投影/端点/工作台/tick 聚合）→ Task 1/2+3/4+5+6/1（视图内含聚合行）。逐条可指。
- **占位符扫描**：无 TBD/TODO；每个代码步骤有真实代码或确切 SQL。
- **类型一致**：BrainEvent 字段在 Task 2 定义、Task 3 dto 映射、Task 4 前端类型同名；BADGE_MAP 键 = 视图 status 词汇表（含 dismissed/patrol_ok——P0 无产出但映射先全）。

## 执行提示

- Rust 侧每 Task 收尾 `cargo fmt`；推送走 pre-push 全量门（fmt+clippy+测试，~10min，后台跑）
- 前端测试命令：`cd web && ./node_modules/.bin/vitest run src/ui/views/dispositions.test.ts`（node 用 v24：PATH 前置 `$HOME/.nvm/versions/node/v24.14.0/bin`）
- 改迁移后必须 `touch crates/db/src/lib.rs`，否则 sqlx 编译期旧缓存不嵌新迁移
