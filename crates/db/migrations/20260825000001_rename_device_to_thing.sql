-- device → thing 彻底重命名（PR-1）。
-- SQLite ≥3.25:RENAME TO/COLUMN 自动更新 FK 引用、索引定义、UNIQUE 约束
-- (触发器体在 SQLite 3.51 起不再自动改写,见 1b 节处理)。
-- 运行环境:run_migrations 已 PRAGMA foreign_keys = OFF。
--
-- 与 baseline(20260819000001)核对后的两处偏差（baseline 为准）:
--   a) knowledge_relations 无 device_id 列,不在列重命名清单内;
--   b) thing_templates 已有 category 列(NOT NULL,FK→template_categories),
--      device_type 无法 RENAME 为 category,改为 DROP COLUMN;
--      idx_thing_templates_category 索引 baseline 已存在,不重复创建。

-- ── 1. 表重命名 ──
ALTER TABLE devices RENAME TO things;
ALTER TABLE device_traces RENAME TO thing_traces;
ALTER TABLE device_memory RENAME TO thing_memory;
ALTER TABLE device_alarm_rules RENAME TO thing_alarm_rules;
ALTER TABLE device_alarms RENAME TO thing_alarms;

-- ── 1b. 先删除 device_memory 的触发器 ──
-- SQLite 3.51 起 RENAME TABLE 只改写触发器 ON 子句、不再改写触发器体
-- (实测:/usr/bin/sqlite3 3.51.0 下体部仍引用 device_memory,使 schema 无法
-- 重新解析,后续任何 RENAME COLUMN 报 "SQL logic error";sqlx 内置旧版
-- SQLite 会改写体部,两种行为都兼容的前提是先把触发器删掉)。
-- 新触发器 keep_thing_memory_limit 在列重命名完成后重建(见第 5 节)。
DROP TRIGGER keep_device_memory_limit;

-- ── 2. device_id → thing_id(13 张表;jobs 单列见下)──
ALTER TABLE messages RENAME COLUMN device_id TO thing_id;
ALTER TABLE thing_traces RENAME COLUMN device_id TO thing_id;
ALTER TABLE events RENAME COLUMN device_id TO thing_id;
ALTER TABLE thing_memory RENAME COLUMN device_id TO thing_id;
ALTER TABLE batch_command_items RENAME COLUMN device_id TO thing_id;
ALTER TABLE resources RENAME COLUMN device_id TO thing_id;
ALTER TABLE thing_properties RENAME COLUMN device_id TO thing_id;
ALTER TABLE thing_actions RENAME COLUMN device_id TO thing_id;
ALTER TABLE thing_alarm_rules RENAME COLUMN device_id TO thing_id;
ALTER TABLE thing_alarms RENAME COLUMN device_id TO thing_id;
ALTER TABLE knowledge_entities RENAME COLUMN device_id TO thing_id;
ALTER TABLE agent_memories RENAME COLUMN device_id TO thing_id;
ALTER TABLE agent_actions RENAME COLUMN device_id TO thing_id;
ALTER TABLE jobs RENAME COLUMN target_device_id TO target_thing_id;

-- ── 3. device_type → category / device_limit → thing_limit ──
ALTER TABLE things RENAME COLUMN device_type TO category;
ALTER TABLE messages RENAME COLUMN device_type TO category;
-- thing_templates.category 已存在:先 drop 其索引再 drop device_type 列
-- (SQLite DROP COLUMN 要求列不被任何索引引用)。
DROP INDEX idx_thing_templates_device_type;
ALTER TABLE thing_templates DROP COLUMN device_type;
ALTER TABLE subscription_plans RENAME COLUMN device_limit TO thing_limit;

-- ── 4. tags 重建(CHECK 不可 ALTER);存量 'device' 归入 'thing' ──
UPDATE tags SET type = 'thing' WHERE type = 'device';
CREATE TABLE tags_new (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL CHECK (type IN ('app', 'thing')),
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
    SELECT id, type, name, description, color, tenant_id, created_by, created_at, updated_at
    FROM tags;
DROP TABLE tags;
ALTER TABLE tags_new RENAME TO tags;
CREATE UNIQUE INDEX idx_tags_type_name_tenant ON tags(COALESCE(tenant_id, ''), type, name);
CREATE INDEX idx_tags_type ON tags(type);
CREATE INDEX idx_tags_name ON tags(name);
CREATE INDEX idx_tags_tenant_id ON tags(tenant_id);

-- ── 5. 触发器重建(旧触发器已在 1b 删除;此处换新名 + thing_id 列)──
CREATE TRIGGER keep_thing_memory_limit
AFTER INSERT ON thing_memory
BEGIN
    DELETE FROM thing_memory
    WHERE workspace_id = NEW.workspace_id
      AND agent_id = NEW.agent_id
      AND thing_id = NEW.thing_id
      AND id NOT IN (
          SELECT id FROM thing_memory
          WHERE workspace_id = NEW.workspace_id
            AND agent_id = NEW.agent_id
            AND thing_id = NEW.thing_id
          ORDER BY snapshot_time DESC
          LIMIT 100
      );
END;

-- ── 6. 索引换名(定义已随 RENAME 自动更新,此处 DROP+CREATE 仅为命名一致)──
DROP INDEX idx_devices_name_workspace;
CREATE UNIQUE INDEX idx_things_name_workspace ON things(COALESCE(workspace_id, ''), name);
DROP INDEX idx_devices_device_type;
CREATE INDEX idx_things_category ON things(category);
DROP INDEX idx_devices_state;
CREATE INDEX idx_things_state ON things(state);
DROP INDEX idx_devices_parent_id;
CREATE INDEX idx_things_parent_id ON things(parent_id);
DROP INDEX idx_devices_organization_id;
CREATE INDEX idx_things_organization_id ON things(organization_id);
DROP INDEX idx_devices_tenant_id;
CREATE INDEX idx_things_tenant_id ON things(tenant_id);
DROP INDEX idx_devices_workspace;
CREATE INDEX idx_things_workspace ON things(workspace_id);
DROP INDEX idx_devices_driver_name;
CREATE INDEX idx_things_driver_name ON things(driver_name);
DROP INDEX idx_devices_factory_name;
CREATE INDEX idx_things_factory_name ON things(factory_name);
DROP INDEX idx_devices_linked_gateway;
CREATE INDEX idx_things_linked_gateway ON things(linked_gateway);
DROP INDEX idx_devices_fingerprint;
CREATE INDEX idx_things_fingerprint ON things(fingerprint);
DROP INDEX idx_devices_thing_type;
CREATE INDEX idx_things_thing_type ON things(thing_type);
DROP INDEX idx_devices_template_id;
CREATE INDEX idx_things_template_id ON things(template_id);
DROP INDEX idx_device_traces_device_id;
CREATE INDEX idx_thing_traces_thing_id ON thing_traces(thing_id);
DROP INDEX idx_device_traces_device_time;
CREATE INDEX idx_thing_traces_thing_time ON thing_traces(thing_id, created_at DESC);
DROP INDEX idx_device_traces_device_type;
CREATE INDEX idx_thing_traces_thing_type ON thing_traces(thing_id, trace_type);
DROP INDEX idx_device_traces_device_level;
CREATE INDEX idx_thing_traces_thing_level ON thing_traces(thing_id, level);
-- 以下 6 个 device_traces 索引列不含 device_id,但名字带 device 前缀,一并换名。
DROP INDEX idx_device_traces_trace_type;
CREATE INDEX idx_thing_traces_trace_type ON thing_traces(trace_type);
DROP INDEX idx_device_traces_level;
CREATE INDEX idx_thing_traces_level ON thing_traces(level);
DROP INDEX idx_device_traces_category;
CREATE INDEX idx_thing_traces_category ON thing_traces(category);
DROP INDEX idx_device_traces_created_at;
CREATE INDEX idx_thing_traces_created_at ON thing_traces(created_at);
DROP INDEX idx_device_traces_user_id;
CREATE INDEX idx_thing_traces_user_id ON thing_traces(user_id);
DROP INDEX idx_device_traces_source;
CREATE INDEX idx_thing_traces_source ON thing_traces(source);
DROP INDEX idx_device_memory_lookup;
CREATE INDEX idx_thing_memory_lookup ON thing_memory(workspace_id, agent_id, thing_id, snapshot_time DESC);
DROP INDEX idx_device_alarm_rules_device_id;
CREATE INDEX idx_thing_alarm_rules_thing_id ON thing_alarm_rules(thing_id);
-- 名字带 device 前缀的 alarm 索引一并换名。
DROP INDEX idx_device_alarm_rules_is_enabled;
CREATE INDEX idx_thing_alarm_rules_is_enabled ON thing_alarm_rules(is_enabled);
DROP INDEX idx_device_alarms_device_id;
CREATE INDEX idx_thing_alarms_thing_id ON thing_alarms(thing_id);
DROP INDEX idx_device_alarms_alarm_level;
CREATE INDEX idx_thing_alarms_alarm_level ON thing_alarms(alarm_level);
DROP INDEX idx_device_alarms_alarm_time;
CREATE INDEX idx_thing_alarms_alarm_time ON thing_alarms(alarm_time);
DROP INDEX idx_device_alarms_is_acknowledged;
CREATE INDEX idx_thing_alarms_is_acknowledged ON thing_alarms(is_acknowledged);
DROP INDEX idx_device_alarms_is_resolved;
CREATE INDEX idx_thing_alarms_is_resolved ON thing_alarms(is_resolved);
DROP INDEX idx_messages_device_id;
CREATE INDEX idx_messages_thing_id ON messages(thing_id);
DROP INDEX idx_events_device;
CREATE INDEX idx_events_thing ON events(thing_id);
DROP INDEX idx_events_device_timestamp;
CREATE INDEX idx_events_thing_timestamp ON events(thing_id, timestamp) WHERE thing_id IS NOT NULL;
DROP INDEX idx_events_timestamp_level_device;
CREATE INDEX idx_events_timestamp_level_thing ON events(timestamp, event_level, thing_id) WHERE thing_id IS NOT NULL;
DROP INDEX idx_events_device_id;
CREATE INDEX idx_events_thing_id ON events(thing_id);
DROP INDEX idx_jobs_target_device_id;
CREATE INDEX idx_jobs_target_thing_id ON jobs(target_thing_id);
DROP INDEX idx_batch_command_items_device_id;
CREATE INDEX idx_batch_command_items_thing_id ON batch_command_items(thing_id);
DROP INDEX idx_resources_device_id;
CREATE INDEX idx_resources_thing_id ON resources(thing_id);
DROP INDEX idx_thing_properties_device_id;
CREATE INDEX idx_thing_properties_thing_id ON thing_properties(thing_id);
DROP INDEX idx_thing_actions_device_id;
CREATE INDEX idx_thing_actions_thing_id ON thing_actions(thing_id);
DROP INDEX idx_knowledge_entities_device;
CREATE INDEX idx_knowledge_entities_thing ON knowledge_entities(thing_id);

-- idx_events_status_dedup(partial unique,名字不含 device,
-- 定义已被 RENAME COLUMN 自动更新为 thing_id)保持不变。
