# Task 3 报告：ZoneService + LiveState zone 权威切换

- 状态：完成
- Commit：`feat(e4d): ZoneService 与 LiveState zone 权威切换`（见 git log）
- 文件：
  - `crates/runtime/src/maps/service.rs`（新增 `ZoneService` + 缓存 + 11 个单测）
  - `crates/runtime/src/maps/mod.rs`（导出）
  - `crates/runtime/src/watch/live_state.rs`（zone_source 切换 + 4 个切换测试）
  - `crates/runtime/src/watch/engine.rs`（summary 链路 async 化）
  - `crates/agent/src/runtime/thing_agent/{watch_loop.rs,manager.rs}`（`WatchDeps.zones` 注入）
  - `apps/cloud/src/shared/runtime_ports.rs`（`DbMapStore`）
  - `apps/cloud/src/shared/service_manager.rs`（bootstrap 构造并注入）
  - `apps/cloud/src/tests/watch_e2e_tests.rs`（`zones: None`，保持 hierarchy）

## 实现

- `ZoneService::new(Arc<dyn MapStore>, TrustThresholds)` / `with_ttl(...)`：
  - `zone_source(ws)`：`hierarchy|maps`，未知/缺失归一化 hierarchy；按工作区缓存 5s。
  - `zone_for(ws, thing, explicit_zone, now)`：位置缺失或 `trust=Lost` → None；map = `position.map_id`（未知图 fail-closed）或默认图；places 经 `resolve_zone_for`；显式 zoneRef 未知 → None（不回退点选/层级）。
  - maps/places 懒加载 + 按工作区 TTL 缓存（默认 5s；`Duration::ZERO` 关闭，测试用）。runtime 仍零 db，只经 `MapStore`。
- LiveState：新增 `zones: Option<Arc<ZoneService>>`（`with_zone_service`）。`zone_source=maps` → ZoneService（None → 空串，**不回退** hierarchy）；`hierarchy`/未注入 → 原 parent_id 祖先链；MapStore 读失败 warn + 降级 hierarchy（可用性优先）。
- `summary` / `workspace_snapshot` / `WatchEngine::upsert_rule` 改为 async（ZoneService 端口为 async）；调用方（watch_loop 命令循环、事件循环、测试）已 await。`ThingLiveSummary.zone` 形状不变。
- 显式 `placement.zoneRef` 暂传 `None`：`Thing` 尚无该字段，待 Task 4 物化后接入。

## 测试

- `cargo test -p runtime --lib maps`：36 passed（含 11 个 ZoneService 新测）。
- `cargo test -p runtime --lib watch`：24 passed（含 4 个 map/hierarchy 切换新测；E1b hierarchy 用例保持通过）。
- `cargo test -p agent --lib thing_agent`：158 passed。
- `cargo test -p tinyiothub-cloud workbench`：2 passed（E3 World 快照不回归）。
- `cargo clippy -p runtime -p agent -p tinyiothub-cloud --all-targets`：无警告。
- `cargo fmt --all`：已执行；`crates/agent/.../scheduler.rs` 与 `crates/core/src/watch.rs` 的既有 fmt churn 已还原。

覆盖：maps 多边形命名、多 zone 最小面积、显式 zoneRef 优先/未知 fail-closed、trust Lost、无位置/无默认图、position.map_id 优先级与未知图、TTL 复用与零 TTL 回源、maps→hierarchy 切换恢复祖先链、未注入 ZoneService 降级、快照 zone。

## 关注点

- trust 直接消费 `things.position_trust`（Task 4 物化的生效 trust）；`now`/`TrustThresholds` 已预留但本任务不做 age 派生（fixed 恒 trusted 需 placement mode，留到 Task 4）。
- 每次 summary 都会多一次 `get_position` 查询（maps 模式）；maps/places 有 5s TTL，快照 N 设备为 N 次 position 查询。如需批量化可在 Task 5/7 评估。
- CRUD 主动失效未接线（Task 5），当前依赖 TTL 5s 窗口。
- `summary`/`workspace_snapshot`/`upsert_rule` 从 sync 变 async，属内部 API 变更；仓库内调用点已全部更新，无外部 crate 直接调用。
