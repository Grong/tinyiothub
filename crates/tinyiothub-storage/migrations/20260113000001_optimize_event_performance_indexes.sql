-- ============================================================================
-- Event Performance Optimization Migration
-- Adds additional indexes and optimizations for better event system performance
-- ============================================================================

-- Additional composite indexes for common query patterns
CREATE INDEX IF NOT EXISTS idx_events_timestamp_level_device ON events (timestamp, event_level, device_id) WHERE device_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_events_timestamp_type_subtype ON events (timestamp, event_type, event_subtype);
CREATE INDEX IF NOT EXISTS idx_events_level_created ON events (event_level, created_at);
CREATE INDEX IF NOT EXISTS idx_events_source_timestamp ON events (source_type, source_id, timestamp);

-- Partial indexes for better performance on filtered queries (without datetime() in WHERE)
CREATE INDEX IF NOT EXISTS idx_events_critical ON events (timestamp) WHERE event_level >= 4;
CREATE INDEX IF NOT EXISTS idx_events_device_timestamp ON events (device_id, timestamp) WHERE device_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_events_user_timestamp ON events (user_id, timestamp) WHERE user_id IS NOT NULL;

-- Real-time events performance indexes
CREATE INDEX IF NOT EXISTS idx_real_time_level_update ON real_time_events (event_level, last_update);
CREATE INDEX IF NOT EXISTS idx_real_time_device_level_ack ON real_time_events (device_id, event_level, acknowledged) WHERE device_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_real_time_unack_critical ON real_time_events (event_level, last_update) WHERE acknowledged = 0 AND event_level >= 4;

-- Notification system performance indexes
CREATE INDEX IF NOT EXISTS idx_notification_rules_enabled_level ON notification_rules (enabled, event_level) WHERE enabled = 1;
CREATE INDEX IF NOT EXISTS idx_notification_history_status_method ON notification_history (status, notification_method, created_at);
CREATE INDEX IF NOT EXISTS idx_notification_history_created ON notification_history (created_at);

-- Performance monitoring tables
CREATE TABLE IF NOT EXISTS event_performance_metrics (
    id TEXT PRIMARY KEY,
    metric_type TEXT NOT NULL, -- 'processing_time', 'queue_size', 'error_rate', etc.
    metric_value REAL NOT NULL,
    timestamp TEXT NOT NULL,
    metadata TEXT, -- JSON format additional data
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_performance_metrics_type_timestamp ON event_performance_metrics (metric_type, timestamp);
CREATE INDEX IF NOT EXISTS idx_performance_metrics_timestamp ON event_performance_metrics (timestamp);

-- Performance alerts table
CREATE TABLE IF NOT EXISTS event_performance_alerts (
    id TEXT PRIMARY KEY,
    alert_type TEXT NOT NULL, -- 'high_processing_time', 'high_queue_size', etc.
    severity TEXT NOT NULL, -- 'info', 'warning', 'critical'
    message TEXT NOT NULL,
    current_value REAL NOT NULL,
    threshold_value REAL NOT NULL,
    resolved BOOLEAN DEFAULT FALSE,
    resolved_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_performance_alerts_type_severity ON event_performance_alerts (alert_type, severity);
CREATE INDEX IF NOT EXISTS idx_performance_alerts_unresolved ON event_performance_alerts (resolved, created_at) WHERE resolved = 0;
CREATE INDEX IF NOT EXISTS idx_performance_alerts_created ON event_performance_alerts (created_at);

-- Database optimization statistics table
CREATE TABLE IF NOT EXISTS event_optimization_history (
    id TEXT PRIMARY KEY,
    optimization_type TEXT NOT NULL, -- 'index_creation', 'vacuum', 'analyze', etc.
    description TEXT NOT NULL,
    execution_time_ms REAL,
    rows_affected INTEGER,
    success BOOLEAN DEFAULT TRUE,
    error_message TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_optimization_history_type ON event_optimization_history (optimization_type, created_at);
CREATE INDEX IF NOT EXISTS idx_optimization_history_created ON event_optimization_history (created_at);

-- Load balancer statistics table
CREATE TABLE IF NOT EXISTS event_load_balancer_stats (
    id TEXT PRIMARY KEY,
    worker_count INTEGER NOT NULL,
    active_workers INTEGER NOT NULL,
    queue_size INTEGER NOT NULL,
    total_processed INTEGER NOT NULL,
    total_errors INTEGER NOT NULL,
    success_rate REAL NOT NULL,
    throughput_per_second REAL NOT NULL,
    backpressure_active BOOLEAN DEFAULT FALSE,
    timestamp TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_load_balancer_stats_timestamp ON event_load_balancer_stats (timestamp);
CREATE INDEX IF NOT EXISTS idx_load_balancer_stats_created ON event_load_balancer_stats (created_at);

-- Query performance tracking table
CREATE TABLE IF NOT EXISTS event_query_performance (
    id TEXT PRIMARY KEY,
    query_name TEXT NOT NULL,
    query_type TEXT NOT NULL, -- 'select', 'insert', 'update', 'delete'
    execution_time_ms REAL NOT NULL,
    rows_affected INTEGER,
    success BOOLEAN DEFAULT TRUE,
    error_message TEXT,
    timestamp TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_query_performance_name_timestamp ON event_query_performance (query_name, timestamp);
CREATE INDEX IF NOT EXISTS idx_query_performance_slow ON event_query_performance (execution_time_ms, timestamp) WHERE execution_time_ms > 100;
CREATE INDEX IF NOT EXISTS idx_query_performance_created ON event_query_performance (created_at);

-- Performance views for quick access to common metrics
CREATE VIEW IF NOT EXISTS event_performance_summary AS
SELECT 
    'events_per_hour' as metric_name,
    COUNT(*) as metric_value,
    strftime('%Y-%m-%d %H:00:00', timestamp) as time_bucket
FROM events 
WHERE timestamp > datetime('now', '-24 hours')
GROUP BY strftime('%Y-%m-%d %H:00:00', timestamp)
UNION ALL
SELECT 
    'errors_per_hour' as metric_name,
    COUNT(*) as metric_value,
    strftime('%Y-%m-%d %H:00:00', timestamp) as time_bucket
FROM events 
WHERE timestamp > datetime('now', '-24 hours') AND event_level >= 4
GROUP BY strftime('%Y-%m-%d %H:00:00', timestamp);

CREATE VIEW IF NOT EXISTS real_time_events_summary AS
SELECT 
    event_level,
    event_type,
    event_subtype,
    COUNT(*) as active_count,
    COUNT(CASE WHEN acknowledged = 0 THEN 1 END) as unacknowledged_count,
    AVG(occurrence_count) as avg_occurrence_count,
    MAX(last_update) as latest_update
FROM real_time_events
GROUP BY event_level, event_type, event_subtype;

-- Triggers for automatic performance tracking

-- Track event insertion performance
CREATE TRIGGER IF NOT EXISTS track_event_insertion_performance
    AFTER INSERT ON events
    FOR EACH ROW
BEGIN
    INSERT INTO event_performance_metrics (
        id, metric_type, metric_value, timestamp, metadata
    ) VALUES (
        'perf_' || hex(randomblob(8)),
        'event_insertion',
        1.0,
        NEW.timestamp,
        json_object('event_type', NEW.event_type, 'event_level', NEW.event_level)
    );
END;

-- Track real-time event updates
CREATE TRIGGER IF NOT EXISTS track_real_time_event_updates
    AFTER UPDATE ON real_time_events
    FOR EACH ROW
    WHEN NEW.last_update != OLD.last_update
BEGIN
    INSERT INTO event_performance_metrics (
        id, metric_type, metric_value, timestamp, metadata
    ) VALUES (
        'perf_' || hex(randomblob(8)),
        'real_time_update',
        1.0,
        NEW.last_update,
        json_object('event_type', NEW.event_type, 'occurrence_count', NEW.occurrence_count)
    );
END;

-- Auto-cleanup old performance data (keep last 7 days)
CREATE TRIGGER IF NOT EXISTS cleanup_old_performance_metrics
    AFTER INSERT ON event_performance_metrics
    FOR EACH ROW
    WHEN (SELECT COUNT(*) FROM event_performance_metrics) > 10000
BEGIN
    DELETE FROM event_performance_metrics 
    WHERE created_at < datetime('now', '-7 days');
END;

-- Auto-cleanup old performance alerts (keep last 30 days)
CREATE TRIGGER IF NOT EXISTS cleanup_old_performance_alerts
    AFTER INSERT ON event_performance_alerts
    FOR EACH ROW
    WHEN (SELECT COUNT(*) FROM event_performance_alerts WHERE resolved = 1) > 1000
BEGIN
    DELETE FROM event_performance_alerts 
    WHERE resolved = 1 AND created_at < datetime('now', '-30 days');
END;

-- Insert initial performance thresholds
INSERT OR IGNORE INTO event_performance_alerts (
    id, alert_type, severity, message, current_value, threshold_value, resolved
) VALUES 
    ('threshold_processing_time', 'configuration', 'info', 'Max processing time threshold: 100ms', 100.0, 100.0, 1),
    ('threshold_queue_size', 'configuration', 'info', 'Max queue size threshold: 1000', 1000.0, 1000.0, 1),
    ('threshold_error_rate', 'configuration', 'info', 'Max error rate threshold: 1%', 0.01, 0.01, 1),
    ('threshold_memory_usage', 'configuration', 'info', 'Max memory usage threshold: 80%', 80.0, 80.0, 1);

-- Update database statistics for better query planning
ANALYZE;

-- Optimize database settings for performance
PRAGMA optimize;

-- ============================================================================
-- Performance optimization migration complete
-- ============================================================================