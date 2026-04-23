use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Event processing performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventPerformanceMetrics {
    /// Total events processed
    pub total_events_processed: u64,
    /// Events processed per second
    pub events_per_second: f64,
    /// Average processing time in milliseconds
    pub avg_processing_time_ms: f64,
    /// Peak processing time in milliseconds
    pub peak_processing_time_ms: f64,
    /// Current queue size
    pub current_queue_size: u64,
    /// Peak queue size
    pub peak_queue_size: u64,
    /// Database query performance
    pub db_query_metrics: DatabaseQueryMetrics,
    /// Memory usage metrics
    pub memory_metrics: MemoryMetrics,
    /// Error metrics
    pub error_metrics: ErrorMetrics,
    /// Last updated timestamp
    pub last_updated: DateTime<Utc>,
}

/// Database query performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseQueryMetrics {
    /// Average query time in milliseconds
    pub avg_query_time_ms: f64,
    /// Peak query time in milliseconds
    pub peak_query_time_ms: f64,
    /// Total queries executed
    pub total_queries: u64,
    /// Slow queries count (>100ms)
    pub slow_queries_count: u64,
    /// Connection pool metrics
    pub pool_metrics: ConnectionPoolMetrics,
}

/// Connection pool metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionPoolMetrics {
    /// Active connections
    pub active_connections: u32,
    /// Idle connections
    pub idle_connections: u32,
    /// Maximum connections
    pub max_connections: u32,
    /// Connection wait time in milliseconds
    pub avg_connection_wait_ms: f64,
}

/// Memory usage metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMetrics {
    /// Current memory usage in MB
    pub current_usage_mb: f64,
    /// Peak memory usage in MB
    pub peak_usage_mb: f64,
    /// Memory usage percentage
    pub usage_percentage: f64,
    /// Event cache size
    pub event_cache_size: u64,
}

/// Error metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMetrics {
    /// Total errors
    pub total_errors: u64,
    /// Error rate (errors per second)
    pub error_rate: f64,
    /// Errors by type
    pub errors_by_type: HashMap<String, u64>,
    /// Recent errors
    pub recent_errors: Vec<ErrorRecord>,
}

/// Error record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorRecord {
    pub error_type: String,
    pub error_message: String,
    pub timestamp: DateTime<Utc>,
    pub context: HashMap<String, String>,
}

/// Performance alert
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceAlert {
    pub alert_id: String,
    pub alert_type: AlertType,
    pub severity: AlertSeverity,
    pub message: String,
    pub current_value: f64,
    pub threshold: f64,
    pub timestamp: DateTime<Utc>,
    pub resolved: bool,
}

/// Alert types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertType {
    HighProcessingTime,
    HighQueueSize,
    HighErrorRate,
    SlowDatabaseQuery,
    HighMemoryUsage,
    ConnectionPoolExhaustion,
    LowThroughput,
}

/// Alert severity levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

/// Performance metrics collector
pub struct PerformanceMetricsCollector {
    // Atomic counters for thread-safe updates
    total_events: AtomicU64,
    total_processing_time_ms: AtomicU64,
    peak_processing_time_ms: AtomicU64, // Store as u64 (milliseconds * 1000 for precision)
    current_queue_size: AtomicU64,
    peak_queue_size: AtomicU64,
    
    // Database metrics
    total_queries: AtomicU64,
    total_query_time_ms: AtomicU64,
    peak_query_time_ms: AtomicU64, // Store as u64 (milliseconds * 1000 for precision)
    slow_queries: AtomicU64,
    
    // Error tracking
    total_errors: AtomicU64,
    errors_by_type: Arc<RwLock<HashMap<String, u64>>>,
    recent_errors: Arc<RwLock<Vec<ErrorRecord>>>,
    
    // Memory tracking
    peak_memory_mb: AtomicU64, // Store as u64 (MB * 1000 for precision)
    
    // Alerts
    active_alerts: Arc<RwLock<Vec<PerformanceAlert>>>,
    
    // Start time for rate calculations
    start_time: DateTime<Utc>,
}

impl Default for PerformanceMetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl PerformanceMetricsCollector {
    pub fn new() -> Self {
        Self {
            total_events: AtomicU64::new(0),
            total_processing_time_ms: AtomicU64::new(0),
            peak_processing_time_ms: AtomicU64::new(0),
            current_queue_size: AtomicU64::new(0),
            peak_queue_size: AtomicU64::new(0),
            total_queries: AtomicU64::new(0),
            total_query_time_ms: AtomicU64::new(0),
            peak_query_time_ms: AtomicU64::new(0),
            slow_queries: AtomicU64::new(0),
            total_errors: AtomicU64::new(0),
            errors_by_type: Arc::new(RwLock::new(HashMap::new())),
            recent_errors: Arc::new(RwLock::new(Vec::new())),
            peak_memory_mb: AtomicU64::new(0),
            active_alerts: Arc::new(RwLock::new(Vec::new())),
            start_time: Utc::now(),
        }
    }
    
    /// Record event processing
    pub fn record_event_processed(&self, processing_time_ms: f64) {
        self.total_events.fetch_add(1, Ordering::Relaxed);
        self.total_processing_time_ms.fetch_add(processing_time_ms as u64, Ordering::Relaxed);
        
        // Update peak processing time (store as u64 with precision)
        let processing_time_u64 = (processing_time_ms * 1000.0) as u64;
        let current_peak = self.peak_processing_time_ms.load(Ordering::Relaxed);
        if processing_time_u64 > current_peak {
            self.peak_processing_time_ms.store(processing_time_u64, Ordering::Relaxed);
        }
    }
    
    /// Update queue size
    pub fn update_queue_size(&self, size: u64) {
        self.current_queue_size.store(size, Ordering::Relaxed);
        
        // Update peak queue size
        let current_peak = self.peak_queue_size.load(Ordering::Relaxed);
        if size > current_peak {
            self.peak_queue_size.store(size, Ordering::Relaxed);
        }
    }
    
    /// Record database query
    pub fn record_database_query(&self, query_time_ms: f64) {
        self.total_queries.fetch_add(1, Ordering::Relaxed);
        self.total_query_time_ms.fetch_add(query_time_ms as u64, Ordering::Relaxed);
        
        // Update peak query time (store as u64 with precision)
        let query_time_u64 = (query_time_ms * 1000.0) as u64;
        let current_peak = self.peak_query_time_ms.load(Ordering::Relaxed);
        if query_time_u64 > current_peak {
            self.peak_query_time_ms.store(query_time_u64, Ordering::Relaxed);
        }
        
        // Track slow queries (>100ms)
        if query_time_ms > 100.0 {
            self.slow_queries.fetch_add(1, Ordering::Relaxed);
        }
    }
    
    /// Record error
    pub async fn record_error(&self, error_type: String, error_message: String, context: HashMap<String, String>) {
        self.total_errors.fetch_add(1, Ordering::Relaxed);
        
        // Update errors by type
        {
            let mut errors_by_type = self.errors_by_type.write().await;
            *errors_by_type.entry(error_type.clone()).or_insert(0) += 1;
        }
        
        // Add to recent errors (keep last 100)
        {
            let mut recent_errors = self.recent_errors.write().await;
            recent_errors.push(ErrorRecord {
                error_type,
                error_message,
                timestamp: Utc::now(),
                context,
            });
            
            // Keep only last 100 errors
            if recent_errors.len() > 100 {
                let len = recent_errors.len();
                recent_errors.drain(0..len - 100);
            }
        }
    }
    
    /// Update memory usage
    pub fn update_memory_usage(&self, memory_mb: f64) {
        let memory_u64 = (memory_mb * 1000.0) as u64; // Store as u64 with precision
        let current_peak = self.peak_memory_mb.load(Ordering::Relaxed);
        if memory_u64 > current_peak {
            self.peak_memory_mb.store(memory_u64, Ordering::Relaxed);
        }
    }
    
    /// Get current metrics
    pub async fn get_metrics(&self) -> EventPerformanceMetrics {
        let total_events = self.total_events.load(Ordering::Relaxed);
        let total_processing_time = self.total_processing_time_ms.load(Ordering::Relaxed);
        let total_queries = self.total_queries.load(Ordering::Relaxed);
        let total_query_time = self.total_query_time_ms.load(Ordering::Relaxed);
        
        // Calculate rates
        let elapsed_seconds = (Utc::now() - self.start_time).num_seconds() as f64;
        let events_per_second = if elapsed_seconds > 0.0 {
            total_events as f64 / elapsed_seconds
        } else {
            0.0
        };
        
        let error_rate = if elapsed_seconds > 0.0 {
            self.total_errors.load(Ordering::Relaxed) as f64 / elapsed_seconds
        } else {
            0.0
        };
        
        // Calculate averages
        let avg_processing_time_ms = if total_events > 0 {
            total_processing_time as f64 / total_events as f64
        } else {
            0.0
        };
        
        let avg_query_time_ms = if total_queries > 0 {
            total_query_time as f64 / total_queries as f64
        } else {
            0.0
        };
        
        // Get errors by type
        let errors_by_type = self.errors_by_type.read().await.clone();
        let recent_errors = self.recent_errors.read().await.clone();
        
        // Get current memory usage (simplified - in real implementation would use system APIs)
        let current_memory_mb = self.estimate_current_memory_usage();
        
        EventPerformanceMetrics {
            total_events_processed: total_events,
            events_per_second,
            avg_processing_time_ms,
            peak_processing_time_ms: self.peak_processing_time_ms.load(Ordering::Relaxed) as f64 / 1000.0,
            current_queue_size: self.current_queue_size.load(Ordering::Relaxed),
            peak_queue_size: self.peak_queue_size.load(Ordering::Relaxed),
            db_query_metrics: DatabaseQueryMetrics {
                avg_query_time_ms,
                peak_query_time_ms: self.peak_query_time_ms.load(Ordering::Relaxed) as f64 / 1000.0,
                total_queries,
                slow_queries_count: self.slow_queries.load(Ordering::Relaxed),
                pool_metrics: self.get_connection_pool_metrics(),
            },
            memory_metrics: MemoryMetrics {
                current_usage_mb: current_memory_mb,
                peak_usage_mb: self.peak_memory_mb.load(Ordering::Relaxed) as f64 / 1000.0,
                usage_percentage: self.calculate_memory_percentage(current_memory_mb),
                event_cache_size: self.estimate_event_cache_size(),
            },
            error_metrics: ErrorMetrics {
                total_errors: self.total_errors.load(Ordering::Relaxed),
                error_rate,
                errors_by_type,
                recent_errors,
            },
            last_updated: Utc::now(),
        }
    }
    
    /// Add performance alert
    pub async fn add_alert(&self, alert: PerformanceAlert) {
        let mut alerts = self.active_alerts.write().await;
        alerts.push(alert);
        
        // Keep only last 50 alerts
        if alerts.len() > 50 {
            let len = alerts.len();
            alerts.drain(0..len - 50);
        }
    }
    
    /// Get active alerts
    pub async fn get_active_alerts(&self) -> Vec<PerformanceAlert> {
        self.active_alerts.read().await.clone()
    }
    
    /// Clear resolved alerts
    pub async fn clear_resolved_alerts(&self) {
        let mut alerts = self.active_alerts.write().await;
        alerts.retain(|alert| !alert.resolved);
    }
    
    /// Reset metrics
    pub fn reset(&self) {
        self.total_events.store(0, Ordering::Relaxed);
        self.total_processing_time_ms.store(0, Ordering::Relaxed);
        self.peak_processing_time_ms.store(0, Ordering::Relaxed);
        self.current_queue_size.store(0, Ordering::Relaxed);
        self.peak_queue_size.store(0, Ordering::Relaxed);
        self.total_queries.store(0, Ordering::Relaxed);
        self.total_query_time_ms.store(0, Ordering::Relaxed);
        self.peak_query_time_ms.store(0, Ordering::Relaxed);
        self.slow_queries.store(0, Ordering::Relaxed);
        self.total_errors.store(0, Ordering::Relaxed);
        self.peak_memory_mb.store(0, Ordering::Relaxed);
    }
    
    // Helper methods
    fn estimate_current_memory_usage(&self) -> f64 {
        // Simplified memory estimation - in real implementation would use system APIs
        let base_memory = 50.0; // Base memory usage in MB
        let event_overhead = self.total_events.load(Ordering::Relaxed) as f64 * 0.001; // 1KB per event
        let queue_overhead = self.current_queue_size.load(Ordering::Relaxed) as f64 * 0.002; // 2KB per queued event
        
        base_memory + event_overhead + queue_overhead
    }
    
    fn calculate_memory_percentage(&self, current_mb: f64) -> f64 {
        // Assume 1GB total memory for calculation
        let total_memory_mb = 1024.0;
        (current_mb / total_memory_mb * 100.0).min(100.0)
    }
    
    fn estimate_event_cache_size(&self) -> u64 {
        // Simplified cache size estimation
        self.current_queue_size.load(Ordering::Relaxed) + 100 // Base cache size
    }
    
    fn get_connection_pool_metrics(&self) -> ConnectionPoolMetrics {
        // Simplified connection pool metrics - in real implementation would query actual pool
        ConnectionPoolMetrics {
            active_connections: 5,
            idle_connections: 15,
            max_connections: 20,
            avg_connection_wait_ms: 2.5,
        }
    }
}

/// Performance thresholds for alerting
#[derive(Debug, Clone)]
pub struct PerformanceThresholds {
    pub max_processing_time_ms: f64,
    pub max_queue_size: u64,
    pub max_error_rate: f64,
    pub max_query_time_ms: f64,
    pub max_memory_usage_percentage: f64,
    pub min_throughput_events_per_second: f64,
}

impl Default for PerformanceThresholds {
    fn default() -> Self {
        Self {
            max_processing_time_ms: 100.0,
            max_queue_size: 1000,
            max_error_rate: 0.01, // 1% error rate
            max_query_time_ms: 100.0,
            max_memory_usage_percentage: 80.0,
            min_throughput_events_per_second: 100.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    
    #[tokio::test]
    async fn test_metrics_collection() {
        let collector = PerformanceMetricsCollector::new();
        
        // Record some events
        collector.record_event_processed(50.0);
        collector.record_event_processed(75.0);
        collector.record_event_processed(100.0);
        
        // Record some queries
        collector.record_database_query(25.0);
        collector.record_database_query(150.0); // Slow query
        
        // Update queue size
        collector.update_queue_size(500);
        
        // Record an error
        let mut context = HashMap::new();
        context.insert("component".to_string(), "event_storage".to_string());
        collector.record_error(
            "database_error".to_string(),
            "Connection timeout".to_string(),
            context,
        ).await;
        
        // Get metrics
        let metrics = collector.get_metrics().await;
        
        assert_eq!(metrics.total_events_processed, 3);
        assert_eq!(metrics.avg_processing_time_ms, 75.0);
        assert_eq!(metrics.peak_processing_time_ms, 100.0);
        assert_eq!(metrics.current_queue_size, 500);
        assert_eq!(metrics.db_query_metrics.total_queries, 2);
        assert_eq!(metrics.db_query_metrics.slow_queries_count, 1);
        assert_eq!(metrics.error_metrics.total_errors, 1);
        assert!(metrics.error_metrics.errors_by_type.contains_key("database_error"));
    }
    
    #[tokio::test]
    async fn test_alert_management() {
        let collector = PerformanceMetricsCollector::new();
        
        let alert = PerformanceAlert {
            alert_id: "test-alert-1".to_string(),
            alert_type: AlertType::HighProcessingTime,
            severity: AlertSeverity::Warning,
            message: "Processing time exceeded threshold".to_string(),
            current_value: 150.0,
            threshold: 100.0,
            timestamp: Utc::now(),
            resolved: false,
        };
        
        collector.add_alert(alert).await;
        
        let alerts = collector.get_active_alerts().await;
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].alert_id, "test-alert-1");
    }
    
    #[tokio::test]
    async fn test_metrics_reset() {
        let collector = PerformanceMetricsCollector::new();
        
        // Record some data
        collector.record_event_processed(50.0);
        collector.record_database_query(25.0);
        collector.update_queue_size(100);
        
        // Verify data is recorded
        let metrics_before = collector.get_metrics().await;
        assert_eq!(metrics_before.total_events_processed, 1);
        assert_eq!(metrics_before.db_query_metrics.total_queries, 1);
        assert_eq!(metrics_before.current_queue_size, 100);
        
        // Reset metrics
        collector.reset();
        
        // Verify data is cleared
        let metrics_after = collector.get_metrics().await;
        assert_eq!(metrics_after.total_events_processed, 0);
        assert_eq!(metrics_after.db_query_metrics.total_queries, 0);
        assert_eq!(metrics_after.current_queue_size, 0);
    }
}