use std::sync::Arc;
use crate::modules::gateway::GatewayService;
use crate::modules::offline::{BufferMessage, BufferPriority, OfflineBuffer};
use crate::modules::driver::DriverService;
use super::types::HealthReport;

pub struct HealthService {
    gateway_service: Arc<GatewayService>,
    offline_buffer: Arc<OfflineBuffer>,
    driver_service: Arc<DriverService>,
    start_time: std::time::Instant,
}

impl HealthService {
    pub fn new(
        gateway_service: Arc<GatewayService>,
        offline_buffer: Arc<OfflineBuffer>,
        driver_service: Arc<DriverService>,
    ) -> Arc<Self> {
        Arc::new(Self {
            gateway_service,
            offline_buffer,
            driver_service,
            start_time: std::time::Instant::now(),
        })
    }

    /// Generate a health report with current system metrics
    pub async fn generate_report(&self) -> HealthReport {
        let drivers = self.driver_service.list_drivers().await.unwrap_or_default();
        let buffer_status = self.offline_buffer.get_status().await;

        HealthReport {
            status: "online".into(),
            uptime_secs: self.start_time.elapsed().as_secs(),
            cpu_percent: 0.0, // stub -- use sysinfo in production
            memory_mb: 0.0,
            disk_free_mb: 0.0,
            driver_count: drivers.len() as u32,
            buffer_backlog: buffer_status.total_telemetry + buffer_status.total_alarms,
        }
    }

    /// Send heartbeat with health report, buffer on failure
    pub async fn beat_and_report(
        &self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let report = self.generate_report().await;
        let payload = serde_json::to_vec(&report)?;
        let topic = format!("{}/status", self.gateway_service.topic_prefix());

        if self.gateway_service.publish_status(&payload).await.is_err() {
            tracing::warn!("Heartbeat publish failed, buffering locally");
            self.offline_buffer
                .write(BufferMessage {
                    msg_type: "heartbeat".into(),
                    topic,
                    payload,
                    priority: BufferPriority::Normal,
                })
                .await
                .ok();
        }

        Ok(())
    }
}
