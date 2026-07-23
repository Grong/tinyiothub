-- Thing Ontology Rebuild Migration
-- Foundation migration for the Thing Ontology mega-branch.
-- Orchestrates 6 groups of SQLite table rebuilds in a single migration
-- (sharing the defer_foreign_keys context).
--
-- Order of operations:
--   0a. Drop products table
--   0b. device_templates → thing_templates rebuild
--   1.  devices table Thing generalization (heaviest rebuild)
--   2.  tags table rebuild
--   3.  resources → thing_resources rebuild
--   4.  events table changes
--   5.  Delete dead event tables
--   6.  workspaces add column
--   7.  device_alarm_rules — ensure rule_type CHECK covers 'event'
--   8.  Final PRAGMA foreign_key_check

PRAGMA defer_foreign_keys = ON;

-- ============================================================================
-- 0a. Delete products table
-- ============================================================================
-- Products was a hollow model (6 rows, no workspace_id). It is superseded by
-- thing_templates. The devices rebuild below removes the product_id FK.
DROP TABLE IF EXISTS products;

-- ============================================================================
-- 0b. device_templates → thing_templates rebuild
-- ============================================================================
-- New columns: thing_type, actions (rename of commands), events, default_knowledge.
-- Name unique constraint: global UNIQUE → expression index on
-- (COALESCE(workspace_id, ''), name).

CREATE TABLE thing_templates_new (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    display_name TEXT NOT NULL,
    description TEXT,
    version TEXT NOT NULL,
    author TEXT,
    category TEXT NOT NULL,
    manufacturer TEXT,
    device_type TEXT NOT NULL,
    thing_type TEXT NOT NULL DEFAULT 'device',
    protocol_type TEXT,
    driver_name TEXT,
    tags TEXT NOT NULL DEFAULT '[]',
    device_info TEXT NOT NULL DEFAULT '{}',
    properties TEXT NOT NULL DEFAULT '[]',
    actions TEXT NOT NULL DEFAULT '[]',
    events TEXT NOT NULL DEFAULT '[]',
    default_knowledge TEXT,
    is_builtin INTEGER DEFAULT 0,
    is_active INTEGER DEFAULT 1,
    workspace_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (category) REFERENCES template_categories(name)
);

-- Backfill: commands → actions, thing_type='device' for existing rows
INSERT INTO thing_templates_new
    (id, name, display_name, description, version, author, category,
     manufacturer, device_type, thing_type, protocol_type, driver_name,
     tags, device_info, properties, actions, events, default_knowledge,
     is_builtin, is_active, workspace_id, created_at, updated_at)
SELECT id, name, display_name, description, version, author, category,
       manufacturer, device_type, 'device',
       protocol_type, driver_name,
       COALESCE(tags, '[]'), COALESCE(device_info, '{}'),
       COALESCE(properties, '[]'), COALESCE(commands, '[]'), '[]',
       NULL,
       is_builtin, is_active, workspace_id, created_at, updated_at
FROM device_templates;

DROP TABLE device_templates;
ALTER TABLE thing_templates_new RENAME TO thing_templates;

-- Expression index for workspace-scoped unique name
CREATE UNIQUE INDEX idx_thing_templates_name_workspace
    ON thing_templates(COALESCE(workspace_id, ''), name);

-- Recreate indexes
CREATE INDEX IF NOT EXISTS idx_thing_templates_category ON thing_templates(category);
CREATE INDEX IF NOT EXISTS idx_thing_templates_device_type ON thing_templates(device_type);
CREATE INDEX IF NOT EXISTS idx_thing_templates_thing_type ON thing_templates(thing_type);
CREATE INDEX IF NOT EXISTS idx_thing_templates_manufacturer ON thing_templates(manufacturer);
CREATE INDEX IF NOT EXISTS idx_thing_templates_protocol_type ON thing_templates(protocol_type);
CREATE INDEX IF NOT EXISTS idx_thing_templates_driver_name ON thing_templates(driver_name);
CREATE INDEX IF NOT EXISTS idx_thing_templates_is_builtin ON thing_templates(is_builtin);
CREATE INDEX IF NOT EXISTS idx_thing_templates_is_active ON thing_templates(is_active);
CREATE INDEX IF NOT EXISTS idx_thing_templates_created_at ON thing_templates(created_at);
CREATE INDEX IF NOT EXISTS idx_thing_templates_workspace ON thing_templates(workspace_id);

-- ============================================================================
-- 1. devices table Thing generalization (heaviest rebuild)
-- ============================================================================
-- New columns: thing_type, ontology_summary, summary_status, template_id.
-- Removed: product_id (superseded by template_id → thing_templates).
-- Name constraint: global UNIQUE → expression index (COALESCE(workspace_id,''), name).
-- parent_id FK: ON DELETE SET NULL → ON DELETE RESTRICT.

CREATE TABLE devices_new (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    display_name TEXT,
    thing_type TEXT NOT NULL DEFAULT 'device',
    device_type TEXT,
    address TEXT,
    description TEXT,
    position TEXT,
    driver_name TEXT,
    device_model TEXT,
    protocol_type TEXT,
    factory_name TEXT,
    linked_data TEXT,
    driver_options TEXT,
    state INTEGER NOT NULL DEFAULT 0,
    parent_id TEXT,
    organization_id TEXT,
    tenant_id TEXT,
    workspace_id TEXT,
    linked_gateway TEXT,
    fingerprint TEXT,
    template_id TEXT,
    ontology_summary TEXT,
    summary_status TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (parent_id) REFERENCES devices(id) ON DELETE RESTRICT,
    FOREIGN KEY (organization_id) REFERENCES organizations(id) ON DELETE SET NULL,
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE SET NULL,
    FOREIGN KEY (template_id) REFERENCES thing_templates(id) ON DELETE SET NULL
);

-- Backfill: thing_type='device', template_id=NULL initially
INSERT INTO devices_new
    (id, name, display_name, thing_type, device_type, address, description,
     position, driver_name, device_model, protocol_type, factory_name,
     linked_data, driver_options, state, parent_id, organization_id,
     tenant_id, workspace_id, linked_gateway, fingerprint,
     template_id, ontology_summary, summary_status, created_at, updated_at)
SELECT id, name, display_name, 'device', device_type, address, description,
       position, driver_name, device_model, protocol_type, factory_name,
       linked_data, driver_options, state, parent_id, organization_id,
       tenant_id, workspace_id, linked_gateway, fingerprint,
       NULL, NULL, NULL, created_at, updated_at
FROM devices;

DROP TABLE devices;
ALTER TABLE devices_new RENAME TO devices;

-- Expression index for workspace-scoped unique name
CREATE UNIQUE INDEX idx_devices_name_workspace
    ON devices(COALESCE(workspace_id, ''), name);

-- Recreate existing indexes (minus product_id, which is gone)
CREATE INDEX IF NOT EXISTS idx_devices_device_type ON devices(device_type);
CREATE INDEX IF NOT EXISTS idx_devices_state ON devices(state);
CREATE INDEX IF NOT EXISTS idx_devices_parent_id ON devices(parent_id);
CREATE INDEX IF NOT EXISTS idx_devices_organization_id ON devices(organization_id);
CREATE INDEX IF NOT EXISTS idx_devices_tenant_id ON devices(tenant_id);
CREATE INDEX IF NOT EXISTS idx_devices_workspace ON devices(workspace_id);
CREATE INDEX IF NOT EXISTS idx_devices_driver_name ON devices(driver_name);
CREATE INDEX IF NOT EXISTS idx_devices_factory_name ON devices(factory_name);
CREATE INDEX IF NOT EXISTS idx_devices_linked_gateway ON devices(linked_gateway);
CREATE INDEX IF NOT EXISTS idx_devices_fingerprint ON devices(fingerprint);
-- New indexes for Thing Ontology columns
CREATE INDEX IF NOT EXISTS idx_devices_thing_type ON devices(thing_type);
CREATE INDEX IF NOT EXISTS idx_devices_template_id ON devices(template_id);

-- ============================================================================
-- 2. tags table rebuild
-- ============================================================================
-- CHECK: type IN ('device','app') → type IN ('device','app','thing').
-- Unique: global UNIQUE(type,name) → expression index (COALESCE(tenant_id,''), type, name).

DROP TABLE IF EXISTS tags_new;

CREATE TABLE tags_new (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL CHECK (type IN ('device', 'app', 'thing')),
    name TEXT NOT NULL,
    description TEXT,
    color TEXT,
    tenant_id TEXT,
    created_by TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (created_by) REFERENCES users(id) ON DELETE SET NULL
);

INSERT INTO tags_new
    (id, type, name, description, color, tenant_id, created_by, created_at, updated_at)
SELECT id, type, name, description, color, tenant_id, created_by, created_at, updated_at
FROM tags;

DROP TABLE tags;
ALTER TABLE tags_new RENAME TO tags;

-- Expression index for tenant-scoped unique name within type
CREATE UNIQUE INDEX idx_tags_type_name_tenant
    ON tags(COALESCE(tenant_id, ''), type, name);

-- Recreate indexes
CREATE INDEX IF NOT EXISTS idx_tags_type ON tags(type);
CREATE INDEX IF NOT EXISTS idx_tags_name ON tags(name);
CREATE INDEX IF NOT EXISTS idx_tags_tenant_id ON tags(tenant_id);

-- ============================================================================
-- 3. resources → thing_resources rebuild
-- ============================================================================
-- New table with device_id FK, workspace_id NOT NULL, parse_status dropped.

CREATE TABLE thing_resources (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    device_id TEXT,
    type TEXT NOT NULL DEFAULT 'document',
    name TEXT NOT NULL,
    file_path TEXT NOT NULL DEFAULT '',
    content TEXT,
    tags TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE,
    FOREIGN KEY (device_id) REFERENCES devices(id) ON DELETE SET NULL
);

-- Backfill: content and tags from resources (parse_status dropped)
INSERT INTO thing_resources
    (id, workspace_id, device_id, type, name, file_path, content, tags, created_at, updated_at)
SELECT id, workspace_id, NULL, resource_type, name, file_path, content, tags, created_at, updated_at
FROM resources;

DROP TABLE resources;
ALTER TABLE thing_resources RENAME TO resources;

-- Recreate indexes
CREATE INDEX IF NOT EXISTS idx_resources_workspace ON resources(workspace_id);
CREATE INDEX IF NOT EXISTS idx_resources_type ON resources(type);
CREATE INDEX IF NOT EXISTS idx_resources_name ON resources(name);
CREATE INDEX IF NOT EXISTS idx_resources_workspace_type ON resources(workspace_id, type);
CREATE INDEX IF NOT EXISTS idx_resources_device_id ON resources(device_id);

-- ============================================================================
-- 4. events table changes
-- ============================================================================
-- Drop the cleanup_old_events TRIGGER (1万行清理触发器 — we don't want row
-- limits with thing events).
DROP TRIGGER IF EXISTS cleanup_old_events;

-- Drop triggers referencing tables that will be deleted
DROP TRIGGER IF EXISTS update_real_time_events_timestamp;
DROP TRIGGER IF EXISTS track_event_insertion_performance;
DROP TRIGGER IF EXISTS track_real_time_event_updates;
DROP TRIGGER IF EXISTS cleanup_old_performance_metrics;

-- Drop views referencing real_time_events (which will be deleted)
DROP VIEW IF EXISTS real_time_event_summary;
DROP VIEW IF EXISTS real_time_events_summary;

-- Add columns for real-time status and workspace scoping
ALTER TABLE events ADD COLUMN occurrence_count INTEGER DEFAULT 1;
ALTER TABLE events ADD COLUMN acknowledged BOOLEAN DEFAULT 0;
ALTER TABLE events ADD COLUMN acknowledged_by TEXT;
ALTER TABLE events ADD COLUMN acknowledged_at TEXT;
ALTER TABLE events ADD COLUMN workspace_id TEXT NOT NULL DEFAULT '';

-- Upsert dedup expression index on source dimensions
-- Allows INSERT ... ON CONFLICT(event_type, event_subtype, device_id)
-- DO UPDATE SET occurrence_count = occurrence_count + 1 for real-time status upserts.
CREATE UNIQUE INDEX IF NOT EXISTS idx_events_dedup
    ON events(event_type, event_subtype, device_id)
    WHERE device_id IS NOT NULL;

-- ============================================================================
-- 5. Delete dead event tables
-- ============================================================================
-- real_time_events merged into events (step 4).
-- lost_events: dead letter queue, 0 rows, no code writers.
-- event_performance_metrics: unused monitoring table.

DROP TABLE IF EXISTS real_time_events;
DROP TABLE IF EXISTS lost_events;
DROP TABLE IF EXISTS event_performance_metrics;

-- ============================================================================
-- 6. workspaces add column
-- ============================================================================
-- No rebuild needed: ALTER TABLE ADD COLUMN is natively supported by SQLite.

ALTER TABLE workspaces ADD COLUMN require_action_confirm BOOLEAN DEFAULT 1;

-- ============================================================================
-- 7. device_alarm_rules — ensure rule_type CHECK covers 'event'
-- ============================================================================
-- The current device_alarm_rules table has no CHECK constraint on rule_type
-- (it was removed in the relax_alarm_rule_fks migration).
-- We add a CHECK covering all valid rule types including 'event'.

DROP TABLE IF EXISTS device_alarm_rules_new;

CREATE TABLE device_alarm_rules_new (
    id TEXT PRIMARY KEY,
    device_id TEXT,
    property_id TEXT,
    rule_name TEXT NOT NULL,
    rule_type TEXT NOT NULL CHECK (rule_type IN ('threshold', 'range', 'change', 'offline', 'event')),
    condition_config TEXT NOT NULL,
    alarm_level TEXT NOT NULL CHECK (alarm_level IN ('info', 'warning', 'error', 'critical')),
    is_enabled BOOLEAN NOT NULL DEFAULT true,
    description TEXT,
    workspace_id TEXT,
    notification_config TEXT,
    created_by TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (device_id) REFERENCES devices(id) ON DELETE CASCADE,
    FOREIGN KEY (property_id) REFERENCES device_properties(id) ON DELETE CASCADE,
    FOREIGN KEY (created_by) REFERENCES users(id) ON DELETE SET NULL
);

INSERT INTO device_alarm_rules_new
    (id, device_id, property_id, rule_name, rule_type, condition_config,
     alarm_level, is_enabled, description, workspace_id, notification_config,
     created_by, created_at, updated_at)
SELECT id, device_id, property_id, rule_name, rule_type, condition_config,
       alarm_level, is_enabled, description, workspace_id,
       COALESCE(notification_config, NULL),
       created_by, created_at, updated_at
FROM device_alarm_rules;

DROP TABLE device_alarm_rules;
ALTER TABLE device_alarm_rules_new RENAME TO device_alarm_rules;

-- Recreate indexes
CREATE INDEX IF NOT EXISTS idx_device_alarm_rules_device_id ON device_alarm_rules(device_id);
CREATE INDEX IF NOT EXISTS idx_device_alarm_rules_is_enabled ON device_alarm_rules(is_enabled);

-- ============================================================================
-- 8. Final PRAGMA foreign_key_check
-- ============================================================================
-- Verify referential integrity before committing the transaction.
PRAGMA foreign_key_check;
