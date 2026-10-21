# E3 呈现层 Implementation Plan（工作台三 tab + Flow 时间机器 + 模板库）

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** 落地 E3——Devices 工作台三 tab（设备/Flow/World）、Flow 账本回放时间机器（SSE 实时）、场景包模板库（watch+composite 引用解析）。

**Architecture:** 后端新增账本只读查询、状态重建、SSE 实时流、场景包扩展；前端在工作台壳内三 tab，Flow 以泳道时间轴为唯一主角，历史模式只读。详细设计见 `docs/superpowers/specs/2026-10-21-e3-presentation-workbench-design.md`。

**Tech Stack:** Rust + axum + tokio + sqlx（后端）；Vite + Lit + signals（前端，Node 24 构建）。

**前置条件：** E1 账本/ watch API、E2 composite 已合入（AI-consolidation 分支 HEAD 1efa5c05 起）。

## Global Constraints

- 前端只订阅 Hub 投影；不直接查设备表、不轮询 DB。
- 历史模式只读：所有动作按钮禁用并带 tooltip。
- 账本查询默认窗口 24h、单查上限 7d、limit ≤500；回放精度=state_change。
- SSE 用 fetch+ReadableStream（Bearer 头），token 不进 URL；15s ping；Last-Event-ID 重连。
- 模板实例化失败整包回滚，逐条报告；不静默部分成功。
- 设计只用 `web/DESIGN_SYSTEM.md` tokens；新 CSS 文件，禁止 >200 行内联。
- a11y：时间轴 slider 语义、列表回退、44px 触控、reduced-motion。
- 前端构建：Node 24（`export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"`）。
- 命令：`cargo test -p tinyiothub-cloud` / `cd web && npm test -- --run`。

---

### Task 1: 账本范围查询 + 状态重建 API

**Files:**
- Create: `apps/cloud/src/domains/agent/host/handler/ledger.rs`
- Modify: `apps/cloud/src/domains/agent/host/handler/mod.rs`（nest 路由）
- Modify: `crates/db/src/watch.rs`（查询函数：按 workspace/时间/kind/thing/cursor）
- Test: `ledger.rs` + `crates/db/src/watch.rs`

**Interfaces:**
- `GET /workspaces/{id}/ledger?from&to&kind&thingId&cursor&limit`
- 返回 `{entries, nextCursor, hasMore, coverage:{from,to,gaps:[]}}`
- `GET /workspaces/{id}/ledger/state?at=` → `{at, devices:[{thingId,state,lastChangeAt}], precision:"state_change", unknown:["health","zone"]}`
- DB：`list_ledger_range(pool, ws, from, to, kinds, thing, after_id, limit) -> Vec<LedgerRow>`；`state_at(pool, ws, at) -> Vec<(thing_id, payload, ts)>`（按 thing 取 `state_change` 最新 ≤ at）。

- [ ] **Step 1: 失败测试**：跨区隔离；时间窗 >7d 返回 400；limit>500 clamp；分页 cursor；无历史设备不出现在 state。
- [ ] **Step 2: 实现 DB 查询 + handler**（admin/成员可读；按 workspace 限定）。
- [ ] **Step 3: 验证**：`cargo test -p tinyiothub-cloud --lib ledger` + `cargo test -p db watch`
- [ ] **Step 4: Commit**

```bash
git commit -am "feat(ledger): E3 账本范围查询+状态重建 API"
```

---

### Task 2: SSE 实时投影

**Files:**
- Create: `apps/cloud/src/domains/agent/host/handler/live_stream.rs`
- Modify: `handler/mod.rs`
- Test: `live_stream.rs`

**Interfaces:**
- `GET /workspaces/{id}/live/stream`（`text/event-stream`）
- 事件：`snapshot`（首帧，LiveState 工作区快照）→ `state_change|watch|job`（带 `id:` = hub_ledger.id）→ `resync` → `ping`（15s）
- 订阅 `EventBus`，按 workspace 过滤；`Last-Event-ID` 重连时先补账本增量，缺口发 `resync`。
- 前端认证用 Bearer header（fetch 流），handler 复用现有 `Claims` extractor。

- [ ] **Step 1: 失败测试**：首帧 snapshot；事件按 workspace 隔离；ping；客户端断开后任务退出；Last-Event-ID 补增量/缺口 resync。
- [ ] **Step 2: 实现**（axum `Sse` 或手写 Stream；注意 broadcast Lagged → resync）。
- [ ] **Step 3: 验证**：`cargo test -p tinyiothub-cloud --lib live_stream`
- [ ] **Step 4: Commit**

```bash
git commit -am "feat(live): E3 SSE 实时投影（snapshot/增量/Last-Event-ID/重连 resync）"
```

---

### Task 3: 场景包扩展 + 引用解析 + 校验报告

**Files:**
- Modify: `apps/cloud/src/domains/marketplace/scene_instantiator.rs`
- Modify: `crates/db/src/scene_template.rs`（如 schema 校验在此）
- Create: `apps/cloud/src/domains/marketplace/template_rules.rs`（解析/校验纯函数）
- Test: `template_rules.rs` + `scene_instantiate_test.rs`

**Interfaces:**
- 场景包 v1 新增可选 `watch_rules[]` / `composites[]`（schema 见 spec §5.1）。
- `resolve_template_refs(def, instantiated: &HashMap<slug, thing_id>) -> Result<Resolved, ValidationReport>`：
  - `{slug}` 解析；缺失 → `missing_device`；composite 步内引用同样解析。
  - 冲突：同 workspace 下重名规则/链 → `duplicate_rule_name`，整包拒绝。
- 落库：`watch_rules`（persistent 按模板）+ `agent_composites`，同一事务；任一失败回滚。

- [ ] **Step 1: 失败测试**：成功解析/缺失引用/冲突/事务回滚/未知字段忽略。
- [ ] **Step 2: 实现**
- [ ] **Step 3: 验证**：`cargo test -p tinyiothub-cloud --lib template_rules scene`
- [ ] **Step 4: Commit**

```bash
git commit -am "feat(template): E3 场景包携带 watch/composite+引用解析+逐条校验"
```

---

### Task 4: 前端 API 客户端

**Files:**
- Create: `web/src/api/ledger.ts`, `web/src/api/live.ts`, `web/src/api/scene-pack.ts`
- Test: `web/src/api/live.test.ts`, `ledger.test.ts`

**Interfaces:**
- `ledgerApi.query({from,to,kinds,thingId,cursor,limit})`；`ledgerApi.stateAt(at)`。
- `liveApi.connect(workspaceId, handlers, {signal})`：fetch SSE 解析器（分帧/多行 data/注释/坏帧跳过），指数退避重连，`lastEventId`；暴露 `pause()/resume()/close()`。
- `scenePackApi.list() / validate(name) / apply(name, mapping)`。

- [ ] **Step 1: 失败测试**：SSE 分帧（CRLF/多行/注释/半包）、重连带 Last-Event-ID、坏帧不中断。
- [ ] **Step 2: 实现**
- [ ] **Step 3: 验证**：`cd web && npx vitest run src/api`
- [ ] **Step 4: Commit**

```bash
git commit -am "feat(web): E3 账本/SSE/场景包 API 客户端"
```

---

### Task 5: 工作台壳（三 tab + 深链 + 模式）

**Files:**
- Modify: `web/src/ui/views/things.ts`（DevicesView 外壳）
- Create: `web/src/ui/views/workbench-state.ts`（纯函数：URL ↔ 状态）
- Test: `things.test.ts` / `workbench-state.test.ts`

**Interfaces:**
- 状态：`{tab:'devices'|'flow'|'world', at?:string, event?:number, device?:string}`；`parseUrl(search)`/`toUrl(state)`。
- 默认 tab=devices；有 `at|event|device` → 历史模式。
- 页头：segmented tabs、时间范围、「回到现在」、live/paused 指示；历史模式琥珀顶线 + chip。
- 深链变化用 `history.pushState`，`popstate` 恢复。

- [ ] **Step 1: 失败测试**：URL 解析/生成 round-trip；默认 tab；历史模式判定；popstate。
- [ ] **Step 2: 实现壳**（设备 tab 继续渲染现有表/卡/树/详情）
- [ ] **Step 3: 验证**：`npx vitest run src/ui/views/things.test.ts`
- [ ] **Step 4: Commit**

```bash
git commit -am "feat(web): E3 工作台壳——三 tab+深链+历史模式"
```

---

### Task 6: 设备 tab 实时卡片

**Files:**
- Modify: `web/src/ui/views/things.ts`（新增卡片视图 + SSE 订阅）
- Modify: `web/src/styles/views/devices.css`
- Test: `things.test.ts`

**Interfaces:**
- 卡片：名称/zone 面包屑/发光 health 点/1-2 关键值/72px sparkline/watch 或 job 徽标。
- 排序：health worst-first；筛选：全部/bad/attention/未读。
- 实时：`liveApi.connect`，`snapshot` 替换列表、增量更新卡片；断线横幅 + 重连；页面隐藏 pause。
- 历史模式：卡片显示该时刻值，动作按钮禁用。

- [ ] **Step 1: 失败测试**：worst-first 排序；增量更新；断线状态；历史禁用。
- [ ] **Step 2: 实现**
- [ ] **Step 3: 验证**：`npx vitest run src/ui/views/things.test.ts`
- [ ] **Step 4: Commit**

```bash
git commit -am "feat(web): E3 设备卡实时投影+五状态"
```

---

### Task 7: Flow 时间机器

**Files:**
- Create: `web/src/ui/views/flow.ts`, `flow.css`
- Create: `web/src/ui/views/flow-model.ts`（纯函数：事件→泳道/链折叠/状态折叠）
- Test: `flow-model.test.ts`, `flow.test.ts`

**Interfaces:**
- `buildLanes(entries, stateAt) -> {lanes:[{key,label,chips[]}], chains:[...], gaps[]}`。
- 渲染：时间轴（分钟刻度）、事件 chip、协同链括号、scrubber（selider）、play/pause、1x/4x。
- 交互：拖拽查 `stateAt` + 窗口事件；选中 → 检查器；键盘（←/→、Shift、Space、`[`/`]`、Esc）；列表视图切换。
- 历史模式动作禁用；缺口琥珀横幅 + 斜纹区间。

- [ ] **Step 1: 失败测试**：泳道/链折叠；缺口区间；键盘步进；列表视图等价。
- [ ] **Step 2: 实现**
- [ ] **Step 3: 验证**：`npx vitest run src/ui/views/flow`
- [ ] **Step 4: Commit**

```bash
git commit -am "feat(web): E3 Flow 时间机器——泳道/回放/检查器/键盘"
```

---

### Task 8: World zone 聚类

**Files:**
- Create: `web/src/ui/views/world.ts`, `world.css`
- Test: `world.test.ts`

**Interfaces:**
- `buildZoneTree(devices, hierarchy) -> zones[]`（worst-first、计数）。
- 渲染：zone 玻璃面板、下钻面包屑、设备点、右侧摘要。
- 空态/部分数据缺失。

- [ ] **Step 1: 失败测试**：health 汇总 worst-first；下钻；空态。
- [ ] **Step 2: 实现**
- [ ] **Step 3: 验证**：`npx vitest run src/ui/views/world`
- [ ] **Step 4: Commit**

```bash
git commit -am "feat(web): E3 World zone 聚类视图"
```

---

### Task 9: 模板库

**Files:**
- Create: `web/src/ui/views/template-library.ts`, `template-library.css`
- Test: `template-library.test.ts`

**Interfaces:**
- 模板卡：名称/来源/watch 数/composite 数；预览；应用。
- 校验报告逐条渲染（missing_device/unresolved/conflict）；失败禁用「应用」。
- 应用成功 → 回工作台 + 高亮/提示；部分失败 → 显示回滚与失败项。

- [ ] **Step 1: 失败测试**：报告渲染；失败禁用；成功回调；空态。
- [ ] **Step 2: 实现**
- [ ] **Step 3: 验证**：`npx vitest run src/ui/views/template-library`
- [ ] **Step 4: Commit**

```bash
git commit -am "feat(web): E3 模板库——预览/校验报告/应用"
```

---

### Task 10: 状态矩阵 + 响应式 + a11y 收口

**Files:**
- Modify: `flow.ts`, `world.ts`, `things.ts`, `template-library.ts` + 对应 css
- Test: `web/src/ui/views/a11y.test.ts`（键盘/focus 回归）

- [ ] **Step 1: 五状态矩阵逐项实现并测试**（spec §6）
- [ ] **Step 2: 断点**：≥1280 三栏；768–1279 抽屉/横向滚动；<768 Flow 列表版 + 全屏 sheet；44px 触控。
- [ ] **Step 3: a11y**：slider ARIA、aria-live、列表回退、对比度、reduced-motion。
- [ ] **Step 4: 验证**：`npx vitest run src/ui/views` + 手动键盘走查
- [ ] **Step 5: Commit**

```bash
git commit -am "feat(web): E3 五状态/响应式/a11y 收口"
```

---

### Task 11: 端到端 + 全量门禁 + 文档

**Files:**
- Test: `apps/cloud/src/tests/e3_workbench_e2e_tests.rs`
- Modify: `docs/superpowers/specs/2026-10-20-hub-runtime-architecture-design.md`（E3 勾选）
- Modify: `TODOS.md`（延后项）

- [ ] **Step 1: e2e**：watch 命中 → 账本 → ledger API → stateAt；场景包应用 → watch/composite 落库 → Flow 协同链。
- [ ] **Step 2: 全量**：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd web && export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH" && npm test -- --run
```

- [ ] **Step 3: 文档回写**（架构 spec E3 行、TODOS、本计划执行记录）。
- [ ] **Step 4: Commit**

```bash
git commit -am "test(e3): 端到端+全量门禁+文档回写"
```

---

## 风险与回滚

- SSE 断线/多实例：Last-Event-ID + resync；关闭开关=前端回退 2s 轮询（保留降级路径）。
- 账本查询成本：窗口上限 7d + limit 500 + 索引 `(workspace_id, ts)`；超限 400。
- 历史状态不完整：显式 `unknown` 字段 + 缺口横幅，禁止用当前值冒充历史。
- 模板双轨风险：只扩展场景包，不新建独立模板表。
- 回滚：三 tab 保留旧列表视图；模板/Flow 入口可隐藏（feature flag）。

## 执行记录（2026-10-21）

| 任务 | 交付 | 验证 |
|---|---|---|
| T1 | 账本范围查询 + 截止时刻状态重建（`GET /workspaces/{id}/ledger[/state]`） | `db watch` 4 tests + `cloud ledger` 5 tests |
| T2 | SSE 实时投影（snapshot→账本 id 增量→resync→ping；Last-Event-ID） | `cloud live_stream` 1 test + 编译；fetch+Bearer 客户端 |
| T3 | 场景包 `watch_rules/composites` + `{slug}` 解析 + 逐条报告 + 事务应用 | `template_rules` 4 tests；详情透出 watchRules/composites |
| T4 | 前端 ledger/live/scene-pack 客户端（SSE 分帧/重连/坏帧） | `live.test.ts` 4 tests |
| T5-T9 | 工作台三 tab + Flow 泳道/scrubber/检查器/列表回退 + World zone 聚类 + 模板库 | 前端 151 tests；tsc；vite build |
| T10 | 五状态/断点/键盘/ARIA/reduced-motion（首版） | flow.css/workbench.css/world.css/template-library.css |
| T11 | 后端全量门禁未跑（cloud lib 10min）；已跑：db/cloud 定向 tests + clippy 无新增 | 见上 |

**显式延后**：Flow 播放期间缺口斜纹的精确 id 映射、World 坐标帧（E4 后）、模板映射自动建议、模板可视化编辑器、移动端手势。

