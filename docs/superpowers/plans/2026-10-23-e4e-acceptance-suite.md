# E4e 全套验收 + 迁移计划 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 补齐 lifecycle/telemetry/tools/streaming 自动验收 profile，用真实包 + 内存 Hub 跑通 demo-declarative 全套；交付 `driver-check inventory` 与迁移计划。

**Architecture:** TestHub 加载包 `.so` 到真实 DriverRegistry，以内存在 ThingCache/EventBus/DemandTracker/DataServer/worker 构建隔离环境；profiles 只观察 Hub 公开行为；runner/CLI 增加 profile 与 inventory；manual/--interactive 明确不做。

**Tech Stack:** Rust 2024、tokio、现有 acceptance/worker/sampler/registry、driver-check CLI。

## Global Constraints

- 上位 spec：`docs/superpowers/specs/2026-10-23-e4e-acceptance-suite-design.md`；偏差先改 spec。
- runtime 零 db/sqlx；TestHub 全内存。
- 真实包 `.so` 优先；v1 拒载；测试结束卸载驱动 registry 条目。
- 不做 manual/`--interactive`；五级枚举保留。
- skip 必须带理由；demo 全套不得有 fail。
- 每次提交前 `cargo fmt --all`（回滚无关 scheduler.rs/watch.rs churn）。

---

### Task 1: TestHub + lifecycle profile

**Files:** `crates/runtime/src/acceptance/test_hub.rs` (extend), `profiles/lifecycle.rs`, `profiles/mod.rs`, tests.

**Interfaces:** `TestHub::{new, load_package(dir), spawn_worker(thing), read_once, stop_worker, status}`, in-memory ThingCache/EventBus; `lifecycle::run(dir) -> ProfileReport`.

- [ ] Load package `.so` via `DriverRegistry::load_named` (unique test name), create a Thing with level/properties, spawn worker, read; assert online/status.
- [ ] Stop worker -> offline; respawn -> online; failed read doesn't panic.
- [ ] Unload driver after tests.
- [ ] Commit `feat(e4e): TestHub + lifecycle profile`.

---

### Task 2: telemetry profile

**Files:** `profiles/telemetry.rs`, tests.

- [ ] Read once via TestHub; required state fields present (demo level); type/range check.
- [ ] Interval: cadence is sampler-owned (the driver only reads on `submit_read`). Covered by `sampler::always_source_samples_every_interval` / `interval_is_clamped_to_min_interval`; `telemetry.interval` records a reasoned skip (spec §2, 2026-10-23 amendment). No self-sleeping measurement.
- [ ] Push driver: if manifest has push source, use ValueSink; else Skip with reason.
- [ ] Commit `feat(e4e): telemetry profile`.

---

### Task 3: tools profile

**Files:** `profiles/tools.rs`, tests.

- [ ] Resolve declared safe tools from manifest; demo `echo` valid params -> success; invalid params -> rejection with range/path.
- [ ] Undeclared tool -> rejection.
- [ ] control/dangerous tools -> Skip (not auto-called).
- [ ] Commit `feat(e4e): tools profile`.

---

### Task 4: streaming profile + demo demandOnly source

**Files:** `drivers/demo-declarative/driver.manifest.json`, `src/lib.rs` (read returns level2), `profiles/streaming.rs`, tests.

- [ ] Add `level2` state field and `slow` source (`kind=poll`, `demandOnly=true`, `intervalMs`, `minIntervalMs`); health telemetry remains.
- [ ] Driver read returns both level and level2.
- [ ] Profile: no demand -> slow not submitted/read; subscription protects -> slow read; drop + 30s grace -> stop; telemetry/health continues (deterministic fake-worker tick).
- [ ] `streaming.driver_source`: TestHub loads the real `.so`, spawns a real worker, reads only `slow`, and asserts `level2` is returned (real-driver evidence; push workers skip).
- [ ] Update E4a acceptance counts/tests if needed.
- [ ] Commit `feat(e4e): streaming profile + demo demandOnly source`.

---

### Task 5: runner/CLI profiles + inventory

**Files:** `crates/runtime/src/acceptance/runner.rs`, `crates/driver-check/src/main.rs`, tests.

- [ ] Add lifecycle/telemetry/tools/streaming to PROFILE_NAMES/all/named/help.
- [ ] `driver-check inventory --data-dir <dir>`: list built-in manifests, installed packages (driver/version/manifest hash/binary/none), v1 `.so` detection (reject list), package dirs.
- [ ] Tests: all profile order, named selection, inventory output.
- [ ] Commit `feat(e4e): 验收 profile 注册与 inventory`.

---

### Task 6: 迁移计划 + 全套 E2E + 执行记录

**Files:** `docs/superpowers/plans/2026-10-23-e4e-migration-plan.md`, `docs/superpowers/plans/2026-10-23-e4e-execution-record.md`, `crates/driver-check/tests/e2e.rs`.

- [ ] Migration plan: inventory (built-ins/drivers/*/external .so), v2 hard cut, data/zone/position_ingest migration, maintenance-window runbook, rollback, per-package acceptance gates.
- [ ] E2E: build demo `.so`; `driver-check --profile all` passes all 10 auto profiles; inventory output includes demo.
- [ ] Full workspace/clippy/web/driver-check; execution record with evidence/deviations.
- [ ] Commit `docs(e4e): 迁移计划 + 执行记录`.

## Self-Review

- Spec coverage: TestHub/lifecycle (1), telemetry (2), tools (3), streaming+demo (4), runner/inventory (5), migration plan/E2E (6).
- Manual/interactive explicitly excluded.
- Known risk: global DriverRegistry test pollution; unload and unique names.
