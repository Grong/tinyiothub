// Probe Scheduler Infrastructure
// Handles periodic health probes for system, device, and task monitoring

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use tokio::sync::broadcast;
use tokio::time::{interval, Duration};
use tokio::sync::RwLock;

use crate::domain::self_healing::{
    PolicyEvaluator, ProbeFinding, ProbeResult, ProbeType, SeverityLevel,
};
use crate::dto::entity::self_healing::ProbeConfig;

/// Probe scheduler that runs periodic health checks
pub struct ProbeScheduler {
    config: Arc<RwLock<ProbeConfig>>,
    evaluator: Arc<PolicyEvaluator>,
    last_probe_results: Arc<RwLock<HashMap<ProbeType, ProbeResult>>>,
    shutdown_rx: Arc<RwLock<Option<broadcast::Receiver<()>>>>,
}

impl ProbeScheduler {
    /// Create a new ProbeScheduler
    pub fn new(
        config: ProbeConfig,
        evaluator: PolicyEvaluator,
        shutdown_rx: broadcast::Receiver<()>,
    ) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            evaluator: Arc::new(evaluator),
            last_probe_results: Arc::new(RwLock::new(HashMap::new())),
            shutdown_rx: Arc::new(RwLock::new(Some(shutdown_rx))),
        }
    }

    /// Run the probe scheduler
    /// Takes ownership of the shutdown receiver and runs until shutdown signal
    pub async fn run(&self) {
        // Take the shutdown receiver from the struct
        let mut shutdown_rx = self.shutdown_rx.write().await.take()
            .expect("ProbeScheduler::run() called twice - shutdown_rx already taken");

        // Read config to get probe intervals
        let config_guard = self.config.read().await;

        // Create independent interval tickers based on config values
        let mut system_ticker = if config_guard.system_probe_enabled {
            Some(interval(Duration::from_secs(config_guard.system_probe_interval_secs)))
        } else {
            None
        };
        let mut device_ticker = if config_guard.device_probe_enabled {
            Some(interval(Duration::from_secs(config_guard.device_probe_interval_secs)))
        } else {
            None
        };
        let mut task_ticker = if config_guard.task_probe_enabled {
            Some(interval(Duration::from_secs(config_guard.task_probe_interval_secs)))
        } else {
            None
        };

        // Discard immediate first tick for all intervals
        if let Some(ref mut ticker) = system_ticker {
            ticker.tick().await;
        }
        if let Some(ref mut ticker) = device_ticker {
            ticker.tick().await;
        }
        if let Some(ref mut ticker) = task_ticker {
            ticker.tick().await;
        }

        drop(config_guard); // release lock before entering loop

        // Run initial probes before entering the loop
        self.run_system_probe().await;
        self.run_device_probe().await;
        self.run_task_probe().await;

        loop {
            tokio::select! {
                biased;
                // Shutdown signal as first priority
                _ = shutdown_rx.recv() => {
                    tracing::info!("ProbeScheduler received shutdown signal");
                    break;
                }
                // System probe ticker
                _ = async {
                    if let Some(ref mut ticker) = system_ticker {
                        ticker.tick().await;
                    }
                } => {
                    self.run_system_probe().await;
                }
                // Device probe ticker
                _ = async {
                    if let Some(ref mut ticker) = device_ticker {
                        ticker.tick().await;
                    }
                } => {
                    self.run_device_probe().await;
                }
                // Task probe ticker
                _ = async {
                    if let Some(ref mut ticker) = task_ticker {
                        ticker.tick().await;
                    }
                } => {
                    self.run_task_probe().await;
                }
            }
        }
    }

    /// Run system probe - checks CPU, memory, and disk health
    async fn run_system_probe(&self) {
        let config = self.config.read().await;
        if !config.system_probe_enabled {
            tracing::debug!("System probe disabled, skipping");
            return;
        }
        drop(config);

        let mut findings = Vec::new();
        let mut metadata = HashMap::new();

        // Use sysinfo crate to get system information
        let mut sys = sysinfo::System::new_all();
        // Refresh CPU to get accurate readings
        sys.refresh_cpu_usage();
        sys.refresh_memory();

        // Check CPU usage
        let cpus = sys.cpus();
        let cpu_usage = if !cpus.is_empty() {
            cpus.iter().map(|cpu| cpu.cpu_usage()).sum::<f32>() / cpus.len() as f32
        } else {
            0.0
        };
        metadata.insert("cpu_usage".to_string(), format!("{:.1}%", cpu_usage));

        if cpu_usage >= 90.0 {
            findings.push(ProbeFinding {
                finding_type: "HighCpu".to_string(),
                severity: SeverityLevel::L3,
                message: format!("CPU usage critical: {:.1}%", cpu_usage),
                target: "system".to_string(),
                value: Some(format!("{:.1}%", cpu_usage)),
            });
        } else if cpu_usage >= 70.0 {
            findings.push(ProbeFinding {
                finding_type: "HighCpu".to_string(),
                severity: SeverityLevel::L1,
                message: format!("CPU usage high: {:.1}%", cpu_usage),
                target: "system".to_string(),
                value: Some(format!("{:.1}%", cpu_usage)),
            });
        }

        // Check memory usage
        let total_memory = sys.total_memory();
        let used_memory = sys.used_memory();
        let memory_usage_pct = if total_memory > 0 {
            (used_memory as f64 / total_memory as f64) * 100.0
        } else {
            0.0
        };
        metadata.insert("memory_usage".to_string(), format!("{:.1}%", memory_usage_pct));

        if memory_usage_pct >= 90.0 {
            findings.push(ProbeFinding {
                finding_type: "HighMemory".to_string(),
                severity: SeverityLevel::L3,
                message: format!("Memory usage critical: {:.1}%", memory_usage_pct),
                target: "system".to_string(),
                value: Some(format!("{:.1}%", memory_usage_pct)),
            });
        } else if memory_usage_pct >= 75.0 {
            findings.push(ProbeFinding {
                finding_type: "HighMemory".to_string(),
                severity: SeverityLevel::L1,
                message: format!("Memory usage high: {:.1}%", memory_usage_pct),
                target: "system".to_string(),
                value: Some(format!("{:.1}%", memory_usage_pct)),
            });
        }

        // Check disk usage
        let disks = sysinfo::Disks::new_with_refreshed_list();
        for disk in disks.iter() {
            let disk_name = disk.mount_point().to_string_lossy().to_string();
            let total_space = disk.total_space();
            let available_space = disk.available_space();
            let disk_usage_pct = if total_space > 0 {
                ((total_space - available_space) as f64 / total_space as f64) * 100.0
            } else {
                0.0
            };
            metadata.insert(format!("disk_{}_usage", disk_name), format!("{:.1}%", disk_usage_pct));

            if disk_usage_pct >= 95.0 {
                findings.push(ProbeFinding {
                    finding_type: "DiskFull".to_string(),
                    severity: SeverityLevel::L3,
                    message: format!("Disk {} usage critical: {:.1}%", disk_name, disk_usage_pct),
                    target: disk_name.clone(),
                    value: Some(format!("{:.1}%", disk_usage_pct)),
                });
            } else if disk_usage_pct >= 80.0 {
                findings.push(ProbeFinding {
                    finding_type: "HighDisk".to_string(),
                    severity: SeverityLevel::L1,
                    message: format!("Disk {} usage high: {:.1}%", disk_name, disk_usage_pct),
                    target: disk_name.clone(),
                    value: Some(format!("{:.1}%", disk_usage_pct)),
                });
            }
        }

        // Create probe result
        let healthy = findings.is_empty();
        let severity = if healthy {
            SeverityLevel::L0
        } else {
            findings.iter().map(|f| f.severity).max().unwrap_or(SeverityLevel::L0)
        };

        let probe_result = ProbeResult {
            probe_type: ProbeType::System,
            timestamp: Utc::now(),
            healthy,
            severity,
            findings: findings.clone(),
            metadata,
        };

        // Evaluate severity using policy evaluator (result logged for debugging)
        tracing::debug!(
            "System probe completed: healthy={}, findings={}, evaluated_severity={:?}",
            healthy,
            findings.len(),
            self.evaluator.evaluate(&probe_result)
        );

        // Store result
        let mut results = self.last_probe_results.write().await;
        results.insert(ProbeType::System, probe_result);
    }

    /// Run device probe - checks device health (stub for Phase 2)
    async fn run_device_probe(&self) {
        let config = self.config.read().await;
        if !config.device_probe_enabled {
            tracing::debug!("Device probe disabled, skipping");
            return;
        }
        drop(config);

        let probe_result = ProbeResult {
            probe_type: ProbeType::Device,
            timestamp: Utc::now(),
            healthy: true,
            severity: SeverityLevel::L0,
            findings: Vec::new(),
            metadata: HashMap::new(),
        };

        tracing::debug!("Device probe completed: healthy=true (stub)");

        let mut results = self.last_probe_results.write().await;
        results.insert(ProbeType::Device, probe_result);
    }

    /// Run task probe - checks task health (stub for Phase 2)
    async fn run_task_probe(&self) {
        let config = self.config.read().await;
        if !config.task_probe_enabled {
            tracing::debug!("Task probe disabled, skipping");
            return;
        }
        drop(config);

        let probe_result = ProbeResult {
            probe_type: ProbeType::Task,
            timestamp: Utc::now(),
            healthy: true,
            severity: SeverityLevel::L0,
            findings: Vec::new(),
            metadata: HashMap::new(),
        };

        tracing::debug!("Task probe completed: healthy=true (stub)");

        let mut results = self.last_probe_results.write().await;
        results.insert(ProbeType::Task, probe_result);
    }

    /// Get the last probe result for a specific probe type
    pub async fn get_last_result(&self, probe_type: ProbeType) -> Option<ProbeResult> {
        let results = self.last_probe_results.read().await;
        results.get(&probe_type).cloned()
    }

    /// Get all last probe results
    pub async fn get_all_results(&self) -> HashMap<ProbeType, ProbeResult> {
        let results = self.last_probe_results.read().await;
        results.clone()
    }

    /// Update the probe configuration
    pub async fn update_config(&self, new_config: ProbeConfig) {
        let mut config = self.config.write().await;
        *config = new_config;
    }

    /// Get a copy of the current configuration
    pub async fn get_config(&self) -> ProbeConfig {
        let config = self.config.read().await;
        config.clone()
    }
}
