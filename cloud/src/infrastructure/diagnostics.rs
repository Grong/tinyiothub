// Diagnostics Infrastructure
// Fault analysis and device health diagnostics

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::domain::device::trace_service::DeviceTraceStatistics;
use crate::shared::app_state::AppState;

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
    pub severity: String,     // "critical", "warning", "info"
    pub code: String,        // e.g., "OFFLINE", "HIGH_ERROR_RATE"
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
    /// Diagnose a device for common fault patterns
    pub async fn diagnose_device(
        state: &Arc<AppState>,
        device_id: &str,
    ) -> Result<DeviceDiagnosis, String> {
        // Get device info
        let device = state
            .device_service
            .get_device_by_id(device_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Device not found".to_string())?;

        // Get trace statistics (last 7 days)
        let trace_stats = state
            .trace_service
            .get_device_trace_statistics(device_id, Some(7))
            .await
            .ok();

        let mut issues = Vec::new();
        let mut fault_score: u32 = 0;
        let mut recommendations = Vec::new();

        // Check offline state
        if device.state == Some(0) {
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
                        message: format!("Error rate is {:.1}% ({} errors / {} total traces)",
                            error_rate, stats.error_traces, stats.total_traces),
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
                    message: format!("{} warning traces in 7 days, device may be unstable",
                        stats.warning_traces),
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
            device_id: device_id.to_string(),
            device_name: device.name.clone(),
            is_healthy,
            fault_score,
            issues,
            trace_stats,
            recommendations,
        })
    }

    /// Compare a property across multiple devices
    pub async fn compare_properties(
        state: &Arc<AppState>,
        device_ids: &[String],
        property_name: &str,
    ) -> Result<PropertyComparison, String> {
        let mut values = Vec::new();

        for device_id in device_ids {
            // Get device info
            let device = state
                .device_service
                .get_device_by_id(device_id)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("Device {} not found", device_id))?;

            // Get property from data context (real-time) or database
            let property_value = if let Some(cached) = state.device_cache.get(device_id) {
                cached.properties.as_ref().and_then(|props| {
                    props.iter().find(|p| p.name == property_name).map(|p| {
                        (
                            p.current_value.clone(),
                            p.unit.clone(),
                            p.updated_at.clone(),
                        )
                    })
                })
            } else {
                None
            };

            let (value, unit, timestamp) = property_value.unwrap_or((None, None, None));

            values.push(PropertyValueEntry {
                device_id: device_id.clone(),
                device_name: device.name,
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

    /// Scan for available serial ports
    #[cfg(feature = "harmonyos")]
    pub fn scan_serial_ports() -> Result<Vec<SerialPortInfo>, String> {
        use crate::infrastructure::hardware::list_serial_ports;
        let ports = list_serial_ports().map_err(|e| e.to_string())?;
        Ok(ports
            .into_iter()
            .map(|port| SerialPortInfo {
                port,
                available: true,
            })
            .collect())
    }

    /// Scan for available serial ports (non-harmonyos stub)
    #[cfg(not(feature = "harmonyos"))]
    pub fn scan_serial_ports() -> Result<Vec<SerialPortInfo>, String> {
        // On non-HarmonyOS platforms, return empty list
        Ok(vec![])
    }
}