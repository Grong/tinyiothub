-- Add heartbeat_trust_config column to workspaces table
ALTER TABLE workspaces ADD COLUMN heartbeat_trust_config TEXT NOT NULL DEFAULT '';