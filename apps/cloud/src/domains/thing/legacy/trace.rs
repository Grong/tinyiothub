// Thing trace service — migrated from domain/device/trace_service.rs

use tinyiothub_core::{error::Error, generate_id};
use tinyiothub_storage::Db;

pub use tinyiothub_storage::thing_trace::{SystemTraceOverview, ThingTrace, ThingTraceStatistics};

pub struct ThingTraceService {
    db: Db,
}

impl ThingTraceService {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn record_device_trace(
        &self,
        thing_id: &str,
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
        if !self.db.thing_exists_by_id(thing_id).await? {
            return Err(Error::IOError("Thing not found".to_string()));
        }
        let trace_id = generate_id();
        let details_json = details.map(|d| d.to_string());
        let source = source.unwrap_or("system");
        self.db
            .insert_thing_trace(
                &trace_id,
                thing_id,
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
            "Thing trace recorded: device={}, type={}, level={}, title={}, trace_id={}",
            thing_id,
            trace_type,
            level,
            title,
            trace_id
        );
        if level == "error" || level == "critical" {
            tracing::warn!(
                "Critical trace recorded for device {}: {} - {}",
                thing_id,
                title,
                message
            );
        }
        Ok(trace_id)
    }

    pub async fn get_device_traces(
        &self,
        thing_id: &str,
        trace_types: Option<&[String]>,
        levels: Option<&[String]>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<ThingTrace>, Error> {
        if !self.db.thing_exists_by_id(thing_id).await? {
            return Err(Error::NotFound);
        }
        let limit = limit.unwrap_or(50);
        let offset = offset.unwrap_or(0);
        self.db
            .find_thing_traces(thing_id, trace_types, levels, limit, offset)
            .await
    }

    pub async fn get_thing_trace_statistics(
        &self,
        thing_id: &str,
        days: Option<u32>,
    ) -> Result<ThingTraceStatistics, Error> {
        if !self.db.thing_exists_by_id(thing_id).await? {
            return Err(Error::NotFound);
        }
        self.db.get_thing_trace_statistics(thing_id, days.unwrap_or(7)).await
    }

    pub async fn clear_device_traces(
        &self,
        thing_id: &str,
        before_date: Option<&str>,
        trace_types: Option<&[String]>,
    ) -> Result<u32, Error> {
        if !self.db.thing_exists_by_id(thing_id).await? {
            return Err(Error::IOError("Thing not found".to_string()));
        }
        self.db.delete_thing_traces(thing_id, before_date, trace_types).await
    }

    pub async fn cleanup_expired_traces(&self, days_to_keep: u32) -> Result<u32, Error> {
        self.db.cleanup_expired_thing_traces(days_to_keep).await
    }

    pub async fn get_system_trace_overview(&self, days: Option<u32>) -> SystemTraceOverview {
        self.db.get_thing_trace_system_overview(days.unwrap_or(7)).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn find_all_traces(
        &self,
        levels: Option<&[String]>,
        sources: Option<&[String]>,
        thing_id: Option<&str>,
        device_ids: Option<&[String]>,
        start_time: Option<&str>,
        end_time: Option<&str>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<ThingTrace>, Error> {
        self.db
            .find_all_thing_traces(
                levels,
                sources,
                thing_id,
                device_ids,
                start_time,
                end_time,
                limit.unwrap_or(50),
                offset.unwrap_or(0),
            )
            .await
    }
}
