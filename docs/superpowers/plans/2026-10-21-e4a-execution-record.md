# E4a 执行记录与验证结果

- 日期：2026-10-21
- 提交范围：`8652ab98..4ad52474`（6 个提交，全部在 `ai-consolidation`）
  - `a58e11bb` feat(e4a): AccessService、安装 API 与建物物化接线
  - `3ab67a9c` fix(e4a): 安装原子性、动态驱动引用释放与包重水化
  - `94b4026f` feat(e4a): 设备调用走声明校验（未声明/越界拒绝）
  - `b68cf74e` fix(e4a): 声明校验拒绝走失败 ToolResult；修正 clamped 语义
  - `2315ee7a` feat(e4a): 验收 runner（core/materialization/migration）+ acceptance API + driver-check
  - `4ad52474` fix(e4a): warn 不阻断验收 + core 检查独立 + 二进制覆盖确定性
- 工具链：rustc 1.95.0 / cargo 1.95.0，macOS（darwin）
- 计划：`docs/superpowers/plans/2026-10-21-e4a-access-declaration-protocol.md`
- 上位 spec：`docs/superpowers/specs/2026-10-21-e4-access-declaration-design.md`

## 1. 命令与结果

| # | 命令 | 结果 |
|---|---|---|
| 1 | `cargo fmt --all -- --check` | **失败（预期内）**：仅 `crates/agent/src/runtime/thing_agent/scheduler.rs`、`crates/core/src/watch.rs` 两处存量漂移，均为 E4a 之前既有问题，未修改 |
| 2 | `cargo clippy --workspace --all-targets -- -D warnings` | 通过（exit 0，59.49s）；仅 `redis v0.25.4` future-incompat 提示，非本次改动 |
| 3 | `cargo build -p demo-declarative` | 通过（exit 0） |
| 4 | `cargo test -p core` | 122 passed / 0 failed / 1 ignored（`generate_manifest_schema` 为手动生成器；`schema_file_matches_types` gen:check 已通过） |
| 5 | `cargo test -p runtime` | 161 passed / 0 failed（141 + 10 + 2 + 3 + 5） |
| 6 | `cargo test -p db` | 207 passed / 0 failed（含 14 个测试二进制） |
| 7 | `cargo test -p demo-declarative` | 5 passed / 0 failed |
| 8 | `cargo test -p driver-check` | 2 passed / 0 failed（E2E，cdylib 已先构建） |
| 9 | `cargo run -p driver-check -- --package drivers/demo-declarative --profile all` | exit 0；`demo-declarative@1.0.0 -> PASS`，core=pass、materialization=pass、migration=pass，共 8 项检查全 pass |
| 10 | `cargo test -p core driver::dynamic::tests::v1_message_names_the_artifact` | 1 passed（122 filtered out） |
| 11a | `cargo test -p tinyiothub-cloud access_declaration` | 6 passed / 0 failed |
| 11b | `cargo test -p tinyiothub-cloud invoke_action` | 11 passed / 0 failed |
| 12 | `cargo test -p tinyiothub-cloud`（全量，仅跑一次） | 1097 passed / 0 failed / 1 ignored（`judgment_eval_verdict_accuracy` 标注需真实 LLM 凭据，长期在 CI 忽略），用时 630.57s |

CLI 验收详情（`driver-check --profile all`）：

- `core`：`manifest.schema`、`identity.abi`（abi=2）、`config.schema` 全 pass。
- `materialization`：`declaration.valid`、`projection.write`、`projection.count`（props=1 tools=3）全 pass；无模板能力拷贝。
- `migration`：`v1.rejected`、`v2.version` 全 pass。
- v1 拒载文案（实际输出）：`driver 'libv1.so' does not export tinyiothub_driver_vtable_v2; ABI v1 is unsupported by E4. Migrate to ABI v2 (async read/execute + cancel + describe). See docs/superpowers/specs/2026-10-21-e4-access-declaration-design.md §2.3.`

## 2. 偏差与说明

1. **fmt 非全局干净（存量）**：`cargo fmt --all` 会改写 `scheduler.rs` 与 `watch.rs` 两处非 E4a 文件；按要求未提交、未修改。E4a 触及的文件均无 fmt 漂移。
2. **过滤测试命令拆分**：任务书中的 `cargo test -p tinyiothub-cloud access_declaration invoke_action` 不是合法 cargo 调用（`cargo test [TESTNAME]` 只接受一个过滤词，第二个参数会报 `unexpected argument 'invoke_action'`）。已拆成 11a/11b 两条命令分别执行，均通过；这不是代码失败。
3. **driver-check E2E 前置**：`crates/driver-check/tests/e2e.rs` 断言 `target/debug/libdemo_declarative.{so,dylib}` 存在，缺失时直接失败而非跳过。本次先 `cargo build -p demo-declarative` 再跑测试/CLI。runtime-only 的 cdylib 相关用例在未构建时仍会自行 skip。
4. **warn 语义**：验收报告 `Level::Warn` 不阻断 `report.passed()`（Task 12 裁定）；本次 demo-declarative 全部为 pass，未触发 warn。
5. 工作区在验证后保持干净（`git status --short` 无输出）。

## 3. 交给 E4b-e 的开放项（非本计划目标）

1. **E4b 采样**：per-source 调度、DemandTracker、health 慢通道、push `set_demand`。`ValidatedCall.timeout_ms` 已计算但 dispatch 尚无逐命令超时，需并入 E4b 命令预算。
2. **E4c 资源与取消**：多资源 claim/lease、busy 持有者、lease 到期释放；`stop_device` 尚未接在途取消（ABI cancel 手柄与 `long_task` 协作已就绪，1s 宽限/cancelled_unknown 待接）。
3. **E4d 空间**：maps/places 数据模型与 API、坐标/trust 列、point-in-polygon、zone 权威切换 flag；E4a 只解析/校验 `placement`，物化不含坐标列。
4. **E4e 全套验收**：本期仅交付 core/materialization/migration 三个 profile；lifecycle/telemetry/tools/streaming/resources/cancel/placement/manual、`--interactive`、五级报告与 test hub 仍待实现。
5. **场景/MCP 建物回填**：MCP `create_thing` 的模板路径走 `domains::driver::legacy::service::create_thing_from_template`，直接按模板写属性/命令，未经过声明物化；另无 pre-E4a 存量物的声明回填/开关，调用校验当前无条件 fail-closed。
6. **UI 直连调用绕过**：`POST /v1/things/{id}/actions/{name}/invoke` 只校验注册动作与参数 schema，不经过 `AccessService` 的声明校验（未声明工具/越界拒绝/超时钳制不生效），待 E4e/final 统一。
7. **plugin-sdk v2 迁移**：trait/FFI 与 README 仍是 v1 签名，动态驱动需自行导出 `tinyiothub_driver_vtable_v2`。
8. **driver-check cdylib 前置**：E2E 与 CLI 均要求先构建 demo-declarative；pre-push 钩子目前不构建它。
9. **Task 1-12 结转 minor**（详见 SDD ledger）：rehydrate 在 `DataServer::new` 之后执行，既有动态物可能需重启才恢复轮询；`DataServer::create_driver` 不释放 ref_count；旧版本目录不清理；安装失败返回 HTTP 200；物化失败可能留下孤儿物；声明缓存无失效；投影写入非事务；嵌套对象 schema 未递归校验等。

## 4. E4a 退出清单核对

| 退出项 | 证据 | 结论 |
|---|---|---|
| `demo-declarative` 仅凭 manifest + `.so` 接入，Hub 零代码 | 包仅含 `driver.manifest.json`、`Cargo.toml`、`src/lib.rs`；`apps/cloud`/`runtime` 生产代码无引用（仅测试 fixture 的 `include_str!` 与 cdylib 文件名候选）；`driver-check` 直接按目录加载并 PASS | PASS |
| core / materialization / migration profiles | `driver-check --profile all`：3/3 profile、8/8 检查 pass | PASS |
| v1 拒载且带迁移指引 | `migration.v1.rejected` pass；`v1_message_names_the_artifact` 1 passed；文案含 `.so` 名、`tinyiothub_driver_vtable_v2` 与 spec §2.3 | PASS |
| 调用校验：未声明拒绝、业务越界拒绝、超时钳制 | runtime `undeclared_tool_is_rejected` / `out_of_range_param_is_rejected` / `timeout_defaults_to_declared_and_clamps_to_max`；cloud `test_invoke_action_rejects_undeclared_tool_when_access_wired` / `test_invoke_action_rejects_missing_declaration_when_access_wired` 均通过 | PASS |
| 安装 / 持久化 / 重水化 | runtime `install_persists_package_and_records_active_row`、`manifest_only_reinstall_removes_stale_binary`、`bind_fixed_driver_persists_and_materializes_declaration`、`dynamic_bind_releases_ref_count_and_allows_reinstall`、`rehydrate_restores_active_packages_and_skips_retired`；cloud `installs_multipart_package_and_lists_declaration`、`reinstall_keeps_single_declaration_entry` 等 6 项全通过 | PASS |

**结论：E4a 范围内的自动检查全部通过；fmt 仅剩两处非本计划的存量漂移，作为已知项记录。**
