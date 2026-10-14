-- T15（HeartbeatRunner 退役）：DROP 孤儿视图 heartbeat_tick_map。
--
-- 该视图由 20261013000001_heartbeat_runs_backfill.sql Step 3 创建，供
-- 双跑期人工对账；无任何代码消费（Task 10 起 get_logs 直接按 run_id
-- 联结 agent_actions）。HeartbeatRunner 退役、双跑结束后不留孤儿视图。
DROP VIEW IF EXISTS heartbeat_tick_map;
