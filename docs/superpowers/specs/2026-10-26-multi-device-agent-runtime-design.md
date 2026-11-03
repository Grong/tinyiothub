# 通用 Agent Runtime 与多设备协同工作台设计

- 日期：2026-10-26

- 分支：`e4-p2-declaration-closeout`

- 状态：架构、Task/Job/Watch、媒体、两栏工作台、错误恢复、验收六节已与作者逐节确认；
CEO HOLD SCOPE 评审与实施计划已完成；R2 follow-up A 已批准，待聚焦工程复审

- 上位文档：`docs/superpowers/specs/2026-10-20-hub-runtime-architecture-design.md`

- 定位：以通用 Task Runtime 为产品主线，把现有 Run Loop、Device Hub、事件账本和
实时投影连接成多设备协同闭环；不是 GoalWatch 管理页，也不是任何单一场景的
定制 Agent

## 1. 目标与非目标

### 1.1 目标闭环

```text
任意声明式设备/传感器/视频源
  → 统一 Device Hub 契约
  → 通用 Agent Loop 感知、推理、调用、观察并继续
  → 统一 Task / Run / Job / Watch / Event 事实
  → 通用两栏 Workbench 实时投影
```

设备类型只提供声明、数据和工具。Runtime、prompt 和 Workbench 不得出现
`if storm`、`if fire`、`if camera_model_x` 一类业务分支。

**通用性判据**：两套仅 manifest、声明和模拟数据不同的设备夹具，使用同一
Runtime、同一 prompt 框架和同一 Workbench 完成多设备 Task，核心代码与
prompt 零场景改动。

### 1.2 非目标

- 不做 GoalWatch、异常检测或场景包管理页；它们是后续扩展，不是本设计中心。

- 不为风暴、火情、园区或 Mars 编写专用 Agent、专用工具或专用卡片。

- 不做 AGH OpState 的模型 mid-turn 指令级恢复；首版交付 Task 级恢复，后者仍归 E5。

- 不在 Hub 内建 GPU 感知，不默认录制视频，不宣称 MHS/MOS 线上协议兼容。

- 不一次性替换现有 Runner、固定设备工具或 ABI v2；采用绞杀者演进。

## 2. 参考与现状

### 2.1 AGH 参考事实

参考 `AgnesAI-Labs/agnes-harness` main 快照（2026-10-26）及作者提供的视频/截图：

- [docs/develop/architecture.md][agh-architecture]：Host 装配能力，Core 推进
model/tool loop，session/task/event 保存事实。

- [packages/mhs/server/hub-api.md][agh-hub-api]：固定设备工具、job/watch 唤醒、
快照、image/video 分流。

- [packages/mhs/plugin/client/src/cards/cards.tsx][agh-cards]：通用
`read_device/call_device` ToolCard；运行中的调用通过 `toolUseId` 关联设备
job，图片嵌入结果。

- [layout/overview.tsx][agh-overview]：设备卡优先展示声明的图片/视频，
没有媒体时展示关键字段。

- [hosts/agnes/index.tsx][agh-host]：右侧 dock 可拖拽调宽、折叠/展开；
点击工具设备可打开并定位设备。

- [layout/flow.tsx][agh-flow]：MHS 控制通道与 MOS 数据通道的总线、端口和粒子。

[agh-architecture]:
https://github.com/AgnesAI-Labs/agnes-harness/blob/main/docs/develop/architecture.md
[agh-hub-api]:
https://github.com/AgnesAI-Labs/agnes-harness/blob/main/packages/mhs/server/hub-api.md
[agh-cards]:
https://github.com/AgnesAI-Labs/agnes-harness/blob/main/packages/mhs/plugin/client/src/cards/cards.tsx
[agh-overview]:
https://github.com/AgnesAI-Labs/agnes-harness/blob/main/packages/mhs/plugin/client/src/layout/overview.tsx
[agh-host]:
https://github.com/AgnesAI-Labs/agnes-harness/blob/main/packages/mhs/plugin/client/src/hosts/agnes/index.tsx
[agh-flow]:
https://github.com/AgnesAI-Labs/agnes-harness/blob/main/packages/mhs/plugin/client/src/layout/flow.tsx

本设计借用这些**通用信息结构与运行时语义**，不复制其代码，也不虚构协议兼容。

### 2.2 现有复用与真实缺口

复用：

- `crates/agent`：Runner、RunReport、TurnEvent、固定接缝。

- 固定设备工具：`list_devices/read_device/call_device/set_device/stop/watch/unwatch`。

- `crates/runtime`：LiveState、WatchEngine、声明校验、资源仲裁、DataServer、Hub stream。

- `hub_ledger`、EventBus、E3 Flow/World 投影和 Lit 前端。

缺口：

- 多次 Run 之间缺少持久 Task 主线，唤醒容易成为孤立 run。

- 设备调用入队即返回 `executed`，缺少跨 Run 的 job 等待、结果唤醒和进度投影。

- `task/run/tool_call/job/device/source` 缺少稳定关联。

- Source 只有数值属性，缺少通用 image/video/detection 与证据契约。

- 工作台主要是设备卡与事后 Flow，缺少以 Agent Loop 为主的实时任务视图。

- 已确认的 E4 缺口必须随主链修复：声明 timeout 未进入执行、stop 未覆盖已排队命令。

## 3. 总体架构与边界

```text
┌─ Workbench / Chat / SDK ─────────────────────────────┐
│  提交任务、审批、停止、订阅 Task/Devices/Flow/World   │
└──────────────────────┬───────────────────────────────┘
                       │
┌─ Task Runtime ───────▼───────────────────────────────┐
│ Task Registry：目标、状态、当前 Run、等待关系           │
│ Wake Router：job/watch/event → 原 Task                │
│ Run Loop：Reason → Act → Tool → Result → Continue     │
│ Seams：Budget / Approval / Policy / ToolExec / Trigger │
└──────────────────────┬───────────────────────────────┘
                       │ 固定设备工具，不含场景分支
┌─ Device Hub ─────────▼───────────────────────────────┐
│ 声明注册 / LiveState / Jobs / Watch / Sources         │
│ 参数校验 / 资源仲裁 / 超时与取消 / 图像与视频入口       │
└──────────────────────┬───────────────────────────────┘
                       │ 统一关联事实
┌─ Event Ledger ───────▼───────────────────────────────┐
│ task_id / run_id / tool_call_id / job_id / device_id  │
│ source_id / seq / time / evidence_ref                 │
│ → Task 时间线、Devices、Flow、World、历史回放           │
└──────────────────────────────────────────────────────┘
```

边界规则：

1. `core` 只保存共享值契约；不得有 I/O 或业务编排。
2. `runtime` 持有 Device Hub、LiveState、Job、Watch 和媒体机制；持久化经 ports。
3. `agent` 持有 Task/Run/Trigger/循环内核；不得依赖 db/sqlx。
4. `apps/cloud` 是组合根：Task 编排、DB 适配、HTTP/SSE、播放描述与权限。
5. `db` 拥有全部 SQL 与新表；`web` 只消费投影，不直接写设备或查询实时真相。
6. Agent 不直接访问设备；所有感知、读图、调用、停止和订阅都经 Device Hub。

## 4. Task Runtime

### 4.1 Task 与 Run

- **Task 是持续任务，Run 是一次推进。** 一个 Task 可因 job 结果、watch 或设备事件产生多个 Run。
- Task 状态：`active | waiting_job | waiting_watch | needs_approval | paused |
cancelling |
completed | failed | cancelled`。

- 人工 `paused/cancelling` 优先于其他非终态；未暂停时，等待状态按
`needs_approval > waiting_job > waiting_watch > active` 归并，
因而同一 Task 等待多种事件也有确定含义。

- Run 终态单独包含 `interrupted`，用于进程重启；Task 不因此直接失败。

- Task 的 `state_version` 每次状态迁移递增，DB 使用 CAS 防止旧 Run 覆盖新状态。

- Task 终止后，其 watch 被清理；关联历史 job、Run 和证据保留用于审计。

### 4.2 一次推进

```text
创建 Task(objective)
  → 启动 Run(task_id, run_id)
  → 注入 Task 状态 + 设备快照 + 未完成 job/watch
  → Agent 经固定工具感知和调用
  → 每个 ToolCall 获得稳定 tool_call_id
```

每次 Run 的输入必须包含该 Task 已关联设备、未完成 job/watch 和最近的阶段结果。快照只含文本与媒体元数据，不自动包含图片或视频。

### 4.3 完成、审批与取消

- Run 正常结束（模型给出阶段结论且未报告失败）且没有
pending job/watch/approval → Task `completed`；Run 失败、无效输出或预算耗尽
按失败/重试规则处理，不得进入 `completed`。

- 仍有等待项 → Task 保持对应等待状态，最终文本只作为阶段输出。

- `paused` 保留 job/watch 与事件记录但不再启动新 Run；恢复时按当时等待关系
回到 `waiting_job`、`waiting_watch`、`needs_approval` 或启动恢复 Run。

- 审批请求 → `needs_approval`；批准、拒绝、过期、不可用分别处理，批准后沿原 Task 继续并使用批准时权限。
  2026-10-27 R2 后续决策选择 A：审批必须持久化，展示与执行共享同一份冻结的
  `tool/args_json`；`approve` 的 pending decision CAS 与唯一决策/执行 outbox 在同一
  SQLite 事务提交，`reject` 只写决策事实，不创建设备执行。恢复器按 `approval_id`、
  `tool_call_id`、`decision_event_id` 和既有 Job/outbox 唯一约束续跑，不调用旧 DashMap
  或直接同步执行。详细事务与崩溃矩阵见 R2 的“Follow-up resolution”。

- 用户暂停 Task 与取消 Task 不同：暂停只停止后续推进；取消先进入 `cancelling`，
先 abort 当前 Run 的 CancellationToken 并阻止任何新工具派发，再取消所属 watch、
将 pending outbox 标 dead，并停止全部所属活跃 job。
所有相关 job 都有明确终态后才迁移到 `cancelled`；任一 `cancelled_unknown`
则迁移到 `failed` 并记录 `cancel_unknown`，不得显示已全部停止。

取消接线必须落到现有 Run seam：`Runner::execute` 接收调用方持有的同一个
`CancellationToken`，不再只在内部创建私有 token；`ActiveRunRegistry` 将该 token 与
RunContext 一起登记，`manager` 在绑定 `agent_tasks.current_run_id` 后可按 Task 找到并 abort。
token 继续传入 rig tool loop，工具派发前再检查 Task cancelling/token，关闭“模型已返回
下一工具、取消尚未来得及落库”的竞态窗口。Run 结束后清理登记，迟到完成只能经
`state_version` CAS 更新投影。Pause 不取消该 token，也不调用此 abort 路径。

- 重复取消幂等，不重复发出 stop；terminal Task 的迟到事件只更新投影，不再唤醒。
`Stop all` 仍是独立的设备级操作，不与 Task 取消混用。

### 4.4 Device Job

每次设备调用创建 Hub 生成的稳定 `job_id`，并关联：

```text
task_id / run_id / tool_call_id / job_id / device_id
```

状态：`accepted | running | done | failed | rejected | cancelled |
cancelled_unknown | interrupted`。`accepted/running` 为活跃态，
其余为终态；每个 job 最多一个终态。首版不提供 pause/resume。

执行顺序：

1. Hub 完成声明校验、参数校验、资源 claim 和 timeout 钳制。
2. 持久化 job `accepted` 后再向 worker 派发，保证返回 `running` 前可追溯。
3. `accepted/progress/result/rejected` 全部落账；progress 仅表示 accepted/running 状态，
不含百分比或 done/total，只更新投影，不唤醒模型。
4. 声明的 `ValidatedCall.timeout_ms` 必须成为真实执行 deadline，覆盖排队与在途阶段。
5. 工具等待窗口为 `min(50s, 剩余 Run 预算, job deadline)`；
完成则直接返回结果，窗口耗尽但 job 仍活跃则返回
`{status:"running", job_id}`。
6. job deadline 先于等待窗口到达时，返回真实 timeout/cancelled 结果，而不是 `running`。
7. 最终结果唤醒原 Task；重复终态只落一次事实、最多唤醒一次。

单 Job Stop 先在 DB 校验 workspace、Task 与 job 归属；仅对 `accepted/running` CAS 写入
`stop_requested_at`，非所属 job 返回 404，终态或重复 stop 返回当前投影，不重复发取消。
`WorkerCommand::Execute`、worker 的 CancelSlot 与 `WorkerResult::Cancelled` 全程携带同一
`job_id`：queued 在领取时若已请求停止则直接形成 `cancelled` 终态且不调用驱动；in-flight
只在当前 job_id 匹配时取消对应 token，结果按该 job_id 回写。终态竞态只允许一个终态，
同设备其他 Task 的 job 与后续 claim 不受影响，禁止用 `cancel_calls(thing_id)` 或设备级
`set_stop` 冒充单 Job Stop。重启按持久 stop 请求重断言可恢复的 job 取消；没有可恢复
driver 调用时沿用 4.6 的 `interrupted` 恢复，不静默重新派发或假称 `cancelled`。

迟到结果保留账本但按 `job_id` 去重。`cancelled_unknown` 表示物理结果未知，
Agent 重试前必须先读设备状态，禁止盲目重复副作用。

### 4.5 Task-owned Watch

- Agent 可为 Task 注册 watch，包含 `owner_task_id`。
- upsert 与启动重织必须校验 `rule.workspace_id == owner_task.workspace_id`，
  `outbox.signal.workspace_id == task.workspace_id`；跨区不加载、不派发并记录拒绝。
- Task 的 `session_key` 对应会话也必须属于同一 workspace，禁止把其他工作区会话或审批关联进来。

- `persistent` 继续表示“是否免超时”，不表示是否可重启恢复。

- Task-owned one-shot watch 可持久保存并参与启动重织；命中、超时或失效后删除并唤醒原 Task。

- 非 terminal Task（含 `active`、等待态与 `paused`）的 watch 在重启后重建；
terminal Task 的 watch 不重建。

- 原有临时 session watch 保持不持久语义。

- watch 求值仍是确定性比较器，不运行模型。

### 4.6 Wake Router 与 Task 级恢复

唤醒以 `task_id + event_id` 幂等去重。Task 状态迁移与 `task_wake_outbox`
写入必须在同一 DB 事务中完成；dispatcher 在提交后投递 pending 信号。
`agent_runs` 另以 `wake_event_id` 做第二道去重，覆盖“已投递、未标记”后的
崩溃重试。

- job 最终 result、watch 命中/超时/失效、明确设备错误可唤醒 Task。
- `RunStarted` 与 `RunReport` 必须携带同一 `wake_event_id` 并写入 `agent_runs`；
  只在 outbox 有 ID 而未进入 RunReport 时，唯一约束不得视为已生效。

- job progress、普通遥测和视频帧只更新投影。

- 唤醒到达时，若 Task 正在 Run 中，作为 steer 输入；若空闲，创建下一 Run。

- `paused` 只记录事件和投影，不启动或 steer Run；恢复后按当时等待关系处理已到期事件。

- 重启时，未结束模型 Run 标记 `interrupted`。Task 根据持久等待关系恢复；无等待项的 active Task 创建恢复 Run。

- 对重启前仍活跃的 job，首版若没有可恢复的 driver 调用，将其标记 `interrupted` 并唤醒 Task；不得静默重复派发。

- 迟到的终态若与已持久化 job 匹配，只按幂等规则处理一次。

首版不提供模型 mid-turn 恢复，也不承诺外部副作用 exactly-once。

## 5. Source、媒体与证据

### 5.1 声明扩展

现有 `sources[].kind` 继续表示采集方式，新增可选 `dataKind` 表示内容类型，默认 `values` 兼容现有驱动：

```text
values | detections | image | video | x_<custom>
```

- `values`：状态与遥测。

- `detections`：标签、置信度、区域等结构化事实；由驱动或外部感知服务产生。

- `image`：Agent 按需读取的低频图片。

- `video`：供人观看的实时流。

- `stateFields[].key` 可选；设备卡优先展示 key 字段，缺省取前三个字段。

新增字段将 Hub 当前 `manifestVersion` 升至 `1.1`；`1.0` 声明按默认值解释，
旧消费者忽略新字段，新消费者不得要求存量清单补写 `dataKind` 或 `key`。

未知 `dataKind` 不使 Hub 崩溃；运行时和 UI 保留原始元数据并以通用详情展示。

### 5.2 SourceItem

统一版本化信封：

```json
{
  "v": 1,
  "source": "front",
  "dataKind": "image",
  "seq": 812,
  "time": "2026-10-26T10:00:00Z",
  "text": "optional untrusted summary",
  "data": {},
  "media": null,
  "error": null
}
```

- 小型结构化数据放 `data`。

- 图片通过 `media` 携带受限二进制或引用，Hub 校验后写入 MediaStore。

- 视频通过 `media` 携带播放描述；播放描述只供 Hub 使用，不把驱动凭证返回浏览器。

- ABI v2 不变：现有 `read_async` 的版本化 JSON 可承载这些字段，`set_demand` 继续控制采集需求。

### 5.3 Agent 感知通道

- turn 快照只带 Source 元数据。

- `read_device(sources=[...])` 才读取 detections 或图片。

- detections 和设备文本都按不可信内容围栏后进入模型。

- 图片仅 JPEG/PNG，单张硬上限 2 MiB，并校验声明新鲜度；每次工具调用最多一张，每个 Run 的图片字节预算为 8 MiB。

- ToolResult/TurnEvent 可携带图片附件；trace 只保存 `evidence_id/mime/seq/time`，不复制 base64。

- `ModelProvider`、`ChatMessage`、ToolResult 和 rig 适配器必须端到端保留结构化图片附件。
当前 `provider.rs` 对多媒体块静默跳过与 `tools.rs` 仅输出文本的行为必须修正，
不得用 UI 图片展示冒充模型已读取。附件传递、证据引用和非 vision 失败均需 roundtrip 测试。

- 模型适配器声明支持 vision 时才把图片作为附件输入；不支持时返回 `image_unavailable`，禁止生成“已看到图片”的描述。

- 读取 `video` 返回 `video_is_for_human_not_model`。

### 5.4 Workbench 视频通道

- 首版支持 MJPEG 与 HLS；MJPEG 使用图片帧播放，HLS 使用原生播放器或 hls.js。

- **浏览器直连播放**：驱动/上游必须提供浏览器可直接访问的短时签名播放 URL。
Hub 只返回脱敏 playback descriptor（protocol/url/expiresAt），不代理视频字节，
不重写 HLS 清单，也不转发驱动凭证。

- 签名 URL 必须有短有效期并按 `workspace/device/source` 限定；Hub 日志只记录
origin 和过期时间，签名 URL 仅允许 `https`（本机开发允许 `http://localhost`），拒绝 `file/javascript/data`
与 userinfo（`user:pass@host`）；query/signature 一律脱敏。descriptor 端点本身使用 5 分钟票据，
到期前由主会话刷新。

- CORS、混合内容、网络不可达、上游不支持签名 URL 或协议不支持时，显式返回
`playback_unavailable`；不得回退 Hub 代理、显示静态图冒充实时视频。

- 视频只在用户观看时启用，离开设备、切换 Task 或超时后停止并 `destroy()` 播放器。

- 首版 playback descriptor 不新增 captions 轨；保留原声与原生音量控制，播放器
  区域明确 `Captions unavailable`，不根据视频猜测有无语音，也不宣称自动转录。
  含语音证据的字幕可达性作为 deferred 缺口保留。

- 视频不默认录像，历史回放展示保存的图片证据与设备状态。

### 5.5 证据

每个证据引用携带：

```text
task_id / run_id / tool_call_id? / job_id?
device_id / source_id / seq / time / evidence_id
```

- 图片二进制进入 `MediaStore`，首版本地磁盘实现；路径为 `data/media/<hex(workspace_id)>/<sha256>`。

- DB 保存元数据、哈希、关联 ID 和过期时间；Hub ledger 只存引用。

- 图片证据默认保留 7 天；过期后显示“证据已过期”，不显示加载失败或另一张图。

- MediaStore 故障只影响相关媒体读取，不影响其他感知和设备控制。

## 6. 事件、持久化与 API

### 6.1 关联事件信封

Runtime 事件使用 `runtime.v1` payload：

```text
event_id / event_type / occurred_at / workspace_id
task_id? / run_id? / tool_call_id? / job_id?
device_id? / source_id? / source_seq? / evidence_id?
data
```

- 控制、job、watch、Task 状态使用有序事件。

- 遥测与视频使用 latest-wins，不与控制事件争夺队列。

- Task 时间线分页；SSE 使用 cursor 重连和 resync，重复 event_id 不得产生重复步骤。

### 6.2 新表与字段

#### `agent_tasks`

```text
id, workspace_id, idempotency_key, objective, session_key, created_by, status,
current_run_id,
state_version, created_at, updated_at, ended_at
```

索引：`(workspace_id, status, updated_at)`；唯一索引 `(workspace_id, idempotency_key)`。
`session_key` 用于 Chat 与 Workbench 指向同一 Task，API 创建时可为空；
`created_by` 记录创建者供审批与审计使用。

**`agent_runs`**：新增可空 `task_id` 与 `wake_event_id`；分别为其建索引，
并对非空 `(task_id,wake_event_id)` 建唯一约束，旧行保持 NULL。

#### `task_wake_outbox`（持久唤醒与去重）

```text
task_id, event_id, signal_json, dispatch_state,
created_at, dispatched_at, resolved_at
```

主键 `(task_id,event_id)`；`dispatch_state` 为 `pending | dispatched | dead`。
dispatcher 扫描 pending，崩溃后重试；连续失败按现有 DLQ 模式处理并保留告警。

**`watch_rules`**：新增可空 `owner_task_id` 与索引。

#### `device_jobs`（活跃与查询投影，生命周期事实仍在 ledger）

```text
id, workspace_id, device_id, tool, args_json, state, stop_requested_at,
progress_json, result_json, task_id, run_id, tool_call_id,
driver_call_id, started_at, ended_at
```

约束：`tool_call_id` 唯一；索引 `(task_id, state)`、
`(workspace_id, device_id, state)`。人工 UI 直接调用产生的 job 允许
`task_id/run_id/tool_call_id` 为 NULL；SQLite 的 UNIQUE 不限制多个 NULL。

#### `task_approvals`（持久决策与唯一执行信箱）

```text
id, workspace_id, task_id, run_id, tool_call_id, device_id, tool, args_json,
reason, state, decision, decider_id, requested_at, decided_at, expires_at,
decision_event_id, updated_at
```

`state = pending | approved | rejected | expired | unavailable`；`decision` 仅在实际
approve/reject 时写入。`tool_call_id` 与 `decision_event_id` 分别唯一，审批请求创建时
即分配后者；索引 `(workspace_id,task_id,state)`、`(expires_at,state)`。`args_json` 是
展示与执行共同使用的冻结参数，批准后禁止重新生成或由客户端提交替换。`decider_id`
记录批准时通过 Agent 管理权限校验的用户；执行恢复只引用该持久决策，不继承模型权限。

pending 请求与 Task `needs_approval` 同事务创建。approve/reject 使用条件 decision CAS，
并以预分配 `decision_event_id` 写原 Task 的决策 outbox；approve 的该 outbox 同时是唯一
执行 intent，reject 的 outbox 只传决策事实，不创建 Job。重复同 decision 返回当前投影，
不同 decision 409；过期、不可用、暂停、取消或终态 Task 不得新建设备执行。取消优先于
尚未 dispatch 的批准；已产生 Job 后仍走 Task cancel/Job Stop 与 `cancelled_unknown`。
批准后的 Job 插入与 Task `needs_approval → waiting_job` 必须在同一 DB 事务内完成，
该事务重新检查 Task/approval 状态；paused/cancelling/terminal 不得绕过此门插入 Job。
同 `tool_call_id` 只有在全部冻结执行字段一致时才能返回既有 Job，否则是执行冲突。
暂停导致 defer 时保留原 decision outbox pending，resume 重用原 event，不新建决策事件。

#### `media_evidence`

```text
id, workspace_id, task_id, run_id, tool_call_id, job_id,
device_id, source_id, source_seq, media_time,
mime, size_bytes, sha256, storage_path, dedup_key, created_at, expires_at
```

关联字段 `task_id/run_id/tool_call_id/job_id` 可空，便于设备独立图片复用同一存储；
Task 证据至少包含 `task_id`，Run 证据至少同时包含 `task_id/run_id`。
`dedup_key` 由规范化 JSON 元组 `[workspace,task,run,tool_call,job,device,source,seq,sha256]`
做 SHA-256；禁止用可能含分隔符的裸字符串拼接，避免 ID 碰撞。
重试只能复用已有证据，不能重复写行或重复附件。

所有新持久化都经 ports；capability crate 不得直接写 SQL。

### 6.3 主要 ports

- `TaskStore`：创建、读取、列表、CAS 状态迁移、当前 Run 绑定，以及“状态迁移 + outbox”事务和 pending 唤醒领取/标记。

- `DeviceJobStore`：accepted/progress/result 投影、活跃列表、按 tool_call/job 查询；
  校验 Task/job 归属并 CAS 写 `stop_requested_at`，提供重启时需重断言取消的活跃 job；
  批准执行时在同事务检查 Task/approval、插入原 `tool_call_id` Job 并迁移 Task 等待关系。

- `TaskApprovalStore`：创建 pending 与 Task 状态同事务、按 workspace/Task/approval 读取、
  条件 decision CAS 与决策 outbox 同事务、到期/不可用处置，以及恢复所需的 pending/approved
  记录查询；所有 SQL 留在 `crates/db`。

- `MediaStore`：写入受限二进制、读取、删除过期内容。

- `MediaEvidenceStore`：证据元数据与关联查询。

- 既有 `WatchRuleStore`：增加按 active Task 重织与清理。

- 既有 `HubLedger`：承载版本化事件与引用。

### 6.4 HTTP/SSE

复用现有 Task 创建入口，不另建重复 API：

- `POST /workspaces/{id}/agent/tasks`：`{text}` + 必需 `Idempotency-Key` → 持久 Task，
返回 `{taskId,status:"accepted"}`；缺 key 400，同 key 重放返回原 Task 且不重复 outbox，
不同 objective 复用 key 返回 409。

- `GET /workspaces/{id}/agent/tasks`：分页列表。

- `GET /workspaces/{id}/agent/tasks/{task_id}`：Task、Run、job、watch 与关联摘要。

- `POST /workspaces/{id}/agent/tasks/{task_id}/cancel`：幂等取消。

- `POST /workspaces/{id}/agent/tasks/{task_id}/pause` 与 `/resume`：幂等暂停/恢复；
  pause 只阻止后续新 Run，不 abort 当前 Run，不停止所属 job/watch。

- `POST /workspaces/{id}/agent/tasks/{task_id}/jobs/{job_id}/stop`：只停止该 Task
  所属的 accepted/running job，按 Task/job 幂等；queued/in-flight 都按 job_id 隔离，
  重复或已有终态返回当前投影，不取消整个 Task，不影响同设备其他 Task，不自动重放设备命令。

- `POST /workspaces/{id}/agent/tasks/{task_id}/approvals/{approval_id}/approve` 与
  `/reject`：按 Task/approval/decision 幂等，冲突决定返回 409；expired、unavailable
  或已有终态时返回当前投影，不重新授权或创建重复 Run。请求体为空，服务端从持久记录
  读取冻结参数；decider 来自已鉴权 Claims，客户端不能提交参数、decider 或执行 ID。
  **Implementation gate（2026-10-27 R2 follow-up，resolved）：** 采用持久 decision＋
  唯一执行信箱方案；实现与验收必须覆盖 decision CAS、事务回滚、双击、崩溃后续跑和
  不盲重放，不得复用旧 DashMap/heartbeat 的翻转后直接执行路径。

- `GET /workspaces/{id}/runtime/stream?taskId=...&after=...`：
snapshot/event/resync。

- `GET /workspaces/{id}/evidence/{evidence_id}`：鉴权图片证据。

- `GET /workspaces/{id}/devices/{device_id}/sources/{source_id}/playback`：
返回脱敏 playback descriptor（短时签名 URL、协议、过期时间），不代理视频字节。

- `GET /admin/runtime/outbox?state=dead`：管理员读取 dead 唤醒与失败原因。

- `POST /admin/runtime/outbox/{event_id}/retry`：管理员按原 event_id 重试 dead 信号，
不创建新事件，唤醒去重仍然生效。

- `GET /admin/runtime/jobs?state=active|unknown`：管理员读取活跃/未知 job 及设备状态。

Task 创建、暂停/恢复、取消、审批、设备调用和停止沿用 Agent 管理权限；读取
Task/Devices/证据沿用 workspace 权限，所有路径做 workspace 隔离。

## 7. 两栏 Agent Runtime Workbench

### 7.1 页面结构

新增 `#/runtime`（当前/最近 Task）、`#/runtime/new` 与 `#/runtime/tasks/{id}`，现有 AI 中心链接到该视图；不新增 Goal 页面。

`#/runtime` 是 Task 列表而非统计仪表盘：

- 顶栏只有 `Runtime` 标题、当前连接状态与 `New task` 主行动；不堆 KPI 卡片。
- `Current` 展示非终态 Task，`Recent` 展示终态 Task，两组均按最近活动倒序；
  每行显示 objective、Task 状态、等待/失败原因摘要、最近活动时间和关联设备数。
- 点击整行进入 `#/runtime/tasks/{id}`；空列表保留新建入口，分页沿用 Task API。
- 详情页提供返回列表的 `Runtime` 链接，并在标题旁提供紧凑的 `Switch task`
  菜单；菜单复用 Current/Recent 投影，标记当前 Task，选择后切换路由。
- 切换 Task 后重建详情投影并释放旧 Task 的 SSE、媒体与播放器；不得把旧 Task
  的关联设备或实时事件带入新页面。

```text
#/runtime
├─ Runtime / New task
├─ Current → objective · status · waiting reason · last activity · device count
└─ Recent  → objective · terminal state · result summary · last activity
       ↓ select
#/runtime/tasks/{id}
├─ Runtime back link / Switch task / objective / Task status / task actions
└─ Agent Loop ↔ Devices
```

#### New Task 创建页

- `New task` 进入 `#/runtime/new`。页面为单列创建表单，H1 为 `New runtime task`，
  只有可见标签的 Objective 多行输入、持续 Task 说明、`Create task` 主按钮与
  `Cancel` 次按钮；不以 placeholder 充当标签，不增加场景或设备型号选择。
- 表单复用 `POST /workspaces/{id}/agent/tasks`，客户端为每个新 objective 生成
  Idempotency-Key；同一未修改输入的网络重试保留原 key，输入改变后使用新 key。
- 提交时显示 `Creating task` 并禁止重复提交；错误保留 objective 与可理解原因，
  400/409 不做静默重试。accepted 后以 replace 导航到 `#/runtime/tasks/{taskId}`，
  避免返回键再次提交；Cancel 在未提交时返回 `#/runtime`。

详情首屏只优先回答三件事：**任务要做什么、现在处于什么状态、需要用户做什么**。

- 第一层是 objective（页面 H1）、Task 状态和当前需要的干预；审批请求留在 Loop
  对应步骤，不在头部重复一个竞争主按钮。
- 第二层是当前 Run/等待原因、关联设备与证据；第三层才是统计和实现 ID。
- 头部提供 `Pause task`/`Resume task` 次按钮；`Cancel task` 放入溢出菜单并以
  危险操作样式呈现。历史或终态详情不显示可用的推进/取消操作。
- Task ID、Run ID、wake/event ID 与关联 ID 收入 `Details` 折叠区，不以 mono ID
  替代 objective 作为标题，也不占首屏主视觉。

```text
┌──────────────────────────────────┬─ seam ─┬──────────────────┐
│ Agent Loop（主视图）              │ 拖拽/折叠 │ Devices          │
│ Reason / Act / Tool / Result     │          │ Task 关联设备卡    │
└──────────────────────────────────┴─────────┴──────────────────┘
```

- 使用紧凑、主题感知的 ToolCard 信息流；默认深空暗色并完整支持 light 主题，
  不使用深色大仪表盘、Goal 卡片或固定浅色卡片孤岛。

- Loop 直接展示设备状态、工具结果、accepted/running 状态、图片和 `Done/Details`。

- 点击 Loop 的设备名或证据，右侧定位并高亮对应设备卡；折叠时先展开。

### 7.2 关联设备规则

只有以下行为才建立 Task 设备关联：

- `read_device`、`call_device`、`set_device`、`stop_device`、`watch_device`。

- job、watch 或 source evidence 已关联到 Task。

`list_devices` 只提供候选，不使设备出现在右侧。右侧按首次实际引用顺序展示卡片；切换 Task 时清空并重建关联集合。

### 7.3 通用设备卡

卡片内容只由声明与实时状态决定：

- 首个声明的视频源作为预览；无视频则使用首个图片源；无媒体则展示关键数值。

- 有视频时，卡片内同时提供最近图片证据入口。

- 展示 health、zone/position、Ready/Busy、job accepted/running 状态和关键字段；
不展示百分比或进度条。

- 点击卡片进入通用设备详情；不为设备型号创建专用组件。

### 7.4 Seam 与 Flow

- 主栏顶部使用 `Agent Loop` / `Flow` 两个同级标签，默认 Agent Loop；Flow 使用
  `?view=flow` 进入，浏览器前进/后退可恢复标签。右侧 Devices 在两种视图中保留。
- 切换标签保留 Task、历史游标和关联设备选择；复用同一 runtime 投影，不复制
  一套 Flow 专用事件顺序。Flow 默认不挤占 Agent Loop 的纵向空间。

- seam 支持拖拽调宽，最小 360px，最大视口 75%；折叠后为 36px 竖向 Devices rail。

- 中央按钮折叠/展开；键盘左右键每次调整 24px；宽度与展开状态本地保存。

- ≥980px 使用两栏与 seam；<980px 的单栏/标签导航及缩放重排见 7.6，不能只写
  “Devices 移到下方”而不定义发现路径。

- Flow 采用双通道视觉：`CONTROL`（对应参考截图 MHS）与 `DATA`（对应 MOS），不代表协议兼容。

- CONTROL 展示 call/result 方向，DATA 展示数据源粒子；设备端口与卡片位置一致。

- `prefers-reduced-motion` 下停用 DATA 粒子、live pulse、spinner、平滑滚动与
  非必要过渡，以静态总线、图标和状态文本替代；不是把动画放慢。Following latest
  改为即时定位，用户主动播放的视频仍保留原生控制，不自动播放。

### 7.5 状态与可访问性

覆盖 loading、empty、error、offline、partial、expired evidence、
unsupported media、waiting、approval、cancelled。

设备链接、详情、折叠、停止均可键盘操作；seam 有 separator 语义和宽度说明；视频有暂停与断线状态；图片有可理解的替代文本。

#### Live 与历史模式

- 复用 `at`/`event` 时间游标；在 Flow 选择历史事件进入历史模式，仅选择 device
  不进入历史。Agent Loop 与 Flow 通过同一 URL 游标保持一致。
- 历史模式在头部下方持续显示 amber `History · read-only` 横幅、事实时间和
  `Back to live`；不使用仅有小 chip 或颜色变化表达模式，也不与 stale/offline
  混为同一提示。
- 历史投影不合并新事件；移除 Pause/Cancel/Approve/Reject/Stop 等可执行操作，
  但保留有效的详情与证据查看。过期媒体仍按证据状态说明，不伪装成实时可用。
- `Back to live` 清除 at/event、保留当前 Task 和 view 标签，经 snapshot/resync
  回到最新状态；切换 Task 清除旧 Task 的历史游标。

#### Task 暂停与恢复

- `Pause task`/`Resume task` 是可逆操作，点击后直接提交，不使用危险确认框；
  本地 pending 显示 `Pausing…`/`Resuming…` 并禁止重复提交，不新增 Task 状态。
- Pause 旁持续说明 `Stops new runs; active jobs and watches continue`；不得暗示
  当前 Run 被 abort 或设备动作已停止。需要停止副作用时使用独立 Job Stop/Task Cancel。
- 以服务器投影显示 paused 及恢复后的 waiting/active 状态；请求失败保留原状态和
  可见错误。历史或终态详情不提供暂停/恢复操作。

#### Task 取消交互

- 从溢出菜单选择 `Cancel task` 后打开确认对话框：显示 objective、当前活跃
  job/watch 数量与设备名，并说明取消会停止所属副作用、未知结果仍需核查。
- 主按钮为 `Keep running`，危险按钮为 `Cancel task`；确认前不调用取消 API。
- 确认后禁用重复提交并显示 `Cancelling`；允许关闭对话框，但头部保留持续
  取消状态，关闭不代表撤销取消。
- 全部所属副作用得到确定终态后才显示 `Cancelled`；任一 `cancelled_unknown`
  显示 `Cancellation needs attention`、未知设备与 `Refresh status`，不得显示
  `All stopped` 或自动承诺重试成功。

#### Device Job 停止交互

- 只有当前投影中的 accepted/running job 在所属 ToolCard 显示 `Stop job`；
  历史、终态或非所属 job 不显示可执行停止操作。
- 点击后在卡内展开确认区，显示目标设备与动作，提供 `Keep job` 和危险操作
  `Confirm stop`；文案明确只停止该 job，不取消整个 Task。
- 确认后显示 `Stopping` 并禁止重复提交，以设备侧事实更新为 done/cancelled/
  failed；`cancelled_unknown` 显示 `Stop result unknown` 与 `Refresh status`，
  不显示安全成功，也不自动重试。

#### 审批交互

- 审批请求内联在发起它的 Run 步骤，显示具体动作、目标设备、参数摘要、原因
  与有效时间；`Review details` 展开完整参数和相关证据，不弹离当前上下文。
- `Approve once` 是唯一主按钮，`Reject` 为次级操作；默认不预批准、不自动提交。
- 任一提交后两个按钮进入 pending 并禁止重复提交，以服务器投影为准显示
  approved/rejected；本地不得先改写为已批准。
- expired/unavailable/rejected 保留原请求与结果说明，移除可用授权按钮；
  切换 Task 不得把 pending 提交状态或审批上下文带到另一 Task。

### 7.6 响应式与缩放

- **≥980px 桌面：** 保持 Agent Loop/Flow 主栏 + Devices 两栏、seam 与 36px
  折叠 rail；Task 列表使用整行投影。
- **600–979px 平板：** 隐藏 seam，页面按头部 → Agent Loop/Flow 主标签 →
  Associated devices 顺序单栏排列；Devices 标题保持可见锚点，不做悬浮小窗。
- **<600px 手机：** 详情顶部使用 `Loop`、`Flow`、`Devices` 三个同级标签，一次
  展示一个视图；Devices 显示关联数量，Loop 中定位设备时切到 Devices 并聚焦目标。
  头部操作换行或进入溢出菜单，objective 不截断成不可理解的一行。
- **<400px 与 Task 列表：** 不新增另一套组件；列表行改为 objective/status 优先的
  两行布局，次要元数据换行，禁止横向表格滚动。所有触摸目标至少 44px。
- **200%/400% zoom 与文字放大：** 按实际可用 CSS 宽度落入相同断点；正文、审批和
  操作不得要求双向页面滚动。Flow 图形可局部平移，但必须同时提供等价列表视图，
  关键事实不能只存在宽画布中。

### 7.7 键盘、焦点与滚动

- 页面使用唯一 H1、`main` 与有名称的导航 landmarks。Loop/Flow/Devices 标签采用
  `tablist`/`tab`/`tabpanel`、roving tabindex、左右键与 Home/End；选中态同时
  反映在 `aria-selected` 和 URL，不只改变颜色。
- Switch task 与溢出菜单支持方向键、Esc 和焦点归还。危险对话框默认聚焦
  `Keep running`/`Keep job`，具有 role、aria-modal、名称、focus trap；关闭后
  归还触发控件，若触发控件已不存在则聚焦头部 Task 状态。
- 直接进入或切换 Task 后聚焦详情 H1（`tabindex=-1`）并从页面顶部开始；返回
  `#/runtime` 恢复原触发行与滚动位置。旧异步响应不得再次移动新页面的焦点。
- 从 Loop 定位设备时先展开 rail/切换手机标签，再滚动并聚焦目标设备卡；
  目标有可见焦点样式，不只短暂高亮背景。
- seam 使用可聚焦 `role=separator`、`aria-orientation=vertical`、
  aria-valuemin/max/now 与可读宽度；左右键 24px，Home/End 到最小/最大，
  折叠按钮有随状态变化的名称。Flow 保留现有时间键盘操作。
- 实时事件、审批状态和媒体重连不得抢焦点。只有用户原本处于 `Following latest`
  且停在末尾时才跟随滚动；阅读历史步骤时保持位置并显示 `N new events` 按钮，
  由用户主动跳到最新。 keyed 更新不得因重建节点而丢失当前焦点。

### 7.8 读屏与动态播报

- Run 时间线使用有序列表与步骤标题；状态有可见文本，装饰图标 `aria-hidden`，
  参数/结果使用可识别的 disclosure 名称。加载区域用 `aria-busy`，不靠 spinner
  文本反复播报。
- 全页一个 `aria-live=polite` 状态区，只播报新步骤、Task/Job 关键状态迁移、
  审批请求、连接状态变化和终态；按关联 event/wake ID 去重。cancel_unknown
  等需要立即核查的危险结果只 alert 一次。
- 不播报逐 token 输出、隐藏推理、每次遥测刷新、媒体 buffered 百分比或重复 SSE
  事件；历史浏览和 resync 重放不得把旧步骤当新事件再次播报。
- 信息图片 alt 包含设备、来源、采集时间和可见内容摘要；相同信息已在相邻文本
  出现时缩略图使用空 alt，避免重复。视频有可理解名称、键盘原生控制和暂停，
  不自动播放声音；首版无字幕轨时显示 `Captions unavailable`，错误与播放器通过
  `aria-describedby` 关联。
- Flow 列表回退逐项提供 from/to、CONTROL/DATA、状态和时间，与图形共享游标；
  关键关系不能只存在 SVG/canvas。时间 slider 保留 aria-valuetext 与事件时间。

## 8. 错误、安全与背压

- 模型失败、无效输出或预算耗尽形成明确 Run 结果；不得伪造推进。

- 审批过期、拒绝、不可用分别呈现，均不执行设备动作。

- busy 返回资源与持有者；设备删除/离线使 watch 失效并通知 Task。

- 设备文本、detections 和媒体元数据按不可信输入处理；媒体入口校验 workspace、MIME 和大小。

- 播放描述中的驱动凭证只留 Hub；浏览器只获短时签名 URL，Hub 不代理流量、不转发 Authorization。

- 原始视频不进 prompt、DB 或账本；progress 不唤醒模型。

- Task、Run、Job、Watch、设备和证据均以 workspace 隔离，跨区统一拒绝且不泄露存在性。

- 账本/媒体故障采用显式降级；不能把旧图片、旧遥测或旧状态当成实时事实。

## 9. 验收与测试

### 9.1 通用性

- 夹具 A：数值遥测 + 图片 + 视频 + 可执行动作。

- 夹具 B：数值遥测 + detections + 不同工具与资源，无视频。

- 同一 Runtime、prompt 和 Workbench 各完成一个多设备 Task，核心与 prompt 零场景改动。

### 9.2 核心端到端

1. **持续推进**：数值感知 → 图片 → 调用 → job accepted/running 状态上面板
→ result 唤醒原 Task → 验证 → completed；完整断言关联 ID。
2. **Watch**：注册后等待；命中唤醒原 Task；重复事件不重复推进；重启重织。
3. **单 Job Stop**：stop 覆盖在途与排队；同设备两个 Task 互不误停，重复 stop 幂等，
`cancelled_unknown` 后先读状态再决定重试。审批 pending/approve/reject/unavailable 及其
崩溃恢复验收因 R2/D2=C 暂列 blocked，不进入当前可执行测试任务。
4. **重启恢复**：accepted 后、result 前重启，不重复执行设备动作；Task 恢复，
迟到结果最多唤醒一次。
5. **媒体**：大小/类型/新鲜度、非 vision、视频需求生命周期、断流重连、证据过期
全部有明确结果。
6. **Workbench**：关联设备出现规则、Loop 定位、seam 拖拽/折叠/键盘、Flow、
减少动态效果、图片/视频、SSE resync、历史只读。
7. **性能与隔离**：测试环境 watch 命中到 Run 入队 `<2s`，job 状态更新到面板
`<1s`；所有跨区访问拒绝。

### 9.3 测试分层

- 纯函数：Task 状态机、条件求值、关联 ID、幂等、媒体校验。

- 集成：真实 SQLite + EventBus + worker + ledger + 重启恢复。

- 前端：Lit/Vitest + 浏览器验证 seam、联动、媒体和键盘路径。

- 模型 eval：声明驱动的通用多设备任务并比较基线；测试场景不写入 prompt。

- 全量门禁：Rust fmt/clippy/workspace tests；Web type-check/vitest/build。

## 10. 交付切分

1. **Task + Job 主链**：Task 持久化、job 生命周期、结果唤醒、关联账本，并修复 timeout/stop 缺口。
2. **Source/Media + 两栏 Workbench**：图片/视频、实时投影、关联设备卡、seam/Flow。
3. **恢复与通用性收口**：重启、幂等、第二套设备夹具、模型 eval 与全量回归。

每段独立可回滚；不得把某个场景、GoalWatch 或管理页当成独立产品交付。

## CEO Review — Step 0 working record

- 基线：`main`（GitHub）；工作文档为本文件。旧 `docs/designs/ai-brain-p0-pr99-merge-gate.md`
仅作历史参考。

- UI 范围：Section 11 必需。PR #101 仍 open；五份旧 stash 均不相关，保持不动。

- 相关历史：E1–E4 已提供 Runner、LiveState、固定工具、Watch、声明/仲裁、账本和
前端投影。GoalWatch/E1d 仍是独立后续项，不在本设计内重新启动。

- 反复问题：阶段完成声明与真实主链不一致。本评审优先验证 Task/Job/Watch 的真实
调用链、持久事实和恢复语义。

### 0A. Premise Challenge

真实问题不是缺少 Goal 管理，而是设备感知、模型推进、设备执行和现场证据彼此断裂。
do-nothing 的结果是：调用入队就被当作执行完成，后续 Run 无法可靠继承任务上下文，
用户也看不到 Agent 为什么操作某台设备及其真实结果。本设计直接处理该主链，不是代理功能。

### 0B. Existing Code Leverage

- Task/Run：`crates/agent/src/runtime/thing_agent/runner.rs`、`manager.rs`、RunReport。
Task/Run：`crates/agent/src/runtime/thing_agent/runner.rs`、`manager.rs`、RunReport。
- 设备工具与仲裁：`apps/cloud/src/domains/agent/host/tools/thing/`、`crates/runtime/src/access/`。
设备工具与仲裁：`apps/cloud/src/domains/agent/host/tools/thing/`、`crates/runtime/src/access/`。
- Watch/重织：`crates/agent/src/runtime/thing_agent/watch_loop.rs`、`crates/runtime/src/watch/`。
Watch/重织：`crates/agent/src/runtime/thing_agent/watch_loop.rs`、`crates/runtime/src/watch/`。

- 事件/投影：`crates/runtime/src/hub_stream.rs`、hub_ledger、E3 Flow。

- API/装配：`apps/cloud/src/domains/agent/host/handler/agent_tasks.rs` 与组合根。

- 反例不得复制：`composite.rs` 的 partial 状态错误、`invoke_action.rs` 丢弃 timeout。

没有新建第二套 Runner 或第二套设备事件库的理由；需要补的是 Task 主线、job 语义和关联。

### 0C. Dream State Mapping

```text
当前：声明/Watch/Runner/账本各自可用，但多设备任务断裂
  → 本设计：Task 持续驱动多设备，job/source 证据关联，Loop+Devices 两栏可见
  → 12 个月：任意设备按声明接入即可获得通用协同、可恢复执行与可解释现场运维
```

### 0D. Approach Decisions

路径已在设计阶段选择“绞杀者通用 Runtime”（2026-10-26 用户实际回答），因此本轮不
重开“重写内核 vs 绞杀者”的选项。若后续证据推翻该前提，按 reopened 单独裁定。

### 0E. Mode Inputs

当前设计含 Task+Job、Source/Media+Workbench、恢复与通用性三切分。按模块粗估会触及
超过 15 个文件 `[estimated]`，需要按规则推荐 SCOPE REDUCTION 并显式裁定第一波范围；
这不改变三切分均已获设计确认的事实。

### Decision ledger

- ID and owner: D0-path / architecture
- Contract and evidence: 用户 2026-10-26 选择绞杀者
- Current: 新 Core 或仅 UI 均未选
- Proposed: 复用 Runner+Hub，补 Task 主线
- Status: approved
- Exact approval and scope: 不重写内核；Task 级恢复，非 OpState

- ID and owner: D0-task / runtime
- Contract and evidence: 用户确认 Task/Job/Watch 节
- Current: 孤立 Run、入队即 executed
- Proposed: 持久 Task，job 结果回原 Task，progress 只上投影
- Status: approved
- Exact approval and scope: Task-owned watch 可重织；幂等/outbox

- ID and owner: D0-media / runtime+UI
- Contract and evidence: 用户确认媒体节
- Current: 数值属性为主
- Proposed: `dataKind`、image/detection/video、证据引用
- Status: approved
- Exact approval and scope: 模型读图、人看视频；ABI v2 不变

- ID and owner: D0-ui / presentation
- Contract and evidence: 用户确认两栏+seam+Flow
- Current: 无 Goal 页、无独立 World 栏
- Proposed: 浅色 Agent Loop+Devices，视频/图片入设备卡
- Status: approved
- Exact approval and scope: 右侧仅 Task 关联设备；seam 可调宽/折叠

- ID and owner: D0-recovery / architecture
- Contract and evidence: 用户确认恢复边界
- Current: mid-turn 恢复未承诺
- Proposed: Task 级恢复，active watch 重织，迟到终态去重
- Status: approved
- Exact approval and scope: 不重复派发；未知结果先读状态再重试

- ID and owner: D0-generic / acceptance
- Contract and evidence: 用户强调通用 Agent
- Current: 不做场景定制
- Proposed: 两套声明不同夹具复用同一 Runtime/UI
- Status: approved
- Exact approval and scope: 核心与 prompt 零场景改动

- ID and owner: D0-mode / review
- Contract and evidence: 用户 D1 实际回答 HOLD SCOPE
- Current: 三切分均获设计确认
- Proposed: 全量保留，聚焦错误/迁移/验收
- Status: approved
- Exact approval and scope: 不增不删；仅交付节奏由后续计划拆分

- ID and owner: D0-pr / delivery
- Contract and evidence: `.gstack/ship-policy.json` 不存在
- Current: 默认每波 1 个 PR
- Proposed: 三切分可能形成多 PR
- Status: unresolved
- Exact approval and scope: 若拆分需评估队列成本与跨 PR 用户引用

- ID and owner: D1-media-adapter / implementation
- Contract and evidence: 用户 D2 实际回答
- Current: 图片被跳过，工具/消息仅文本
- Proposed: 扩展反腐败层媒体契约
- Status: approved
- Exact approval and scope: port/rig/工具输出端到端附件；非 vision 显式失败；保留媒体目标

## answeredDecision (D1-media-adapter)

Commitment comparison:

- Commitment: 模型真实读图
- Source/approval or pending: 用户已确认媒体目标
- Current: 不支持，图片被跳过
- A（反腐败层扩展）: 端到端支持
- B（provider 旁路）: 仅特定 provider 支持
- C（移除读图）: 不支持，违反目标

- Commitment: provider 边界
- Source/approval or pending: AGENTS.md 反腐败层约束
- Current: 仅文本
- A（反腐败层扩展）: 保留并扩展
- B（provider 旁路）: 绕过
- C（移除读图）: 保留文本边界

- Commitment: 证据关联
- Source/approval or pending: D0-media approved
- Current: 引用待实现
- A（反腐败层扩展）: ToolResult/TurnEvent 携带附件，trace 存引用
- B（provider 旁路）: 同左但跨层契约分叉
- C（移除读图）: 仅 UI 证据

- Commitment: 非 vision 失败
- Source/approval or pending: D0-media approved
- Current: 静默丢图风险
- A（反腐败层扩展）: 显式 `image_unavailable`
- B（provider 旁路）: 依 provider 各自处理
- C（移除读图）: 不支持模型读图

- Commitment: 测试
- Source/approval or pending: 必须具备
- Current: 无图片 roundtrip
- A（反腐败层扩展）: port/rig/provider roundtrip + 非 vision
- B（provider 旁路）: 单 provider 测试
- C（移除读图）: 现有文本回归

- Commitment: Effort/risk
- Source/approval or pending: 估计
- Current: —
- A（反腐败层扩展）: M，中风险
- B（provider 旁路）: M，中高风险
- C（移除读图）: S，零 provider 实现，范围违约

Question: D2 — D1-media-adapter: 如何兑现模型读图而不破坏 provider 边界
Project/branch/task: e4-p2-declaration-closeout，通用 Agent Runtime 的 Source/Media
主链。
ELI10: 当前工具结果和模型消息只能携带文本，rig 适配器还会直接丢弃图片。若只补设备图片接口，UI 能看见图，模型却根本收不到，形成假验收。
Stakes if we pick wrong: 可能让“Agent 已看到现场”成为假象，或者把每家 provider 都变成一套不可迁移的专用实现。
Recommendation: 扩展反腐败层媒体契约，因为它同时保留模型可移植性、证据关联和明确的非 vision 失败，是根因修复。
Note: options differ in kind, not coverage — no completeness score.
Pros / cons:
A) 扩展反腐败层媒体契约 (recommended)
✅ 图片可从 ToolResult 经 TurnEvent 到模型请求端到端传递，trace 仍只存证据引用。
✅ 非 vision provider 可在同一边界明确失败，避免不同模型静默差异。
❌ 需要同时修改 port、rig、工具输出与测试，改动面大于单点适配。
B) Provider 专用图片旁路
✅ 可复用某家模型现有图片字段，首条链路较短。
✅ 只覆盖 image，不必先抽象其他媒体类型。
❌ 绕过反腐败层，换 provider 就要重写且容易再次出现静默丢图。
C) 移除模型读图范围
✅ 不需要修改模型接口，当前文本架构保持不变。
✅ 图片只供人观看，媒体实现与测试更少。
❌ 直接违反 HOLD SCOPE 和用户已确认的“传感器/视频反馈给 Agent”目标。
Net: 现在决定根因边界，不决定减少已批准能力。
Header: 媒体契约
A) 扩展反腐败层媒体契约 (recommended)
M（human ~2–4 天 / CC ~0.5–1 天 [estimated]），中风险，复用 port/rig 现有适配。✅ 图片端到端传递，证据引用不进
prompt 文本。✅ 非 vision 显式失败并有 roundtrip 测试。❌ 需要修改 port、rig、工具输出与测试。
B) Provider 专用图片旁路
M（human ~1–3 天 / CC ~0.5 天 [estimated]），中高风险，复用单 provider 图片字段。✅ 首条图片链路较短。✅ 只覆盖
image，无需抽象其他媒体。❌ 绕过反腐败层，provider 迁移与静默丢图风险高。
C) 移除模型读图范围
S（human ~0.5 天 / CC ~15 分钟 [estimated]），零 provider 实现，文本架构不变。✅ 不需要修改模型接口。✅
图片仅供人观看，实现和测试更少。❌ 违反 HOLD SCOPE 与已批准的 Agent 感知目标。

Answer: 用户 D2 选择 A（扩展反腐败层媒体契约），2026-10-26。
Scope: port/ToolResult/TurnEvent/rig/ModelProvider 全链路结构化图片附件；
trace 仅保存证据引用；非 vision provider 必须显式失败；禁止 provider 专用旁路。

## CEO Review — 0G / 0I

### 0G. HOLD Scope Complexity Check

文件与新服务数量会超过 HOLD 检查阈值，但 Task 状态、持久唤醒、Device Job、媒体存储
和 UI 投影分别拥有不同失败语义，不能为减少文件合成上帝服务。保留 `task_wake_outbox`
是因为“状态迁移 + 持久唤醒”需要同一事务；保留 `device_jobs` 是因为活跃查询与恢复
不应扫描完整账本。没有新的 deferral：GoalWatch、GPU 感知、OpState、视频录像和
场景专用 Agent 均为独立后续范围。

### 0I. Temporal Interrogation

```text
HOUR 1：以 PR #101 当前代码为基线，验证迁移夹具、现有 wake/steer、worker
        completion signaling 和 rig 文本边界，禁止在旧 main 上另起失真分支。
HOUR 2-3：实现 Task 状态/CAS、outbox、job 终态幂等和 ToolResult 附件；
        先证明 device result → 原 Task，再接媒体与 UI。
HOUR 4-5：接 image/detection/video、播放描述与浏览器直连、SSE resync、关联设备集合和
        两栏/seam；重点处理重启、迟到结果和跨 workspace 访问。
HOUR 6+：完成两套设备夹具、崩溃矩阵、性能/a11y、模型 eval 与全量回归。
```

- HLS 采用 `video-dev/hls.js` v1.7.3；上游 issue #8119 表明 `stopLoad()` 在分片
加载中可能被后续状态撤销，设备/Task 切换必须 `destroy()` 播放器并验证无后台请求。

- AGH issue #454 的运动风险元数据与本设计固定设备首版无关；不提前引入移动机器人安全范围。

- 估计总投入：human ~3–5 周 / CC ~3–5 天 `[estimated]`，按三个独立可回滚切分交付。

- 除 D1-media-adapter 已裁定外，0I 未发现需要重新选择实现路线的阻塞。

## answeredDecision (D2-job-progress)

Commitment comparison:

- Commitment: 真实 job progress
- Source/approval or pending: D0-task/UI approved
- Current: 仅 accepted/result
- A（声明式进度）: source 字段提供 done/total/text
- B（ABI v3 回调）: driver callback 提供
- C（仅运行态）: 无百分比/详情

- Commitment: ABI 契约
- Source/approval or pending: D0-media approved：v2 不变
- Current: 无进度通道
- A（声明式进度）: 保持 v2
- B（ABI v3 回调）: 升级 v3
- C（仅运行态）: 保持 v2

- Commitment: 错误归属
- Source/approval or pending: 多 job 同设备
- Current: 无 job 关联
- A（声明式进度）: driverCallId 优先；否则仅唯一活跃 job；歧义忽略
- B（ABI v3 回调）: callback 直接关联
- C（仅运行态）: 无进度可错报

- Commitment: 无进度设备
- Source/approval or pending: 通用接入要求
- Current: 无
- A（声明式进度）: 明确 `progress_unavailable`，不伪造
- B（ABI v3 回调）: 同左
- C（仅运行态）: 只显示 running

- Commitment: 测试
- Source/approval or pending: 必须具备
- Current: 终态测试
- A（声明式进度）: 进度映射、乱序、歧义、迟到、无字段
- B（ABI v3 回调）: ABI/驱动/集成测试
- C（仅运行态）: 只测 running

- Commitment: Effort/risk
- Source/approval or pending: 估计
- Current: —
- A（声明式进度）: M，中风险
- B（ABI v3 回调）: L，高迁移风险
- C（仅运行态）: S，范围不完整

Question: D3 — D2-job-progress: 不改变 ABI v2 时如何获得真实 job progress
Project/branch/task: e4-p2-declaration-closeout，通用 Device Job 与 Agent Loop 进度投影。
ELI10: 当前驱动只告诉我们一个调用最终成功、失败或取消，没有执行到百分之几的事实。工作台若直接显示进度条，就会编造设备从未上报的信息。
Stakes if we pick wrong: 用户会依据虚假进度判断设备状态，或者为了进度功能破坏刚稳定的 ABI v2 接入层。
Recommendation: 通过 manifest 声明 progress source 字段，因为现有属性/遥测通道已经能携带事实，且不需要改变
ABI；只有能关联到确切 job 的进度才能展示。
Note: options differ in kind, not coverage — no completeness score.
Pros / cons:
A) 声明式 source 进度 (recommended)
✅ 复用现有属性与 EventBus，驱动按自身真实状态上报 done/total/text。
✅ 保持 ABI v2，未知设备无进度时明确显示 unavailable，不伪造百分比。
❌ 需要定义 job 关联与歧义拒绝规则，否则可能把进度贴错 job。
B) ABI v3 progress 回调
✅ 进度天然携带 job/driver call 关联，语义直接。
✅ 不依赖设备把进度建模为遥测字段。
❌ 违背 ABI v2 不变约束，所有驱动与 Hub 都要迁移，范围与回滚风险更高。
C) 仅显示 accepted/running
✅ 不需要修改驱动，现有终态语义不变。
✅ 不会出现错误的百分比。
❌ 不满足已批准的 progress 展示与长任务可观测性，属于缩减目标。
Net: 进度必须来自设备事实；本项只决定事实通道与关联规则。
Header: Job 进度
A) 声明式 source 进度 (recommended)
M（human ~2–4 天 / CC ~0.5–1 天 [estimated]），中风险，复用属性/EventBus。✅ 保持 ABI v2，真实
done/total/text 可进投影。✅ 无字段或歧义时显式 unavailable，测试覆盖关联与乱序。❌ 需要新增 progress 声明和严格的
job 归属规则。
B) ABI v3 progress 回调
L（human ~1–2 周 / CC ~2–3 天 [estimated]），高迁移风险，修改 SDK/Hub/驱动。✅ 进度直接绑定 driver
call。✅ 不依赖遥测建模。❌ 违反 ABI v2 不变约束，迁移与回滚范围显著扩大。
C) 仅显示 accepted/running
S（human ~0.5 天 / CC ~15 分钟 [estimated]），零驱动改动，终态语义不变。✅ 无错误百分比。✅ 实现最少。❌ 缺少真实
progress，违反已批准的长任务可观测目标。

Answer: 用户 D3 选择 C（仅显示 accepted/running），2026-10-26。
Scope: job 仅展示 accepted/running/终态，不展示百分比、done/total 或进度条；
不新增 progress 声明、source 字段或 ABI 回调。UI 不得伪造执行进度。

## answeredDecision (D3-job-pause)

Commitment comparison:

- Commitment: Task paused
- Source/approval or pending: D0-task/UI approved
- Current: 保留并阻止推进
- A（移除 Job paused）: 不变
- B（补 pause/resume）: 不变
- C（仅展示 paused）: 不变

- Commitment: Device Job 控制
- Source/approval or pending: spec 原状态有 paused 无行为
- Current: 无 pause/resume
- A（移除 Job paused）: accepted/running/final + stop/cancel
- B（补 pause/resume）: 声明 pausable + 控制端点 + timeout
- C（仅展示 paused）: 状态可见但无实际控制

- Commitment: 语义诚实
- Source/approval or pending: 禁止假控制/假状态
- Current: paused 无事实来源
- A（移除 Job paused）: 不出现 paused
- B（补 pause/resume）: 有真实设备暂停事实
- C（仅展示 paused）: 容易伪造暂停

- Commitment: 测试
- Source/approval or pending: 必须具备
- Current: 无 pause 测试
- A（移除 Job paused）: 状态机拒绝 paused + stop/cancel 回归
- B（补 pause/resume）: 暂停/恢复/超时/跨区/断连
- C（仅展示 paused）: 只测展示

- Commitment: Effort/risk
- Source/approval or pending: 估计
- Current: —
- A（移除 Job paused）: S，低风险
- B（补 pause/resume）: M/L，中高风险
- C（仅展示 paused）: S，高语义风险

Question: D4 — D3-job-pause: Device Job 首版是否需要 pause/resume
Project/branch/task: e4-p2-declaration-closeout，Device Job 生命周期与停止控制。
ELI10: Task 可以暂停推进，但设备 Job 的 paused
目前没有对应的声明、端点或驱动行为。保留这个状态却不实现控制，会让用户误以为设备真的暂停了。
Stakes if we pick wrong: 可能把“Agent 暂停思考”误当成“设备停止动作”，或者为暂停功能引入新的失控路径。
Recommendation: 首版移除 Device Job 的 paused，因为 stop/cancel
已覆盖明确的停止语义；未来有真实设备需求时再按声明加入 pause/resume。
Note: options differ in kind, not coverage — no completeness score.
Pros / cons:
A) 移除 Device Job paused (recommended)
✅ 状态机只保留真实可达状态，stop/cancel 的语义清晰且可测试。
✅ 不需要新增设备控制面，避免把 Task paused 与设备暂停混淆。
❌ 暂不支持长任务暂停后继续，某些设备工作流需要改用 stop 后新建 job。
B) 补声明式 pause/resume
✅ 支持真实长任务暂停/恢复，覆盖更多设备操作模式。
✅ 可要求驱动声明 pausable，不支持时明确拒绝。
❌ 增加端点、驱动语义、超时与恢复测试，首版复杂度和失控面更大。
C) 仅展示 paused
✅ 不需要实现 pause/resume 控制。
✅ 可以保留历史或未来状态的展示兼容。
❌ 没有真实暂停事实，容易误导用户并制造不可达状态。
Net: 本项只决定 Device Job 是否支持暂停，不改变 Task paused 的既定行为。
Header: Job 暂停
A) 移除 Device Job paused (recommended)
S（human ~0.5 天 / CC ~15 分钟 [estimated]），低风险，复用 stop/cancel。✅ 状态机只含真实可达状态。✅ 避免
Task 与设备暂停混淆，并保留停止回归。❌ 暂停后不能恢复，需 stop 后新建 job。
B) 补声明式 pause/resume
M/L（human ~1 周 / CC ~1–2 天 [estimated]），中高风险，新增声明/端点/驱动语义。✅ 支持真实长任务暂停恢复。✅ 不支持
pausable 时明确拒绝。❌ 增加超时、恢复、断连和跨区测试面。
C) 仅展示 paused
S（human ~0.5 天 / CC ~15 分钟 [estimated]），高语义风险，无实际控制。✅ 无需实现 pause/resume。✅
可兼容历史状态展示。❌ 没有真实暂停事实，容易误导用户并制造不可达状态。

Answer: 用户 D4 选择 A（移除 Device Job paused），2026-10-26。
Scope: Device Job 仅 accepted/running/终态，首版不提供 pause/resume；
Task paused 保留，暂停的是 Task 推进而非设备动作。

## Section 1: Architecture Review

### 1.1 组件与依赖

内部 ID 不做第二套映射：`device_id` 是 API/展示层别名，持久化仍使用现有 `thing_id`；
`source_id` 是 SourceItem 外层字段名，对应声明的 `sources[].id`。

```text
                 ┌──────────── Chat / SDK / Workbench ────────────┐
                 │ create/cancel/approve/subscribe/read only      │
                 ▼                                                │
       ┌─────────────────────┐                         ┌─────────▼─────────┐
       │ Task Runtime (agent)│                         │ UI projection     │
       │ TaskRegistry/CAS    │──runtime.v1 events──────▶│ Loop + Devices    │
       │ WakeRouter/Outbox   │                         │ seam/Flow         │
       └──────────┬──────────┘                         └───────────────────┘
                  │ fixed tools / attachments
                  ▼
       ┌─────────────────────┐      job/watch/source    ┌───────────────────┐
       │ Device Hub (runtime)│─────────────────────────▶│ LiveState/Watch   │
       │ Jobs/LiveState      │                         │ Engine/EventBus   │
       │ Source/Media        │                         └───────────────────┘
       └──────────┬──────────┘
                  │ ports only
                  ▼
       ┌─────────────────────┐     media bytes      ┌───────────────────┐
       │ db (all SQL)        │                     │ Local MediaStore  │
       │ tasks/runs/jobs/    │                     │ data/media/<ws>   │
       │ outbox/evidence     │                     └───────────────────┘
       └─────────────────────┘
```

新耦合只有两条正当主链：Task Runtime 通过固定工具调用 Device Hub；UI 订阅统一投影。
DB 与媒体二进制分离，runtime/agent 不直接写 SQL，浏览器不直接读取驱动或磁盘。

### 1.2 四条主数据流

#### A. Task 创建与首轮推进

```text
POST text → 校验 workspace/admin → insert Task(v=1) → enqueue Run
  → 注入 Task+设备快照 → tool calls/results → RunReport+task_id
Nil:    空 text/空 workspace → 400/404，不落 Task。
Empty:  无设备 → 空快照，Run 可说明无设备，不生成假动作。
Error:  Task insert 失败 → 500，不 enqueue；enqueue 持续失败 → pending 重试，最终 dead+alert。
```

#### B. 长 Job 与原 Task 唤醒

```text
call_device → validate/claim → insert job accepted → worker execute
  ├─ result ≤50s → tool result → current Run
  └─ >50s → running(job_id) → waiting_job → final result
             → outbox(task,event) → next Run → verify/complete
Nil:    无 declaration → 拒绝，不创建 job。
Empty:  progress 不存在 → 仅 accepted/running，不生成百分比。
Error:  派发失败 → rejected；超时 → 真 deadline；重复终态 → 去重，不重复唤醒。
```

#### C. Task-owned Watch

```text
register watch(owner_task) → persist+upsert → event hit/timeout
  → ledger+outbox → 原 Task → next Run → one-shot 删除
Nil:    条件/设备不存在 → 拒绝。
Empty:  无新数据 → 保持等待，不命中；unknown ≠ false。
Error:  设备删除/离线 → 失效事件；重启 → 非 terminal Task watch 重织。
```

#### D. 图片与视频

```text
read_device(image) → source read → MIME/size/freshness → MediaStore
  → evidence_id → ToolAttachment → vision provider / explicit unavailable
Video: UI selected → playback descriptor → browser direct upstream → deselect →
destroy player
Nil:    source 不存在 → 明确 error，不读其他 source 代替。
Empty:  无图片帧/无视频流 → empty state，不显示旧帧冒充实时。
Error:  非 vision/媒体失败/断流 → 显式降级；数值感知与其他控制不中断。
```

### 1.3 状态机

#### Task

```text
active / waiting_job / waiting_watch / needs_approval
  │  ├─正常 Run 完成且有等待──▶ waiting_job / waiting_watch / needs_approval
  │  ├─暂停────────────────────▶ paused（只记录，不启动/steer）
  │  ├─恢复正常────────────────▶ 按等待关系恢复 active 或启动恢复 Run
  ├─正常 Run 完成且无等待──────────────────────────────▶ completed
  ├─Run 失败且不可重试──────────────────────────────────▶ failed
  └─cancel request──▶ cancelling ──全部 job/watch 有终态──▶ cancelled
                        └────任一 cancelled_unknown ────────▶
failed(cancel_unknown)

非法：terminal 后再次推进；旧 Run 覆盖新状态；paused/cancelling 时启动 Run。
防护：terminal 不可迁移、state_version CAS、outbox 去重、wake_event_id 唯一。
```

#### Device Job

```text
accepted ──worker 接收──▶ running ──done/failed/cancelled/cancelled_unknown
   │                          └─ stop/timeout/driver error → 对应终态
   └─校验/派发失败────────▶ rejected
重启前活跃且不可恢复────────▶ interrupted

首版无 paused、无百分比。终态不可再次迁移；迟到重复终态只幂等处理。
```

### 1.4 安全与 API 边界

- 新入口/变更: `POST agent/tasks`
- 调用者与权限: workspace admin
- 能读/改什么: 创建本区 Task/objective
- 拒绝与审计: 空输入 400；创建事件含 created_by

- 新入口/变更: Task list/detail/cancel
- 调用者与权限: admin 管理；授权成员只读
- 能读/改什么: 本区 Task/Run/Job/Watch
- 拒绝与审计: 跨区 404；取消幂等落账

- 新入口/变更: runtime/stream
- 调用者与权限: 授权 workspace 成员
- 能读/改什么: 本区 Task 与关联设备投影
- 拒绝与审计: cursor 限界、无跨区 IDOR

- 新入口/变更: evidence/{id}
- 调用者与权限: 授权 workspace 成员
- 能读/改什么: 单张图片及元数据
- 拒绝与审计: 过期 410；哈希/工作区校验

- 新入口/变更: source/playback
- 调用者与权限: 授权成员 + 5 分钟限定票据
- 能读/改什么: 单个声明的脱敏播放描述
- 拒绝与审计: 无长期凭证；不支持时明确 `playback_unavailable`

- 新入口/变更: device call/stop
- 调用者与权限: Agent/Admin 经既有审批
- 能读/改什么: 声明的工具与资源
- 拒绝与审计: 声明校验、资源 busy、完整 job 账本

### 1.5 负载、SPOF 与生产失败

- 10x：首先放大的是 outbox/ledger 写入与 Task 关联查询；需要索引、批量查询和 DLQ 指标。

- 100x：浏览器到上游的视频连接、图片字节与 MediaStore 磁盘成为首要约束；latest-wins、按需播放和过期清理必须成立。

- 单点：SQLite、媒体磁盘、outbox dispatcher、模型 provider、视频上游。各自故障不得互相伪装成功。

- D8 已选择不新增固定配额；容量指标、背压、降级和告警必须成立，不能宣称资源风险已消除。

| 集成失败场景 | 计划处理 |
| --- | --- |
| accepted 后进程崩溃 | job interrupted，不重复派发；Task 恢复并被唤醒 |
| outbox 已投递未标记 | dispatcher 重试；wake_event_id 第二道去重 |
| 模型不支持 vision | `image_unavailable`，UI 仍可看图，禁止假装已读 |
| HLS 分片加载中切 Task | `destroy()` 播放器，验证无后台请求 |
| SQLite 写锁/冲突 | 限定重试，最终响亮失败；不部分提交 Task/job/outbox |
| 媒体目录不可写 | 图片读取显式失败，数值控制继续；指标与 runbook 可见 |

### 1.6 回滚与产物价值

回滚顺序：关闭 Task Runtime 与 Workbench 开关 → 恢复旧 `POST agent/tasks` 指令路径
→ 旧 binary 继续忽略 additive 表/列与 manifest 1.1 新字段 → 保留账本/媒体审计。
预计维护窗口约 15 分钟 `[estimated]`，无需 destructive DB 回滚。

新增表、outbox 和事件都有直接消费者：任务推进、重启恢复、实时 UI 与审计回放；
不是仅供展示的治理报表。Section 7 的容量指标是产品可观测性，服务于背压和告警。

**Decision gate：** D1-media-adapter、D2-job-progress、D3-job-pause 已按实际回答应用。
第二轮新增 R1 drain 与 R3 job 级停止已批准并写回；R2 审批持久决策/恢复经 D2=C 保持
pending，是未解决的架构阻断项。D0-pr 仅影响交付拆分，留待 Deployment/Next Steps。

### 1.7 工程复核

- [INFORMATIONAL] (confidence: 9/10) `manager.rs:723-760` — cancel 只改 Task 状态不足以阻止
  当前 Run 继续调用工具。新增约束：先 abort 当前 Run 的 CancellationToken 并阻断新工具派发，
  再停止所属 job/watch；`paused` 仍按 Run 边界暂停，不强制中断模型。
- [INFORMATIONAL] (confidence: 9/10) spec:185-205 — `owner_task_id`、`session_key` 和
  `WakeSignal.workspace_id` 必须与 Task workspace 一致，upsert/重织/outbox dispatch 都要拒绝
  跨区关联，防止误唤醒其他工作区任务。
- [INFORMATIONAL] (confidence: 9/10) spec:203-205,333-334 — `wake_event_id` 必须同时进入
  RunStarted/RunReport/agent_runs；只有 outbox ID 没有 RunReport ID 时，唯一去重不能成立。
- [INFORMATIONAL] (confidence: 9/10) `manager.rs:742-833` — 当前已有近期记录、resolution、
  快照和 principles 多段 prompt，新增 pending Task/job/watch 后可能超过 6KB。D10 必须决定
  上下文预算与裁减顺序，而不是无界拼接。
- [INFORMATIONAL] (confidence: 9/10) spec:281-286 — 播放 URL 需限制协议与 userinfo，
  避免把 `file/javascript/data` 或 `user:pass@host` 直接交给浏览器。
- [INFORMATIONAL] (confidence: 9/10) spec:368-371 — `dedup_key` 改为规范化 JSON 元组哈希，
  防止裸分隔符拼接导致 ID 冲突。
- [INFORMATIONAL] (confidence: 9/10) rig `completion/message.rs:314-324,1042-1071` — 已验证
  `ToolResultContent::Image` 与 Base64/Raw 字节源存在，统一附件不需要 provider 专用旁路。

第二轮工程复审（设计评审新增控制契约后）：

- [CRITICAL] (confidence: 9/10) R1/flags/rollback — 禁用与先关 UI 未定义存量 Task 行为；
  D1=A 已批准显式 drain 后关闭，旗标不隐式批量暂停/取消，回滚顺序与 smoke 已写回 §9。
- [CRITICAL] (confidence: 9/10) R2/6.2-6.4 — 缺少持久 approval decision、冻结参数和
  崩溃执行关联；D2=C 选择保持待补，未批准任何实现，审批路由/执行/UI 任务保持 blocked。
- [CRITICAL] (confidence: 10/10) R3/4.4 — worker 缺 job 身份，设备级取消会误伤其他 Task；
  D3=A 已批准 job_id 贯穿 queued/in-flight/result/restart，并持久 stop_requested。

**Decision gate：** D1-media-adapter 的实现路径已证实可行；D10 的 Task 上下文预算已按
既有决定保留。第二轮 R1/R3 已解决，R2 未解决；未把 pending remedy 计入已批准实现。

## Section 2: Error & Rescue Map

### 2.1 失败类别与救援

- METHOD/CODEPATH: `POST agent/tasks`
- WHAT CAN GO WRONG: 空输入/无权限/DB 失败
- EXCEPTION CLASS: Validation/Auth/Sqlx
- RESCUED? / ACTION / USER SEES: Y：400/403/500，无 Task、无 Run；明确响应

- METHOD/CODEPATH: `POST agent/tasks`
- WHAT CAN GO WRONG: outbox/scheduler 满或关闭
- EXCEPTION CLASS: EnqueueError
- RESCUED? / ACTION / USER SEES: Y：不重复创建；重试与双击语义见 Section 4 D5

- METHOD/CODEPATH: `TaskStore::transition`
- WHAT CAN GO WRONG: state_version 冲突
- EXCEPTION CLASS: ConcurrentUpdate
- RESCUED? / ACTION / USER SEES: Y：旧 Run 不得覆盖；重读后决定 steer/interrupted

- METHOD/CODEPATH: `TaskStore::transition+outbox`
- WHAT CAN GO WRONG: 事务任一步失败
- EXCEPTION CLASS: Sqlx::Error
- RESCUED? / ACTION / USER SEES: Y：整体回滚，无状态/无半条 outbox；可见失败

- METHOD/CODEPATH: `WakeDispatcher::dispatch`
- WHAT CAN GO WRONG: 队列满/关闭/重复投递
- EXCEPTION CLASS: EnqueueError
- RESCUED? / ACTION / USER SEES: Y：pending 退避重试，dead+alert；wake_event_id 去重

- METHOD/CODEPATH: `DeviceJobStore::accepted`
- WHAT CAN GO WRONG: DB 写失败
- EXCEPTION CLASS: Sqlx::Error
- RESCUED? / ACTION / USER SEES: Y：不派发；调用方得到失败，不产生物理副作用

- METHOD/CODEPATH: `DeviceJobStore::result`
- WHAT CAN GO WRONG: 终态/ledger/outbox 写失败
- EXCEPTION CLASS: Sqlx::Error
- RESCUED? / ACTION / USER SEES: Y：终态保留在内存重试；未确认完成，重启按活跃恢复

- METHOD/CODEPATH: `DataServer::execute`
- WHAT CAN GO WRONG: busy/声明/timeout/stop
- EXCEPTION CLASS: CallError
- RESCUED? / ACTION / USER SEES: Y：带资源、持有者或原因；未声明不执行

- METHOD/CODEPATH: `DataServer::execute`
- WHAT CAN GO WRONG: accepted 后崩溃
- EXCEPTION CLASS: Interrupted
- RESCUED? / ACTION / USER SEES: Y：不盲重放；job interrupted，Task 读状态后决定

- METHOD/CODEPATH: `WatchEngine::upsert`
- WHAT CAN GO WRONG: 条件/引用非法
- EXCEPTION CLASS: WatchValidation
- RESCUED? / ACTION / USER SEES: Y：注册拒绝并给字段原因；不启动非法 watch

- METHOD/CODEPATH: `WatchEngine::event`
- WHAT CAN GO WRONG: 无新数据/设备删除/Lagged
- EXCEPTION CLASS: Empty/Invalid/Lagged
- RESCUED? / ACTION / USER SEES: Y：等待/失效事件/全量重同步；不当作命中

- METHOD/CODEPATH: `ModelProvider::chat`
- WHAT CAN GO WRONG: 超时/429/provider 不可用
- EXCEPTION CLASS: Timeout/RateLimited/Unavailable
- RESCUED? / ACTION / USER SEES: Y：Run failed(retryable)，退避且受预算；不重复设备动作

- METHOD/CODEPATH: `ModelProvider::chat`
- WHAT CAN GO WRONG: malformed/empty/refusal
- EXCEPTION CLASS: Parse/Empty/Refused
- RESCUED? / ACTION / USER SEES: Y：分别记录，空/拒绝不进入 completed，不执行工具

- METHOD/CODEPATH: `read_device(args)`
- WHAT CAN GO WRONG: hallucinated 非法设备/参数/schema
- EXCEPTION CLASS: Validation
- RESCUED? / ACTION / USER SEES: Y：模型可见拒绝原因，未执行；RunReport rejected

- METHOD/CODEPATH: `read_device(image)`
- WHAT CAN GO WRONG: 非 vision/超限/非 JPEG-PNG/过期
- EXCEPTION CLASS: Unsupported/Validation
- RESCUED? / ACTION / USER SEES: Y：`image_unavailable` 或明确错误；不静默丢图

- METHOD/CODEPATH: `MediaStore::write+EvidenceStore`
- WHAT CAN GO WRONG: 二进制已写、DB 失败/重复
- EXCEPTION CLASS: Sqlx::Error/Unique
- RESCUED? / ACTION / USER SEES: Y：孤儿清理/去重复用；不返回半条证据

- METHOD/CODEPATH: `MediaStore::read`
- WHAT CAN GO WRONG: 过期/越权/文件缺失
- EXCEPTION CLASS: NotFound/Auth
- RESCUED? / ACTION / USER SEES: Y：404/410，不泄露其他 workspace 或新图替代

- METHOD/CODEPATH: `MediaStore::retention`
- WHAT CAN GO WRONG: 删除失败/磁盘不可写
- EXCEPTION CLASS: IoError
- RESCUED? / ACTION / USER SEES: Y：下周期重试+容量告警；不静默撑满磁盘

- METHOD/CODEPATH: `PlaybackDescriptor::read`
- WHAT CAN GO WRONG: 上游超时/不可信 URL/协议
- EXCEPTION CLASS: Timeout/Security/Unsupported
- RESCUED? / ACTION / USER SEES: Y：`playback_unavailable`；直连凭证/CORS 策略见
Section 3 D7

- METHOD/CODEPATH: `VideoPlayer::switch/close`
- WHAT CAN GO WRONG: 分片加载中断流/切 Task
- EXCEPTION CLASS: PlaybackError
- RESCUED? / ACTION / USER SEES: Y：destroy 后重建；无后台请求，显示重连状态

- METHOD/CODEPATH: `runtime/stream`
- WHAT CAN GO WRONG: 快照失败/断连/重复 cursor
- EXCEPTION CLASS: Upstream/Closed
- RESCUED? / ACTION / USER SEES: Y：错误帧+重连 resync；旧数据标 stale，不重复步骤

- METHOD/CODEPATH: `StopAll::execute`
- WHAT CAN GO WRONG: 部分设备停止失败/结果未知
- EXCEPTION CLASS: Partial/Unknown
- RESCUED? / ACTION / USER SEES: Y：逐设备结果，不能宣称全停；相关 Task 被通知

- METHOD/CODEPATH: `Task::cancel`
- WHAT CAN GO WRONG: 仍有活跃 job/watch
- EXCEPTION CLASS: ActiveConflict
- RESCUED? / ACTION / USER SEES: Y：cancelling→停止所属 job/watch；任一 unknown 则
failed(cancel_unknown)，不假称全停

### 2.2 事务与不变量

```text
Task 状态迁移 + outbox insert       = 同一 DB 事务，任败全回滚
Job 终态投影 + terminal ledger       = 同一 DB 事务，任败保留内存终态重试
MediaStore 二进制 + evidence DB 行   = 非原子外部资源；失败进孤儿清理，dedup_key 防重复
```

- 任何“已执行”响应必须晚于对应持久事实；不能让 UI 先看到成功再补账。

- 设备物理结果与 DB 不可能是单一天然事务，因此用终态重试、恢复与去重，禁止盲目重放。

- 日志必须带 workspace/task/run/job/device/event_id 和失败动作，不能只写 `error occurred`。

### 2.3 Failure Modes Registry（本轮初查）

```text
CODEPATH                  | FAILURE MODE       | RESCUED? | TEST? | USER SEES? |
LOGGED?
  --------------------------|--------------------|----------|-------|------------|--------
Task create/double-click  | 重复 Task/Run      | Y key+tx  | required| 原 Task    |
Y
Task transition+outbox    | 半提交/重复唤醒     | Y tx+dedup| required| 明确错误   | Y
Job result                | DB 失败/进程崩溃    | Y retry   | required| 等待/恢复  | Y
Model response            | 空/拒答/非法 JSON   | Y typed   | required| 失败原因   | Y
Image                     | 非 vision/超限/过期 | Y explicit| required| 明确不可用 | Y
Playback descriptor       | CORS/凭证/断流      | Y explicit| required| 明确不可用 | Y
MediaStore                | 孤儿文件/重复证据   | Y cleanup | required| 可重试     | Y
Task cancel               | 活跃设备 job        | Y stop+wait| required| 明确终态   | Y
StopAll                   | 部分失败            | Y partial | required| 逐设备结果 | Y
```

**Decision gate：** D5（强制 Idempotency-Key）和 D6（cancelling 后停止所属 job/watch）
均已按实际回答闭合；D7 已选择浏览器直连。其余错误路径按既定幂等、恢复和显式降级约束映射。

## Section 3: Security & Threat Model

### 3.1 输入、授权与数据边界

- Task 文本：`trim` 后 1..=8000 字符；控制字符、空文本和超长输入响亮拒绝，不进入 prompt。

- `Idempotency-Key` 必需，trim 后 1..=128 字符；workspace 内唯一，异 objective 复用返回 409。

- ID/cursor：UUID/声明 ID/workspace 归属校验；分页 limit ≤200，非法 cursor 400。

- 图片：检查真实 magic bytes、MIME、2 MiB 和像素上限 16MP；拒绝 SVG、脚本和 decompression bomb。

- detections/设备文本：按不可信输入围栏；UI 使用 Lit 文本绑定，不新增 unsafeHTML。

- 媒体路径：使用 `data/media/<hex(workspace_id)>/<sha256>`，禁止任何用户路径片段进入文件系统。

- 所有 SQL 继续由 db 门面参数化执行；Task/Run/Job/Watch/证据全部 workspace scoped，跨区统一 404。

- 图片和视频可能包含人员或敏感现场：默认 7 天保留、访问记元数据日志，不生成公网 URL，不把凭证写进 trace。

- `hls.js` 固定 v1.7.3 并提交 lockfile；设备/Task 切换销毁播放器，防止旧上游连接残留。

- 创建/取消/审批/设备调用/停止都写审计事件；媒体读取只记 ID、大小、MIME 和结果，不记图片内容。

### 3.2 主要威胁

- 威胁: 跨 workspace Task/Job/证据 IDOR
- 可能性: 中
- 影响: 高
- 当前缓解: workspace scope + 统一 404 + handler 集成测试

- 威胁: 恶意/超大 objective 或媒体导致资源耗尽
- 可能性: 中
- 影响: 高
- 当前缓解: 文本/像素/字节/Run 附件硬上限与背压指标

- 威胁: 恶意 detections 文本进行 prompt injection
- 可能性: 中
- 影响: 高
- 当前缓解: 不可信围栏、固定工具白名单、参数 schema、审批

- 威胁: 伪造图片/过期图片冒充现场
- 可能性: 中
- 影响: 高
- 当前缓解: magic bytes/MIME/freshness/hash/evidence 关联

- 威胁: 浏览器直连的 CORS、混合内容或签名 URL 泄漏
- 可能性: 中
- 影响: 高
- 当前缓解: D7 已裁定：短时签名 URL、query 脱敏、失败显式降级、无代理兜底

- 威胁: 媒体播放描述把长期凭证暴露给浏览器
- 可能性: 中
- 影响: 高
- 当前缓解: 长期凭证只留 Hub；短时签名 URL 与限定票据，Hub 不转发 Authorization

- 威胁: 播放器切换后继续拉流
- 可能性: 中
- 影响: 中
- 当前缓解: destroy + 后台请求断言；上游 #8119 已知约束

- 威胁: 新型后台任务被滥用创建海量 Task/outbox
- 可能性: 中
- 影响: 高
- 当前缓解: admin + 背压/告警，不设固定 Task 配额，必须监控并降级

### 3.3 已裁定：浏览器直连播放

用户 D5 选择浏览器直连。Hub 不获取视频流，因此不存在 Hub SSRF 代理面；
剩余风险是 CORS、混合内容和签名 URL 泄漏，必须用短时签名、query 脱敏、
失败显式降级与无代理兜底控制。

## answeredDecision (D7-video-proxy)

Commitment comparison:

- Commitment: SSRF 防护
- Source/approval or pending: 安全必需
- Current: 未定义完整策略
- A（allowlist+全代理）: 每个 playlist/segment/key/map 均校验并重写
- B（声明 origin）: 只校验入口 origin，子资源风险残留
- C（浏览器直连）: 无 Hub 代理，风险转浏览器/凭证

- Commitment: 内网设备支持
- Source/approval or pending: 用户摄像头需求
- Current: 私网地址常见
- A（allowlist+全代理）: workspace admin 明确允许 origin
- B（声明 origin）: 驱动声明 origin 即可
- C（浏览器直连）: 浏览器可直连但 CORS/凭证复杂

- Commitment: 凭证保护
- Source/approval or pending: D0-media approved
- Current: 不暴露长期 JWT
- A（allowlist+全代理）: Hub 使用并隐藏驱动凭证，短期票据给浏览器
- B（声明 origin）: 同左
- C（浏览器直连）: 可能必须把上游凭证给浏览器

- Commitment: 失败可见性
- Source/approval or pending: 错误矩阵必需
- Current: 明确 unsupported
- A（allowlist+全代理）: 非 allowlist/非法 URI 明确拒绝并记审计
- B（声明 origin）: 子资源失败较难统一
- C（浏览器直连）: CORS/混合内容/凭证错误暴露给终端

- Commitment: 测试
- Source/approval or pending: 必须具备
- Current: 无
- A（allowlist+全代理）: 入口+嵌套 URI+重定向+私网 allowlist+拒绝清单
- B（声明 origin）: origin 变更测试
- C（浏览器直连）: CORS/凭证泄漏测试

- Commitment: Effort/risk
- Source/approval or pending: 估计
- Current: —
- A（allowlist+全代理）: M/L，中高风险
- B（声明 origin）: M，中风险
- C（浏览器直连）: S/M，高安全风险

Question: D5 — D7-video-proxy: 视频代理如何兼顾内网摄像头与 SSRF 防护
Project/branch/task: e4-p2-declaration-closeout，通用视频播放与媒体代理。
ELI10: 摄像头常在私网，Hub 需要代理视频，但 HLS 播放列表还会让服务器继续请求分片和密钥。若只检查第一个
URL，恶意清单仍能把请求引向云元数据或内网服务。
Stakes if we pick wrong: 可能把 Hub 变成内网攻击代理，或者把上游凭证直接暴露给浏览器。
Recommendation: 采用 workspace 显式 allowlist，并代理重写所有 HLS 子资源
URI，因为入口和后续请求必须受同一条来源与凭证策略约束。
Note: options differ in kind, not coverage — no completeness score.
Pros / cons:
A) workspace allowlist + 全 URI 重写代理 (recommended)
✅ 入口、分片、密钥、map 和重定向都按同一 allowlist 校验，SSRF 边界完整。
✅ 驱动凭证只留 Hub，浏览器只拿限定票据，私网 origin 可由 admin 显式开启。
❌ 需要实现 HLS 清单解析/重写、请求审计与网络测试，复杂度最高。
B) 仅允许声明的 origin
✅ 配置较少，可直接复用驱动声明的播放地址。
✅ 对普通单源 MJPEG 已能限制入口。
❌ HLS 子资源与重定向可绕过，不能形成完整安全边界。
C) 浏览器直连播放
✅ 不需要实现代理转发和清单重写。
✅ Hub 不承受视频流量与连接压力。
❌ CORS、混合内容和上游凭证容易暴露，不能作为通用安全方案。
Net: 本项决定的是代理安全边界，不是视频编码或设备类型。
Header: 视频代理
A) workspace allowlist + 全 URI 重写代理 (recommended)
M/L（human ~1 周 / CC ~1–2 天 [estimated]），中高风险，复用 Hub 鉴权与媒体声明。✅ 所有子请求统一校验，凭证不出
Hub。✅ 私网 origin 可审计开启并有 SSRF 回归。❌ 需要清单解析重写、网络测试与运维配置。
B) 仅允许声明的 origin
M（human ~2–4 天 / CC ~0.5–1 天 [estimated]），中风险，复用声明 origin。✅ 配置较少。✅ 对简单 MJPEG
可限制入口。❌ HLS 子资源和重定向可绕过，无法完成 SSRF 防护。
C) 浏览器直连播放
S/M（human ~1–3 天 / CC ~0.5 天 [estimated]），高安全风险，无 Hub 代理。✅ 无需转发实现。✅ Hub
不承担视频连接压力。❌ CORS、混合内容和凭证泄漏无法统一控制。

Answer: 用户 D5 选择 C（浏览器直连播放），2026-10-26。
Scope: Hub 仅返回脱敏 playback descriptor；驱动/上游提供浏览器可用的短时签名 URL；
浏览器直连播放，失败显式降级，不回退 Hub 代理，不转发长期凭证。

## Section 4: Data Flow & Interaction Edge Cases

### 4.1 当前待裁定：Task 创建幂等

`agent_tasks.rs:97-102` 每次生成新 UUID。持久化后，scheduler 的 60 秒去重不能阻止
双击先创建两行 Task。创建必须让 Task 与 outbox 在同一事务中持久化，dispatcher 再投递；
受理重放返回同一 task_id，而不是依赖调用方避免双击。

## answeredDecision (D5-task-create)

Commitment comparison:

- Commitment: 双击与重试
- Source/approval or pending: 用户创建可靠性
- Current: 每次新 Task
- A（可选 key+兼容去重）: 同 key 或短窗同 objective 返回原 Task
- B（强制 key）: 仅同 key 返回原 Task
- C（不去重）: 每次创建新 Task

- Commitment: 现有客户端
- Source/approval or pending: `POST {text}` 已存在
- Current: 无 key
- A（可选 key+兼容去重）: 保持兼容，可渐进加 key
- B（强制 key）: 必须改所有客户端
- C（不去重）: 兼容但错误

- Commitment: 崩溃后重试
- Source/approval or pending: Task 级恢复要求
- Current: 未定义
- A（可选 key+兼容去重）: Task+outbox 已提交则重放受理，dispatcher 重试
- B（强制 key）: 同左
- C（不去重）: 崩溃后易重复创建

- Commitment: 不同任务区分
- Source/approval or pending: 同文不同意图可共存
- Current: 未定义
- A（可选 key+兼容去重）: workspace+session/objective 短窗；key 独立
- B（强制 key）: 仅 key 独立
- C（不去重）: 同文也重复

- Commitment: 测试
- Source/approval or pending: 必须具备
- Current: 无双击测试
- A（可选 key+兼容去重）: key/双击/并发/短窗过期/outbox 失败
- B（强制 key）: key 缺失/重复/并发
- C（不去重）: 无

- Commitment: Effort/risk
- Source/approval or pending: 估计
- Current: —
- A（可选 key+兼容去重）: M，中风险
- B（强制 key）: M，兼容风险高
- C（不去重）: S，高执行风险

Question: D6 — D5-task-create: 如何防止双击和重试创建重复 Task
Project/branch/task: e4-p2-declaration-closeout，持久 Task 创建与唤醒受理。
ELI10: 现在每次 POST 都生成新 ID。用户双击、网络重试或进程崩溃后重发，都可能让同一个意图变成多个真实任务和设备动作。
Stakes if we pick wrong: 同一设备可能被重复控制，审计也无法判断两个 Task 哪个才是用户真正想要的。
Recommendation: 使用可选 Idempotency-Key 并保留短窗口兼容去重，因为现有 `{text}` 客户端不能被强制立刻修改，同时
Task+outbox 必须先于受理持久化。
Note: options differ in kind, not coverage — no completeness score.
Pros / cons:
A) 可选 Idempotency-Key + 短窗兼容去重 (recommended)
✅ 同 key 永远返回原 Task；无 key 的旧客户端在短窗内也不会因双击重复执行。
✅ Task+outbox 事务化后，崩溃重试可恢复受理，不需要重新创建。
❌ 需要唯一定义 session/objective 窗口与并发冲突行为，避免误合并不同任务。
B) 强制 Idempotency-Key
✅ 语义最明确，任何重试都依赖唯一 key。
✅ 不需要推断同文短窗，减少误合并。
❌ 所有现有客户端必须同时修改，兼容与迁移风险高。
C) 不做创建幂等
✅ API 最简单，现有行为不用改。
✅ 同文任务可以任意重复创建。
❌ 双击和重试会产生真实重复任务与设备副作用，不可接受。
Net: 本项只决定创建受理的幂等语义，不涉及设备 pause/resume。
Header: 创建幂等
A) 可选 Idempotency-Key + 短窗兼容去重 (recommended)
M（human ~2–4 天 / CC ~0.5–1 天 [estimated]），中风险，复用现有 POST 与 outbox。✅
兼容旧客户端并阻止双击重复执行。✅ 崩溃后重放受理，覆盖并发/短窗/outbox 测试。❌ 需要精确定义 session/objective 窗口与冲突行为。
B) 强制 Idempotency-Key
M（human ~2–4 天 / CC ~0.5–1 天 [estimated]），高兼容风险，所有客户端改契约。✅ 重试语义明确。✅ 无需推断同文窗口。❌
现有客户端必须同步修改，迁移成本较高。
C) 不做创建幂等
S（human ~0.5 天 / CC ~15 分钟 [estimated]），高执行风险，不改现有端点。✅ API 最简单。✅ 同文任务可任意创建。❌
双击/重试会产生真实重复任务与设备副作用。

Answer: 用户 D6 选择 B（强制 Idempotency-Key），2026-10-26。
Scope: 所有创建请求必须携带 Idempotency-Key；缺失 400。
同 key 重放返回原 Task 且不重复 outbox；不同 objective 复用 key 返回 409。
Task+outbox 同一事务，UI/Chat 必须同步生成与重放 key。

### 4.2 当前待裁定：取消 Task 时设备 Job 的归属

`paused` 只停止 Task 推进，不取消设备 job。`cancelled` 也不能只改 Task 状态：
若 Task 已取消而设备仍继续动作，用户会看到错误的“已停止”。必须明确取消、
detached 或等待完成的语义与部分失败结果。

## answeredDecision (D6-task-cancel)

Commitment comparison:

- Commitment: 用户取消含义
- Source/approval or pending: 停止 Task 与相关副作用
- Current: 未定义
- A（停止所属 job）: cancelling→停止 job/watch→终态
- B（detached 继续）: Task cancelled，job detached 继续
- C（等待完成）: cancelling 等 job 完成，不主动 stop

- Commitment: 部分失败
- Source/approval or pending: 设备停止可能 unknown
- Current: 未定义
- A（停止所属 job）: 任一 unknown 则 failed(cancel_unknown)，不假称 cancelled
- B（detached 继续）: 明示 detached，不声称停止
- C（等待完成）: 到终态后仍可能失败需可见

- Commitment: 迟到事件
- Source/approval or pending: Task 终态后不得推进
- Current: 去重已批准
- A（停止所属 job）: 只更新投影，不唤醒 terminal Task
- B（detached 继续）: 同左
- C（等待完成）: 同左

- Commitment: watch
- Source/approval or pending: 防新唤醒
- Current: 待定义
- A（停止所属 job）: 取消并删除，pending outbox 标 dead
- B（detached 继续）: 保留为 detached watch 或明确移交
- C（等待完成）: 取消，不再启动新 Run

- Commitment: 测试
- Source/approval or pending: 必须具备
- Current: 无取消矩阵
- A（停止所属 job）: 全停/部分 unknown/重复取消/迟到/暂停中取消
- B（detached 继续）: detached 展示/重复取消
- C（等待完成）: 不主动 stop/终态后 cancel

- Commitment: Effort/risk
- Source/approval or pending: 估计
- Current: —
- A（停止所属 job）: M/L，中高风险
- B（detached 继续）: M，中语义风险
- C（等待完成）: M，中风险

Question: D7 — D6-task-cancel: 取消 Task 时如何处理仍在运行的设备 Job
Project/branch/task: e4-p2-declaration-closeout，Task 取消与设备副作用边界。
ELI10: 用户取消一个 Task，通常期待它不再继续控制设备。如果 Task 已标 cancelled 而设备还在执行，界面和真实世界会分离；但简单
kill 所有 job 也可能在部分设备上结果未知。
Stakes if we pick wrong: 可能留下无人拥有的设备动作，或者在停止失败时错误显示“已全部停止”。
Recommendation: 取消 Task 时进入 cancelling，停止其所属 job 与 watch，全部有明确终态后才 cancelled；任一
unknown 则 failed(cancel_unknown)，绝不假称全停。
Note: options differ in kind, not coverage — no completeness score.
Pros / cons:
A) 停止所属 job 与 watch (recommended)
✅ Task 取消与设备停止语义一致，不留下无人拥有的活动副作用。
✅ 部分 unknown 会明确失败并要求读状态，重复取消与迟到结果可幂等处理。
❌ 需要实现 cancelling、逐 job 停止、部分失败和恢复矩阵，复杂度最高。
B) Job detached 后继续
✅ Task 可立即 cancelled，设备长任务不被中断。
✅ 可保留后台作业与观察其结果的通道。
❌ 用户可能误以为已停止，必须明确展示 detached，否则语义危险。
C) 等待 Job 完成
✅ 不主动打断设备，终态后取消 Task 相对简单。
✅ 不需要处理立即 stop 的部分失败。
❌ 长任务可能让取消长期等待，且不能用于需要立即停止的场景。
Net: 本项决定取消与设备副作用的关系，不改变 Stop all 是独立设备级操作。
Header: 取消语义
A) 停止所属 job 与 watch (recommended)
M/L（human ~1 周 / CC ~1–2 天 [estimated]），中高风险，复用 stop/cancel 与去重。✅ Task
与设备停止语义一致，不留下活动副作用。✅ 部分 unknown 明确失败并有完整取消矩阵。❌ 需要实现 cancelling、逐 job 停止和恢复测试。
B) Job detached 后继续
M（human ~2–4 天 / CC ~0.5–1 天 [estimated]），中语义风险，Task 立即 cancelled。✅ 长任务不被中断。✅
可继续观察后台 job。❌ 用户可能误以为已停止，必须显式展示 detached。
C) 等待 Job 完成
M（human ~2–4 天 / CC ~0.5–1 天 [estimated]），中风险，cancelling 等待终态。✅ 不主动打断设备。✅ 无需立即
stop 的部分失败。❌ 长任务取消可能长期等待，不能用于立即停止。

Answer: 用户 D7 选择 A（停止所属 job 与 watch），2026-10-26。
Scope: Task 先进入 cancelling，取消 watch 与 pending outbox，并停止全部所属活跃 job；
全部有明确终态后才 cancelled；任一 cancelled_unknown 则 failed(cancel_unknown)。
重复取消不重复 stop，terminal Task 的迟到事件只更新投影，不再唤醒。

### 4.3 主链输入、变换与影子路径

```text
INPUT(text,key) → VALIDATION(workspace/admin/length/schema)
  → TRANSFORM(TaskRegistry.create) → PERSIST(Task+outbox, one tx)
  → DISPATCH(pending) → OUTPUT(taskId + runtime.v1 events)

空/错误文本/key       → 400，无 Task/outbox
重复 key 同 objective  → 返回原 Task，不重复 outbox
重复 key 异 objective  → 409，原 Task 不变
DB 冲突/写锁          → 限定重试，最终 500，无部分提交
Dispatcher 满/关闭    → pending 退避，dead+alert；Task 不假装已完成推进
```

Job result 仍需满足：先持久终态再响应；worker 的物理结果与 DB 失败通过重试、恢复和
去重处理，不能为了“成功响应”跳过账本。

### 4.4 并发顺序与回归证明

#### A. 同 key 并发创建

```text
请求 A: insert Task+outbox ──commit──返回 taskA
请求 B:        └─unique 冲突→读取既有 Task → 返回 taskA，deduplicated=true

顺序 B 先提交时结果相同。异 objective 的 B 收到 409，taskA 不变。
回归：两个 barrier 同时放行，断言一行 Task、一条 outbox、两个相同 taskId。
```

#### B. job result 与 Task cancel

```text
顺序 1：result 先持久终态 → cancel 查无活跃 job → cancelled；迟到唤醒被抑制。
顺序 2：cancel 先进入 cancelling → stop 产生结果 → 按结果决定 cancelled/failed。
不变量：Task 不被旧 result 覆盖；任一 cancelled_unknown 都不得变成 cancelled。
回归：控制 result/cancel 两向 pause-release，检查 state_version、stop 次数和唤醒次数。
```

#### C. 设备级 stop 与新调用（不是单 Job Stop）

```text
设备级 stop 先到：stop gate+claim 释放 → 后续 call 拒绝；stop 前 queued Execute 不执行。
call 先到：已进入 worker 的调用按取消协议终态；之后的新调用仍被拒绝。
回归：两个顺序都检查没有 stop 后副作用，队列中的命令不绕过已释放 claim。
单 Job Stop 不走此 gate，另按 job_id 验证同设备其他 Task 与新调用不受影响。
```

#### D. outbox 投递与标记

```text
dispatch 成功 → 进程在 mark dispatched 前崩溃 → pending 重试
  → agent_runs(task_id,wake_event_id) 唯一 → 仍只有一次 Run。
回归：模拟两次 dispatch 和一次崩溃，检查 outbox 可恢复且 Run 不重复。
```

#### E. 媒体与播放器

```text
同一 evidence 并发写入：dedup_key 唯一，后者复用，不产生第二行/第二份附件。
旧播放器加载中切换 Task：先 destroy 旧实例 → 再创建新实例；旧请求不得继续。
```

### 4.5 用户与数据边缘用例

| INTERACTION | EDGE CASE | HANDLED? / HOW |
| --- | --- | --- |
| Task create 双击/刷新/重试 | 重复 Task/Run | Y：强制 key + unique + 同事务 outbox |
| 审批双击/导航离开 | 重复执行或丢失审批上下文 | 产品契约：按 Task/approval 幂等并返回原投影；持久恢复与执行实现因 R2 blocked |
| Task pause/resume 与 wake 竞态 | 重复 Run/误停设备 | Y：paused 不启动或 steer，恢复按等待关系；不 abort Run、不停 job/watch |
| 同设备跨 Task Job Stop | 误停其他 Task/重复停止 | Y：Task/job 归属、job_id 队列与在途隔离、持久 stop_requested、重复返回投影 |
| 空设备/空关联集合 | 空白或假卡片 | Y：Loop 空态说明，Devices 不展示未关联设备 |
| 大型关联集合 | DOM/视频实例过多 | Y：无固定配额，采用增量渲染与按需媒体，监控实例和内存增长 |
| 设备删除/离线 | 卡片陈旧、watch 悬挂 | Y：失效事件+卡片状态；历史保留 tombstone |
| 老 cursor/重复 SSE 帧 | 重复步骤/丢状态 | Y：resync snapshot+event_id 去重，旧数据标 stale |
| 长任务 accepted 无结果 | UI 假进度 | Y：只显示 accepted/running，无百分比；dead 告警 |
| cancel 中再次 cancel/关闭页面 | 重复 stop/假完成 | Y：cancelling 幂等；unknown→failed |
| 返回键/历史 Task | 重新执行或误操作 | Y：只读投影，所有控制禁用 |
| 视频 CORS/断流/切 Task | 泄漏连接/假实时 | Y：显式 unavailable，destroy，后台请求断言 |
| MediaStore 满/证据过期 | 加载错误或错误替代 | Y：容量告警/410，不加载其他证据替代 |
| outbox 积压/重复投递 | 推进停滞或重复 Run | Y：pending 指标、dead 告警、wake_event_id 去重 |

**Decision gate：** D5-task-create 与 D6-task-cancel 已按实际回答应用；第二轮 R3 单 Job
隔离已补齐。R2 审批持久恢复仍 pending，容量与背压转到 Section 7。

## Section 5: Code Quality Review

### 5.1 模块与类型归属

- `agent/src/runtime/thing_agent/task/`：Task 状态机、Run 协调、WakeRouter。
`ThingAgentManager` 只做装配，不把 Task 方法继续堆进 `manager.rs`。

- `agent/src/port/task.rs`：TaskStore/outbox port；agent 不依赖 db。

- `runtime/src/job/`：Device Job 生命周期、终态去重和 worker 适配；`DataServer` 只执行，
不把 Job 全生命周期继续堆进 `data_server.rs` 或 `access/service.rs`。

- `runtime/src/media/`：SourceItem、媒体校验、MediaStore port。

- `db`：Row、迁移与全部 SQL；`core` 只放确实跨三个以上 crate 的值类型。

- `apps/cloud/src/domains/agent/runtime/`：Task handler/projection/SSE 组合。
- `apps/cloud/src/domains/media/`：播放描述与证据 handler；不新建 `utils/`、`helpers/` 或
`dto.rs`。

Task/Job/SourceItem 的共享值类型可进 core；HTTP DTO 留 cloud `types.rs`；
DB Row 留 db，禁止 re-export shim。

### 5.2 附件扩展而非文本重写

`ChatMessage` 保留 `role + content: String`，新增默认空的可选 attachments；
`ToolResult` 与 `TurnEvent::ToolResult` 同样添加附件，现有构造器与文本调用点不必重写。
Rig 适配器把附件映射为 provider 图片，trace 仅保留 evidence 引用。
禁止 provider 专用旁路，也禁止把 base64 拼进 prompt 文本。

### 5.3 复杂度与重复行为

- Task 状态迁移使用表驱动纯函数；WakeRouter、Job 终态和播放器生命周期各有单一所有者。

- RunReport 继续是 Run 的报告，TaskProjection 只组合 Run/Job/Watch/Event，不复制第二份报告。

- `device_id ↔ thing_id` 只在 API 别名层转换，禁止在 DB/引擎双写两套 ID。

- `cancelled_unknown` 与现有 `Outcome::UnknownCancelled` 建立单一映射，不平行造状态。

- 证据 `dedup_key` 由一个共享构造函数生成，避免 handler/adapter 各自拼接导致重复。

- `describe()` 保持 256 KiB 上限；图片走最终回调，但在 `into_owned` 前强制 2 MiB，
再叠加每次调用一张、每 Run 8 MiB 的预算。

- 分支超过 5 的 handler 必须拆成命名阶段，禁止把 Task/媒体/取消全部塞进一个 match。

### 5.4 错误与可测试性

- 错误类型按 Validation/Auth/ConcurrentUpdate/Unavailable/Unsupported/IoError 分类，
不用字符串匹配驱动业务分支。

- `TaskStore::transition` 用 expected_version，测试先证明旧版本零写入。

- `SourceItem` 的 unknown `dataKind` 走通用元数据，不把未知误当 values。

- `Task/Job` 状态反序列化遇到未知值必须显式 unknown/error，不能默认为 completed。

**Decision gate：** 当前模块与契约组织满足现有 relay/ports 约束。
未发现需要改变用户已批准范围的质量替代项；实现计划必须按上述小模块落点，而非继续扩大巨型文件。

### 5.5 工程复核

- `TaskStore`/`WakeOutbox` 归 `agent/src/port/task.rs`；`DeviceJobStore`/`MediaStore`
  归 `runtime/src/ports.rs`，保持 crate 依赖方向，不让 Task 直接依赖 db。
- `ChatMessage`/`ToolResult`/`TurnEvent` 用默认空 attachments 扩展，旧 JSON/构造器保持兼容；
  `ToolResultContent::Image` 用 `DocumentSourceKind::Base64/Raw` 传字节，不复制进 prompt 文本。
- `dedup_key` 用规范化 JSON 元组哈希，避免字符串拼接碰撞；相同重试复用证据与附件，
  相同内容跨 Task 可共享 hash 文件但关联行独立。
- `manager.rs`、`DataServer`、`AccessService` 不再吸收 Task/Job/Media 新服务，按
  `agent/task`、runtime/job、runtime/media 小模块实现，避免继续扩大巨型文件。
- 所有 workspace 校验在组合根与 DB 适配层共用，禁止 handler 和 runtime 各写一份
  导致跨区语义漂移。
- 第二轮 [INFORMATIONAL] (confidence: 10/10)：Task cancel 已批准的“abort 当前 Run、
  阻断新工具”原先未映射到代码。现按 D6-task-cancel 的必要实现补入 `Runner::execute`、
  `ActiveRunRegistry`、`manager` 与 rig loop 的同一 token 接线及 T3 测试；不是新架构选择，
  不改变 pause 语义，也无需另开用户决定。

**Decision gate：** 当前模块归属、附件扩展和去重实现方式满足 relay/ports 与兼容约束。
第二轮新增 1 个 cancel 接线缺口，已作为既有批准契约的必要代码与测试写回；没有改变
已批准结构的实现替代项。

## Section 6: Test Review

### 6.1 覆盖图

```text
CODE PATHS                         USER FLOWS / TEST TYPES
Task create/idempotency           双击/异 key/异 objective/崩溃重试
  ├─ Unit: text/key validation    → 400/409，无 Task
  ├─ Integration: unique+tx       → 并发仅一行/outbox
  └─ E2E: POST replay             → 同 taskId、deduplicated=true
Task state/wake                   正常/等待/暂停/取消/重启
  ├─ Unit: 状态机+优先级          → 非法迁移拒绝
  ├─ Integration: CAS+outbox      → 旧版本零写入、重复唤醒零新增 Run
  ├─ Race: pause/resume+wake      → paused 不启动/steer；resume 只按等待关系恢复一次
  ├─ Cancel: Runner/rig token     → abort 当前 Run，取消后不再派发工具；pause 不 abort
  └─ E2E: result→原 Task          → completed/failed 均符合事实
Device Job                        accepted/running/final
  ├─ Unit: 终态去重/超时          → 无 paused/百分比
  ├─ Integration: worker+DB       → 结果先落账再响应；job_id queued stop 不执行
  ├─ Isolation: 同设备两 Task     → 只停目标 job；另一 job 与后续 claim 不受影响
  ├─ Restart: stop_requested      → 重断言目标取消或 interrupted，不盲重放、不假 cancelled
  └─ E2E: >50s running→wake       → 同 Task 继续，不重复 job
Watch                             hit/timeout/invalid/restart
  ├─ Unit: 比较器/unknown         → unknown 不命中
  ├─ Integration: Task-owned      → hit 唤醒原 Task，重复事件只一次
  └─ E2E: restart                 → 非 terminal 重织，terminal 不重建
Media                             values/detections/image/video
  ├─ Unit: MIME/像素/dedup        → 超限/SVG/重复拒绝
  ├─ Integration: MediaStore      → 孤儿清理、证据引用、过期 410
  ├─ Integration: rig attachments → 图片到 provider；非 vision 显式失败
  └─ Browser: direct playback     → CORS 失败可见，destroy 后无请求
Workbench                         关联卡片/seam/Flow/历史
  ├─ Vitest: ToolCard/device card → 无场景组件、无伪造进度
  ├─ Component: 关联/定位/折叠     → 首次引用才出现，点击定位
  └─ Browser: 键盘/resync/媒体     → 无重复步骤、无后台播放、历史只读
Rollout/drain                    flags/存量 Task/unknown
  ├─ Integration: flag transition → off 只挡新建，不隐式 pause/cancel 存量 Task
  └─ Runbook: drain gate          → 未清零/unknown 未处置不得关 Workbench；完成后才关 UI/Media
Security/recovery                 跨区/崩溃/锁/迟到
  ├─ Integration: tenant+replay   → 跨区 404，崩溃不重复执行
  └─ Chaos: outbox/DB/Media 故障   → 可恢复或响亮失败，无静默成功
```

### 6.2 关键断言与 no_claim

- Requirement: Task 持续推进
- Observable assertion / rejected wrong result: result 持久化后唤醒原 Task；无 pending
才 completed；不得出现孤立 Run 或重复 job
- no_claim: 集成夹具，非真实硬件；不证明所有驱动行为

- Requirement: 创建幂等
- Observable assertion / rejected wrong result: 同 key 并发只有一行 Task/outbox；异
objective 409
- no_claim: DB 集成，不证明 provider 调用次数

- Requirement: stop/timeout
- Observable assertion / rejected wrong result: 目标 job 的 queued 命令不执行；in-flight
只取消匹配 job_id；同设备另一 Task 与后续 claim 不受影响；声明 deadline 真触发；
stop 请求后崩溃按持久身份恢复，重复 stop 不重复取消
- no_claim: fake worker，不证明真实设备物理停止

- Requirement: Task pause/cancel 边界
- Observable assertion / rejected wrong result: pause 与 wake 竞态时不启动或 steer 新 Run，
当前 Run、job、watch 继续；resume 按等待关系至多恢复一次；cancel abort Runner/rig 的同一
token 且之后无新工具调用
- no_claim: 并发集成测试，不证明所有模型提供商能及时响应取消

- Requirement: rollout drain
- Observable assertion / rejected wrong result: Task flag off 不改写存量 Task；非终态或
unknown 未处置时 Workbench 仍可用，drain 检查通过后才允许关闭 UI/Media
- no_claim: runbook 演练，不把配置检查当作物理急停

- Requirement: 重启恢复
- Observable assertion / rejected wrong result: accepted 后崩溃不重复派发；重复终态最多一次唤醒
- no_claim: 不证明 exactly-once 物理副作用

- Requirement: 图片到模型
- Observable assertion / rejected wrong result: vision 收到真实图片字节，trace 只有引用；非
vision 显式失败
- no_claim: mock provider；真实模型验收必须另跑 eval

- Requirement: 浏览器直连
- Observable assertion / rejected wrong result: 只返回脱敏 descriptor；Hub
不请求上游；CORS 失败可见；destroy 无后台请求
- no_claim: 本地视频夹具，不证明所有摄像头兼容

- Requirement: Workbench
- Observable assertion / rejected wrong result:
仅实际引用设备出现；点击定位；seam/Flow/历史只读正确
- no_claim: 组件测试；最终视觉效果需 live design review

- Requirement: 通用性
- Observable assertion / rejected wrong result: 两套声明只换数据即完成不同 Task，无场景分支或专用卡片
- no_claim: 只证明两个夹具，不证明未来所有设备

- Requirement: 性能
- Observable assertion / rejected wrong result: 本地 watch→Run `<2s`、job 状态到 UI
`<1s`
- no_claim: 不代表生产 p99，部署前需 staging 复测

### 6.3 2am / hostile / chaos

- **2am**：>50s job 返回 running，最终结果只唤醒原 Task；同 key 并发、重复终态、
stop 前 queued 命令、取消出现 `cancelled_unknown`。

- **Hostile QA**：非 vision 模型要求图片、伪造 PNG、超大 objective、重复 key 异 objective、
HLS 不支持签名 URL、跨 workspace 请求 evidence、播放器加载中连续切换。

- **Chaos**：accepted 后崩溃、outbox 已投递未标记、DB locked、媒体目录只读、
同一 evidence 并发写入、HLS 分片加载中切 Task；stop 与 queued 领取/in-flight 完成/
进程重启竞态，同设备跨 Task 停止隔离，pause/resume 与 wake 同时到达，drain 中途出现
`cancelled_unknown` 时不得关闭 Workbench。

### 6.4 LLM/eval 与基线

本设计改变 Task 上下文与附件，必须新增独立 runtime eval，不复用报警 verdict 准确率作为
完成证明。新增场景：

- `runtime-multi-device-01`：数值+图片+控制，要求正确 read/call/verify 顺序。

- `runtime-multi-device-02`：detections+不同工具/资源，无视频，仍按声明协同。

- `runtime-long-job-01`：长 job 返回 running 后利用结果继续，不重复调用。

- `runtime-image-unavailable-01`：非 vision provider 必须明确不可用，禁止声称已看图。

计划命令 `[planned]`：

```bash
MINIMAX_API_KEY=... cargo test -p tinyiothub-cloud runtime_eval -- --ignored
--nocapture
```

基线对比：`origin/main` 当前 prompt vs Task-aware prompt，比较正确工具选择、非法动作率、
假图片声明、Task 完成率、重复 job 数、prompt 字节/token 用量和模型上下文错误。没有可用
vision provider/key 时结果为
**unavailable**，不得记 PASS。现有 `judgment_eval` 仅作为既有回归哨兵，不替代 runtime eval。

### 6.5 金字塔、负载与可重复性

- 以纯函数/DB 集成/少量浏览器 E2E 为正金字塔；真实 LLM 与真实摄像头只做独立验收层。

- 时间、重试、过期和超时均注入时钟；不用 wall-clock sleep 证明状态机。

- 本地媒体与 provider 全部夹具化；网络、真实模型、真实摄像头不进单元测试。

- outbox 积压、媒体容量和 SSE 重连有负载测试；`<2s`、`<1s` 是待实测门槛，不是当前结果。

- 本轮只做了静态代码与文档核查，没有运行新功能的实现或实时模型/摄像头验收。

**Decision gate：** 当前测试范围与已批准行为一一对应，没有需要额外裁定的测试方法选择。
所有新功能测试均标为 required/planned，未把现有测试绿灯计作新能力证据。

### 6.6 工程复核

测试框架：Rust `cargo test` + web `vitest`（`RUNTIME:rust`，TESTFILES:141）。
当前没有新实现测试，以下全部为 required/planned；不得把历史全绿计作验收。

关键 value cards：

- `Task create`：protects=同 key 只创建一次；fails_when=并发双击产生两行或 outbox 重复；
  why_new=现有 POST 每次新 UUID；seam=none；no_claim=provider 调用次数（mock only）。
- `cancel`：protects=当前 Run 不再发新工具；fails_when=cancel 后仍调用 invoke_action；
  why_new=仅改 Task 状态会产生后续副作用；seam=CancellationToken；
  no_claim=不证明真实设备已停（fake worker）。
- `wake_event_id`：protects=重复终态只产生一个 Run；fails_when=只写 outbox 未写 RunReport，
  或 RunStarted 没带 wake_event_id；seam=RunStarted/RunReport；no_claim=不证明 exactly-once
  外部副作用。
- `owner/workspace`：protects=跨区 watch/session/outbox 不加载；fails_when=owner_task_id
  或 session_key 属于其他 workspace；seam=workspace guard；no_claim=仅 DB/handler 集成。
- `stop/timeout`：protects=目标 queued 不执行、in-flight 只停匹配 job_id、同设备跨 Task
  隔离、重启不遗忘 stop，声明 deadline 真触发；fails_when=按 thing 取消当前 token、误停
  另一 Task、FIFO 绕过 stop 或重启后丢失请求；seam=worker job_id/CancelSlot/result；
  no_claim=不证明真实物理停止。
- `image attachment`：protects=图片字节到 provider 且 trace 只有引用；fails_when=rig 静默丢图、
  非 vision 未显式失败；seam=rig provider；no_claim=provider acceptance（mocked provider; unit only）。
- `playback`：protects=只返回脱敏短时 URL；fails_when=接受 file/javascript/data、userinfo、
  过期 URL 或 CORS 失败仍显示实时画面；seam=playback handler/player；no_claim=真实摄像头兼容性
  需本地上游与 staging，不在单测证明。
- `Task context`：protects=pending job/watch、关联 ID、快照和阶段结果完整且顺序稳定；
  fails_when=新增 Task 段被隐藏截断、遗漏 pending 或拼接顺序随 HashMap 变化；
  why_new=现有快照测试不覆盖 Task 段；seam=prompt assembler；
  no_claim=不证明模型上下文永不超限，字节/token 与成本需 eval 和指标观测。

新增/更新测试必须放对应 owning module，而不是只测 helper。`runtime_eval` 是 `[planned]` 新套件，
现有 `judgment_eval` 仅作回归哨兵。所有失败、边界和跨区用例均标 required/planned。

第二轮 [INFORMATIONAL] (confidence: 10/10)：控制测试原先缺少同设备跨 Task stop、
stop/restart、pause/resume+wake 与 flag drain 竞态。上述测试是 R1、R3、既有 pause/cancel
批准契约的必要证明，已写入 6.1-6.3、T2/T3/T6/T8，不另开测试方法选择。R2 未批准，
因此没有新增可执行审批崩溃测试，相关验收明确 blocked。

**Decision gate：** 当前测试价值卡均对应真实回归风险，没有无意义的新测试。
D10 已选择无 Task prompt 总硬顶；测试固定完整拼接与确定顺序，eval 和指标负责暴露模型上下文与成本风险。

## Section 7: Performance Review

### 7.1 查询、索引与慢路径

- Task 详情必须批量查询 runs/jobs/watches/evidence，禁止逐设备或逐 Run 再查一次。

- `task_wake_outbox` 增加 `(dispatch_state,created_at)` 索引；dispatcher 每批最多 100 条。

- `media_evidence` 增加 `(workspace_id,expires_at)` 与 `(task_id,created_at)` 索引；
retention 每批最多 100 个证据，过期删除与容量指标分开记录。

- `agent_tasks`、`device_jobs`、`agent_runs` 的既有新索引覆盖列表、活跃查询和去重。

- LiveState/manifest 使用现有内存读面；视频帧与遥测不得触发 DB 查询。

- Top 3 慢路径 `[estimated, re-measure before gate]`：

1. Task detail 50 设备批量装配，目标 p99 <200ms；
2. 单张图片读取+证据写入，受声明 read timeout 与 2 MiB 上限约束，目标 p99 <2s；
3. runtime/stream 首帧 snapshot，50 设备目标 p99 <200ms。模型与上游视频延迟单列，不能混入这些指标掩盖。

### 7.2 连接、后台任务与内存

- outbox 单 DB writer 批量派发；失败指数退避上限 60s，dead 后告警，不无限热循环。

- 浏览器直连视频，Hub 不保留视频字节流连接；descriptor 刷新必须有界，播放器销毁终止上游请求。

- `ACTIVE_RUNS_CAPACITY=64`，D8 已明确不新增进程级固定图片字节上限；
只能通过流式处理、附件去重、背压指标和告警降低风险，不得声称有容量保护。

- MediaStore 不读全文件进内存做重复拷贝；二进制在 `into_owned` 前检查大小，哈希/写盘流式处理。

- 新 DB 连接只来自有界 API/outbox/retention 工作，遥测与视频不创建 DB 连接。

### 7.3 容量选择

`ACTIVE_RUNS_CAPACITY=64`（`manager.rs:56-60`）是当前实测常量。D8 已选择不设置
新的固定配额，因此背压只能来自既有 scheduler/Run 限制、流式处理、生命周期清理和
指标告警，不能把固定上限写成救援。

## answeredDecision (D8-resource-budget)

Commitment comparison:

- Commitment: active Tasks/workspace
- Source/approval or pending: 现有 `HOURLY_WAKE_LIMIT=20`（不是 watch 数上限）
- Current: 未定义
- A（保守配额）: 20
- B（较大配额）: 100
- C（无配额）: 不限

- Commitment: linked devices/task
- Source/approval or pending: 快照基线 50
- Current: 未定义
- A（保守配额）: 50
- B（较大配额）: 100
- C（无配额）: 不限

- Commitment: pending outbox/workspace
- Source/approval or pending: 需要背压
- Current: 未定义
- A（保守配额）: 1000，拒新增并落指标
- B（较大配额）: 5000，拒新增并落指标
- C（无配额）: 不限

- Commitment: 同时视频会话/workspace
- Source/approval or pending: 需要生命周期控制
- Current: 未定义
- A（保守配额）: 3，超出拒绝
- B（较大配额）: 10，超出拒绝
- C（无配额）: 不限

- Commitment: 进程图片字节
- Source/approval or pending: active runs=64
- Current: 理论 512 MiB
- A（保守配额）: 64 MiB，超出拒绝加载
- B（较大配额）: 256 MiB，超出拒绝加载
- C（无配额）: 理论 512 MiB+

- Commitment: 其他既有预算
- Source/approval or pending: D0/D1 approved
- Current: Run 8 MiB、图片 2 MiB
- A（保守配额）: 不变
- B（较大配额）: 不变
- C（无配额）: 不变

- Commitment: 可观测/运维
- Source/approval or pending: 启动必需
- Current: 未定义
- A（保守配额）: 拒绝原因+使用率+dead 告警
- B（较大配额）: 同左
- C（无配额）: 无可靠保护

- Commitment: Effort/risk
- Source/approval or pending: 估计
- Current: —
- A（保守配额）: M，中风险
- B（较大配额）: M，中高风险
- C（无配额）: S，高 OOM/运维风险

Question: D8 — D8-resource-budget: 首版 Task/Media 采用哪组容量预算
Project/branch/task: e4-p2-declaration-closeout，Task、outbox、视频会话与图片内存。
ELI10: 没有容量上限时，多建几个 Task 或同时读几张图片，就可能让
DB、内存和视频连接一起失控。容量需要在开发前变成明确拒绝与告警，而不是等事故发生再补。
Stakes if we pick wrong: 可能让单个 workspace 拖垮整个 Hub，或者让正常多设备任务被意外限死。
Recommendation: 采用保守配额（20 Task/50 设备/1000 pending/3 视频/64 MiB），因为它与现有 `HOURLY_WAKE_LIMIT=20`
和快照=50 基线一致，且把图片峰值从 512 MiB 降到 64 MiB。
Note: options differ in kind, not coverage — no completeness score.
Pros / cons:
A) 保守配额 (recommended)
✅ 与现有 `HOURLY_WAKE_LIMIT=20`、快照=50 对齐，首版容量和 UI 行为容易预测。
✅ 图片峰值有界，outbox/视频拒绝可观测，适合先验证真实负载。
❌ 大工作区可能更快触顶，需要后续按实测扩容。
B) 较大配额
✅ 支持更多设备、任务和并发视频，减少正常任务触顶。
✅ 仍保留明确背压与指标。
❌ 内存与 DB 压力显著增加，64 个 Run 下更容易出现长延迟和 OOM。
C) 不设配额
✅ 不会因为固定上限拒绝正常任务。
✅ 不需要调容量参数。
❌ 单 workspace 可拖垮 Hub，缺少可预测的背压和救援。
Net: 选择首版容量档位，后续只能用实测数据调整，不能假装容量无限。
Header: 容量预算
A) 保守配额 (recommended)
M（human ~2–4 天 / CC ~0.5–1 天 [estimated]），中风险，复用 watch/快照基线。✅
Task/outbox/视频/图片全部有界，拒绝可观测。✅ 图片峰值 64 MiB，适合先验证真实负载。❌ 大工作区可能更快触顶，需实测后扩容。
B) 较大配额
M（human ~2–4 天 / CC ~0.5–1 天 [estimated]），中高风险，保留全部背压。✅ 支持更多任务/设备/视频。✅
仍有明确拒绝与指标。❌ 内存和 DB 压力更大，长延迟与 OOM 风险更高。
C) 不设配额
S（human ~0.5 天 / CC ~15 分钟 [estimated]），高 OOM/运维风险。✅ 无固定上限拒绝。✅ 无需容量调参。❌ 单
workspace 可拖垮 Hub，缺少可预测背压和救援。

Answer: 用户 D8 选择 C（不设固定配额），2026-10-26。
Scope: 不为 active Task、linked devices、pending outbox、视频会话或进程图片字节设置新固定上限；
保留每图片 2 MiB、每 Run 8 MiB、active runs=64 与 scheduler 既有队列限制。
以流式处理、增量渲染、按需媒体、背压指标和告警处理压力；容量保护不作承诺。

**Decision gate：** D8 已按实际回答取消固定配额。批大小、流式写盘、播放器销毁、
增量渲染和背压指标仍为实现义务；OOM/DB 膨胀风险保留，不得宣称已关闭。

### 7.4 Task 上下文预算

`SNAPSHOT_MAX_BYTES=6144`（`prompt/snapshot.rs:11`）只覆盖现有设备快照。
新增 pending Task/job/watch 与阶段结果需要确定拼接顺序和可观测性；D10 已选择不设置
Task prompt 总硬顶，不能用新增隐藏截断控制成本。

| D10-context-budget / performance | `snapshot.rs:11` 已有 6144 bytes 上限 | 新 Task 元数据增加 prompt 成本与上下文超限风险 | 无总硬顶，完整拼接并观测 | approved | 用户 D10 选择 C |

## currentDecision (D10-context-budget)

Commitment comparison:

| Commitment | Source/approval or pending | Current | A（总 6KB，优先 pending） | B（总 8KB） | C（无硬顶） |
|---|---|---|---|---|---|
| 总 prompt 预算 | 现有 6144 bytes | 不含 pending Task | 全部 ≤6144 | 全部 ≤8192 | 无硬顶 |
| pending job/watch | 恢复所需事实 | 不可丢失 | 优先保留并压缩摘要/引用 | 同左但额度更宽 | 不做裁减 |
| 设备快照 | E2 已有摘要 | 可能挤压 | pending 后剩余额度，超限省略并计数 | 同左但额度更宽 | 不做裁减 |
| 阶段结果/证据 | 辅助推理 | 可能膨胀 | 最后截断并标注省略，不删除 pending | 同左但额度更宽 | 不做裁减 |
| token/成本 | E1/E2 预算约束 | 新增上下文不可控 | 保持现有预算，约不增加 | 增加约 33% | 不可预测 |
| 测试 | 必须具备 | 快照测试不含 Task | 总量/pending 保留/截断/省略顺序/中文 | 同左 | 只测拼接 |

Question: D10 — D10-context-budget: Task 上下文采用哪种预算与裁减顺序
Project/branch/task: 通用 Task Runtime，现有设备快照上限 6144 bytes，新增 pending Task/job/watch 与阶段结果。
ELI10: 如果所有内容都塞进 prompt，成本和截断顺序会失控。等待中的 job/watch 是恢复任务的关键事实，必须先保留；设备列表和历史结果可以摘要、计数或省略。
Stakes if we pick wrong: 可能丢失任务恢复所需状态，或者让 token 成本随设备数和历史无限增长。
Recommendation: 总 6KB 且优先 pending，因为现有预算已被验证，而且 pending 是恢复主链；设备快照与历史证据应被压缩或省略，而不是反过来挤掉等待关系。
Note: options differ in kind, not coverage — no completeness score.
Pros / cons:
A) 总 6KB，优先 pending (recommended)
  ✅ 保持现有预算，先保留恢复所需的 job/watch 与关联 ID，设备快照和历史按剩余空间摘要。
  ✅ 中文截断与省略计数可测，避免无限 token 增长。
  ❌ 大工作区会省略更多设备细节，需要让 Agent 通过 read_device 按需补读。
B) 总 8KB
  ✅ 为 pending 保留更多设备上下文，减少按需补读。
  ✅ 仍能保留确定截断与省略规则。
  ❌ 比现有预算增加约 33% token 与 prompt 成本。
C) 无硬顶
  ✅ 不需要做预算或裁减逻辑。
  ✅ 不会因额度不足遗漏 pending 或设备事实。
  ❌ token 成本不可预测，大工作区下提示词会无限膨胀。
Net: 决定 pending 优先级的预算方式，不决定设备类型或功能范围。
Header: 上下文预算
A) 总 6KB，优先 pending (recommended)
M（human ~2–4 天 / CC ~0.5–1 天 [estimated]），中风险，复用现有 6144 上限。✅ 保留恢复事实并限制 token 成本。✅ 中文截断/省略计数可测，Agent 可按需补读。❌ 大工作区省略更多设备细节。
B) 总 8KB
M（human ~2–4 天 / CC ~0.5–1 天 [estimated]），中风险，增加 Task 专项额度。✅ 保留更多设备上下文，减少补读。✅ 仍有确定截断与省略规则。❌ token 成本增加约 33%。
C) 无硬顶
S（human ~0.5 天 / CC ~15 分钟 [estimated]），高成本风险，只做拼接。✅ 无需预算裁减。✅ 不遗漏 pending 或设备事实。❌ token 成本不可预测，大工作区 prompt 会膨胀。

Answer: 用户 D10 选择 C（无硬顶），2026-10-26。
Scope: 不设置 Task prompt 总字节硬顶，不按总额度截断 pending job/watch、设备快照或阶段结果。
保留现有 `render_snapshot` 自身的 6144 bytes 设备快照边界，新增 Task 段完整拼接并采用确定顺序；
不得新增隐藏截断。测试验证 pending/关联 ID、快照与阶段结果均完整保留及输入顺序稳定；
以 prompt 字节/token 估算、模型上下文错误率和成本指标观测风险，不宣称已有容量保护。

State: approved
Actual answer: C（无硬顶），用户 D10，2026-10-26。
Accepted scope: 无 Task prompt 总硬顶；保留既有设备快照边界；完整拼接、确定顺序与成本观测。

第二轮工程复审：Performance 新增 0 个 issue。R3 只增加 job 身份与 stop 请求关联，沿用
`(task_id,state)`、`(workspace_id,device_id,state)` 活跃查询；R1 drain 是有界管理操作，
不引入固定 Task/device/outbox/video/process-image 配额，也不改变 D8/D10 的保留风险。

## Section 8: Observability & Debuggability Review

### 8.1 结构化日志与 trace

所有新代码路径在入口、成功、失败和关键分支记录结构化 tracing，字段包括：
`workspace_id/task_id/run_id/tool_call_id/job_id/device_id/source_id/source_seq/event_id`。
禁止记录 objective 原文、图片内容、签名 URL query、驱动凭证和 Authorization。
`runtime.v1` 是跨 Run/Job/Watch/UI 的关联信封，provider 错误也带同一 run_id。

### 8.2 指标

- 范围: Task
- 工作信号: created/completed/failed/cancelled、active、Run 数、等待时长、prompt 字节/token 估算
- 故障信号: wake_lag、waiting 超期、非法迁移、重复唤醒、模型上下文错误与成本异常增长

- 范围: Outbox
- 工作信号: pending、 dispatched、 oldest、 dispatch latency
- 故障信号: pending 持续增长、dead、retry 失败、queue closed

- 范围: Device Job
- 工作信号: accepted/running/终态、执行时长、busy
- 故障信号: timeout、cancelled_unknown、重复终态、stop 后执行

- 范围: Watch
- 工作信号: active、命中、唤醒延迟
- 故障信号: timeout、失效、重织失败、Lagged

- 范围: Image/evidence
- 工作信号: 读取成功、证据字节、附件次数
- 故障信号: 非 vision、超限、过期、孤儿、MediaStore 错误

- 范围: Video/playback
- 工作信号: active session、descriptor 刷新、播放错误
- 故障信号: CORS/签名/协议/断流、后台连接未销毁

- 范围: Runtime stream
- 工作信号: 首帧耗时、连接数、resync
- 故障信号: 重连风暴、重复帧、cursor lag

- 范围: DB/process
- 工作信号: 事务耗时、锁冲突、RSS、磁盘用量
- 故障信号: lock 错误、RSS>75% 容器限额、磁盘>80%

容量指标只用于背压、降级和告警，不与 D8 的“无固定配额”矛盾。

### 8.3 Day-1 dashboard 与告警

Dashboard 展示：active/waiting/failed Task、outbox pending/oldest、running/unknown
job、
watch 命中与 wake lag、image 失败、视频 active/error、MediaStore 字节、RSS 和 SSE resync。

告警：

- `outbox_dead > 0` 或 oldest pending >60s / pending 连续 5 分钟增长。

- `cancelled_unknown > 0`、stop 后仍出现命令执行、job 超过声明 deadline 仍未结束。

- MediaStore 磁盘 >80%、进程 RSS >75% 容器限额、图片失败率异常上升。

- SSE 重连风暴、resync 后重复 event、视频播放错误或后台连接未销毁。

- waiting_job 超过 job deadline+60s 没有终态。

### 8.4 管理入口与 runbook

- `GET /admin/runtime/outbox?state=dead`：先定位 scheduler/DB/权限错误，修复后用
`POST /admin/runtime/outbox/{event_id}/retry` 按原 event_id 重试，禁止直接删除
pending/dead 行。

- `GET /admin/runtime/jobs?state=active|unknown`：先读设备真实状态，再人工裁定；
`cancelled_unknown` 禁止盲目重试，必要时先 stop 其他相关 job。

- Runtime drain（R1/D1=A）：先关闭新持久 Task 创建，保留 Workbench；从 Task 列表固定
  非终态 ID 批次并逐个调用既有 cancel，不做批量隐式状态改写。用 Task 详情、admin jobs
  和只读 DB/投影查询核对非终态 Task、活跃 Run/job/watch/pending outbox 均归零；
  每个 `cancel_unknown` 以 task/job/device ID 记录人工核查与处置。清单未完成时不得关闭
  Workbench，完成后才关 UI/Media；该流程不是物理急停。

- 图片/证据失败：检查 `data/media` 权限与磁盘、孤儿清理任务和证据元数据；
不用旧图替代，不手工改 hash。

- 视频失败：检查签名 URL 过期、origin、CORS、HTTPS 和上游可达性；不得用代理绕过裁定。

- 非 vision：检查 provider capability，不将图片转文本伪造“已读”。

- DB locked：检查事务时长、批量大小和并发写冲突，先做有界重试，不以重启作为第一响应。

三周后的回放必须能由 ledger + RunReport + device_jobs + evidence_ref 还原
“哪个 Task 在哪一步感知到什么、调用哪台设备、为何等待/取消/失败”。

**Decision gate：** 所有新失败都有结构化日志、指标、告警和人工处置路径。
没有把指标当作固定配额，也没有把现有可观测性当作尚未实现的完成证明。

## Section 9: Deployment & Rollout Review

### 9.1 迁移与兼容

- 原方案为先合入 PR #101 再开发 Runtime；2026-10-27 D6 用户改为在当前
  `e4-p2-declaration-closeout` 分支继续实现，声明收口与完整 Runtime 最终全部并入现有
  PR #101，不另开 Runtime PR。以该 PR 的声明提交 `6876707f` 为代码基线，保留分阶段提交、
  三个默认关闭 flags 和完整验收；push/merge 仍需单独授权。

- 新表/列均为 additive DDL：`agent_tasks`、`task_wake_outbox`、`device_jobs`、
`media_evidence`，以及
`agent_runs.task_id/wake_event_id`、`watch_rules.owner_task_id`。

- 新唯一索引只作用于新表或既有 NULL 列，不重建历史表、不写 DML；迁移使用短维护窗口，
启动后执行 `foreign_key_check` 与既有 baseline 对照。

- `manifestVersion=1.1` 新字段对旧消费者按 unknown 忽略；旧 Hub 不需要理解 `dataKind/key`。

- 老 binary 忽略新表/列，旧 `POST agent/tasks` 指令路径保持可用；新 binary 在开关关闭时
也走同一路径，保证新旧并存与回滚。

- `device_jobs`、`media_evidence` 和 outbox 历史不因回滚删除，旧 UI 不显示它们。

### 9.2 Feature flags 与启用顺序

在既有 `workspaces.heartbeat_config` JSON 增加三个默认关闭的 rollout 开关：

- `runtime_tasks_enabled`：关闭时 `POST agent/tasks` 走旧 UserDirective 路径，只阻止新持久
  Task 创建；不隐式暂停、取消或改写既有 Task，既有 Run/job/watch/outbox 仍按原契约推进。
  该开关不是紧急 kill switch；停止副作用必须显式使用 Task Cancel 或 Job Stop。

- `runtime_media_enabled`：关闭时 read_device 不返回图片附件或 playback descriptor。

- `runtime_workbench_enabled`：关闭时 `#/runtime*` 不注册/不展示，其他 AI 页面不变。

启用顺序：迁移 → 部署默认关闭 → staging 开 Task → 开 Media → 开 Workbench →
逐项验证后扩大工作区。每一段只启用已验证部分，不用“全量一次开灯”。

### 9.3 Staging 与 smoke checks

- Linux 文件库跑完整迁移链，不只内存 baseline；HLS 用本地源分别覆盖 CORS 成功/失败，
验证浏览器直连而非 Hub 代理；hls.js 使用 Node 24 构建与测试。

- 自动 smoke：Task create 重放、job 长调用唤醒、stop queued、同设备跨 Task 停止隔离、
非 vision、脱敏播放描述、跨区拒绝、outbox 重试去重。

- Flag/rollback smoke：关闭 Task flag 后新持久 Task 走旧路径而既有 Task 不被隐式改写；
drain 未完成时禁止关闭 Workbench，完成后核对非终态 Task 与活跃 Run/job/watch/outbox
均为 0，且每个 unknown 都有管理员处置记录。

- 前 5 分钟：迁移/FK 检查通过；旧 API 健康；同 key 同 id；首个 job 有真实终态；
UI 无 paused/百分比；runtime stream 可连接。

- 前 1 小时：watch→Run <2s、job 状态到 UI <1s；dead=0、unknown=0（或均有处置记录）；
图片到 vision 或明确不可用；视频切换无后台请求；指标和告警可见。

### 9.4 回滚流程

```text
保持 Workbench/Media 可用，关闭 runtime_tasks_enabled（只阻止新持久 Task）
  → 列出并逐个显式取消非终态存量 Task，不批量隐式改状态
  → 等待 Task 到达确定终态；任一 cancel_unknown 必须由管理员核查设备并留处置记录
  → 确认非终态 Task=0、活跃 Run/job/watch/outbox=0，且 unknown 均有处置记录
  → 关闭 runtime_workbench_enabled，再关闭 runtime_media_enabled
  → 旧 binary/旧 UI 继续运行，保留 additive 表列与证据
  → 修复后按 Task → Media → Workbench 顺序重新启用
```

预计维护窗口约 15 分钟 `[estimated]`，实际以 drain 与 unknown 核查完成为准，不能为赶窗口
跳过检查或先隐藏 Workbench。不 drop 新表、不删除证据、不做 destructive 数据回滚。

### 9.5 交付批次选择

默认 ship policy 为每波 1 个 PR（无 `.gstack/ship-policy.json`）。三切分可以独立回滚，
但跨 PR 的 `runtime.v1`、Task 状态、SourceItem 和播放描述必须保持 additive 兼容。

## answeredDecision (D0-pr)

Commitment comparison:

- Commitment: 批次
- Source/approval or pending: 三切分已批准
- Current: Task+Job / Media+UI / 恢复+通用
- A（3 PR）: 每切分一个 PR
- B（2 PR）: 后端+媒体一波，UI+恢复一波
- C（单 PR）: 全部一波

- Commitment: 审查/回滚
- Source/approval or pending: HOLD SCOPE
- Current: 大分支风险
- A（3 PR）: 每波最小，易定位回滚
- B（2 PR）: 中等
- C（单 PR）: 最大，跨域问题难拆

- Commitment: 跨 PR 兼容
- Source/approval or pending: 需要稳定契约
- Current: 未实现
- A（3 PR）: 必须保留 additive 兼容测试
- B（2 PR）: 同左但批次较少
- C（单 PR）: 无跨 PR 兼容成本

- Commitment: 用户可见完整体验
- Source/approval or pending: 两栏+媒体是目标
- Current: 尚未交付
- A（3 PR）: 第三波才完整
- B（2 PR）: 第二波完整
- C（单 PR）: 一波完整

- Commitment: 队列成本
- Source/approval or pending: 默认每波 1 PR
- Current: 可接受评审节奏
- A（3 PR）: 3 次评审/合并
- B（2 PR）: 2 次评审/合并
- C（单 PR）: 1 次但最大

- Commitment: Effort/risk
- Source/approval or pending: 估计
- Current: —
- A（3 PR）: 多两次评审，低集成风险
- B（2 PR）: 中等
- C（单 PR）: 单次评审，高集成风险

Question: D9 — D0-pr: 通用 Runtime 如何拆分交付批次
Project/branch/task:
e4-p2-declaration-closeout，Task+Job、Source/Media+Workbench、恢复与通用性三切分。
ELI10: 全部范围保留，但可以分批交付。批次越大，越快看到完整界面；批次越小，越容易发现主链问题并安全回滚。
Stakes if we pick wrong: 可能让评审再次失效，或者让用户在多个 PR 后才看到任何完整体验。
Recommendation: 三个 PR，因为 Task+Job 必须先证明，再接媒体与 UI，最后用第二套夹具和恢复矩阵收口；每波都能独立审查和回滚。
Note: options differ in kind, not coverage — no completeness score.
Pros / cons:
A) 三个 PR (recommended)
✅ 每波边界清晰，Task+Job、媒体 UI、恢复通用性可独立审查和回滚。
✅ 主链问题不会拖到完整界面完成后才暴露，跨 PR 可用 additive 契约控制。
❌ 需要三次评审/合并，用户到第三波才看到完整体验。
B) 两个 PR
✅ 后端与媒体先稳定，第二波 UI 与恢复即可完成，减少一次队列成本。
✅ 比单 PR 更容易回滚和分派审查重点。
❌ 第一波仍融合 Task/Job 与媒体，故障定位与审查面较大。
C) 单个完整 PR
✅ 一次看到全部功能，没有跨 PR 接口兼容成本。
✅ 不需要等待多个波次。
❌ 多表、Runtime、媒体和前端同时变更，评审与回滚最困难。
Net: 只决定交付节奏，三切分的功能范围不因此增减。
Header: 交付批次
A) 三个 PR (recommended)
M（human ~3–5 周 / CC ~3–5 天 [estimated]），低集成风险，每波独立验证。✅ 每波可独立审查/回滚，主链先证明。✅
additive 契约控制跨 PR 兼容。❌ 三次评审/合并，完整体验最后才出现。
B) 两个 PR
M（human ~3–5 周 / CC ~3–5 天 [estimated]），中集成风险，减少一次队列。✅ 后端媒体先稳定，UI 恢复后完成。✅ 比单 PR
更易回滚。❌ 第一波仍融合 Task/Job 与媒体，审查面较大。
C) 单个完整 PR
M（human ~3–5 周 / CC ~3–5 天 [estimated]），高集成风险，无跨 PR 兼容成本。✅ 一次完整交付。✅ 无需等待多个波次。❌
多表/Runtime/媒体/前端同时变更，评审和回滚最困难。

Answer: 用户 D9 选择 C（单个完整 PR），2026-10-26。
Scope: 一次完整评审与合并，无跨 PR 兼容成本；Task+Job、Source/Media+Workbench、
恢复与通用性仍按三个提交/启用切分，默认关闭开关分阶段启用。

**Decision gate：** D0-pr 已按实际回答确定单 PR 交付。单 PR 不改变范围，但必须保留
分阶段提交、全量审查和默认关闭的 rollout 开关，不能以单 PR 为由一次性全量启用。

## Section 10: Long-Term Trajectory Review

- **可逆性：4/5。** 新表/列、manifest 1.1、播放描述和 UI 路由均可回滚且无需删数据；
单 PR 的审查成本、Task 状态迁移和已发生的物理副作用使它不是 5/5。

- **路径依赖：正向。** Task/Job/Watch/evidence 的统一关联可服务后续 GoalWatch、异常检测、
场景包与 E5；OpState 可替换 Run 恢复实现而不重写 Workbench。

- **明确债务：** 单 PR 很大；rig 附件契约跨 provider；签名播放 URL 依赖上游实现；
D8 无固定配额；D10 无 Task prompt 总硬顶；outbox/ledger/Task 历史持续增长。

- **清理纪律：** `task_wake_outbox` 的 dispatched 行保留 30 天，dead 行只在人工处置后
再按处置时间保留 7 天；清理只删过期审计，不删 pending。Task/job/ledger 本版不做
破坏性 retention，先以体积指标决定是否另立保留策略。

- **未来显式重开：** 真实百分比进度、Job pause/resume、OpState、固定配额、Task prompt 总硬顶和 Hub 视频代理
都被本次裁定排除；只有出现真实需求与新证据才重新评审，不能留下隐式入口。

- **生态与可读性：** relay/ports、DB 单一 SQL 住所、Lit 组件和现有 Run/Watch 先例一致。
新工程师可按 Task 状态机、Job 生命周期、SourceItem 和 runtime.v1 四条契约理解系统。

## Section 11: Design & UX Review

### 11.1 信息层级与用户旅程

```text
提交 objective
  → 当前 Reason/Act（为什么做）
  → 具体设备/图像/遥测证据（依据是什么）
  → 设备调用 accepted/running（正在发生什么）
  → 真实 result（成功/失败/未知）
  → Continue（下一步）或 completed/failed/cancelled（终态）

任何一步都可点设备 → 右侧定位该设备卡；任何历史状态都不允许再执行。
```

| STEP | USER DOES | USER FEELS | PLAN SPECIFIES? |
| --- | --- | --- | --- |
| 1. 进入 Runtime | 扫 Current/Recent，判断哪些 Task 需要关注 | 有方向感，而非面对指标墙 | 是：按状态分组、objective/等待原因/最近活动优先，New task 唯一主行动 |
| 2. 提交 objective | 创建持续 Task 并等待 accepted | 期待但不怀疑是否重复创建 | 是：持久 Task、幂等结果、accepted 后进入同一 Task；不伪造即时完成 |
| 3. 首次打开详情 | 在 5 秒内读标题、状态和当前推进 | “我知道它在做什么” | 是：objective H1、Task 状态、当前 Run/等待原因三层优先级 |
| 4. 核查依据 | 从 Loop 定位设备、图片和遥测证据 | 从好奇转为有据可信 | 是：关联设备规则、证据来源/时间、stale 与模型感知边界 |
| 5. 等待 job/watch | 查看等待对象和已等待时间，继续只读检查 | 平静，不因无百分比而猜测 | 是：真实 accepted/running/waiting 状态，无假进度，其他证据仍可读 |
| 6. 收到审批 | 就地审查设备、参数与原因后决定 | 紧张但保有控制权 | 是：内联 Review details、Approve once/Reject、服务器结果为准 |
| 7. 调查历史 | 在 Loop/Flow 对照过去事实，再回到实时 | 不迷路，不混淆过去与现在 | 是：共享 at/event、持续 History 横幅、Back to live |
| 8. 遇到断线/失败 | 保留最后有效上下文，理解缺口与恢复状态 | 担忧但不被突然白屏抛弃 | 是：stale/最后更新时间、局部错误、resync 与列表回退 |
| 9. 取消或停止 | 先确认影响，再跟踪真实副作用终态 | 能干预，也知道何时仍不确定 | 是：Task/Job 分离确认、cancelling/stopping、cancel_unknown 明确警示 |
| 10. 完成与复盘 | 阅读最终结论和证据，之后返回列表 | 完成感与可审计的长期信任 | 是：completed/failed/cancelled 不混用，历史证据与 Recent 保留 |

时间尺度验收：

- **5 秒：** objective、当前状态、需要的干预先于实现 ID 和统计被看见。
- **5 分钟：** 用户能不靠刷新完成核查证据 → 审批/停止 → 历史对照 → 返回实时。
- **5 年 / 长期：** 重复使用后仍相信状态与设备事实；任何 unknown、stale 和失败都
  保留诚实的不确定性，不用动效、假进度或成功文案掩盖。

首要信息是 Agent 的推进，其次是关联设备事实，最后才是统计与实现 ID。
没有 Goal 管理页、独立 World 栏或深色大仪表盘。

### 11.2 状态覆盖

矩阵描述用户看到的界面，不把后端状态名当作完整 UX。首次加载使用结构 skeleton；
后续刷新保留已读内容并显示局部 updating。错误不清空最后一次有效数据；保留时
必须标 `Stale` 与最后更新时间。终态、等待与降级均使用文字说明，不互相冒充。

| FEATURE | LOADING | EMPTY | ERROR | SUCCESS | PARTIAL |
| --- | --- | --- | --- | --- | --- |
| Task 列表 | Current/Recent 行 skeleton，保留页面标题与 New task | 显示 `No runtime tasks yet`、说明持续 Task 的用途，主行动为 New task | 已读行保留并标 stale；顶部错误条给出原因与 Retry，不用错误替换整页 | Current/Recent 分组、状态和最近活动时间可见 | 单组为空不影响另一组；分页失败保留已读行并在末尾提供 Retry |
| Agent Loop | 保留 objective/头部，时间线 skeleton；后续推进不清空旧步骤 | 显示 `No runs yet`，说明 Task 已创建、等待首次推进；无伪造 ToolCard | Run 失败原因、可理解的恢复提示和只读历史保留；失败步骤不显示 Done | 按序显示 Reason/Act/Tool/Result；completed 明确显示最终结论 | waiting_job/waiting_watch 显示等待对象与开始时间；部分证据不可用时保留其余步骤并就地说明 |
| 关联设备 | 右栏结构 skeleton，不清空 Loop | 显示 `No devices referenced yet`，说明实际 read/call/watch 后才会出现，不提供无效设备选择 | 离线/删除设备保留名称、最后状态和 tombstone/stale 文案；详情行动按可用性禁用 | 按首次引用顺序显示声明、health、关键字段和 Task 关系 | 单设备失败不拖空整栏；旧值标 stale，未知值显示 `Unknown` 而非 0/正常 |
| Job | 首次读取显示状态 skeleton；accepted/running 显示真实状态与已等待时间 | 无 job 时不渲染进度或空进度条，仅在该 ToolCard 显示 `No active job` | rejected/failed/interrupted 显示原因与设备；unknown 显示结果未确定 | done/cancelled 显示终态、完成时间与结果摘要，不显示百分比 | cancelling/cancelled_unknown 明示设备侧停止未完全确认，禁止显示 `All stopped` |
| 图片证据 | 缩略图区域 skeleton 与加载说明，证据元数据先可读 | Evidence 区显示 `No image evidence`；不渲染破图或空 modal | 非 vision/超限/校验失败/过期分别说明原因；过期图不当作当前事实 | 显示图片、来源、时间、证据关联和 Details；模型不可读时不得标为已感知 | 单张失败保留其他证据；unsupported/expired 在原位置说明，可提供的重试/刷新入口就地出现 |
| 视频播放 | 播放器区域显示获取播放描述状态，保留设备与证据上下文 | 无声明视频时显示文本说明，不显示假 poster 或不可用播放按钮 | CORS/签名/协议/断流分别给出可见原因；失败时不以静态帧冒充播放成功 | 原生播放器显示时长、暂停和播放位置；过期后明确刷新播放描述 | 重连时保留上下文并标缓冲/离线；不支持时提供证据详情而非黑屏 |
| SSE / Flow | 保留当前快照，显示 Connecting；首次无快照使用结构 skeleton | 已连接但无事件时显示 `Waiting for the first event`，不编造节点 | 断连显示 Disconnecting/Reconnecting 与最后更新时间；旧快照标 stale，提供重试状态 | 显示 Live、有序追加和 following 状态；resync 后不重复步骤 | 事件缺口显示 `Updates may be incomplete`，resync 期间保留可读旧快照与列表回退 |
| 审批 | 请求区 skeleton，其他 Loop 内容保持可读 | 无 pending 请求时不渲染空审批卡或按钮 | unavailable/expired/rejected 分别显示结果与不可执行原因，不保留可点的 Approve | approved 后显示审批人/时间（若已有投影）并继续原 Task，不新建重复步骤 | needs_approval 显示动作、设备、原因与风险；其他设备证据仍可只读检查 |

### 11.3 设计系统与可访问性

- 采用紧凑 ToolCard 与既有 typography/token；表面遵循 dark/light 主题 token，
  不用可见 border，以 inset 高光与阴影表达层级；Reason/Act/Result、
审批原因等连续正文使用 16px，设备数值、时间戳和密集元数据使用 14px，
辅助文字 ≥12px；不使用彩色 glow 或低对比灰字。CSS 超 200 行拆为同目录 `.css` 文件。

- 双主题按实际表面/文字组合验证 WCAG AA：普通正文与 14px 数据对比度 ≥4.5:1，
  大字 ≥3:1，焦点环与必要非文本控件对相邻颜色 ≥3:1。stale/history/muted 等
  重要事实不得借 disabled 例外降低可读性；覆盖默认、hover、focus、error 和
  offline 状态，不只检查静态默认色。

- 主 UI 字体沿用现有 `--font-body` / `--font-display` 系统栈与中文回退，mono
  沿用 JetBrains Mono；本期不增加 Runtime 专属字体资产。默认字体的通用感作为
  已接受的视觉缺口保留，不宣称已消除。

- 深色用于长时间监控与低照度操作环境，light 用于明亮现场；跟随现有主题选择，
  不按“运维工具”类别强制单一主题。具体映射如下：

| 设计面 | 既有契约 |
| --- | --- |
| 表面/文字 | `{colors.deep-space}`、`{colors.surface-card}`、`{colors.surface-elevated}`、`{colors.text-primary}` 对应的双主题 token；不新增 Runtime palette |
| 行动/状态 | `{colors.signal-blue}` 仅用于交互/焦点；`{colors.ok-green}`、`{colors.warn-amber}`、`{colors.danger-red}` 配文字/形状；焦点使用 `--focus-ring` |
| 形状/间距 | 面板 `{rounded.lg}`、控件 `{rounded.md}`；间距使用 `--space-1`…`--space-9`，不写零散像素值 |
| 深度 | 使用 `--shadow-md`/`--shadow-lg` 与 inset 高光；装饰性零偏移 halo 和静态 glow 不进入 Loop |
| 动效 | 使用 `--duration-fast`/`--duration-normal` 与 `--ease-out`；弹簧/bounce 不用于状态反馈 |
| 局部扩展 | 只有重复出现的 Runtime 时间线/双通道样式才建立 `--runtime-*` view-scoped tokens，不新增全局色板 |

- 组件复用边界：
  - 扩展 `run-timeline` 的投影与样式以表达 waiting_job/waiting_watch/approval/
    cancelled_unknown，不把这些状态伪装成 done/active/failed。
  - 复用 `live-devices` 的健康、关键字段、watch/job 徽标与键盘卡片模式；卡片与
    详情按钮保持独立，不嵌套交互控件。
  - 复用 Flow 的 live/history、键盘时间控制、代际守卫和列表回退；Runtime 只增加
    Task 投影与 CONTROL/DATA 语义，不另造时间导航。
  - 借鉴 `workbench-state` 的纯函数 URL 模式和 `run-detail` 的异步代际守卫；
    不复制其 device 选中即历史的语义，也不把 3s polling 带入新 Runtime SSE。
  - 复用设计系统 dialog/focus 规范与全局 selection/scrollbar/tabular-nums；
    新组件不覆盖这些浏览器表面为浏览器默认样式。

- 响应式遵循 7.6：桌面双栏、平板单栏、手机 Loop/Flow/Devices 标签；触摸目标
  至少 44px，200%/400% zoom 与 Flow 列表回退纳入验收。

- 设备链接、Details、停止和 seam 均可键盘操作；separator 有名称、宽度和展开状态。

- 状态不只依赖颜色，错误有文本；图片有替代文本；视频可暂停；减少动态效果时
  停止所有非必要动画与平滑滚动，不只停用粒子。

- API 只走 `web/src/api` 层，不在组件直接 fetch；不在 Lit 模板新增 unsafeHTML。

### 11.4 设计评审要求

本设计属于显著 UI scope。`/plan-design-review` 已完成七个 pass，新增列表/创建/详情
信息架构、完整交互状态、响应式与可访问性契约；总体分 4→8，剩余字体与 captions
为用户明确 deferred 的缺口，不计为已解决。

**Decision gate：** 21 个设计问题均有独立回答，19 项进入本期规格，2 项 deferred 并
写入 TODOS。没有生成或批准 mockup，独立 design outside voice 按用户选择 skipped。
新增创建流程、pause/resume 与审批/停止 API 后应重跑 `/plan-eng-review`；实现后必须
运行 `/design-review` 对真实页面验证两栏、seam、关联卡片、视频、双主题和错误状态，
不能只凭 Vitest 判定视觉完成。

## Decision ledger

### R1: Runtime 禁用时的存量 Task 处置
Finding: [CRITICAL] (confidence: 9/10) 本文件 :2588-2634 — 三个开关只定义新创建、媒体和 UI 的关闭行为；先关 Workbench 再关 Task 的回滚顺序未定义存量 Task/Run/job/watch/outbox，可能让仍在推进的任务失去操作入口。第二轮工程评审。
Plan baseline: flags 默认关闭、单完整 PR、`runtime_tasks_enabled=false` 时创建走旧 UserDirective 路径；没有存量 Task 的禁用裁定。
Runtime evidence: 当前持久 Task Runtime 尚未实现，不能从旧 UserDirective 路径推导新开关的存量行为；现有 Task 取消闭环已批准停止所属副作用并保留 `cancel_unknown`。
Comparison grid:

| Choice | Current | A | B | C |
| --- | --- | --- | --- | --- |
| 新 Task 创建 | flag off 走旧 UserDirective | 不变 | 不变 | 不变 |
| 非终态存量 Task | 未定义，pending | 仅由操作员显式取消，否则保持原推进语义 | 增加 rollout 来源的 paused，区分用户主动 paused | 进入既有 cancelling 闭环 |
| 当前 Run / job / watch | 未定义，pending | 未显式取消则继续；drain 时按 cancel 闭环终止 | 按 pause 契约继续，不 abort、不停止 | abort Run，停止所属 watch/outbox/job |
| Workbench 关闭顺序 | 当前先关 UI | 先取消并非终态清零/处理 unknown，再关 UI | 先确认无活跃 Run/job，再关 UI | 等取消闭环与 unknown 处置后再关 UI |
| 单 PR / flags 默认关闭 | 已批准 | 不变 | 不变 | 不变 |
| 其他审批/停止修复 | pending | 仍 pending | 仍 pending | 仍 pending |

Question D1:
D1 — 禁用 Runtime 时如何处理存量 Task
Project/branch/task: e4-p2-declaration-closeout，通用 Runtime 的三开关灰度与回滚。
ELI10: 关闭“新建任务”开关，不会让已经创建的任务自动停下。若先藏起 Workbench 而旧任务仍唤醒 Run，用户将看不到、也停不了后续设备动作。
Stakes if we pick wrong: 回滚可能留下无人可见的设备副作用，或仅因配置切换就批量中断物理操作。
Recommendation: A) 显式 drain 后关闭，因为它不把配置开关变成隐式批量物理控制，同时给回滚一条可验证的安全顺序。
Note: options differ in kind, not coverage — no completeness score.
Pros / cons:
A) 显式 drain 后关闭 (recommended)
  ✅ `runtime_tasks_enabled=false` 只阻止新持久 Task 创建；既有 Task 不被隐式改写，回滚前先逐个取消非终态 Task 并等待确定终态或处理 `cancel_unknown`，最后才关闭 Workbench。
  ✅ 保留三个 flags 默认关闭和单 PR 既定方案；只需补 runbook、状态检查与 smoke，human ~0.5d / CC ~30min [estimated]，维护成本低。
  ❌ 它不是紧急 kill switch；跳过 drain 仍会让旧 Task 推进，操作员必须看到并执行前置检查。
B) 禁用即全部暂停
  ✅ 禁用时将非终态 Task 统一置为 rollout 来源的 paused，保留 job/watch 与事件，重新启用后按等待关系恢复，避免新增 Run。
  ✅ 与已批准 pause 的设备语义一致，不批量取消物理 job；human ~2d / CC ~2h [estimated]，需要持久批量状态迁移和恢复测试。
  ❌ 当前 Run 与设备 job 仍继续；还必须区分 rollout paused 与用户主动 paused，否则重新启用会误恢复用户原本暂停的任务。
C) 禁用即全部取消
  ✅ 禁用时走既有 Task cancel 闭环，abort 当前 Run 并停止所属 watch/outbox/job，提供最强的回滚隔离。
  ✅ 复用 cancelling/cancel_unknown，不另造设备停止结果；human ~3d / CC ~3h [estimated]，需要批量取消进度与失败处置。
  ❌ 配置开关会触发真实物理副作用，可能中断仍需完成的设备动作；unknown 会阻塞干净回滚，不能承诺立即安全。
Net: 只决定禁用/回滚对既有任务的控制语义；不改变的单 PR、默认关闭、Task pause 与 Task cancel 契约。
Header: 存量任务回滚
Options:
A) 显式 drain 后关闭
  ✅ `runtime_tasks_enabled=false` 只阻止新持久 Task 创建；既有 Task 不被隐式改写，回滚前先逐个取消非终态 Task 并等待确定终态或处理 `cancel_unknown`，最后才关闭 Workbench。
  ✅ 保留三个 flags 默认关闭和单 PR 既定方案；只需补 runbook、状态检查与 smoke，human ~0.5d / CC ~30min [estimated]，维护成本低。
  ❌ 它不是紧急 kill switch；跳过 drain 仍会让旧 Task 推进，操作员必须看到并执行前置检查。
B) 禁用即全部暂停
  ✅ 禁用时将非终态 Task 统一置为 rollout 来源的 paused，保留 job/watch 与事件，重新启用后按等待关系恢复，避免新增 Run。
  ✅ 与已批准 pause 的设备语义一致，不批量取消物理 job；human ~2d / CC ~2h [estimated]，需要持久批量状态迁移和恢复测试。
  ❌ 当前 Run 与设备 job 仍继续；还必须区分 rollout paused 与用户主动 paused，否则重新启用会误恢复用户原本暂停的任务。
C) 禁用即全部取消
  ✅ 禁用时走既有 Task cancel 闭环，abort 当前 Run 并停止所属 watch/outbox/job，提供最强的回滚隔离。
  ✅ 复用 cancelling/cancel_unknown，不另造设备停止结果；human ~3d / CC ~3h [estimated]，需要批量取消进度与失败处置。
  ❌ 配置开关会触发真实物理副作用，可能中断仍需完成的设备动作；unknown 会阻塞干净回滚，不能承诺立即安全。

State: approved
Actual answer: A) 显式 drain 后关闭（D1，本轮 AskUserQuestion）
Accepted scope: 仅 R1/A。`runtime_tasks_enabled=false` 只阻止新持久 Task 创建，不隐式改写既有 Task；回滚先显式取消非终态 Task，等待确定终态或处理 `cancel_unknown`，再关闭 Workbench。它不是紧急 kill switch。单 PR、flags 默认关闭、Task pause/cancel 契约保持不变；R2/R3 分别按各自记录处置。
History: 第一轮只批准 flags 默认关闭和 gated rollout，未裁定存量 Task；设计评审新增控制 API 后的第二轮工程复审发现。用户已选择 D1=A。

### R2: Task 审批的持久决策与崩溃恢复
Finding: [CRITICAL] (confidence: 9/10) 本文件 :403-496 — API 已批准 Task/approval/decision 幂等、冲突 409、过期/不可用不执行且防重复 Run，但 6.2/6.3 没有 approval 持久记录、冻结参数、decider、expiry、decision 与执行/job/wake 的恢复关联。Section 1，第二轮工程评审。
Plan baseline: 设计评审 D8/D21 等已批准内联审批和控制 API；6.4 的幂等与冲突语义是固定契约，但崩溃期间如何保证批准一次、执行可恢复尚未批准。
Runtime evidence: `pending_action.rs:11-23` 是进程内 `DashMap`，`take_pending_action_reasoned` 在 :74-80 先 remove 再消费；`workspace_heartbeat.rs:777-796` 虽有条件状态翻转，但翻转后直接执行，崩溃仍可能留下已批准未执行或结果未知。现有实现不能证明新 Task 审批的重启恢复与持久幂等。
Comparison grid:

| Choice | Current | A | B | C |
| --- | --- | --- | --- | --- |
| 审批身份与冻结参数 | API 有 ID，表/字段未定义 | 持久 Task/run/approval、展示与执行参数、decider、expires_at、decision | 持久同类必要身份与参数 | 未定义，pending |
| approve/reject 原子性 | 仅定义 HTTP 结果 | 条件 decision CAS；approve 同事务写唯一执行 intent/outbox，reject 不写执行 | 条件 decision CAS 后同步执行 | 未定义，pending |
| 崩溃恢复 | 未定义 | 按 approval 唯一执行关联恢复；设备结果未知走既有 job unknown，不盲重放 | 已批准未执行或结果未知转人工核查，不自动恢复执行 | 未定义，pending |
| 幂等/409/expiry/权限 | 已批准 | 不变 | 不变 | 不变，仍缺实现契约 |
| R1 drain | 已批准 A | 不变 | 不变 | 不变 |
| R3 单 Job Stop | 已批准 A | 不变 | 不变 | 不变 |

Question D2:
D2 — Task 审批如何保证崩溃后不丢、不重、不错发
Project/branch/task: e4-p2-declaration-closeout，通用 Runtime 的持久审批与设备副作用恢复。
ELI10: 用户点“批准一次”后，服务可能刚好重启。系统既要记住人究竟批准了什么，也要知道设备命令是否已进入执行流程，否则可能永远不执行，或恢复时重复执行。
Stakes if we pick wrong: 崩溃可能让审批卡死、让人看到的参数与实际动作不同，或对物理设备重复下发命令。
Recommendation: A) 持久决策＋执行信箱，因为它把已批准的幂等、参数一致性与重启恢复落到同一事务和唯一执行关联，而不是依赖进程内存或崩溃后的猜测。
Completeness: A=10/10, B=6/10, C=2/10
Pros / cons:
A) 持久决策＋执行信箱 (recommended)
  ✅ 新增持久 approval 记录，冻结展示/执行参数并记录 Task/run、decider、expiry 与 decision；approve 的 CAS 与唯一执行 intent/outbox 同事务，reject 不产生执行。
  ✅ 重启后按 approval 唯一关联恢复，设备结果未知沿用 job unknown 而不盲重放；同 decision 重放返回投影、冲突 409、过期/不可用不执行，human ~3d / CC ~4h [estimated]。
  ❌ 需要新表/port、迁移、执行关联与崩溃注入测试，维护成本高；不能只复用旧 heartbeat 翻转后直接调用的路径。
B) 持久决策＋同步执行
  ✅ 持久化审批身份、冻结参数和 decision CAS，可复用旧 heartbeat 的条件翻转与防盲签校验；同 decision 与冲突决定仍有明确持久依据。
  ✅ 不新增自动执行恢复器，代码与运维状态更少，human ~1.5d / CC ~2h [estimated]；正常进程内批准路径更直接。
  ❌ CAS 后崩溃可能留下已批准未执行或设备结果未知，只能转人工核查且不能自动续跑；不满足完整重启恢复，unknown 时仍不得盲目重试。
C) 保持现状待补
  ✅ 本轮不引入新表或执行恢复机制，也不假装旧 DashMap/heartbeat 已满足新契约；文档与核对成本约 human ~0.5d / CC ~30min [estimated]。
  ✅ 保留现有 API 产品语义和其他已批准决定，不会在未裁定实现前写入错误的持久化保证。
  ❌ 审批持久性、崩溃恢复与防重复执行仍未定义，本评审保持 unresolved，不能据此开始审批实现或宣称 Approval readiness 通过。
Net: 只选择审批决策到执行关联的恢复机制；不改变已批准的内联审批、权限、幂等、409、expiry、单 PR 或 R1 drain。
Header: 审批恢复语义
Options:
A) 持久决策＋执行信箱
  ✅ 新增持久 approval 记录，冻结展示/执行参数并记录 Task/run、decider、expiry 与 decision；approve 的 CAS 与唯一执行 intent/outbox 同事务，reject 不产生执行。
  ✅ 重启后按 approval 唯一关联恢复，设备结果未知沿用 job unknown 而不盲重放；同 decision 重放返回投影、冲突 409、过期/不可用不执行，human ~3d / CC ~4h [estimated]。
  ❌ 需要新表/port、迁移、执行关联与崩溃注入测试，维护成本高；不能只复用旧 heartbeat 翻转后直接调用的路径。
B) 持久决策＋同步执行
  ✅ 持久化审批身份、冻结参数和 decision CAS，可复用旧 heartbeat 的条件翻转与防盲签校验；同 decision 与冲突决定仍有明确持久依据。
  ✅ 不新增自动执行恢复器，代码与运维状态更少，human ~1.5d / CC ~2h [estimated]；正常进程内批准路径更直接。
  ❌ CAS 后崩溃可能留下已批准未执行或设备结果未知，只能转人工核查且不能自动续跑；不满足完整重启恢复，unknown 时仍不得盲目重试。
C) 保持现状待补
  ✅ 本轮不引入新表或执行恢复机制，也不假装旧 DashMap/heartbeat 已满足新契约；文档与核对成本约 human ~0.5d / CC ~30min [estimated]。
  ✅ 保留现有 API 产品语义和其他已批准决定，不会在未裁定实现前写入错误的持久化保证。
  ❌ 审批持久性、崩溃恢复与防重复执行仍未定义，本评审保持 unresolved，不能据此开始审批实现或宣称 Approval readiness 通过。

State: pending
Actual answer: C) 保持现状待补（D2，本轮 AskUserQuestion）
Accepted scope: none; 用户选择暂不补充审批恢复机制，A/B 均未批准。既有 API 产品语义与其他已批准决定保留，但审批持久性、崩溃恢复与防重复执行仍未定义，不得开始该审批实现；R1 drain 与 R3 job 级停止按各自批准记录执行。
History: 第一轮没有新增审批控制 API；设计评审批准产品语义后，第二轮工程评审发现持久化与崩溃恢复缺口。用户已选择 D2=C，remedy 保持 unresolved。

Follow-up resolution（2026-10-27，显式重开 R2）: 用户在开始开发前选择
“持久化并幂等续跑”，即本 finding 的 A。上述 State/Actual answer/Accepted scope 保留为
第二轮历史记录，由本段取代其当前门控状态；没有改写历史回答。

- 持久身份：`approval_id` 为服务器生成 UUID；请求在同一事务写入 pending approval、冻结
  `task_id/run_id/tool_call_id/device_id/tool/args_json/reason/expires_at`、预分配
  `decision_event_id`，并把 Task 迁为 `needs_approval`。客户端和后续模型输出都不能改写
  这些执行字段。
- 决策事务：approve/reject 先校验 workspace、Task、approval 和 Agent 管理权限，再以
  `WHERE state='pending'` CAS 写 decision/decider/time。approve 与 `approval_decided`
  outbox 同事务；该 outbox 是唯一执行 intent。reject 也唤醒原 Task 记录拒绝，但不创建
  DeviceJob。相同 decision 重放返回原投影，冲突 decision 409；expired/unavailable/terminal
  只返回或记录当前事实，不授权执行。pending 首次到期仍以原 `decision_event_id` 原子写
  `expired + outbox`，恢复扫描先处理到期记录；重复到期决策不再新增 wake。
- 执行接续：WakeRouter 读取 approved 决策后，ApprovalExecutor 只使用冻结参数和原
  `tool_call_id` 调 JobService；Job 插入与 Task `needs_approval → waiting_job` 同事务，
  事务内重查 Task/approval，cancel/pause/terminal 不能绕过。`device_jobs.tool_call_id`
  唯一约束是第二层防重，既有记录必须逐字段匹配冻结执行身份。设备 Job 终态仍经原有
  wake 回原 Task。批准不新跑模型来“猜测”原动作，也不由浏览器直接调用设备。
- 崩溃矩阵：pending 在重启后仍为 pending；approve 提交前回滚则仍可重新决定；approve
  提交后、outbox dispatch 前由 pending 扫描续跑；dispatch 后崩溃按 decision_event_id 与
  tool_call_id 去重；Job 已 accepted/running 则恢复原等待关系；Job 已有终态只补投影；
  `cancelled_unknown`、物理结果不明或不可恢复调用不得自动重放。
- 控制竞态：pause 只记录 decision/outbox，恢复前不派发新 Job；approve 与
  reject/expired/unavailable 的 Task advance 若返回 `Deferred`，都必须把原 outbox 留作
  pending，不得标为 dispatched；resume 重试同一 `decision_event_id`。cancel/terminal 使未 dispatch
  intent 失效并走既有 outbox/job 解决流程。approve 与 cancel 竞争时只能由 Task 状态 CAS
  决定一个顺序，cancel 先提交则不得执行。过期使用注入时钟测试，不新增固定全局 TTL。
- 验收：双击/并发同 decision 仅一个决策与 outbox；冲突 409；事务故障无半提交；冻结参数
  逐字节一致；在上述每个崩溃点恢复后最多一个 Job；reject/expired/unavailable 零 Job；
  跨 workspace/Task 404；非管理员 403；历史模式无审批操作。恢复不承诺物理 exactly-once，
  只承诺不盲重放和明确 unknown。

Current state: resolved / approved for implementation planning，仍须通过本文件 11.4 要求的
聚焦 `/plan-eng-review` 和实现测试；不代表功能已经实现或验收。

### R3: 单 Job Stop 的排队与在途隔离
Finding: [CRITICAL] (confidence: 10/10) 本文件 :529-532、:663-671 — 已批准只停止当前 Task 所属 accepted/running job、按 Task/job 幂等且不取消 Task，但现有 worker 命令与结果没有 job 身份，无法安全覆盖 queued/in-flight 及重启恢复。Section 1，第二轮工程评审。
Plan baseline: 设计 D10 与 6.4/7.5 已批准单 Job Stop 的产品边界；不得取消整个 Task、不得自动重放设备命令、不得影响同设备其他 Task，迟到终态与 `cancelled_unknown` 必须保留。
Runtime evidence: `worker.rs:54-64` 的 CancelSlot 只有当前 token；`WorkerCommand::Execute` 在 :129-138 没有 job_id，`WorkerResult::Cancelled` 在 :173-178 也没有；`data_server.rs:152` 按 thing 调 `cancel_current`，`arbitration.rs:161-168` 的 set_stop 是设备级 gate 并释放资源。直接复用两者可能误停另一 Task 或阻断该设备后续 claim。
Comparison grid:

| Choice | Current | A | B | C |
| --- | --- | --- | --- | --- |
| 排队取消 | job_id 未传入 worker | Execute 携带 job_id；领取时检查持久 stop_requested，目标 job 直接形成 cancelled 终态 | 新单设备 Job 调度器持有队列；queued 目标不再 submit 到 worker | 未定义，pending |
| 在途取消 | 仅 thing 当前 token | CancelSlot 绑定 job_id，仅匹配目标时 cancel；结果携带同一 job_id | 调度器只跟踪一个当前 job，仅映射匹配时 cancel_current | 未定义，pending |
| 重启与幂等 | API 幂等已批准，恢复未定义 | 持久 stop_requested；重启按 job_id 重断言，重复 stop 返回投影 | 同样持久 stop_requested；重启重建单设备当前映射 | 未定义，pending |
| 同设备/跨 Task 隔离与 unknown | 已批准 | 不变，不用设备 stop gate | 不变，不用设备 stop gate | 不变，仍缺实现契约 |
| Task 不取消、不自动重放 | 已批准 | 不变 | 不变 | 不变 |
| R1 drain / R2 审批 | R1 已批准；R2 pending | 均不变 | 均不变 | 均不变 |

Question D3:
D3 — 单 Job Stop 如何覆盖排队、在途与重启
Project/branch/task: e4-p2-declaration-closeout，通用 Runtime 的 job 级设备取消隔离。
ELI10: 一台设备可能同时替多个任务排队执行动作。停止其中一个动作时，系统必须准确认出它在队列里还是正在执行，不能误停同设备的另一个任务，也不能重启后忘记这次停止。
Stakes if we pick wrong: 用户可能停错设备动作、让另一 Task 意外失败，或在崩溃恢复后重复/漏掉停止并误报设备已安全。
Recommendation: A) job_id 贯穿 worker，因为它让排队跳过、在途取消、结果关联与重启重断言使用同一个身份，避免另建一层隐含的当前调用映射。
Completeness: A=10/10, B=9/10, C=2/10
Pros / cons:
A) job_id 贯穿 worker (recommended)
  ✅ Execute、CancelSlot 与 WorkerResult 全程携带 job_id；queued 领取时按持久 stop_requested 跳过，in-flight 只取消匹配 token，终态准确回到原 Task/job。
  ✅ 重复 stop 返回当前投影，重启按 job_id 重断言；同设备其他 Task 不受影响，保留 cancelled_unknown 且不自动重放，human ~3d / CC ~4h [estimated]。
  ❌ 要修改 worker 命令/结果、取消槽、DataServer 投影和迁移，并补竞态与重启测试；旧调用点都要显式关联或保留人工 job 的空身份语义。
B) 单设备 Job 调度器
  ✅ 在 worker 前增加每设备串行 Job 调度层，queued stop 不再 submit，in-flight 仅当前映射匹配时调用 cancel_current，也能保持跨 Task 隔离。
  ✅ 可少改 WorkerCommand/WorkerResult 的公开形状，并集中处理持久 stop_requested 与重启映射，human ~5d / CC ~6h [estimated]。
  ❌ 新增调度状态与当前映射竞态，重启和结果迟到时更容易错配；仍然需要持久身份、终态去重、unknown 与完整隔离测试，维护成本更高。
C) 保持现状待补
  ✅ 不误用设备级 stop gate 或 thing 当前调用来冒充单 Job Stop，避免在方案未定时引入误停其他 Task 的实现。
  ✅ 保留已批准的 UI/API 产品语义和其他决定，文档核对成本约 human ~0.5d / CC ~30min [estimated]。
  ❌ queued/in-flight、重启和结果关联仍未定义，本评审保持 unresolved，不能开始 Job Stop 实现或宣称整体 readiness 通过。
Net: 只选择单 Job Stop 的工程隔离机制；不改变 Task 不取消、不自动重放、unknown 语义、R1 drain 或仍 pending 的 R2。
Header: Job 停止隔离
Options:
A) job_id 贯穿 worker
  ✅ Execute、CancelSlot 与 WorkerResult 全程携带 job_id；queued 领取时按持久 stop_requested 跳过，in-flight 只取消匹配 token，终态准确回到原 Task/job。
  ✅ 重复 stop 返回当前投影，重启按 job_id 重断言；同设备其他 Task 不受影响，保留 cancelled_unknown 且不自动重放，human ~3d / CC ~4h [estimated]。
  ❌ 要修改 worker 命令/结果、取消槽、DataServer 投影和迁移，并补竞态与重启测试；旧调用点都要显式关联或保留人工 job 的空身份语义。
B) 单设备 Job 调度器
  ✅ 在 worker 前增加每设备串行 Job 调度层，queued stop 不再 submit，in-flight 仅当前映射匹配时调用 cancel_current，也能保持跨 Task 隔离。
  ✅ 可少改 WorkerCommand/WorkerResult 的公开形状，并集中处理持久 stop_requested 与重启映射，human ~5d / CC ~6h [estimated]。
  ❌ 新增调度状态与当前映射竞态，重启和结果迟到时更容易错配；仍然需要持久身份、终态去重、unknown 与完整隔离测试，维护成本更高。
C) 保持现状待补
  ✅ 不误用设备级 stop gate 或 thing 当前调用来冒充单 Job Stop，避免在方案未定时引入误停其他 Task 的实现。
  ✅ 保留已批准的 UI/API 产品语义和其他决定，文档核对成本约 human ~0.5d / CC ~30min [estimated]。
  ❌ queued/in-flight、重启和结果关联仍未定义，本评审保持 unresolved，不能开始 Job Stop 实现或宣称整体 readiness 通过。

State: approved
Actual answer: A) job_id 贯穿 worker（D3，本轮 AskUserQuestion）
Accepted scope: 仅 R3/A。Execute、CancelSlot、WorkerResult 使用同一 job_id；持久 stop_requested 覆盖 queued 跳过、in-flight 匹配取消、重启重断言与重复 stop 投影。保留同设备/跨 Task 隔离、cancelled_unknown、不取消 Task、不自动重放，不使用设备级 stop gate。R1 drain 不变，R2 仍 pending。
History: 第一轮批准设备生命周期与 stop 边界；设计评审新增单 Job Stop 操作后，第二轮工程评审发现 job 身份与隔离缺口。用户已选择 D3=A。

## Approval readiness

Approval readiness: BLOCKED — R2/D2=C 保留审批持久 decision、执行关联与崩溃恢复未决，
不能开始完整 Runtime PR。R1/D1=A 与 R3/D3=A 已批准；R4 cancel 接线与 R5 控制测试是
D6-task-cancel、既有 pause 契约及 R1/R3 的必要实现与证明，没有另开范围。

第一轮 D0-path、D0-task、D0-media、D0-ui、D0-recovery、D0-generic、D0-mode、
D0-pr、D1-media-adapter、D2-job-progress、D3-job-pause、D5-task-create、
D6-task-cancel、D7-video-proxy、D8-resource-budget、D10-context-budget 的实际回答保持有效。
本轮没有把 R2/A 或 R2/B 混入 accepted scope，也已从 T6/T7B/T8 移除可执行审批任务。
设计评审的字体与 captions 两项 deferred/TODO 继续单独登记。

## Review facts

- Mode：HOLD SCOPE；单个完整 PR，三切分作为提交/启用顺序。

- 当前结论：第二轮工程复审为 ISSUES OPEN；R1 drain 与 R3 job 级停止已写回，R4/R5
  接线与测试已补齐，但 R2/D2=C 使审批持久恢复和完整 PR 保持 blocked。未实现、未验收；
  D8 容量风险与 D10 无 Task prompt 总硬顶的上下文/成本风险明确保留。

- Outside Voice：本轮 Codex 配置失败 + native fallback 超时，unavailable，无 clean 信用；
  两次 unavailable 均已记录，未采用部分输出。

- 工程评审未决项：1（R2）。GoalWatch/E1d/OpState 等独立后续项不是未决项；设计评审的
  2 个 deferred 缺口在下方单独登记，不回写为工程决策。

## NOT in scope

| 工作 | 处置 | 实际依据 |
| --- | --- | --- |
| GoalWatch / E1d 知觉权 | deferred | 独立后续项，当前不启动；上位文档已回写 |
| OpState mid-turn 恢复 | deferred to E5 | 首版只承诺 Task 级恢复，D0-recovery |
| Hub 内建 GPU 感知 / 默认录像 | deferred | 首版不做，避免扩大感知与存储边界 |
| 场景专用 Agent/prompt/卡片 | rejected | 用户要求通用 Agent，禁止场景分支 |
| 真实百分比进度/done/total | rejected | D3 用户选择仅 accepted/running，不新增事实通道 |
| Device Job pause/resume | rejected | D4 用户选择 stop/cancel，Task paused 仍保留 |
| provider 专用图片旁路 | rejected | D2 用户选择反腐败层统一附件契约 |
| Hub 视频代理/清单重写 | rejected | D5 用户选择浏览器直连，失败不兜底代理 |
| 新增固定 Task/媒体配额 | rejected | D8 用户选择无固定配额，仅保留背压与告警 |
| Task prompt 总字节硬顶/隐藏截断 | rejected | D10 用户选择完整拼接；保留现有设备快照边界并观测上下文与成本 |
| Runtime 专属字体资产 | deferred | 设计 D13/D24 选择本期保留系统中英文字体栈与 JetBrains Mono；默认字体的通用感进入 TODOS，不在本期伪装成已解决 |
| 视频 captions 轨与自动转录 | deferred | 设计 D22/D25 选择首版保留原声并显示 `Captions unavailable`；含语音证据的 WebVTT 可达性进入 TODOS，不宣称已有字幕或转录 |
| Task 审批持久 decision/执行恢复与审批实现 | blocked / unresolved | 第二轮 R2/D2=C 选择保持待补；旧 DashMap/heartbeat 不满足恢复契约，解决前不得实现审批路由、执行恢复或审批按钮 |
| MHS/MOS wire 兼容 | rejected | 只借信息结构，不宣称协议兼容 |

## What already exists

- `runner.rs` / `manager.rs`：Run Loop、预算、scheduler、RunReport、wake/steer。

- 固定设备工具与 LiveState/WatchEngine：感知、watch 命中和重织。

- `DataServer`/worker：声明校验、资源仲裁、取消、completion signaling。现有
  `cancel_calls(thing_id)` 与设备级 `set_stop` 不能复用为单 Job Stop；R3 要求补 job_id
  队列/在途/结果关联与持久 stop_requested。

- `Runner`/rig：rig 已消费 CancellationToken，可复用同一 token 完成 Task cancel；
  但 Runner 当前内部私有 token、ActiveRunRegistry 只有 RunContext，必须按 R4 补外部绑定。

- heartbeat proposal：已有 workspace/admin、防盲签与条件状态翻转先例，可复用校验思路；
  `PendingActionStore` 是进程内 DashMap，翻转后直接执行，不是新 Task 审批的持久恢复方案，
  R2 未批准前不得拿来冒充实现。

- `hub_ledger`/EventBus/hub_stream：有序控制与 latest-wins 遥测、现有 Flow。

- `agent_tasks.rs`：现有创建入口，可原位补持久 Task 与强制 Idempotency-Key。

- `provider.rs`/rig 适配器：可扩展为统一附件，不重写 provider。

- Lit 前端、`web/src/api` 与 SSE 基建：可组成 Task Runtime 列表、创建页和详情视图，
  组件不直接 fetch。

- `DESIGN.md`、`web/DESIGN_SYSTEM.md`、`web/src/styles/base.css`：自治机舱方向、
  完整 dark/light token、无边框层级、spacing/radius/shadow/duration/ease、focus ring、
  selection/scrollbar/tabular-nums 和系统字体栈均已存在；Runtime 不另建全局色板。

- `web/src/ui/shared/run-timeline.ts`：复用步骤与折叠结果模式，扩展 waiting、approval、
  cancelled_unknown 等真实投影，不新造聊天式时间线。

- `web/src/ui/views/live-devices.ts`：复用健康、关键字段、watch/job 徽标与键盘卡片模式；
  Task 关联集合仍由本 spec 的实际引用规则决定。

- `web/src/ui/views/flow.ts`：已有 live/history、键盘时间控制、代际守卫、CONTROL/DATA
  与列表回退，可承载 Task 投影和共享游标。

- `web/src/ui/views/workbench-state.ts` 与 `run-detail.ts`：复用纯函数 URL 状态与异步
  代际守卫；不复制 device 选中即历史或 3s polling。

- 既有设计系统 modal/focus 规范：提供 dialog、菜单、focus trap/restoration 和可见焦点
  的复用边界；创建表单继续使用可见 label 与原生输入语义。

## Dream state delta

```text
当前：声明、Watch、Runner、账本各自可用，但多设备任务与证据仍断裂
  → 本设计批准后：形成 Task 持续推进、job/watch 唤醒、关联证据与两栏投影的明确契约
  → 实施并验收后：任意声明式设备可进入同一通用协同闭环
  → 12 个月差距：OpState、知觉权、更多真实驱动与恢复演练、真实容量基线
```

## Error & Rescue Registry

- 能力: Task 创建
- 失败机制: 缺 key/异 key/DB 失败/重复请求
- 用户影响: 400/409/500 或原 Task，不产生重复执行
- 已有/批准护栏: 强制 key、unique、Task+outbox 事务
- 验证责任: handler+并发测试

- 能力: Task 状态迁移
- 失败机制: 旧 Run/CAS 冲突
- 用户影响: 不覆盖新状态
- 已有/批准护栏: expected_version 与重读
- 验证责任: 状态机+冲突测试

- 能力: Wake 投递
- 失败机制: pending/dead/重复/崩溃
- 用户影响: 可恢复唤醒，不重复 Run
- 已有/批准护栏: outbox+retry+DLQ+wake_event_id
- 验证责任: 崩溃与重复投递测试

- 能力: Device Job
- 失败机制: busy/timeout/stop/崩溃/重复终态
- 用户影响: 拒绝或明确终态，不盲重放
- 已有/批准护栏: 真实 deadline、job_id queued/in-flight stop、持久 stop_requested、终态去重、恢复
- 验证责任: worker+recovery+同设备跨 Task 隔离测试

- 能力: Task 取消
- 失败机制: 部分停止/未知
- 用户影响: failed(cancel_unknown)，不假称全停
- 已有/批准护栏: cancelling、Runner/rig 同一 cancel token、停止所属 job/watch、迟到去重
- 验证责任: 两向取消矩阵与取消后无新工具测试

- 能力: Task 审批
- 失败机制: decision 提交前后崩溃、执行结果未知、重放
- 用户影响: 若按旧内存/同步路径实现，可能卡死、丢失批准或重复副作用
- 已有/批准护栏: 只有 HTTP 产品语义；R2 持久恢复方案未批准，当前 blocked
- 验证责任: R2 unresolved；没有可执行崩溃恢复测试或实现任务

- 能力: Runtime 回滚
- 失败机制: 先关 UI/跳过 drain/cancel_unknown
- 用户影响: 旧 Task 可能继续推进但失去操作入口，或物理结果未核查
- 已有/批准护栏: R1 显式逐个取消、非终态/活跃项清零与 unknown 处置后才关 Workbench
- 验证责任: flag transition + rollback runbook 演练

- 能力: Watch
- 失败机制: 非法/无数据/失效/Lagged
- 用户影响: 拒绝/等待/失效/恢复
- 已有/批准护栏: schema、unknown 三态、重织
- 验证责任: 单测+重启测试

- 能力: 模型
- 失败机制: 超时/429/空/拒答/非法输出
- 用户影响: Run failed 或拒绝，不伪造完成
- 已有/批准护栏: 分类错误、预算、重试限制
- 验证责任: provider 夹具

- 能力: 图片
- 失败机制: 非 vision/超限/过期/重复
- 用户影响: 明确不可用，不静默丢图
- 已有/批准护栏: 统一附件、大小/像素/dedup_key
- 验证责任: port/rig/store 测试

- 能力: 视频
- 失败机制: CORS/签名/断流/不支持
- 用户影响: `playback_unavailable`，不冒充实时
- 已有/批准护栏: 短时签名 URL、destroy、无代理兜底
- 验证责任: 浏览器+本地上游测试

- 能力: MediaStore
- 失败机制: 写失败/孤儿/磁盘满
- 用户影响: 数值控制继续，媒体明确失败
- 已有/批准护栏: 流式、清理、过期与容量告警
- 验证责任: 故障注入

- 能力: SSE/UI
- 失败机制: 断连/重复/stale
- 用户影响: 显式重连与旧数据标识
- 已有/批准护栏: cursor+resync+event_id 去重
- 验证责任: 组件与浏览器测试

## Failure Modes Registry

```text
CODEPATH                  | FAILURE MODE       | RESCUED? | TEST?     | USER
SEES? | LOGGED?
  --------------------------|--------------------|----------|-----------|------------|--------
Task create               | 重复/异 key        | Y unique  | required  | 原
Task/409| Y
Task transition+outbox    | 半提交/重复唤醒     | Y tx+dedup| required  | 明确错误   | Y
Wake dispatch             | 崩溃/重复/死信      | Y retry   | required  | 可恢复/告警| Y
Job result                | DB 失败/进程崩溃    | Y retry   | required  | 等待/恢复  | Y
Task cancel               | 部分停止未知        | Y failed  | required  | 不明确终态 | Y
Job stop                  | 误停他 Task/重启丢失| Y job_id  | required  | 当前投影/未知| Y
Approval decision/execute | 崩溃/重放           | N pending | blocked   | 未实现     | N
Rollout drain             | 先关 UI/unknown     | Y gate    | required  | 阻止并告警 | Y
Watch                     | 无数据/失效/Lagged  | Y rebuild | required  | 等待/失效  | Y
Model response            | 空/拒答/非法 JSON   | Y typed   | required  | 失败原因   | Y
Image attachment          | 非 vision/超限/过期 | Y explicit| required  | 明确不可用 | Y
Playback                  | CORS/签名/断流      | Y explicit| required  | 明确不可用 | Y
MediaStore                | 孤儿文件/重复证据   | Y cleanup | required  | 可重试     | Y
SSE                       | 重复/断连/stale     | Y resync  | required  | 重连/旧标识| Y
StopAll                   | 部分失败            | Y partial | required  | 逐设备结果 | Y
```

CRITICAL GAPS: 1（R2 审批持久 decision/执行恢复未批准且无可执行测试；D2=C 保持 blocked。
不得按旧 DashMap/heartbeat 路径实现。R1/R3/R4/R5 已有获批护栏或必要测试，实现验证尚未发生）。

## Diagrams

本文件包含：总体架构、四条主数据流及影子路径、Task/Job 状态机、错误与事务流、
部署启用顺序、回滚流程、测试覆盖图、用户旅程。所有图均标出边界、失败机制与恢复方式。

## Stale Diagram Audit

- 上位 `2026-10-20-hub-runtime-architecture-design.md` 已补 2026-10-26 后续方向，
不再按旧 E1-E5 顺序暗示当前实施。

- 本文件中的 `paused` 已区分为 Task 状态，Device Job 不含 paused。

- 所有百分比进度图已移除，视频图已改为浏览器直连，无 Hub 代理图残留。

## Scope Challenge Result

- Feature answers: 当前 spec 全量接受（D0-path/task/media/ui/recovery/generic/mode/pr、D1-media-adapter、D2-job-progress、D3-job-pause、D5-task-create、D6-task-cancel、D7-video-proxy、D8-resource-budget、D10-context-budget）。
- Structure: **Original arrangement**（用户 D2，2026-10-26）。按 Task/Job/Media/Projection 分离约 35 文件、6 新服务 [estimated]，不为减少文件合并事务、设备副作用、媒体凭证与 UI 生命周期。
- Accepted scope: 与当前 spec 一致，不减少能力。
- Pending remedies: none at structure gate.

## Worktree parallelization strategy

| Step | Modules touched | Depends on |
|------|-----------------|------------|
| A: T1→T2→T3 | agent task/ports/prompt、runtime job/worker、db、cloud agent/admin | —；T1 先稳定关联 ID 与 Task 契约 |
| B: T4→T5 | core manifest、runtime media/job、agent provider/rig、cloud media、web playback/api | T1 的关联 ID；与 A 协调 runtime/job 和 db migration |
| C: T6→T6A→T7A→T7→T7B→T7C | cloud agent/admin/router、web api/views | A 的 wake/API 与 B 的媒体/playback 契约 |
| D: T8 | acceptance/evals、web、docs、rollout | A+B+C 全部合并 |

**Parallel lanes：** Lane A 先落地 T1；随后 Lane B 可与 A 的 T2/T3 并行，但共享
`runtime/`、`db/` 的改动必须按 migration 与 job 契约顺序协调。A+B 合并后启动 Lane C；
`cloud/` 与 `web/` 不在 C 内再拆分并行。Lane D 最后串行执行全量验收。

**Execution order：** Launch A；T1 合并后 launch B；A+B 合并后 launch C，依次执行
T6→T6A→T7A→T7→T7B→T7C；C 合并后运行 D。所有 lane 仍进入同一个完整 PR，不改变 D9。

**Conflict flags：** A/B 共享 runtime job、db migrations 与关联 ID；B/C 共享 cloud media/API
和 web 播放契约；不得让并行 lane 各自发明事件、证据或路由格式。

## Implementation Tasks

Synthesized from the engineering and design reviews. All tasks belong to one complete PR;
commit/enable order is T1→T2→T3→T4→T5→T6→T6A→T7A→T7→T7B→T7C→T8. Each task derives from
an approved finding above. T7A/T7B/T7C extend the original T7 Workbench scope without
splitting the PR; captions and a dedicated font are not accepted implementation work.
The 2026-10-27 R2 follow-up approves durable approvals, frozen arguments, decision CAS,
a unique decision/execution outbox and idempotent crash continuation. T6A/T7B/T8 include
that mechanism and its tests; the old in-memory/heartbeat direct-execution path is not an
implementation substitute.

- [ ] **T1 (P1, human: ~4d / CC: ~4h)** — Task model — 持久 Task、强制
Idempotency-Key 与迁移
- Surfaced by: Section 1/4 — 多次 Run 缺少 Task 主线，现有 POST 每次生成新 UUID。
- Files: `crates/agent/src/runtime/thing_agent/task/`,
`crates/agent/src/port/task.rs`,
`crates/db/migrations/`, `crates/db/src/agent_runs.rs`,
`apps/cloud/src/domains/agent/host/handler/agent_tasks.rs`,
`apps/cloud/src/domains/agent/runtime/`
- Verify: `cargo test -p tinyiothub-cloud task_create`（同 key 并发/重放/异
objective/缺 key）

- [ ] **T2 (P1, human: ~5d / CC: ~5h)** — Device Job — 生命周期、真实 timeout 与 job 级 stop
- Surfaced by: Section 1/2/4 + 第二轮 R3/D3 — 入队即 executed、timeout 未消费、stop 不 drain queued
Execute，且 worker 缺 job_id 会误停同设备其他 Task。
- Files: `crates/runtime/src/job/`, `crates/runtime/src/data_server.rs`,
`crates/runtime/src/driver/worker.rs`, `crates/runtime/src/ports.rs`,
`crates/runtime/src/access/service.rs`, `crates/db/migrations/`, `crates/db/src/`,
`apps/cloud/src/domains/agent/host/tools/thing/invoke_action.rs`,
`apps/cloud/src/domains/thing/handler/actions.rs`
- Verify: `cargo test -p runtime job` + `cargo test -p tinyiothub-cloud stop`
（长 job、deadline、queued/in-flight job_id stop、重复 stop、同设备跨 Task 隔离、
stop 后重启、重复终态、cancelled_unknown、无 paused/百分比、不取消 Task）

- [ ] **T3 (P1, human: ~4d / CC: ~4h)** — Wake — outbox、Task-owned watch、取消与恢复
- Surfaced by: Section 2/4/9 — 唤醒去重、取消所属 job/watch、重启恢复与 outbox 重试。
- Files: `crates/agent/src/runtime/thing_agent/task/`,
`crates/agent/src/runtime/thing_agent/runner.rs`,
`crates/agent/src/runtime/thing_agent/active_runs.rs`,
`crates/agent/src/runtime/thing_agent/manager.rs`,
`crates/agent/src/adapters/rig/loop_.rs`,
`crates/agent/src/port/task.rs`, `crates/agent/src/prompt/`,
`crates/db/src/`, `apps/cloud/src/domains/agent/runtime/`,
`apps/cloud/src/domains/agent/host/handler/`,
`apps/cloud/src/domains/admin/`
- Verify: `cargo test -p tinyiothub-cloud task_recovery` + `cargo test -p agent task_prompt`
（重复投递、accepted 后崩溃、取消 unknown、dead retry、watch 重织、完整拼接与确定顺序；
Runner/ActiveRunRegistry/rig 使用同一 cancel token，取消后无新工具，pause 不 abort；
pause/resume 与 wake 竞态不重复 Run）

- [ ] **T4 (P1, human: ~6d / CC: ~6h)** — Source/Media — dataKind、证据与统一附件
- Surfaced by: D1-media-adapter + Section 2/3/5 — rig 静默丢图、证据关联与去重。
- Files: `crates/core/src/driver/manifest.rs`, `crates/runtime/src/media/`,
`crates/agent/src/port/provider.rs`, `crates/agent/src/port/tool.rs`,
`crates/agent/src/port/events.rs`, `crates/agent/src/adapters/rig/`,
`apps/cloud/src/domains/media/`,
`apps/cloud/src/domains/agent/host/tools/thing/device.rs`,
`crates/db/src/`
- Verify: `cargo test -p agent rig` + `cargo test -p runtime media`
（图片 roundtrip、非 vision、大小/像素/dedup/过期、unknown dataKind）

- [ ] **T5 (P1, human: ~3d / CC: ~3h)** — Playback — 浏览器直连与签名播放描述
- Surfaced by: D7-video-proxy + Section 3/7/11 — 无 Hub 代理、CORS/凭证/播放器生命周期。
- Files: `apps/cloud/src/domains/media/`, `web/src/api/`, `web/src/ui/views/`,
`web/package.json`, `web/pnpm-lock.yaml`
- Verify: `npm --prefix web test -- playback` + 浏览器本地上游验证
（脱敏 URL、CORS 失败、切 Task destroy、无后台请求、无代理调用；首版显示
`Captions unavailable`，不虚构 captions 或自动转录）

- [ ] **T6 (P1, human: ~5d / CC: ~5h)** — Projection — Task 控制 API、runtime stream 与排障入口
- Surfaced by: Section 1/4/8 + Design Issues 6/21 — 关联投影、游标重连、dead/unknown
运维处置，以及已批准的单 job Stop 和一键 Pause/Resume 缺少传输契约。审批持久化与执行恢复在 T6A。
- Files: `apps/cloud/src/domains/agent/runtime/`,
`apps/cloud/src/domains/agent/host/handler/`, `apps/cloud/src/domains/admin/`,
`apps/cloud/src/router.rs`, `web/src/api/`, `web/src/ui/views/`
- Verify: `cargo test -p tinyiothub-cloud runtime_stream` + Task 控制 handler 测试
（snapshot/resync/去重/跨区/outbox retry/unknown 查询；pause/resume 幂等且不 abort
当前 Run 或停止 job/watch；Job Stop 校验 Task 归属、重复返回投影、不取消 Task，
同设备另一 Task 不受影响）

- [ ] **T6A (P1, human: ~3d / CC: ~4h)** — Durable approvals — 冻结参数、决策信箱与幂等恢复
- Surfaced by: 第二轮 R2 + 2026-10-27 follow-up A — 内存审批不能满足决策持久性、
  执行关联与崩溃恢复。
- Files: `crates/core/src/task_approval.rs`, `crates/agent/src/port/approval.rs`,
  `crates/agent/src/runtime/thing_agent/task/approval.rs`, `crates/db/src/task_approval.rs`,
  `crates/db/migrations/20261027000004_task_approvals.sql`,
  `apps/cloud/src/domains/agent/runtime/approvals.rs`
- Verify: `cargo test -p tinyiothub-storage task_approval` +
  `cargo test -p tinyiothub-agent thing_agent::task::approval` +
  `cargo test -p tinyiothub-cloud domains::agent::runtime::approvals`
  （decision CAS/409、冻结参数、approve 同事务 outbox、reject 零 Job、各崩溃点最多一个
  Job、pending 恢复、未知物理结果不盲重放、pause/cancel 竞态与权限/跨区隔离）

- [ ] **T7A (P1, human: ~2d / CC: ~2h)** — Runtime shell — Task 列表、专用创建页与切换
- Surfaced by: Design Issues 1/19 — `#/runtime` 不能是 KPI 仪表盘；需要 Current/Recent
列表、`#/runtime/new` 可见 label 表单、创建幂等重试和安全的详情切换。
- Files: `web/src/ui/views/runtime-list.ts`, `web/src/ui/views/runtime-new.ts`,
`web/src/ui/views/runtime-task.ts`, `web/src/ui/app.ts`, `web/src/api/`
- Verify: `npm --prefix web test -- runtime`（分组/空态/分页错误、objective 保留、
pending 防重复、同输入重试保留 key、400/409 不静默重试、accepted replace 导航、
Switch task 清理旧 SSE/媒体/游标）

- [ ] **T7 (P1, human: ~5d / CC: ~5h)** — Workbench — 两栏 ToolCard、关联设备卡与
seam/Flow
- Surfaced by: D0-ui + Design Issues 2/3/13 — objective/状态/干预优先，Agent Loop 主导、
图片/视频入卡、详情内 Flow 标签、关联定位与既有 token/component 复用。
- Files: `web/src/ui/views/runtime-task.ts`,
`web/src/ui/views/runtime-task.css`,
`web/src/ui/views/runtime-devices.ts`,
`web/src/ui/views/runtime-devices.css`,
`web/src/ui/views/runtime-flow.ts`, `web/src/ui/shared/run-timeline.ts`,
`web/src/ui/app.ts`, `web/src/api/`
- Verify: `npm --prefix web test -- runtime` + 浏览器键盘/seam/关联/Flow URL 检查
（无 Goal 页、无独立 World 栏、无场景专用组件、无固定浅色卡片或新增全局色板）

- [ ] **T7B (P1, human: ~4d / CC: ~4h)** — Interaction states — 可见五态、历史模式与干预闭环
- Surfaced by: Design Issues 4/5/6/7/21 — loading/empty/error/success/partial 矩阵、
History read-only、Task cancel、Job Stop 和 Pause/Resume 必须以服务器投影为准。
R2 已按 follow-up A 解除：T7B 包含内联 Approve once/Reject、pending 防双击、冲突/过期/不可用投影和 Task 切换清理；执行恢复仍归 T6A。
- Files: `web/src/ui/views/runtime-task.ts`, `web/src/ui/views/runtime-devices.ts`,
`web/src/ui/views/runtime-flow.ts`, `web/src/ui/shared/run-timeline.ts`, `web/src/api/`
- Verify: `npm --prefix web test -- runtime` + 浏览器交互检查（stale/局部错误保留旧数据；
历史移除所有推进操作；暂停不暗示副作用停止；取消/停止 pending 与 unknown 不假成功；
Back to live 执行 resync）

- [ ] **T7C (P1, human: ~3d / CC: ~3h)** — Responsive & accessibility — 三档布局、键盘、读屏与双主题
- Surfaced by: Design Issues 12/14/15/16/17/18/20 — 双主题无边框、三档响应式、焦点与滚动、
动态播报、全面 reduced-motion、WCAG AA；首版 captions 缺口必须可见而非隐藏。
- Files: `web/src/ui/views/runtime-*.ts`, `web/src/ui/views/runtime-*.css`,
`web/src/ui/shared/run-timeline.ts`, `web/src/styles/`
- Verify: `npm --prefix web test -- runtime` + axe/键盘/读屏与浏览器人工验收
（600/980px 断点、<400px 列表、200%/400% zoom、44px 目标、tablist/menu/dialog/separator、
焦点归还、单一 polite region、Flow 等价列表、reduced-motion 停非必要动画、dark/light AA；
视频可键盘暂停且无自动声音，显示 `Captions unavailable`）

- [ ] **T8 (P1, human: ~5d / CC: ~5h)** — Acceptance — 第二夹具、eval、指标与 rollout
- Surfaced by: Section 6/7/8/9/10 — 通用性、恢复矩阵、容量风险、灰度与回滚。
- Files: `crates/agent/src/runtime/thing_agent/task/`,
`crates/runtime/src/job/`,
`apps/cloud/src/domains/agent/runtime/`, `apps/cloud/src/tests/`, `evals/`,
`web/src/`,
`docs/superpowers/plans/`
- Verify: `cargo fmt --all -- --check` + `cargo clippy --workspace
--all-targets -- -D warnings`
- `cargo test --workspace --lib --bins --tests` + `npm --prefix web run
type-check`
- `npm --prefix web test` + `npm --prefix web run build`
- `MINIMAX_API_KEY=... cargo test -p tinyiothub-cloud runtime_eval --
--ignored --nocapture`
- staging 启用/回滚演练；覆盖 flag off 不改写存量 Task、显式 drain、unknown 阻断关 UI、
  同设备跨 Task stop、pause/resume+wake 与 stop/restart；vision 不可用不得记 PASS。
- 实现后对真实 Runtime 页面运行 `/design-review`，覆盖 dark/light、三档布局、键盘/读屏、
  reduced-motion、历史只读和已批准干预状态；审批双击、拒绝、过期、不可用、切换清理与
  恢复后继续均计入验收，没有 approved mockup 不能替代视觉 QA。

## Reality rows

```text
REALITY: premise-table pass（3e4847223c468d5358718455c7d3d5091cb21416）unverified=0：
  Runner 内部创建 cancel token（`runner.rs:180-220`），ActiveRunRegistry 仅保存 RunContext；
  worker Execute/Cancelled 无 job_id（`worker.rs:129-138,173-178`），旧审批为进程内
  DashMap（`pending_action.rs:11-23`）；均为当前代码证据，不把规划行为写成已实现。
REALITY: already-done pass（3e4847223c468d5358718455c7d3d5091cb21416）：
  `origin/main` 已有 E1-E4 底座但无 Task Runtime； 证据 commits `061f20bc`（E2
  接缝）、`92052090`（Watch 装配）、`9bb05f8b`（WatchEngine）； E4 P2 仍在 PR #101。
REALITY: surface-check pass（3e4847223c468d5358718455c7d3d5091cb21416）：
  现有创建端点仅 `agent_tasks.rs:56`；新路由/flags/runtime_eval 均为 planned，未宣称存在。既有 eval
  命令见 `evals/README.md:8-10`。
REALITY: binding-decisions pass（3e4847223c468d5358718455c7d3d5091cb21416）：
  第二轮 R1/D1=A、R2/D2=C、R3/D3=A 均有独立 ledger 与实际回答；R2 保持 pending，
  未重开 GoalWatch、固定配额、prompt 硬顶或单 PR 等既有决定（本文件 Decision ledger）。
REALITY: numbers pass（3e4847223c468d5358718455c7d3d5091cb21416）：
  `ACTIVE_RUNS_CAPACITY=64` 实测（`manager.rs:56-60`）；`HOURLY_WAKE_LIMIT=20`
  实测（`scheduler.rs:54`，不是 watch 数上限）；快照 6144 bytes 实测（`snapshot.rs:11`）。
  其他文件数、耗时与性能目标均标 `[estimated]` 或 re-measure。
REALITY: deferred-asks pass（3e4847223c468d5358718455c7d3d5091cb21416）：
  无关联 issue 的未答请求；PR #101 合入依赖显式记录。R2/D2=C 是已回答但明确 pending
  的 remedy，未静默转成已实现或 TODO；两个字体的/captions 设计 deferred 仍有 TODOS 记录。
REALITY: pr-count pass（3e4847223c468d5358718455c7d3d5091cb21416）：
  D9 用户选择单个完整 PR（本文件 :2687-2719），无跨 PR 队列或引用成本。
REALITY: prior-art pass（3e4847223c468d5358718455c7d3d5091cb21416）：
  采用 AGH ToolCard/Flow/dock 结构； hls.js v1.7.3 + #8119 要求
  destroy（`docs/superpowers/specs/2026-10-26-multi-device-agent-runtime-design.md:1043-1047`）；无
  vendored/patched 依赖。
REALITY: transaction-semantics finding（3e4847223c468d5358718455c7d3d5091cb21416）：
  Task+outbox 同事务、job 终态去重与 stop_requested 已明确；R2 审批 decision/执行关联
  没有获批事务与恢复语义（`workspace_heartbeat.rs:777-796` 仍有旧 CAS 后直执窗口），保持 blocked。
REALITY: contract-assumptions pass（3e4847223c468d5358718455c7d3d5091cb21416）：
  50s 等待窗口与 AGH `hub/call` 一致； 当前 `negotiate_manifest_version` 只限制 major，故 1.1
  minor 可接受，未知字段忽略；不宣称 MHS/MOS wire 兼容。
REALITY: units pass（3e4847223c468d5358718455c7d3d5091cb21416）：
  6144 bytes 快照（`snapshot.rs:11`）；2 MiB/16MP/8 MiB 是本设计新限制（本文件
  :346、:1584、:2036-2037），50s 等待窗口是设计值（:226）；Run 5min 是现有限制
  （`runner.rs:32`）。性能目标 re-measure。
REALITY: perf-table pass（3e4847223c468d5358718455c7d3d5091cb21416）：
  未声称已有 benchmark；`<2s`/`<1s` 是计划门槛，当前无测量（本文件 :2172-2174、
  :2222），部署前必须按实际机器/数据量复测。
REALITY: dependency-fallback pass（3e4847223c468d5358718455c7d3d5091cb21416）：
  三个 flags 默认关闭分别回退旧指令、数值读取和旧页面；Task flag off 不改写存量，
  Workbench 关闭前显式 drain（本文件 :2588-2634）；vision provider/key 不可用只记
  unavailable（本文件 :1368）。
REALITY: obligation-tiers pass（3e4847223c468d5358718455c7d3d5091cb21416）：
  必须交付当前批准范围；GoalWatch/OpState 为 deferred，百分比/Device Job pause/provider
  bypass/Hub proxy/固定配额按 D3/D4/D5/D8 排除；Task pause/resume 已批准。R2 审批持久化、
  执行恢复、路由和 UI 控件明确 blocked，不进入当前必交付任务。

PLAN_REALITY: phase=eng rows=7/7 missing=none unverified=0： verdict=complete

```

## Completion Summary

- Step 0: Scope Challenge — scope accepted as-is（FULL_REVIEW；单个完整 PR）
- Reality rows: 7/7 applicable rows emitted（`PLAN_REALITY: phase=eng rows=7/7 missing=none unverified=0： verdict=complete`），0 unverified premises
- Architecture Review: 3 issues found（R1/R3 已批准并写回；R2 保持 blocked）
- Code Quality Review: 1 issue found（R4 cancel token 接线已写入既定取消范围）
- Test Review: diagram produced, 1 gap identified（R5 控制竞态证明已补齐，未执行实现测试）
- Performance Review: 0 issues found
- NOT in scope: written（14 items；含 2 个 design-deferred 与 1 个 R2 blocked 项）
- What already exists: written（区分可复用校验/取消底座与缺失的持久恢复/接线）
- TODOS.md updates: 0 items proposed in this engineering review；design review 已登记 2 items
- Failure modes: 1 critical gap flagged（R2 审批持久 decision/执行恢复未批准；实现验证尚未发生）
- Unresolved decisions: 1 in this review（R2/D2=C）
- Outside voice: codex / plan-review — unavailable（自定义 endpoint 模型配置无效；原生 fallback 5 分钟无完整报告，未采用部分输出）
- Parallelization: 3 implementation lanes + final acceptance，3 parallel / 1 sequential；共享 runtime/db/cloud/web 按依赖串行协调
- Lake Score: 1/2（D3 选择 10/10；D2 选择 2/10 的保持待补；D1 无 coverage 计分）
- Test Plan: `~/.gstack/projects/Grong-tinyiothub/chenguorong-e4-p2-declaration-closeout-eng-review-test-plan-20261026-192418.md`
- Task JSONL: `~/.gstack/projects/Grong-tinyiothub/tasks-eng-review-20261027-151659.jsonl`（11 tasks；无 R2 审批实现任务）

## Unresolved decisions that may bite you later

- **R2 / D2=C（pending）**：用户选择保持现状待补，没有批准持久 approval、冻结参数、decision 与执行关联或崩溃恢复方案。审批路由、执行恢复、审批按钮及其崩溃测试继续 blocked；完整 Runtime PR 在 R2 解决前不可开始，也不得把旧 DashMap/heartbeat 路径当成满足契约的实现。

## Suppressed findings

None recorded in this review.

## Design Review Pass Record

- **Step 0 — 7/10。** 用户选择全部七个 pass；目标是补全实现前设计契约，不重做工程评审。
  分类为 **OPERATE / APP UI**：用户要完成持续 Task 的监督、核查与干预，不是阅读营销页。
- **Step 0.5 — unavailable。** 请求 3 个视觉 variants 时设计工具报告无 OpenAI API key，
  0/3 图片保存；按规则停止生成、check 与 compare board。没有手工替代 mockup，
  后续评分只依据计划、DESIGN.md 与现有前端契约。
- **Design Outside Voices — skipped。** 用户选择跳过；该阶段没有外部覆盖或 clean 信用。

| Pass | 分数 | 已批准改动与剩余缺口 |
| --- | --- | --- |
| 1. Information Architecture | 6→10 | Issues 1/2/3/19：Task 列表、专用创建页、objective 优先的详情头部、详情内 Flow 标签与切换清理均有明确契约 |
| 2. Interaction State Coverage | 5→10 | Issues 4/5/6/7/8/21：完整五态矩阵、历史只读、Task cancel、Job Stop、内联审批与 Pause/Resume 的服务器投影和未知结果 |
| 3. User Journey & Emotional Arc | 6→10 | Issue 9：10 步信任故事板与 5 秒/5 分钟/长期使用验收已写入 §11.1 |
| 4. AI Slop Risk | 7→8 | Issues 10/11：字号与实用型 OPERATE 层级已明确；用户保留系统字体，默认字体通用感仍是缺口，因此不评 10 |
| 5. Design System Alignment | 5→10 | Issues 12/13：双主题无边框、token 路径、组件复用与浏览器表面均有明确映射 |
| 6. Responsive & Accessibility | 4→9 | Issues 14/15/16/17/18/20：三档布局、键盘/焦点/滚动、读屏、reduced-motion 与 AA 已明确；captions deferred，保留 1 分缺口 |
| 7. Unresolved Design Decisions | unscored | 21 项均有独立回答：19 项 resolved，2 项 deferred；deferred 项只进入 NOT in scope/TODOS，不进入本期 accepted tasks |

Pass 4 采用 OPERATE 规则。七个 hard-rejection 模式在修订后的计划中没有未处置项：
首屏不是 KPI/通用 SaaS 卡片网格，主工作区不是堆叠营销卡，标题有明确行动，文字不压在
繁忙图片上，没有重复情绪文案、无目的 carousel 或弱品牌大图。唯一保留的 AI-slop 风险是
系统字体作为默认主字体；它是用户明确接受的视觉债务，而非遗漏。

Litmus：品牌与产品在首屏可辨识 **YES**（Runtime + objective + 既有品牌表面）；有单一视觉
锚点 **YES**（Agent Loop 主工作区）；只看标题可理解 **YES**（Runtime/Current/Recent/
Agent Loop/Devices/Flow）；每个区域一个职责 **YES**；卡片确有必要 **YES**（ToolCard 与
设备实体承载交互，不是装饰网格）；动效服务层级 **YES**（只用于状态与空间关系，且有完整
reduced-motion）；移除装饰阴影仍成立 **YES**（依赖布局、字号、间距与文字，不依赖 glow）。
这些结论没有 PNG 或浏览器实测支持，实现后仍需 `/design-review`。

## Design Review Completion Summary

```text
+====================================================================+
|         DESIGN PLAN REVIEW — COMPLETION SUMMARY                    |
+====================================================================+
| System Audit         | DESIGN.md present; Vite + Lit; OPERATE UI   |
| Step 0               | 7/10; all seven passes selected             |
| Pass 1  (Info Arch)  | 6/10 → 10/10 after fixes                   |
| Pass 2  (States)     | 5/10 → 10/10 after fixes                   |
| Pass 3  (Journey)    | 6/10 → 10/10 after fixes                   |
| Pass 4  (AI Slop)    | 7/10 → 8/10; system font deferred          |
| Pass 5  (Design Sys) | 5/10 → 10/10 after fixes                   |
| Pass 6  (Responsive) | 4/10 → 9/10; captions deferred             |
| Pass 7  (Decisions)  | 19 resolved, 2 deferred; unscored           |
+--------------------------------------------------------------------+
| NOT in scope         | written (13 items total, 2 design-deferred) |
| What already exists  | written; tokens and concrete UI components  |
| TODOS.md updates     | 2 proposed, 2 added                         |
| Approved Mockups     | 0 generated, 0 approved                     |
| Decisions made       | 21 added to plan                            |
| Decisions deferred   | 2 (listed below)                            |
| Overall design score | 4/10 → 8/10                                 |
+====================================================================+
```

总体分取六个评分 pass 的最低值，不是平均分；Pass 7 不评分。所有评分 pass 均达到 8+，
但两个用户明确 deferred 的缺口仍存在，因此本轮状态是 **ISSUES OPEN**，不是 clean。
设计契约已写入，尚未实现或做真实页面视觉验收。

### Unresolved Decisions

| DECISION NEEDED | IF DEFERRED, WHAT HAPPENS |
| --- | --- |
| 是否以有意图的自托管主字体替代系统栈 | 本期保留 `--font-body`/`--font-display` 与中文系统回退，默认字体通用感仍存在；Pass 4 保持 8。后续按 `TODOS.md` 的 Runtime 主字体项评估，不阻塞本期 |
| 何时为含语音视频证据提供 WebVTT captions | 首版只提供画面/原声与 `Captions unavailable`，语音内容没有等价文本访问；Pass 6 保持 9。后续按 `TODOS.md` 的 captions 项在可信字幕来源就绪后实现 |

- **Task JSONL:** `~/.gstack/projects/Grong-tinyiothub/tasks-design-review-20261027-120412.jsonl`（7 tasks）
- **Deferred TODOs:** `TODOS.md` → `Multi-device Agent Runtime — Deferred`（2 items）
- **Outside voice:** design phase skipped by user；engineering phase 的 unavailable 记录独立保留。

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 1 | CLEAR | mode: HOLD_SCOPE, 0 critical gaps |
| Outside Review | codex plan-review + design outside-voice | Independent 2nd opinion | 5 | UNAVAILABLE / SKIPPED | 0 completed external reviews; 0 usable findings |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 2 | ISSUES OPEN | 5 issues, 1 critical gap; 1 unresolved decision |
| Design Review | `/plan-design-review` | UI/UX gaps | 1 | ISSUES OPEN | score: 4/10 → 8/10, 21 decisions, 2 deferred |
| DX Review | `/plan-devex-review` | Developer experience gaps | 0 | — | not run |

- **OUTSIDE COVERAGE:** codex, engineering plan-review phase, unavailable——本轮自定义 endpoint 未配置 frontier plan-review 模型；原生 fallback 5 分钟内无完整报告，两次 unavailable 均已记录，未采用部分输出。没有外部覆盖或 clean 信用。
- **OUTSIDE COVERAGE:** codex, design phase, skipped by user——未启动独立设计 reviewer，0 findings；不得从 engineering 记录推断 design 外部覆盖。
- **VERDICT:** CEO CLEAR；ENG ISSUES OPEN（R1/R3 已处置，R2 审批恢复仍 blocked）；DESIGN ISSUES OPEN（2 个 deferred 缺口）。eng review required；完整 Runtime PR 在 R2 解决前不可开始，未实现、未验收。

**UNRESOLVED DECISIONS:**
- 本轮 R2 / D2=C：Task 审批的持久 decision、执行关联与崩溃恢复保持 pending；审批路由、执行恢复、审批按钮及其测试均 blocked，完整 Runtime PR 在解决并批准该机制前不可开始。
- 前次设计评审 deferred：系统字体继续作为 Runtime 主字体；有意图的字体选型已登记 `TODOS.md`，Pass 4 保持 8/10。
- 前次设计评审 deferred：含语音视频证据的 WebVTT captions 已登记 `TODOS.md`；首版显示 `Captions unavailable`，Pass 6 保持 9/10。

## Current Follow-up Status（2026-10-27）

以上 GSTACK REVIEW REPORT 保留第二轮工程评审的历史结果，不改写当时的 R2/D2=C 回答。
用户随后显式重开 R2 并选择 A“持久化并幂等续跑”；规范契约见 R2 Follow-up resolution，
实施任务见 `docs/superpowers/plans/2026-10-27-multi-device-agent-runtime.md` 的 T6A/T7B/T8。

- **当前 R2：** resolved / approved for implementation planning；持久 decision、冻结参数、
  原子 outbox、Job 插入/Task 状态事务及幂等崩溃续跑均已写入。
- **当前工程门：** 2026-10-27 F1/F2 聚焦复核通过：paused reject/expiry/unavailable
  的 `Deferred` 传播、原 decision outbox 保留与同事件 resume，以及 T6A/T8/QA 回归契约
  均已核对；本轮 0 个新发现、0 个新未决决策、0 个 critical gap，工程计划门已满足。
  报告附在实施计划末尾；实现测试与验收仍待完成。
- **当前交付门（D6）：** 用户选择“全部并入 PR #101”。在当前分支以声明提交 `6876707f`
  为基线继续 SDD，不再等待 PR #101 先合入，也不另开 Runtime PR；声明收口与完整 Runtime
  最终进入同一个 PR。该决定不授权 push/merge，实现测试与验收仍是交付门。
- **当前交付：** 12 个任务按 D6 全部并入现有 PR #101，三个默认关闭 flags；没有审批实现的
  范围豁免，也没有 commit/push/merge 授权。功能尚未实现或验收，历史 blocked 文字仅作评审记录。
- **仍保留的非阻塞 deferred：** 系统主字体与 WebVTT captions，按设计评审和 TODOS 处置。
