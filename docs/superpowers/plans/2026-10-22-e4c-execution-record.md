# E4c 执行记录与验证结果

- 日期：2026-10-22
- 分支：`ai-consolidation`
- 提交范围：`42908cca..HEAD`。终审（`final-review.md`）时 HEAD=`186be11c`，`git rev-list --count 42908cca..HEAD` = **14**（终审记录曾误写 13，已校正）。终审修复后 HEAD=`db34dc44` = **16**；终审回归修复（composite run 级并集 claim，即本记录所在提交）后 = **17**。
- 评审修复提交：`d8e6dc86` fix(e4c): 评审收口——stop 归属/闸门原子性、租约移交 worker、确认路径声明 claim（第 **15** 个）；本记录校正为第 **16** 个提交（`42908cca..HEAD` = **16**）。
  - 终审后修复的评审项：Issue 1、2、3、4、5、7、8、12 与 Issue 10（记录校正）。Defer：Issue 6、9、11、13、set_device stop E2E、跨进程仲裁、fmt 基线（见 §4）。
  - `3696ece0` feat(e4c): 多资源 claim 注册表
  - `ec6d808a` fix(e4c): stop 与 claim 互斥 + 去重/排序加固
  - `0860be25` feat(e4c): call/set 按声明 claim 资源
  - `28329c6c` feat(e4c): worker 取消令牌与 in-flight outcome
  - `e43b264a` fix(e4c): 一次调用一个 outcome + 取消终态不重试
  - `afa0e891` feat(e4c): stop 立即返回与异步取消留痕
  - `f4e90277` feat(e4c): composite 步骤资源 claim
  - `e64b4ff5` feat(e4c): resources/cancel 验收 profile
  - `7a900abf` fix(e4c): resources/cancel 验收 profile 修复评审缺陷
  - `0f1ef113` test(e4c): 资源/取消 E2E
  - 另含收口修正 `test(e4c): 不合作驱动阻塞窗口 1.2s→1.5s …`（Task 8 内提交）
  - `d1f9d8b0` fix(e4c): set stop 闸门单测 + cancel profile 宽限余量 + 记录校正
- 工具链：rustc 1.95.0 / cargo 1.95.0，macOS（darwin）；Node v24.14.0（web）
- 计划：`docs/superpowers/plans/2026-10-22-e4c-resources-stop.md`
- 上位 spec：`docs/superpowers/specs/2026-10-22-e4c-resources-stop-design.md`

## 1. 命令与结果

| # | 命令 | 结果 |
|---|---|---|
| 1 | `cargo fmt --all`（评审修复后） | 已对 E4c 触及文件格式化；随后按“不混入无关格式变更”`git checkout` 还原了既有漂移 `crates/agent/src/runtime/thing_agent/scheduler.rs`、`crates/core/src/watch.rs`。`cargo fmt --all -- --check` 仍会因这两处存量漂移失败（见 §4 #9） |
| 2 | `cargo clippy --workspace --all-targets -- -D warnings` | 通过（exit 0，56s）；仅 `redis v0.25.4` future-incompat 提示，非本次改动 |
| 3 | `cargo test --workspace --lib --bins --tests` | **2338 passed / 0 failed / 4 ignored**（53 个测试二进制，exit 0）。E4b 基线 2326；`186be11c` 起净增 **12** 个测试（9 async + 3 sync；终审记录曾误写 11/9+2）。本次 completions 修复另加 **6** 个 async 测试（worker 2 + composite E2E 4），故 `186be11c→本记录提交` 净增 **18**（15 async + 3 sync） |
| 4 | `cargo build -p demo-declarative` | 通过（exit 0） |
| 5 | `cargo test -p driver-check` | 4 passed / 0 failed（exit 0；lib 0 + e2e 4） |
| 6 | `cargo run -p driver-check -- --package drivers/demo-declarative --profile all` | exit 0；`demo-declarative@1.0.0 -> PASS (5 profiles: core/materialization/migration/resources/cancel 全部 pass)` |
| 7 | `npx tsc --noEmit`（web，Node 24） | 通过（exit 0） |
| 8 | `npx vitest run --config vitest.config.ts`（web，Node 24） | **22 files / 169 tests passed**（exit 0，5.40s） |
| 9 | `npm run build`（web，Node 24） | 通过（exit 0，2.98s）；仅既有 chunk >500kB 提示 |

未运行 `cargo test --doc`（任务要求为 `--lib --bins --tests`）。

### 1.1 定向回归

收口修复后（HEAD）补跑：`set_device_stop_gate_rejects_and_publishes_no_event` 1 passed；`e4c_resources_stop` **6 passed**（新增慢驱动真实并发工具调用）；`acceptance::profiles::cancel` 1 passed；`driver::worker` 19 passed；`arbitration` 21 passed；`things::thing` 46 passed；`autonomous_invoke` 18 passed；`workspace_heartbeat` 21 passed。

终审回归修复后定向复跑：`cargo test -p tinyiothub-cloud composite` **19 passed / 0 failed**；`cargo test -p tinyiothub-cloud e4c_resources_stop` **6 passed / 0 failed**；`cargo clippy --workspace --all-targets -- -D warnings` 与 `cargo clippy -p tinyiothub-cloud --all-targets -- -D warnings` 均 exit 0。

completions 修复（本记录所在提交）后定向复跑：`cargo test -p tinyiothub-cloud composite` **23 passed / 0 failed**；`cargo test -p tinyiothub-cloud e4c_resources_stop` **10 passed / 0 failed**；`cargo test -p runtime` **282 lib + 24 集成/其他 passed / 0 failed**；`cargo clippy --workspace --all-targets -- -D warnings` exit 0。

| 过滤词 | 结果 |
|---|---|
| `e4c_resources_stop` | 10 passed / 0 failed（E4c 全部 E2E；含 4 个 composite 执行窗口用例） |
| `hub_stream` | 22 passed / 0 failed（runtime hub stream + cloud handler + demand 跨层） |
| `live_stream` | 1 passed / 0 failed |
| `watch_e2e` | 1 passed / 0 failed（`watch_hit_wakes_run_within_2s_and_restart_reweaves`） |
| `derived` | 36 passed / 0 failed |
| `push_driver_sink` | 无同名测试；以等价的 `sink`（8 passed，含 `full_value_sink_does_not_block_execute`、`sink_reading_for_thing_without_wrapper_falls_back_to_cache`）+ `push_reading_flows`（1 passed，`push_reading_flows_through_shared_pipeline`）覆盖，均 0 failed |

## 2. E4c 退出清单证据

| 退出项 | 证据 | 结论 |
|---|---|---|
| 两 run 并发同设备：后者收到带持有者的 busy | E2E `concurrent_real_tool_calls_second_busy_while_first_executes`（慢驱动在途 + 两条真实 `call_device`，第二条 `busy: chat:ws-e4c holds actuator`；终审 Issue 4 的“真实并发”证据）；`concurrent_call_same_exclusive_resource_second_busy_then_first_completes`（外部 lease）；`arbitration::busy_message_carries_holder_and_resource`、`concurrent_acquire_exactly_one_wins` | PASS |
| 不同资源可并行；shared/capacity 正确 | E2E `different_resources_do_not_conflict_and_expired_lease_releases`、`shared_capacity_allows_up_to_capacity_for_call_and_set`；`exclusive_claims_are_per_resource`、`shared_capacity_counts_alive_holders`、`shared_and_exclusive_conflict`；driver-check `resources.exclusive/shared` | PASS |
| 多资源 all-or-nothing / TTL | `sorted_all_or_nothing_releases_partial_claims`、`sorted_order_reports_first_busy_resource`、`ttl_expiry_allows_reacquire`、`stale_multi_resource_drop_only_releases_own_tokens`；driver-check `resources.all_or_nothing/ttl` | PASS |
| stop 立即返回 `cancellation_requested` | E2E 两个 stop 用例均在 `stop` 调用上断言 <100ms 返回且 `status=cancellation_requested`；`device::…stop_returns_cancellation_requested_and_gate_rejects_next_call` | PASS |
| 协作驱动 ≤1s 退出 → `cancelled` 事件 + 账本 | E2E `stop_cooperative_in_flight_cancels_within_grace_and_gates_next_call`（`CommandCancelled outcome=cancelled` + 恰好一条 `call_cancelled`）；`data_server::cancelled_result_publishes_event_and_appends_ledger_row`；driver-check `cancel.cooperative/cancel.pipeline` | PASS |
| 不合作驱动 → `cancelled_unknown` | E2E `stop_non_cooperative_driver_records_cancelled_unknown_after_grace`（阻塞 1.5s，`elapsedMs >= 1000`，恰好一条 `call_cancelled_unknown`，事件 metadata `outcome=cancelled_unknown`，`cancelled_counts=(0,1)`）；`non_cooperative_read_reports_cancelled_unknown_after_return`；driver-check `cancel.non_cooperative` | PASS |
| stop 后新 call/set 被 `stopped:` 拒绝；claim 已释放 | E2E 协作用例断言闸门期间下一次调用 `busy: stop:ops emergency holds __device__`（claim 兜底）且 `stop_reason` 生效；`invoke_action::…`/`device::…` 单测断言工具层统一 `stopped: <reason>`；`arbitration::stop_gate_releases_all_claims_and_rejects_new_claims`、`stopped_claim_leaves_no_partial_claims`；driver-check `cancel.stop_gate` | PASS |
| composite 按声明 resources 并集 claim 全程，busy 文案可读 | `composite_tools::sequential_steps_same_exclusive_resource_do_not_self_conflict`（同 thing 顺序步骤共享 exclusive 资源不 self-conflict）、`concurrent_composite_overlapping_resource_is_busy_with_holder_and_resource`（并发 composite → busy 带 holder+resource，零步骤执行）、`failed_composite_releases_its_claims`、`rollback_releases_composite_claims`、`shared_capacity_allows_concurrent_claim_until_full`、`busy_first_step_fails_without_partial_success`（resolver None 保持 E1 每设备互斥）；`composite.rs` run 开始时 `claim_run` 一次 `try_acquire_resources`。completions 修复后新增 E2E（真实 DataServer + worker）：`composite_run_lease_covers_worker_execution_and_blocks_second_run`（首步驱动内阻塞 → composite 不返回 + 第二个 composite/call busy → 放行后完成并释放）、`composite_sequential_same_resource_steps_still_complete`、`composite_worker_failure_marks_step_failed_and_releases_run_lease`、`composite_step_failure_rolls_back_and_releases_run_lease`；worker 单测 `waiting_submission_receives_outcome_only_after_execution` / `waiting_submission_reports_cancelled_outcome` | PASS |
| 默认无 resources 的变更工具保持 E1 每设备互斥 | `invoke_action` / `device` 单测断言未声明 resources 时 claim 合成 `__device__`；`arbitration::mutual_exclusion_per_thing`、`try_acquire` 兼容入口；driver-check `resources.default_device`；全量 E1 套件 0 failed | PASS |
| E4b Hub stream / E3 live stream / watch / derived / push 不回归 | 定向回归表（§1.1）全部 0 failed；全量 2326 passed / 0 failed | PASS |

## 3. 偏差与说明

1. **stop 也会取消采样读，但审计已能区分 kind**：`WorkerHandle::cancel_current` 设置 worker 当前在途 token，read 与 execute 共用同一取消槽，故 stop 同样会取消在途采样读并留 `cancelled*` 痕迹。E4c 评审修复后 `WorkerResult::Cancelled` 携带 `kind: "read"|"execute"`，事件 metadata 与账本 payload 均含 `kind`；账本行 kind 仍保持 kind-agnostic（`call_cancelled` / `call_cancelled_unknown`），读取消的审计保真依靠 payload.kind，而不是独立账本类别。E2E 驱动为 `DriverMode::Push`（execute-only），未直接覆盖读取消；runtime 单测 `cancel_current_cancels_cooperative_read_within_grace`（断言 `kind=read`）与 `non_cooperative_read_reports_cancelled_unknown_after_return` 覆盖读路径。
2. **指标为 atomics + getter，无命名 metric 日志/Prometheus**：`DataServer::cancelled_counts() -> (cancelled, cancelled_unknown)` 由 `AtomicU64` 维护；`stop_status`/latency 等未单独命名导出。spec §2.4 的 `arbitration_busy_total`、`stop_requests_total`、`stop_latency_ms` 等未接线为独立指标，留 E4e 运维可见性。
3. **composite 的 stop 文案已统一为 `stopped: <reason>`（终审后修复）**：run 级租约被 `set_stop` 强制释放后，`run_op` 在每步开始前复检 `stop_reason` 并返回 `stopped: <reason>`，不再依赖逐步骤 `try_acquire_resources` 的 Busy 兜底（旧文案 `busy: stop:<reason> holds <resource>`）。
4. **`set_device` 的 stop E2E 缺口**：E2E 的资源/停止场景以 `call_device` 为主；`set_device` 由 `device.rs` 单测 `set_device_stop_gate_rejects_and_publishes_no_event` 覆盖 stop 闸门（拒绝且不发布属性事件），另有 claim/E1 合成资源单测；未在 E2E rig 中重复 stop 管线。
5. **账本 append 在 DataServer 消费循环内完成**：`consume_worker_result` 收到 `Cancelled` 后发布事件并 `append` 账本；stop 工具本身不等待写账本，符合“立即返回、异步留痕”。账本失败只记 warn，不影响取消终态；慢 DB 可能延迟同循环的读数应用（已列为 E4d/e 性能/隔离项）。
6. **stop 闸门检查已移到 workspace 解析之后（已修复）**：`invoke_action` / `set_device` 先 `get_thing(thing_id, workspace_id)`（物不存在 → 正常 not-found），再读 `stop_reason`；`StopTool` 也新增 `workspace_id` + `ThingService`，跨租户 id 不会置闸门/取消在途调用。claim 路径仍天然 fail-closed（停闸期间 `try_acquire_resources` 返回 `Busy{holder:"stop:<reason>"}`）。单测：`cross_tenant_call_does_not_leak_stop_reason`、`set_device_cross_tenant_does_not_leak_stop_reason`、`stop_rejects_cross_workspace_thing_without_setting_gate`。
7. **确认门路径的声明资源 claim（已修复）**：`dispatch_command` 现接收 `workspace_id` + `Option<&dyn ResourceResolver>`，按工具声明 `resolve_action` 后 `try_acquire_resources`（未接线时才回落 E1 合成 `__device__`），并对 stop 闸门返回 `stopped: <reason>`。两条此前绕过声明的路径均已接线：
   - **自治自动确认**（`autonomous_invoke.rs::auto_confirm`）传 `self.inner.resources`；
   - **人工批准**（`workspace_heartbeat.rs::execute_approved_tool` ← `approve_and_execute` ← `approve_proposal`）传 `state.access`（`AccessService` 即 `ResourceResolver`）。
   已加单测：`auto_confirm_claims_declared_resources`、`approve_dispatch_claims_declared_resources`（占用声明资源 → busy 带 holder+resource）。剩余说明：若工作区未装配 `AccessService`，两条路径保持 E1 合成 `__device__` 互斥（与 chat 工具路径一致）；声明缺失/包不可用时 fail-closed（`resolve_action` 语义，见 §3.11）。
8. **不做排队/分布式仲裁**：资源 busy 立即拒绝，无等待队列；仲裁仅进程内 `DashMap`，跨进程/边缘（X5）不在本期（spec §4）。
9. **not covered**：`cargo test --doc`；fmt 存量漂移（`scheduler.rs`、`watch.rs`）未修复，按“不混入无关格式变更”原则保留原样并在 §1 记录。
10. **1.5s 阻塞窗口**：不合作驱动 E2E 的阻塞从 1.2s 提升到 1.5s，给 >1s 宽限断言留出 500ms 余量，降低负载下 `elapsedMs` 抖动导致的假 unknown/cancelled 误判风险。
11. **`resolve_action` 的空声明快路径（Issue 12）**：先找声明/工具，`tool.resources.is_empty()` 直接返回合成 `device_claim()`，不再要求包清单可用；只有真正声明了资源 id 的工具才 `fail-closed`（包不可用/未知 id → 拒绝）。`resolve_properties` 维持既有语义（无声明 → 合成设备资源）。测试：`resolve_action_without_package_uses_device_claim_for_empty_resources`。
12. **资源租约覆盖真实执行窗口（Issue 4）**：`WorkerCommand::Execute` 增加 `lease: Option<ResourceLease>`；`DataServer::execute_command_with_lease` 把租约移交给 worker，worker 在调用结束（成功/失败/取消）后 Drop。已接线的工具路径：`call_device`（`InvokeActionTool`）、自动确认/人工批准派发（`dispatch_command`）。composite 不再逐步移交（见 §3.16）；`WorkerCommand::Execute` 新增可选的 `done: oneshot::Sender<CommandOutcome>`，Call 步骤经 `DataServer::execute_command_with_lease_and_wait` 等待 worker 执行终态后才进入下一步，因此 run 级租约覆盖排队 + **完整执行窗口**（不只是入队瞬间）；等待返回错误 → 该步 failed，租约保持到回滚/run 结束。`set_device` 不经过 worker（发布属性事件是同步路径），租约保持到工具返回（作用域 RAII）。证据：E2E `concurrent_real_tool_calls_second_busy_while_first_executes`（慢驱动在途，第二条真实工具调用得到 `busy: chat:ws-e4c holds actuator`）与 worker 单测 `submitted_lease_covers_execution_then_is_released`。
13. **stop 不 drain 已入队的 Execute（已知限制）**：`cancel_calls` 只取消驱动内当前调用；worker 的 FIFO 中在 stop 之前入队、尚未取出的命令仍会在 stop 后执行（其 claim 已被 `set_stop` 强制释放，因此可能绕过资源互斥）。单 worker FIFO 下窗口很小，但不是零。留 E4d（stop epoch/代际跳过）。
14. **工具层之外的派发入口仍是无 claim 的旧行为**：管理面（`admin/open`、`admin/thing/commands`、`thing/handler/actions`）、MCP 适配器、cron executor 直接 `DataServer::execute_command`（`lease: None`），不参与声明资源仲裁；它们不在 E4c 的 agent 工具边界内。已在代码注释中保留旧行为，E4d 评估是否统一接入 resolver。
15. **stop 闸门的原子清理与 ttl clamp（Issues 1/7）**：`stop_reason` 的过期清扫改用 `DashMap::remove_if` 在分片锁内复检截止时刻，关闭 get→remove 竞态（并发 `set_stop` 的新闸门不会被误删；回归测试 `expired_stop_sweep_never_deletes_freshly_set_gate`）；`set_stop` 用 `checked_add` 兜底超大 TTL；`StopTool` 将 `ttlSecs` clamp 到 1..=3600s（测试 `stop_clamps_ttl_seconds`、`set_stop_with_huge_ttl_does_not_panic`）。
16. **composite 改为 run 级并集 claim（终审回归修复）**：`CompositeExecutor::execute` 在步骤循环前调用 `claim_run`，解析全部步骤（含回滚）的声明 resources，按 thing 分组、同 id 合并（exclusive 优先 / shared capacity 取最小），`BTreeMap` 按 thing id 排序后逐 thing `try_acquire_resources`，全部租约持有到整条链结束（成功/失败/回滚）。Call 步改用 `dispatch_command_with_lease_and_wait(..., None)`：等待 worker 执行终态后才进入下一步（顺序步骤不 self-conflict，且 run 级租约覆盖 worker 执行窗口），不再把每步租约交给异步 worker——修复“顺序步骤在同一 holder 下 self-conflict”的终审回归与“入队即返回导致租约不覆盖执行”的复核发现。resolver 未接线时每台不同的物各 `try_acquire` 一次并全部持有（E1 语义），任一 busy all-or-nothing 释放。claim 失败不执行任何步骤（`status=failed`，`steps[0]` 带 holder/resource）。测试见 §2 composite 行。

## 4. 结转 E4d/E4e

> E4c 终审（`.superpowers/sdd/2026-10-22-e4c-resources-stop/final-review.md`）的 Defer 项在此显式登记，避免静默接受。

| # | 事项 | 来源 | 归属 |
|---|---|---|---|
| 1 | **stop 不 drain 已入队 Execute**：stop epoch/代际，跳过 stop 前入队的命令（当前会绕过被强制释放的 claim 执行，窗口小但真实） | 评审 Issue 6；本文 §3.13 | E4d |
| 2 | **账本 append 内联在 DataServer 消费循环**：慢 DB 阻塞同循环的读数应用；改为有界旁路 writer | 评审 Issue 9；本文 §3.5 | E4d |
| 3 | **指标导出**：把 `cancelled_counts` 与 spec §2.4 的 busy/stop 计数、stop 延迟接线到健康/指标端点（当前仅测试可读） | 评审 Issue 11；本文 §3.2 | E4e |
| 4 | **声明重复解析的性能**：每次 action/set/busy 检查都会重新读取并解析 device declaration（`AccessService::resolve_action`/`resolve_properties`）；当前调用频率下可接受，若热点路径需要可加 workspace+thing 的短 TTL 缓存（需与 E4a 声明变更失效联动） | 终审 Defer 列表；本文 §3.11 | E4d |
| 5 | **`set_device` stop 管线 E2E**：补一条 set 驱动的 stop 立即返回/取消留痕端到端用例 | 评审 Defer 列表；本文 §3.4 | E4d |
| 6 | **工具层之外的派发入口统一接入 resolver**（管理面/MCP/cron）：当前仅 agent 工具边界做声明 claim | 本文 §3.14 | E4d |
| 7 | **跨进程/分布式仲裁**：进程内注册表向 edge/X5 扩展时需重新设计 claim 键与租约同步 | spec §4；本文 §3.8 | E4e / X5 |
| 8 | **资源预占/排队**（spec §4 非目标）：如需排队，扩展 `try_acquire_resources` 为可等待队列；当前明确立即 busy | spec §4 | E4e |
| 9 | **fmt 基线漂移**：`crates/agent/src/runtime/thing_agent/scheduler.rs` 与 `crates/core/src/watch.rs` 仍不满足 `cargo fmt --all -- --check`；建议单独 `style:` 提交统一修复，避免混入功能提交 | 评审 Defer 列表；本文 §3.9 | E4d |
| 10 | **`(workspace, thing, resource)` claim 键**：当前注册表仍按 `(thing, resource)` 全局键，workspace 归属由工具层先鉴权（thing id 为全局主键）；若未来允许跨 workspace 复用 thing id 或引入多租户共享边缘设备，再扩展 claim 键并同步租约语义 | 终审 Defer 列表；spec §1 边界 5 | E4e / X5 |
| 11 | **`stop_reason` 未命中路径的写锁**：未命中时 `remove_if` 会取分片写锁，且每次 `try_acquire_resources` 前后各调用一次；当前频率可忽略，热点路径可改无锁读+后台清扫 | final-fix-review 性能备注 | E4e |
