-- Dead-letter queue for events that failed to persist after retries.
-- Used by ActionRepo subscriber when PatrolCompleted → insert fails
-- 3 retries with exponential backoff, then lands here.
-- Cron job retries hourly within 24h window; abandoned after 24h, cleaned after 90 days.
CREATE TABLE IF NOT EXISTS lost_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_type TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    error TEXT NOT NULL,
    retry_count INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'retrying', 'abandoned', 'recovered')),
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_retry_at TIMESTAMP,
    recovered_at TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_lost_events_status_created ON lost_events(status, created_at);
