# AI 子系统归一 — Phase 2 测试分类清单(Task 2)

> 分类判据:
> - **契约保** = 行为契约测试。Phase 2 内核归一(心跳 runner 并入统一 Run 循环、落库从 `agent_actions` 改向 `agent_runs`)后必须仍绿,断言不变。
> - **实现细节重写** = 直接断言 `agent_actions` 落库细节(表名、action_type 行形态、content JSON 字段、tick_id 列)的测试。落库改向后允许重写断言;其守护的行为语义(幂等、审批可查、trace 不截断等)须以新断言等价保留。

## 1. `crates/db/src/heartbeat.rs`

| 测试函数 | 分类 | 理由 |
|---|---|---|
| tests::workspace_config_roundtrips_json | 契约保 | WorkspaceHeartbeatConfig JSON 序列化往返,纯值类型,不触 agent_actions |
| tests::workspace_config_empty_is_none | 契约保 | 空串/None → None 语义,不触 agent_actions |
| tests::workspace_config_rejects_sub_minimum_interval | 契约保 | 最小间隔校验,纯值逻辑 |
| tests::replace_all_replaces_task_set | 契约保 | heartbeat_tasks 表 CRUD 契约,与落库改向无关 |
| tests::insert_result_roundtrips_through_log_and_approval_queries | 实现细节重写 | 直接查 `agent_actions` 断言 summary/auto_executed/proposal 三行形态与 content JSON 字段(taskCount/result/tool/thingId/proposalId/status/risk);并复刻 get_logs/update_proposal_status 的读查询。锚行/提案共存语义改向后须以 agent_runs 断言等价保留(审批按 proposalId 可查是契约) |
| tests::insert_result_persists_proposal_parameters | 实现细节重写 | 直接查 agent_actions 的 proposal 行 content 断言 parameters 回读;审批后取参执行是契约,落库行形态是实现细节 |
| tests::proposal_row_carries_tick_id | 实现细节重写 | 锚行/提案共存测试:断言 agent_actions 三类行同 tick_id 共存 + 锚行 content 携带全量 trace(不截断是契约,行形态是实现细节) |
| tests::insert_result_error_status_writes_error_row | 实现细节重写 | 断言 agent_actions 单行 action_type='error' 且 content.error 等于错误消息;error 可见性是契约,行形态是实现细节 |
| tests::heartbeat_config_save_and_load_roundtrip | 契约保 | workspaces.heartbeat_config 列读写往返,与 agent_actions 无关 |
| tests::save_trust_config_persists_to_workspace_column | 契约保 | 信任配置写路径契约(含 fencing 时间戳维护) |
| tests::load_trust_config_reads_workspace_column | 契约保 | 信任配置读路径契约(空列/缺行 = None) |
| tests::fenced_save_rejects_stale_occurred_at | 契约保 | fencing upsert 契约:旧事件不覆盖新配置(CEO review T2) |
| tests::fenced_save_first_write_and_missing_workspace | 契约保 | fencing 边界契约:NULL 首写成功、缺行 Ok(false) |
| tick_id_tests::insert_result_with_tick_id_is_idempotent | 实现细节重写 | 幂等契约本身须保留,但断言直接数 agent_actions 的 tick_id/action_type 行;改向后按 agent_runs 重写断言(T22 语义:重复 tick 整组跳过) |
| tick_id_tests::error_anchor_with_tick_id_is_idempotent | 实现细节重写 | 同上:error 锚行幂等断言绑定 agent_actions tick_id 列 |
| corrupt_row_tests::corrupt_trust_config_row_yields_none_loudly | 契约保 | 损坏信任配置行响亮回退默认(F10/T11),与落库改向无关 |

小计:契约保 10,实现细节重写 6。

## 2. `apps/cloud/src/tests/agent_persist_tests.rs`

| 测试函数 | 分类 | 理由 |
|---|---|---|
| projects_run_recorded_to_agent_runs | 契约保 | RunRecorded → agent_runs 投影契约,已是改向后的目标表 |
| stale_event_does_not_overwrite_newer_row | 契约保 | insert-once 幂等即 fencing 契约(agent_runs) |
| lagged_subscriber_resyncs_from_dump_state | 契约保 | Lagged → dump_state 全量 resync 契约(runs + trust config) |
| periodic_reconcile_projects_dump_state | 契约保 | 周期对账投影契约(agent_runs) |
| projects_heartbeat_result_to_agent_actions | 实现细节重写 | 直接断言 HeartbeatResultReady → agent_actions 行数;落库改向后此投影目标变 agent_runs,断言重写 |
| resync_escalates_to_dlq_after_consecutive_failures_then_recovers | 契约保 | resync 失败升级 error!+DLQ 阈值契约;失败注入点是 drop agent_runs(目标表不变) |
| supervisor_restarts_panicked_subscriber_and_exits_on_shutdown | 契约保 | 监管重启+关停退出契约(T1),不触落库表 |
| resync_restores_dedup_metadata_on_missing_rows | 契约保 | resync 恢复 problem_key/dedup_key 契约(T3),断言 agent_runs 列 |
| resync_without_sidecar_meta_falls_back_to_null_keys | 契约保 | 旁路元数据缺失回退 None 契约 |
| supervisor_restarts_after_unexpected_clean_exit | 契约保 | 意外干净退出同样重启的契约 |
| supervisor_does_not_restart_when_panic_happens_during_shutdown | 契约保 | 关停语境 panic 不重启契约 |
| trust_config_event_fencing_at_projection_level | 契约保 | TrustConfigChanged 事件级 fencing 契约(stale 拦截/新事件写入/远未来守卫) |
| shutdown_drains_buffered_events | 契约保 | 关停排空契约(OV3),断言 agent_runs |
| projects_dlq_entry_added_to_agent_dead_letters | 契约保 | DlqEntryAdded → agent_dead_letters 投影契约(T17) |
| resync_recovers_heartbeat_results_idempotently | 实现细节重写 | tick 幂等契约(T22)须保留,但断言直接数 agent_actions tick_id 行;改向后重写为 agent_runs 断言 |
| heartbeat_retry_aborted_by_shutdown_goes_to_dlq | 实现细节重写 | DLQ 行为断言(agent_dead_letters)保留,但失败注入依赖「心跳落库 = agent_actions」(DROP TABLE agent_actions 制造失败);改向后注入点须随目标表调整,否则测试空转超时 |

小计:契约保 13,实现细节重写 3。

## 3. `apps/cloud/src/tests/heartbeat_bridge_tests.rs`

(实际路径:`apps/cloud/src/tests/heartbeat_bridge_tests.rs`,经 `tests/mod.rs:26` 挂载;crates/agent 内的 `heartbeat_bridge` 引用是 HeartbeatBridge 实现本体,非测试。)

| 测试函数 | 分类 | 理由 |
|---|---|---|
| outcome_matrix_against_in_memory_dedup | 契约保 | O11 全 outcome 抑制/放行矩阵,纯内存 RunRegistry + mock sink,不触 DB;dedup 语义是归一后必须保持的行为契约 |
| acted_unverified_retry_only_once_in_memory | 契约保 | acted+未 verified 窗口内仅放行一次契约 |
| recurrence_beyond_6h_dispatches_in_memory | 契约保 | 超 6h 复发放行契约 |
| ack_suppression_windows_in_memory | 契约保 | ack 抑制 7 天三档契约(6h 内/6h 外/超 7 天) |
| no_proposals_dispatches_nothing_in_memory | 契约保 | 无提案不投递契约 |
| heartbeat_directive_shape_in_memory | 契约保 | 心跳 directive 形态契约(O5/O24):Normal 优先级、source=heartbeat、problem_key 随指令、dedup_key=None 不参与合并 |

小计:契约保 6,实现细节重写 0。

## 汇总

| 文件 | 契约保 | 实现细节重写 |
|---|---|---|
| crates/db/src/heartbeat.rs | 10 | 6 |
| apps/cloud/src/tests/agent_persist_tests.rs | 13 | 3 |
| apps/cloud/src/tests/heartbeat_bridge_tests.rs | 6 | 0 |
| **合计** | **29** | **9** |

## 争议点 / 判据应用说明

1. **幂等类测试(tick_id_tests ×2 + resync_recovers_heartbeat_results_idempotently)**:幂等是行为契约,但现有断言逐字绑定 agent_actions 的 tick_id/action_type 行形态,改向后必然变红。按判据「直接断言 agent_actions 落库细节」归实现细节重写;重写时幂等语义(同 tick 整组跳过)必须以新断言等价保留,不允许删测试。
2. **heartbeat_retry_aborted_by_shutdown_goes_to_dlq**:断言目标(agent_dead_letters)不随改向变化,看似契约保;但失败注入 fixture(DROP TABLE agent_actions)绑定旧落库目标,改向后测试会空转超时而非静默变绿——归实现细节重写,但只需改注入点,DLQ 断言原样保留。
3. **heartbeat_directive_shape_in_memory 等 bridge 测试**:按判据(不触 agent_actions)全部契约保。若后续 Task(T15 HeartbeatRunner 退役 / persist 心跳支路)连 HeartbeatBridge 本体一并重构,这些测试应随被测单元迁移/更名而非删除——O11 dedup 矩阵与 directive 形态是产品级行为契约。
4. **insert_result_roundtrips_through_log_and_approval_queries**:既是锚行/提案共存测试,也复刻了 get_logs 与审批读查询。T10(get_logs 读侧切换)改读查询时本测试的查询副本同步重写;「审批按 proposalId 找到提案」是跨改向的硬契约。
