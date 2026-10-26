# 通用 Agent Runtime 与多设备协同工作台设计

- 日期：2026-10-26
- 分支：`e4-p2-declaration-closeout`
- 状态：架构、Task/Job/Watch、媒体、两栏工作台、错误恢复、验收六节已与作者逐节确认；待作者文本审阅
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

[agh-architecture]: https://github.com/AgnesAI-Labs/agnes-harness/blob/main/docs/develop/architecture.md
[agh-hub-api]: https://github.com/AgnesAI-Labs/agnes-harness/blob/main/packages/mhs/server/hub-api.md
[agh-cards]: https://github.com/AgnesAI-Labs/agnes-harness/blob/main/packages/mhs/plugin/client/src/cards/cards.tsx
[agh-overview]: https://github.com/AgnesAI-Labs/agnes-harness/blob/main/packages/mhs/plugin/client/src/layout/overview.tsx
[agh-host]: https://github.com/AgnesAI-Labs/agnes-harness/blob/main/packages/mhs/plugin/client/src/hosts/agnes/index.tsx
[agh-flow]: https://github.com/AgnesAI-Labs/agnes-harness/blob/main/packages/mhs/plugin/client/src/layout/flow.tsx

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
4. `apps/cloud` 是组合根：Task 编排、DB 适配、HTTP/SSE、媒体代理和权限。
5. `db` 拥有全部 SQL 与新表；`web` 只消费投影，不直接写设备或查询实时真相。
6. Agent 不直接访问设备；所有感知、读图、调用、停止和订阅都经 Device Hub。

## 4. Task Runtime

### 4.1 Task 与 Run

- **Task 是持续任务，Run 是一次推进。** 一个 Task 可因 job 结果、watch 或设备事件产生多个 Run。
- Task 状态：`active | waiting_job | waiting_watch | needs_approval | paused |
  completed | failed | cancelled`。
- 人工 `paused` 优先于所有非终态；未暂停时，等待状态按
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
- 用户暂停/取消 Task 与 `Stop all` 不同：前者停止推进；后者停止设备并把取消结果交给相关 Task。
- Task 终止采用幂等迁移；重复取消不重复发出设备副作用。

### 4.4 Device Job

每次设备调用创建 Hub 生成的稳定 `job_id`，并关联：

```text
task_id / run_id / tool_call_id / job_id / device_id
```

状态：`accepted | running | paused | done | failed | rejected | cancelled |
cancelled_unknown | interrupted`。`accepted/running/paused` 为活跃态，
其余为终态；每个 job 最多一个终态。

执行顺序：

1. Hub 完成声明校验、参数校验、资源 claim 和 timeout 钳制。
2. 持久化 job `accepted` 后再向 worker 派发，保证返回 `running` 前可追溯。
3. `accepted/progress/result/rejected` 全部落账；progress 只更新投影，不唤醒模型。
4. 声明的 `ValidatedCall.timeout_ms` 必须成为真实执行 deadline，覆盖排队与在途阶段。
5. 工具等待窗口为 `min(50s, 剩余 Run 预算, job deadline)`；
   完成则直接返回结果，窗口耗尽但 job 仍活跃则返回
   `{status:"running", job_id}`。
6. job deadline 先于等待窗口到达时，返回真实 timeout/cancelled 结果，而不是 `running`。
7. 最终结果唤醒原 Task；重复终态只落一次事实、最多唤醒一次。

`stop` 必须覆盖在途与已排队 Execute；stop 前入队的命令不得继续绕过闸门执行。
迟到结果保留账本但按 `job_id` 去重。`cancelled_unknown` 表示物理结果未知，
Agent 重试前必须先读设备状态，禁止盲目重复副作用。

### 4.5 Task-owned Watch

- Agent 可为 Task 注册 watch，包含 `owner_task_id`。
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
- 模型适配器声明支持 vision 时才把图片作为附件输入；不支持时返回 `image_unavailable`，禁止生成“已看到图片”的描述。
- 读取 `video` 返回 `video_is_for_human_not_model`。

### 5.4 Workbench 视频通道

- 首版支持 Hub 鉴权代理后的 MJPEG 与 HLS；MJPEG 使用图片帧播放，HLS 使用原生播放器或 hls.js。
- 视频只在用户观看时启用，离开设备、切换 Task 或超时后停止。
- 播放票据按 `workspace/device/source` 限定且 5 分钟有效，到期前由主会话刷新；不把长期 JWT 放入 URL。
- 其他协议明确返回不支持；不得显示假播放器或静态图冒充实时视频。
- 视频不默认录像，历史回放展示保存的图片证据与设备状态。

### 5.5 证据

每个证据引用携带：

```text
task_id / run_id / tool_call_id? / job_id?
device_id / source_id / seq / time / evidence_id
```

- 图片二进制进入 `MediaStore`，首版本地磁盘实现；路径为 `data/media/<workspace>/<sha256>`。
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

**`agent_tasks`**

```text
id, workspace_id, objective, session_key, created_by, status, current_run_id,
state_version, created_at, updated_at, ended_at
```

索引：`(workspace_id, status, updated_at)`。`session_key` 用于 Chat 与
Workbench 指向同一 Task，API 创建时可为空；`created_by` 记录创建者供审批与
审计使用。

**`agent_runs`**：新增可空 `task_id` 与 `wake_event_id`；分别为其建索引，
并对非空 `(task_id,wake_event_id)` 建唯一约束，旧行保持 NULL。

**`task_wake_outbox`（持久唤醒与去重）**

```text
task_id, event_id, signal_json, dispatch_state,
created_at, dispatched_at
```

主键 `(task_id,event_id)`；`dispatch_state` 为 `pending | dispatched | dead`。
dispatcher 扫描 pending，崩溃后重试；连续失败按现有 DLQ 模式处理并保留告警。

**`watch_rules`**：新增可空 `owner_task_id` 与索引。

**`device_jobs`（活跃与查询投影，生命周期事实仍在 ledger）**

```text
id, workspace_id, device_id, tool, args_json, state,
progress_json, result_json, task_id, run_id, tool_call_id,
driver_call_id, started_at, ended_at
```

约束：`tool_call_id` 唯一；索引 `(task_id, state)`、
`(workspace_id, device_id, state)`。人工 UI 直接调用产生的 job 允许
`task_id/run_id/tool_call_id` 为 NULL；SQLite 的 UNIQUE 不限制多个 NULL。

**`media_evidence`**

```text
id, workspace_id, task_id, run_id, tool_call_id, job_id,
device_id, source_id, source_seq, media_time,
mime, size_bytes, sha256, storage_path, created_at, expires_at
```

关联字段 `task_id/run_id/tool_call_id/job_id` 可空，便于设备独立图片复用同一存储；
Task 证据至少包含 `task_id`，Run 证据至少同时包含 `task_id/run_id`。

所有新持久化都经 ports；capability crate 不得直接写 SQL。

### 6.3 主要 ports

- `TaskStore`：创建、读取、列表、CAS 状态迁移、当前 Run 绑定，以及“状态迁移 + outbox”事务和 pending 唤醒领取/标记。
- `DeviceJobStore`：accepted/progress/result 投影、活跃列表、按 tool_call/job 查询。
- `MediaStore`：写入受限二进制、读取、删除过期内容。
- `MediaEvidenceStore`：证据元数据与关联查询。
- 既有 `WatchRuleStore`：增加按 active Task 重织与清理。
- 既有 `HubLedger`：承载版本化事件与引用。

### 6.4 HTTP/SSE

复用现有 Task 创建入口，不另建重复 API：

- `POST /workspaces/{id}/agent/tasks`：`{text}` → 持久 Task，返回
  `{taskId,status:"accepted"}`；返回的 UUID 不再是与 Run 无关的受理号。
- `GET /workspaces/{id}/agent/tasks`：分页列表。
- `GET /workspaces/{id}/agent/tasks/{task_id}`：Task、Run、job、watch 与关联摘要。
- `POST /workspaces/{id}/agent/tasks/{task_id}/cancel`：幂等取消。
- `GET /workspaces/{id}/runtime/stream?taskId=...&after=...`：
  snapshot/event/resync。
- `GET /workspaces/{id}/evidence/{evidence_id}`：鉴权图片证据。
- `GET /workspaces/{id}/devices/{device_id}/sources/{source_id}/video`：
  5 分钟票据鉴权的 MJPEG/HLS 入口。

Task 创建、取消、设备调用和停止沿用 Agent 管理权限；读取 Task/Devices/证据
沿用 workspace 权限，所有路径做 workspace 隔离。

## 7. 两栏 Agent Runtime Workbench

### 7.1 页面结构

新增 `#/runtime`（当前/最近 Task）与 `#/runtime/tasks/{id}`，现有 AI 中心链接到该视图；不新增 Goal 页面。

```text
┌──────────────────────────────────┬─ seam ─┬──────────────────┐
│ Agent Loop（主视图）              │ 拖拽/折叠 │ Devices          │
│ Reason / Act / Tool / Result     │          │ Task 关联设备卡    │
└──────────────────────────────────┴─────────┴──────────────────┘
```

- 使用浅色、紧凑的 ToolCard 信息流，不使用深色大仪表盘或 Goal 卡片。
- Loop 直接展示设备状态、工具结果、progress、图片和 `Done/Details`。
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
- 展示 health、zone/position、Ready/Busy、job/progress 和关键字段。
- 点击卡片进入通用设备详情；不为设备型号创建专用组件。

### 7.4 Seam 与 Flow

- seam 支持拖拽调宽，最小 360px，最大视口 75%；折叠后为 36px 竖向 Devices rail。
- 中央按钮折叠/展开；键盘左右键每次调整 24px；宽度与展开状态本地保存。
- 小屏（<980px）隐藏 seam，Loop 优先，Devices 移到下方。
- Flow 采用双通道视觉：`CONTROL`（对应参考截图 MHS）与 `DATA`（对应 MOS），不代表协议兼容。
- CONTROL 展示 call/result 方向，DATA 展示数据源粒子；设备端口与卡片位置一致。
- `prefers-reduced-motion` 下停用粒子动画，保留静态总线与状态文本。

### 7.5 状态与可访问性

覆盖 loading、empty、error、offline、partial、expired evidence、
unsupported media、waiting、approval、cancelled。

设备链接、详情、折叠、停止均可键盘操作；seam 有 separator 语义和宽度说明；视频有暂停与断线状态；图片有可理解的替代文本。

## 8. 错误、安全与背压

- 模型失败、无效输出或预算耗尽形成明确 Run 结果；不得伪造推进。
- 审批过期、拒绝、不可用分别呈现，均不执行设备动作。
- busy 返回资源与持有者；设备删除/离线使 watch 失效并通知 Task。
- 设备文本、detections 和媒体元数据按不可信输入处理；媒体入口校验 workspace、MIME 和大小。
- 视频播放描述中的驱动凭证只留 Hub；媒体代理限制协议、重定向、连接与读取超时。
- 原始视频不进 prompt、DB 或账本；progress 不唤醒模型。
- Task、Run、Job、Watch、设备和证据均以 workspace 隔离，跨区统一拒绝且不泄露存在性。
- 账本/媒体故障采用显式降级；不能把旧图片、旧遥测或旧状态当成实时事实。

## 9. 验收与测试

### 9.1 通用性

- 夹具 A：数值遥测 + 图片 + 视频 + 可执行动作。
- 夹具 B：数值遥测 + detections + 不同工具与资源，无视频。
- 同一 Runtime、prompt 和 Workbench 各完成一个多设备 Task，核心与 prompt 零场景改动。

### 9.2 核心端到端

1. **持续推进**：数值感知 → 图片 → 调用 → job progress → result 唤醒原 Task
   → 验证 → completed；完整断言关联 ID。
2. **Watch**：注册后等待；命中唤醒原 Task；重复事件不重复推进；重启重织。
3. **审批与停止**：pending/approve/reject/unavailable；stop 覆盖在途与排队；
   `cancelled_unknown` 后先读状态再决定重试。
4. **重启恢复**：accepted 后、result 前重启，不重复执行设备动作；Task 恢复，
   迟到结果最多唤醒一次。
5. **媒体**：大小/类型/新鲜度、非 vision、视频需求生命周期、断流重连、证据过期
   全部有明确结果。
6. **Workbench**：关联设备出现规则、Loop 定位、seam 拖拽/折叠/键盘、Flow、
   减少动态效果、图片/视频、SSE resync、历史只读。
7. **性能与隔离**：测试环境 watch 命中到 Run 入队 `<2s`，job progress 到面板
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
