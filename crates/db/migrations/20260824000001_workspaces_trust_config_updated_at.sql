-- CEO review T2：TrustConfigChanged 事件的 DB fencing 时间戳列。
-- 事件路径 UPDATE 携带 `heartbeat_trust_config_updated_at <= occurred_at` 守卫，
-- 乱序/回放的旧事件无法覆盖 handler 先写路径的更新配置（spec events.rs 的
-- occurred_at fencing 契约此前只有注释没有实现）。
ALTER TABLE workspaces ADD COLUMN heartbeat_trust_config_updated_at TEXT;
