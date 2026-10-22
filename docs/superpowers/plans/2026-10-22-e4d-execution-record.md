# E4d 执行记录与验证结果

- 日期：2026-10-22
- 分支：`ai-consolidation`
- 提交范围：`c2a9f818..HEAD`。实施（Task 1–6）共 **10** 个提交；Task 7 增加 1 个 E2E/加固提交与 1 个本记录提交，`git rev-list --count c2a9f818..HEAD` = **12**。
  - `5699aa4a` feat(e4d): maps/places/坐标持久化
  - `374e01c6` fix(e4d): harden spatial tenant isolation and zone_source contract
  - `709fb53a` feat(e4d): 几何/zone/trust 纯函数
  - `5787dd9d` feat(e4d): ZoneService 与 LiveState zone 权威切换
  - `44bf8630` feat(e4d): 驱动/placement 位置写入与物化
  - `900d500d` fix(e4d): 重绑清理旧 placement 状态 + 位置摄取门
  - `607e629e` feat(e4d): maps/places/position API
  - `7116d561` fix(e4d): maps/places API review hardening
  - `439e920d` feat(e4d): placement 验收 profile
  - `84e3f186` fix(e4d): placement 验收 profile 评审修正
  - `test(e4d): 空间 E2E + 并发 upsert 锁重试`（新增 5 个 E2E；见 §3.7）
  - 本记录提交：`docs(e4d): 执行记录与验证结果`
- 工具链：rustc 1.95.0 / cargo 1.95.0，macOS（darwin）；Node v24.14.0（web）
- 计划：`docs/superpowers/plans/2026-10-22-e4d-spatial-model.md`
- 子 spec：`docs/superpowers/specs/2026-10-22-e4d-spatial-model-design.md`
- 新增 E2E：`apps/cloud/src/tests/e4d_spatial_e2e_tests.rs`（真实 Router + 真实 SQLite 迁移 + 真实 `ZoneService` / `DataServer`）

## 1. 命令与结果

| # | 命令 | 结果 |
|---|---|---|
| 1 | `cargo fmt --all` | 已对 E4d 触及文件格式化；按“不混入无关格式变更”`git checkout` 还原既有漂移 `crates/agent/src/runtime/thing_agent/scheduler.rs`、`crates/core/src/watch.rs`。`cargo fmt --all -- --check` 仍 exit 1：仅上述 2 个文件、24 个 hunk 的存量漂移（见 §3.8） |
| 2 | `cargo clippy --workspace --all-targets -- -D warnings` | 通过（exit 0，最终代码复跑）；仅 `redis v0.25.4` future-incompat 提示，非本次改动 |
| 3 | `cargo test --workspace --lib --bins --tests` | **2440 passed / 0 failed / 4 ignored**（53 个测试二进制，exit 0；并发重试修复后复跑）。E4c 基线 2347；E4d 净增 **93**（实施提交 95 加/7 删 = +88，Task 7 新增 5 个 E2E） |
| 4 | `cargo build -p demo-declarative` | 通过（exit 0） |
| 5 | `cargo test -p driver-check` | 5 passed / 0 failed（exit 0；lib 0 + e2e 5） |
| 6 | `cargo run -p driver-check -- --package drivers/demo-declarative --profile all` | exit 0；`demo-declarative@1.0.0 -> PASS (6 profile(s): core=pass, materialization=pass, migration=pass, resources=pass, placement=pass, cancel=pass)` |
| 7 | `npx tsc --noEmit`（web，Node 24） | 通过（exit 0） |
| 8 | `npx vitest run --config vitest.config.ts`（web，Node 24） | **22 files / 169 tests passed**（exit 0，4.99s） |
| 9 | `npm run build`（web，Node 24） | 通过（exit 0，5.09s）；仅既有 chunk >500 kB 提示 |

未运行 `cargo test --doc`（任务要求为 `--lib --bins --tests`）。

### 1.1 定向回归

| 过滤词 / 范围 | 结果 |
|---|---|
| `e4d_spatial`（新 E2E） | 5 passed / 0 failed |
| `spatial`（maps/places/position API） | 15 passed / 0 failed |
| `runtime --lib live_state`（E1b hierarchy 回退） | 14 passed / 0 failed |
| `runtime --lib hub_stream`（E4b） | 12 passed / 0 failed |
| `runtime --lib maps::`（几何/zone/ZoneService） | 35 passed / 0 failed |
| `runtime --lib placement`（验收 profile + bind 物化） | 14 passed / 0 failed |
| `runtime --lib data_server::tests::`（位置摄取/越界） | 18 passed / 0 failed |
| `db --lib map::`（空间持久化） | 14 passed / 0 failed |
| `tinyiothub-cloud --lib e3_workbench`（E3 工作台） | 2 passed / 0 failed |
| `tinyiothub-cloud --lib watch_e2e`（E1b 唤醒/重启） | 1 passed / 0 failed |
| `tinyiothub-cloud --lib live_stream`（E4b 流式快照面） | 1 passed / 0 failed |
| `tinyiothub-cloud --lib e4c_resources_stop`（E4c 资源/stop） | 10 passed / 0 failed |
| `web workbench-state.test.ts`（E3 工作台状态） | 4 passed / 0 failed |

## 2. 退出清单证据

| 退出项 | 证据 | 结论 |
|---|---|---|
| 创建 maps/places 后设备坐标落区 | E2E `overlapping_zones_resolve_to_smallest_area`：API 建图/两 zone → `PUT /v1/things/{id}/position` → `GET` 回读坐标 → `ZoneService::zone_for` 命中 zone；`maps_mode_resolves_polygon_zone_name`、`zone.point_in_polygon`（边界/顶点含入） | PASS |
| 多 zone 取面积最小 | E2E 同名点 (5,5) 同时落 2500 m² `warehouse` 与 100 m² `rack-a` → `rack-a`；(30,30) → `warehouse`；(90,90) 图内 zone 外 → `None`。单测 `smallest_area_zone_wins`、`smallest_zone_wins_when_polygons_overlap`；profile `zone.smallest_area` | PASS |
| 显式 `placement.zoneRef` 优先 | E2E `explicit_zone_ref_wins_and_unknown_ref_fails_closed`：点在 `zone-a`，物化列指向 `zone-b` → 返回 `zone-b`（点选不生效）；`stored_zone_ref_resolves_without_position`（无坐标也可解析）；profile `zone.explicit_ref` | PASS |
| 未知 zoneRef fail-closed | 同一 E2E：`does-not-exist` → `None`（不回退点选命中的 `zone-a`）；`unknown_explicit_zone_ref_fails_closed`；profile `zone.unknown_ref` | PASS |
| `zone_source=maps` 时 LiveState/read_device zone 来自 maps | E2E `zone_source_switch_and_rollback_keeps_hierarchy_and_snapshot_strings`：默认 hierarchy → `Building A`；切 maps → `map-zone`；`workspace_snapshot`（E4b/E3 读面）同步为 `map-zone`。profile `zone.source_modes` 走真实 `LiveState::derive_zone` | PASS |
| hierarchy 回退仍可用、切换可回滚 | 同一 E2E：maps → hierarchy → `Building A`（摘要与快照）；再切回 maps → `map-zone`。单测 `switching_zone_source_back_to_hierarchy_restores_ancestor_chain`、`zone_source_gate_starts_on_hierarchy_even_with_maps_data`、`hierarchy_and_unknown_sources_normalize_to_hierarchy` | PASS |
| trust：fixed/30s/5min/驱动只降级 | `fixed_placement_is_always_trusted`、`runtime_trust_thresholds_are_inclusive`（0/30/31/300/301s）、`driver_trust_can_only_downgrade`、`lost_trust_yields_none`、`stale_position_ages_to_lost_and_yields_no_zone`、`position_without_timestamp_is_trusted`；profile `trust.fixed/thresholds/driver_downgrade/lost`。E2E 驱动读数 `position_trust=uncertain` 落库为 Uncertain | PASS |
| 驱动 position source 与 PUT position 都更新坐标/trust | E2E `driver_position_ingest_gate_and_out_of_bounds`：真实 `DataServer` value_sink → 消费管线，摄取门开启后写 x/y/trust/updated_at；PUT 路径见 `position_get_put_validation_and_isolation`（API 校正不动物化 `zone_ref` 与摄取门） | PASS |
| 摄取门：runtime+fromSource 才写；fixed 忽略 position_* | 同一 E2E：门关闭时读数不写坐标；开启后写；再次关闭后静态坐标不被漂移。单测 `position_readings_ignored_when_ingest_disabled`；E4a/E4d bind 测试 `bind_rejects_invalid_or_unwired_placement`、`bind_invalidates_zone_cache_after_materialization`、`materialize_ingest` 相关断言 | PASS |
| 越界：保留坐标、zone=None + warn | E2E 读数 x=200 落库（坐标保留）但 `zone_for=None`；单测 `out_of_bounds_driver_position_is_kept_but_zone_is_none`、`out_of_bounds_position_yields_no_zone`、`bounds_check_uses_stored_map_not_default`；profile `zone.out_of_bounds` | PASS |
| 多租户 map/place/position 隔离 | E2E `maps_places_positions_and_zones_are_workspace_isolated`：ws-b 读/改 ws-a 图/地点/坐标均 404；`GET /v1/maps` 为空；ws-a 物的坐标/显式引用在 ws-b 作用域解析为 `None`。API 测试 `maps_and_places_are_tenant_isolated`、`position_get_put_validation_and_isolation`；db `map::` 套件 | PASS |
| placement 验收 profile 通过 | `cargo run -p driver-check … --profile all` → `placement=pass`（6 profiles 全 pass）；runtime `acceptance::profiles::placement` 单测；demo 无地图声明时 map 检查 Skip 而非 Fail（`demo_placement_profile_skips_map_checks_without_failing`） | PASS |
| API 校验/错误矩阵 | `spatial_api_tests` 15 个用例：bounds 溢出/非正尺寸、zone <3 点/共线/重复点、landmark 点形、默认图唯一、锚图同工作区、admin 403、跨工作区 404、越界 400、并发重名 201+409 | PASS |
| E3 World zone 名称投影不回归 | E3 工作台云测试 2 passed；web `workbench-state` 4 passed；E2E 断言 `workspace_snapshot.devices[].zone` 在 maps/hierarchy 下分别为 `map-zone` / `Building A`（World 直接消费该字符串） | PASS |
| E1b / E4b / E4c 不回归 | 定向回归表（§1.1）全部 0 failed；全量 2440 passed / 0 failed | PASS |

## 3. 偏差与说明

1. **E3 World 地图渲染不在本期**：World 继续消费 LiveState 快照的 `zone` 字符串（祖先链或 maps zone 名），不渲染多边形/地图帧；本次 E2E 只保证字符串语义在两种权威模式下都正确。地图/多边形编辑 UI 另立。
2. **不做地球锚定/帧旋转**：`maps.bounds` 是工作区本地 2D 米制平面；`anchor_map_id` 只做父子图引用与同工作区校验，不做坐标换算（spec §5 非目标）。
3. **位置缓存/批量调优有限**：`ZoneService` 对 maps/places 与坐标各 5s TTL，`warm_workspace` 在快照路径批量读坐标/显式引用，避免逐物 N 次查询；`position_ingest` 门也缓存 5s（重绑时经 `ZoneService::set_position_ingest` / `invalidate` 写穿）。`ingests` map 无上界，但条目数受“上报位置字段的物数”约束。事件热路径仍按物调用 `zone_for`（命中缓存），未做进一步批量/异步化。
4. **新工作区默认 `zone_source='maps'` 只覆盖 `create_workspace`**：其他创建路径（导入/模板/场景等直接 `INSERT workspaces` 的地方）走迁移默认 `hierarchy`。本次未逐一接线；迁移期语义安全（hierarchy 回退），列为结转。
5. **`anchor_map_id` 所有权校验在 API 层，不在 DB 层**：`validate_anchor` 拒绝跨工作区/自引用；DB `upsert_map` 仅靠 FK 校验存在性，直接调用 DB 的路径仍可锚到别的 workspace 的图。当前所有写入口均经 API，列为结转。
6. **`position_ingest` 无回填/启动重绑**：存量 `things` 行由迁移得到 `position_ingest=0`；`rebind_bound_devices` 只在驱动安装/升级时执行，重启不会恢复旧 runtime 设备的摄取。当前无内置驱动上报 `position_*`，功能影响为零；列结转。
7. **并发同名地图 500（已修复）**：Task 5 的 `concurrent_duplicate_map_create_returns_conflict_not_500` 在隔离运行时稳定复现 `(code: 6) database is deadlocked`（内存共享缓存 SQLite 的 `SQLITE_LOCKED`，`busy_timeout` 不覆盖表锁），HTTP 产生 500。修复：`Db::upsert_map` 把事务体拆为可重放函数，遇 `SQLITE_BUSY(5)`/`SQLITE_LOCKED(6)` 最多重试 8 次（5ms×n 退避）；重试后第二个请求走 UNIQUE → 409。隔离复跑 6/6 passed，全量套件 0 failed。`upsert_place` 未改（无并发用例），同类锁风险列为结转。
8. **fmt 基线漂移**：`crates/agent/src/runtime/thing_agent/scheduler.rs` 与 `crates/core/src/watch.rs` 仍不满足 `cargo fmt --all -- --check`（24 hunks）；按“不混入无关格式变更”保留原样，建议单独 `style:` 提交。
9. **demo 清单没有真实 `kind=position` source**：Task 6 评审修正后移除了悬空的 `fromSource: "position"`，demo 保持 PASS；驱动位置摄取由 E2E 合成读数 + `data_server` 单测覆盖，尚未有出厂驱动实际产生 `position_*`。
10. **MSRV 声明滞后**：workspace `rust-version = 1.85`，但 E4d 使用的 let-chains 需要 1.88+；当前工具链 1.95 构建通过，声明值未更新（Task 1 既有 residual）。
11. **`MapStore` 端口形状**：brief 列出的写方法最终并入 `MapStore`（读+写+位置+摄取门+`zone_source`），未拆出独立 `ZoneSource`；runtime 仍零 db/sqlx，cloud 经 `DbMapStore` 适配。属已记录的实现取舍。

## 4. 结转

| # | 事项 | 来源 | 归属 |
|---|---|---|---|
| 1 | E3 World 2D 地图/多边形渲染与地图编辑 UI | spec §5；本文 §3.1 | 另立（E4e/后续） |
| 2 | 地球锚定/帧旋转/坐标系换算 | spec §5；本文 §3.2 | 未排期 |
| 3 | 位置热路径缓存/批量与 `ingests` 上界、快照 120ms 预算复测 | Task 3/4 评审；本文 §3.3 | E4e 性能 |
| 4 | 非 `create_workspace` 路径的新工作区 `zone_source='maps'` | Task 1 residual；本文 §3.4 | E4e |
| 5 | DB 层 `anchor_map_id` 同工作区校验 | Task 1 fix-review residual；本文 §3.5 | E4d 后续 |
| 6 | `position_ingest` 回填/启动重绑，使存量 runtime 设备重启后恢复摄取 | Task 4 fix-review；本文 §3.6 | E4e 迁移 |
| 7 | `upsert_place` 的并发锁重试（与 `upsert_map` 同类） | 本文 §3.7 | E4d 后续 |
| 8 | 内存共享缓存测试夹具的 `SQLITE_LOCKED` 根因（文件库/单连接夹具） | Task 5 minor；本文 §3.7 | 测试基建 |
| 9 | fmt 基线漂移（scheduler.rs / watch.rs） | Task 1/5；本文 §3.8 | 单独 style 提交 |
| 10 | 出厂 `kind=position` 驱动 source | Task 4/6；本文 §3.9 | E4e |
| 11 | workspace MSRV 声明 1.85 → 1.88+ | Task 1 residual；本文 §3.10 | 仓库维护 |
| 12 | 多 zone 同面积 tie-break 的 ID 次序契约测试 | Task 1 residual | E4d 后续 |
