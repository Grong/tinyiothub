-- Migration: Create batch command tables
-- Batch command execution with idempotency support

CREATE TABLE IF NOT EXISTS batch_commands (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    command_name TEXT NOT NULL,
    command_type TEXT NOT NULL DEFAULT 'custom',
    parameters TEXT, -- JSON string
    total_devices INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'pending',
    -- status: pending, running, completed, partial_failure, failed
    submitted_by TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    completed_at DATETIME,
    UNIQUE(workspace_id, idempotency_key)
);

CREATE INDEX idx_batch_commands_workspace_id ON batch_commands(workspace_id);
CREATE INDEX idx_batch_commands_idempotency ON batch_commands(workspace_id, idempotency_key);
CREATE INDEX idx_batch_commands_status ON batch_commands(status);
CREATE INDEX idx_batch_commands_created_at ON batch_commands(created_at);

CREATE TABLE IF NOT EXISTS batch_command_items (
    id TEXT PRIMARY KEY,
    batch_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    device_name TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    -- status: pending, sent, success, failure, timeout
    result_message TEXT,
    command_id TEXT,
    executed_at DATETIME,
    completed_at DATETIME,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (batch_id) REFERENCES batch_commands(id) ON DELETE CASCADE
);

CREATE INDEX idx_batch_command_items_batch_id ON batch_command_items(batch_id);
CREATE INDEX idx_batch_command_items_device_id ON batch_command_items(device_id);
CREATE INDEX idx_batch_command_items_status ON batch_command_items(status);
