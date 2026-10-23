# E4e 执行记录与验证结果

- 日期：2026-10-23
- 分支：`ai-consolidation`
- 提交范围：`c487d801..HEAD`，共 **12** 个提交（E4e 全部）：
  - `7ab8c742` docs(e4e): 全套自动验收 + 迁移计划子 spec 与实施计划
  - `104d1411` feat(e4e): TestHub + lifecycle profile
  - `40406d0e` fix(e4e): harden TestHub drop and lifecycle evidence
  - `fc8bbd83` feat(e4e): telemetry profile
  - `5cfdd584` fix(e4e): harden telemetry checks against false passes
  - `a321d3b4` feat(e4e): tools profile
  - `ed00d469` fix(e4e): harden tools profile selection and rejection paths
  - `caa46fc0` feat(e4e): streaming profile + demo demandOnly source
  - `5fabf88c` feat(e4e): 验收 profile 注册与 inventory
  - `6bb4bc82` fix(e4e): inventory 拒绝无效 data-dir，内置清单错误与路径身份显式上报
  - `7ebf22c0` test(e4e): demo inventory-v2 与 CLI 全套 E2E
  - 本记录提交：`docs(e4e): 迁移计划 + 执行记录`
- 工具链：rustc 1.95.0 / cargo 1.95.0，macOS（darwin）；Node v24.14.0（web）
- 计划：`docs/superpowers/plans/2026-10-23-e4e-acceptance-suite.md`
- 子 spec：`docs/superpowers/specs/2026-10-23-e4e-acceptance-suite-design.md`
- 迁移计划：`docs/superpowers/plans/2026-10-23-e4e-migration-plan.md`
- 判定：**自动验收全部通过；E4e 收口。迁移执行、manual/交互按裁定不在本期。**

## 1. 命令与结果

| # | 命令 | 结果 |
|---|---|---|
| 1 | `cargo fmt --all` | 已格式化 E4e 触及文件；按「不混入无关格式变更」`git checkout` 还原存量漂移 `crates/agent/src/runtime/thing_agent/scheduler.rs`、`crates/core/src/watch.rs` |
| 2 | `cargo fmt --all -- --check` | exit 1；**仅**上述 2 个文件（scheduler.rs 5 个 hunk、watch.rs 19 个 hunk）的存量漂移，与 E4a/E4d 记录一致，非本期引入 |
| 3 | `cargo clippy --workspace --all-targets -- -D warnings` | 通过（exit 0，1m14s）；仅 `redis v0.25.4` future-incompat 提示，非本次改动 |
| 4 | `cargo test --workspace --lib --bins --tests` | **2507 passed / 0 failed / 4 ignored**（54 个测试二进制，exit 0）。E4d 基线 2447；E4e 净增 **60** |
| 5 | `cargo build -p demo-declarative` | 通过（exit 0） |
| 6 | `cargo test -p driver-check` | **22 passed / 0 failed**（lib 9 + e2e 7 + inventory 6，exit 0） |
| 7 | `cargo run -p driver-check -- --package drivers/demo-declarative --profile all` | exit 0；`demo-declarative@1.0.0 -> PASS (10 profile(s): core=pass, materialization=pass, migration=pass, lifecycle=pass, telemetry=pass, tools=pass, streaming=pass, resources=pass, cancel=pass, placement=pass)` |
| 8 | `cargo run -p driver-check -- inventory --data-dir <临时空目录> --strict` | exit 0；`3 builtin manifest(s) (errors=0), 0 installed package(s) ... -> OK` |
| 9 | `cargo run -p driver-check -- inventory --data-dir data` | exit 0；`3 builtin manifest(s) (errors=0), 0 installed package(s) ... -> OK` |
| 10 | `npx tsc --noEmit`（web，Node 24） | 通过（exit 0，无输出） |
| 11 | `npx vitest run --config vitest.config.ts`（web，Node 24） | **22 files / 169 tests passed**（exit 0，6.51s） |
| 12 | `npm run build`（web，Node 24） | 通过（exit 0，6.14s）；仅既有 chunk >500 kB（`three-*.js` 732.69 kB）提示 |

未运行 `cargo test --doc`（任务要求为 `--lib --bins --tests`）。

### 1.1 新增/修改测试

`crates/driver-check/tests/e2e.rs`（7 个测试）：

| 测试 | 覆盖 |
|---|---|
| `demo_declarative_passes_all_profiles` | 进程内 `all`：10 profiles 顺序 + 全通过 |
| `every_named_profile_runs_alone` | 每个具名 profile 单独运行无 fail |
| `demo_placement_profile_skips_map_checks_without_failing` | demo 无地图声明时 map 检查 Skip、结构检查 Pass |
| `cli_all_profile_runs_ten_profiles_and_exits_zero` | **新增**：CLI `--profile all` exit 0；JSON 10 profiles、顺序正确、无 fail；summary `PASS` |
| `inventory_lists_installed_demo_package_as_v2` | **新增**：构建 demo `.so` → 复制 manifest+二进制到 `<data-dir>/drivers/workspaces/ws-e4e/demo-declarative/1.0.0/` → `inventory` 报告 `abi=v2`、两个 hash 64 位、无 error；默认与 `--strict` 均 exit 0、`-> OK` |
| `demo_declarative_binary_passes_core_when_built` | 真实二进制 `binary.abi_v2` Pass |
| `cli_help_lists_every_profile` | `--help` 列出全部 profile |

`crates/driver-check/src/inventory.rs` 与 `tests/inventory.rs`（9 + 6）为 Task 5 交付，覆盖内置清单、安装包、v1/无效/不可读分类、`--strict`、无效 data-dir。

## 2. 退出清单证据

| 退出项 | 证据 | 结论 |
|---|---|---|
| demo-declarative 通过全部自动 profile | 命令 #7：10/10 profile `pass`，exit 0；e2e `demo_declarative_passes_all_profiles` + `cli_all_profile_runs_ten_profiles_and_exits_zero` | PASS |
| 无 manual/交互项 | `PROFILE_NAMES` 仅含 10 个自动 profile；无 `--interactive`、无 manual profile；`Level::Manual` 枚举保留用于历史报告但不会产生 | PASS（按裁定） |
| lifecycle/telemetry/tools/streaming 补齐 | runtime `acceptance::profiles::{lifecycle,telemetry,tools,streaming}` 单元测试在 #4 全量中通过；CLI 输出四者 `pass` | PASS |
| `driver-check inventory` 盘点包与 v1 产物 | 命令 #8/#9；单测 `missing_v2_symbol_is_classified_as_v1_unsupported`、`non_driver_binaries_are_not_reported_as_v2`、`inventory_reports_non_driver_binaries_and_strict_turns_them_into_exit_1`；E2E v2 happy path | PASS |
| 迁移计划覆盖内置/`drivers/*`/外部 `.so`/数据/空间/回滚/runbook | `docs/superpowers/plans/2026-10-23-e4e-migration-plan.md` §1–§8 | PASS |
| 全量 workspace/clippy/web 回归 | 命令 #3/#4/#10/#11/#12 全部 exit 0，0 failed | PASS |
| 迁移执行本身不在 E4e | 计划 §8 显式声明；`drivers/*` 未改源码/未入 workspace；无外部 `.so` 重建/切换 | PASS（按裁定） |

## 3. 偏差与说明

1. **不做 manual/`--interactive`**：按 E4e 子 spec §0 裁定 #2，显式覆盖上位 spec §4.2 的 AGH manual 项；所有验收均为自动检查，`--no-hardware` 未实现。`Level::Manual` 仍保留在枚举中，只为兼容历史报告，runner 不产生该级别。
2. **迁移计划与执行分离**：E4e 只交计划 + inventory 工具；`drivers/*` 仍是桩/注释（4 个 `driver_init`、3 个仅注释），未纳入 workspace；外部 v1 `.so` 未重建；维护窗口切换/数据回填未执行（计划 §8）。
3. **fmt 基线漂移仍在**：`scheduler.rs`/`watch.rs` 不符合 `cargo fmt --all -- --check`（24 hunks），按「surgical changes」回滚，建议单独 `style:` 提交清偿（见 §4）。
4. **E2E 前置构建**：`inventory_lists_installed_demo_package_as_v2` 与 `demo_declarative_binary_passes_core_when_built` 需要 `cargo build -p demo-declarative` 先完成；缺失时直接失败并提示构建命令，不静默跳过。全量 `cargo test --workspace` 会先构建 workspace 内的 demo cdylib，但 driver-check 集成测试不保证顺序，故验证按 #5→#6 执行。
5. **inventory 的 v1 判定依赖可加载**：v1 `.so` 若完全无法加载会归类 `invalid`（仍被 `--strict` 拒绝）；只有「可加载但缺少 `tinyiothub_driver_vtable_v2`」才归类 `v1/unsupported`。两者都进入拒绝清单，迁移计划 §1.3/§7.1 已说明。final review 后，v1 manifest（`abiVersion:1`）不再让整包退化为 `unreadable`：manifest 校验失败路径也独立定位/哈希/分类二进制（§3 #11）。
6. **`inventory` 只扫规范目录**：`<data-dir>/drivers/workspaces/<ws>/<driver>/<version>/`；非规范路径的 `.so` 与「DB 有 active 行但目录缺失」不在静态盘点内，迁移计划 §7.1 要求全盘扫描 + `driver_declarations` 对账。
7. **测试计数基线**：E4d 记录 2447 passed（4 ignored）；本期 2507 passed（4 ignored），净增 60，与 Task 1–5 的 profile/TestHub/runner/inventory 测试 + Task 6 的 2 个 E2E 相符。
8. **inventory 空目录可 OK**：命令 #8 对空临时目录 `--strict` exit 0 是预期（无 v1 残留 = 无问题）；但 data-dir 不存在/不是目录/不可读会 exit 2，避免空盘点假通过（Task 5 加固）。
9. **streaming 的真实驱动证据（final review 修正）**：demand 门控仍由确定性 `Sampler` + fake worker 契约测试覆盖（无 sleep）；新增 `streaming.driver_source` 用 TestHub 加载真实 `.so`、spawn 真实 worker、只读 `demandOnly` 源并校验声明字段。spec §0 #1/§2 已同步说明。
10. **`telemetry.interval` 是显式 skip（final review 修正）**：cadence 归 sampler 所有，驱动仅在 `submit_read` 时读取，profile 自身两次 read 的时间差不能证明 interval。spec §2 与 plan Task 2 已正式把 cadence 归属写到 `sampler::always_source_samples_every_interval` / `interval_is_clamped_to_min_interval`，skip 带理由，不再是无说明偏差。
11. **inventory v1 分类独立于 manifest 校验（final review 修正）**：`read_package` 因 `abiVersion:1` 失败时仍 `find_binary` + 分类/哈希；真实 v1 包（v1 manifest + v1 `.so`）报 `v1/unsupported` 并保留 manifest 错误为次级证据，新增真实 cdylib 回归测试。
12. **Task 6/ledger 状态**：Task 6（迁移计划 + 全套 E2E + 执行记录）已完成；`.superpowers/sdd/2026-10-23-e4e-acceptance-suite/progress.md` 已补 Task 6 与 Task 1/2/4 residual 处置。
13. **同批加固（final review cheap hardenings）**：`worker.read_failure` Pass 要求 `alive`；TestHub 拒绝与内置驱动同名的包；`parse_builtin` 执行 `validate_manifest`；streaming 常开源按 sampled kind 过滤、无常开源时 quiet/stop 记 skip；E2E 断言必需 profile 为 `Pass` 而非仅「无 fail」。

## 4. 结转（E4e 之后）

| # | 事项 | 来源 | 归属 |
|---|---|---|---|
| 1 | `drivers/*` 7 包迁移执行：纳入 workspace/CI、补 manifest、v2 导出、逐包验收（顺序 mqtt → modbus/bacnet/opcua/snmp → onvif → mqtt-bridge） | 迁移计划 §3 | 独立执行阶段 |
| 2 | 外部 v1 `.so` 盘点→v2 重建→staging 验收→维护窗口原子替换→回滚演练 | 迁移计划 §4 | 独立执行阶段 |
| 3 | 数据迁移执行：全量重绑重物化 + drift 报告、模板/场景能力回写、`zone_source` 切换管理入口、`position_ingest` 回填与启动重绑 | 迁移计划 §5；E4d 结转 #4/#6 | 独立执行阶段 |
| 4 | `drivers/modbus` 与内置 `modbus_rtu` 命名区分；`examples/bacnet-driver`（引用不存在的 `sdks/driver-sdk`）与 `examples/example-plugin`（旧 `iot_edge_driver_sdk`）删除或重建 | 迁移计划 §1.2/§3.2 | 执行阶段 |
| 5 | fmt 存量漂移（`scheduler.rs` / `watch.rs`）单独 `style:` 提交 | 本文 §3 第 3 条；E4a/E4d 结转 | 仓库维护 |
| 6 | workspace `rust-version = 1.85` 与 let-chains（1.88+）不符；CI 固定 1.95 | 迁移计划 §7.4；E4d 结转 #11 | 仓库维护 |
| 7 | UI 直连 `POST /v1/things/{id}/actions/{name}/invoke` 仍绕过 `AccessService` 声明校验 | E4a 记录 §3.6（原定 E4e/final） | 后续 |
| 8 | 场景/MCP 建物的声明物化回填（模板路径） | E4a 记录 §3.5 | 后续 |
| 9 | E3 World 2D 地图渲染、地球锚定/帧旋转 | E4d 结转 #1/#2 | 另立 |
| 10 | `upsert_place`/`set_position` 的 SQLITE_BUSY/LOCKED 重试、`anchor_map_id` DB 层校验、`MapStore` 端口整理 | E4d 结转 #5/#7/#14 | E4d 后续 |
| 11 | telemetry `u64` 载荷 > `i64::MAX` 被拒（假失败，非假通过）；按声明宽度解析或文档化范围 | E4e final review Minor #11 | 后续 |
| 12 | inventory 扫描安全：`sorted_dirs` 跟随 symlink、`classify` 在进程内 dlopen 每个 `.so`；在 staging 副本上运行或改 out-of-process/`nm` | E4e final review Minor #12 | 后续 |
| 13 | async 上下文 `TestHub::drop` 有意保留 registry 条目（文档化例外）；今日仅 sync runner 使用，安全 | E4e final review Minor #9 | 后续 |
| 14 | TestHub `target/debug` 回退可能掩盖 staging 缺失打包 `.so`；考虑显式 dev/test 开关 | E4e final review Minor #7 | 后续 |
| 15 | streaming grace 可被 `SUBSCRIPTION_TTL == DEMAND_GRACE` 掩盖（未真正 unsubscribe）；断言 `has_demand_at` 或拉长 TTL | E4e final review Minor #2 | 后续 |
| 16 | inventory `classify_error` 仍按错误文案匹配 v1；改为结构化 variant | E4e final review Minor #6 | 后续 |

## 5. 结论

E4e 退出标准中的自动项全部满足：demo-declarative 10/10 自动 profile 通过，`inventory` 可盘点内置/安装/v1 产物且 `--strict` 可用于 CI，迁移计划覆盖内置、`drivers/*`、外部 `.so`、数据/空间切换与回滚 runbook，全量 Rust/Web 回归 0 failed。manual/交互按裁定未实现；迁移执行按裁定另立阶段。

## 6. Final review 修正验证（2026-10-23）

| # | 命令 | 结果 |
|---|---|---|
| 1 | `cargo fmt --all` | 已格式化；回滚无关存量漂移 `scheduler.rs`/`watch.rs` |
| 2 | `cargo build -p demo-declarative` | exit 0 |
| 3 | `cargo test -p runtime acceptance` | **85 passed / 0 failed**（新增 streaming 无常开源 skip 测试、TestHub 内置名冲突测试） |
| 4 | `cargo test -p driver-check` | **24 passed / 0 failed**（lib 11 + e2e 7 + inventory 6；新增真实 v1 cdylib 与内置 manifest 校验测试） |
| 5 | `cargo clippy --workspace --all-targets -- -D warnings` | exit 0（仅 `redis v0.25.4` future-incompat 提示） |
| 6 | `cargo test --workspace --lib --bins --tests` | exit 0，**0 failed** |
| 7 | `cargo run -p driver-check -- --package drivers/demo-declarative --profile all` | exit 0，10/10 profile `pass`；`streaming` 6 checks（含 `driver_source=pass`） |
