-- Bounded Task timeline pages and runtime event-id dedupe.
--
-- The queries bind `kind` as a parameter, so the partial predicate from
-- 20261030000001 cannot be proven and was never used. These non-partial
-- expression indexes match the bound queries and stay usable for every kind.

CREATE INDEX IF NOT EXISTS idx_hub_ledger_runtime_task_page
    ON hub_ledger (workspace_id, kind, json_extract(payload, '$.taskId'), id);

CREATE INDEX IF NOT EXISTS idx_hub_ledger_runtime_event_id_full
    ON hub_ledger (workspace_id, kind, json_extract(payload, '$.eventId'));
