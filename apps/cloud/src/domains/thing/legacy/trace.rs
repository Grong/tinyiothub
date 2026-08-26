// Device trace service — migrated from domain/device/trace_service.rs

use tinyiothub_core::{error::Error, generate_id};
use tinyiothub_storage::Db;

pub use tinyiothub_storage::thing_trace::{DeviceTrace, DeviceTraceStatistics, SystemTraceOverview};

pub struct DeviceTraceService {
    db: Db,
}

impl DeviceTraceService {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    #[allow(clippy::too_many_arguments)]
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
        if !self.db.device_exists_by_id(device_id).await? {
            return Err(Error::IOError("Device not found".to_string()));
        }
        let trace_id = generate_id();
        let details_json = details.map(|d| d.to_string());
        let source = source.unwrap_or("system");
        self.db
            .insert_device_trace(
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
        if !self.db.device_exists_by_id(device_id).await? {
            return Err(Error::NotFound);
        }
        let limit = limit.unwrap_or(50);
        let offset = offset.unwrap_or(0);
        self.db
            .find_device_traces(device_id, trace_types, levels, limit, offset)
            .await
    }

    pub async fn get_device_trace_statistics(
        &self,
        device_id: &str,
        days: Option<u32>,
    ) -> Result<DeviceTraceStatistics, Error> {
        if !self.db.device_exists_by_id(device_id).await? {
            return Err(Error::NotFound);
        }
        self.db.get_device_trace_statistics(device_id, days.unwrap_or(7)).await
    }

    pub async fn clear_device_traces(
        &self,
        device_id: &str,
        before_date: Option<&str>,
        trace_types: Option<&[String]>,
    ) -> Result<u32, Error> {
        if !self.db.device_exists_by_id(device_id).await? {
            return Err(Error::IOError("Device not found".to_string()));
        }
        self.db.delete_device_traces(device_id, before_date, trace_types).await
    }

    pub async fn cleanup_expired_traces(&self, days_to_keep: u32) -> Result<u32, Error> {
        self.db.cleanup_expired_device_traces(days_to_keep).await
    }

    pub async fn get_system_trace_overview(&self, days: Option<u32>) -> SystemTraceOverview {
        self.db.get_device_trace_system_overview(days.unwrap_or(7)).await
    }

    #[allow(clippy::too_many_arguments)]
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
        self.db
            .find_all_device_traces(
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
