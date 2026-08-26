# SDD ledger — plan: docs/superpowers/plans/2026-08-18-db-crate-overhaul.md
Task 1: 实现者 stall（更名完成未提交，db 84 测试绿）；resume 收尾
Task 1: resume 二次 stall；派全新 opus 收尾（只验证+提交+报告）
Task 1: note — pre-existing flaky test thing_agent_loop_tests.rs:604（并行负载下 2/4 失败，隔离跑通过；与本更名无关；建议后续加固为轮询断言）
Task 1: minor (deferred): 局部变量命名不一致（少数 database 局部绑定未改 db，无害）
Task 1: complete (commits 88791b2a..9c42c806, review clean; 2 stall + opus 收尾)
Task 2: parked — fresh 库在 Task 2→Task 3 之间 bootstrap FK 失败（initialization.rs:184 ensure_default_admin_user）— ruling: 计划内排序代价，单分支不合并不部署，Task 3 紧随消解
Task 2: minor (deferred): test_utils.rs:148-158 注释描述已删补丁层（stale comment，Task 3 顺带修）
Task 2: minor (deferred): 存量库 VersionMissing 报错文案误导（Q2 已 sanction 删库重建，文案可改进）
Task 2: complete (commits 9c42c806..11a18b4d, review clean — schema diff 双层验证为空)
Task 3: minor (deferred): seed_system 幂等测试只断言 workspaces 计数（plans/templates/admin 未覆盖，SQL 结构性幂等）
Task 3: minor (deferred): main.rs:85 种子失败 fail-fast（与旧行为等价，有意为之）
Task 3: minor (deferred): app_settings.toml gitignored，[seed] 节仅入 example（serde default 兜底）
Task 3: complete (commits 11a18b4d..bae40691, review clean)
Task 4: minor (deferred): jobs/handler.rs list_job_executions 用 query param 而非 path id（pre-existing 潜在 bug，转换如实保留；建议后续修）
Task 4: recipe 实况（Task 5-12 参照）：struct 字段名多为 db（非 database）；方法体 accessor 为 self.db.pool() 变体；crate 包名 db（cargo test -p db）
Task 4: complete (commits bae40691..fbea4f63, review clean) — 试点配方成立
Task 5: minor (deferred): history.rs/types.rs 两处注释过时（数据实现已迁 db，注释仍写"留 cloud"）——随 Task 12 清理
Task 5: complete (commits fbea4f63..efebc7f5, review clean)
Task 6: minor (deferred): 报告方法数笔误（9 vs 10，代码无影响）
Task 6: minor (deferred): tag.rs binding_* 前缀未含 tag 限定（理论碰撞风险，与配方一致）
Task 6: note — access_control.rs 裸 SQL 经审查证实为 legacy schema 路径，归 Task 12 收编
Task 6: complete (commits efebc7f5..ff0eb35d, review clean)
Task 7: 实现者探索期 stall（零产出）；重派并给硬性入场序列（逐步 inventory→转换→编译小循环）
Task 7: minor (deferred): auth.rs 返回 sqlx::Error 与其他领域 core::Result 双轨（有声明理由，可后续统一）
Task 7: pointer for Task 13: social.rs:454,506 的 get_social_config/get_wechat_config 走 Db::query 通用助手——Task 13 删助手时须补建 auth.rs 的 social_configs SELECT 领域函数（当前只收编了 UPDATE 侧）
Task 7: complete (commits ff0eb35d..9a09ce63, review clean; 1 次 stall 重派)
Task 8: fix round 1/5 派发（1 Important: test_criteria_builder 断言弱化——device_type/driver_name 两断言被丢）
Task 8: minor (deferred): query_service_impl.rs 4 个 QueryBuilder 动态 SQL（QuickDevice 等 cloud 本地类型）——登记为 Task 12/后续专项收编
Task 8: minor (deferred): device.rs:1378 冗余自引用 import（下次触文件顺手清）
Task 8: fix round 1/5 (1 addressed, 0 open; commits e9232d8b..885f7df5)
Task 8: complete (commits 9a09ce63..885f7df5, review clean after fix round)
Task 9: minor (deferred): heartbeat/agent_runs/policy 自由函数为模块私有而非配方的 pub(crate)（可见性更严，无行为影响；Task 14 统一收紧/放宽口径时处理）
Task 9: complete (commits 885f7df5..40253f13, review clean)
Task 10: 歧义抉择 — notification_channel.rs 保持 pub 自由函数（brief 仅要求核实形态），仅删 lib.rs glob 再导出；notify/handler.rs 改模块路径导入
Task 10: note — event.rs 现含两条语义相同的 occurrence 删除（cleanup_old_realtime_events vs delete_occurrence_events_before，SQL 子句顺序/参数类型不同，按 brief 逐字保留未合并；Task 13/14 可考虑归一）
Task 10: complete (commit 75af78f9; 4 struct grep 零命中，thing_agent_host 非测试裸 SQL 零命中，db/agent/event_retention 测试全绿)
Task 10: minor (deferred): event.rs 的 realtime 系方法前缀不统一（upsert_event_status 等用 event 系命名，无碰撞）
Task 10: minor (deferred): delete_occurrence_events_before 与 cleanup_old_realtime_events 语义近重复（有意临时重复，Task 13 后可收敛）
Task 10: complete (commits 40253f13..75af78f9, review clean)
Task 11: 实现者 stall（零产出，工作区干净）；同 brief 重派（硬性入场序列）
Task 11: fix round 1/5 派发（1 Important: alarm.rs 通配摆渡再导出违反硬规则须删，改消费方；4 Minor: fmt 未回退/mod 排序/缩进/残留注释）
Task 11: fix round 1/5 (5 addressed, 0 open; commits 8c5ef49e..c647ff85)
Task 11: complete (commits 75af78f9..c647ff85, review clean after fix round; 1 次 stall 重派)
Task 12: complete (4 cloud struct 转换 + 23 个生产文件裸 SQL 收编；policy_engine.rs 按 brief 排除并有依赖不变量据；check/tests 全绿，详见 task-12-report.md)
Task 12: minor (deferred): admin/open get_thing 的 NOT NULL 列解码失败语义从 500 变缺省值（现实不可达，已披露）
Task 12: minor (deferred): summary.rs SQL 字符串排版差异（语义逐字等价）
Task 12: complete (commits c647ff85..b70703d5, review clean)
Task 13: minor (deferred): ci.yml sed 截断守门有结构性盲区（mid-file cfg(test) 后的生产代码不可见——当前树无此形态，YAML 注释已认知）
Task 13: minor (deferred): social.rs:485 wechat 映射 FromRow 严格化（畸形行从容错变 None，仅影响畸形数据，报告未披露但审查已记录）
Task 13: minor (deferred): edge.rs 拥有 offline_buffer/config_meta 领域但不管建表（生产 edge 无 CREATE TABLE——pre-existing，仅测试建表；建议后续补 ensure）
Task 13: complete (commits b70703d5..9c6a878e, review clean — 范围扩大经审查判定为必要前提)
Task 14: complete (commits 9c6a878e..b9bef394, review clean) — 全部 14 任务完成
FINAL REVIEW: MERGE-READY（opus，16 commits；1 Important + 6 Minor/Info）
终审 Important: QueryBuilder/raw_sql 守门盲区（query_service_impl.rs 4 处生产动态 SQL 对守卫不可见）
终审 Minor: state.rs:780 AgentPoolLifecycle 持裸 pool 与 AGENTS.md 规则 1 张力（cosmetic）；错误类型三轨（sqlx::Error 主导/core::Result/DbError，统一另立项）；lib.rs 陈旧文档；Task 12 漏清的"留 cloud"注释；守门仅扫 cloud+edge（cli/marketplace 未扫，当前干净）
终审修复波已派发（3 项：QueryBuilder 收编+守门扩展 / 过时注释 / lib.rs 文档）
终审修复波：两次 stall（疑似长跑 cargo 静默触发 600s watchdog）；三派加硬性规则——长 cargo 命令必须 run_in_background+轮询
终审修复波: complete (commits b9bef394..0e2b7a26, scoped re-review clean — 3/3 addressed)
FINAL: 14/14 tasks + final review + fix wave 全部完成
