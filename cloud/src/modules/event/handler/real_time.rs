// Real-time event status API endpoints
// Provides current active event status and acknowledgment functionality

use axum::{
    extract::{Path, Query, State},
    response::Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tinyiothub_web::response::ApiResponseBuilder;

use crate::{
    modules::event::{
        repositories::{RealTimeEvent, RealTimeFilter, StatusSummary},
        value_objects::{EventId, EventLevel, EventType},
    },
    shared::{api_response::ApiResponse, app_state::AppState, security::jwt::Claims},
};

/// Query parameters for real-time event filtering
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RealTimeQueryParams {
    // Filters
    pub device_ids: Option<String>,   // Comma-separated list
    pub event_types: Option<String>,  // Comma-separated list
    pub source_types: Option<String>, // Comma-separated list
    pub acknowledged: Option<bool>,   // Filter by acknowledgment status
    pub min_level: Option<String>,    // Minimum level (debug, info, warning, error, critical)
}

/// Real-time event response DTO
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RealTimeEventResponse {
    pub id: String,
    pub event_type: String,
    pub event_subtype: String,
    pub level: i32,
    pub level_name: String,
    pub source_type: String,
    pub source_id: Option<String>,
    pub device_id: Option<String>,
    pub title: String,
    pub content_preview: String,
    pub timestamp: DateTime<Utc>,
    pub acknowledged: bool,
    pub acknowledged_by: Option<String>,
    pub acknowledged_at: Option<DateTime<Utc>>,
}

/// Status summary response DTO
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct StatusSummaryResponse {
    pub total_active: u64,
    pub critical_count: u64,
    pub error_count: u64,
    pub warning_count: u64,
    pub unacknowledged_count: u64,
    pub health_status: String,
    pub by_device: Vec<DeviceStatusResponse>,
    pub by_type: Vec<TypeStatusResponse>,
}

/// Device status summary response DTO
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceStatusResponse {
    pub device_id: String,
    pub active_count: u64,
    pub highest_level: i32,
    pub highest_level_name: String,
    pub latest_timestamp: DateTime<Utc>,
}

/// Event type status summary response DTO
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct TypeStatusResponse {
    pub event_type: String,
    pub event_subtype: String,
    pub active_count: u64,
    pub highest_level: i32,
    pub highest_level_name: String,
}

/// Get real-time active events
///
/// Query parameters:
/// - device_ids: Comma-separated device IDs to filter by
/// - event_types: Comma-separated event types (e.g., "system.user_auth,device.connection")
/// - source_types: Comma-separated source types
/// - acknowledged: Filter by acknowledgment status (true/false)
/// - min_level: Minimum event level (debug, info, warning, error, critical)
#[axum::debug_handler]
pub async fn get_real_time_events(
    Query(params): Query<RealTimeQueryParams>,
    State(state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<Vec<RealTimeEventResponse>>> {
    tracing::info!("Getting real-time events with params: {:?}", params);

    // Build real-time filter
    let mut filter = RealTimeFilter::default();

    // Device IDs
    if let Some(device_ids_str) = params.device_ids {
        let device_ids: Vec<String> = device_ids_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !device_ids.is_empty() {
            filter.device_ids = Some(device_ids);
        }
    }

    // Event types
    if let Some(types_str) = params.event_types
        && let Ok(types) = EventType::parse_multiple(&types_str)
    {
        filter.event_types = Some(types);
    }

    // Source types
    if let Some(source_types_str) = params.source_types {
        let source_types: Vec<String> = source_types_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !source_types.is_empty() {
            filter.source_types = Some(source_types);
        }
    }

    // Acknowledged status
    if let Some(acknowledged) = params.acknowledged {
        filter.acknowledged = Some(acknowledged);
    }

    // Minimum level
    if let Some(min_level_str) = params.min_level
        && let Ok(min_level) = parse_event_level(&min_level_str)
    {
        filter.min_level = Some(min_level);
    }

    // Get real-time event repository from application state
    let real_time_repo = &state.real_time_event_repository;

    // Query real-time events
    match real_time_repo.find_active_events(&filter).await {
        Ok(events) => {
            // Convert domain events to response DTOs
            let event_responses: Vec<RealTimeEventResponse> =
                events.into_iter().map(convert_real_time_event_to_response).collect();

            ApiResponseBuilder::success(event_responses)
        }
        Err(e) => {
            tracing::error!("Failed to query real-time events: {}", e);
            ApiResponseBuilder::error(format!("Failed to query real-time events: {}", e))
        }
    }
}

/// Get real-time status summary
///
/// Query parameters: Same as get_real_time_events
#[axum::debug_handler]
pub async fn get_status_summary(
    Query(params): Query<RealTimeQueryParams>,
    State(state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<StatusSummaryResponse>> {
    tracing::info!("Getting status summary with params: {:?}", params);

    // Build real-time filter (same as get_real_time_events)
    let mut filter = RealTimeFilter::default();

    // Apply filters (same logic as above)
    if let Some(device_ids_str) = params.device_ids {
        let device_ids: Vec<String> = device_ids_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !device_ids.is_empty() {
            filter.device_ids = Some(device_ids);
        }
    }

    if let Some(types_str) = params.event_types
        && let Ok(types) = EventType::parse_multiple(&types_str)
    {
        filter.event_types = Some(types);
    }

    if let Some(source_types_str) = params.source_types {
        let source_types: Vec<String> = source_types_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !source_types.is_empty() {
            filter.source_types = Some(source_types);
        }
    }

    if let Some(acknowledged) = params.acknowledged {
        filter.acknowledged = Some(acknowledged);
    }

    if let Some(min_level_str) = params.min_level
        && let Ok(min_level) = parse_event_level(&min_level_str)
    {
        filter.min_level = Some(min_level);
    }

    // Get real-time event repository from application state
    let real_time_repo = &state.real_time_event_repository;

    // Get status summary
    match real_time_repo.get_status_summary(&filter).await {
        Ok(summary) => {
            let response = convert_status_summary_to_response(summary);
            ApiResponseBuilder::success(response)
        }
        Err(e) => {
            tracing::error!("Failed to get status summary: {}", e);
            ApiResponseBuilder::error(format!("Failed to get status summary: {}", e))
        }
    }
}

/// Acknowledge a real-time event
///
/// Path parameters:
/// - id: Event ID to acknowledge
#[axum::debug_handler]
pub async fn acknowledge_event(
    Path(id): Path<String>,
    State(state): State<AppState>,
    claims: Claims,
) -> Json<ApiResponse<bool>> {
    tracing::info!("Acknowledging event {} by user {}", id, claims.user_id);

    // Parse event ID
    let event_id = EventId::from_string(id);

    // Get real-time event repository from application state
    let real_time_repo = &state.real_time_event_repository;

    // Acknowledge the event
    match real_time_repo.acknowledge_event(&event_id, &claims.user_id).await {
        Ok(_) => {
            tracing::info!("Event {} acknowledged by user {}", event_id, claims.user_id);
            ApiResponseBuilder::success(true)
        }
        Err(e) => {
            tracing::error!("Failed to acknowledge event {}: {}", event_id, e);
            ApiResponseBuilder::error(format!("Failed to acknowledge event: {}", e))
        }
    }
}

/// Parse single event level
fn parse_event_level(level_str: &str) -> Result<EventLevel, String> {
    match level_str.trim() {
        "debug" | "1" => Ok(EventLevel::Debug),
        "info" | "2" => Ok(EventLevel::Info),
        "warning" | "3" => Ok(EventLevel::Warning),
        "error" | "4" => Ok(EventLevel::Error),
        "critical" | "5" => Ok(EventLevel::Critical),
        _ => Err(format!("Unknown event level: {}", level_str)),
    }
}

/// Convert domain RealTimeEvent to response DTO
fn convert_real_time_event_to_response(event: RealTimeEvent) -> RealTimeEventResponse {
    RealTimeEventResponse {
        id: event.id.to_string(),
        event_type: match &event.event_type {
            EventType::System(_) => "system".to_string(),
            EventType::Device(_) => "device".to_string(),
            EventType::Ai(_) => "ai".to_string(),
        },
        event_subtype: match &event.event_type {
            EventType::System(subtype) => format!("{:?}", subtype).to_lowercase(),
            EventType::Device(subtype) => format!("{:?}", subtype).to_lowercase(),
            EventType::Ai(subtype) => format!("{:?}", subtype).to_lowercase(),
        },
        level: event.level as i32,
        level_name: format!("{:?}", event.level).to_lowercase(),
        source_type: event.source.source_type().to_string(),
        source_id: Some(event.source.source_id().to_string()),
        device_id: event.source.device_id().map(|s| s.to_string()),
        title: event.title,
        content_preview: event.content_preview,
        timestamp: event.timestamp,
        acknowledged: event.acknowledged,
        acknowledged_by: event.acknowledged_by,
        acknowledged_at: event.acknowledged_at,
    }
}

/// Convert domain StatusSummary to response DTO
fn convert_status_summary_to_response(summary: StatusSummary) -> StatusSummaryResponse {
    StatusSummaryResponse {
        total_active: summary.total_active,
        critical_count: summary.critical_count,
        error_count: summary.error_count,
        warning_count: summary.warning_count,
        unacknowledged_count: summary.unacknowledged_count,
        health_status: format!("{:?}", summary.health_status()).to_lowercase(),
        by_device: summary
            .by_device
            .into_iter()
            .map(|device_summary| DeviceStatusResponse {
                device_id: device_summary.device_id,
                active_count: device_summary.active_count,
                highest_level: device_summary.highest_level as i32,
                highest_level_name: format!("{:?}", device_summary.highest_level).to_lowercase(),
                latest_timestamp: device_summary.latest_timestamp,
            })
            .collect(),
        by_type: summary
            .by_type
            .into_iter()
            .map(|type_summary| TypeStatusResponse {
                event_type: match &type_summary.event_type {
                    EventType::System(_) => "system".to_string(),
                    EventType::Device(_) => "device".to_string(),
                    EventType::Ai(_) => "ai".to_string(),
                },
                event_subtype: match &type_summary.event_type {
                    EventType::System(subtype) => format!("{:?}", subtype).to_lowercase(),
                    EventType::Device(subtype) => format!("{:?}", subtype).to_lowercase(),
                    EventType::Ai(subtype) => format!("{:?}", subtype).to_lowercase(),
                },
                active_count: type_summary.active_count,
                highest_level: type_summary.highest_level as i32,
                highest_level_name: format!("{:?}", type_summary.highest_level).to_lowercase(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_event_level() {
        assert!(matches!(parse_event_level("debug").unwrap(), EventLevel::Debug));
        assert!(matches!(parse_event_level("1").unwrap(), EventLevel::Debug));
        assert!(matches!(parse_event_level("critical").unwrap(), EventLevel::Critical));
        assert!(matches!(parse_event_level("5").unwrap(), EventLevel::Critical));

        let invalid = parse_event_level("invalid");
        assert!(invalid.is_err());
    }
}
