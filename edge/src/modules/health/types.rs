use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct HealthReport {
    pub status: String,
    pub uptime_secs: u64,
    pub cpu_percent: f64,
    pub memory_mb: f64,
    pub disk_free_mb: f64,
    pub driver_count: u32,
    pub buffer_backlog: u64,
}

impl HealthReport {
    pub fn sample() -> Self {
        Self {
            status: "online".into(),
            uptime_secs: 0,
            cpu_percent: 0.0,
            memory_mb: 0.0,
            disk_free_mb: 0.0,
            driver_count: 0,
            buffer_backlog: 0,
        }
    }
}
