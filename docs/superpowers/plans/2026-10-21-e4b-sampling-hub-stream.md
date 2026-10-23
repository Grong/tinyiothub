# E4b 需求驱动采样 + Hub 订阅流 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 DataServer 按 source 需求采样（默认全采、`demandOnly` 门控、health 常开、30s 宽限），用每设备 worker 执行驱动，并交付按 thing/source 的 Hub SSE 订阅流驱动 demand；E3 `/live/stream` 不变。

**Architecture:** 中央 sampler（50ms tick）计算 due/demand，向每设备 worker 发送 Read/Execute/SetDemand；worker 独占驱动，读合并 latest-wins、Execute 优先；DemandTracker 由 port 快照（1s）+ 显式订阅 + 近期读取 TTL 并集驱动；Hub stream 连接生命周期写入订阅，慢客户端读 latest-wins、控制有序。

**Tech Stack:** Rust 2024、tokio（mpsc/broadcast/select）、axum SSE、serde、web Lit + vitest。

## Global Constraints

- 上位 spec：`docs/superpowers/specs/2026-10-21-e4b-sampling-hub-stream-design.md`；偏差先改 spec。
- runtime 零 db/sqlx；demand 全走 `DemandSource` port。
- 默认全采；只有 `demandOnly:true` 受需求门控；`health:true` 恒采。
- 需求宽限 30s；近期读取 TTL 60s；订阅 TTL 30s（心跳续期）。
- worker：读合并 latest-wins；Execute FIFO 优先；队列上限 32；满时丢最旧 Read/拒绝新 Execute。
- E3 `/live/stream` 账本尾随保持不变；Hub stream 是新增端点。
- 多租户：所有 key `(workspace, thing, source)`；跨区拒绝。
- E4c/d/e 不做（资源、stop 取消、maps、全套验收）。
- 每次提交前 `cargo fmt --all`（回滚无关的 scheduler.rs/watch.rs churn）。

---

### Task 1: DemandSource port + DemandTracker

**Files:**
- Modify: `crates/runtime/src/ports.rs`
- Create: `crates/runtime/src/access/demand.rs`
- Modify: `crates/runtime/src/access/mod.rs`
- Test: inline

**Interfaces:**
- `DemandPropertyRef { workspace_id, thing_id, property }`
- `DemandSource::snapshot() -> Result<Vec<DemandPropertyRef>, String>`
- `DemandTracker::{new, record_read, subscribe, touch, unsubscribe, set_snapshot, has_demand, gc}`
- `SubscriptionId = u64`

- [ ] Write tests for: TTL expiry, subscription union, snapshot union, unsubscribe, duplicate reads, expired subscription GC.
- [ ] Implement `DemandTracker` with `RwLock<HashMap>` and `Instant` (inject `now` in tests via `has_demand_at`/`gc_at` helpers to avoid sleeps).
- [ ] Add `DemandSource` trait to `ports.rs`.
- [ ] Run `cargo test -p runtime access::demand`; commit `feat(e4b): DemandTracker 与 DemandSource port`.

---

### Task 2: property→source 映射 + SamplingCatalog

**Files:**
- Create: `crates/runtime/src/access/catalog.rs`
- Modify: `crates/runtime/src/access/mod.rs`
- Test: inline

**Interfaces:**
- `SampledThing { workspace_id, thing_id, driver_name, sources: Vec<SourceDecl> }`
- `SamplingCatalog::list() -> Vec<SampledThing>`
- `source_for_property(&[SourceDecl], property: &str) -> Option<String>`

- [ ] Test: exact `fields` match; empty-field source fallback when exactly one poll source; ambiguous → None; non-poll source excluded.
- [ ] Implement the mapping.
- [ ] Implement `AccessSamplingCatalog` in `apps/cloud` (Task 5) later; for now define the trait + a fake test catalog.
- [ ] Run tests; commit `feat(e4b): sampling catalog 与 property→source 映射`.

---

### Task 3: DriverWorker + queue policy

**Files:**
- Create: `crates/runtime/src/driver/worker.rs`
- Modify: `crates/runtime/src/driver/mod.rs`
- Test: inline with a counting fake driver

**Interfaces:**
- `WorkerCommand::{Read{sources}, Execute{cmd}, SetDemand{sources}, Stop}`
- `WorkerHandle { tx: mpsc::Sender<WorkerCommand>, status: Arc<RwLock<ThingOverview>> }`
- `spawn_worker(thing, driver: DriverWrapper, value_sink: ValueSink, events: Arc<dyn PropertyChangeSource>) -> WorkerHandle`

- [ ] Test: two Reads while busy coalesce to the latest source set (read_count proves one extra call); Execute is served before a pending Read; queue cap drops oldest Read; Execute overflow returns busy.
- [ ] Implement two channels: `exec_tx` (bounded 32) and `read_tx` (capacity 1 coalescing slot); worker `tokio::select!` biased to exec.
- [ ] Worker applies readings to its own `Thing` and publishes a `DriverReading` to the ValueSink; updates status overview.
- [ ] Run tests; commit `feat(e4b): 每设备 driver worker 与队列策略`.

---

### Task 4: DataServer 切换到 worker

**Files:**
- Modify: `crates/runtime/src/data_server.rs`
- Modify: `crates/runtime/src/driver/mod.rs`
- Modify tests: `crates/runtime/tests/push_driver_sink_test.rs`, `crates/runtime/tests/derived_driver_test.rs`, `apps/cloud/src/tests/park_derived_e2e_test.rs`
- Test: existing + new worker path

**Interfaces:**
- `DataServer::new(device_cache, event_bus, sampler: Arc<Sampler>, catalog: Arc<dyn SamplingCatalog>, demand: Arc<DemandTracker>)`
- `DataServer::{get_device, get_things, execute_command, execute_commands, get_driver_overview, reset_device_driver, set_device_offline, remove_device, worker_count}`

- [ ] Replace `driver_cache: Cache<String, Arc<RwLock<DriverWrapper>>>` with `workers: Cache<String, Arc<WorkerHandle>>`.
- [ ] `initialize_drivers` spawns workers; Push drivers call `start_push(value_sink, event_bus)` and `set_demand` with health sources.
- [ ] `execute_command` sends `Execute` to the worker; queue-full returns `Error::DriverError("busy")`.
- [ ] Keep `value_sink` consumer applying readings to cache/EventBus (worker sends `DriverReading`).
- [ ] Update tests that inserted into `driver_cache` to use worker handles or a test-only `register_worker`.
- [ ] Run `cargo test -p runtime` and `cargo test -p tinyiothub-cloud park_derived`; commit `refactor(e4b): DataServer 每设备 worker`.

---

### Task 5: 中央 sampler

**Files:**
- Create: `crates/runtime/src/access/sampler.rs`
- Modify: `crates/runtime/src/data_server.rs` (start sampler loop)
- Modify: `apps/cloud/src/shared/service_manager.rs` (construct DemandTracker/Sampler/Catalog/DemandSource)
- Test: inline with fake worker/catalog/demand

**Interfaces:**
- `Sampler::new(catalog, demand, workers, tick: Duration)`
- `Sampler::run(shutdown_rx)`
- `Sampler::tick_once(now)` for tests

- [ ] Tests: always-source samples every interval; `demandOnly` skips without demand; health samples regardless; 30s grace keeps sampling then stops; due sources grouped into one Read per thing.
- [ ] Implement `tick_once`: maintain `HashMap<(ws,thing,source), last_sample>`; map demand properties to sources via catalog; on demand transition log `sampling_demand_start/stop` (once).
- [ ] Wire `Sampler::run` with a 50ms interval and shutdown receiver.
- [ ] Run tests; commit `feat(e4b): 需求驱动 sampler（门控/health/宽限）`.

---

### Task 6: AccessService 近期读取 + 订阅 API

**Files:**
- Modify: `crates/runtime/src/access/service.rs`
- Modify: `apps/cloud/src/domains/agent/host/tools/thing/{device.rs,read_property.rs}` (record reads)
- Test: inline + cloud read tests

**Interfaces:**
- `AccessService::{record_read, subscribe, touch_subscription, unsubscribe}`
- `AccessService.demand: Arc<DemandTracker>` (new constructor arg)

- [ ] `read_device` records the requested properties or all declared properties; `read_property` records one.
- [ ] Subscribe API returns `SubscriptionId`, TTL 30s; `touch` refreshes.
- [ ] Tests: read makes a demandOnly source demand for 60s; subscribe/unsubscribe; workspace isolation.
- [ ] Run tests; commit `feat(e4b): 读取/订阅写入 DemandTracker`.

---

### Task 7: Cloud DbDemandSource

**Files:**
- Create: `crates/db/src/demand.rs` (queries)
- Modify: `crates/db/src/lib.rs`
- Modify: `apps/cloud/src/shared/access_ports.rs` (`DbDemandSource`)
- Test: db tests + cloud adapter test

**Interfaces:**
- Db methods: `list_watch_property_refs() -> Vec<(ws, thing, property)>`, `list_derived_property_refs()`, `list_alarm_property_refs()`
- `DbDemandSource::snapshot()` unions them.

- [ ] SQL: watch_rules (kind=property -> condition JSON property; health/availability -> property=""), derived thing_properties (`expression IS NOT NULL` -> refs JSON), device_alarm_rules joined to thing_properties for property name.
- [ ] Tests: seed one of each; snapshot contains all; tenant scoping.
- [ ] Run `cargo test -p db demand`; commit `feat(e4b): cloud demand 快照适配器`.

---

### Task 8: Hub stream backend

**Files:**
- Create: `crates/runtime/src/hub_stream.rs`
- Create: `apps/cloud/src/domains/agent/host/handler/hub_stream.rs`
- Modify: `apps/cloud/src/domains/agent/host/handler/mod.rs` or `api/mod.rs` route
- Test: handler tests with a fake AccessService/EventBus

**Interfaces:**
- `HubStreamRegistry::{subscribe, unsubscribe, connections}` (or direct DemandTracker)
- `GET /v1/workspaces/{id}/hub/stream?things=&sources=`
- Events: `snapshot`（每 source 带 `lastHeartbeatAt` RFC3339）、`reading`、`control`（job/state_change；watch 仍在 E3 `/live/stream`）、`ping`、`resync`

- [ ] Auth/tenant check from `Claims`; validate things/sources against manifests; limits 50/16; return 400/403.
- [ ] First frame: snapshot from `DataServer.get_device` + LiveState summary.
- [ ] Subscribe to EventBus; map PropertyChange -> `reading`, watch/job/state_change -> `control`; per-connection coalescing map + control VecDeque (256); Lagged -> `resync`.
- [ ] Subscribe to DemandTracker on connect (TTL 30s); Drop guard unsubscribe; touch on each event/heartbeat.
- [ ] Tests: 200 frame order snapshot->reading; invalid source 400; cross-workspace 403; disconnect removes demand; control overflow resync.
- [ ] Run tests; commit `feat(e4b): Hub 订阅流端点与连接需求生命周期`.

---

### Task 9: Web hub-stream client + device card

**Files:**
- Create: `web/src/api/hub-stream.ts`
- Modify: `web/src/ui/views/live-devices.ts` (selected device card)
- Create: `web/src/api/hub-stream.test.ts`
- Test: vitest

**Interfaces:**
- `connectHubStream(workspaceId, things, sources, handlers)` returns `{close, pause, resume}`; reuse `parseSseChunk` from live.ts.
- Device card shows `source` readings and a live status; Flow keeps `/live/stream`.

- [ ] Parser/reconnect tests; client sends Bearer; exponential backoff 1s→30s; pause on hidden.
- [ ] Wire selected device card to `connectHubStream` with its declared sources; cleanup on selection change.
- [ ] Run `npm run test` (Node 24 path) and build; commit `feat(e4b): Hub 订阅流 web 客户端与设备卡接入`.

---

### Task 10: 指标/账本 + E2E + 执行记录

**Files:**
- Modify: sampler/worker/hub stream for counters
- Create: `docs/superpowers/plans/2026-10-21-e4b-execution-record.md`
- Test: E2E

- [ ] Add counters: demand active/skipped, sampling lag, worker queue depth/coalesced, hub stream connections/resyncs.
- [ ] Ledger `sampling_demand_start/stop` on transitions (not every tick).
- [ ] E2E: fake/demo driver with `demandOnly` source -> Hub stream connect -> readings; disconnect -> 30s grace -> stop; health source continues.
- [ ] Regression: E3 `/live/stream` tests, watch/derived/alarm tests, push driver tests.
- [ ] Run full `cargo test --workspace --lib --bins --tests`, clippy, web vitest/build.
- [ ] Write execution record; commit `docs(e4b): 执行记录与验证结果`.

---

## Self-Review

- Spec coverage: demand port/tracker (1), mapping/catalog (2), worker/queue (3-4), sampler/health/grace (5), recent reads/subscriptions (6), cloud adapter (7), Hub stream + lifecycle (8), web client/card (9), metrics/E2E (10). E4c-e excluded.
- Type consistency: `DemandPropertyRef`, `DemandTracker`, `SamplingCatalog`, `WorkerHandle`, `Hub stream` event names are defined once.
- Known risk: DataServer worker refactor touches tests and push driver behavior; Task 4 must run the full runtime + cloud derived/push suites.
