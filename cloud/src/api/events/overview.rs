// Event overview and statistics API endpoints
// Provides event statistics, trends, and analysis functionality

use axum::{
    extract::{Query, State},
    response::Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    domain::event::repositories::{EventStatistics, GroupBy, StatisticsGroup, StatisticsParams},
    dto::response::{builder::ApiResponseBuilder, ApiResponse},
    shared::{app_state::AppState, security::jwt::Claims},
};

/// Query parameters for event overview/statistics
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct OverviewQueryParams {
    // Time range
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,

    // Grouping
    pub group_by: Option<String>, // level, event_type, source, hour, day, week, month

    // Filters
    pub device_ids: Option<String>, // Comma-separated list
}

/// Event overview response DTO
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct EventOverviewResponse {
    pub total_count: u64,
    pub time_range: TimeRangeInfo,
    pub level_summary: LevelSummary,
    pub type_summary: TypeSummary,
    pub trend_data: Vec<TrendDataPoint>,
    pub top_devices: Vec<DeviceEventCount>,
    pub recent_critical: Vec<RecentCriticalEvent>,
}

/// Time range information
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct TimeRangeInfo {
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub duration_hours: i64,
}

/// Event level summary
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct LevelSummary {
    pub critical_count: u64,
    pub error_count: u64,
    pub warning_count: u64,
    pub info_count: u64,
    pub debug_count: u64,
    pub critical_percentage: f64,
    pub error_percentage: f64,
    pub warning_percentage: f64,
    pub info_percentage: f64,
    pub debug_percentage: f64,
}

/// Event type summary
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct TypeSummary {
    pub system_events: u64,
    pub device_events: u64,
    pub system_percentage: f64,
    pub device_percentage: f64,
    pub by_subtype: Vec<SubtypeSummary>,
}

/// Event subtype summary
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SubtypeSummary {
    pub event_type: String,
    pub event_subtype: String,
    pub count: u64,
    pub percentage: f64,
}

/// Trend data point for time series
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct TrendDataPoint {
    pub timestamp: DateTime<Utc>,
    pub count: u64,
    pub critical_count: u64,
    pub error_count: u64,
    pub warning_count: u64,
}

/// Device event count for top devices
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceEventCount {
    pub device_id: String,
    pub event_count: u64,
    pub critical_count: u64,
    pub error_count: u64,
    pub latest_event_time: DateTime<Utc>,
}

/// Recent critical event summary
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RecentCriticalEvent {
    pub id: String,
    pub title: String,
    pub device_id: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub acknowledged: bool,
}

/// Get event overview and statistics
///
/// Query parameters:
/// - start_time: Start of time range (ISO 8601) - default: 24 hours ago
/// - end_time: End of time range (ISO 8601) - default: now
/// - group_by: Grouping for trend data (hour, day, week, month) - default: hour
/// - device_ids: Comma-separated device IDs to filter by
#[axum::debug_handler]
pub async fn get_event_overview(
    Query(params): Query<OverviewQueryParams>,
    State(state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<EventOverviewResponse>> {
    tracing::info!("Getting event overview with params: {:?}", params);

    // Set default time range (last 24 hours)
    let end_time = params.end_time.unwrap_or_else(Utc::now);
    let start_time = params.start_time.unwrap_or_else(|| end_time - chrono::Duration::hours(24));

    // Parse grouping
    let group_by = parse_group_by(params.group_by.as_deref()).unwrap_or(GroupBy::Hour);

    // Parse device IDs filter
    let device_ids = if let Some(device_ids_str) = params.device_ids {
        let ids: Vec<String> = device_ids_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    } else {
        None
    };

    // Build statistics parameters
    let stats_params = StatisticsParams {
        start_time: Some(start_time),
        end_time: Some(end_time),
        group_by: group_by.clone(),
        device_ids: device_ids.clone(),
    };

    // Get repositories from application state
    let event_repo = &state.event_repository;
    let real_time_repo = &state.real_time_event_repository;

    // Get basic statistics
    let statistics = match event_repo.get_statistics(&stats_params).await {
        Ok(stats) => stats,
        Err(e) => {
            tracing::error!("Failed to get event statistics: {}", e);
            return ApiResponseBuilder::error(format!("Failed to get event statistics: {}", e));
        }
    };

    // Get real-time status for recent critical events
    let real_time_filter = crate::domain::event::repositories::RealTimeFilter {
        device_ids: device_ids.clone(),
        event_types: None,
        source_types: None,
        acknowledged: Some(false), // Only unacknowledged
        min_level: Some(crate::domain::event::value_objects::EventLevel::Critical),
    };

    let recent_critical_events = match real_time_repo.find_active_events(&real_time_filter).await {
        Ok(events) => events
            .into_iter()
            .take(10) // Limit to 10 most recent
            .map(|event| RecentCriticalEvent {
                id: event.id.to_string(),
                title: event.title,
                device_id: event.source.device_id().map(|s| s.to_string()),
                timestamp: event.timestamp,
                acknowledged: event.acknowledged,
            })
            .collect(),
        Err(e) => {
            tracing::warn!("Failed to get recent critical events: {}", e);
            Vec::new()
        }
    };

    // Build response
    let overview =
        build_overview_response(statistics, start_time, end_time, recent_critical_events);

    ApiResponseBuilder::success(overview)
}

/// Parse group by parameter
fn parse_group_by(group_by: Option<&str>) -> Result<GroupBy, String> {
    match group_by {
        Some("level") => Ok(GroupBy::Level),
        Some("event_type") => Ok(GroupBy::EventType),
        Some("source") => Ok(GroupBy::Source),
        Some("hour") | None => Ok(GroupBy::Hour),
        Some("day") => Ok(GroupBy::Day),
        Some("week") => Ok(GroupBy::Week),
        Some("month") => Ok(GroupBy::Month),
        Some(other) => Err(format!("Unknown group by: {}", other)),
    }
}

/// Build overview response from statistics
fn build_overview_response(
    statistics: EventStatistics,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    recent_critical: Vec<RecentCriticalEvent>,
) -> EventOverviewResponse {
    let total_count = statistics.total_count;

    // Calculate time range info
    let duration_hours = (end_time - start_time).num_hours();
    let time_range = TimeRangeInfo { start_time, end_time, duration_hours };

    // Build level summary
    let level_summary = build_level_summary(&statistics.groups, total_count);

    // Build type summary
    let type_summary = build_type_summary(&statistics.groups, total_count);

    // Build trend data (for now, just create empty trend data)
    // In a full implementation, this would require additional queries
    let trend_data = Vec::new();

    // Build top devices (for now, just create empty list)
    // In a full implementation, this would require additional queries
    let top_devices = Vec::new();

    EventOverviewResponse {
        total_count,
        time_range,
        level_summary,
        type_summary,
        trend_data,
        top_devices,
        recent_critical,
    }
}

/// Build level summary from statistics groups
fn build_level_summary(groups: &[StatisticsGroup], total_count: u64) -> LevelSummary {
    let mut critical_count = 0u64;
    let mut error_count = 0u64;
    let mut warning_count = 0u64;
    let mut info_count = 0u64;
    let mut debug_count = 0u64;

    // Extract level counts from groups
    for group in groups {
        match group.key.as_str() {
            "Critical" => critical_count = group.count,
            "Error" => error_count = group.count,
            "Warning" => warning_count = group.count,
            "Info" => info_count = group.count,
            "Debug" => debug_count = group.count,
            _ => {}
        }
    }

    // Calculate percentages
    let total = total_count as f64;
    let critical_percentage =
        if total > 0.0 { (critical_count as f64 / total) * 100.0 } else { 0.0 };
    let error_percentage = if total > 0.0 { (error_count as f64 / total) * 100.0 } else { 0.0 };
    let warning_percentage = if total > 0.0 { (warning_count as f64 / total) * 100.0 } else { 0.0 };
    let info_percentage = if total > 0.0 { (info_count as f64 / total) * 100.0 } else { 0.0 };
    let debug_percentage = if total > 0.0 { (debug_count as f64 / total) * 100.0 } else { 0.0 };

    LevelSummary {
        critical_count,
        error_count,
        warning_count,
        info_count,
        debug_count,
        critical_percentage,
        error_percentage,
        warning_percentage,
        info_percentage,
        debug_percentage,
    }
}

/// Build type summary from statistics groups
fn build_type_summary(groups: &[StatisticsGroup], total_count: u64) -> TypeSummary {
    let mut system_events = 0u64;
    let mut device_events = 0u64;
    let mut by_subtype = Vec::new();

    // Extract type counts from groups
    for group in groups {
        if group.key.starts_with("System") {
            system_events += group.count;
            by_subtype.push(SubtypeSummary {
                event_type: "system".to_string(),
                event_subtype: group.key.clone().to_lowercase(),
                count: group.count,
                percentage: group.percentage,
            });
        } else if group.key.starts_with("Device") {
            device_events += group.count;
            by_subtype.push(SubtypeSummary {
                event_type: "device".to_string(),
                event_subtype: group.key.clone().to_lowercase(),
                count: group.count,
                percentage: group.percentage,
            });
        }
    }

    // Calculate percentages
    let total = total_count as f64;
    let system_percentage = if total > 0.0 { (system_events as f64 / total) * 100.0 } else { 0.0 };
    let device_percentage = if total > 0.0 { (device_events as f64 / total) * 100.0 } else { 0.0 };

    TypeSummary { system_events, device_events, system_percentage, device_percentage, by_subtype }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_group_by() {
        assert!(matches!(parse_group_by(Some("level")).unwrap(), GroupBy::Level));
        assert!(matches!(parse_group_by(Some("hour")).unwrap(), GroupBy::Hour));
        assert!(matches!(parse_group_by(None).unwrap(), GroupBy::Hour));

        let invalid = parse_group_by(Some("invalid"));
        assert!(invalid.is_err());
    }

    #[test]
    fn test_build_level_summary() {
        let groups = vec![
            StatisticsGroup { key: "Critical".to_string(), count: 5, percentage: 10.0 },
            StatisticsGroup { key: "Error".to_string(), count: 15, percentage: 30.0 },
            StatisticsGroup { key: "Warning".to_string(), count: 30, percentage: 60.0 },
        ];

        let summary = build_level_summary(&groups, 50);

        assert_eq!(summary.critical_count, 5);
        assert_eq!(summary.error_count, 15);
        assert_eq!(summary.warning_count, 30);
        assert_eq!(summary.critical_percentage, 10.0);
        assert_eq!(summary.error_percentage, 30.0);
        assert_eq!(summary.warning_percentage, 60.0);
    }
}
