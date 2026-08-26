-- CEO review T22：心跳结果幂等投影的锚列。
-- HeartbeatResult.id（创建点 uuid）写入 marker 行（summary/error），
-- 部分唯一索引让事件重放/重试/resync 的重复投影被 INSERT OR IGNORE
-- 静默跳过；历史行 tick_id 为 NULL，不受索引约束。
ALTER TABLE agent_actions ADD COLUMN tick_id TEXT;
CREATE UNIQUE INDEX idx_agent_actions_tick_id ON agent_actions(tick_id) WHERE tick_id IS NOT NULL;
