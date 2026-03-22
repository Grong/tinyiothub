-- Add missing indexes for query performance
-- Created: 2026-03-22

-- Index for users.parent_id (hierarchical user queries)
CREATE INDEX IF NOT EXISTS idx_users_parent_id ON users(parent_id);

-- Index for devices.driver_name (filtering by driver type)
CREATE INDEX IF NOT EXISTS idx_devices_driver_name ON devices(driver_name);

-- Index for devices.factory_name (filtering by factory)
CREATE INDEX IF NOT EXISTS idx_devices_factory_name ON devices(factory_name);

-- Index for jobs.target_device_id (job targeting queries)
CREATE INDEX IF NOT EXISTS idx_jobs_target_device_id ON jobs(target_device_id);

-- Index for sms_codes (phone lookup and expiration queries)
CREATE INDEX IF NOT EXISTS idx_sms_codes_phone_expires ON sms_codes(phone, expires_at);

-- Add updated_at columns to tables missing them (SQLite 3.35+)
-- These tables track mutable data but lack timestamps

ALTER TABLE sms_codes ADD COLUMN updated_at TEXT;

ALTER TABLE social_bindings ADD COLUMN updated_at TEXT;

ALTER TABLE notification_channels ADD COLUMN updated_at TEXT;
