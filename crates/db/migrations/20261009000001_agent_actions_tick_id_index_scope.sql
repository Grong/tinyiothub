-- AI 运行透明化 Task 9：tick_id 唯一索引收窄到 anchor 行。
-- 原索引（20260824000002）是 WHERE tick_id IS NOT NULL 的部分唯一索引——
-- 本任务起提案/auto_executed 行与 anchor 行共享同一 tick_id（巡检提案的
-- 事后轨迹链接键），继续全域唯一会让提案行插入撞 UNIQUE 约束。
-- 重建为仅 anchor 行唯一：summary/error 两类都是 tick 锚行（均经
-- insert_tick_anchor_row 以 INSERT OR IGNORE 幂等写入，CEO review T22），
-- 谓词必须两者都含——只含 'summary' 会让 error tick 的重复投影失去约束
-- 保护而重复落行。提案/动作行放行。
--
-- 逆迁移（回到 20260824000002 形状；前提：非 anchor 行的 tick_id 已清空，
-- 否则 CREATE UNIQUE INDEX 会因重复值失败）：
--   DROP INDEX IF EXISTS idx_agent_actions_tick_id;
--   CREATE UNIQUE INDEX idx_agent_actions_tick_id ON agent_actions(tick_id) WHERE tick_id IS NOT NULL;
DROP INDEX IF EXISTS idx_agent_actions_tick_id;
CREATE UNIQUE INDEX idx_agent_actions_tick_id ON agent_actions(tick_id)
    WHERE tick_id IS NOT NULL AND action_type IN ('summary', 'error');
