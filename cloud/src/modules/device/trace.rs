// Device trace service — migrated from domain/device/trace_service.rs

use std::sync::Arc;

use tinyiothub_core::{error::Error, generate_id};

use crate::shared::persistence::repositories::DeviceTraceRepository;

pub struct DeviceTraceService {
    repository: Arc<DeviceTraceRepository>,
}

impl DeviceTraceService {
    pub fn new(repository: Arc<DeviceTraceRepository>) -> Self {
        Self { repository }
    }

    pub async fn record_device_trace(
        &self,
        device_id: &str,
        trace_type: &str,
        level: &str,
        category: &str,
        title: &str,
        message: &str,
        details: Option<serde_json::Value>,
        source: Option<&str>,
        user_id: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<String, Error> {
        if !self.repository.device_exists(device_id).await? {
            return Err(Error::IOError("Device not found".to_string()));
        }
        let trace_id = generate_id();
        let details_json = details.map(|d| d.to_string());
        let source = source.unwrap_or("system");
        self.repository
            .insert_trace(
                &trace_id,
                device_id,
                trace_type,
                level,
                category,
                title,
                message,
                details_json,
                source,
                user_id,
                session_id,
            )
            .await?;
        tracing::info!(
            "Device trace recorded: device={}, type={}, level={}, title={}, trace_id={}",
            device_id,
            trace_type,
            level,
            title,
            trace_id
        );
        if level == "error" || level == "critical" {
            tracing::warn!(
                "Critical trace recorded for device {}: {} - {}",
                device_id,
                title,
                message
            );
        }
        Ok(trace_id)
    }

    pub async fn get_device_traces(
        &self,
        device_id: &str,
        trace_types: Option<&[String]>,
        levels: Option<&[String]>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<DeviceTrace>, Error> {
        if !self.repository.device_exists(device_id).await? {
            return Err(Error::NotFound);
        }
        let limit = limit.unwrap_or(50);
        let offset = offset.unwrap_or(0);
        self.repository.find_traces(device_id, trace_types, levels, limit, offset).await
    }

    pub async fn get_device_trace_statistics(
        &self,
        device_id: &str,
        days: Option<u32>,
    ) -> Result<DeviceTraceStatistics, Error> {
        if !self.repository.device_exists(device_id).await? {
            return Err(Error::NotFound);
        }
        self.repository.get_trace_statistics(device_id, days.unwrap_or(7)).await
    }

    pub async fn clear_device_traces(
        &self,
        device_id: &str,
        before_date: Option<&str>,
        trace_types: Option<&[String]>,
    ) -> Result<u32, Error> {
        if !self.repository.device_exists(device_id).await? {
            return Err(Error::IOError("Device not found".to_string()));
        }
        self.repository.delete_traces(device_id, before_date, trace_types).await
    }

    pub async fn cleanup_expired_traces(&self, days_to_keep: u32) -> Result<u32, Error> {
        self.repository.cleanup_expired(days_to_keep).await
    }

    pub async fn get_system_trace_overview(&self, days: Option<u32>) -> SystemTraceOverview {
        self.repository.get_system_overview(days.unwrap_or(7)).await
    }

    pub async fn find_all_traces(
        &self,
        levels: Option<&[String]>,
        sources: Option<&[String]>,
        device_id: Option<&str>,
        device_ids: Option<&[String]>,
        start_time: Option<&str>,
        end_time: Option<&str>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<DeviceTrace>, Error> {
        self.repository
            .find_all_traces(
                levels,
                sources,
                device_id,
                device_ids,
                start_time,
                end_time,
                limit.unwrap_or(50),
                offset.unwrap_or(0),
            )
            .await
    }
}

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct DeviceTrace {
    pub id: String,
    pub device_id: String,
    pub trace_type: String,
    pub level: String,
    pub category: String,
    pub title: String,
    pub message: String,
    pub details: Option<String>,
    pub source: Option<String>,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeviceTraceStatistics {
    pub device_id: String,
    pub total_traces: u32,
    pub error_traces: u32,
    pub warning_traces: u32,
    pub info_traces: u32,
    pub days_range: u32,
    pub last_trace_time: Option<String>,
    pub last_updated: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SystemTraceOverview {
    pub total_traces: u32,
    pub error_traces: u32,
    pub warning_traces: u32,
    pub info_traces: u32,
    pub active_devices: u32,
    pub days_range: u32,
    pub last_updated: String,
}
