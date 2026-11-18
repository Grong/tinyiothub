-- Task 19C4: Task ids are workspace-Harness facts.
--
-- The retired platform Task plane no longer writes `agent_tasks`, so the
-- foreign key on `watch_rules.owner_task_id` can never be satisfied and would
-- reject every Task-owned watch rule. Watch rules remain a platform fact
-- (they are not Task/Run/Wake facts); only the stale FK is removed. The table
-- is rebuilt through the sanctioned `<name>_new` self-copy so existing rows
-- (including owner associations) survive; DDL-only.
CREATE TABLE watch_rules_new (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    condition TEXT NOT NULL,
    persistent INTEGER NOT NULL DEFAULT 0,
    timeout_secs INTEGER NOT NULL DEFAULT 600,
    created_at TEXT NOT NULL,
    owner_task_id TEXT
);
INSERT INTO watch_rules_new (id, workspace_id, kind, condition, persistent, timeout_secs, created_at, owner_task_id)
SELECT id, workspace_id, kind, condition, persistent, timeout_secs, created_at, owner_task_id FROM watch_rules;
DROP TABLE watch_rules;
ALTER TABLE watch_rules_new RENAME TO watch_rules;
CREATE INDEX idx_watch_rules_ws ON watch_rules(workspace_id);
CREATE INDEX idx_watch_rules_owner_task ON watch_rules(owner_task_id);
