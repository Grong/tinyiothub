-- Durable finalize-pending state for a Task Run whose model work finished but
-- whose teardown could not complete. Staged atomically with the wake requeue so
-- the router can retry persistence/emission/terminal transition without
-- re-executing the model or resubmitting device actions.
CREATE TABLE IF NOT EXISTS task_finalize_intents (
    task_id TEXT NOT NULL REFERENCES agent_tasks(id) ON DELETE CASCADE,
    workspace_id TEXT NOT NULL,
    wake_event_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    report_json TEXT NOT NULL,
    report_persisted INTEGER NOT NULL,
    report_emitted INTEGER NOT NULL,
    undelivered_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (task_id, wake_event_id)
);

CREATE INDEX IF NOT EXISTS idx_task_finalize_intents_workspace
ON task_finalize_intents(workspace_id, task_id);
