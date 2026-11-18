-- Task 14 (Spec T17): one mutable Supervisor health row plus the durable
-- dead-wake recovery facts. Normal scheduler ticks only advance this row and
-- never append permanent events.
CREATE TABLE supervisor_state (
    id                INTEGER PRIMARY KEY CHECK (id = 1),
    tick_seq          INTEGER NOT NULL DEFAULT 0,
    last_started_at   TEXT,
    last_completed_at TEXT,
    last_error        TEXT,
    stalled_reason    TEXT
);

INSERT INTO supervisor_state (id, tick_seq) VALUES (1, 0);

-- Dead-wake rows keep the original event identity. The failure reason is
-- written when the wake is marked dead; retry audit stays on the same row.
ALTER TABLE task_wake_outbox ADD COLUMN failure_reason TEXT;
ALTER TABLE task_wake_outbox ADD COLUMN retried_by TEXT;
ALTER TABLE task_wake_outbox ADD COLUMN retried_at TEXT;
