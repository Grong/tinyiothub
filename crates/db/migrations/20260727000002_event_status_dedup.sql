-- Event status dedup redesign (eng-review OV-2, 2026-07-27)
--
-- The 20260723000001 dedup index spanned ALL events rows on
-- (event_type, event_subtype, device_id) — architecturally incompatible
-- with the design's append semantics for occurrence-type events: the
-- second same-subtype event for a thing violated UNIQUE and was silently
-- dropped (reported as malformed).
--
-- Fix: dedup applies ONLY to status-type rows (is_status = 1, the
-- current-state upsert path). Occurrence-type events (is_status = 0,
-- e.g. thing events from MQTT) are pure appends outside the index.

ALTER TABLE events ADD COLUMN is_status INTEGER NOT NULL DEFAULT 0;

DROP INDEX IF EXISTS idx_events_dedup;

CREATE UNIQUE INDEX idx_events_status_dedup
    ON events(event_type, event_subtype, device_id)
    WHERE is_status = 1 AND device_id IS NOT NULL;
