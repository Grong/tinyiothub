use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Performance thresholds for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
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
            max_error_rate: 0.01,
            max_query_time_ms: 100.0,
            max_memory_usage_percentage: 80.0,
            min_throughput_events_per_second: 100.0,
        }
    }
}

/// Event performance monitor
pub struct EventPerformanceMonitor {
    thresholds: PerformanceThresholds,
    metrics: PerformanceMetrics,
}

/// Performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub total_events_processed: u64,
    pub events_per_second: f64,
    pub avg_processing_time_ms: f64,
    pub peak_processing_time_ms: f64,
    pub current_queue_size: u64,
    pub peak_queue_size: u64,
    pub error_count: u64,
    pub error_rate: f64,
    pub memory_usage_mb: f64,
    pub memory_usage_percentage: f64,
    pub last_updated: DateTime<Utc>,
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            total_events_processed: 0,
            events_per_second: 0.0,
            avg_processing_time_ms: 0.0,
            peak_processing_time_ms: 0.0,
            current_queue_size: 0,
            peak_queue_size: 0,
            error_count: 0,
            error_rate: 0.0,
            memory_usage_mb: 0.0,
            memory_usage_percentage: 0.0,
            last_updated: Utc::now(),
        }
    }
}

impl EventPerformanceMonitor {
    /// Create a new performance monitor
    pub fn new() -> Self {
        Self {
            thresholds: PerformanceThresholds::default(),
            metrics: PerformanceMetrics::default(),
        }
    }

    /// Create with custom thresholds
    pub fn with_thresholds(thresholds: PerformanceThresholds) -> Self {
        Self {
            thresholds,
            metrics: PerformanceMetrics::default(),
        }
    }

    /// Get current thresholds
    pub fn thresholds(&self) -> &PerformanceThresholds {
        &self.thresholds
    }

    /// Update thresholds
    pub fn update_thresholds(&mut self, thresholds: PerformanceThresholds) {
        self.thresholds = thresholds;
    }

    /// Get current metrics
    pub fn metrics(&self) -> &PerformanceMetrics {
        &self.metrics
    }

    /// Update metrics
    pub fn update_metrics(&mut self, metrics: PerformanceMetrics) {
        self.metrics = metrics;
    }

    /// Check if performance is within thresholds
    pub fn is_healthy(&self) -> bool {
        self.metrics.avg_processing_time_ms <= self.thresholds.max_processing_time_ms
            && self.metrics.current_queue_size <= self.thresholds.max_queue_size
            && self.metrics.error_rate <= self.thresholds.max_error_rate
            && self.metrics.memory_usage_percentage <= self.thresholds.max_memory_usage_percentage
            && self.metrics.events_per_second >= self.thresholds.min_throughput_events_per_second
    }

    /// Get performance alerts
    pub fn get_alerts(&self) -> Vec<PerformanceAlert> {
        let mut alerts = Vec::new();

        if self.metrics.avg_processing_time_ms > self.thresholds.max_processing_time_ms {
            alerts.push(PerformanceAlert {
                alert_type: AlertType::HighProcessingTime,
                severity: AlertSeverity::Warning,
                message: format!(
                    "Average processing time ({:.1}ms) exceeds threshold ({:.1}ms)",
                    self.metrics.avg_processing_time_ms, self.thresholds.max_processing_time_ms
                ),
                current_value: self.metrics.avg_processing_time_ms,
                threshold: self.thresholds.max_processing_time_ms,
                timestamp: Utc::now(),
            });
        }

        if self.metrics.current_queue_size > self.thresholds.max_queue_size {
            alerts.push(PerformanceAlert {
                alert_type: AlertType::HighQueueSize,
                severity: AlertSeverity::Critical,
                message: format!(
                    "Queue size ({}) exceeds threshold ({})",
                    self.metrics.current_queue_size, self.thresholds.max_queue_size
                ),
                current_value: self.metrics.current_queue_size as f64,
                threshold: self.thresholds.max_queue_size as f64,
                timestamp: Utc::now(),
            });
        }

        if self.metrics.error_rate > self.thresholds.max_error_rate {
            alerts.push(PerformanceAlert {
                alert_type: AlertType::HighErrorRate,
                severity: AlertSeverity::Warning,
                message: format!(
                    "Error rate ({:.3}) exceeds threshold ({:.3})",
                    self.metrics.error_rate, self.thresholds.max_error_rate
                ),
                current_value: self.metrics.error_rate,
                threshold: self.thresholds.max_error_rate,
                timestamp: Utc::now(),
            });
        }

        if self.metrics.memory_usage_percentage > self.thresholds.max_memory_usage_percentage {
            alerts.push(PerformanceAlert {
                alert_type: AlertType::HighMemoryUsage,
                severity: AlertSeverity::Warning,
                message: format!(
                    "Memory usage ({:.1}%) exceeds threshold ({:.1}%)",
                    self.metrics.memory_usage_percentage,
                    self.thresholds.max_memory_usage_percentage
                ),
                current_value: self.metrics.memory_usage_percentage,
                threshold: self.thresholds.max_memory_usage_percentage,
                timestamp: Utc::now(),
            });
        }

        if self.metrics.events_per_second < self.thresholds.min_throughput_events_per_second {
            alerts.push(PerformanceAlert {
                alert_type: AlertType::LowThroughput,
                severity: AlertSeverity::Warning,
                message: format!(
                    "Throughput ({:.1} events/sec) below threshold ({:.1} events/sec)",
                    self.metrics.events_per_second,
                    self.thresholds.min_throughput_events_per_second
                ),
                current_value: self.metrics.events_per_second,
                threshold: self.thresholds.min_throughput_events_per_second,
                timestamp: Utc::now(),
            });
        }

        alerts
    }
}

/// Performance alert
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceAlert {
    pub alert_type: AlertType,
    pub severity: AlertSeverity,
    pub message: String,
    pub current_value: f64,
    pub threshold: f64,
    pub timestamp: DateTime<Utc>,
}

/// Alert type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertType {
    HighProcessingTime,
    HighQueueSize,
    HighErrorRate,
    HighMemoryUsage,
    LowThroughput,
}

/// Alert severity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

impl std::fmt::Display for AlertType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlertType::HighProcessingTime => write!(f, "HighProcessingTime"),
            AlertType::HighQueueSize => write!(f, "HighQueueSize"),
            AlertType::HighErrorRate => write!(f, "HighErrorRate"),
            AlertType::HighMemoryUsage => write!(f, "HighMemoryUsage"),
            AlertType::LowThroughput => write!(f, "LowThroughput"),
        }
    }
}

impl std::fmt::Display for AlertSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlertSeverity::Info => write!(f, "Info"),
            AlertSeverity::Warning => write!(f, "Warning"),
            AlertSeverity::Critical => write!(f, "Critical"),
        }
    }
}

impl Default for EventPerformanceMonitor {
    fn default() -> Self {
        Self::new()
    }
}
