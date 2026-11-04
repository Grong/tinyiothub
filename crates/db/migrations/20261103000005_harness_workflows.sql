CREATE TABLE agent_workflows (
    workflow_id     TEXT PRIMARY KEY,
    enabled         INTEGER NOT NULL,
    current_version INTEGER NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE TABLE workflow_revisions (
    workflow_id     TEXT NOT NULL,
    version         INTEGER NOT NULL,
    definition_json TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    PRIMARY KEY (workflow_id, version),
    FOREIGN KEY (workflow_id) REFERENCES agent_workflows(workflow_id)
);

ALTER TABLE agent_tasks ADD COLUMN workflow_id TEXT;
ALTER TABLE agent_tasks ADD COLUMN workflow_version INTEGER;
ALTER TABLE agent_tasks ADD COLUMN parent_task_id TEXT;
ALTER TABLE agent_runs ADD COLUMN parent_run_id TEXT;

CREATE TABLE workflow_occurrences (
    source_key       TEXT PRIMARY KEY,
    workflow_id      TEXT NOT NULL,
    trigger_id       TEXT NOT NULL,
    occurrence_id    TEXT NOT NULL,
    workflow_version INTEGER NOT NULL,
    task_id          TEXT NOT NULL UNIQUE,
    run_id           TEXT NOT NULL UNIQUE,
    session_id       TEXT NOT NULL,
    source_json      TEXT NOT NULL,
    created_at       TEXT NOT NULL,
    FOREIGN KEY (workflow_id, workflow_version)
        REFERENCES workflow_revisions(workflow_id, version)
);

CREATE INDEX idx_workflow_occurrences_task
    ON workflow_occurrences (task_id, run_id);
CREATE INDEX idx_workflow_occurrences_workflow
    ON workflow_occurrences (workflow_id, trigger_id, occurrence_id);

CREATE TABLE workflow_cron_cursors (
    workflow_id TEXT NOT NULL,
    trigger_id  TEXT NOT NULL,
    next_slot   TEXT NOT NULL,
    PRIMARY KEY (workflow_id, trigger_id)
);

CREATE TABLE workflow_event_cursors (
    workflow_id TEXT NOT NULL,
    trigger_id  TEXT NOT NULL,
    last_seq    INTEGER NOT NULL,
    PRIMARY KEY (workflow_id, trigger_id)
);
