# E4b 执行记录与验证结果

- 日期：2026-10-21
- 提交范围：`190d0e86..HEAD`（16 个提交，全部在 `ai-consolidation`；Task 10 的代码与文档提交为最后两个）
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
  - Task 10（本记录所在提交）`feat(e4b): 采样/Hub 指标与确定性需求 E2E` + `docs(e4b): 执行记录与验证结果`
- 工具链：rustc 1.95.0 / cargo 1.95.0，macOS（darwin）；Node v24.14.0（web）
- 计划：`docs/superpowers/plans/2026-10-21-e4b-sampling-hub-stream.md`
- 上位 spec：`docs/superpowers/specs/2026-10-21-e4b-sampling-hub-stream-design.md`

## 1. 命令与结果

| # | 命令 | 结果 |
|---|---|---|
| 1 | `cargo fmt --all -- --check` | **失败（预期内，存量）**：仅 `crates/agent/src/runtime/thing_agent/scheduler.rs`、`crates/core/src/watch.rs` 两处既有漂移；本任务未修改、已 `git checkout` 还原。E4b 触及文件均无 fmt 漂移 |
| 2 | `cargo clippy --workspace --all-targets -- -D warnings` | 通过（exit 0，57.68s）；仅 `redis v0.25.4` future-incompat 提示，非本次改动 |
| 3 | `cargo test --workspace --lib --bins --tests` | **2254 passed / 0 failed / 4 ignored**（53 个测试二进制，exit 0）；其中 cloud 1113 passed / 1 ignored（636.73s），runtime 222 passed |
| 4 | `cargo test -p runtime --test e4b_demand_e2e` | 3 passed / 0 failed（确定性 E2E，无 sleep） |
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
| Sampler | `Sampler::metrics() -> SamplerMetrics` | 一次性快照 |
| Worker | `queue_depth` | Execute FIFO（32）中排队待处理命令数；入队先加、失败回滚，worker 取出时递减，停止时归零 |
| Worker | `reads_coalesced` | worker 忙期间覆盖未处理 Read（latest-wins）的次数 |
| Worker | `readings_dropped` | ValueSink 满/关闭导致丢弃的读数条数 |
| Worker | `WorkerHandle::{metrics,queue_depth,reads_coalesced,readings_dropped}` | 快照 + 单项 getter |
| Hub stream | `hub_stream_connections` | 进程级累计成功建立（通过校验并登记订阅）的连接数 |
| Hub stream | `hub_stream_resyncs` | 进程级累计发出的 `resync` 帧（reconnect/lagged/overflow） |
| Hub stream | `hub_stream_metrics()` / `HubStreamMetrics` | 进程级静态实例 + getter；cloud handler 在连接建立与 resync 出队时累加 |

`sampling_demand_start/stop` tracing 事件保持不变（仍只在门控状态变化时记录一次）。未引入 HubLedger 依赖，未新增账本事件（spec 标注为可选）。

### 2.2 确定性 E2E

新增 `crates/runtime/tests/e4b_demand_e2e.rs`（3 个测试，全部注入 `Instant`，0 真实等待）：

1. `hub_subscription_drives_demand_only_source_and_health_continues`：
   - fake catalog 一个物，`slow`（demandOnly poll）+ `health`（health poll）；
   - 无订阅 tick：`slow` 跳过、`health` 采样；
   - `DemandTracker.subscribe_at(slow)` 后下一 due tick：`slow + health`；
   - unsubscribe 后 19s（宽限内）：`slow` 仍采；
   - 31s（>30s 宽限）：`slow` 停采、`health` 继续；核对 active/skipped/reads 计数。
2. `push_worker_set_demand_is_health_union_demanded_and_only_on_transitions`：
   - 首 tick 下发 `["health"]`；集合不变不重发；
   - 订阅 `slow` → `["health","slow"]`；持续/宽限内不抖动；
   - 宽限结束 → 回到 `["health"]`；push 路径 `submit_read` 恒为 0。
3. `demand_metrics_match_the_push_active_set`：tick 级计数与 push 活跃集一致，push 不提交读。

E2E 直接使用真实 `DemandTracker`（`AccessService` 的 `subscribe/touch` 最终落到同一 tracker），未额外拉起 AccessService；Hub 连接生命周期另有 cloud handler 测试覆盖（见 §3）。

### 2.3 指标单测

- runtime `access::sampler::tests::metrics_track_gate_transitions_and_reads`（skipped → active、`reads_submitted` 按 source 计）。
- runtime `access::sampler::tests::demand_only_source_skips_without_demand`（追加 skipped 断言）。
- runtime `driver::worker::tests::two_reads_while_busy_coalesce_to_latest_source_set`（coalesced = 1）。
- runtime `driver::worker::tests::exec_queue_overflow_returns_busy`（队列满 depth = 32；busy 不改变深度；消费完归零）。
- runtime `driver::worker::tests::full_value_sink_does_not_block_execute`（sink 满 dropped = 1）。
- runtime `hub_stream::tests::metrics_count_connections_and_resyncs`。

## 3. E4b 退出清单证据

| 退出项 | 证据 | 结论 |
|---|---|---|
| Hub stream 连接驱动 demand → `demandOnly` 源按 interval 采样 | E2E `hub_subscription_drives_demand_only_source_and_health_continues`；cloud `next_frame_emits_snapshot_before_reading`；runtime `demand_only_source_samples_with_source_subscription` | PASS |
| 断开 30s 后停采，health 源继续 | E2E 同上（unsubscribe → 19s 仍采 → 31s 停采，health 持续）；runtime `demand_grace_keeps_sampling_then_stops`、`health_source_samples_regardless_of_demand`；cloud `dropping_the_stream_state_unsubscribes_demand`（Drop 立即 unsubscribe，TTL 兜底） | PASS |
| push 驱动 `set_demand` 起停 | E2E `push_worker_set_demand_is_health_union_demanded_and_only_on_transitions`；runtime `push_worker_gets_set_demand_only_on_changes_and_never_reads`、`push_health_source_is_always_active` | PASS |
| worker 队列策略（合并/优先/上限） | runtime `two_reads_while_busy_coalesce_to_latest_source_set`、`execute_is_served_before_pending_read`、`exec_queue_overflow_returns_busy`、`full_value_sink_does_not_block_execute`、`failure_streak_emits_read_failed_once_and_recovers` | PASS |
| Devices 工作台选中设备卡 Hub stream latest-wins | web `src/api/hub-stream.test.ts`（分帧/重连/resync/pause/visibility）+ `src/ui/views/live-devices.test.ts`「选中卡片后按声明 source 订阅，并用 reading 合并、snapshot 替换」「切换选中与卸载都会关闭上一条连接」「A→B→A 时丢弃第一次 A 的过期声明结果」；vitest 22 files / 169 tests | PASS |
| E3 `/live/stream` 回归 | 全量 `cargo test --workspace --lib --bins --tests`：cloud 1113 passed / 0 failed（含 E3 live/ledger/watch/derived/alarm 套件）；E3 端点代码未改动 | PASS |
| 指标 | 本任务 atomics + getter（§2.1），无新依赖 | PASS |

## 4. 偏差与说明

1. **`sampling_lag_ms` 未实现**：Task 10 的 Required 明确以 `reads_submitted` 替代；lag 需要记录提交→读数回写时延，worker/ValueSink 当前不回传时间戳，留 E4c/E4e。记录为已知缺口，不影响退出标准。
2. **指标未暴露 HTTP/Prometheus 端点**：按约束只做 atomics + getter；进程级 Hub 计数为全局静态，跨多个 runtime 实例共享（单进程生产可接受）。
3. **`demand_active_sources` 为 source·tick 累计值**，不是当前活跃 source 的 gauge；宽限内也计为 active（门控仍开）。
4. **`reads_coalesced` 为尽力计数**：以「覆盖尚未被 worker 取走的 Read」为准；worker 正在执行读时的新提交会在下一次 `changed()` 处理，不计合并（它确实会产生一次新读）。存在极小窗口的近似，不影响队列策略正确性。
5. **`queue_depth` 在 worker 停止时归零**：未执行完的排队命令不会继续运行（E4c 接在途取消），指标不长期悬空。
6. **E2E 未额外组合 `AccessService::subscribe`**：`AccessService` 的订阅最终写入同一 `DemandTracker`，直接测试真实 tracker 已覆盖退出语义；AccessService 的 TTL/快照路径另有 `e3f62061` 单测。
7. **fmt 非全局干净（存量）**：同 E4a，仅 `scheduler.rs`、`watch.rs` 两处非 E4b 漂移；未提交、未修改。
8. 工作区在验证后仅含本任务文件（`git status --short` 无格式化噪声）。

## 5. 结转 E4c-E4e

1. **per-source 读 payload 仍为 advisory**：`Read { sources }` 只决定调度时机；worker 调用 `read_data_once_with` 仍返回驱动全部值，未按 source 裁剪。按 source 精确读取/裁剪留 E4c。
2. **catalog 20Hz 成本**：sampler 每 50ms 全量 `catalog.list()`（device_cache 遍历 + manifest 克隆）；大设备量下需增量/缓存。E4c-e 性能验收时评估。
3. **sampler 状态不清理**：`last_sample` / `last_demand` / `demand_gate` 以 `(ws,thing,source)` 为键，物/驱动卸载后条目不会移除（`last_active` 同）。重启易失可自愈，长期运行需随 catalog 差集修剪。
4. **push 断开/存活语义**：push 驱动断连时 sampler 仅按健康 source 的 `set_demand` 维持集合，未做断线重连后的需求重放/对账；`WorkerResult::Disconnected` 与 demand 的联动留 E4c。
5. **watch control 仍留 E3**：Hub stream `control` 只含 job/state_change；watch 命中/超时继续由 `/live/stream` 账本尾随提供（spec 裁定，非缺口）。
6. **snapshot 过度记录需求**：`read_device` 未指定 properties 时会记录该物全部声明属性，可能放大 demandOnly 采样；E4c/e 按调用实际字段收窄。
7. **声明失败 UI 状态**：设备卡在驱动未声明 source 时显示离线且不建流（已有测试），但声明获取失败/重试的可见状态与手动重试留 E4e。
8. **DataServer/worker 卸载引用计数**：`remove_device`/动态驱动卸载时 worker 停止与 refcount 释放路径需在 E4c 资源仲裁中复核（E4a 已记录 `create_driver` 不释放 ref_count 的存量问题）。
9. **E4c 资源/取消**：claim/lease、busy 持有者文案、`Stop` 在途取消（ABI cancel 手柄已就绪）、`ValidatedCall.timeout_ms` 强制生效。
10. **E4d 空间**：maps/coords/zone 权威；**E4e 全套验收**：lifecycle/telemetry/tools/streaming/resources/cancel/placement/manual profile、`--interactive`、test hub。
11. **指标接线**：若后续需要运维可见性，把 sampler/worker/hub stream getter 接入现有 metrics/健康端点；当前仅测试与诊断可读。
