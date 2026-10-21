# E4b 需求驱动采样 + Hub 订阅流子 spec

- 日期：2026-10-21
- 分支：ai-consolidation
- 上位文档：`docs/superpowers/specs/2026-10-21-e4-access-declaration-design.md`（§3.3 采样、§3.2 SSE 需求面、§1 每设备 worker）
- 范围：E4b。E4c/d/e（资源/取消、maps、全套验收）不在本期
- 状态：设计五问已与作者确认（2026-10-21）

## 0. 关键裁定

| # | 主题 | 裁定 |
|---|---|---|
| 1 | 需求来源 | runtime `DemandSource` port；cloud 适配器每 1s 返回 watch ∪ 派生 ∪ 告警的 property 引用快照。显式订阅与近期读取由 runtime 本地记录；需求=并集 |
| 2 | 调度架构 | DataServer 中央采样器（50ms tick）+ 每设备 driver worker；worker 独占驱动，不持锁跨 await |
| 3 | worker 队列 | 读合并 latest-wins；Execute FIFO 且优先；队列上限 32；满时丢最旧 Read、拒绝新 Execute（busy） |
| 4 | SSE 关系 | 新增 Hub 订阅流 `GET /v1/workspaces/{id}/hub/stream`（按 thing/source，首帧 snapshot + 遥测 latest-wins + 控制有序）；E3 `/live/stream` 账本尾随保持不变 |
| 5 | 前端范围 | 交付 `web/src/api/hub-stream.ts`，Devices 工作台选中设备卡改用它；Flow 继续 `/live/stream` |

沿用上位 spec 已定：默认全采；仅 `demandOnly` source 门控；`health:true` 慢通道常开；需求消失 30s 宽限；近期读取 TTL 60s；push/stream 用 `set_demand` 起停订阅。

## 1. 组件与边界

```
crates/runtime/src/
  access/
    demand.rs        DemandTracker（本地订阅 + 近期读取 TTL + port 快照并集）
    sampler.rs       中央调度器（due 时间、demand 门控、health 慢通道、宽限）
  driver/
    worker.rs        DriverWorker（独占驱动；Read/Execute/SetDemand/Stop；双通道优先级）
  hub_stream.rs      Hub 订阅流注册表（连接生命周期 → demand；读数/控制分发）
  ports.rs           新增 DemandSource
crates/db/
  src/demand.rs      watch_rules / 派生 refs / alarm rules 快照查询（供 cloud 适配）
apps/cloud/src/
  shared/access_ports.rs   DbDemandSource
  domains/driver/...       不新增；demand 适配走 access_ports
  domains/agent/host/handler/hub_stream.rs  GET /v1/workspaces/{id}/hub/stream
web/src/
  api/hub-stream.ts
  ui/views/live-devices.ts（选中设备卡接入）
```

边界规则：

1. **runtime 零 db**：demand 快照经 `DemandSource` port；cloud 实现查询，runtime 只做并集与调度。
2. **worker 单写者**：每设备一个 worker 任务，驱动生命周期/调用都归它；DataServer 只持有 `WorkerHandle`。
3. **采样与呈现解耦**：sampler 只决定“何时向 worker 发 Read”；读数仍走 `cache_and_publish → EventBus → LiveState`，SSE/前端订阅 EventBus 投影。
4. **E3 账本流不动**：`/live/stream` 继续用于 Flow 回放；Hub 流是原始/派生读数的需求面，不承担账本游标。
5. **多租户**：所有 key = `(workspace_id, thing_id, source)`；跨工作区订阅拒绝。

## 2. 契约

### 2.1 DemandSource（port）

```rust
pub struct DemandPropertyRef {
    pub workspace_id: String,
    pub thing_id: String,
    pub property: String,   // health/availability watch 可为 ""
}

#[async_trait]
pub trait DemandSource: Send + Sync {
    async fn snapshot(&self) -> Result<Vec<DemandPropertyRef>, String>;
}
```

- cloud 适配器每 1s 调用；失败保留上一份快照并记 `demand_snapshot_error` 指标。
- 查询来源：`watch_rules`（kind=property/health/availability）、派生属性 `thing_properties.expression/refs`、`device_alarm_rules`（is_enabled=1，property_id → 属性名）。
- runtime 将 property 引用映射到 source：manifest 中 `sources[].fields` 精确匹配优先；否则若只有一个人可读 poll source，映射到它；否则丢弃并记 `demand_unmapped`。

### 2.2 DemandTracker

```rust
pub struct DemandTracker {
    snapshot: RwLock<HashMap<(String,String), BTreeSet<String>>>, // (ws,thing)->properties
    subscriptions: RwLock<HashMap<SubscriptionId, Subscription>>,  // ws, things, sources, expires_at
    recent_reads: RwLock<HashMap<(String,String,String), Instant>>, // (ws,thing,property)->at
}

impl DemandTracker {
    pub fn record_read(&self, ws:&str, thing:&str, property:&str);
    pub fn subscribe(&self, ws:&str, things:&[String], sources:&[String], ttl: Duration) -> SubscriptionId;
    pub fn touch(&self, id:&SubscriptionId);
    pub fn unsubscribe(&self, id:&SubscriptionId);
    pub fn set_snapshot(&self, refs: Vec<DemandPropertyRef>);
    pub fn has_demand(&self, ws:&str, thing:&str, source:&str, now: Instant) -> bool;
}
```

- 近期读取 TTL 60s；订阅 TTL 30s，客户端心跳/重连续期；过期清理由 sampler tick 执行。
- `has_demand` = 快照属性映射到该 source ∪ 订阅 source ∪ 近期读取属性映射。
- 默认全采：只有 `demandOnly:true` 的 source 查询 `has_demand`；`health:true` 恒 true。

### 2.3 Worker

```rust
pub enum WorkerCommand {
    Read { sources: Vec<String> },
    Execute { cmd: ThingCommand },
    SetDemand { sources: Vec<String> },
    Stop,
}

pub struct WorkerHandle { pub tx: mpsc::Sender<WorkerCommand> }
```

- worker 内两条通道：`exec_tx`（FIFO 优先）与 `read_tx`（容量 1 的合并在途槽；新 Read 合并 source 集，latest-wins）。
- 队列总量上限 32；满时丢最旧 Read；Execute 满 → 调用方收到 busy。
- worker 创建时：Poll 驱动按 sampler 调度；Push 驱动调用 `start_push(value_sink, event_source)`，并由 `SetDemand` 驱动 `set_demand`。
- `Stop` 停止任务；在途调用在 E4c 接取消（本期 worker 只停止后续）。

### 2.4 Hub 订阅流

```
GET /v1/workspaces/{id}/hub/stream?things=t1,t2&sources=telemetry,position
Accept: text/event-stream
Authorization: Bearer <jwt>
```

- 校验：workspace 归属；things 必须属于该 workspace；sources 必须在各 thing 的 manifest 中声明；单连接 things ≤50、sources ≤16；非法 400/403。
- 首帧：`snapshot` = 当前 LiveState/缓存读数（每 thing/source 最新值 + `lastHeartbeatAt`，RFC3339）。`lastChangeAt`（逐属性变更时刻）留待后续，不在 E4b 伪造。
- 增量：
  - `reading` `{v:1, ts, thingId, source, values}` — 遥测 latest-wins；慢客户端只保留每个 `(thing,source)` 最新一条。
  - `control` `{v:1, ts, thingId, kind, payload}` — job/state_change，按到达顺序；有界队列（256），溢出发 `resync` 并清空。watch 命中/超时仍由 E3 `/live/stream` 账本流提供（E4b control 不含 watch）。
  - `ping` 15s；`resync` `{v:1, reason:"lagged"|"overflow"|"reconnect"}`。
- 生命周期：连接建立 → `DemandTracker.subscribe`；SSE 断开/Drop → `unsubscribe`；heartbeat 续期。
- 实现：EventBus broadcast 订阅（1000 容量）；Lagged → `resync`；`reading` 由 PropertyChange 事件映射；`control` 由 watch/job 事件映射；首帧从 `DataServer.get_device` + LiveState 摘要。

## 3. 数据流

### 3.1 采样

```
每 50ms：
  for (ws, thing, source) in catalog:
    due = now >= last_sample + interval
    if !due: continue
    if source.health: sample           // 慢通道常开，按 interval
    else if source.demand_only && !tracker.has_demand(...): skip (记 demand_skip)
    else: sample
  group due sources per thing → worker.Read(sources)
  on reading → apply_property_values → cache_and_publish → LiveState/EventBus
```

- 需求消失后 30s 宽限：`last_demand_at`，宽限内继续按 interval 采；超时后 `demandOnly` 源停采。
- 目录来源：device_cache 的全部物 + 各自 manifest（`AccessService.manifests.get`）；manifest 缺失的物不采（fail-closed，记 metric）。
- 重启：sampler 状态易失；从 cache/manifest 重建；无需持久化。

### 3.2 近期读取

`read_device` / `read_property` / 快照装配经 `AccessService` 记录 `record_read`；`read_device` 可指定 properties，否则记录该 thing 的全部 declared properties。

### 3.3 push/stream

- sampler 计算某 thing 的活跃 source 集（demand ∪ health），变化时 `worker.SetDemand(sources)`。
- 驱动 `set_demand` 起停订阅；读数经 ValueSink 进入现有 `consume_driver_reading`。

## 4. 错误、指标与测试

| 失败 | 处理 |
|---|---|
| DemandSource 快照失败 | 保留上一份 + `demand_snapshot_error` 指标；不清空需求 |
| property 无法映射 source | 丢弃 + `demand_unmapped` |
| worker 队列满 | 丢最旧 Read；Execute busy（带持有者文案在 E4c 接） |
| SSE EventBus Lagged | 发 `resync`，客户端重取 snapshot |
| SSE 慢客户端 | reading latest-wins；control 溢出 `resync` |
| 连接断开 | Drop guard unsubscribe；TTL 兜底 |
| manifest 缺失 | 不采样 + `sampling_no_manifest` |

指标：`demand_active_sources`、`demand_skipped_sources`、`sampling_lag_ms`、`worker_queue_depth`、`worker_reads_coalesced`、`hub_stream_connections`、`hub_stream_resyncs`。
账本：`sampling_demand_start/stop`（仅状态变化，避免刷屏）。

测试：
- 单测：demand 并集/TTL/宽限、property→source 映射、due 调度、health 常开、queue 合并/优先/上限。
- 集成：fake driver + fake DemandSource → 无需求停采、有需求恢复、30s 宽限、push set_demand；Hub stream 鉴权/首帧/reading/control/resync/断开释放。
- Web：`hub-stream.ts` 分帧/重连；设备卡选中设备订阅。
- 回归：E3 `/live/stream` 全量测试；DataServer push/poll 测试改写为 worker 语义。

## 5. 退出标准与非目标

**退出**：Hub stream 连接驱动 demand → `demandOnly` 源按 interval 采样；断开 30s 后停采而 health 源继续；push 驱动 `set_demand` 起停；worker 队列策略（合并/优先/上限）有测试；Devices 工作台选中设备卡通过 Hub stream 收到 latest-wins 读数；E3 `/live/stream` 回归通过。

**非目标**：资源/仲裁与 stop 在途取消（E4c）；maps/coords（E4d）；全套验收 profile（E4e）；edge/X5；告警系统改造（只读取规则 refs）；E3 Flow 切流（保持账本尾随）。
