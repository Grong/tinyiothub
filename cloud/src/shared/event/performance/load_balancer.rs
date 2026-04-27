use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Event load balancer for distributing work across workers
pub struct EventLoadBalancer {
    config: LoadBalancerConfig,
    workers: Vec<WorkerStats>,
    task_queue: Vec<Task>,
    metrics: LoadBalancerMetrics,
}

/// Load balancer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadBalancerConfig {
    pub worker_count: usize,
    pub max_queue_size: usize,
    pub backpressure_threshold: usize,
    pub task_timeout_seconds: u64,
}

impl Default for LoadBalancerConfig {
    fn default() -> Self {
        Self {
            worker_count: 4,
            max_queue_size: 10000,
            backpressure_threshold: 8000,
            task_timeout_seconds: 30,
        }
    }
}

/// Worker statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerStats {
    pub worker_id: usize,
    pub is_busy: bool,
    pub current_task: Option<String>,
    pub processed_count: u64,
    pub error_count: u64,
    pub success_rate: f64,
    pub last_activity: DateTime<Utc>,
    pub average_processing_time_ms: f64,
}

/// Task to be processed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub task_type: TaskType,
    pub priority: TaskPriority,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub retry_count: u32,
}

/// Task type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskType {
    EventProcessing,
    NotificationDelivery,
    DatabaseOperation,
    PerformanceMonitoring,
}

/// Task priority
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskPriority {
    Low = 1,
    Normal = 2,
    High = 3,
    Critical = 4,
}

/// Load balancer metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadBalancerMetrics {
    pub total_workers: usize,
    pub active_workers: usize,
    pub idle_workers: usize,
    pub queue_size: usize,
    pub max_queue_size: usize,
    pub total_processed: u64,
    pub total_errors: u64,
    pub success_rate: f64,
    pub average_queue_wait_time_ms: f64,
    pub throughput_per_second: f64,
    pub backpressure_active: bool,
}

impl EventLoadBalancer {
    /// Create a new load balancer
    pub fn new() -> Self {
        let config = LoadBalancerConfig::default();
        let workers = (0..config.worker_count)
            .map(|id| WorkerStats {
                worker_id: id,
                is_busy: false,
                current_task: None,
                processed_count: 0,
                error_count: 0,
                success_rate: 100.0,
                last_activity: Utc::now(),
                average_processing_time_ms: 0.0,
            })
            .collect();

        Self {
            config: config.clone(),
            workers,
            task_queue: Vec::new(),
            metrics: LoadBalancerMetrics {
                total_workers: config.worker_count,
                active_workers: 0,
                idle_workers: config.worker_count,
                queue_size: 0,
                max_queue_size: config.max_queue_size,
                total_processed: 0,
                total_errors: 0,
                success_rate: 100.0,
                average_queue_wait_time_ms: 0.0,
                throughput_per_second: 0.0,
                backpressure_active: false,
            },
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: LoadBalancerConfig) -> Self {
        let workers = (0..config.worker_count)
            .map(|id| WorkerStats {
                worker_id: id,
                is_busy: false,
                current_task: None,
                processed_count: 0,
                error_count: 0,
                success_rate: 100.0,
                last_activity: Utc::now(),
                average_processing_time_ms: 0.0,
            })
            .collect();

        Self {
            metrics: LoadBalancerMetrics {
                total_workers: config.worker_count,
                active_workers: 0,
                idle_workers: config.worker_count,
                queue_size: 0,
                max_queue_size: config.max_queue_size,
                total_processed: 0,
                total_errors: 0,
                success_rate: 100.0,
                average_queue_wait_time_ms: 0.0,
                throughput_per_second: 0.0,
                backpressure_active: false,
            },
            config,
            workers,
            task_queue: Vec::new(),
        }
    }

    /// Get current configuration
    pub fn config(&self) -> &LoadBalancerConfig {
        &self.config
    }

    /// Update configuration
    pub fn update_config(&mut self, config: LoadBalancerConfig) {
        // Adjust worker count if needed
        if config.worker_count != self.config.worker_count {
            self.adjust_worker_count(config.worker_count);
        }

        self.config = config;
        self.metrics.max_queue_size = self.config.max_queue_size;
        self.metrics.total_workers = self.config.worker_count;
    }

    /// Adjust worker count
    fn adjust_worker_count(&mut self, new_count: usize) {
        let current_count = self.workers.len();

        if new_count > current_count {
            // Add new workers
            for id in current_count..new_count {
                self.workers.push(WorkerStats {
                    worker_id: id,
                    is_busy: false,
                    current_task: None,
                    processed_count: 0,
                    error_count: 0,
                    success_rate: 100.0,
                    last_activity: Utc::now(),
                    average_processing_time_ms: 0.0,
                });
            }
        } else if new_count < current_count {
            // Remove workers (keep only the first new_count workers)
            self.workers.truncate(new_count);
        }

        self.update_metrics();
    }

    /// Submit a task for processing
    pub fn submit_task(&mut self, task: Task) -> Result<(), String> {
        if self.task_queue.len() >= self.config.max_queue_size {
            return Err("Queue is full".to_string());
        }

        self.task_queue.push(task);
        self.task_queue.sort_by(|a, b| b.priority.cmp(&a.priority));

        self.update_metrics();
        Ok(())
    }

    /// Get next available worker
    pub fn get_available_worker(&mut self) -> Option<usize> {
        self.workers.iter().enumerate().find(|(_, worker)| !worker.is_busy).map(|(idx, _)| idx)
    }

    /// Assign task to worker
    pub fn assign_task_to_worker(
        &mut self,
        worker_id: usize,
        task_id: String,
    ) -> Result<(), String> {
        if worker_id >= self.workers.len() {
            return Err("Invalid worker ID".to_string());
        }

        let worker = &mut self.workers[worker_id];
        if worker.is_busy {
            return Err("Worker is already busy".to_string());
        }

        worker.is_busy = true;
        worker.current_task = Some(task_id);
        worker.last_activity = Utc::now();

        self.update_metrics();
        Ok(())
    }

    /// Complete task for worker
    pub fn complete_task(
        &mut self,
        worker_id: usize,
        success: bool,
        processing_time_ms: f64,
    ) -> Result<(), String> {
        if worker_id >= self.workers.len() {
            return Err("Invalid worker ID".to_string());
        }

        let worker = &mut self.workers[worker_id];
        worker.is_busy = false;
        worker.current_task = None;
        worker.last_activity = Utc::now();
        worker.processed_count += 1;

        if !success {
            worker.error_count += 1;
        }

        worker.success_rate = if worker.processed_count > 0 {
            ((worker.processed_count - worker.error_count) as f64 / worker.processed_count as f64)
                * 100.0
        } else {
            100.0
        };

        // Update average processing time
        if worker.average_processing_time_ms == 0.0 {
            worker.average_processing_time_ms = processing_time_ms;
        } else {
            worker.average_processing_time_ms =
                (worker.average_processing_time_ms + processing_time_ms) / 2.0;
        }

        self.update_metrics();
        Ok(())
    }

    /// Update metrics
    fn update_metrics(&mut self) {
        let active_workers = self.workers.iter().filter(|w| w.is_busy).count();
        let idle_workers = self.workers.len() - active_workers;

        let total_processed: u64 = self.workers.iter().map(|w| w.processed_count).sum();
        let total_errors: u64 = self.workers.iter().map(|w| w.error_count).sum();

        let success_rate = if total_processed > 0 {
            ((total_processed - total_errors) as f64 / total_processed as f64) * 100.0
        } else {
            100.0
        };

        let avg_processing_time: f64 = self
            .workers
            .iter()
            .filter(|w| w.average_processing_time_ms > 0.0)
            .map(|w| w.average_processing_time_ms)
            .sum::<f64>()
            / self.workers.len().max(1) as f64;

        self.metrics = LoadBalancerMetrics {
            total_workers: self.workers.len(),
            active_workers,
            idle_workers,
            queue_size: self.task_queue.len(),
            max_queue_size: self.config.max_queue_size,
            total_processed,
            total_errors,
            success_rate,
            average_queue_wait_time_ms: avg_processing_time * 0.1, // Mock calculation
            throughput_per_second: total_processed as f64 / 60.0,  // Mock calculation
            backpressure_active: self.task_queue.len() >= self.config.backpressure_threshold,
        };
    }

    /// Get current metrics
    pub fn metrics(&self) -> &LoadBalancerMetrics {
        &self.metrics
    }

    /// Get worker statistics
    pub fn worker_stats(&self) -> &[WorkerStats] {
        &self.workers
    }

    /// Get queue size
    pub fn queue_size(&self) -> usize {
        self.task_queue.len()
    }

    /// Check if backpressure is active
    pub fn is_backpressure_active(&self) -> bool {
        self.metrics.backpressure_active
    }

    /// Get next task from queue
    pub fn get_next_task(&mut self) -> Option<Task> {
        if self.task_queue.is_empty() {
            return None;
        }

        let task = self.task_queue.remove(0);
        self.update_metrics();
        Some(task)
    }
}

impl Default for EventLoadBalancer {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TaskType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskType::EventProcessing => write!(f, "EventProcessing"),
            TaskType::NotificationDelivery => write!(f, "NotificationDelivery"),
            TaskType::DatabaseOperation => write!(f, "DatabaseOperation"),
            TaskType::PerformanceMonitoring => write!(f, "PerformanceMonitoring"),
        }
    }
}

impl std::fmt::Display for TaskPriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskPriority::Low => write!(f, "Low"),
            TaskPriority::Normal => write!(f, "Normal"),
            TaskPriority::High => write!(f, "High"),
            TaskPriority::Critical => write!(f, "Critical"),
        }
    }
}
