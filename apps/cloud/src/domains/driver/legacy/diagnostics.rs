// Diagnostics Infrastructure
// Fault analysis and device health diagnostics

use serde::{Deserialize, Serialize};

use crate::domains::thing::legacy::trace::DeviceTraceStatistics;
use tinyiothub_core::models::device::Device;
use tinyiothub_storage::cache::DeviceCache;

/// Device fault diagnosis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceDiagnosis {
    pub device_id: String,
    pub device_name: String,
    pub is_healthy: bool,
    pub fault_score: u32, // 0-100, higher = more faulty
    pub issues: Vec<DeviceIssue>,
    pub trace_stats: Option<DeviceTraceStatistics>,
    pub recommendations: Vec<String>,
}

/// Individual device issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceIssue {
    pub severity: String, // "critical", "warning", "info"
    pub code: String,     // e.g., "OFFLINE", "HIGH_ERROR_RATE"
    pub message: String,
    pub timestamp: Option<String>,
}

/// Serial port scan result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerialPortInfo {
    pub port: String,
    pub available: bool,
}

/// Property comparison result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyComparison {
    pub property: String,
    pub values: Vec<PropertyValueEntry>,
    pub statistics: PropertyStatistics,
}

/// A single device's property value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyValueEntry {
    pub device_id: String,
    pub device_name: String,
    pub value: Option<String>,
    pub unit: Option<String>,
    pub timestamp: Option<String>,
}

/// Statistical summary of compared values
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyStatistics {
    pub max_diff: Option<f64>,
    pub average: Option<f64>,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    pub count: usize,
}

pub struct DiagnosticsService;

impl DiagnosticsService {
    /// Diagnose a device for common fault patterns.
    ///
    /// Pure analysis: the caller resolves the `Device` and its trace
    /// statistics (last 7 days) beforehand — this service was dead code in
    /// cloud and previously took `&Arc<AppState>` directly (P4-Task20).
    pub fn diagnose_device(
        device: &Device,
        trace_stats: Option<DeviceTraceStatistics>,
    ) -> Result<DeviceDiagnosis, String> {
        let mut issues = Vec::new();
        let mut fault_score: u32 = 0;
        let mut recommendations = Vec::new();

        // Check offline state
        if device.status == tinyiothub_core::models::device::DeviceStatus::Offline {
            issues.push(DeviceIssue {
                severity: "critical".to_string(),
                code: "OFFLINE".to_string(),
                message: "Device is currently offline".to_string(),
                timestamp: None,
            });
            fault_score += 50;
            recommendations.push("Check device power and network connectivity".to_string());
        }

        // Analyze trace statistics if available
        if let Some(stats) = &trace_stats {
            // High error rate
            if stats.total_traces > 0 {
                let error_rate = (stats.error_traces as f64 / stats.total_traces as f64) * 100.0;
                if error_rate > 20.0 {
                    issues.push(DeviceIssue {
                        severity: "critical".to_string(),
                        code: "HIGH_ERROR_RATE".to_string(),
                        message: format!(
                            "Error rate is {:.1}% ({} errors / {} total traces)",
                            error_rate, stats.error_traces, stats.total_traces
                        ),
                        timestamp: stats.last_trace_time.clone(),
                    });
                    fault_score += 30;
                    recommendations.push("Review error traces to identify root cause".to_string());
                } else if error_rate > 5.0 {
                    issues.push(DeviceIssue {
                        severity: "warning".to_string(),
                        code: "ELEVATED_ERROR_RATE".to_string(),
                        message: format!("Error rate is {:.1}%, slightly elevated", error_rate),
                        timestamp: stats.last_trace_time.clone(),
                    });
                    fault_score += 10;
                }
            }

            // Check for frequent reconnections (many traces in short time)
            if stats.warning_traces > 10 {
                issues.push(DeviceIssue {
                    severity: "warning".to_string(),
                    code: "UNSTABLE".to_string(),
                    message: format!(
                        "{} warning traces in 7 days, device may be unstable",
                        stats.warning_traces
                    ),
                    timestamp: stats.last_trace_time.clone(),
                });
                fault_score += 15;
                recommendations.push("Consider checking physical connections and signal strength".to_string());
            }

            // No recent traces
            if stats.total_traces == 0 {
                issues.push(DeviceIssue {
                    severity: "info".to_string(),
                    code: "NO_ACTIVITY".to_string(),
                    message: "No trace data in the past 7 days".to_string(),
                    timestamp: None,
                });
            }
        } else {
            // No trace stats available
            issues.push(DeviceIssue {
                severity: "info".to_string(),
                code: "NO_TRACE_DATA".to_string(),
                message: "No trace statistics available for this device".to_string(),
                timestamp: None,
            });
        }

        let is_healthy = fault_score < 30;

        if is_healthy && recommendations.is_empty() {
            recommendations.push("Device is operating normally".to_string());
        }

        Ok(DeviceDiagnosis {
            device_id: device.id.clone(),
            device_name: device.name.clone(),
            is_healthy,
            fault_score,
            issues,
            trace_stats,
            recommendations,
        })
    }

    /// Compare a property across multiple devices.
    ///
    /// The caller resolves the `Device`s; real-time property values are read
    /// from the shared `DeviceCache` (may miss — value falls back to None).
    pub fn compare_properties(
        devices: &[Device],
        device_cache: &DeviceCache,
        property_name: &str,
    ) -> Result<PropertyComparison, String> {
        let mut values = Vec::new();

        for device in devices {
            // Get property from data context (real-time) or database
            let property_value = if let Some(cached) = device_cache.get(&device.id) {
                cached.properties.as_ref().and_then(|props| {
                    props
                        .iter()
                        .find(|p| p.name == property_name)
                        .map(|p| (p.current_value.clone(), p.unit.clone(), p.updated_at.clone()))
                })
            } else {
                None
            };

            let (value, unit, timestamp) = property_value.unwrap_or((None, None, None));

            values.push(PropertyValueEntry {
                device_id: device.id.clone(),
                device_name: device.name.clone(),
                value,
                unit,
                timestamp,
            });
        }

        // Calculate statistics
        let numeric_values: Vec<f64> = values
            .iter()
            .filter_map(|v| v.value.as_ref().and_then(|s| s.parse::<f64>().ok()))
            .collect();

        let statistics = if numeric_values.is_empty() {
            PropertyStatistics {
                max_diff: None,
                average: None,
                min_value: None,
                max_value: None,
                count: 0,
            }
        } else {
            let min = numeric_values.iter().cloned().fold(f64::INFINITY, f64::min);
            let max = numeric_values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let sum: f64 = numeric_values.iter().sum();
            let avg = sum / numeric_values.len() as f64;
            let max_diff = max - min;

            PropertyStatistics {
                max_diff: Some(max_diff),
                average: Some(avg),
                min_value: Some(min),
                max_value: Some(max),
                count: numeric_values.len(),
            }
        };

        Ok(PropertyComparison {
            property: property_name.to_string(),
            values,
            statistics,
        })
    }

    /// Scan for available serial ports.
    ///
    /// Stub: the HarmonyOS-gated implementation depended on
    /// `cloud::shared::hardware` (composition-layer HAL) and did not move
    /// with this crate (P4-Task20). Returns an empty list on all platforms.
    pub fn scan_serial_ports() -> Result<Vec<SerialPortInfo>, String> {
        Ok(vec![])
    }
}
