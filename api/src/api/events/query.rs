// Event query API endpoints
// Provides event search, filtering, and pagination functionality

use axum::{
    extract::{Query, State},
    response::Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    domain::event::{
        repositories::{EventCriteria, SortBy, SortOrder},
        value_objects::{EventLevel, EventType},
    },
    dto::{
        request::pagination::{DataObjectWithPagination, PaginationQuery},
        response::{builder::ApiResponseBuilder, ApiResponse},
    },
    shared::{app_state::AppState, security::jwt::Claims},
};

/// Query parameters for event search
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct EventQueryParams {
    #[serde(flatten)]
    pub pagination: PaginationQuery,

    // Time range filters
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,

    // Event type filters
    pub event_types: Option<String>, // Comma-separated list
    pub levels: Option<String>,      // Comma-separated list

    // Source filters
    pub device_ids: Option<String>,   // Comma-separated list
    pub user_ids: Option<String>,     // Comma-separated list
    pub source_types: Option<String>, // Comma-separated list

    // Search
    pub search: Option<String>,

    // Sorting
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
}

/// Event response DTO
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "snake_case")]
pub struct EventResponse {
    pub id: String,
    pub event_type: String,
    pub event_subtype: String,
    pub level: i32,
    pub level_name: String,
    pub source_type: String,
    pub source_id: Option<String>,
    pub device_id: Option<String>,
    pub user_id: Option<String>,
    pub title: String,
    pub content_preview: String,
    pub timestamp: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// Get events with filtering, sorting, and pagination
///
/// Query parameters:
/// - page: Page number (default: 1)
/// - page_size: Items per page (default: 20, max: 100)
/// - start_time: Filter events after this time (ISO 8601)
/// - end_time: Filter events before this time (ISO 8601)
/// - event_types: Comma-separated event types (e.g., "system.user_auth,device.connection")
/// - levels: Comma-separated levels (e.g., "1,2,3,4,5" or "critical,error,warning,info,debug")
/// - device_ids: Comma-separated device IDs
/// - user_ids: Comma-separated user IDs
/// - source_types: Comma-separated source types
/// - search: Full-text search in title and content
/// - sort_by: Sort field (timestamp, level, event_type, source) - default: timestamp
/// - sort_order: Sort order (asc, desc) - default: desc
#[axum::debug_handler]
pub async fn get_events(
    Query(params): Query<EventQueryParams>,
    State(state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<DataObjectWithPagination<EventResponse>>> {
    tracing::info!("Getting events with params: {:?}", params);

    // Parse pagination parameters
    let page = params.pagination.page.unwrap_or(1).max(1);
    let page_size = params.pagination.page_size.unwrap_or(20).min(100).max(1);

    // Build event criteria
    let mut criteria = EventCriteria::default();

    // Time range
    if let Some(start) = params.start_time {
        criteria.start_time = Some(start);
    }
    if let Some(end) = params.end_time {
        criteria.end_time = Some(end);
    }

    // Event types
    if let Some(types_str) = params.event_types {
        if let Ok(types) = EventType::parse_multiple(&types_str) {
            criteria.event_types = Some(types);
        }
    }

    // Levels
    if let Some(levels_str) = params.levels {
        if let Ok(levels) = parse_event_levels(&levels_str) {
            criteria.levels = Some(levels);
        }
    }

    // Device IDs
    if let Some(device_ids_str) = params.device_ids {
        let device_ids: Vec<String> = device_ids_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !device_ids.is_empty() {
            criteria.device_ids = Some(device_ids);
        }
    }

    // User IDs
    if let Some(user_ids_str) = params.user_ids {
        let user_ids: Vec<String> = user_ids_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !user_ids.is_empty() {
            criteria.user_ids = Some(user_ids);
        }
    }

    // Source types
    if let Some(source_types_str) = params.source_types {
        let source_types: Vec<String> = source_types_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !source_types.is_empty() {
            criteria.source_types = Some(source_types);
        }
    }

    // Search text
    if let Some(search) = params.search {
        if !search.trim().is_empty() {
            criteria.search_text = Some(search.trim().to_string());
        }
    }

    // Sorting
    criteria.sort_by = parse_sort_by(params.sort_by.as_deref()).unwrap_or(SortBy::Timestamp);
    criteria.sort_order =
        parse_sort_order(params.sort_order.as_deref()).unwrap_or(SortOrder::Descending);

    // Pagination
    criteria.limit = Some(page_size);
    criteria.offset = Some((page - 1) * page_size);

    // Get event repository from application state
    let event_repo = &state.event_repository;

    // Query events
    match event_repo.find_by_criteria(&criteria).await {
        Ok(events) => {
            // Convert domain events to response DTOs
            let event_responses: Vec<EventResponse> = events
                .into_iter()
                .map(|event| EventResponse {
                    id: event.id().to_string(),
                    event_type: format!("{:?}", event.event_type()).to_lowercase(),
                    event_subtype: match event.event_type() {
                        EventType::System(subtype) => format!("{:?}", subtype).to_lowercase(),
                        EventType::Device(subtype) => format!("{:?}", subtype).to_lowercase(),
                    },
                    level: event.level() as i32,
                    level_name: format!("{:?}", event.level()).to_lowercase(),
                    source_type: event.source().source_type().to_string(),
                    source_id: Some(event.source().source_id().to_string()),
                    device_id: event.source().device_id().map(|s| s.to_string()),
                    user_id: event.source().user_id().map(|s| s.to_string()),
                    title: event.content().title().to_string(),
                    content_preview: generate_content_preview(event.content()),
                    timestamp: event.timestamp(),
                    created_at: event.timestamp(), // For now, same as timestamp
                })
                .collect();

            // Create paginated response
            let paginated_data = DataObjectWithPagination::new(&event_responses, page, page_size);

            ApiResponseBuilder::success(paginated_data)
        }
        Err(e) => {
            tracing::error!("Failed to query events: {}", e);
            ApiResponseBuilder::error(format!("Failed to query events: {}", e))
        }
    }
}

/// Parse comma-separated event levels
fn parse_event_levels(levels_str: &str) -> Result<Vec<EventLevel>, String> {
    let mut levels = Vec::new();

    for level_str in levels_str.split(',') {
        let level_str = level_str.trim();
        if level_str.is_empty() {
            continue;
        }

        let level = match level_str {
            "1" | "debug" => EventLevel::Debug,
            "2" | "info" => EventLevel::Info,
            "3" | "warning" => EventLevel::Warning,
            "4" | "error" => EventLevel::Error,
            "5" | "critical" => EventLevel::Critical,
            _ => return Err(format!("Unknown event level: {}", level_str)),
        };

        levels.push(level);
    }

    Ok(levels)
}

/// Parse sort by parameter
fn parse_sort_by(sort_by: Option<&str>) -> Result<SortBy, String> {
    match sort_by {
        Some("timestamp") | None => Ok(SortBy::Timestamp),
        Some("level") => Ok(SortBy::Level),
        Some("event_type") => Ok(SortBy::EventType),
        Some("source") => Ok(SortBy::Source),
        Some(other) => Err(format!("Unknown sort field: {}", other)),
    }
}

/// Parse sort order parameter
fn parse_sort_order(sort_order: Option<&str>) -> Result<SortOrder, String> {
    match sort_order {
        Some("asc") => Ok(SortOrder::Ascending),
        Some("desc") | None => Ok(SortOrder::Descending),
        Some(other) => Err(format!("Unknown sort order: {}", other)),
    }
}

/// Generate a preview of the event content
fn generate_content_preview(content: &crate::domain::event::value_objects::RichContent) -> String {
    // For now, just return the first text element or title
    if let Some(first_element) = content.elements().first() {
        match first_element {
            crate::domain::event::value_objects::ContentElement::Text { content, .. } => {
                if content.len() > 100 {
                    format!("{}...", &content[..97])
                } else {
                    content.clone()
                }
            }
            _ => content.title().to_string(),
        }
    } else {
        content.title().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_event_levels() {
        let result = parse_event_levels("1,3,5").unwrap();
        assert_eq!(result.len(), 3);

        let result2 = parse_event_levels("debug,warning,critical").unwrap();
        assert_eq!(result2.len(), 3);

        let invalid = parse_event_levels("invalid");
        assert!(invalid.is_err());
    }

    #[test]
    fn test_parse_sort_by() {
        assert!(matches!(parse_sort_by(Some("timestamp")).unwrap(), SortBy::Timestamp));
        assert!(matches!(parse_sort_by(Some("level")).unwrap(), SortBy::Level));
        assert!(matches!(parse_sort_by(None).unwrap(), SortBy::Timestamp));

        let invalid = parse_sort_by(Some("invalid"));
        assert!(invalid.is_err());
    }

    #[test]
    fn test_parse_sort_order() {
        assert!(matches!(parse_sort_order(Some("asc")).unwrap(), SortOrder::Ascending));
        assert!(matches!(parse_sort_order(Some("desc")).unwrap(), SortOrder::Descending));
        assert!(matches!(parse_sort_order(None).unwrap(), SortOrder::Descending));

        let invalid = parse_sort_order(Some("invalid"));
        assert!(invalid.is_err());
    }
}

/// Create a new event
///
/// Creates a new event with the provided data. This endpoint supports
/// creating events for testing and manual event generation.
#[axum::debug_handler]
pub async fn create_event(
    State(_state): State<AppState>,
    claims: Claims,
    Json(request): Json<CreateEventRequest>,
) -> Json<ApiResponse<EventResponse>> {
    tracing::info!("Creating event requested by user: {}", claims.user_id);

    // For now, return a mock response since we need to integrate with the secure event service
    let event_response = EventResponse {
        id: "mock-event-id".to_string(),
        event_type: request.event_type.unwrap_or("system".to_string()),
        event_subtype: "manual".to_string(),
        level: request.level.unwrap_or(3),
        level_name: "info".to_string(),
        source_type: request
            .source
            .as_ref()
            .map(|s| s.source_type.clone())
            .unwrap_or("system".to_string()),
        source_id: request.source.as_ref().map(|s| s.source_id.clone()),
        device_id: request.source.as_ref().and_then(|s| s.device_id.clone()),
        user_id: Some(claims.user_id),
        title: request.content.title,
        content_preview: request.content.description.chars().take(100).collect(),
        timestamp: Utc::now(),
        created_at: Utc::now(),
    };

    ApiResponseBuilder::success(event_response)
}

/// Request DTO for creating events
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateEventRequest {
    pub event_type: Option<String>,
    pub level: Option<i32>,
    pub source: Option<CreateEventSourceRequest>,
    pub content: CreateEventContentRequest,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateEventSourceRequest {
    pub source_type: String,
    pub source_id: String,
    pub device_id: Option<String>,
    pub user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateEventContentRequest {
    pub title: String,
    pub description: String,
    pub metadata: Option<serde_json::Value>,
}
