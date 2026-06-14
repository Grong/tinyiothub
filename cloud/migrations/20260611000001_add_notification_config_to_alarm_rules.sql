-- Add notification_config column to device_alarm_rules
-- Stores the full NotificationConfig JSON for alarm rules
ALTER TABLE device_alarm_rules ADD COLUMN notification_config TEXT;
