// Device performance service — migrated from domain/device/performance_service.rs

use std::sync::Arc;

use tinyiothub_storage::cache::DeviceCache;

use super::{super::alarm::repo::AlarmRepository, monitoring::DeviceMonitoringService};
use crate::shared::{error::Error, persistence::Database};

pub struct DevicePerformanceService {
    #[allow(dead_code)]
    database: Arc<Database>,
    device_cache: Arc<DeviceCache>,
    monitoring_service: DeviceMonitoringService,
    alarm_repository: Arc<dyn AlarmRepository>,
}

impl DevicePerformanceService {
    pub fn new(
        database: Arc<Database>,
        device_cache: Arc<DeviceCache>,
        alarm_repository: Arc<dyn AlarmRepository>,
    ) -> Self {
        let monitoring_service = DeviceMonitoringService::new(
            database.clone(),
            device_cache.clone(),
            alarm_repository.clone(),
        );
        Self { database, device_cache, monitoring_service, alarm_repository }
    }

    pub async fn get_device_performance_metrics(
        &self,
        device_id: &str,
    ) -> Option<DevicePerformanceMetrics> {
        if let Some(device) = self.device_cache.get(device_id) {
            let mut metrics = DevicePerformanceMetrics {
                device_id: device_id.to_string(),
                cpu_usage: None,
                memory_usage: None,
                network_latency_ms: None,
                response_time_ms: None,
                throughput_ops_per_sec: None,
                error_rate: None,
                uptime_percentage: None,
                last_updated: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            };
            if let Some(last_heartbeat) = &device.last_heartbeat
                && let Ok(heartbeat_time) = chrono::DateTime::parse_from_str(
                    &format!("{} +00:00", last_heartbeat),
                    "%Y-%m-%d %H:%M:%S %z",
                )
            {
                let now = chrono::Utc::now();
                let response_time =
                    (now - heartbeat_time.with_timezone(&chrono::Utc)).num_milliseconds() as f64;
                if (0.0..300000.0).contains(&response_time) {
                    metrics.response_time_ms = Some(response_time);
                }
            }
            if let Some(connection_quality) =
                self.monitoring_service.get_device_connection_quality(device_id)
            {
                let estimated_latency = match connection_quality {
                    90..=100 => 10.0 + (100 - connection_quality) as f64 * 0.5,
                    80..=89 => 15.0 + (90 - connection_quality) as f64 * 1.0,
                    70..=79 => 25.0 + (80 - connection_quality) as f64 * 2.0,
                    60..=69 => 45.0 + (70 - connection_quality) as f64 * 3.0,
                    _ => 75.0 + (60 - connection_quality.min(60)) as f64 * 5.0,
                };
                metrics.network_latency_ms = Some(estimated_latency);
            }
            metrics.uptime_percentage = self.calculate_device_uptime_percentage(device_id).await;
            if let Some(stats) = self.monitoring_service.get_device_metrics(device_id).await {
                if stats.total_properties > 0 {
                    let error_rate =
                        (stats.active_alarms as f64 / stats.total_properties as f64) * 0.1;
                    metrics.error_rate = Some(error_rate.min(1.0));
                }
                if stats.online_properties > 0 {
                    let base_throughput = stats.online_properties as f64 * 0.5;
                    let quality_factor = self
                        .monitoring_service
                        .get_device_connection_quality(device_id)
                        .map(|q| q as f64 / 100.0)
                        .unwrap_or(0.5);
                    metrics.throughput_ops_per_sec = Some(base_throughput * quality_factor);
                }
            }
            let (cpu_usage, memory_usage) = self.estimate_device_resource_usage(device_id).await;
            metrics.cpu_usage = cpu_usage;
            metrics.memory_usage = memory_usage;
            Some(metrics)
        } else {
            None
        }
    }

    async fn calculate_device_uptime_percentage(&self, device_id: &str) -> Option<f64> {
        if let Some(device) = self.device_cache.get(device_id)
            && let Some(created_at) = &device.created_at
            && let Ok(created_time) = chrono::DateTime::parse_from_str(
                &format!("{} +00:00", created_at),
                "%Y-%m-%d %H:%M:%S %z",
            )
        {
            let now = chrono::Utc::now();
            let total_time = (now - created_time.with_timezone(&chrono::Utc)).num_hours() as f64;
            if total_time > 0.0 {
                let offline_hours = self.calculate_device_offline_hours(device_id).await;
                let uptime_percentage =
                    ((total_time - offline_hours) / total_time * 100.0).clamp(0.0, 100.0);
                return Some(uptime_percentage);
            }
        }
        if self.monitoring_service.is_device_online(device_id) { Some(95.0) } else { Some(85.0) }
    }

    async fn calculate_device_offline_hours(&self, device_id: &str) -> f64 {
        self.alarm_repository.count_offline_alarms(device_id, 30).await.unwrap_or(0) as f64
    }

    async fn estimate_device_resource_usage(&self, device_id: &str) -> (Option<f64>, Option<f64>) {
        if let Some(device) = self.device_cache.get(device_id) {
            let property_count = device.properties.as_ref().map(|p| p.len()).unwrap_or(0) as f64;
            let command_count = device.commands.as_ref().map(|c| c.len()).unwrap_or(0) as f64;
            let base_cpu = 5.0;
            let base_memory = 10.0;
            let online_factor =
                if self.monitoring_service.is_device_online(device_id) { 1.2 } else { 0.3 };
            let quality_factor = self
                .monitoring_service
                .get_device_connection_quality(device_id)
                .map(|q| 0.5 + (q as f64 / 100.0) * 0.5)
                .unwrap_or(0.7);
            let cpu = ((base_cpu + property_count * 0.5 + command_count * 0.3)
                * online_factor
                * quality_factor)
                .clamp(1.0, 95.0);
            let mem = ((base_memory + property_count * 1.0 + command_count * 0.5)
                * online_factor
                * quality_factor)
                .clamp(5.0, 90.0);
            (Some(cpu), Some(mem))
        } else {
            (None, None)
        }
    }

    pub async fn get_device_performance_history(
        &self,
        device_id: &str,
        hours: u32,
    ) -> Result<Vec<DevicePerformanceMetrics>, Error> {
        if self.device_cache.get(device_id).is_none() {
            return Err(Error::NotFound);
        }
        let mut history = Vec::new();
        let now = chrono::Utc::now();
        for i in 0..hours {
            let timestamp = now - chrono::Duration::hours(i as i64);
            if let Some(mut metrics) = self.get_device_performance_metrics(device_id).await {
                let variation_factor = 0.9 + (i as f64 * 0.02);
                if let Some(cpu) = metrics.cpu_usage {
                    metrics.cpu_usage = Some((cpu * variation_factor).clamp(0.0, 100.0));
                }
                if let Some(memory) = metrics.memory_usage {
                    metrics.memory_usage = Some((memory * variation_factor).clamp(0.0, 100.0));
                }
                if let Some(latency) = metrics.network_latency_ms {
                    metrics.network_latency_ms =
                        Some((latency * (2.0 - variation_factor)).max(1.0));
                }
                metrics.last_updated = timestamp.format("%Y-%m-%d %H:%M:%S").to_string();
                history.push(metrics);
            }
        }
        history.reverse();
        Ok(history)
    }

    pub async fn check_device_performance_alerts(&self, device_id: &str) -> Vec<PerformanceAlert> {
        let mut alerts = Vec::new();
        if let Some(metrics) = self.get_device_performance_metrics(device_id).await {
            if let Some(cpu) = metrics.cpu_usage {
                if cpu > 90.0 {
                    alerts.push(PerformanceAlert::new(
                        device_id,
                        "high_cpu",
                        "critical",
                        &format!("CPU usage critical: {:.1}%", cpu),
                        cpu,
                        90.0,
                    ));
                } else if cpu > 80.0 {
                    alerts.push(PerformanceAlert::new(
                        device_id,
                        "high_cpu",
                        "warning",
                        &format!("CPU usage high: {:.1}%", cpu),
                        cpu,
                        80.0,
                    ));
                }
            }
            if let Some(mem) = metrics.memory_usage {
                if mem > 95.0 {
                    alerts.push(PerformanceAlert::new(
                        device_id,
                        "high_memory",
                        "critical",
                        &format!("Memory usage critical: {:.1}%", mem),
                        mem,
                        95.0,
                    ));
                } else if mem > 85.0 {
                    alerts.push(PerformanceAlert::new(
                        device_id,
                        "high_memory",
                        "warning",
                        &format!("Memory usage high: {:.1}%", mem),
                        mem,
                        85.0,
                    ));
                }
            }
            if let Some(latency) = metrics.network_latency_ms {
                if latency > 200.0 {
                    alerts.push(PerformanceAlert::new(
                        device_id,
                        "high_latency",
                        "critical",
                        &format!("Latency critical: {:.1}ms", latency),
                        latency,
                        200.0,
                    ));
                } else if latency > 100.0 {
                    alerts.push(PerformanceAlert::new(
                        device_id,
                        "high_latency",
                        "warning",
                        &format!("Latency high: {:.1}ms", latency),
                        latency,
                        100.0,
                    ));
                }
            }
            if let Some(rt) = metrics.response_time_ms {
                if rt > 5000.0 {
                    alerts.push(PerformanceAlert::new(
                        device_id,
                        "slow_response",
                        "critical",
                        &format!("Response time critical: {:.1}ms", rt),
                        rt,
                        5000.0,
                    ));
                } else if rt > 3000.0 {
                    alerts.push(PerformanceAlert::new(
                        device_id,
                        "slow_response",
                        "warning",
                        &format!("Response time high: {:.1}ms", rt),
                        rt,
                        3000.0,
                    ));
                }
            }
            if let Some(er) = metrics.error_rate {
                if er > 0.1 {
                    alerts.push(PerformanceAlert::new(
                        device_id,
                        "high_error_rate",
                        "critical",
                        &format!("Error rate critical: {:.1}%", er * 100.0),
                        er * 100.0,
                        10.0,
                    ));
                } else if er > 0.05 {
                    alerts.push(PerformanceAlert::new(
                        device_id,
                        "high_error_rate",
                        "warning",
                        &format!("Error rate high: {:.1}%", er * 100.0),
                        er * 100.0,
                        5.0,
                    ));
                }
            }
            if let Some(up) = metrics.uptime_percentage {
                if up < 90.0 {
                    alerts.push(PerformanceAlert::new(
                        device_id,
                        "low_uptime",
                        "critical",
                        &format!("Uptime critical: {:.1}%", up),
                        up,
                        90.0,
                    ));
                } else if up < 95.0 {
                    alerts.push(PerformanceAlert::new(
                        device_id,
                        "low_uptime",
                        "warning",
                        &format!("Uptime low: {:.1}%", up),
                        up,
                        95.0,
                    ));
                }
            }
        }
        alerts
    }

    pub async fn get_system_performance_overview(&self) -> SystemPerformanceOverview {
        let all_devices = self.device_cache.all();
        let total_devices = all_devices.len() as u32;
        let mut total_cpu = 0.0;
        let mut total_mem = 0.0;
        let mut total_latency = 0.0;
        let mut total_throughput = 0.0;
        let mut devices_with_metrics = 0u32;
        let mut high_cpu = 0u32;
        let mut high_mem = 0u32;
        let mut high_latency = 0u32;
        for device in &all_devices {
            if let Some(metrics) = self.get_device_performance_metrics(&device.id).await {
                devices_with_metrics += 1;
                if let Some(cpu) = metrics.cpu_usage {
                    total_cpu += cpu;
                    if cpu > 80.0 {
                        high_cpu += 1;
                    }
                }
                if let Some(mem) = metrics.memory_usage {
                    total_mem += mem;
                    if mem > 85.0 {
                        high_mem += 1;
                    }
                }
                if let Some(lat) = metrics.network_latency_ms {
                    total_latency += lat;
                    if lat > 100.0 {
                        high_latency += 1;
                    }
                }
                if let Some(tp) = metrics.throughput_ops_per_sec {
                    total_throughput += tp;
                }
            }
        }
        let avg =
            |total: f64, count: u32| if count > 0 { Some(total / count as f64) } else { None };
        SystemPerformanceOverview {
            total_devices,
            devices_with_metrics,
            average_cpu_usage: avg(total_cpu, devices_with_metrics),
            average_memory_usage: avg(total_mem, devices_with_metrics),
            average_network_latency_ms: avg(total_latency, devices_with_metrics),
            total_throughput_ops_per_sec: Some(total_throughput),
            high_cpu_devices: high_cpu,
            high_memory_devices: high_mem,
            high_latency_devices: high_latency,
            last_updated: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DevicePerformanceMetrics {
    pub device_id: String,
    pub cpu_usage: Option<f64>,
    pub memory_usage: Option<f64>,
    pub network_latency_ms: Option<f64>,
    pub response_time_ms: Option<f64>,
    pub throughput_ops_per_sec: Option<f64>,
    pub error_rate: Option<f64>,
    pub uptime_percentage: Option<f64>,
    pub last_updated: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SystemPerformanceOverview {
    pub total_devices: u32,
    pub devices_with_metrics: u32,
    pub average_cpu_usage: Option<f64>,
    pub average_memory_usage: Option<f64>,
    pub average_network_latency_ms: Option<f64>,
    pub total_throughput_ops_per_sec: Option<f64>,
    pub high_cpu_devices: u32,
    pub high_memory_devices: u32,
    pub high_latency_devices: u32,
    pub last_updated: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PerformanceAlert {
    pub device_id: String,
    pub alert_type: String,
    pub severity: String,
    pub message: String,
    pub current_value: f64,
    pub threshold: f64,
    pub timestamp: String,
}

impl PerformanceAlert {
    pub fn new(
        device_id: &str,
        alert_type: &str,
        severity: &str,
        message: &str,
        current_value: f64,
        threshold: f64,
    ) -> Self {
        Self {
            device_id: device_id.to_string(),
            alert_type: alert_type.to_string(),
            severity: severity.to_string(),
            message: message.to_string(),
            current_value,
            threshold,
            timestamp: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        }
    }
}
