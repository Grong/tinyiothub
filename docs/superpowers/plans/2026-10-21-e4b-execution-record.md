# E4b 执行记录与验证结果

- 日期：2026-10-21
- 提交范围：`f55364a6..HEAD`（E4b 首个提交 `0433f9d1`；评审时 `git rev-list --count f55364a6..HEAD` = **24**，全部在 `ai-consolidation`；Task 10 的代码/修复与文档为最后五个提交 `1bd6c594`、`38dc9dea`、`c63b8210`、`21d7674b`、`b1c4863b`，此后的全分支评审收口提交另计）
  - `0433f9d1` feat(e4b): DemandTracker 与 DemandSource port
  - `c6cb2935` fix(e4b): DemandTracker 属性级需求访问器
  - `72836d0c` feat(e4b): sampling catalog 与 property→source 映射
  - `0acab38b` fix(e4b): fallback 不因 health 排除唯一 poll source
  - `190d0e86` feat(e4b): 每设备 driver worker 与队列策略
  - `21f41573` fix(e4b): worker 写回 Thing、sink 非阻塞上报并补失败读测试
  - `823b3e04` refactor(e4b): DataServer 每设备 worker
  - `288b751a` fix(e4b): worker 心跳、单消费者与失败节流
  - `a232eb1b` feat(e4b): 需求驱动 sampler（门控/health/宽限）
  - `93c932d7` fix(e4b): per-source demand gating + catalog log throttle
  - `9eae0331` feat(e4b): 读取/订阅写入 DemandTracker
  - `e3f62061` fix(e4b): 读工具/快照按声明属性记需求 + 订阅 TTL 边界测试
  - `1a045b07` feat(e4b): cloud demand 快照适配器
  - `378212fa` feat(e4b): Hub 订阅流端点与连接需求生命周期
  - `188d75d4` docs(e4b): snapshot 心跳时间语义 + control 范围（watch 留 E3）
  - `86a514e6` fix(e4b): Hub stream 兄弟属性合并、心跳 RFC3339 与生命周期测试
  - `29eaac32` docs(e4b): 修正 control/告警表引用
  - `df82bf07` feat(e4b): Hub 订阅流 web 客户端与设备卡接入
  - `5f2f58bb` fix(e4b): Hub resync 连接清理与设备卡可访问性
  - `1bd6c594` feat(e4b): 采样/Hub 指标与确定性需求 E2E
  - `38dc9dea` docs(e4b): 执行记录与验证结果
  - `c63b8210` fix(e4b): 补 sampling_lag_ms 指标与 Hub 连接生命周期 E2E（评审修复）
  - 本记录更新提交 `docs(e4b): 补齐 sampling_lag 指标记录与验证结果`
- 工具链：rustc 1.95.0 / cargo 1.95.0，macOS（darwin）；Node v24.14.0（web）
- 计划：`docs/superpowers/plans/2026-10-21-e4b-sampling-hub-stream.md`
- 上位 spec：`docs/superpowers/specs/2026-10-21-e4b-sampling-hub-stream-design.md`

## 1. 命令与结果

| # | 命令 | 结果 |
|---|---|---|
| 1 | `cargo fmt --all -- --check` | **失败（预期内，存量）**：仅 `crates/agent/src/runtime/thing_agent/scheduler.rs`、`crates/core/src/watch.rs` 两处既有漂移；本任务未修改、已 `git checkout` 还原。E4b 触及文件均无 fmt 漂移 |
| 2 | `cargo clippy --workspace --all-targets -- -D warnings` | 通过（exit 0；评审修复后复跑 exit 0）；仅 `redis v0.25.4` future-incompat 提示，非本次改动 |
| 3 | `cargo test --workspace --lib --bins --tests`（评审修复前基线） | **2254 passed / 0 failed / 4 ignored**（53 个测试二进制，exit 0）；其中 cloud 1113 passed / 1 ignored（636.73s），runtime 222 passed |
| 3b | `cargo test -p runtime`（评审修复后） | **247 passed / 0 failed**（223 lib + 10 + 2 + 4 E2E + 3 + 5，exit 0） |
| 3c | `cargo test -p tinyiothub-cloud domains::agent::host::handler::hub_stream`（评审修复后） | 8 passed / 0 failed（含新增 resync 计数断言） |
| 4 | `cargo test -p runtime --test e4b_demand_e2e` | 4 passed / 0 failed（确定性 E2E，无 sleep；含真实 `DemandGuard` 跨层用例） |
| 5 | `cargo build -p demo-declarative` | 通过（exit 0） |
| 6 | `cargo test -p driver-check` | 2 passed / 0 failed |
| 7 | `cargo run -p driver-check -- --package drivers/demo-declarative --profile all` | exit 0；`demo-declarative@1.0.0 -> PASS`（core/materialization/migration 3/3 profile） |
| 8 | `npx tsc --noEmit`（web，Node 24） | 通过（exit 0） |
| 9 | `npx vitest run --config vitest.config.ts`（web，Node 24） | **22 files / 169 tests passed**（exit 0，12.35s） |
| 10 | `npm run build`（web，Node 24） | 通过（exit 0，4.74s）；仅既有 chunk >500kB 提示（three.js 732.69 kB） |

未运行 `cargo test --doc`（任务要求为 `--lib --bins --tests`）。

## 2. 本任务改动（Task 10）

### 2.1 指标（轻量 atomics + getter，无指标后端）

| 组件 | 指标 | 语义 / 位置 |
|---|---|---|
| Sampler | `demand_active_sources` | `demand_only && !health` 的 source 本 tick 处于活跃（需求或宽限）的次数累计；`tick_once` 内累加 |
| Sampler | `demand_skipped_sources` | 同上门控 source 因无需求且宽限已过被跳过的次数累计 |
| Sampler | `reads_submitted` | 提交给 worker 的 source 读次数累计（同一 thing 的 due source 仍合并为一次 `submit_read`，计数按 source 数） |
| Sampler | `sampling_lag_ms_total` | 调度滞后累计毫秒：每次到期 source 的实际采样时刻晚于 `due_at = last_sample + interval` 的时长之和；首次采样与准点 tick 记 0 |
| Sampler | `sampling_lag_ms_max` | 单次调度滞后最大值（毫秒），`fetch_max` 维护 |
| Sampler | `Sampler::metrics() -> SamplerMetrics` | 一次性快照（含上述全部计数） |
| Worker | `queue_depth` | Execute FIFO（32）中排队待处理命令数；入队先加、失败回滚，worker 取出时递减，停止时归零 |
| Worker | `reads_coalesced` | worker 忙期间覆盖未处理 Read（latest-wins）的次数 |
| Worker | `readings_dropped` | ValueSink 满/关闭导致丢弃的读数条数 |
| Worker | `WorkerHandle::{metrics,queue_depth,reads_coalesced,readings_dropped}` | 快照 + 单项 getter |
| Hub stream | `hub_stream_connections` | 进程级累计成功建立（通过校验并登记订阅）的连接数 |
| Hub stream | `hub_stream_resyncs` | 进程级累计发出的 `resync` 帧（reconnect/lagged/overflow） |
| Hub stream | `hub_stream_metrics()` / `HubStreamMetrics` | 进程级静态实例 + getter；cloud handler 在连接建立与 resync 出队时累加 |

`sampling_demand_start/stop` tracing 事件保持不变（仍只在门控状态变化时记录一次）。未引入 HubLedger 依赖，未新增账本事件（spec 标注为可选）。

### 2.2 确定性 E2E

新增 `crates/runtime/tests/e4b_demand_e2e.rs`（4 个测试，全部注入 `Instant`，0 真实等待）：

1. `hub_subscription_drives_demand_only_source_and_health_continues`：
   - fake catalog 一个物，`slow`（demandOnly poll）+ `health`（health poll）；
   - 无订阅 tick：`slow` 跳过、`health` 采样；
   - `DemandTracker.subscribe_at(slow)` 后下一 due tick：`slow + health`；
   - unsubscribe 后 19s（宽限内）：`slow` 仍采；
   - 31s（>30s 宽限）：`slow` 停采、`health` 继续；核对 active/skipped/reads 计数。
2. `push_worker_set_demand_is_health_union_demanded_and_only_on_transitions`：
   - 首 tick 下发 `["health"]`；集合不变且未满 30s 不重发（满 30s 的周期重发见
     `push_demand_is_re_sent_periodically_even_when_unchanged`，I-2 修复）；
   - 订阅 `slow` → `["health","slow"]`；持续/宽限内不抖动；
   - 宽限结束 → 回到 `["health"]`；push 路径 `submit_read` 恒为 0。
3. `demand_metrics_match_the_push_active_set`：tick 级计数与 push 活跃集一致，push 不提交读。
4. `hub_stream_guard_lifecycle_drives_demand_only_sampling`（跨层）：用生产 handler 的同一入口
   `subscribe_with_guard` 建立连接 → 下一 due tick 采 `slow + health`；`drop(guard)` 立即
   `unsubscribe`（断言 `has_subscription_demand_at` 为 false），宽限内仍采；31s 后停采
   `slow`、`health` 继续。证明 Hub 连接生命周期（而非直接操作 tracker）驱动采样。

前 3 项直接使用真实 `DemandTracker`；第 4 项组合真实 `DemandGuard` + tracker + sampler + fake worker。cloud handler 另有 SSE 连接/断开与 resync 计数测试（见 §3）。

### 2.3 指标单测

- runtime `access::sampler::tests::sampling_lag_is_zero_on_time_and_positive_when_late`（首次/准点 = 0；晚 250ms 与 350ms 累计 600、max 350）。
- runtime `access::sampler::tests::metrics_track_gate_transitions_and_reads`（skipped → active、`reads_submitted` 按 source 计）。
- runtime `access::sampler::tests::sampling_lag_ignores_reactivation_idle`（重新激活的空闲期不计入 lag，M-2）。
- runtime `access::sampler::tests::push_demand_is_re_sent_periodically_even_when_unchanged`（集合不变每 30s 重发，I-2）。
- runtime `access::sampler::tests::demand_only_source_skips_without_demand`（追加 skipped 断言）。
- runtime `driver::worker::tests::two_reads_while_busy_coalesce_to_latest_source_set`（coalesced = 1）。
- runtime `driver::worker::tests::exec_queue_overflow_returns_busy`（队列满 depth = 32；busy 不改变深度；消费完归零）。
- runtime `driver::worker::tests::full_value_sink_does_not_block_execute`（sink 满 dropped = 1）。
- runtime `hub_stream::tests::metrics_count_connections_and_resyncs`。
- cloud `domains::agent::host::handler::hub_stream::tests::resync_frames_increment_the_process_counter`（帧出队累加进程级 resync 计数）。

## 3. E4b 退出清单证据

| 退出项 | 证据 | 结论 |
|---|---|---|
| Hub stream 连接驱动 demand → `demandOnly` 源按 interval 采样 | 跨层 E2E `hub_stream_guard_lifecycle_drives_demand_only_sampling`（真实 `DemandGuard` → sampler）；E2E `hub_subscription_drives_demand_only_source_and_health_continues`；cloud `next_frame_emits_snapshot_before_reading`；runtime `demand_only_source_samples_with_source_subscription` | PASS |
| 断开 30s 后停采，health 源继续 | 跨层 E2E 同上（`drop(guard)` 立即 unsubscribe → 19s 仍采 → 31s 停采，health 持续）；E2E `hub_subscription_drives_demand_only_source_and_health_continues`；runtime `demand_grace_keeps_sampling_then_stops`、`health_source_samples_regardless_of_demand`；cloud `dropping_the_stream_state_unsubscribes_demand` | PASS |
| push 驱动 `set_demand` 起停 | E2E `push_worker_set_demand_is_health_union_demanded_and_only_on_transitions`；runtime `push_worker_gets_set_demand_only_on_changes_and_never_reads`、`push_health_source_is_always_active` | PASS |
| worker 队列策略（合并/优先/上限） | runtime `two_reads_while_busy_coalesce_to_latest_source_set`、`execute_is_served_before_pending_read`、`exec_queue_overflow_returns_busy`、`full_value_sink_does_not_block_execute`、`failure_streak_emits_read_failed_once_and_recovers` | PASS |
| Devices 工作台选中设备卡 Hub stream latest-wins | web `src/api/hub-stream.test.ts`（分帧/重连/resync/pause/visibility）+ `src/ui/views/live-devices.test.ts`「选中卡片后按声明 source 订阅，并用 reading 合并、snapshot 替换」「切换选中与卸载都会关闭上一条连接」「A→B→A 时丢弃第一次 A 的过期声明结果」；vitest 22 files / 169 tests | PASS |
| E3 `/live/stream` 回归 | 全量 `cargo test --workspace --lib --bins --tests`：cloud 1113 passed / 0 failed（含 E3 live/ledger/watch/derived/alarm 套件）；E3 端点代码未改动 | PASS |
| 指标 | sampler（含 `sampling_lag_ms_total/max`）、worker、Hub stream 全部 atomics + getter（§2.1），无新依赖；runtime `sampling_lag_is_zero_on_time_and_positive_when_late`、cloud `resync_frames_increment_the_process_counter` | PASS |

## 4. 偏差与说明

1. **`sampling_lag_ms` 已实现（评审修复）**：按 `due_at = last_sample + interval` 计算每次到期 source 的调度滞后，累计 `sampling_lag_ms_total` 并维护 `sampling_lag_ms_max`；`reads_submitted` 保留为额外计数。该指标衡量 **tick 调度滞后**（`now - due_at`），不含「提交 → 读数回写」时延；后者需 ValueSink 携带提交时间戳，留 E4c/e。
2. **指标未暴露 HTTP/Prometheus 端点**：按约束只做 atomics + getter；进程级 Hub 计数为全局静态，跨多个 runtime 实例共享（单进程生产可接受）。
3. **`demand_active_sources` 为 source·tick 累计值**，不是当前活跃 source 的 gauge；宽限内也计为 active（门控仍开）。
4. **`reads_coalesced` 为尽力计数**：以「覆盖尚未被 worker 取走的 Read」为准；worker 正在执行读时的新提交会在下一次 `changed()` 处理，不计合并（它确实会产生一次新读）。存在极小窗口的近似，不影响队列策略正确性。
5. **`queue_depth` 在 worker 停止时归零**：未执行完的排队命令不会继续运行（E4c 接在途取消），指标不长期悬空。
6. **E2E 未额外组合 `AccessService::subscribe`**：跨层 E2E 已组合生产 handler 的同一入口 `subscribe_with_guard`（Hub 连接生命周期 → 同一 `DemandTracker`）；AccessService 的 TTL/快照路径另有 `e3f62061` 单测。
7. **fmt 非全局干净（存量）**：同 E4a，仅 `scheduler.rs`、`watch.rs` 两处非 E4b 漂移；未提交、未修改。
8. 工作区在验证后仅含本任务文件（`git status --short` 无格式化噪声）。

## 5. 结转 E4c-E4e

1. **per-source 读 payload 仍为 advisory**：`Read { sources }` 只决定调度时机；worker 调用 `read_data_once_with` 仍返回驱动全部值，未按 source 裁剪。按 source 精确读取/裁剪留 E4c。
2. **catalog 20Hz 成本**：sampler 每 50ms 全量 `catalog.list()`（device_cache 遍历 + manifest 克隆）；大设备量下需增量/缓存。E4c-e 性能验收时评估。
3. **sampler 状态不清理**：`last_sample` / `last_demand` / `demand_gate` 以 `(ws,thing,source)` 为键，物/驱动卸载后条目不会移除（`last_active` 同）。重启易失可自愈，长期运行需随 catalog 差集修剪。
4. **push 断开/存活语义**：全分支评审后已加 30s 周期重发（worker 被替换/重建后其空 demand watch 能恢复），但断线事件驱动的即时重放/对账仍未做；`WorkerResult::Disconnected` 与 demand 的联动留 E4c。
5. **watch control 仍留 E3**：Hub stream `control` 只含 job/state_change；watch 命中/超时继续由 `/live/stream` 账本尾随提供（spec 裁定，非缺口）。
6. **snapshot 过度记录需求**：`read_device` 未指定 properties 时会记录该物全部声明属性，可能放大 demandOnly 采样；E4c/e 按调用实际字段收窄。
7. **声明失败 UI 状态**：设备卡在驱动未声明 source 时显示离线且不建流（已有测试），但声明获取失败/重试的可见状态与手动重试留 E4e。
8. **DataServer/worker 卸载引用计数**：`remove_device`/动态驱动卸载时 worker 停止与 refcount 释放路径需在 E4c 资源仲裁中复核（E4a 已记录 `create_driver` 不释放 ref_count 的存量问题）。
9. **E4c 资源/取消**：claim/lease、busy 持有者文案、`Stop` 在途取消（ABI cancel 手柄已就绪）、`ValidatedCall.timeout_ms` 强制生效。
10. **E4d 空间**：maps/coords/zone 权威；**E4e 全套验收**：lifecycle/telemetry/tools/streaming/resources/cancel/placement/manual profile、`--interactive`、test hub。
11. **指标接线**：若后续需要运维可见性，把 sampler/worker/hub stream getter 接入现有 metrics/健康端点；当前仅测试与诊断可读。
