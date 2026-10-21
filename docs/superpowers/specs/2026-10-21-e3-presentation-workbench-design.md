# E3 呈现层子 spec（Devices 工作台三 tab + Flow 时间机器 + 模板库）

- 日期：2026-10-21
- 分支：ai-consolidation
- 上位文档：`docs/superpowers/specs/2026-10-20-hub-runtime-architecture-design.md` §6 E3（X6 Flow 时间机器 / X7 规则模板库）
- 前序：E1（LiveState/Watch/账本）、E2（接缝/固定工具/快照/composite）
- 设计评审：`/plan-design-review` 2026-10-21，7 维全量；本文件末尾为评审报告
- 设计系统：`web/DESIGN_SYSTEM.md`（深空底/玻璃面板/渐变描边/发光状态点/tokens/a11y 规则）

## 0. 承接收尾（评审裁定 + 架构约束）

| 来源 | 裁定 | 落点 |
|---|---|---|
| D3 | 默认「设备」tab；World=zone 层级聚类（不做伪 2D 地图）；模板库不进三 tab；URL 可深链 tab/时刻/事件 | §2 |
| D4 | **SSE 实时投影**（用户裁定，覆盖推荐轮询）；账本范围查询 API；缺口显式标注 | §3/§4 |
| D5 | 五状态矩阵 + 桌面/平板/手机 + 键盘/ARIA/读屏列表视图，一次成型 | §6/§9 |
| D6 | 模板=场景包超集：`watch_rules` + `composites` + `{device-slug}` 引用解析 + 逐条校验报告 | §5 |
| 架构 R2-3-1 | 回放精度=状态变更粒度；遥测毛刺不保证回放 | §3.3 |
| 架构 X6 | Flow 数据源=E1 账本（watch/job/state_change） | §3 |
| 架构 X7 | 场景包携带 watch 条件与目标模板；规则引用解析复用 E1 重织逻辑 | §5 |
| E2 | composite job 事件已逐步落账；固定设备工具契约 | §3/§5 |
| 设计系统 | 深空底、`--accent-gradient`、玻璃面板、发光点、tokens、reduced-motion | §8 |

## 1. 组件与边界

```
apps/cloud/src/domains/agent/host/handler/
  ledger.rs       GET /workspaces/{id}/ledger（范围查询 + 状态重建）
  live_stream.rs  GET /workspaces/{id}/live/stream（SSE）
  scene_pack.rs   模板引用解析校验（扩展现有 marketplace 实例化）

apps/cloud/src/domains/marketplace/
  scene_instantiator.rs  扩展 watch_rules/composites 实例化 + 引用解析

web/src/api/
  ledger.ts      账本查询客户端
  live.ts        SSE 客户端（fetch ReadableStream，Bearer 头）
  scene-pack.ts  模板列表/校验/实例化

web/src/ui/views/
  things.ts      演进为工作台壳：三 tab + 深链（复用现有列表/详情/向导）
  flow.ts        Flow 时间机器（新）
  flow.css
  world.ts       World zone 聚类（新）
  world.css
  template-library.ts  模板库（新）
  template-library.css
```

边界规则：

1. **前端只订阅 Hub 投影**：设备卡与 Flow 的实时数据来自 `/live/stream`；不直接查 DB，不轮询设备表。
2. **历史只读**：历史模式下所有设备动作（call/set/stop/apply）禁用；模式切换必须显式且不可与实时混淆。
3. **账本只读**：新增 API 只读；不改变 E1 账本写入语义。
4. **SSE 认证走 Bearer**：EventSource 不能带 header，前端用 `fetch` + `ReadableStream` 实现 SSE 解析；token 不进 URL/日志。
5. **模板实例化复用现有管线**：不新建双轨模板系统；新增字段版本化（`v`），未知字段忽略。
6. **多租户**：账本/实时流/模板全部按 workspace_id 隔离。

## 2. 信息架构（D3）

### 2.1 路由与默认

- 工作台 = 现有 `/things` 页面演进（旧 `/devices` 已重定向到 `/things`）。
- 三 tab：`设备`（默认）/ `Flow` / `World`。
- 深链：`/things?tab=flow&at=2026-10-21T14:32:07Z&event=<ledgerId>&device=<thingId>`。
  - `at` + `event` + `device` 任一存在 → 直接进入历史模式并定位；否则实时模式。
  - 浏览器前进/后退恢复完整工作台状态（tab/时间/选中项）。

### 2.2 首屏层级

```
全局顶栏（工作区/搜索/健康汇总/实时时钟）
工作台头部：设备工作台 | [设备] [Flow] [World] | 时间范围 · 回到现在 · live/paused
──────────────────────────────────────────────────────────────
设备 tab（复用现有 DevicesView）
  筛选条（状态/协议/搜索）→ 列表|网格|树 → 详情抽屉

Flow tab
  筛选条（zone/health/kind/设备）
  ├─ 泳道时间轴（每设备一泳道 + 协同链泳道）
  ├─ 底部分钟级 scrubber + play/pause + 1x/4x
  └─ 右检查器（事件详情 + 该时刻状态快照 + 账本链接）

World tab
  zone 层级聚类（园区→楼栋→楼层→设备点）+ 每 zone 健康汇总
  点击 zone 下钻；右侧 zone 设备清单

模板库（入口：设备页头「模板库」+ Marketplace）
  模板卡（名称/来源/含 watch 数/composite 数/上次应用）
  → 预览 → 引用解析结果 → 应用（逐条校验报告）
```

**Constraint worship**：每屏只允许一个主角。设备 tab 的主角是设备列表/卡片；Flow 的主角是时间轴；World 的主角是 zone 聚类；模板库的主角是模板卡与校验结果。

### 2.3 模板库位置

- 不是第四个 tab（保持三 tab 契约）。
- 入口：设备页头次级按钮「模板库」；Marketplace 场景包详情页「应用到工作区」。
- 应用完成后回到工作台并高亮新规则/链（可选跳转 Flow）。

## 3. Flow 时间机器（X6 + D4）

### 3.1 数据源

- `hub_ledger`（E1）：`state_change` / `watch_hit` / `watch_timeout` / `watch_expired` / `rate_limited` / `job`（E2 composite 逐步事件）。
- 范围查询 API（新）：

```
GET /workspaces/{id}/ledger
  ?from=<RFC3339>&to=<RFC3339>&kind=<csv>&thingId=<id>&cursor=<id>&limit=<1..500>
返回：
{ "entries": [{ "id": 123, "ts": "...", "kind": "...", "thingId": "...", "payload": {...} }],
  "nextCursor": 124|null, "hasMore": bool,
  "coverage": { "from": "...", "to": "...", "gaps": [{ "from": "...", "to": "...", "reason": "retention|lagged|query_cap" }] } }
```

- 默认窗口 24h；单次查询窗口上限 7d；limit 默认 200、上限 500；按 `id` 升序。
- **状态重建**：`GET /workspaces/{id}/ledger/state?at=<RFC3339>` 返回每设备在 `at` 时刻的最近 `state_change` 折叠值：

```
{ "at": "...", "devices": [{ "thingId": "...", "state": {...}, "lastChangeAt": "..." }],
  "precision": "state_change", "unknown": ["health", "zone"] }
```

  - `precision` 明示回放精度；`unknown` 列出账本无法重建的派生字段（E1 账本未落 health/zone 变化）。
  - 没有历史值的设备显示「历史状态不可得」，不用当前值冒充。

### 3.2 实时（用户裁定 SSE）

```
GET /workspaces/{id}/live/stream   (Accept: text/event-stream)
事件：
  event: snapshot   data: {v:1, devices:[{thingId,name,health,zone,online,values}]}   # 首帧
  event: state_change data: {v:1, ledgerId, ts, thingId, payload}
  event: watch        data: {v:1, ledgerId, ts, ruleId, thingId, matched, ...}
  event: job          data: {v:1, ledgerId, ts, compositeId, stepIndex, status, ...}
  event: resync       data: {v:1, reason:"lagged"|"reconnect"}                        # 客户端丢弃本地态并等下一帧 snapshot
  event: ping         data: {}                                                        # 15s keepalive
```

- 服务端：每工作区订阅 EventBus；首帧读 LiveState 快照；账本事件按 `hub_ledger.id` 作为 SSE `id:`，支持 `Last-Event-ID` 断线续传（回放缺口 → `resync`）。
- 前端：fetch + ReadableStream（Bearer 认证），指数退避重连（1s→30s，上限），页面隐藏时暂停、可见时重连。
- 客户端状态：实时模式只接受 `ts >= lastSeen` 的事件；历史模式下 SSE 连接保持但事件只更新「回到现在」徽标计数，不改变画面。

### 3.3 回放语义（显式局限）

- 拖动 scrubber = 查询该时刻状态 + 前后窗口事件；播放 = 匀速重放窗口内事件。
- **精度=状态变更粒度**；遥测毛刺不保证回放（架构裁定）。界面固定一行说明，不隐藏。
- 账本缺口（保留期外/查询上限/Lagged）→ 顶部琥珀横幅「回放不完整：<区间>」，并在时间轴对应区间画斜纹。
- 历史模式：琥珀顶线 + 「历史回放 · {time}」chip；所有动作按钮禁用并带 tooltip「历史模式不可执行」。

### 3.4 时间轴交互

- 泳道：每台出现过的设备一泳道；协同链（compositeId 相同）合并为一条「协同链」泳道并以括号框住其步骤。
- 事件 chip：watch 命中（琥珀）、job accepted（蓝描边）、result（青）、rejected（红）、state_change（中性小点）。
- scrubber：分钟级刻度；拖拽即查；`play/pause`；速度 1x/4x；「回到现在」。
- 选中 chip → 右检查器：事件类型/设备/zone/规则或目标/执行体/参数/结果/账本 id；以及该时刻设备状态快照。
- 「以列表查看」切换：同一数据渲染为按时间分组的事件列表（读屏/手机复用）。

## 4. World（zone 聚类）

- 数据：`Thing.parent_id` 层级 + E1 LiveState 的 zone/health；后端复用 `/live/stream` 快照（或账本 `state` 历史）。
- 视图：从根 zone 开始的分组画布；每 zone 一张玻璃面板，显示 zone 名、设备数、worst health、bad/attention 计数、关键设备点。
- 点击 zone 下钻一级；面包屑返回；设备点选中 → 右侧设备摘要（复用 read_device 展示字段）。
- 空态：无层级 → 「还没有空间层级」+ 去建设备层级 CTA。
- 不做坐标/朝向/地图帧（E4 前无数据支撑）。

## 5. 模板库（X7 + D6）

### 5.1 模板 schema（场景包超集，v=1）

场景包定义新增可选段（未知字段忽略，P5）：

```json
{
  "v": 1,
  "name": "storm_guard",
  "things": [ ... 现有场景包 things ... ],
  "watch_rules": [
    { "ref": "wind", "kind": "property", "property": "风速", "op": "gt", "value": 20,
      "persistent": true, "timeout_secs": 3600 }
  ],
  "composites": [
    { "ref": "guard", "name": "storm_guard",
      "steps": [
        { "op": "call", "thing": "{window}", "action": "close" },
        { "op": "call", "thing": "{panel}", "action": "retract",
          "rollback": { "op": "call", "thing": "{panel}", "action": "extend" } }
      ] }
  ]
}
```

- 引用语法：`{device-slug}` 指向场景包内 `things[].ref`/slug；`{ref}` 解析到实例化后的真实 thing_id。
- 解析时机：实例化**后**、落库**前**；解析失败 → 整包拒绝（不静默部分成功），返回逐条报告。
- 校验报告：

```json
{ "ok": false, "references": [
    { "path": "watch_rules[0].ref", "ref": "wind", "status": "missing_device", "message": "物 wind 未实例化" },
    { "path": "composites[0].steps[0].thing", "ref": "window", "status": "unresolved", "message": "..." }
  ], "conflicts": [ { "kind": "duplicate_rule_name", "message": "..." } ] }
```

- 应用：watch 规则落 `watch_rules`（persistent 按模板值）；composite 落 `agent_composites`；任何一条失败 → 事务回滚 + 报告。
- 重名冲突：同 workspace 下同 ref 的规则/链 → 报告 `conflict`，用户选择覆盖或跳过（E3 默认拒绝，不自动覆盖）。

## 6. 交互状态矩阵（D5）

| FEATURE | LOADING | EMPTY | ERROR | SUCCESS | PARTIAL |
|---|---|---|---|---|---|
| 设备 tab | 骨架卡 ×6 | 「还没有设备」+ 添加设备/接入网关 | 保留筛选的 retry 横幅 | 列表/网格/树 | 某 zone 分组加载失败：该组内联错误，其余可用 |
| Flow 时间轴 | 泳道骨架 + scrubber 禁用 | 「该时段没有事件」+ 放宽时间窗/回到现在 | 时间轴区域 retry，保留已选时刻 | 泳道+事件 chip+检查器 | 账本缺口：琥珀横幅+斜纹区间；derived 未知标「历史不可得」 |
| Flow 实时连接 | 首帧前显示连接中 | 空工作区同上 | 断线横幅「实时连接中断，重连中」+ 手动重连 | live 绿点 + 时钟 | 重连后 resync：保留历史视图并提示 |
| World | zone 骨架 | 「还没有空间层级」+ 去建设备层级 | retry | zone 面板+下钻 | 部分 zone 无实时数据 → 「数据暂缺」 |
| 模板库 | 模板卡骨架 | 「还没有可用模板」+ 去 Marketplace | retry | 卡片网格 | 部分模板加载失败：标「不可用」 |
| 应用模板 | 校验中 | — | 校验失败：逐条报告 + 不应用 | 成功摘要 + 查看 Flow | 部分成功：事务回滚并显示失败项 |

状态必须用 **视觉 + 文本** 双通道；不能只靠颜色。

## 7. 用户旅程（storyboard）

| STEP | USER DOES | USER FEELS | PLAN SPECIFIES |
|---|---|---|---|
| 1 | 收到告警，打开 AI 中心 | 紧迫、需要定位 | 告警卡带「查看 Flow」深链（at=事件时刻） |
| 2 | 点深链进入 Flow 历史态 | 需要确定性：这是过去 | 琥珀顶线 + 历史 chip + 动作禁用 |
| 3 | 看泳道与协同链，选中事件 | 想理解「AI 为什么这么做」 | 检查器显示规则/参数/结果/账本链接 |
| 4 | 拖时间轴回看前后 | 需要连续上下文 | scrubber + 播放 + 状态快照 + 缺口显式 |
| 5 | 回到设备 tab 处置 | 需要掌控 | 「回到现在」一键；设备卡动作可用 |
| 6 | 对复发模式应用模板 | 想减少下次工作量 | 模板预览 + 校验报告 + 应用后高亮 |

时间尺度：5 秒（异常是否真实）、5 分钟（因果链）、5 年（可审计、可回放、可信任）。

## 8. 设计系统映射

- 画布：`--bg-deep-space`；面板 `--glass-bg` + `--glass-border`；主操作 `--accent-gradient`；发光 `--accent-glow(-strong)`。
- 间距/圆角/动效：只用 `--space-*` / `--radius-*` / `--duration-*` / `--ease-*`。
- 状态点：ok = cyan→green，attention = amber，bad = red；脉冲只用于 live 与 attention，不做全屏脉冲。
- 字体：沿用 `--font-display`；时间/数值用 tabular numerals（`font-variant-numeric: tabular-nums`）。
- 新组件（每件必须有 hover/focus/disabled/selected 四态）：
  `EventChip` / `TimelineLane` / `Scrubber` / `ZoneCluster` / `HealthDot` / `TemplateCard` / `ValidationReport`。
- CSS：`styles/views/devices.css` 扩展；新增 `flow.css`、`world.css`、`template-library.css`，禁止 >200 行内联样式。
- 动效：一次主入场（时间轴淡入）+ scrubber 拖动跟手；`prefers-reduced-motion` 全部关闭。

## 9. 响应式与可访问性（D5 全量）

- 断点：
  - ≥1280：Flow 三区（筛选/时间轴/检查器）；World 多列 zone。
  - 768–1279：检查器变底部抽屉；时间轴横向滚动；World 两列。
  - <768：Flow 变纵向事件列表 + 时间范围选择器（保留 slider 语义）；检查器全屏 sheet；设备卡单列；触控目标 ≥44px。
- 键盘：
  - Tab 焦点顺序：tab 栏 → 筛选 → 时间轴 → 检查器。
  - 时间轴 `role="slider"`，`aria-valuemin/max/now/valuetext`；←/→ 移动 1 事件，Shift+←/→ 移动 1 分钟；Space 播放/暂停；`[`/`]` 缩放时间窗；Esc 关闭检查器并把焦点还给触发元素。
  - 「以列表查看」提供与时间轴等价的键盘/读屏路径。
- ARIA：模式切换 `aria-live="polite"`；连接状态 `role="status"`；泳道与 chip 有可读名称（时间+事件+设备）；图表类装饰 `aria-hidden`。
- 对比度：正文 ≥4.5:1；状态不只靠颜色（点 + 文本标签）。
- reduced motion：脉冲、播放动画、入场动画关闭。

## 10. 非目标

- 2D/3D 地图与坐标帧（E4 前无数据）
- Flow 编辑/回滚操作（只读回放；动作回设备 tab）
- 模板可视化编辑器（E3 只应用/校验；编辑留后续）
- 跨工作区模板共享
- 移动端原生手势（仅标准滚动/点击）
- SSE 之外的新推送基础设施

## 11. 退出标准

1. 多设备协同链（watch→job→结果）在 Flow 中可见、可溯、可回放：深链复现时刻、事件与状态一致、缺口显式。
2. 历史模式不可误操作：动作全部禁用且视觉不可混淆。
3. 实时 SSE 断线可重连并 resync；首帧快照与 LiveState 一致。
4. 装一个场景包即自带值守规则：watch_rules + composites 实例化成功，引用解析失败时整包拒绝并逐条报告。
5. 三 tab 五状态矩阵、桌面/平板/手机、键盘/读屏路径均有测试或可验收说明。

## 12. 测试策略

- 后端：账本范围查询（分页/窗口上限/缺口）、状态重建（无历史返回不可得）、SSE（首帧/增量/Last-Event-ID/重连 resync/租户隔离）、模板 schema 校验与引用解析（成功/缺失/冲突/事务回滚）。
- 前端：vitest 组件测试（五状态、键盘、深链解析、历史模式禁用）、SSE 解析器单测（分帧/重连/坏帧）、Flow 数据折叠纯函数单测。
- 端到端：watch 命中 → 账本 → Flow 查询 → 状态重建；场景包应用 → 规则与链落库 → Flow 可见协同链。
- a11y：axe 或等价检查；键盘走查脚本。

## 13. GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 2 | CLEAR | 7 proposals, 6 accepted, 1 deferred |
| Outside Review | codex (auto) | Independent 2nd opinion | 10 | unavailable | CODEX_MODEL invalid；native fallback 报告丢失 |
| Eng Review | `/plan-eng-review` | Architecture & tests | 1 | ISSUES OPEN（历史记录） | E1-E2 已落地；E3 实施前需 eng 复审 |
| Design Review | `/plan-design-review` | UI/UX gaps | 1 | ISSUES OPEN | score: 2/10 → 8/10, 4 decisions |
| DX Review | `/plan-devex-review` | Developer experience gaps | 0 | — | — |

- **OUTSIDE COVERAGE:** codex unavailable（CODEX_MODEL invalid，已知问题）；native fallback 报告内容丢失；本轮设计评审为单模型，mockup 生成不可用（缺 OpenAI API key）。
- **VERDICT:** Design Review CLEARED（4 项裁定全落地）；E3 实施前仍需 `/plan-eng-review` 验证后端 API/SSE 架构。

**Unresolved design decisions:** 0（D3/D4/D5/D6 已逐项裁定并回写本 spec）。

NO UNRESOLVED DECISIONS
