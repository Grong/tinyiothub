-- Covering index for workspace-scoped event queries (pre-landing perf review).
-- list_all_events, the real-time filter, and open-API event endpoints all
-- filter WHERE workspace_id = ? ORDER BY created_at DESC; the status-dedup
-- partial index does not cover these scans.
CREATE INDEX IF NOT EXISTS idx_events_workspace_created
    ON events(workspace_id, created_at DESC);
