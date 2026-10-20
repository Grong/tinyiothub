-- E1 值守中枢：watch 规则持久源 + Hub 账本（spec 2026-10-20-e1 §4）
CREATE TABLE watch_rules (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    kind TEXT NOT NULL,               -- property | health | availability
    condition TEXT NOT NULL,          -- JSON，词表版本见 payload.v
    persistent INTEGER NOT NULL DEFAULT 0,
    timeout_secs INTEGER NOT NULL DEFAULT 600,
    created_at TEXT NOT NULL
);
CREATE INDEX idx_watch_rules_ws ON watch_rules(workspace_id);

CREATE TABLE hub_ledger (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id TEXT NOT NULL,
    ts TEXT NOT NULL,                 -- RFC3339
    kind TEXT NOT NULL,               -- state_change|watch_hit|watch_timeout|watch_expired|job|rate_limited
    thing_id TEXT,
    payload TEXT NOT NULL             -- JSON，含 v=1 词表版本（P5）
);
CREATE INDEX idx_hub_ledger_ws_ts ON hub_ledger(workspace_id, ts);
