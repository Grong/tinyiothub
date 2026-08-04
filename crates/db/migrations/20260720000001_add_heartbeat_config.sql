-- Per-workspace heartbeat config (JSON: {"enabled": bool, "interval_minutes": u32})
ALTER TABLE workspaces ADD COLUMN heartbeat_config TEXT NOT NULL DEFAULT '';
