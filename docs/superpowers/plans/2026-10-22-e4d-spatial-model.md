# E4d 空间模型 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 建立工作区本地 maps/places/zone 空间模型，设备坐标/trust 可写入，zone 由多边形权威派生（hierarchy 回退可切换），并交付 placement 验收。

**Architecture:** DB additive maps/places/坐标列/zone_source；runtime 几何与 ZoneService 纯逻辑（MapStore port）；LiveState zone 摘要按 flag 切换 maps/hierarchy；驱动 position source 与 PUT API 写坐标；cloud 提供 admin CRUD；World 渲染不在本期。

**Tech Stack:** Rust 2024、sqlx（仅 db）、axum、serde、geo 计算（手写 point-in-polygon，不引入依赖）。

## Global Constraints

- 上位 spec：`docs/superpowers/specs/2026-10-22-e4d-spatial-model-design.md`；偏差先改 spec。
- runtime 零 db/sqlx；maps/places 持久化经 `MapStore` port。
- maps 为 zone 权威（裁定 #3）；`zone_source` 切换可回滚；hierarchy 回退保留。
- 坐标：x/y 米、level int、heading 可选；zone 多边形≥3，边界含入，最小面积。
- trust：fixed trusted；runtime 30s/5min 阈值；driver trust 只降级。
- 多租户隔离；maps/places/position 管理 admin。
- E3 World 渲染不在本期；不得破坏现有 hierarchy zone 投影。
- 每次提交前 `cargo fmt --all`（回滚无关 scheduler.rs/watch.rs churn）。

---

### Task 1: DB 迁移 + MapStore/Db 实现

**Files:** `crates/db/migrations/20261022000001_spatial.sql`, `crates/db/src/map.rs`, `crates/db/src/lib.rs`, `crates/runtime/src/ports.rs`; tests.

**Interfaces:** `MapRecord`, `PlaceRecord`, `ThingPosition`; `MapStore::{list_maps,upsert_map,delete_map,list_places,upsert_place,delete_place,get_position,set_position}`; Db methods.

- [ ] Migration: maps/places tables + indexes; things 坐标列（additive）；workspaces.zone_source default hierarchy.
- [ ] DB methods with workspace scoping; default-map uniqueness app-level; geometry JSON round-trip.
- [ ] Tests: CRUD, unique name, default map, position columns, zone_source.
- [ ] Commit `feat(e4d): maps/places/坐标持久化`.

---

### Task 2: 几何/zone/trust 纯函数

**Files:** `crates/runtime/src/maps/{mod.rs,model.rs,zone.rs}`; tests.

**Interfaces:** `MapBounds`, `PlaceKind`, `Geometry`, `Map`, `Place`, `LocationTrust`; `point_in_polygon`, `polygon_area`, `resolve_zone_in(places, point)`, `derive_trust`.

- [ ] Tests: boundary inclusive; inside/outside; smallest-area selection; explicit zoneRef precedence helper; trust thresholds/fixed/driver downgrade.
- [ ] Implement pure functions; no db.
- [ ] Commit `feat(e4d): 几何/zone/trust 纯函数`.

---

### Task 3: ZoneService + LiveState 切换

**Files:** `crates/runtime/src/maps/service.rs`, `crates/runtime/src/watch/live_state.rs`, `crates/runtime/src/ports.rs`; tests.

**Interfaces:** `ZoneService::new(Arc<dyn MapStore>)`; `zone_for(ws, thing, now) -> Option<String>`; `zone_source(ws)`; LiveState summary uses it.

- [ ] ZoneService caches maps/places per workspace with invalidation on CRUD (or short TTL); resolves map via thing.map_id/default; explicit zoneRef.
- [ ] LiveState zone summary: `zone_source=maps` → ZoneService; hierarchy → existing parent derivation; flag read via MapStore.
- [ ] Tests: maps mode uses polygon names; hierarchy fallback; no maps → None; switch back restores hierarchy.
- [ ] Commit `feat(e4d): ZoneService 与 LiveState zone 权威切换`.

---

### Task 4: 位置输入 + placement 物化

**Files:** `crates/runtime/src/data_server.rs`, `crates/runtime/src/access/service.rs`, `apps/cloud` thing create/bind wiring; tests.

**Interfaces:** reserved reading fields `position_x/position_y/position_level/position_heading/position_trust`; `set_position`; materialize fixed/declared placement.

- [ ] DataServer reading path: write reserved fields to thing columns + updated_at; out-of-bounds → keep coords, zone None + warn.
- [ ] AccessService bind_thing: materialize `placement.mode=fixed|declared` position/mapRef/zoneRef; missing map/zone → bind error.
- [ ] Tests: driver position reading; fixed placement; invalid refs; trust age.
- [ ] Commit `feat(e4d): 驱动/placement 位置写入与物化`.

---

### Task 5: maps/places/position APIs

**Files:** `apps/cloud/src/domains/map/{mod.rs,handler.rs}`, `apps/cloud/src/domains/thing/handler/{mod.rs,position.rs}`, route wiring; tests.

- [ ] `/v1/maps` CRUD, `/v1/maps/{id}/places` CRUD, `GET/PUT /v1/things/{id}/position`; admin gate (driver install pattern), workspace scope.
- [ ] Validation: bounds, polygon≥3, name unique, default map uniqueness, position in bounds; 400/403/404/409.
- [ ] Tests: CRUD, tenant isolation, admin 403, invalid geometry.
- [ ] Commit `feat(e4d): maps/places/position API`.

---

### Task 6: placement 验收 profile

**Files:** `crates/runtime/src/acceptance/profiles/placement.rs`, `profiles/mod.rs`, `runner.rs`, driver-check tests.

- [ ] Manifest placement refs/maps validation; point-in-polygon/min-area; trust thresholds; out-of-bounds warn; zone_source behavior.
- [ ] Add to `all`/CLI; tests; demo passes (skip/warn if no maps declared).
- [ ] Commit `feat(e4d): placement 验收 profile`.

---

### Task 7: E2E + 回归 + 执行记录

**Files:** tests + `docs/superpowers/plans/2026-10-22-e4d-execution-record.md`.

- [ ] E2E: create map/places/device position → zone; multiple zones smallest; explicit zoneRef; zone_source switch/rollback; cross-tenant isolation.
- [ ] Regression: E1b zone/hierarchy tests, E3 World/workbench (zone names), E4b hub stream zone summary, E4c.
- [ ] Full workspace/clippy/driver-check/web; execution record with evidence/deviations/carry-forwards (World rendering, earth anchor, position source for dynamic drivers, cache invalidation).
- [ ] Commit `docs(e4d): 执行记录与验证结果`.

## Self-Review

- Spec coverage: persistence (1), geometry/trust (2), zone authority/fallback (3), inputs/materialization (4), APIs (5), acceptance (6), E2E/verification (7). World rendering/earth anchor excluded.
- Compatibility: zone_source default hierarchy; E3 World reads zone strings unchanged.
- Known risk: LiveState zone service needs MapStore injection; ensure runtime stays db-free.
