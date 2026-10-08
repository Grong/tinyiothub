# AI 运行透明化设计（处置中心处理过程可见）

- 日期：2026-10-08
- 分支：ai-brain-p0
- 状态：已与作者分节确认（目的/范围/实时性/载体/用户文案/简化版架构/三节设计均逐项批准）

## 背景与问题

AI 中心（处置中心 `web/src/ui/views/dispositions.ts` + AI 运维中心 `ai-ops.ts`）当前只展示 AI 处理的**终态成品**：卡片流（状态徽章+标题+理由+建议动作）、统计摘要、审批按钮，以及懒加载的"证据"面板（折叠的 think 块、verdict 原文、聚合动作清单、工具调用计数/耗时/tokens）。

**处理过程是黑盒**：

- runner 每收到一个 ToolCall/ToolResult 事件就往 `RunContextInner.trace` push 一条 `ToolTraceEntry`（`crates/agent/src/runtime/thing_agent/runner.rs:226`）——逐步记录全程都在做；
- 但 `assemble_report` 只把聚合后的 `actions` 装进 `RunReport`，原始 trace 随 run 结束在内存中销毁（`crates/core/src/agent_runs.rs:104` RunReport 无 trace 字段）；
- 进行中的 trace 锁在 runner 任务内存里，无任何对外出口；`investigating` 卡片不可展开（`dispositions.ts:274-276`）。

这是 thing-agent-loop spec（`2026-07-29-thing-agent-loop-design.md`）中"物事件/定时触发静默落库、前端本期最小"的既定裁决，现在到期偿还。

## 目标（与作者确认）

- **双受众**：运营排障（全量轨迹）+ 终端用户信任（大白话叙述），同一套数据两层呈现。
- **范围**：AI 自治巡检/处置链路（告警调查 run + 心跳巡检 run）。用户指令对话链路不在本期。
- **实时性**：investigating 进行中可见进度 + 事后完整逐步轨迹。
- **载体**：卡片摘要 + 详情页全量。
- **用户文案**：LLM 生成大白话叙述（verdict 协议块加字段，零额外 LLM 调用）。

## 架构决策：活动 run 注册表 + 轮询（而非 bus 事件 + SSE 推送）

初版方案是 AgentEventBus 加 `RunTraceAppended` 变体、runner 注入 bus、SSE 加事件类型、cloud 加订阅者四道工序。作者认为复杂度过高，改走简化版：

- 进程内 `ActiveRunRegistry`（`DashMap<run_id, RunContext>`）暴露进行中 trace，只读端点查询，前端 3s 轮询；
- 实时性从秒级推送降为 3s 轮询——对几十秒到几分钟的 AI 分析过程人眼无感；
- 已知约束：内存注册表只能查本实例的 run。当前 AgentEventBus 本身就是进程内单实例架构，不引入新约束。

## 设计

### 1. 后端

**① RunReport 加 trace（事后轨迹持久化）**

- `ToolTraceEntry`（id / name / args / output:Option<String>）从 runner 挪到 `crates/core`，runner 引用 core 类型；
- `RunReport` 追加 `#[serde(default)] pub trace: Vec<ToolTraceEntry>`（向后兼容已落库旧 JSON）；
- `assemble_report` 从 `RunContextInner.trace` 搬入。随 `agent_runs.report` 落库——不加表、无迁移。

**② 活动 run 注册表 + 轨迹端点（进行中轨迹出口）**

- `crates/agent` 新文件 `active_runs.rs`：`ActiveRunRegistry`（`DashMap<run_id, RunContext>`），runner `execute` 入口注册、RAII guard 出口注销（panic/cancel 路径不泄漏）；注册表有界（上限 = 最大并发 run 数，超出拒绝注册并 warn，纯防御）。
- cloud 新增只读端点 `GET /agent-runs/{run_id}/trace`：
  - 注册表命中（进行中）→ 返回当前内存 trace + `live: true`；
  - 未命中 → 查 `agent_runs.report`（已结束）→ 返回持久化 trace + `live: false`；
  - 都没有 → 404。
- **run_id 时序修复**：investigating judgment 的 run_id 目前在 run 完成后才回填（`judgment_subscriber.rs:162`），进行中卡片无从轮询。改为 dispatch 调查 run 时（dispatch 方知道 judgment_id 的时刻）即回填；完成时的回填保留且幂等。

**③ narrative（终端用户大白话）**

- verdict JSON 协议块加必填 `narrative` 字段（"我检查了温度曲线，发现 2 点后持续超阈，建议……"），提示词模块加说明 + 宪法段约束；
- `parse_verdict` 解析；`judgments` 表加列 `narrative TEXT`（一个小迁移）；
- 宽松 fallback / 老数据 → NULL，前端降级显示现有 reason。零额外 LLM 调用。

**④ evidence_json 带 trace**

`judgment_subscriber.rs:428` 组装点加 `trace: report.trace`——brain-event detail 端点直接出事后轨迹，前端不用 join agent_runs。`output` 字段进 evidence 前截断到 2KB/条（防御 LLM 读了大遥测 blob；trace 条数本身被 `MAX_TOOL_CALLS_PER_RUN` 天然限界）。

**改动清单**：`crates/core/src/agent_runs.rs`（挪类型+加字段）、`crates/agent/src/runtime/thing_agent/runner.rs`（注册/注销+搬 trace）、`crates/agent` 新文件 `active_runs.rs`、`apps/cloud/.../judgment_subscriber.rs`（narrative+trace 进 evidence、run_id 提前回填）、1 个 judgments 加列迁移、提示词模块、1 个 HTTP handler。

### 2. 前端两层展示

**卡片层（dispositions.ts 改造）**

- `investigating` 卡片变为可展开：展开后启动 3s 轮询 `/agent-runs/{run_id}/trace`，渲染进度时间线——每个工具调用一行（图标 + 工具名中文映射 + 参数摘要），`output` 未到的显示进行中 spinner；收起或卡片变终态时停轮询。
- 终态卡片摘要区显示 `narrative`（NULL 降级为现有 reason）。
- SSE 通道不动（仍靠 judgment_judged/judgment_updated 刷新信号 + 现有轮询/断连细条模式）。

**详情层（新视图）**

- 展开面板底部加"查看完整轨迹"→ 新路由 `#/ai-ops/runs/{run_id}`，全宽时间线：think 块（折叠）、每次工具调用的完整参数+输出（可展开 JSON）、✓/✗ 验证标记、尾部 verdict 原文、元信息（耗时/tokens/工具数）。
- 数据源：进行中走 trace 端点轮询；已结束走 `/brain-events/{id}` detail（evidence 已带 trace）。

**前端新建**：trace API 客户端、进度时间线组件（卡片层/详情层共用，详略两模式）、run 详情视图。

### 3. 错误处理

- **注册表泄漏**：RAII guard（Drop 注销）；cancel/panic 路径覆盖。
- **轮询竞态**：run 在两次轮询间完成 → 端点自动落到 report 分支返回 `live:false` + 完整 trace，前端收到 `live:false` 即停轮询并刷新卡片。无状态缝隙。
- **trace 端点 404**：run_id 错误或实例重启后查进行中的 run → 前端显示"轨迹不可用"并停轮询，不 crash。
- **narrative 缺失**：老数据/fallback → 降级 reason。
- **trace 体积**：条数受工具预算限界；output 截断 2KB/条。

### 4. 测试策略（先写失败测试）

- **crates/agent**：execute 期间注册表可查、trace 逐步增长；结束后注销且 report.trace 完整；cancel 路径也注销。
- **crates/db**：迁移测试——judgments 加列后旧行 narrative 为 NULL。
- **cloud handler 测试**：trace 端点三分支（live / 持久化 / 404）；dispatch 时 run_id 提前回填、完成时幂等再回填。
- **judgment_subscriber 测试**：verdict 带 narrative → 落列；evidence_json 含 trace（output 截断生效）；fallback 无 narrative 不炸。
- **前端 vitest**：investigating 卡片可展开 + 轮询启动/停止；`live:false` 停轮询；时间线组件两模式渲染；narrative 缺失降级 reason。
- **提示词**：现有 verdict 协议测试夹具加 narrative 必填断言。

## 非目标（YAGNI）

- 用户指令对话链路的过程展示；
- SSE 实时推送轨迹（轮询已够）；
- 独立轨迹表（随 report 落库已够）；
- 轨迹的跨实例查询、长期归档策略（沿用 agent_runs 现有生命周期）；
- sms/email 通知通道（仍是打桩，不在本期）。
