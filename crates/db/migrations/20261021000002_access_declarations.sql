-- E4a：双层声明持久化（包级 manifest + 设备级 describe 声明）与验收报告。
-- additive：只新建表；唯一改动 = thing_actions 增加 source 列（与
-- thing_properties.source 对齐，用于物化/陈旧标记）。

CREATE TABLE driver_declarations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id TEXT NOT NULL,
    driver_name TEXT NOT NULL,
    driver_version TEXT NOT NULL,
    manifest_version TEXT NOT NULL,
    manifest_json TEXT NOT NULL,
    manifest_hash TEXT NOT NULL,
    binary_hash TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    installed_at TEXT NOT NULL,
    UNIQUE(workspace_id, driver_name, driver_version)
);
CREATE INDEX idx_driver_declarations_active
    ON driver_declarations(workspace_id, driver_name, status);

CREATE TABLE device_declarations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id TEXT NOT NULL,
    thing_id TEXT NOT NULL,
    driver_name TEXT NOT NULL,
    driver_version TEXT NOT NULL,
    declaration_json TEXT NOT NULL,
    declaration_hash TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    updated_at TEXT NOT NULL,
    UNIQUE(thing_id, driver_name)
);
CREATE INDEX idx_device_declarations_ws ON device_declarations(workspace_id);

CREATE TABLE acceptance_reports (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id TEXT NOT NULL,
    driver_name TEXT NOT NULL,
    driver_version TEXT NOT NULL,
    profile TEXT NOT NULL,
    level TEXT NOT NULL,
    report_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX idx_acceptance_reports_driver
    ON acceptance_reports(workspace_id, driver_name);

ALTER TABLE thing_actions ADD COLUMN source TEXT NOT NULL DEFAULT 'template';
