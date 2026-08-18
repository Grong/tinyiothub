# SDD ledger — plan: docs/superpowers/plans/2026-08-15-agent-crate-extraction.md
Task 1: minor (deferred): core/lib.rs 守门注释未涵盖新迁入的序列化助手（format_summary/from_db 等）
Task 1: minor (deferred): core/policy.rs 区块注释位置与原始略有差异（cosmetic）
Task 1: complete (commits b2c835b2..9dc9db84, review clean)
Task 2: complete (commits 9dc9db84..89285a10, review clean)
Task 3: implementer stalled (watchdog, 600s no progress); uncommitted partial work kept (runtime.rs 413 行可编译)；重新派发 opus
Task 3: minor (deferred): update_heartbeat_interval 内联了 handler 变体逻辑（runtime.rs:192-218，Task 5 收敛）
Task 3: minor (deferred): 未知 workspace 时 facade 状态与 repo 短暂不一致（过渡语义）
Task 3: minor (deferred): runtime.rs:173 锁中毒 unwrap panic（风格一致性）
Task 3: minor (deferred): "先 subscribe 再 restore" 注释不可字面实现——bus 在 restore 内新建；Task 9 接线时需改为 bus 经 RuntimeDeps 注入或 restore 缓冲事件
Task 3: complete (commits 89285a10..a9451c4d, review clean — 4 minors deferred; stall-retry 1 次)
Task 4: minor (deferred): 窗口截断会漏 SQL 全历史命中（方向=少报非误报，等价性论证成立，已文档化）
Task 4: minor (deferred): manager.rs report 被 clone 两次（每 run 一次，代价可忽略）
Task 4: minor (deferred): 测试探针 drain_events 遇 Lagged 静默停（容量 64，现实无风险）
Task 4: minor (deferred): prewarm 输入顺序契约仅靠注释——snapshot.recent_runs 需补交叉引用注释（Task 9 注意）
Task 4: parked — O11 dedup 过渡期断源（HeartbeatBridge 仍读 agent_runs 表）— ruling: 计划内排序空隙，Phase 1 规定单一 PR 合入，中间态不部署；Task 6 将 HeartbeatBridge 读改 registry，Task 8 恢复表写路径
Task 4: complete (commits a9451c4d..463eb61e, review approved with conditions; 实现者中段死亡，controller 代验 170 测试绿)
Task 5: implementer stalled mid-edit（11 编译错误遗留）；resume 原实现者修复并收尾
Task 5: fix round 1/5 派发（2 Important: WorkspaceDeleted 内存泄漏面 + update_tasks 回读吞错；2 Minor: 日志级别、Default 测试夹具）
Task 5: fix round 1/5 (4 addressed, 0 open; commits e660ff0b..4d102722)
Task 5: complete (commits 463eb61e..4d102722, review clean after fix round)
Task 6: fix round 1/5 派发（1 Important: ack 身份塌缩致可达场景过度抑制；1 Minor: occurred_at 单调假设）
Task 6: minor (deferred): idle problem_meta 条目不裁剪（key 基数有界，行为正确）
Task 6: pointer for Task 8: 验收须含重建 persist 重试/DLQ/HeartbeatPersistFailed 语义（原：2s 起指数退避 5 次 → DLQ + 事件；shutdown 排空）
Task 6: pointer for Task 9: snapshot 构建器需从 agent_runs 表构建 dedup 元数据段（problem_key/outcome/verified/acked_at/created_at），否则重启后 dedup 映射为空
Task 6: fix round 1 实现者二次 stall（测试已绿未提交）；resume 收尾
Task 6: fix round 1/5 (2 addressed, 0 open; commits e033efc6..4e3bb38b)
Task 6: complete (commits 4d102722..4e3bb38b, review clean after fix round; 实现者 2 次 stall)
Task 7: fix round 1/5 派发（1 Important: pool.rs/chat.rs/config.rs 残余 Option<SqlitePool> 透传参数违反零 sqlx 约束）
Task 7: minor (deferred): pool_adapter.rs:37-42 缓存命中也查 config（每心跳多一次 DB 查询）
Task 7: minor (deferred): proxy.rs:65-78 config 查询移到鉴权前（错误文案/时序位移，无安全影响）
Task 7: fix round 1/5 (1 addressed, 0 open; commits e7009a95..a91be673)
Task 7: complete (commits 4e3bb38b..a91be673, review clean after fix round)
Task 8: implementer stalled in exploration（无产出）；重派 opus 并加范围纪律
Task 8: opus 二派仍 stall（接口确认后、写测试前）；controller 亲自核实全部接口事实写 task-8-context.md（含 insert_run 裸 INSERT 需幂等包装、retry 语义逐字恢复），三派 sonnet
Task 8: fix round 1/5 派发（1 Important: resync 单项失败无升级路径；1 Minor: retry 未接 shutdown 加 TODO）
Task 8: minor (deferred): UNIQUE 字符串匹配判定唯一冲突（sqlite 有效但脆弱）
Task 8: minor (deferred): TrustConfigChanged 无 fencing（workspaces 表无 updated_at，机制性限制）
Task 8: minor (deferred): DlqEntryAdded 投影失败无重试（瞬时失败即丢 DLQ 记录，仅留日志）
Task 8: fix round 1/5 (2 addressed, 0 open; commits 2e51b2e6..703174f0)
Task 8: complete (commits a91be673..703174f0, review clean after fix round; 2 次 stall + 1 次重派)
Task 9: implementer stalled mid-fix（测试逻辑错误修复中，10 文件未提交）；resume 收尾
Task 9: 实现者二次 stall；工作区编译干净但未提交；派全新 opus 收尾（验证 5 指针 + 修测试 + 提交 + 报告）
Task 9: minor (deferred): repo 单测手工 ALTER TABLE（test_pool 只应用部分迁移，迁移/测试两处定义会漂移）
Task 9: minor (deferred): agent_startup_tests "owned" 语义——status='running' 且有 report 的行永不收敛（生产不可达，语义怪异非 bug）
Task 9: minor (deferred): AppState.agent_hooks 字段不再被读取（pub 无告警，留统一清理）
Task 9: minor (deferred): interrupt_zombie_running_runs 为 SELECT+逐行 UPDATE（启动一次、行数有界、幂等）
Task 9: D14 用户裁决——保留 status 列迁移与 zombie reconcile（当前 insert-once 下 inert 但无害；spec 待补注）
Task 9: complete (commits 703174f0..c37c5dd4, review approved; 实现者 2 次 stall，opus 收尾)
Task 10: minor (deferred): E2E 的 DB 环只断言行存在，字段级一致性靠 HTTP 环传递覆盖（可加强）
Task 10: complete (commits be13d5c9..8b9c5109, review clean) — Phase 1 完成
Task 11: D15 用户裁决——斩断 runtime→db（cron_executors/data_server 存储访问端口化），本期做；新增 Task 11.5，tree 守卫届时升级全树
Task 11: fix round 1/5 派发中 stall（编辑完成未自证未提交）；resume 收尾
Task 11: fix round 1/5 (3 addressed, 0 open; commits 79cf6f23..9352e58d)
Task 11: complete (commits 8b9c5109..9352e58d, review clean after fix round)
Task 11.5: minor (deferred): cron_executors.rs:67 闭包缩进格式（纯格式）
Task 11.5: complete (commits 2d4bd50d..b4ac6db7, review clean) — runtime→db 斩断，tree 守卫全树化
Task 12: minor (deferred): memory/service.rs 部分调用用全限定路径而非顶部 import（风格不一致，minimal-diff 选择）
Task 12: minor (deferred): memory/Cargo.toml 的 sqlx/async-trait/anyhow 仅测试使用仍在 [dependencies]（pre-existing，保持 surgical）
Task 12: complete (commits b4ac6db7..1796ba34, review clean)
Task 13: minor (deferred): manager.rs:~725 过时测试注释（SQLite 夹具已改内存桩，注释理由失效）
Task 13: minor (deferred): rustfmt reflow 混入 rename diff（与 fmt-drift 披露一致，无害）
Task 13: minor (deferred): event/bus.rs ThingEventSignal pub use 判定为模块 API 非 shim；可选后续让 router.rs 直引 core
Task 13: complete (commits 1796ba34..4b57f819, review clean)
Task 14: 实现者 API 流超时中断（mid-move，cloud 25 错/agent 1 错）；模板 include_str! 断裂——指导 crate 内嵌+pub const 导出收口，resume 继续
Task 14: complete (commits 4b57f819..23a3e6a2, review clean; 实现者 1 次流超时中断后 resume 完成) — 4 minors 转 Task 15 处理（见 Task 15 派发上下文）
Task 15: 实现者 stall（搬空模块检查中，12 文件未提交）；resume 收尾。注：用户在分支上并行提交了 2 个 web commit（7fdf6393/d477eaed），无冲突
Task 15: minor (deferred): loop_.rs:559 空任务唤醒不记 tick（与 brief 字面略有出入，语义可辩护）
Task 15: minor (deferred): AGENTS.md:41 memory 行 Forbidden 未提 agent 依赖（文档精度）
Task 15: minor (deferred): lastTick 无 skip_serializing_if（null 语义已注释声明）
Task 15: complete (commits 7fdf6393..15d69565, review clean) — 全部 15+1 任务完成
FINAL REVIEW: MERGE-READY（opus 全分支终审，25 commits；0 Critical/Important；5 Minor 全部 FINE-TO-DEFER）
终审建议后续小清理 PR：① DlqEntryAdded 无生产者（死契约臂）② facade 命令无生产调用方致 TrustConfigChanged 休眠 ③ registry 无 per-workspace 移除 ④ resync 插入 problem_key=None ⑤ spec 未记 D15
