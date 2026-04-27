-- ============================================================================
-- Event Service System Migration
-- Creates tables for the new event service system to replace the simple message system
-- ============================================================================

-- Events table (historical events)
CREATE TABLE events (
    id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL, -- 'system' or 'device'
    event_subtype TEXT NOT NULL, -- specific subtype like 'user_auth', 'connection', etc.
    event_level INTEGER NOT NULL, -- 1: debug, 2: info, 3: warning, 4: error, 5: critical
    timestamp TEXT NOT NULL, -- ISO 8601 format
    source_type TEXT NOT NULL, -- 'system', 'device', 'user'
    source_id TEXT, -- identifier of the source
    title TEXT NOT NULL,
    content TEXT, -- JSON format rich content
    metadata TEXT, -- JSON format additional metadata
    user_id TEXT, -- user who triggered the event (if applicable)
    device_id TEXT, -- device related to the event (if applicable)
    property_id TEXT, -- device property related to the event (if applicable)
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Indexes for events table
CREATE INDEX idx_events_timestamp ON events (timestamp);
CREATE INDEX idx_events_level ON events (event_level);
CREATE INDEX idx_events_type ON events (event_type, event_subtype);
CREATE INDEX idx_events_device ON events (device_id);
CREATE INDEX idx_events_user ON events (user_id);
CREATE INDEX idx_events_source ON events (source_type, source_id);
CREATE INDEX idx_events_created ON events (created_at);

-- Real-time events table (current active events)
CREATE TABLE real_time_events (
    id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,
    event_subtype TEXT NOT NULL,
    event_level INTEGER NOT NULL,
    source_type TEXT NOT NULL,
    source_id TEXT NOT NULL,
    device_id TEXT,
    property_id TEXT,
    title TEXT NOT NULL,
    content TEXT, -- JSON format rich content
    first_occurrence TEXT NOT NULL, -- when this event first occurred
    last_update TEXT NOT NULL, -- when this event was last updated
    occurrence_count INTEGER DEFAULT 1, -- how many times this event occurred
    acknowledged BOOLEAN DEFAULT FALSE,
    acknowledged_by TEXT, -- user who acknowledged the event
    acknowledged_at TEXT, -- when the event was acknowledged
    
    -- Ensure uniqueness per source and event type
    UNIQUE(source_type, source_id, event_type, event_subtype)
);

-- Indexes for real_time_events table
CREATE INDEX idx_real_time_level ON real_time_events (event_level);
CREATE INDEX idx_real_time_device ON real_time_events (device_id);
CREATE INDEX idx_real_time_ack ON real_time_events (acknowledged);
CREATE INDEX idx_real_time_source ON real_time_events (source_type, source_id);
CREATE INDEX idx_real_time_last_update ON real_time_events (last_update);

-- Notification rules table
CREATE TABLE notification_rules (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    event_type TEXT, -- filter by event type (optional)
    event_subtype TEXT, -- filter by event subtype (optional)
    event_level INTEGER, -- minimum event level to trigger (optional)
    device_filter TEXT, -- JSON format device filter conditions (optional)
    notification_methods TEXT NOT NULL, -- JSON array: ["websocket", "email", "sms"]
    recipients TEXT NOT NULL, -- JSON array: recipient list
    enabled BOOLEAN DEFAULT TRUE,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Indexes for notification_rules table
CREATE INDEX idx_notification_rules_enabled ON notification_rules (enabled);
CREATE INDEX idx_notification_rules_type ON notification_rules (event_type, event_subtype);
CREATE INDEX idx_notification_rules_level ON notification_rules (event_level);

-- Notification history table
CREATE TABLE notification_history (
    id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL,
    rule_id TEXT NOT NULL,
    notification_method TEXT NOT NULL, -- 'websocket', 'email', 'sms'
    recipient TEXT NOT NULL,
    status TEXT NOT NULL, -- 'pending', 'sent', 'failed'
    sent_at TEXT,
    error_message TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Indexes for notification_history table
CREATE INDEX idx_notification_history_event ON notification_history (event_id);
CREATE INDEX idx_notification_history_rule ON notification_history (rule_id);
CREATE INDEX idx_notification_history_status ON notification_history (status);
CREATE INDEX idx_notification_history_created ON notification_history (created_at);

-- Event statistics view for quick access to common statistics
CREATE VIEW event_statistics AS
SELECT 
    event_level,
    event_type,
    event_subtype,
    COUNT(*) as count,
    DATE(timestamp) as event_date
FROM events 
WHERE timestamp >= datetime('now', '-30 days')
GROUP BY event_level, event_type, event_subtype, DATE(timestamp);

-- Real-time event summary view
CREATE VIEW real_time_event_summary AS
SELECT 
    event_level,
    event_type,
    event_subtype,
    COUNT(*) as active_count,
    COUNT(CASE WHEN acknowledged = 0 THEN 1 END) as unacknowledged_count
FROM real_time_events
GROUP BY event_level, event_type, event_subtype;

-- Triggers to maintain data consistency

-- Update last_update timestamp when real_time_events is modified
CREATE TRIGGER update_real_time_events_timestamp
    AFTER UPDATE ON real_time_events
    FOR EACH ROW
BEGIN
    UPDATE real_time_events 
    SET last_update = datetime('now')
    WHERE id = NEW.id;
END;

-- Update notification_rules updated_at timestamp
CREATE TRIGGER update_notification_rules_timestamp
    AFTER UPDATE ON notification_rules
    FOR EACH ROW
BEGIN
    UPDATE notification_rules 
    SET updated_at = datetime('now')
    WHERE id = NEW.id;
END;

-- Auto-cleanup old events (keep last 10000 events)
CREATE TRIGGER cleanup_old_events
    AFTER INSERT ON events
    FOR EACH ROW
    WHEN (SELECT COUNT(*) FROM events) > 10000
BEGIN
    DELETE FROM events 
    WHERE id IN (
        SELECT id FROM events 
        ORDER BY timestamp ASC 
        LIMIT (SELECT COUNT(*) FROM events) - 10000
    );
END;

-- Auto-cleanup old notification history (keep last 30 days)
CREATE TRIGGER cleanup_old_notification_history
    AFTER INSERT ON notification_history
    FOR EACH ROW
BEGIN
    DELETE FROM notification_history 
    WHERE created_at < datetime('now', '-30 days');
END;

-- Insert default notification rules

-- Critical events notification rule
INSERT INTO notification_rules (
    id, name, description, event_level, notification_methods, recipients, enabled
) VALUES (
    'default-critical-events',
    '严重事件通知',
    '所有严重级别事件的默认通知规则',
    5, -- Critical level
    '["websocket"]',
    '["admin"]',
    1 -- TRUE
);

-- Error events notification rule
INSERT INTO notification_rules (
    id, name, description, event_level, notification_methods, recipients, enabled
) VALUES (
    'default-error-events',
    '错误事件通知',
    '所有错误级别事件的默认通知规则',
    4, -- Error level
    '["websocket"]',
    '["admin"]',
    1 -- TRUE
);

-- Device connection events notification rule
INSERT INTO notification_rules (
    id, name, description, event_type, event_subtype, notification_methods, recipients, enabled
) VALUES (
    'device-connection-events',
    '设备连接事件通知',
    '设备连接状态变化通知',
    'device',
    'connection',
    '["websocket"]',
    '["admin", "operator"]',
    1 -- TRUE
);

-- ============================================================================
-- Migration complete
-- ============================================================================