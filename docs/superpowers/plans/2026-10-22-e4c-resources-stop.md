# E4c 资源仲裁 + stop 在途取消 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 E1 每设备单资源仲裁细化为 manifest 声明的多资源 claim；stop 立即返回并在 ≤1s 内协作式中断在途调用（否则 `cancelled_unknown` 留痕）；composite 步骤同规则。

**Architecture:** 扩展 agent `ArbitrationRegistry` 为 `(thing, resource)` 多资源 claim（排序 all-or-nothing、exclusive/shared capacity、TTL、busy 带持有者）；工具/composite 从声明解析 `ResourceSpec` 并 RAII claim；worker 共享当前 `CancelToken` + in-flight 标记，stop 置闸门/释放 claim/取消当前令牌；worker 返回后按 1s 宽限发 `CommandCancelled` 异步事件与账本。

**Tech Stack:** Rust 2024、tokio、dashmap、现有 agent/runtime/cloud 分层。

## Global Constraints

- 上位 spec：`docs/superpowers/specs/2026-10-22-e4c-resources-stop-design.md`；偏差先改 spec。
- `runtime` 零 db/sqlx；资源声明来自 AccessService/declaration；账本走 `HubLedger` port。
- 多资源 claim：id 排序、all-or-nothing、失败释放已获；exclusive/shared capacity。
- 未声明 resources 的变更类工具默认 `__device__` exclusive；只读工具不 claim。
- stop 立即返回 `cancellation_requested`；≤1s 合作退出=`cancelled`，否则 `cancelled_unknown`。
- 保留 `try_acquire(thing, holder, ttl)` 兼容入口与 E1 语义。
- E4d/e 不做；不排队（busy 立即拒绝）。
- 每次提交前 `cargo fmt --all`（回滚无关的 scheduler.rs/watch.rs churn）。

---

### Task 1: 多资源 claim 模型（agent）

**Files:** Modify `crates/agent/src/runtime/thing_agent/arbitration.rs`; tests inline.

**Interfaces:** `ResourceMode`, `ResourceSpec`, `Busy { holder, resource }`, `ResourceLease`, `try_acquire_resources`, `try_acquire` (compat), `set_stop`, `force_release_thing`.

- [ ] Tests: exclusive mutual exclusion; shared capacity; sorted all-or-nothing (A ok/B busy -> A released); busy carries holder+resource; TTL expiry/reacquire; stale Drop token; stop releases all + gate; compat `try_acquire` == `__device__` exclusive.
- [ ] Implement claims as `DashMap<(thing, resource), ClaimEntry>` with holders map/token; sorted specs; RAII release all; keep compat wrapper.
- [ ] Run `cargo test -p agent arbitration`; commit `feat(e4c): 多资源 claim 注册表`.

---

### Task 2: 工具层资源解析 + claim（cloud）

**Files:** `invoke_action.rs`, `device.rs` (SetDeviceTool), `tools/thing/mod.rs` (context), tests.

**Interfaces:** resolve `ResourceSpec` from `ValidatedCall.tool.resources` + device declaration; claim with holder; map Busy to failed ToolResult.

- [ ] Helper `resource_specs(decl: &DeviceDeclaration, tool: &ManifestTool) -> Result<Vec<ResourceSpec>, String>`: ids sorted; unknown id -> error; empty -> `[ResourceSpec { id:"__device__", Exclusive, 1 }]`.
- [ ] `InvokeActionTool`: after validation/approval, claim; hold lease until execute returns; Busy -> `tool_err("busy: ...")` (no infra error).
- [ ] `SetDeviceTool`: same, using its action/tool; read paths untouched.
- [ ] Tests: call busy with holder/resource; second different resource can proceed (shared/capacity case via fake registry); unknown resource rejected; no-resources uses `__device__`; read tools don't claim.
- [ ] Run `cargo test -p tinyiothub-cloud invoke_action` and set tests; commit `feat(e4c): call/set 按声明 claim 资源`.

---

### Task 3: worker 取消令牌 + in-flight + outcome（runtime）

**Files:** `crates/runtime/src/driver/worker.rs`; tests; built-in drivers (simulated/modbus/snmp) cancel checks.

**Interfaces:** `WorkerHandle::{cancel_current() -> bool, in_flight() -> bool}`; `WorkerResult::Cancelled { thing_id, within_grace, elapsed_ms }`.

- [ ] Worker stores `current_token: Arc<Mutex<Option<CancelToken>>>`, `cancel_requested_at: Arc<Mutex<Option<Instant>>>`, `in_flight: Arc<AtomicBool>`.
- [ ] Before each read/execute: set new token + in_flight=true + clear cancel_requested_at; after: in_flight=false, clear token; if token cancelled send `Cancelled` with elapsed from cancel time (≤1s -> within_grace=true).
- [ ] `cancel_current` sets the current token and records now; returns whether in_flight.
- [ ] Built-ins: check `cancel.is_cancelled()` at entry and inside simulated property loop / per command; return `Error::Cancelled`.
- [ ] Tests: cancel long read -> `Cancelled{within_grace:true}`; non-cooperative blocking read -> `within_grace:false` after it returns; cancel when idle returns false/no result.
- [ ] Run `cargo test -p runtime driver::worker`; commit `feat(e4c): worker 取消令牌与 in-flight outcome`.

---

### Task 4: DataServer cancel_calls + CommandCancelled + StopTool 立即返回

**Files:** `crates/runtime/src/data_server.rs`, `crates/core/src/models/event/event_type.rs`, `crates/runtime/src/ports.rs` (if ledger setter), `apps/cloud/.../device.rs` StopTool, wiring/tests.

**Interfaces:** `DataServer::cancel_calls(thing_id) -> usize`; `DataServer::set_hub_ledger(Arc<dyn HubLedger>)`; `ThingEventType::CommandCancelled`; StopTool response.

- [ ] Add `CommandCancelled` variant (serde snake_case wire); update exhaustive matches/serialization.
- [ ] `cancel_calls`: iterate workers, call `cancel_current`, return count; no waiting.
- [ ] `WorkerResult::Cancelled` consumer: publish `CommandCancelled` event with metadata `{outcome:"cancelled"|"cancelled_unknown", elapsedMs}`; append ledger `call_cancelled`/`call_cancelled_unknown` via optional HubLedger; increment metrics.
- [ ] StopTool: add optional `data_server`; execute -> `registry.set_stop` + `data_server.cancel_calls` + immediate `{status:"cancellation_requested", reason, until, inFlight}`; keep no-arbitration error.
- [ ] Wiring: tools context gets data_server (already has in ThingToolContext); HubLedger adapter passed to DataServer in ServiceManager.
- [ ] Tests: stop returns immediately and sets gate; cancel_calls sets worker token; cancelled result -> event + ledger; unknown outcome metadata.
- [ ] Run runtime + cloud invoke/stop tests; commit `feat(e4c): stop 立即返回与异步取消留痕`.

---

### Task 5: composite 每步资源 claim

**Files:** `apps/cloud/src/domains/agent/host/composite.rs`; tests.

**Interfaces:** per-step `resource_specs(decl, tool)` + `try_acquire_resources`; release after step.

- [ ] Resolve each step's tool declaration/resources; default `__device__`; claim all-or-nothing before dispatch; Busy -> step rejected with holder/resource; release at step end.
- [ ] Tests: two steps conflicting -> second busy; shared capacity; failure releases; composite result reports the busy step.
- [ ] Run `cargo test -p tinyiothub-cloud composite`; commit `feat(e4c): composite 步骤资源 claim`.

---

### Task 6: acceptance resources/cancel profiles

**Files:** `crates/runtime/src/acceptance/profiles/{resources.rs,cancel.rs}`, `profiles/mod.rs`, `runner.rs`, `driver-check` tests.

- [ ] `resources`: manifest resource specs valid; exclusive/shared/capacity; busy holder/resource; TTL expiry; default `__device__`.
- [ ] `cancel`: cooperative long task stop -> cancelled ≤1s; non-cooperative -> cancelled_unknown; stop gate rejects subsequent call.
- [ ] Add profiles to `all`; CLI output; tests.
- [ ] Run `cargo test -p runtime acceptance`, `cargo test -p driver-check`; commit `feat(e4c): resources/cancel 验收 profile`.

---

### Task 7: E2E 并发 busy + stop 取消/unknown + 回归

**Files:** runtime/cloud E2E tests; no production code expected.

- [ ] E2E: two concurrent call/set on same device+resource -> second `busy: <holder> holds <resource>`; different shared resource capacity allows; TTL expiry.
- [ ] E2E: long cooperative task -> stop returns immediately -> `CommandCancelled outcome=cancelled` + ledger; non-cooperative -> `cancelled_unknown` after >1s; stop gate rejects next call.
- [ ] Regression: E4b hub stream, E3 live stream, watch/derived, push worker tests.
- [ ] Run full workspace; fix only E4c-caused failures.
- [ ] Commit `test(e4c): 资源/取消 E2E`.

---

### Task 8: 收口验证 + 执行记录

**Files:** `docs/superpowers/plans/2026-10-22-e4c-execution-record.md`.

- [ ] `cargo fmt --all` (revert unrelated churn), `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --lib --bins --tests`, driver-check all, web tests/build.
- [ ] Write execution record: commits, evidence, deviations, carry-forwards (queueing, distributed arbitration, composite fine-grained UX, metrics export).
- [ ] Commit `docs(e4c): 执行记录与验证结果`.

## Self-Review

- Spec coverage: multi-resource model (1), tool claims (2), worker cancel/outcome (3), stop async + ledger/event (4), composite (5), acceptance (6), E2E (7), verification (8).
- Compatibility: `try_acquire` wrapper preserves E1; default `__device__`; read tools no claim.
- Known risk: adding `CommandCancelled` touches exhaustive matches; run full workspace check in Task 4.
