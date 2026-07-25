-- Rename device_properties → thing_properties, device_commands → thing_actions
-- SQLite supports ALTER TABLE RENAME since 3.25.0 (2018)
-- Foreign keys (e.g., device_alarm_rules.property_id → thing_properties.id) are preserved through rename

ALTER TABLE device_properties RENAME TO thing_properties;
ALTER TABLE device_commands RENAME TO thing_actions;
